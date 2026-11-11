//! A benchmark that drives the whole app, for comparing with the Swift app's
//! `Autopilot.swift` (the evaluation's harness): the same scenarios, the same
//! library and notes, the same numbers. Main-thread work is measured as busy
//! stretches between run-loop waits; `op` is how long the call itself took.
//!
//! BENCH_LIB=<library> BENCH_SCENARIO=launch|open|idle|type|slow|newline|ime|caret|scroll
//! BENCH_DOC=bench200k. Prints its results and quits.

use crate::root::RootView;
use fumi_core::markdown::Kind;
use fumi_editor::editor::Editor;
use fumi_editor::metrics;
use gpui::*;
use std::time::{Duration, Instant};

struct Pilot {
    t0: Instant,
    slices: Vec<f64>,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.
}

fn pctl(a: &[f64], p: f64) -> f64 {
    if a.is_empty() {
        return 0.;
    }
    let mut s = a.to_vec();
    s.sort_by(|x, y| x.partial_cmp(y).unwrap());
    s[((s.len() as f64 * p) as usize).min(s.len() - 1)]
}

fn log(s: String) {
    println!("{s}");
}

impl Pilot {
    /// Collects what the run-loop observer recorded. Stretches under 0.2 ms are
    /// dropped, as the Swift harness drops them.
    fn drain(&mut self) {
        for ns in metrics::take_slices() {
            let d = ns as f64 / 1e6;
            if d > 0.2 {
                self.slices.push(d);
            }
        }
    }

    fn report(&mut self, name: &str, op: &[f64]) {
        self.drain();
        let s = &self.slices;
        let mut line = format!(
            "{:<26} busy n={:<5} p50={:7.2} p95={:7.2} max={:8.2} sum={:9.1} over16={:<4} over100={:<3}",
            name,
            s.len(),
            pctl(s, 0.5),
            pctl(s, 0.95),
            s.iter().cloned().fold(0., f64::max),
            s.iter().sum::<f64>(),
            s.iter().filter(|d| **d > 16.7).count(),
            s.iter().filter(|d| **d > 100.).count(),
        );
        if !op.is_empty() {
            line += &format!(
                " | op n={} p50={:.2} p95={:.2} max={:.2}",
                op.len(),
                pctl(op, 0.5),
                pctl(op, 0.95),
                op.iter().cloned().fold(0., f64::max)
            );
        }
        log(line);
        self.slices.clear();
    }
}

async fn sleep(cx: &mut AsyncWindowContext, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
}

/// Waits until no stretch over 8 ms has happened for `quiet` ms.
async fn settle(p: &mut Pilot, cx: &mut AsyncWindowContext, quiet: f64, limit: f64) -> f64 {
    let start = Instant::now();
    let mut last_big = start;
    let mut seen = 0;
    while ms(start.elapsed()) < limit {
        sleep(cx, 10).await;
        p.drain();
        for s in &p.slices[seen..] {
            if *s > 8. {
                last_big = Instant::now();
            }
        }
        seen = p.slices.len();
        if ms(last_big.elapsed()) > quiet {
            break;
        }
    }
    ms(last_big - start)
}

fn editor(root: &WeakEntity<RootView>, cx: &mut AsyncWindowContext) -> Option<Entity<Editor>> {
    root.update(cx, |r, cx| {
        let pages = r.pages.read(cx);
        let active = r.workspace.read(cx).model.active_id().to_string();
        pages.pages.get(&active).map(|p| p.editor.clone())
    })
    .ok()
    .flatten()
}

fn text_len(root: &WeakEntity<RootView>, cx: &mut AsyncWindowContext) -> Option<usize> {
    let e = editor(root, cx)?;
    Some(e.read_with(cx, |e, _| e.text().len()))
}

