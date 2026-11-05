use fumi_editor::editor::{self, Editor, Quit};
use fumi_editor::{driver, metrics};
use gpui::*;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let t0 = Instant::now();
    metrics::now();
    let path = std::env::args().nth(1).map(PathBuf::from);
    let drive = std::env::var("FUMI_DRIVE").ok();
    let blur = std::env::var("FUMI_BLUR").is_ok();

    gpui_platform::application().run(move |cx: &mut App| {
        let dark = matches!(fumi_ui::settings::appearance_mode(), fumi_ui::theme::AppearanceMode::Dark);
        fumi_ui::theme::set_active(fumi_ui::theme::Tokens::resolve(
            fumi_ui::settings::color_theme(),
            dark,
            &fumi_ui::settings::accent_hex(),
        ));
        metrics::install_runloop_observer();
        editor::bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let bytes = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .unwrap_or_else(|| b"# Fumi on GPUI\n\nOpen a note: fumi-gpui path/to/note.md\n".to_vec());
        let read_ms = t0.elapsed().as_secs_f64() * 1000.;

        let bounds = Bounds::centered(None, size(px(1100.), px(800.)), cx);
        let blink = drive.is_none();
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Fumi GPUI".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    window_background: if blur {
                        WindowBackgroundAppearance::Blurred
                    } else {
                        WindowBackgroundAppearance::Opaque
                    },
                    ..Default::default()
                },
                move |window, cx| {
                    let path = path.clone();
                    let view = cx.new(move |cx| Editor::new(bytes, path, blink, cx));
                    window.focus(&view.read(cx).focus.clone(), cx);
                    view
                },
            )
            .unwrap();
        cx.activate(true);

        if let Some(scenario) = drive.clone() {
            driver::start(scenario, window, t0, read_ms, cx);
        }
    });
}
