//! Opens this crate's screens in a window, over the library in FUMI_WORKSPACE.
use fumi_workspace::Workspace;
use gpui::*;

struct Demo {
    workspace: Entity<Workspace>,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let notes = self.workspace.read(cx).store.notes().len();
        div().size_full().bg(fumi_ui::theme::t().page).child(format!("{notes} notes"))
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        fumi_ui::text_field::bind_keys(cx);
        fumi_ui::theme::apply_from_settings(false);
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
            |_, cx| {
                let workspace = cx.new(Workspace::new);
                cx.new(|_| Demo { workspace })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
