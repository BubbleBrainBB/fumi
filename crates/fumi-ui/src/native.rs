//! Shared AppKit plumbing for the native-view spikes. Everything goes through
//! the Objective-C runtime by name, so a binary still runs on systems that lack
//! a class: `class_named` just returns `None` there.
#![allow(dead_code, deprecated, unexpected_cfgs)]

use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize, NSString, NSUInteger};
use gpui::*;
use objc::runtime::Class;
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::CString;

pub fn load_framework(name: &str) {
    let path = CString::new(format!("/System/Library/Frameworks/{name}.framework/{name}")).unwrap();
    unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_LAZY) };
}

pub fn class_named(name: &str) -> Option<&'static Class> {
    Class::get(name)
}

pub fn ns(s: &str) -> id {
    unsafe { NSString::alloc(nil).init_str(s) }
}

pub fn ns_window() -> id {
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

pub fn content_view() -> id {
    unsafe { msg_send![ns_window(), contentView] }
}

pub fn content_height() -> f64 {
    unsafe {
        let frame: NSRect = msg_send![content_view(), frame];
        frame.size.height
    }
}

/// GPUI bounds (top-left origin, window coordinates) to a frame in an unflipped
/// parent of the given height.
pub fn ns_frame(b: Bounds<Pixels>, parent_height: f64) -> NSRect {
    let (x, y) = (f32::from(b.origin.x) as f64, f32::from(b.origin.y) as f64);
    let (w, h) = (f32::from(b.size.width) as f64, f32::from(b.size.height) as f64);
    NSRect::new(NSPoint::new(x, parent_height - y - h), NSSize::new(w, h))
}

pub fn set_frame(view: id, frame: NSRect) {
    unsafe {
        let _: () = msg_send![view, setFrame: frame];
    }
}

pub fn set_hidden(view: id, hidden: bool) {
    unsafe {
        let _: () = msg_send![view, setHidden: hidden];
    }
}

pub fn frame_of(view: id) -> NSRect {
    unsafe { msg_send![view, frame] }
}

pub fn class_name(view: id) -> String {
    if view == nil {
        return "nil".into();
    }
    unsafe {
        let c: *const Class = msg_send![view, class];
        (*c).name().to_string()
    }
}

/// What AppKit would deliver a click or wheel event to at a GPUI window point.
pub fn view_at(x: f64, y: f64) -> id {
    unsafe {
        let cv = content_view();
        let p = NSPoint::new(x, content_height() - y);
        msg_send![cv, hitTest: p]
    }
}

pub fn first_responder() -> id {
    unsafe { msg_send![ns_window(), firstResponder] }
}

const NS_KEY_DOWN: u64 = 10;
const NS_KEY_UP: u64 = 11;
const NS_LEFT_DOWN: u64 = 1;
const NS_LEFT_UP: u64 = 2;

pub fn post_key(c: &str, code: u16) {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let number: isize = msg_send![ns_window(), windowNumber];
        for kind in [NS_KEY_DOWN, NS_KEY_UP] {
            let ev: id = msg_send![class!(NSEvent),
                keyEventWithType: kind location: NSPoint::new(10., 10.) modifierFlags: 0u64
                timestamp: 0f64 windowNumber: number context: nil characters: ns(c)
                charactersIgnoringModifiers: ns(c) isARepeat: false keyCode: code];
            let _: () = msg_send![app, postEvent: ev atStart: false];
        }
    }
}