/// A line over 20 characters, not a heading, near `fraction` of the note: a plain
/// paragraph, or with `fast` false a line of any other kind (Swift's slow path).
fn spot(e: &Editor, fraction: f64, fast: bool) -> Option<usize> {
    let text = e.text();
    let start_line = e.doc.line_of((text.len() as f64 * fraction) as usize);
    for line in start_line..e.lines.len() {
        let t = e.doc.line_text(line);
        let ok_kind = (e.lines[line].kind == Kind::Para) == fast;
        if t.chars().count() > 20 && !t.starts_with('#') && ok_kind {
            let at = e.doc.line_start(line);
            let off = t.char_indices().nth(8).map_or(0, |(i, _)| i);
            return Some(at + off);
        }
    }
    None
}

async fn open(p: &mut Pilot, root: &WeakEntity<RootView>, lib: &str, name: &str, cx: &mut AsyncWindowContext) {
    let before = text_len(root, cx);
    settle(p, cx, 200., 60000.).await;
    p.slices.clear();
    metrics::take_slices();
    let start = Instant::now();
    let path = format!("{lib}/{name}.md");
    let _ = root.update(cx, |r, cx| {
        r.workspace.update(cx, |w, cx| {
            w.model.follow_link(&path, false);
            w.navigated(cx);
        });
    });
    while ms(start.elapsed()) < 60000. {
        sleep(cx, 5).await;
        if let Some(l) = text_len(root, cx)
            && Some(l) != before
            && l > 0
        {
            break;
        }
    }
    let d = settle(p, cx, 500., 60000.).await;
    p.report(&format!("open {name}  t={}ms", d as i64), &[]);
}

pub fn start(root: WeakEntity<RootView>, t0: Instant, window: &mut Window, cx: &mut App) {
    let scenario = std::env::var("BENCH_SCENARIO").unwrap_or_default();
    let lib = std::env::var("BENCH_LIB").unwrap_or_default();
    let doc = std::env::var("BENCH_DOC").unwrap_or_else(|_| "bench200k".into());
    metrics::install_runloop_observer();
    metrics::record_frames(true);
    window
        .spawn(cx, async move |cx| {
            let mut p = Pilot { t0, slices: Vec::new() };
            // Notes loaded.
            loop {
                sleep(cx, 5).await;
                let loaded = root
                    .update(cx, |r, cx| r.workspace.read(cx).store.has_loaded_notes())
                    .unwrap_or(false);
                if loaded {
                    break;
                }
                if p.t0.elapsed() > Duration::from_secs(100) {
                    log("never loaded".into());
                    let _ = cx.update(|_, cx| cx.quit());
                    return;
                }
            }
            let (notes, files) = root
                .update(cx, |r, cx| {
                    let w = r.workspace.read(cx);
                    (w.store.notes().len(), w.store.files().len())
                })
                .unwrap_or((0, 0));
            log(format!(
                "launch->notesLoaded {:.0} ms notes={notes} files={files} footprint={:.0}MB",
                ms(p.t0.elapsed()),
                metrics::footprint_mb().0
            ));
            let s = settle(&mut p, cx, 500., 60000.).await;
            log(format!(
                "launch->settled {:.0} ms (settle {:.0}) footprint={:.0}MB",
                ms(p.t0.elapsed()),
                s,
                metrics::footprint_mb().0
            ));
            p.report("launch", &[]);
            run(&mut p, &root, &scenario, &lib, &doc, cx).await;
            log("DONE".into());
            let _ = cx.update(|_, cx| cx.quit());
        })
        .detach();
}

