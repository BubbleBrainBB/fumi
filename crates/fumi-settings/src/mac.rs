//! The bits of AppKit this crate's screens read from.
#![allow(deprecated)]

use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use fumi_ui::native::{self, class_named, load_framework, ns};
use gpui::{RenderImage, Window};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn string_of(s: id) -> Option<String> {
    if s == nil {
        return None;
    }
    unsafe {
        let utf8: *const std::os::raw::c_char = msg_send![s, UTF8String];
        (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }
}

/// Everything autoreleased inside `f` goes when it returns. Background threads
/// have no pool of their own.
pub fn with_pool<R>(f: impl FnOnce() -> R) -> R {
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let out = f();
        let _: () = msg_send![pool, drain];
        out
    }
}

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    fn NSFullUserName() -> id;
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    fn NSBeep();
}

pub fn full_user_name() -> String {
    string_of(unsafe { NSFullUserName() }).unwrap_or_default()
}

pub fn beep() {
    unsafe { NSBeep() }
}

/// `CFBundleShortVersionString`, or "1.0" outside a bundle, as in Swift.
pub fn bundle_short_version() -> String {
    unsafe {
        let bundle: id = msg_send![class!(NSBundle), mainBundle];
        let info: id = msg_send![bundle, infoDictionary];
        if info == nil {
            return "1.0".into();
        }
        let value: id = msg_send![info, objectForKey: ns("CFBundleShortVersionString")];
        string_of(value).unwrap_or_else(|| "1.0".into())
    }
}

/// `ByteCountFormatter.string(fromByteCount:countStyle: .file)`.
pub fn byte_count_string(bytes: i64) -> String {
    with_pool(|| unsafe {
        let s: id = msg_send![class!(NSByteCountFormatter), stringFromByteCount: bytes countStyle: 0i64];
        string_of(s).unwrap_or_default()
    })
}

/// Where a folder lives, as Finder names it.
pub fn display_name_at_path(path: &Path) -> String {
    unsafe {
        let manager: id = msg_send![class!(NSFileManager), defaultManager];
        let name: id = msg_send![manager, displayNameAtPath: ns(&path.to_string_lossy())];
        string_of(name).unwrap_or_default()
    }
}

/// Every installed family, as `NSFontManager` lists them.
pub fn font_families() -> Vec<String> {
    unsafe {
        let manager: id = msg_send![class!(NSFontManager), sharedFontManager];
        let list: id = msg_send![manager, availableFontFamilies];
        let n: u64 = msg_send![list, count];
        (0..n).filter_map(|i| string_of(msg_send![list, objectAtIndex: i])).collect()
    }
}

/// The small system switch's size on this system. Falls back to the size it has
/// had since macOS 11.
pub fn small_switch_size() -> (f32, f32) {
    unsafe {
        let Some(class) = class_named("NSSwitch") else { return (32., 18.) };
        let view: id = msg_send![class, new];
        if view == nil {
            return (32., 18.);
        }
        let _: () = msg_send![view, setControlSize: 1u64];
        let size: NSSize = msg_send![view, intrinsicContentSize];
        let _: () = msg_send![view, release];
        if size.width > 0. && size.height > 0. { (size.width as f32, size.height as f32) } else { (32., 18.) }
    }
}

/// The app's icon, drawn at `points` for a 2x screen.
pub fn app_icon(points: f64) -> Option<Arc<RenderImage>> {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let icon: id = msg_send![app, applicationIconImage];
        if icon == nil {
            return None;
        }
        native::draw_to_render_image(icon, NSSize::new(points, points), 2.)
    }
}

/// A picture file's size in points, as an image view would show it, and its pixels
/// at up to 2x with the long side capped at `max_side`. Safe off the main thread.
pub fn load_image_file(path: &Path, max_side: f64) -> Option<(Arc<RenderImage>, (f32, f32))> {
    with_pool(|| unsafe {
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithContentsOfFile: ns(&path.to_string_lossy())];
        if image == nil {
            return None;
        }
        let size: NSSize = msg_send![image, size];
        let out = if size.width > 0. && size.height > 0. {
            let scale = (max_side / size.width.max(size.height)).min(2.);
            native::draw_to_render_image(image, size, scale).map(|i| (i, (size.width as f32, size.height as f32)))
        } else {
            None
        };
        let _: () = msg_send![image, release];
        out
    })
}

