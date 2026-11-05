//! A benchmark driver that sends real NSEvents through AppKit, so a keystroke
//! takes the same route as one from the keyboard: run loop, NSWindow, GPUI's
//! view, `interpretKeyEvents`, the `NSTextInputClient` methods, then our input
//! handler. Each step checks the document or scroll position really changed,
//! because a driver whose events never land would otherwise report great
//! numbers.

use crate::editor::{Editor, ItemKind};
use crate::markdown::Kind;
use crate::metrics;
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSString, NSUInteger};
use gpui::*;
use objc::{class, msg_send, sel, sel_impl};
use std::time::{Duration, Instant};

const NS_KEY_DOWN: u64 = 10;
const NS_KEY_UP: u64 = 11;
const FLAG_CMD: u64 = 1 << 20;
const FLAG_NUMPAD: u64 = 1 << 21;
const FLAG_FN: u64 = 1 << 23;

struct Key {
    chars: String,
    plain: String,
    code: u16,
    flags: u64,
}

fn char_key(c: char) -> Key {
    let code = match c {
        'a' => 0,
        'b' => 11,
        'c' => 8,
        'd' => 2,
        'e' => 14,
        'f' => 3,
        'z' => 6,
        's' => 1,
        _ => 0,
    };
    Key { chars: c.to_string(), plain: c.to_string(), code, flags: 0 }
}

fn fn_key(ch: u32, code: u16) -> Key {
    let s = char::from_u32(ch).unwrap().to_string();
    Key { chars: s.clone(), plain: s, code, flags: FLAG_NUMPAD | FLAG_FN }
}

fn k_enter() -> Key {
    Key { chars: "\r".into(), plain: "\r".into(), code: 36, flags: 0 }
}
fn k_backspace() -> Key {
    Key { chars: "\u{7f}".into(), plain: "\u{7f}".into(), code: 51, flags: 0 }
}
fn k_left() -> Key {
    fn_key(0xF702, 123)
}
fn k_right() -> Key {
    fn_key(0xF703, 124)
}
fn k_down() -> Key {
    fn_key(0xF701, 125)
}
fn k_up() -> Key {
    fn_key(0xF700, 126)
}
fn k_undo() -> Key {
    Key { chars: "z".into(), plain: "z".into(), code: 6, flags: FLAG_CMD }
}

fn ns(s: &str) -> id {
    unsafe { NSString::alloc(nil).init_str(s) }
}

fn ns_window() -> id {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let windows: id = msg_send![app, windows];
        let count: NSUInteger = msg_send![windows, count];
        for i in 0..count {
            let w: id = msg_send![windows, objectAtIndex: i];
            let visible: bool = msg_send![w, isVisible];
            if visible {
                return w;
            }
        }
        nil
    }
}

fn post(key: &Key) {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let window = ns_window();
        let number: isize = msg_send![window, windowNumber];
        for kind in [NS_KEY_DOWN, NS_KEY_UP] {
            let ev: id = msg_send![class!(NSEvent),
                keyEventWithType: kind
                location: NSPoint::new(10., 10.)
                modifierFlags: key.flags
                timestamp: 0f64
                windowNumber: number
                context: nil
                characters: ns(&key.chars)
                charactersIgnoringModifiers: ns(&key.plain)
                isARepeat: false
                keyCode: key.code];
            let _: () = msg_send![app, postEvent: ev atStart: false];
        }
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateScrollWheelEvent(
        source: *const std::ffi::c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32, ...
    ) -> *mut std::ffi::c_void;
    fn CGEventSetLocation(event: *mut std::ffi::c_void, p: NSPoint);
    fn CFRelease(cf: *const std::ffi::c_void);
}

