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

// Windows ---------------------------------------------------------------------

/// The `NSWindow` behind a GPUI window, by its raw handle.
pub fn ns_window_of(window: &Window) -> id {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else { return nil };
    match handle.as_raw() {
        RawWindowHandle::AppKit(h) => unsafe {
            let view = h.ns_view.as_ptr() as id;
            msg_send![view, window]
        },
        _ => nil,
    }
}

const NS_WINDOW_BELOW: i64 = -1;
const AUTORESIZE_FILL: u64 = 2 | 16;

/// Puts real Liquid Glass under GPUI's view (macOS 26), or the sidebar's
/// behind-window blur kept active on older systems. Returns the view, to take
/// away with `remove_backdrop` when the theme changes.
pub fn install_glass_backdrop(window: id) -> id {
    unsafe {
        let container: id = msg_send![window, contentView];
        let bounds: NSRect = msg_send![container, bounds];
        let backdrop: id = if let Some(class) = class_named("NSGlassEffectView") {
            let view: id = msg_send![class, alloc];
            let view: id = msg_send![view, initWithFrame: bounds];
            let _: () = msg_send![view, setStyle: 0i64];
            view
        } else {
            let view: id = msg_send![class!(NSVisualEffectView), alloc];
            let view: id = msg_send![view, initWithFrame: bounds];
            let _: () = msg_send![view, setMaterial: 7i64];
            let _: () = msg_send![view, setBlendingMode: 0i64];
            let _: () = msg_send![view, setState: 1i64];
            view
        };
        let _: () = msg_send![backdrop, setAutoresizingMask: AUTORESIZE_FILL];
        let _: () = msg_send![container, addSubview: backdrop positioned: NS_WINDOW_BELOW relativeTo: nil];
        let _: () = msg_send![window, setOpaque: false];
        let clear: id = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![window, setBackgroundColor: clear];
        backdrop
    }
}

pub fn remove_backdrop(window: id, backdrop: id) {
    unsafe {
        let _: () = msg_send![backdrop, removeFromSuperview];
        let _: () = msg_send![window, setOpaque: true];
        let color: id = msg_send![class!(NSColor), windowBackgroundColor];
        let _: () = msg_send![window, setBackgroundColor: color];
    }
}

/// Sets the app's appearance: `Some(dark)` forces it, `None` follows the system.
pub fn set_app_appearance(dark: Option<bool>) {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let appearance: id = match dark {
            Some(d) => {
                let name = ns(if d { "NSAppearanceNameDarkAqua" } else { "NSAppearanceNameAqua" });
                msg_send![class!(NSAppearance), appearanceNamed: name]
            }
            None => nil,
        };
        let _: () = msg_send![app, setAppearance: appearance];
    }
}

/// Reveals `path` in the Finder.
pub fn reveal_in_finder(path: &std::path::Path) {
    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let url: id = msg_send![class!(NSURL), fileURLWithPath: ns(&path.to_string_lossy())];
        let urls: id = msg_send![class!(NSArray), arrayWithObject: url];
        let _: () = msg_send![workspace, activateFileViewerSelectingURLs: urls];
    }
}

/// Moves `path` to the Trash. The Swift app deletes the same way.
pub fn trash(path: &std::path::Path) -> bool {
    unsafe {
        let manager: id = msg_send![class!(NSFileManager), defaultManager];
        let url: id = msg_send![class!(NSURL), fileURLWithPath: ns(&path.to_string_lossy())];
        let out: *mut id = std::ptr::null_mut();
        let err: *mut id = std::ptr::null_mut();
        msg_send![manager, trashItemAtURL: url resultingItemURL: out error: err]
    }
}

// Context menus ---------------------------------------------------------------

/// One row of a native menu.
#[derive(Clone, Debug)]
pub enum MenuRow {
    Item { title: String, enabled: bool, destructive: bool, key: Option<String> },
    Separator,
    Submenu { title: String, rows: Vec<MenuRow> },
    /// A row that can't be clicked, as a section label.
    Label(String),
}

impl MenuRow {
    pub fn item(title: impl Into<String>) -> Self {
        MenuRow::Item { title: title.into(), enabled: true, destructive: false, key: None }
    }

    pub fn disabled(title: impl Into<String>) -> Self {
        MenuRow::Item { title: title.into(), enabled: false, destructive: false, key: None }
    }

    pub fn destructive(title: impl Into<String>) -> Self {
        MenuRow::Item { title: title.into(), enabled: true, destructive: true, key: None }
    }
}

