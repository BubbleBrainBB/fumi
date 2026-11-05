//! Opens the Memos page in a window, over the library in FUMI_WORKSPACE, with
//! quick capture installed: ⌃⌥N or the menu bar icon, saving to the same library.
use fumi_memos::{MemosView, QuickCapture};
use fumi_workspace::Workspace;
use gpui::*;

struct Demo {
    memos: Entity<MemosView>,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(fumi_ui::theme::t().page).child(self.memos.clone())
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        fumi_ui::text_field::bind_keys(cx);
        fumi_memos::bind_keys(cx);
        fumi_ui::theme::apply_from_settings(false);
        QuickCapture::install(cx);
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
            |window, cx| {
                let workspace = cx.new(Workspace::new);
                let memos = cx.new(|cx| MemosView::new(workspace.clone(), window, cx));
                cx.new(|_| Demo { memos })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