/// One pixel-precise wheel notch, the kind a trackpad sends.
fn post_scroll(dy: i32) {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let window = ns_window();
        let frame: cocoa::foundation::NSRect = msg_send![window, frame];
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        let sf: cocoa::foundation::NSRect = msg_send![screen, frame];
        let at = NSPoint::new(
            frame.origin.x + frame.size.width / 2.,
            sf.size.height - frame.origin.y - frame.size.height / 2.,
        );
        let cg = CGEventCreateScrollWheelEvent(std::ptr::null(), 0, 1, dy);
        CGEventSetLocation(cg, at);
        let ev: id = msg_send![class!(NSEvent), eventWithCGEvent: cg];
        let _: () = msg_send![window, sendEvent: ev];
        let _ = app;
        CFRelease(cg);
    }
}

const NS_LEFT_DOWN: u64 = 1;
const NS_LEFT_UP: u64 = 2;

/// A click at (x, y) in window coordinates measured from the top left.
fn post_click(x: f64, y: f64) {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let window = ns_window();
        let number: isize = msg_send![window, windowNumber];
        let content: cocoa::foundation::NSRect = msg_send![window, contentLayoutRect];
        let at = NSPoint::new(x, content.size.height - y);
        for kind in [NS_LEFT_DOWN, NS_LEFT_UP] {
            let ev: id = msg_send![class!(NSEvent),
                mouseEventWithType: kind
                location: at
                modifierFlags: 0u64
                timestamp: 0f64
                windowNumber: number
                context: nil
                eventNumber: 0isize
                clickCount: 1isize
                pressure: 1f32];
            let _: () = msg_send![app, postEvent: ev atStart: false];
        }
    }
}

fn window_number() -> isize {
    unsafe { msg_send![ns_window(), windowNumber] }
}

fn shot(name: &str) {
    if let Ok(dir) = std::env::var("FUMI_SHOTS") {
        let path = format!("{dir}/{name}.png");
        let n = window_number().to_string();
        let ok = std::process::Command::new("/usr/sbin/screencapture")
            .args(["-x", "-o", "-l", &n, &path])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        println!("SHOT {path} ok={ok}");
    }
}

#[derive(Clone, Copy, Default)]
struct Sample {
    busy: u64,
    draw: u64,
    handler: u64,
    latency: u64,
}

fn ms(ns: u64) -> f64 {
    ns as f64 / 1e6
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() as f64 - 1.) * p).round() as usize]
}

fn report(name: &str, samples: &[Sample], failed: usize) {
    let col = |f: fn(&Sample) -> u64| samples.iter().map(|s| ms(f(s))).collect::<Vec<f64>>();
    let mut busy = col(|s| s.busy);
    let mut draw = col(|s| s.draw);
    let mut handler = col(|s| s.handler);
    let mut latency = col(|s| s.latency);
    println!(
        "RESULT {name}: n={} failed={failed} | main-thread busy p50={:.2} p95={:.2} max={:.2} | draw p50={:.2} p95={:.2} | edit-handler p50={:.3} | event-to-painted p50={:.2} p95={:.2} ms",
        samples.len(),
        pct(&mut busy, 0.5),
        pct(&mut busy, 0.95),
        pct(&mut busy, 1.0),
        pct(&mut draw, 0.5),
        pct(&mut draw, 0.95),
        pct(&mut handler, 0.5),
        pct(&mut latency, 0.5),
        pct(&mut latency, 0.95),
    );
}

async fn sleep(cx: &AsyncApp, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
}

/// Posts a key, waits for the frame it causes, and returns the timings. `check`
/// must confirm the key did what it should, or the sample counts as failed.
async fn step(
    cx: &mut AsyncApp,
    window: WindowHandle<Editor>,
    key: &Key,
    check: impl Fn(&Editor) -> bool,
) -> Option<Sample> {
    sleep(cx, 25).await;
    let f0 = metrics::frames();
    let t0 = metrics::now();
    let b0 = metrics::busy_ns();
    post(key);
    let deadline = Instant::now() + Duration::from_secs(3);
    while metrics::frames() == f0 {
        if Instant::now() > deadline {
            return None;
        }
        sleep(cx, 1).await;
    }
    // One more wake-up, so the slice that drew (and encoded for Metal) is over.
    sleep(cx, 2).await;
    let b1 = metrics::busy_ns();
    let s = Sample {
        busy: b1 - b0,
        draw: metrics::last_draw(),
        handler: metrics::last_handler(),
        latency: metrics::last_frame_end().saturating_sub(t0),
    };
    let ok = window.update(cx, |e, _, _| check(e)).unwrap_or(false);
    ok.then_some(s)
}