/// A picture file's size in points, without its pixels.
pub fn image_file_size(path: &Path) -> Option<(f32, f32)> {
    with_pool(|| unsafe {
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithContentsOfFile: ns(&path.to_string_lossy())];
        if image == nil {
            return None;
        }
        let size: NSSize = msg_send![image, size];
        let _: () = msg_send![image, release];
        (size.width > 0. && size.height > 0.).then_some((size.width as f32, size.height as f32))
    })
}

// Login item ------------------------------------------------------------------

fn main_app_service() -> Option<id> {
    load_framework("ServiceManagement");
    let class = class_named("SMAppService")?;
    let service: id = unsafe { msg_send![class, mainAppService] };
    (service != nil).then_some(service)
}

/// `SMAppService.mainApp.status == .enabled`.
pub fn login_item_enabled() -> bool {
    let Some(service) = main_app_service() else { return false };
    let status: i64 = unsafe { msg_send![service, status] };
    status == 1
}

/// Registers or unregisters the app; an error is the system's own words.
pub fn set_login_item(enabled: bool) -> Result<(), String> {
    let Some(service) = main_app_service() else { return Err("Login items aren't available.".into()) };
    unsafe {
        let mut error: id = nil;
        let done: objc::runtime::BOOL = if enabled {
            msg_send![service, registerAndReturnError: &mut error as *mut id]
        } else {
            msg_send![service, unregisterAndReturnError: &mut error as *mut id]
        };
        if done != objc::runtime::NO {
            return Ok(());
        }
        let text: id = if error != nil { msg_send![error, localizedDescription] } else { nil };
        Err(string_of(text).unwrap_or_default())
    }
}

// Panels ----------------------------------------------------------------------

/// The avatar picture panel. Blocks in a modal loop: call from a spawned task.
pub fn choose_avatar_file() -> Option<PathBuf> {
    load_framework("UniformTypeIdentifiers");
    unsafe {
        let panel: id = msg_send![class!(NSOpenPanel), openPanel];
        if panel == nil {
            return None;
        }
        if let Some(ut) = class_named("UTType") {
            let types: id = msg_send![class!(NSMutableArray), array];
            for name in ["public.png", "public.jpeg", "public.heic", "org.webmproject.webp", "public.tiff"] {
                let t: id = msg_send![ut, typeWithIdentifier: ns(name)];
                if t != nil {
                    let _: () = msg_send![types, addObject: t];
                }
            }
            let _: () = msg_send![panel, setAllowedContentTypes: types];
        }
        let _: () = msg_send![panel, setAllowsMultipleSelection: false];
        let _: () = msg_send![panel, setMessage: ns("Choose a picture for your avatar on Post cards.")];
        let answer: i64 = msg_send![panel, runModal];
        if answer != 1 {
            return None;
        }
        let url: id = msg_send![panel, URL];
        native::path_of_url(url)
    }
}

/// Fumi's own folder under Application Support.
pub fn support_folder() -> Option<PathBuf> {
    unsafe {
        let manager: id = msg_send![class!(NSFileManager), defaultManager];
        // NSApplicationSupportDirectory, NSUserDomainMask.
        let urls: id = msg_send![manager, URLsForDirectory: 14u64 inDomains: 1u64];
        let first: id = msg_send![urls, firstObject];
        native::path_of_url(first).map(|p| p.join("Fumi"))
    }
}

// Native views over GPUI ------------------------------------------------------

/// GPUI's own view in a window.
pub fn gpui_view_of(window: &Window) -> id {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else { return nil };
    match handle.as_raw() {
        RawWindowHandle::AppKit(h) => h.ns_view.as_ptr() as id,
        _ => nil,
    }
}

pub fn make_first_responder(ns_window: id, view: id) {
    unsafe {
        if ns_window != nil && view != nil {
            let _: bool = msg_send![ns_window, makeFirstResponder: view];
        }
    }
}

pub fn zero_rect() -> NSRect {
    NSRect::new(NSPoint::new(0., 0.), NSSize::new(10., 10.))
}