async fn run(p: &mut Pilot, root: &WeakEntity<RootView>, scenario: &str, lib: &str, doc: &str, cx: &mut AsyncWindowContext) {
    match scenario {
        "launch" => {
            for q in ["性能", "plain", "zzzqqq"] {
                let a = Instant::now();
                let n = root
                    .update(cx, |r, cx| fumi_core::fuzzy_search::hits(r.workspace.read(cx).store.notes(), q, 100).len())
                    .unwrap_or(0);
                log(format!("search {q} hits={n} {:.1} ms", ms(a.elapsed())));
            }
            sleep(cx, 500).await;
        }
        "select" => {
            // A drag across two paragraphs with the pointer's own events, one a
            // step so frames run between them, then what the window shows.
            open(p, root, lib, doc, cx).await;
            let _ = cx.update(|window, cx| {
                cx.activate(true);
                window.activate_window();
                fumi_editor::driver::aim_at(window);
            });
            sleep(cx, 500).await;
            let Some(e) = editor(root, cx) else { return log("FAIL no editor".into()) };
            let spots = e.read_with(cx, |e, _| {
                let painted = e.painted.borrow();
                let long: Vec<_> = painted
                    .iter()
                    .filter(|i| e.doc.line_text(i.first_line).chars().count() > 20)
                    .map(|i| i.bounds)
                    .collect();
                (long.len() >= 2).then(|| (long[0], long[1]))
            });
            let Some((a, b)) = spots else { return log("FAIL no two long lines on screen".into()) };
            let f = |p: Pixels| f32::from(p) as f64;
            let (x0, y0) = (f(a.center().x) - 120., f(a.top()) + 12.);
            let (x1, y1) = (f(b.center().x) + 120., f(b.top()) + 12.);
            let dir = std::env::var("FUMI_SHOTS").unwrap_or_default();
            fumi_editor::driver::post_mouse(1, x0, y0);
            sleep(cx, 150).await;
            fumi_editor::driver::post_mouse(6, (x0 + x1) / 2., (y0 + y1) / 2.);
            sleep(cx, 150).await;
            fumi_editor::driver::post_mouse(6, x1, y1);
            sleep(cx, 400).await;
            let (anchor, head) = e.read_with(cx, |e, _| (e.anchor, e.head));
            log(format!("RESULT select held anchor={anchor} head={head} from=({x0:.0},{y0:.0}) to=({x1:.0},{y1:.0})"));
            log(format!("RESULT selectshot {}", fumi_editor::driver::self_shot(&format!("{dir}/app-held.png"))));
            fumi_editor::driver::post_mouse(2, x1, y1);
            sleep(cx, 400).await;
            let (anchor, head) = e.read_with(cx, |e, _| (e.anchor, e.head));
            log(format!("RESULT select released anchor={anchor} head={head}"));
            log(format!("RESULT selectshot {}", fumi_editor::driver::self_shot(&format!("{dir}/app-released.png"))));
        }
        "open" => {
            for d in ["bench10k", "bench50k", "bench200k", "bench1m", "bench10k", "bench200k"] {
                open(p, root, lib, d, cx).await;
                log(format!("  footprint={:.0}MB", metrics::footprint_mb().0));
            }
        }
        "idle" => {
            open(p, root, lib, doc, cx).await;
            // Focused, the caret blinking: a writer pausing mid-sentence.
            if std::env::var("BENCH_ACTIVATE").is_ok() {
                let _ = cx.update(|window, cx| {
                    cx.activate(true);
                    window.activate_window();
                });
                sleep(cx, 500).await;
            }
            if let Some(e) = editor(root, cx)
                && let Some(at) = e.read_with(cx, |e, _| spot(e, 0.5, true))
            {
                let _ = cx.update(|window, cx| {
                    window.focus(&e.read(cx).focus.clone(), cx);
                    e.update(cx, |e, _| {
                        e.anchor = at;
                        e.head = at;
                        e.set_caret_for_driver();
                    });
                });
            }
            sleep(cx, 5000).await;
            p.drain();
            p.slices.clear();
            let cpu = || {
                let mut u: libc::rusage = unsafe { std::mem::zeroed() };
                unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
                let t = |v: libc::timeval| v.tv_sec as f64 + v.tv_usec as f64 / 1e6;
                t(u.ru_utime) + t(u.ru_stime)
            };
            let (c0, w0) = (cpu(), Instant::now());
            sleep(cx, 10000).await;
            log(format!("idle cpu={:.2}%", (cpu() - c0) / w0.elapsed().as_secs_f64() * 100.));
            p.report("idle", &[]);
        }
        "type" | "slow" | "newline" | "ime" => {
            open(p, root, lib, doc, cx).await;
            let Some(e) = editor(root, cx) else { return log("no text view".into()) };
            let Some(at) = e.read_with(cx, |e, _| spot(e, 0.5, scenario != "slow")) else {
                return log("no spot".into());
            };
            let _ = cx.update(|window, cx| {
                window.focus(&e.read(cx).focus.clone(), cx);
                e.update(cx, |e, _| {
                    e.anchor = at;
                    e.head = at;
                    e.set_caret_for_driver();
                });
            });
            sleep(cx, 300).await;
            p.drain();
            p.slices.clear();
            let mut op = Vec::new();
            let n = match scenario {
                "ime" => 12,
                "newline" => 15,
                _ => 50,
            };
            for i in 0..n {
                let a = Instant::now();
                match scenario {
                    "newline" => {
                        let _ = cx.update(|window, cx| e.update(cx, |e, cx| e.replace_text_in_range(None, "\n", window, cx)));
                    }
                    "ime" => {
                        for py in ["z", "zh", "zho", "zhon", "zhong"] {
                            let b = Instant::now();
                            let len16 = py.len();
                            let _ = cx.update(|window, cx| {
                                e.update(cx, |e, cx| e.replace_and_mark_text_in_range(None, py, Some(len16..len16), window, cx))
                            });
                            op.push(ms(b.elapsed()));
                            sleep(cx, 100).await;
                        }
                        let b = Instant::now();
                        let _ = cx.update(|window, cx| e.update(cx, |e, cx| e.replace_text_in_range(None, "中", window, cx)));
                        op.push(ms(b.elapsed()));
                    }
                    _ => {
                        let s = if i % 2 == 0 { "字" } else { "a" };
                        let _ = cx.update(|window, cx| e.update(cx, |e, cx| e.replace_text_in_range(None, s, window, cx)));
                    }
                }
                if scenario != "ime" {
                    op.push(ms(a.elapsed()));
                }
                sleep(cx, 100).await;
            }
            p.report(&format!("{scenario} {doc}"), &op);
            let d = settle(p, cx, 1500., 15000.).await;
            p.report(&format!("  after-pause (catch-up+save) settled={}ms", d as i64), &[]);
        }
        "caret" => {
            open(p, root, lib, doc, cx).await;
            let Some(e) = editor(root, cx) else { return };
            let Some(at) = e.read_with(cx, |e, _| spot(e, 0.3, true)) else { return };
            let _ = cx.update(|window, cx| {
                window.focus(&e.read(cx).focus.clone(), cx);
                e.update(cx, |e, _| {
                    e.anchor = at;
                    e.head = at;
                    e.set_caret_for_driver();
                });
            });
            sleep(cx, 300).await;
            p.drain();
            p.slices.clear();
            let mut op = Vec::new();
            for _ in 0..60 {
                let a = Instant::now();
                let _ = cx.update(|window, cx| window.dispatch_action(Box::new(fumi_editor::editor::Down), cx));
                op.push(ms(a.elapsed()));
                sleep(cx, 80).await;
            }
            p.report(&format!("caret down {doc}"), &op);
        }
        "scroll" => {
            open(p, root, lib, doc, cx).await;
            let Some(e) = editor(root, cx) else { return };
            sleep(cx, 500).await;
            p.drain();
            p.slices.clear();
            let mut op = Vec::new();
            for _ in 0..500 {
                let a = Instant::now();
                let _ = cx.update(|_, cx| {
                    e.update(cx, |e, cx| {
                        let before = e.list.logical_scroll_top();
                        e.list.scroll_by(px(40.));
                        // At the end: back to the top, as the Swift run wraps.
                        if e.list.logical_scroll_top() == before {
                            e.list.scroll_to(ListOffset { item_ix: 0, offset_in_item: px(0.) });
                        }
                        cx.notify();
                    })
                });
                op.push(ms(a.elapsed()));
                sleep(cx, 8).await;
            }
            p.report(&format!("scroll {doc}"), &op);
        }
        _ => {}
    }
}
