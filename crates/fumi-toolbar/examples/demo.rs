//! Opens the app's shell around a stand-in page, over the library in FUMI_WORKSPACE:
//! how a root wires the toolbar, the menu bar and the Dock. The app's welcome
//! page is `fumi_settings::welcome::WelcomePage`.
use fumi_toolbar::{AppHooks, Toolbar, ToolbarEvent};
use fumi_workspace::{WindowStart, Workspace};
use gpui::prelude::*;
use gpui::*;
use std::rc::Rc;

struct Root {
    workspace: Entity<Workspace>,
    toolbar: Entity<Toolbar>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl Root {
    fn new(start: WindowStart, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(|cx| Workspace::starting(start, cx));
        let toolbar = cx.new(|cx| Toolbar::new(workspace.clone(), window, cx));
        toolbar.update(cx, |toolbar, _| {
            toolbar.set_live_text(Rc::new(|_: &App| None));
        });
        let subs = vec![
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.subscribe(&toolbar, |_, _, event: &ToolbarEvent, _| eprintln!("toolbar event: {event:?}")),
        ];
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Root { workspace, toolbar, focus, _subs: subs }
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = fumi_ui::theme::t();
        fumi_toolbar::keep_focus(&self.focus, window);
        let root = div().id("root").track_focus(&self.focus).size_full().bg(c.ground_fill).text_color(c.text);
        let root = fumi_toolbar::register_actions(root, &self.toolbar, cx);
        let w = self.workspace.read(cx);
        if !w.store.is_open() {
            return root.p(px(40.)).child("No library open: set FUMI_WORKSPACE to a notes folder.");
        }
        let page = format!("{} notes", w.store.notes().len());
        root.flex().flex_col().child(self.toolbar.clone()).child(
            div()
                .flex_1()
                .min_h_0()
                .px(px(8.))
                .pb(px(8.))
                .child(div().size_full().bg(c.page).rounded(px(12.)).border_1().border_color(c.page_edge).p(px(40.)).child(page)),
        )
    }
}

fn open_window(start: WindowStart, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Demo".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(19.), px(18.))),
            }),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| Root::new(start, window, cx)),
    )
    .unwrap();
    cx.activate(true);
}

fn main() {
    let app = gpui_platform::application();
    app.on_reopen(|cx| fumi_toolbar::handle_dock_reopen(cx));
    app.run(|cx: &mut App| {
        fumi_ui::theme::apply_from_settings(false);
        fumi_toolbar::install(
            cx,
            AppHooks { open_window: Rc::new(open_window), new_memo: Rc::new(|_| eprintln!("new memo")) },
        );
        fumi_toolbar::open_first_window(cx);
    });
}
