//! The window: ground, sidebar, the page floating on it, what floats over that.
//! A port of `AppShellView`. Each screen lives in its own crate; this only places
//! them and connects what they ask of each other.

use crate::pages::PageHost;
use fumi_publish::PublishPane;
use fumi_editor::element::CaretSlot;
use fumi_settings::search::SearchPanel;
use fumi_settings::sheet::{OnChange, SettingsChange, SettingsSheet};
use fumi_settings::welcome::{OnOpen, WelcomePage};
use fumi_sidebar::{SidebarEvent, SidebarView};
use fumi_toolbar::layout::Tween;
use fumi_toolbar::{Toolbar, ToolbarEvent};
use fumi_ui::theme::{self, mix, t};
use fumi_ui::window_frame::WindowFrame;
use fumi_ui::{native, settings};
use fumi_workspace::{WindowStart, Workspace, WorkspaceEvent};
use gpui::prelude::*;
use gpui::*;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The editor's caret, drawn on its own layer over the window: a blink redraws
/// this and nothing under it, which keeps an idle note close to free.
pub struct CaretLayer {
    slot: Rc<CaretSlot>,
}

impl Render for CaretLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let slot = self.slot.clone();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                if slot.on.get()
                    && let Some(rect) = slot.rect.get()
                {
                    window.paint_quad(fill(rect, slot.color.get()));
                }
            },
        )
        .absolute()
        .size_full()
    }
}

pub const PAGE_INSET: f32 = 8.;
pub const PAGE_RADIUS: f32 = fumi_ui::theme::PAGE_RADIUS;
pub const TOOLBAR_HEIGHT: f32 = 52.;

/// The welcome page's plain sheet settling into the framed window.
const REVEAL_DELAY_MS: u64 = 120;
const REVEAL_MS: f32 = 500.;
/// The welcome page and the shell trade places, each fading.
const SWITCH: Duration = Duration::from_millis(200);

/// The sidebar's width plus the detail column's minimum, which keeps the text
/// column from reflowing as the sidebar comes and goes.
pub fn min_window_size() -> Size<Pixels> {
    let column = fumi_editor::editor::column_cap(&settings::editor_width());
    size(px(240. + column + 40. * 2. + PAGE_INSET * 2.), px(600.))
}

pub struct RootView {
    pub workspace: Entity<Workspace>,
    pub pages: Entity<PageHost>,
    sidebar: Entity<SidebarView>,
    toolbar: Entity<Toolbar>,
    search: Entity<SearchPanel>,
    sheet: Entity<SettingsSheet>,
    welcome: Entity<WelcomePage>,
    publish: Entity<PublishPane>,
    caret: Entity<CaretLayer>,
    backdrop: Option<usize>,
    focus: FocusHandle,
    /// 0 is the plain sheet, 1 the framed page.
    frame: f32,
    was_open: bool,
    /// 0 is the welcome page, 1 the shell.
    shell: Tween,
    _subs: Vec<Subscription>,
}

