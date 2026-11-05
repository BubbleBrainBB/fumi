//! The window: ground, sidebar, the page floating on it, what floats over that.
//! A port of `AppShellView`. Each screen lives in its own crate; this only places
//! them and connects what they ask of each other.

use crate::pages::PageHost;
use fumi_settings::search::SearchPanel;
use fumi_settings::sheet::{OnChange, SettingsChange, SettingsSheet};
use fumi_settings::welcome::{OnOpen, WelcomePage};
use fumi_sidebar::{SidebarEvent, SidebarView};
use fumi_toolbar::{Toolbar, ToolbarEvent};
use fumi_ui::native;
use fumi_ui::theme::{self, t};
use fumi_workspace::{WindowStart, Workspace, WorkspaceEvent};
use gpui::*;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub const PAGE_INSET: f32 = 8.;
pub const PAGE_RADIUS: f32 = 12.;
pub const TOOLBAR_HEIGHT: f32 = 52.;

/// The welcome page's plain sheet settling into the framed window.
const REVEAL_DELAY_MS: u64 = 120;
const REVEAL_MS: f32 = 500.;

pub struct RootView {
    pub workspace: Entity<Workspace>,
    pub pages: Entity<PageHost>,
    sidebar: Entity<SidebarView>,
    toolbar: Entity<Toolbar>,
    search: Entity<SearchPanel>,
    sheet: Entity<SettingsSheet>,
    welcome: Entity<WelcomePage>,
    backdrop: Option<usize>,
    focus: FocusHandle,
    /// 0 is the plain sheet, 1 the framed page.
    frame: f32,
    was_open: bool,
    _subs: Vec<Subscription>,
}

impl RootView {
    pub fn new(start: WindowStart, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(|cx| Workspace::starting(start, cx));
        let pages = cx.new(|cx| PageHost::new(workspace.clone(), window, cx));
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

        let subs = vec![
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
        ];
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut this = RootView {
            workspace,
            pages,
            sidebar,
            toolbar,
            search,
            sheet,
            welcome,
            backdrop: None,
            focus,
            frame: 1.,
            was_open: false,
            _subs: subs,
        };
        this.apply_theme(window, cx);
        this
    }

    fn settings_changed(&mut self, change: SettingsChange, window: &mut Window, cx: &mut Context<Self>) {
        match change {
            SettingsChange::Appearance | SettingsChange::Writing => self.apply_theme(window, cx),
            SettingsChange::Publish => cx.notify(),
        }
    }

    fn toolbar_event(&mut self, _event: &ToolbarEvent, _window: &mut Window, cx: &mut Context<Self>) {
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
            .bg(if f >= 1. { c.page_sheet } else { c.page })
            .overflow_hidden();
        if framed {
            sheet = sheet
                .rounded(px(PAGE_RADIUS * f))
                .border_1()
                .border_color(c.page_edge.opacity(c.page_edge.a * f));
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

fn mix(a: Hsla, b: Hsla, f: f32) -> Hsla {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    Rgba { r: a.r + (b.r - a.r) * f, g: a.g + (b.g - a.g) * f, b: a.b + (b.b - a.b) * f, a: a.a + (b.a - a.a) * f }.into()
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
            self.welcome.clone().update(cx, |welcome, cx| welcome.focus(window, cx));
        }
        self.was_open = open;

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
            .on_action(move |_: &fumi_toolbar::actions::ShowLibraries, _, cx| {
                libraries.update(cx, |s, cx| s.show_libraries(cx))
            })
        };

        if !open {
            return root.bg(c.page).child(self.welcome.clone());
        }

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
                    .child(self.page_sheet().size_full().child(self.pages.clone())),
            )
            // Painted after the page: the active tab's flare meets its top edge.
            .child(div().absolute().top_0().left_0().right_0().h(px(TOOLBAR_HEIGHT)).child(self.toolbar.clone()));
        root.bg(ground)
            .child(div().size_full().flex().child(self.sidebar.clone()).child(detail))
            .child(self.search.clone())
            .child(self.sheet.clone())
    }
}
