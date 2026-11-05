//! Opens the sidebar in a window, over the library in FUMI_WORKSPACE, the way root
//! puts it together: the column in a row, on the window ground, its commands wired
//! to the outermost element.
use fumi_sidebar::{SidebarEvent, SidebarView, wire_actions};
use fumi_workspace::Workspace;
use gpui::*;

struct Demo {
    workspace: Entity<Workspace>,
    sidebar: Entity<SidebarView>,
    // Commands reach the root element by way of the focused one.
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = fumi_ui::theme::t();
        let w = self.workspace.read(cx);
        let status = format!(
            "{} notes, {:?}, sidebar {}",
            w.store.notes().len(),
            w.model.active_content(),
            if w.model.sidebar_visible { "shown" } else { "hidden" },
        );
        wire_actions(&self.sidebar, div().id("demo").track_focus(&self.focus))
            .size_full()
            .flex()
            .bg(c.ground_fill)
            .font(font(".SystemUIFont"))
            .text_color(c.text)
            .child(self.sidebar.clone())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pt(px(52.))
                    .px(px(8.))
                    .pb(px(8.))
                    .child(div().size_full().rounded(px(12.)).bg(c.page).p(px(16.)).text_size(px(13.)).child(status)),
            )
    }
}

fn open_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Sidebar".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(19.), px(18.))),
            }),
            ..Default::default()
        },
        |window, cx| {
            let workspace = cx.new(Workspace::new);
            let sidebar = cx.new(|cx| SidebarView::new(workspace.clone(), window, cx));
            cx.new(|cx| {
                let subs = vec![cx.subscribe(&sidebar, |_, _, event: &SidebarEvent, cx| match event {
                    SidebarEvent::OpenWindow { .. } => open_window(cx),
                })];
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                Demo { workspace, sidebar, focus, _subs: subs }
            })
        },
    )
    .unwrap();
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        fumi_ui::text_field::bind_keys(cx);
        fumi_sidebar::bind_keys(cx);
        fumi_ui::theme::apply_from_settings(false);
        open_window(cx);
        cx.activate(true);
    });
}