impl RootView {
    pub fn new(start: WindowStart, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(|cx| Workspace::starting(start, cx));
        let pages = cx.new(|cx| PageHost::new(workspace.clone(), window, cx));
        let slot = Rc::new(CaretSlot::default());
        slot.on.set(true);
        let caret = cx.new(|_| CaretLayer { slot: slot.clone() });
        let blink_layer = caret.downgrade();
        let redraw: Rc<dyn Fn(&mut App)> = Rc::new(move |cx: &mut App| {
            let _ = blink_layer.update(cx, |_, cx| cx.notify());
        });
        pages.update(cx, |p, _| p.caret = Some((slot, redraw)));
        let sidebar = cx.new(|cx| SidebarView::new(workspace.clone(), window, cx));
        let toolbar = cx.new(|cx| Toolbar::new(workspace.clone(), window, cx));
        let live_pages = pages.clone();
        toolbar.update(cx, |toolbar, _| {
            toolbar.set_live_text(Rc::new(move |cx: &App| live_pages.read(cx).active_text(cx)));
        });
        let search = cx.new(|cx| SearchPanel::new(workspace.clone(), window, cx));

        let this = cx.entity().downgrade();
        let on_change: OnChange = Rc::new(move |change, window, cx| {
            let _ = this.update(cx, |root, cx| root.settings_changed(change, window, cx));
        });
        let sheet = cx.new(|cx| SettingsSheet::new(workspace.clone(), on_change, window, cx));

        let opening = workspace.clone();
        let on_open: OnOpen = Rc::new(move |path, _, cx| {
            opening.update(cx, |w, cx| {
                w.model.frame_revealed = false;
                w.open_workspace(&path, cx);
            });
        });
        let welcome = cx.new(|cx| WelcomePage::new(workspace.clone(), on_open, window, cx));

        let text_pages = pages.clone();
        let publish = cx.new(|cx| {
            PublishPane::new(workspace.clone(), Rc::new(move |cx: &App| text_pages.read(cx).active_text(cx)), window, cx)
        });
        publish.update(cx, |p, cx| p.watch(&pages, window, cx));

        let subs = vec![
            cx.observe(&publish, |_, _, cx| cx.notify()),
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.observe(&pages, |_, _, cx| cx.notify()),
            cx.observe(&sidebar, |_, _, cx| cx.notify()),
            cx.subscribe(&workspace, |_, _, _: &WorkspaceEvent, cx| cx.notify()),
            cx.subscribe(&sidebar, |_, _, e: &SidebarEvent, cx| {
                let SidebarEvent::OpenWindow { library } = e;
                let start = match library {
                    Some(path) => WindowStart::Library(path.clone()),
                    None => WindowStart::Empty,
                };
                fumi_toolbar::shell::open_window(start, cx);
            }),
            cx.subscribe_in(&toolbar, window, |this, _, e: &ToolbarEvent, window, cx| {
                this.toolbar_event(e, window, cx);
            }),
            cx.observe_window_appearance(window, |this, window, cx| this.apply_theme(window, cx)),
            cx.observe_window_bounds(window, |_, window, cx| {
                let bounds = window.window_bounds().get_bounds();
                if let Some(display) = window.display(cx) {
                    WindowFrame { display: display.id().into(), bounds }.save();
                }
            }),
        ];
        let focus = cx.focus_handle();
        // ↩ reaches the welcome page from the first frame.
        let open = workspace.read(cx).store.is_open();
        if open {
            window.focus(&focus, cx);
        } else {
            welcome.update(cx, |welcome, cx| welcome.focus(window, cx));
        }
        let mut this = RootView {
            workspace,
            pages,
            sidebar,
            toolbar,
            search,
            sheet,
            welcome,
            publish,
            caret,
            backdrop: None,
            focus,
            frame: 1.,
            was_open: open,
            shell: Tween::settled(if open { 1. } else { 0. }),
            _subs: subs,
        };
        this.apply_theme(window, cx);
        this
    }

    fn settings_changed(&mut self, change: SettingsChange, window: &mut Window, cx: &mut Context<Self>) {
        match change {
            // Settings are the app's, not this window's: every window follows.
            // Deferred, as this window is being updated right now.
            SettingsChange::Appearance | SettingsChange::Writing => cx.defer(|cx| {
                for handle in cx.windows() {
                    if let Some(root) = handle.downcast::<RootView>() {
                        let _ = root.update(cx, |root, window, cx| root.apply_theme(window, cx));
                    }
                }
            }),
            SettingsChange::Publish => {
                self.publish.update(cx, |p, cx| p.settings_changed(window, cx));
                cx.notify();
            }
        }
    }

    fn toolbar_event(&mut self, event: &ToolbarEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.publish.update(cx, |p, cx| match event {
            ToolbarEvent::CopyForWeChat => p.copy_for_wechat(cx),
            ToolbarEvent::AddXhsCards => p.add_xhs_cards(cx),
            ToolbarEvent::ExportPdf(id) => p.export_pdf(id, cx),
            ToolbarEvent::ExportAllNotes => p.export_all_notes(cx),
            ToolbarEvent::PreviewToggled => {}
        });
        cx.notify();
    }