/// The middle-most line of the wanted kind that is long enough to type in.
fn find_line(e: &Editor, want: impl Fn(Kind) -> bool, min_len: usize) -> usize {
    let n = e.lines.len();
    let mid = n / 2;
    for off in 0..n {
        for l in [mid + off, mid.wrapping_sub(off)] {
            if l < n && want(e.lines[l].kind) && e.doc.line_text(l).len() >= min_len {
                return l;
            }
        }
    }
    panic!("no such line in this document");
}

fn place(window: WindowHandle<Editor>, cx: &mut AsyncApp, at: impl Fn(&Editor) -> usize) {
    window
        .update(cx, |e, _, cx| {
            let to = at(e);
            e.anchor = to;
            e.head = to;
            e.set_caret_for_driver();
            cx.notify();
        })
        .unwrap();
}

pub fn start(scenario: String, window: WindowHandle<Editor>, t0: Instant, read_ms: f64, cx: &mut App) {
    cx.spawn(async move |cx| {
        // First frame: what a user waits for from launch.
        // A frame only counts once it has laid out lines; GPUI's very first
        // draw can happen before the list knows its size.
        loop {
            let drawn = window.update(cx, |e, _, _| !e.painted.borrow().is_empty()).unwrap_or(false);
            if drawn {
                break;
            }
            sleep(cx, 1).await;
        }
        let first_paint = t0.elapsed().as_secs_f64() * 1000.;
        let (rope, scan) = metrics::load_times();
        let info = window
            .update(cx, |e, _, _| (e.doc.len(), e.lines.len(), e.items.len()))
            .unwrap();
        println!(
            "OPEN bytes={} lines={} items={} | launch-to-first-paint={:.1}ms read+init={:.1}ms rope={:.2}ms scan={:.2}ms first-draw={:.1}ms",
            info.0, info.1, info.2, first_paint, read_ms, ms(rope), ms(scan), ms(metrics::last_draw())
        );
        sleep(cx, 400).await;
        run(&scenario, window, cx).await;
        let (now, peak) = metrics::footprint_mb();
        println!("MEMORY footprint={now:.0}MB peak={peak:.0}MB");
        // Leaves the window up so a screenshot from outside can check the result.
        if let Some(secs) = std::env::var("FUMI_HOLD").ok().and_then(|s| s.parse::<u64>().ok()) {
            println!("HOLD {secs}s");
            sleep(cx, secs * 1000).await;
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

async fn run(scenario: &str, window: WindowHandle<Editor>, cx: &mut AsyncApp) {
    let before = window.update(cx, |e, _, _| e.doc.to_bytes()).unwrap();
    match scenario {
        "idle" => {
            let b0 = metrics::busy_ns();
            let f0 = metrics::frames();
            sleep(cx, 3000).await;
            println!(
                "RESULT idle: busy={:.2}ms over 3s, frames={}",
                ms(metrics::busy_ns() - b0),
                metrics::frames() - f0
            );
        }
        "goto" => {
            let line: usize = std::env::var("FUMI_LINE").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
            place(window, cx, move |e| e.doc.line_start(line));
            sleep(cx, 300).await;
        }
        "reopen" => {
            let mut samples = Vec::new();
            for _ in 0..7 {
                sleep(cx, 80).await;
                let bytes = before.clone();
                let f0 = metrics::frames();
                let b0 = metrics::busy_ns();
                let t0 = metrics::now();
                window
                    .update(cx, |e, _, cx| {
                        e.load_bytes(bytes);
                        cx.notify();
                    })
                    .unwrap();
                while metrics::frames() == f0 {
                    sleep(cx, 1).await;
                }
                sleep(cx, 2).await;
                samples.push(Sample {
                    busy: metrics::busy_ns() - b0,
                    draw: metrics::last_draw(),
                    handler: 0,
                    latency: metrics::last_frame_end() - t0,
                });
            }
            report("open-in-running-window", &samples, 0);
        }
        "type" | "table" | "enter" | "fast" => {
            let n = match scenario {
                "enter" => 40,
                "table" => 60,
                _ => 100,
            };
            let want_table = scenario == "table";
            let line = window
                .update(cx, |e, _, _| {
                    find_line(
                        e,
                        |k| if want_table { k == Kind::TableRow } else { k == Kind::Para },
                        40,
                    )
                })
                .unwrap();
            place(window, cx, |e| {
                let start = e.doc.line_start(line);
                e.doc.floor_boundary(start + e.doc.line_text(line).len() / 2)
            });
            sleep(cx, 300).await;
            if want_table {
                let open = window
                    .update(cx, |e, _, _| e.items[e.item_of(line)].kind == ItemKind::Line)
                    .unwrap();
                println!("CHECK table source is open at the caret: {open}");
            }
            shot(&format!("{scenario}-before"));
            let mut samples = Vec::new();
            let mut failed = 0;
            let len0 = window.update(cx, |e, _, _| e.doc.len()).unwrap();
            if scenario == "fast" {
                // Held-key speed: 30 keys a second, not waiting for frames.
                let t = Instant::now();
                let b0 = metrics::busy_ns();
                for i in 0..n {
                    post(&char_key(if i % 2 == 0 { 'a' } else { 'b' }));
                    sleep(cx, 33).await;
                }
                sleep(cx, 100).await;
                let got = window.update(cx, |e, _, _| e.doc.len()).unwrap() - len0;
                println!(
                    "RESULT fast: sent {n} keys at 30/s in {:.0}ms, {got} applied, busy {:.2}ms per key",
                    t.elapsed().as_secs_f64() * 1000.,
                    ms(metrics::busy_ns() - b0) / n as f64,
                );
            } else {
                for i in 0..n {
                    let expect = len0 + i + 1;
                    let key = if scenario == "enter" {
                        k_enter()
                    } else {
                        char_key(if i % 2 == 0 { 'a' } else { 'b' })
                    };
                    match step(cx, window, &key, move |e| e.doc.len() == expect).await {
                        Some(s) => samples.push(s),
                        None => failed += 1,
                    }
                }
                report(scenario, &samples, failed);
            }
            shot(&format!("{scenario}-after"));
            // Undo everything with real keys, then the bytes must match the file.
            let mut guard = 0;
            loop {
                let same = window.update(cx, |e, _, _| e.doc.len() == len0).unwrap();
                if same || guard > 200 {
                    break;
                }
                post(&k_undo());
                sleep(cx, 12).await;
                guard += 1;
            }
            sleep(cx, 100).await;
            let after = window.update(cx, |e, _, _| e.doc.to_bytes()).unwrap();
            println!("CHECK undo restored the original bytes: {} ({guard} undos)", after == before);
        }
        "caret" => {
            let line = window
                .update(cx, |e, _, _| find_line(e, |k| k == Kind::Para, 40))
                .unwrap();
            place(window, cx, move |e| e.doc.line_start(line));
            sleep(cx, 300).await;
            let moved = |key: Key, name: &'static str| (key, name);
            for (key, name) in [moved(k_down(), "caret-down"), moved(k_up(), "caret-up"), moved(k_right(), "caret-right"), moved(k_left(), "caret-left")] {
                let mut samples = Vec::new();
                let mut failed = 0;
                for _ in 0..60 {
                    let h0 = window.update(cx, |e, _, _| e.head).unwrap();
                    match step(cx, window, &key, move |e| e.head != h0).await {
                        Some(s) => samples.push(s),
                        None => failed += 1,
                    }
                }
                report(name, &samples, failed);
            }
            shot("caret-after");
        }
        "backspace" => {
            let line = window
                .update(cx, |e, _, _| find_line(e, |k| k == Kind::Para, 80))
                .unwrap();
            place(window, cx, move |e| e.doc.line_end(line));
            sleep(cx, 300).await;
            let mut samples = Vec::new();
            let mut failed = 0;
            for _ in 0..20 {
                let len = window.update(cx, |e, _, _| e.doc.len()).unwrap();
                match step(cx, window, &k_backspace(), move |e| e.doc.len() < len).await {
                    Some(s) => samples.push(s),
                    None => failed += 1,
                }
            }
            report("backspace", &samples, failed);
        }
        "scroll" => {
            sleep(cx, 300).await;
            shot("scroll-top");
            let top0 = window
                .update(cx, |e, _, _| e.list.logical_scroll_top().item_ix)
                .unwrap();
            let b0 = metrics::busy_ns();
            metrics::record_frames(true);
            let t = Instant::now();
            let mut sent = 0u32;
            // Six seconds of steady trackpad-speed scrolling, about 1800 pt/s.
            while t.elapsed() < Duration::from_secs(6) {
                post_scroll(-14);
                sent += 1;
                sleep(cx, 8).await;
            }
            sleep(cx, 100).await;
            metrics::record_frames(false);
            let frames = metrics::take_frames();
            let elapsed = t.elapsed().as_secs_f64();
            let top1 = window
                .update(cx, |e, _, _| e.list.logical_scroll_top().item_ix)
                .unwrap();
            let mut draws: Vec<f64> = frames.iter().map(|f| ms(f.1)).collect();
            let gaps: Vec<f64> = frames.windows(2).map(|w| ms(w[1].0 - w[0].0)).collect();
            let over = draws.iter().filter(|d| **d > 16.7).count();
            let gap_over = gaps.iter().filter(|g| **g > 16.7).count();
            let gap_max = gaps.iter().cloned().fold(0., f64::max);
            println!(
                "RESULT scroll: {} wheel events, item {} -> {}, {} frames in {:.1}s ({:.0} fps), draws over 16.7ms={}, frame gaps over 16.7ms={}, longest gap={:.1}ms, draw p50={:.2} p95={:.2} max={:.2}, main-thread busy {:.0}%",
                sent, top0, top1, frames.len(), elapsed, frames.len() as f64 / elapsed,
                over, gap_over, gap_max,
                pct(&mut draws.clone(), 0.5), pct(&mut draws.clone(), 0.95), pct(&mut draws, 1.0),
                ms(metrics::busy_ns() - b0) / (elapsed * 10.),
            );
            println!("CHECK scroll moved: {}", top1 > top0);
            shot("scroll-end");
        }
        "click" => {
            let line: usize = std::env::var("FUMI_LINE").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
            place(window, cx, move |e| e.doc.line_start(line));
            let log = |e: &Editor, tag: &str| {
                let p = e.painted.borrow();
                println!(
                    "CHECK {tag}: scroll top item={} head line={} painted lines {:?}..{:?}",
                    e.list.logical_scroll_top().item_ix,
                    e.doc.line_of(e.head),
                    p.first().map(|i| i.first_line),
                    p.last().map(|i| i.first_line),
                );
            };
            for i in 0..3 {
                sleep(cx, 400).await;
                window.update(cx, |e, _, _| log(e, &format!("settle {i}"))).unwrap();
            }
            post_click(500., 400.);
            sleep(cx, 300).await;
            window.update(cx, |e, _, _| log(e, "after click")).unwrap();
        }
        "ime" => ime(window, cx).await,
        "tour" => tour(window, cx).await,
        other => println!("unknown scenario {other}"),
    }
}

/// The NSTextInputClient calls an input method makes during composition, sent
/// to GPUI's real view. It proves the plumbing, not Pinyin itself.
async fn ime(window: WindowHandle<Editor>, cx: &mut AsyncApp) {
    let line = window
        .update(cx, |e, _, _| find_line(e, |k| k == Kind::Para, 20))
        .unwrap();
    place(window, cx, move |e| e.doc.line_start(line));
    sleep(cx, 200).await;
    let before = window.update(cx, |e, _, _| e.doc.to_bytes()).unwrap();
    let start = window.update(cx, |e, _, _| e.head).unwrap();
    const NOT_FOUND: u64 = i64::MAX as u64;
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct R {
        loc: u64,
        len: u64,
    }
    unsafe impl objc::Encode for R {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{_NSRange=QQ}") }
        }
    }
    let view = || -> id { unsafe { msg_send![ns_window(), contentView] } };
    for (text, sel) in [("n", 1u64), ("ni", 2), ("ni h", 4), ("ni hao", 6)] {
        unsafe {
            let _: () = msg_send![view(),
                setMarkedText: ns(text)
                selectedRange: R { loc: sel, len: 0 }
                replacementRange: R { loc: NOT_FOUND, len: 0 }];
        }
        sleep(cx, 40).await;
        let (marked, typed) = window
            .update(cx, |e, _, _| {
                (e.marked.clone(), e.doc.slice(start..start + text.len()))
            })
            .unwrap();
        println!("CHECK marked {text:?}: range={marked:?} text={typed:?}");
    }
    shot("ime-marked");
    let rect: cocoa::foundation::NSRect = unsafe {
        let mut actual = R { loc: 0, len: 0 };
        let utf16 = window.update(cx, |e, _, _| e.doc.utf16_of(start)).unwrap() as u64;
        msg_send![view(),
            firstRectForCharacterRange: R { loc: utf16, len: 2 }
            actualRange: &mut actual as *mut R]
    };
    println!(
        "CHECK candidate rect from firstRectForCharacterRange: x={:.0} y={:.0} w={:.0} h={:.0}",
        rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
    );
    unsafe {
        let _: () = msg_send![view(), insertText: ns("你好") replacementRange: R { loc: NOT_FOUND, len: 0 }];
    }
    sleep(cx, 60).await;
    let (committed, marked, head) = window
        .update(cx, |e, _, _| (e.doc.slice(start..start + 6), e.marked.clone(), e.head))
        .unwrap();
    println!("CHECK committed {committed:?}, marked={marked:?}, caret after it={}", head == start + 6);
    shot("ime-committed");
    post(&k_undo());
    sleep(cx, 100).await;
    let after = window.update(cx, |e, _, _| e.doc.to_bytes()).unwrap();
    println!("CHECK one undo removes the whole composition: {}", after == before);
}

/// Exercises each key on a known note and prints what it did.
async fn tour(window: WindowHandle<Editor>, cx: &mut AsyncApp) {
    sleep(cx, 600).await;
    shot("tour-start");
    let status = |e: &Editor| {
        let line = e.doc.line_of(e.head);
        format!("line {line} col {} sel {:?} kind {:?}", e.head - e.doc.line_start(line), e.selection(), e.lines[line].kind)
    };
    for (name, key) in [("down", k_down()), ("down", k_down()), ("down", k_down()), ("down", k_down()), ("down", k_down())] {
        let _ = step(cx, window, &key, |_| true).await;
        println!("KEY {name}: {}", window.update(cx, |e, _, _| status(e)).unwrap());
    }
    shot("tour-heading-caret");
    for _ in 0..3 {
        let _ = step(cx, window, &k_down(), |_| true).await;
    }
    println!("KEY body: {}", window.update(cx, |e, _, _| status(e)).unwrap());
    shot("tour-para-caret");
    for _ in 0..4 {
        let _ = step(cx, window, &k_down(), |_| true).await;
    }
    println!("KEY table: {}", window.update(cx, |e, _, _| status(e)).unwrap());
    sleep(cx, 200).await;
    shot("tour-table-open");
    for _ in 0..30 {
        let _ = step(cx, window, &k_down(), |_| true).await;
    }
    println!("KEY end: {}", window.update(cx, |e, _, _| status(e)).unwrap());
    sleep(cx, 300).await;
    shot("tour-end");
}
