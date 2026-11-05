//! The window: ground, sidebar, the page floating on it, what floats over that.
//! A port of `AppShellView`.

use crate::pages::{PageHost, ToolbarContent};
use crate::workspace::{Workspace, WorkspaceEvent};
use fumi_core::tabs::Content;
use fumi_ui::native;
use fumi_ui::theme::{self, t};
use gpui::*;

pub const SIDEBAR_WIDTH: f32 = 240.;
pub const PAGE_INSET: f32 = 8.;
pub const PAGE_RADIUS: f32 = 12.;
/// Height of the toolbar band above the page.
pub const TOOLBAR_HEIGHT: f32 = 52.;

pub struct RootView {
    pub workspace: Entity<Workspace>,
    pub pages: Entity<PageHost>,
    backdrop: Option<usize>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl RootView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(Workspace::new);
        let pages = cx.new(|cx| PageHost::new(workspace.clone(), window, cx));
        let subs = vec![
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.observe(&pages, |_, _, cx| cx.notify()),
            cx.subscribe(&workspace, |_, _, _: &WorkspaceEvent, cx| cx.notify()),
            cx.observe_window_appearance(window, |this, window, cx| {
                this.apply_theme(window, cx);
            }),
        ];
        let mut this = RootView { workspace, pages, backdrop: None, focus: cx.focus_handle(), _subs: subs };
        this.apply_theme(window, cx);
        this
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

    fn page_sheet(&self, framed: bool) -> Div {
        let c = t();
        let flat = c.theme == theme::AppTheme::White;
        let mut sheet = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(if framed { c.page_sheet } else { c.page })
            .overflow_hidden();
        if framed {
            sheet = sheet.rounded(px(PAGE_RADIUS)).border_1().border_color(c.page_edge);
            if !flat {
                sheet = sheet.shadow(vec![
                    BoxShadow {
                        color: hsla(0., 0., 0., 0.05),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(1.5),
                        spread_radius: px(0.),
                        inset: false,
                    },
                    BoxShadow {
                        color: hsla(0., 0., 0., 0.04),
                        offset: point(px(0.), px(4.)),
                        blur_radius: px(8.),
                        spread_radius: px(0.),
                        inset: false,
                    },
                ]);
            }
        }
        sheet
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let w = self.workspace.read(cx);
        let framed = w.model.frame_revealed;
        let ground = if framed { c.ground_fill } else { c.page };
        let open = w.store.is_open();
        let sidebar_visible = w.model.sidebar_visible;
        let toolbar: ToolbarContent = self.pages.read(cx).toolbar(cx);

        let root = div()
            .id("root")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(ground)
            .text_color(c.text)
            .font(fumi_editor::display::ui_font())
            .text_size(px(theme::text_size::BODY));

        if !open {
            return root.child(div().size_full().flex().items_center().justify_center().child("Open a folder to begin"));
        }

        let inset = if framed { px(PAGE_INSET) } else { px(0.) };
        let sidebar = sidebar_visible.then(|| {
            div()
                .w(px(SIDEBAR_WIDTH))
                .h_full()
                .flex_none()
                .pt(px(TOOLBAR_HEIGHT))
                .text_color(c.text3)
                .child(div().px(px(14.)).child("Sidebar"))
        });
        let detail = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                // The band the title and icons sit in; dragging it moves the window.
                div()
                    .h(px(TOOLBAR_HEIGHT))
                    .flex_none()
                    .w_full()
                    .flex()
                    .items_center()
                    .pl(if sidebar_visible { px(14.) } else { px(132.) })
                    .pr(px(14.))
                    .window_control_area(WindowControlArea::Drag)
                    .child(div().text_color(c.text2).font_weight(FontWeight::MEDIUM).child(toolbar.title))
                    .child(div().flex_1()),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px(inset)
                    .pb(inset)
                    .child(self.page_sheet(framed).size_full().child(self.pages.clone())),
            );
        root.child(div().size_full().flex().children(sidebar).child(detail))
    }
}

/// What the active page is, for callers that branch on it.
pub fn content_label(content: &Content) -> &'static str {
    match content {
        Content::Draft => "draft",
        Content::Note(_) => "note",
        Content::Memos => "memos",
        Content::File(_) => "file",
    }
}
