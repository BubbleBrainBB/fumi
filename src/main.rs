//! Fumi. `FUMI_DRIVE=scenario fumi-gpui note.md` runs a benchmark on the bare
//! editor; otherwise this is the app.

mod pages;
mod root;
mod workspace;

use fumi_editor::editor::{self, Editor, Quit};
use fumi_editor::{driver, metrics};
use gpui::*;
use root::RootView;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let t0 = Instant::now();
    metrics::now();
    match std::env::var("FUMI_DRIVE").ok() {
        Some(scenario) => run_bare_editor(Some(scenario), t0),
        None if std::env::var("FUMI_BARE").is_ok() => run_bare_editor(None, t0),
        None => run_app(),
    }
}

fn run_app() {
    gpui_platform::application().run(move |cx: &mut App| {
        editor::bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(1016.), px(600.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Fumi".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(19.), px(18.))),
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| RootView::new(window, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}

fn run_bare_editor(drive: Option<String>, t0: Instant) {
    let path = std::env::args().nth(1).map(PathBuf::from);
    gpui_platform::application().run(move |cx: &mut App| {
        fumi_ui::theme::apply_from_settings(false);
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
                    ..Default::default()
                },
                move |window, cx| {
                    let path = path.clone();
                    let view = cx.new(move |cx| {
                        let mut e = Editor::new(bytes, path, blink, cx);
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
            driver::start(scenario, window, t0, read_ms, cx);
        }
    });
}
