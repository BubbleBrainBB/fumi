//! Fumi. `FUMI_DRIVE=scenario fumi-gpui note.md` runs a benchmark on the bare
//! editor; otherwise this is the app.

mod autopilot;
mod pages;
mod root;

use fumi_editor::editor::{self, Editor, Quit};
use fumi_editor::{driver, metrics};
use fumi_toolbar::AppHooks;
use fumi_ui::window_frame::{FIRST_SIZE, WindowFrame};
use fumi_workspace::WindowStart;
use gpui::*;
use root::RootView;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

/// When the process began, for the benchmark's launch times.
static LAUNCHED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn main() {
    let t0 = *LAUNCHED.get_or_init(Instant::now);
    metrics::now();
    match std::env::var("FUMI_DRIVE").ok() {
        Some(scenario) => run_bare_editor(Some(scenario), t0),
        None if std::env::var("FUMI_BARE").is_ok() => run_bare_editor(None, t0),
        None => run_app(),
    }
}

/// A fresh install's first window is centered at 1200 x 800. After that a launch's
/// first window comes back where the last one was; later ones take its size, centered.
fn opening_frame(start: &WindowStart, min: Size<Pixels>, cx: &App) -> (Bounds<Pixels>, Option<DisplayId>) {
    let first_size = size(px(FIRST_SIZE.0), px(FIRST_SIZE.1));
    let Some(saved) = WindowFrame::load() else {
        return (Bounds::centered(None, first_size, cx), None);
    };
    let display = cx.displays().into_iter().find(|display| u64::from(display.id()) == saved.display);
    match display {
        Some(display) if matches!(start, WindowStart::Restore) => (saved.place(min, display.visible_bounds()), Some(display.id())),
        _ => {
            let visible = cx.primary_display().map_or(saved.bounds.size, |primary| primary.visible_bounds().size);
            (Bounds::centered(None, saved.fitted_size(min, visible), cx), None)
        }
    }
}

fn open_window(start: WindowStart, cx: &mut App) {
    let min = root::min_window_size();
    let (bounds, display_id) = opening_frame(&start, min, cx);
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id,
            window_min_size: Some(min),
            titlebar: Some(TitlebarOptions {
                title: Some("Fumi".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(19.), px(18.))),
            }),
            ..Default::default()
        },
        |window, cx| {
            let root = cx.new(|cx| RootView::new(start, window, cx));
            if std::env::var("BENCH_SCENARIO").is_ok() {
                autopilot::start(root.downgrade(), *LAUNCHED.get_or_init(Instant::now), window, cx);
            }
            root
        },
    );
    cx.activate(true);
}

fn run_app() {
    let app = gpui_platform::application();
    app.on_reopen(|cx| fumi_toolbar::handle_dock_reopen(cx));
    app.run(move |cx: &mut App| {
        fumi_settings::sheet::migrate_old_settings();
        fumi_ui::theme::apply_from_settings(false);
        // The last bound wins: the menu bar's own shortcuts go last.
        editor::bind_keys(cx);
        fumi_sidebar::bind_keys(cx);
        fumi_settings::bind_keys(cx);
        fumi_memos::bind_keys(cx);
        fumi_toolbar::install(
            cx,
            AppHooks { open_window: Rc::new(open_window), new_memo: Rc::new(fumi_memos::QuickCapture::show) },
        );
        fumi_memos::QuickCapture::install(cx);
        fumi_toolbar::open_first_window(cx);
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
