//! Side measurement: the same typing test on `zorite-editor`, an MIT GPUI
//! live-preview editor, to see what a ready-made one costs at note sizes.
#![allow(deprecated, unexpected_cfgs)]
#[path = "../src/metrics.rs"]
#[allow(dead_code)]
mod metrics;

use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSString, NSUInteger};
use gpui::*;
use objc::{class, msg_send, sel, sel_impl};
use std::time::{Duration, Instant};
use zorite_editor::{EditorState, SyntaxStyle};

struct Root {
    editor: Entity<EditorState>,
}
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        metrics::frame_begin();
        div()
            .id("scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(0xFFFFFF))
            .text_size(px(16.))
            .text_color(rgb(0x37352F))
            .child(self.editor.clone())
            .child(canvas(|_, _, _| {}, |_, _, _, _| metrics::frame_end()).absolute().size_full())
    }
}

fn style() -> SyntaxStyle {
    SyntaxStyle {
        block_label: None,
        block_label_gen: 0,
        block_ref_count: None,
        marker: hsla(0., 0., 0.5, 0.55),
        code: hsla(0.05, 0.6, 0.45, 1.),
        code_bg: hsla(0., 0., 0., 0.05),
        link: hsla(0.45, 0.4, 0.3, 1.),
        tag: hsla(0.33, 0.45, 0.4, 1.),
        quote: hsla(0., 0., 0.4, 1.),
        alert_note: hsla(0.58, 0.9, 0.5, 1.),
        alert_tip: hsla(0.36, 0.5, 0.48, 1.),
        alert_important: hsla(0.74, 0.85, 0.6, 1.),
        alert_warning: hsla(0.12, 0.7, 0.48, 1.),
        alert_caution: hsla(0.01, 0.9, 0.5, 1.),
        alert_icons: None,
        rule: hsla(0., 0., 0., 0.18),
        mark_bg: hsla(0.13, 1., 0.5, 0.4),
        popover_bg: hsla(0., 0., 1., 1.),
        popover_border: hsla(0., 0., 0.8, 1.),
        popover_fg: hsla(0., 0., 0.2, 1.),
        popover_hover: hsla(0.58, 0.75, 0.66, 0.16),
        popover_divider: hsla(0., 0., 0., 0.18),
        popover_danger: rgb(0xE5484D).into(),
        mono: font("Menlo"),
        property_icon: None,
    }
}

fn post(c: &str, code: u16) {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let windows: id = msg_send![app, windows];
        let count: NSUInteger = msg_send![windows, count];
        let mut w = nil;
        for i in 0..count {
            let x: id = msg_send![windows, objectAtIndex: i];
            let v: bool = msg_send![x, isVisible];
            if v {
                w = x;
                break;
            }
        }
        let number: isize = msg_send![w, windowNumber];
        for kind in [10u64, 11] {
            let s = NSString::alloc(nil).init_str(c);
            let ev: id = msg_send![class!(NSEvent),
                keyEventWithType: kind location: NSPoint::new(10., 10.) modifierFlags: 0u64
                timestamp: 0f64 windowNumber: number context: nil characters: s
                charactersIgnoringModifiers: s isARepeat: false keyCode: code];
            let _: () = msg_send![app, postEvent: ev atStart: false];
        }
    }
}

fn main() {
    let t0 = Instant::now();
    metrics::now();
    let path = std::env::args().nth(1).unwrap();
    gpui_platform::application().run(move |cx: &mut App| {
        metrics::install_runloop_observer();
        zorite_editor::bind_keys(cx);
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        let bounds = Bounds::centered(None, size(px(1100.), px(800.)), cx);
        let window = cx
            .open_window(
                WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
                move |window, cx| {
                    let editor = cx.new(|cx| {
                        let mut e = EditorState::new(window, cx).with_text(text);
                        e.set_markdown_style(style(), cx);
                        e
                    });
                    editor.update(cx, |e, cx| e.focus(window, cx));
                    cx.new(|_| Root { editor })
                },
            )
            .unwrap();
        cx.activate(true);
        cx.spawn(async move |cx| {
            while metrics::frames() < 2 {
                cx.background_executor().timer(Duration::from_millis(5)).await;
            }
            println!("OPEN launch-to-paint {:.0}ms", t0.elapsed().as_secs_f64() * 1000.);
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let loaded = window
                .update(cx, |r, _, cx| r.editor.read(cx).text().as_bytes().to_vec())
                .unwrap();
            println!("CHECK text() equals the file after load: {}  ({} -> {} bytes)", loaded == bytes, bytes.len(), loaded.len());
            let len0 = loaded.len();
            let mut busy = Vec::new();
            let mut lost = 0;
            for i in 0..40usize {
                cx.background_executor().timer(Duration::from_millis(60)).await;
                let f0 = metrics::frames();
                let b0 = metrics::busy_ns();
                post(if i % 2 == 0 { "a" } else { "b" }, if i % 2 == 0 { 0 } else { 11 });
                let t = Instant::now();
                while metrics::frames() == f0 && t.elapsed() < Duration::from_secs(20) {
                    cx.background_executor().timer(Duration::from_millis(1)).await;
                }
                cx.background_executor().timer(Duration::from_millis(2)).await;
                busy.push((metrics::busy_ns() - b0) as f64 / 1e6);
                let n = window.update(cx, |r, _, cx| r.editor.read(cx).text().len()).unwrap();
                if n != len0 + i + 1 {
                    lost += 1;
                }
            }
            busy.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!(
                "RESULT type: n={} lost={lost} main-thread busy p50={:.1} p95={:.1} max={:.1} ms draw last={:.1}ms",
                busy.len(), busy[busy.len() / 2], busy[busy.len() * 95 / 100], busy[busy.len() - 1],
                metrics::last_draw() as f64 / 1e6
            );
            let (now, peak) = metrics::footprint_mb();
            println!("MEMORY footprint={now:.0}MB peak={peak:.0}MB");
            cx.update(|cx| cx.quit());
        })
        .detach();
    });
}