use std::sync::atomic::{AtomicI64, Ordering as AtomicOrdering};
static CHOSEN: AtomicI64 = AtomicI64::new(-1);

extern "C" fn menu_action(_: &objc::runtime::Object, _: objc::runtime::Sel, sender: id) {
    let tag: i64 = unsafe { msg_send![sender, tag] };
    CHOSEN.store(tag, AtomicOrdering::SeqCst);
}

fn menu_target() -> id {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let superclass = class!(NSObject);
        let mut decl = objc::declare::ClassDecl::new("FumiMenuTarget", superclass).expect("class is new");
        unsafe {
            decl.add_method(
                sel!(fumiMenuAction:),
                menu_action as extern "C" fn(&objc::runtime::Object, objc::runtime::Sel, id),
            );
        }
        decl.register();
    });
    unsafe {
        let class = Class::get("FumiMenuTarget").expect("registered");
        let t: id = msg_send![class, new];
        t
    }
}

fn build_menu(rows: &[MenuRow], target: id, next_tag: &mut i64) -> id {
    unsafe {
        let menu: id = msg_send![class!(NSMenu), new];
        let _: () = msg_send![menu, setAutoenablesItems: false];
        for row in rows {
            match row {
                MenuRow::Separator => {
                    let sep: id = msg_send![class!(NSMenuItem), separatorItem];
                    let _: () = msg_send![menu, addItem: sep];
                }
                MenuRow::Label(title) => {
                    let item: id = msg_send![class!(NSMenuItem), alloc];
                    let item: id = msg_send![item, initWithTitle: ns(title) action: sel!(fumiNoop:) keyEquivalent: ns("")];
                    let _: () = msg_send![item, setEnabled: false];
                    let _: () = msg_send![menu, addItem: item];
                }
                MenuRow::Item { title, enabled, destructive, key } => {
                    let item: id = msg_send![class!(NSMenuItem), alloc];
                    let item: id = msg_send![item,
                        initWithTitle: ns(title)
                        action: sel!(fumiMenuAction:)
                        keyEquivalent: ns(key.as_deref().unwrap_or(""))];
                    let _: () = msg_send![item, setTarget: target];
                    let _: () = msg_send![item, setTag: *next_tag];
                    *next_tag += 1;
                    let _: () = msg_send![item, setEnabled: *enabled];
                    if *destructive {
                        // Red text, as SwiftUI's destructive role draws it.
                        let red: id = msg_send![class!(NSColor), systemRedColor];
                        let attrs: id = msg_send![class!(NSDictionary), dictionaryWithObject: red forKey: ns("NSColor")];
                        let string: id = msg_send![class!(NSAttributedString), alloc];
                        let string: id = msg_send![string, initWithString: ns(title) attributes: attrs];
                        let _: () = msg_send![item, setAttributedTitle: string];
                    }
                    let _: () = msg_send![menu, addItem: item];
                }
                MenuRow::Submenu { title, rows } => {
                    let item: id = msg_send![class!(NSMenuItem), alloc];
                    let item: id = msg_send![item, initWithTitle: ns(title) action: nil keyEquivalent: ns("")];
                    let sub = build_menu(rows, target, next_tag);
                    let _: () = msg_send![item, setSubmenu: sub];
                    let _: () = msg_send![menu, addItem: item];
                }
            }
        }
        menu
    }
}

/// Shows a native menu at `position` (window coordinates, top-left origin) and
/// waits for it to close. Returns the index of the chosen item, counting only
/// `Item` rows in reading order, submenus included.
pub fn popup_menu(window: &Window, rows: &[MenuRow], position: Point<Pixels>) -> Option<usize> {
    unsafe {
        let ns_win = ns_window_of(window);
        if ns_win == nil {
            return None;
        }
        let view: id = msg_send![ns_win, contentView];
        let frame: NSRect = msg_send![view, frame];
        let x = f32::from(position.x) as f64;
        let y = frame.size.height - f32::from(position.y) as f64;
        let target = menu_target();
        let mut next = 0;
        let menu = build_menu(rows, target, &mut next);
        CHOSEN.store(-1, AtomicOrdering::SeqCst);
        let flipped: bool = msg_send![view, isFlipped];
        let at = NSPoint::new(x, if flipped { f32::from(position.y) as f64 } else { y });
        let _: bool = msg_send![menu, popUpMenuPositioningItem: nil atLocation: at inView: view];
        let _: () = msg_send![menu, release];
        let _: () = msg_send![target, release];
        let chosen = CHOSEN.load(AtomicOrdering::SeqCst);
        (chosen >= 0).then_some(chosen as usize)
    }
}
