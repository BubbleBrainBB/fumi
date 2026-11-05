//! Opens the note in FUMI_NOTE in the editor alone, with FUMI_LIBRARY as its
//! library, at FUMI_START_LINE. `FUMI_DRIVE=script` runs a driver scenario.
use fumi_editor::editor::{self, Editor, Quit};
use fumi_editor::{driver, metrics};
use gpui::*;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let t0 = Instant::now();
    metrics::now();
    let path = std::env::var_os("FUMI_NOTE").map(PathBuf::from);
    let drive = std::env::var("FUMI_DRIVE").ok();
    gpui_platform::application().run(move |cx: &mut App| {
        fumi_ui::theme::apply_from_settings(false);
        metrics::install_runloop_observer();
        editor::bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let bytes = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .unwrap_or_else(|| b"Set FUMI_NOTE to a note.\n".to_vec());
        let bounds = Bounds::centered(None, size(px(1100.), px(800.)), cx);
        let blink = drive.is_none();
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Editor demo".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    let path = path.clone();
                    let view = cx.new(move |cx| {
                        let mut e = Editor::new(bytes, path, blink, cx);
                        e.library = std::env::var_os("FUMI_LIBRARY").map(PathBuf::from);
                        if let Some(line) = std::env::var("FUMI_START_LINE").ok().and_then(|v| v.parse().ok()) {
                            e.scroll_to_line(line, cx);
                        }
                        e
                    });
                    window.focus(&view.read(cx).focus.clone(), cx);
                    view
                },
            )
            .unwrap();
        cx.activate(true);
        if let Some(scenario) = drive.clone() {
            driver::start(scenario, window, t0, 0., cx);
        }
    });
}