pub fn post_click(x: f64, y: f64) {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let number: isize = msg_send![ns_window(), windowNumber];
        let at = NSPoint::new(x, content_height() - y);
        for kind in [NS_LEFT_DOWN, NS_LEFT_UP] {
            let ev: id = msg_send![class!(NSEvent),
                mouseEventWithType: kind location: at modifierFlags: 0u64 timestamp: 0f64
                windowNumber: number context: nil eventNumber: 0isize clickCount: 1isize
                pressure: 1f32];
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

/// A pixel-precise wheel event at a GPUI window point, sent the way AppKit
/// routes one: to whatever view is under that point.
pub fn scroll_at(x: f64, y: f64, dy: i32) {
    unsafe {
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        let sf: NSRect = msg_send![screen, frame];
        let frame: NSRect = msg_send![ns_window(), frame];
        // Screen point of the window point, in CG's top-left space.
        let cg_at = NSPoint::new(
            frame.origin.x + x,
            sf.size.height - (frame.origin.y + content_height() - y),
        );
        let cg = CGEventCreateScrollWheelEvent(std::ptr::null(), 0, 1, dy);
        CGEventSetLocation(cg, cg_at);
        let ev: id = msg_send![class!(NSEvent), eventWithCGEvent: cg];
        let target = view_at(x, y);
        let _: () = msg_send![target, scrollWheel: ev];
        CFRelease(cg);
    }
}

/// A press, a move and a release in window coordinates.
pub fn post_mouse_drag(x0: f64, y0: f64, x1: f64, y1: f64) {
    const NS_LEFT_DRAGGED: u64 = 6;
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let number: isize = msg_send![ns_window(), windowNumber];
        let h = content_height();
        for (kind, x, y) in [
            (NS_LEFT_DOWN, x0, y0),
            (NS_LEFT_DRAGGED, (x0 + x1) / 2., (y0 + y1) / 2.),
            (NS_LEFT_DRAGGED, x1, y1),
            (NS_LEFT_UP, x1, y1),
        ] {
            let ev: id = msg_send![class!(NSEvent),
                mouseEventWithType: kind location: NSPoint::new(x, h - y) modifierFlags: 0u64
                timestamp: 0f64 windowNumber: number context: nil eventNumber: 0isize
                clickCount: 1isize pressure: 1f32];
            let _: () = msg_send![app, postEvent: ev atStart: false];
        }
    }
}

// SF Symbols ------------------------------------------------------------------

/// A system symbol drawn in one color, at the screen's scale. GPUI can't draw
/// SF Symbols itself; this gives every screen the same glyphs AppKit would.
/// Returns the image and its size in points.
pub fn sf_symbol(
    name: &str,
    point_size: f64,
    weight: f64,
    rgba: (f64, f64, f64, f64),
    scale: f64,
) -> Option<(std::sync::Arc<RenderImage>, (f32, f32))> {
    unsafe {
        let image: id = msg_send![class!(NSImage), imageWithSystemSymbolName: ns(name) accessibilityDescription: nil];
        if image == nil {
            return None;
        }
        let color: id = msg_send![class!(NSColor), colorWithSRGBRed: rgba.0 green: rgba.1 blue: rgba.2 alpha: rgba.3];
        let colors: id = msg_send![class!(NSArray), arrayWithObject: color];
        let sized: id = msg_send![class!(NSImageSymbolConfiguration), configurationWithPointSize: point_size weight: weight];
        let tinted: id = msg_send![class!(NSImageSymbolConfiguration), configurationWithPaletteColors: colors];
        let config: id = msg_send![sized, configurationByApplyingConfiguration: tinted];
        let image: id = msg_send![image, imageWithSymbolConfiguration: config];
        if image == nil {
            return None;
        }
        let size: NSSize = msg_send![image, size];
        draw_to_render_image(image, size, scale).map(|i| (i, (size.width as f32, size.height as f32)))
    }
}

/// Rasterizes an `NSImage` at `scale`, as BGRA, the order GPUI wants.
pub fn draw_to_render_image(image: id, size: NSSize, scale: f64) -> Option<std::sync::Arc<RenderImage>> {
    unsafe {
        let (w, h) = ((size.width * scale).ceil() as i64, (size.height * scale).ceil() as i64);
        if w <= 0 || h <= 0 {
            return None;
        }
        let rep: id = msg_send![class!(NSBitmapImageRep), alloc];
        let null: *mut *mut u8 = std::ptr::null_mut();
        let rep: id = msg_send![rep,
            initWithBitmapDataPlanes: null
            pixelsWide: w
            pixelsHigh: h
            bitsPerSample: 8i64
            samplesPerPixel: 4i64
            hasAlpha: true
            isPlanar: false
            colorSpaceName: ns("NSDeviceRGBColorSpace")
            bytesPerRow: 0i64
            bitsPerPixel: 32i64];
        if rep == nil {
            return None;
        }
        let _: () = msg_send![rep, setSize: size];
        let context: id = msg_send![class!(NSGraphicsContext), graphicsContextWithBitmapImageRep: rep];
        let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
        let _: () = msg_send![class!(NSGraphicsContext), setCurrentContext: context];
        let rect = NSRect::new(NSPoint::new(0., 0.), size);
        let zero = NSRect::new(NSPoint::new(0., 0.), NSSize::new(0., 0.));
        let _: () = msg_send![image, drawInRect: rect fromRect: zero operation: 2u64 fraction: 1f64];
        let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];
        let data: *const u8 = msg_send![rep, bitmapData];
        let row: i64 = msg_send![rep, bytesPerRow];
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let line = std::slice::from_raw_parts(data.add((y * row) as usize), (w * 4) as usize);
            for px in line.chunks_exact(4) {
                bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        }
        let _: () = msg_send![rep, release];
        let buffer = image::RgbaImage::from_raw(w as u32, h as u32, bgra)?;
        let frame = image::Frame::new(buffer);
        Some(std::sync::Arc::new(RenderImage::new(smallvec::smallvec![frame])))
    }
}

/// Weights as `NSFont.Weight` has them.
pub mod weight {
    pub const REGULAR: f64 = 0.0;
    pub const MEDIUM: f64 = 0.23;
    pub const SEMIBOLD: f64 = 0.3;
    pub const BOLD: f64 = 0.4;
}

/// Cached symbols by name, size, weight, color and scale.
#[derive(Default)]
pub struct SymbolCache {
    map: std::collections::HashMap<String, Option<(std::sync::Arc<RenderImage>, (f32, f32))>>,
}

impl SymbolCache {
    pub fn get(
        &mut self,
        name: &str,
        point_size: f32,
        weight: f64,
        color: Hsla,
        scale: f32,
    ) -> Option<(std::sync::Arc<RenderImage>, (f32, f32))> {
        let c = Rgba::from(color);
        let key = format!("{name}|{point_size}|{weight}|{:?}|{scale}", (c.r, c.g, c.b, c.a));
        self.map
            .entry(key)
            .or_insert_with(|| {
                sf_symbol(name, point_size as f64, weight, (c.r as f64, c.g as f64, c.b as f64, c.a as f64), scale as f64)
            })
            .clone()
    }
}