    /// After Settings changes the theme, the mode or the accent, or the system
    /// changes appearance: the whole window redraws from new tokens.
    pub fn apply_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let system_dark = matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark);
        let tokens = theme::apply_from_settings(system_dark);
        native::set_app_appearance(match fumi_ui::settings::appearance_mode() {
            theme::AppearanceMode::Light => Some(false),
            theme::AppearanceMode::Dark => Some(true),
            theme::AppearanceMode::Auto => None,
        });
        let ns_window = native::ns_window_of(window);
        if !ns_window.is_null() {
            match (tokens.glass, self.backdrop) {
                (true, None) => self.backdrop = Some(native::install_glass_backdrop(ns_window) as usize),
                (false, Some(view)) => {
                    native::remove_backdrop(ns_window, view as _);
                    self.backdrop = None;
                }
                _ => {}
            }
            // Line Length moves the minimum; a window under it grows to fit.
            let min = min_window_size();
            native::set_content_min_size(ns_window, f32::from(min.width) as f64, f32::from(min.height) as f64);
            let current = window.viewport_size();
            if current.width < min.width && !window.is_fullscreen() {
                window.resize(size(min.width, current.height));
            }
        }
        window.set_background_appearance(if tokens.glass {
            WindowBackgroundAppearance::Transparent
        } else {
            WindowBackgroundAppearance::Opaque
        });
        self.pages.update(cx, |p, cx| p.theme_changed(cx));
        window.refresh();
    }

    /// Opened from the welcome page, the shell starts as one plain sheet, then
    /// settles into its frame: 0.12 s later, over 0.5 s, ease in-out.
    fn start_reveal(&mut self, cx: &mut Context<Self>) {
        self.frame = 0.;
        cx.spawn(async move |this: WeakEntity<RootView>, cx| {
            cx.background_executor().timer(Duration::from_millis(REVEAL_DELAY_MS)).await;
            let t0 = Instant::now();
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let p = (t0.elapsed().as_secs_f32() * 1000. / REVEAL_MS).min(1.);
                let eased = if p < 0.5 { 2. * p * p } else { 1. - (-2. * p + 2.).powi(2) / 2. };
                let alive = this.update(cx, |root, cx| {
                    root.frame = eased;
                    if p >= 1. {
                        root.workspace.update(cx, |w, _| w.model.frame_revealed = true);
                    }
                    cx.notify();
                });
                if alive.is_err() || p >= 1. {
                    break;
                }
            }
        })
        .detach();
    }

    fn page_sheet(&self) -> Div {
        let c = t();
        let flat = c.theme == theme::AppTheme::White;
        let framed = self.frame > 0.;
        let f = self.frame;
        let mut sheet = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(mix(c.page, c.page_sheet, f))
            .overflow_hidden();
        if framed {
            sheet = sheet
                .rounded(px(PAGE_RADIUS * f))
                .border_1()
                .border_color(c.page_edge.opacity(f));
            if !flat {
                let shadow = |a: f32, y: f32, blur: f32| BoxShadow {
                    color: hsla(0., 0., 0., a * f),
                    offset: point(px(0.), px(y)),
                    blur_radius: px(blur),
                    spread_radius: px(0.),
                    inset: false,
                };
                sheet = sheet.shadow(vec![shadow(0.05, 1., 1.5), shadow(0.04, 4., 8.)]);
            }
        }
        sheet
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        fumi_toolbar::keep_focus(&self.focus, window);
        let (open, revealed) = {
            let w = self.workspace.read(cx);
            (w.store.is_open(), w.model.frame_revealed)
        };
        if open && !self.was_open && !revealed {
            self.start_reveal(cx);
        }
        if !open && self.was_open {
            self.welcome.clone().update(cx, |welcome, cx| welcome.shown_again(window, cx));
        }
        self.was_open = open;
        let now = Instant::now();
        self.shell.go(if open { 1. } else { 0. }, SWITCH, now);
        if self.shell.is_moving(now) {
            window.request_animation_frame();
        }
        let shell = self.shell.value(now);

        let f = self.frame;
        let ground = mix(c.page, c.ground_fill, f);
        let root = div()
            .id("root")
            .key_context("FumiRoot")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .text_color(c.text)
            .font(fumi_editor::display::ui_font())
            .text_size(px(theme::text_size::BODY));
        let root = fumi_toolbar::register_actions(root, &self.toolbar, cx);
        let root = fumi_sidebar::wire_actions(&self.sidebar, root);
        let root = {
            let (toggle, libraries) = (self.sidebar.clone(), self.sidebar.clone());
            root.on_action(move |_: &fumi_toolbar::actions::ToggleSidebar, _, cx| {
                toggle.update(cx, |s, cx| s.toggle_sidebar(cx))
            })
            // Without a library there's nothing to show: the menu item greys out.
            .when(open, |root| {
                root.on_action(move |_: &fumi_toolbar::actions::ShowLibraries, _, cx| {
                    libraries.update(cx, |s, cx| s.show_libraries(cx))
                })
            })
            .bg(mix(c.page, ground, shell))
        };
        let layer = |opacity: f32| div().absolute().inset_0().opacity(opacity);

        let root = root.when(shell < 1., |root| root.child(layer(1. - shell).child(self.welcome.clone())));
        if shell <= 0. {
            return root;
        }

        // Views that redraw only when their own state changes: a blink of the
        // caret layer, above them, then repaints none of them.
        let sidebar_width = self.sidebar.read(cx).current_width();
        let sidebar = self
            .sidebar
            .clone()
            .cached(StyleRefinement::default().flex_none().h_full().w(px(sidebar_width)));
        let inset = px(PAGE_INSET * f);
        let detail = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            .child(
                div()
                    .size_full()
                    .pt(px(TOOLBAR_HEIGHT))
                    .px(inset)
                    .pb(inset)
                    .child(fumi_publish::beside_page(
                        self.page_sheet().size_full().child(self.pages.clone().cached(StyleRefinement::default().size_full())),
                        &self.publish,
                        PAGE_RADIUS,
                        cx,
                    )),
            )
            // Painted after the page: the active tab's flare meets its top edge.
            .child(div().absolute().top_0().left_0().right_0().h(px(TOOLBAR_HEIGHT)).child(self.toolbar.clone().cached(StyleRefinement::default().size_full())));
        root.child(
            layer(shell)
                .child(div().size_full().flex().child(sidebar).child(detail))
            .child(self.caret.clone())
                .child(self.search.clone())
                .child(self.sheet.clone()),
        )
    }
}
