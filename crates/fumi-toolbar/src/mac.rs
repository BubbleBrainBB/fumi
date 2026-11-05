//! The bits of AppKit the toolbar and the menu bar need that GPUI doesn't offer.
#![allow(deprecated, unexpected_cfgs)]

use cocoa::base::{id, nil};
use cocoa::foundation::NSSize;
use fumi_ui::native::ns;
use gpui::RenderImage;
use objc::{class, msg_send, sel, sel_impl};
use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::sync::Arc;

fn app() -> id {
    unsafe { msg_send![class!(NSApplication), sharedApplication] }
}

fn string_of(s: id) -> String {
    if s == nil {
        return String::new();
    }
    unsafe {
        let utf8: *const std::os::raw::c_char = msg_send![s, UTF8String];
        if utf8.is_null() { String::new() } else { CStr::from_ptr(utf8).to_string_lossy().into_owned() }
    }
}

pub fn about_panel() {
    unsafe {
        let _: () = msg_send![app(), orderFrontStandardAboutPanel: nil];
    }
}

pub fn show_help() {
    unsafe {
        let _: () = msg_send![app(), showHelp: nil];
    }
}

pub fn bring_all_to_front() {
    unsafe {
        let _: () = msg_send![app(), arrangeInFront: nil];
    }
}

/// Takes the window off screen without closing it.
pub fn order_out(window: id) {
    if window != nil {
        unsafe {
            let _: () = msg_send![window, orderOut: nil];
        }
    }
}

/// Opts the window out of cross-launch restoration.
pub fn set_not_restorable(window: id) {
    if window != nil {
        unsafe {
            let _: () = msg_send![window, setRestorable: false];
        }
    }
}

/// What the menu bar can only be told in AppKit: the ⇥ of ⌃⇥ and ⌃⇧⇥, which GPUI
/// doesn't turn into a key equivalent, and which menu is Help.
pub fn polish_main_menu() {
    const CONTROL: u64 = 1 << 18;
    const SHIFT: u64 = 1 << 17;
    unsafe {
        let main: id = msg_send![app(), mainMenu];
        if main == nil {
            return;
        }
        let count: i64 = msg_send![main, numberOfItems];
        for i in 0..count {
            let top: id = msg_send![main, itemAtIndex: i];
            let menu: id = msg_send![top, submenu];
            if menu == nil {
                continue;
            }
            match string_of(msg_send![menu, title]).as_str() {
                "Help" => {
                    let _: () = msg_send![app(), setHelpMenu: menu];
                }
                "Window" => {
                    for (title, mask) in [("Show Next Tab", CONTROL), ("Show Previous Tab", CONTROL | SHIFT)] {
                        let item: id = msg_send![menu, itemWithTitle: ns(title)];
                        if item != nil {
                            let _: () = msg_send![item, setKeyEquivalent: ns("\t")];
                            let _: () = msg_send![item, setKeyEquivalentModifierMask: mask];
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// A folder's name as Finder shows it: "Documents", "iCloud Drive"…
pub fn display_name(path: &str) -> String {
    unsafe {
        let manager: id = msg_send![class!(NSFileManager), defaultManager];
        string_of(msg_send![manager, displayNameAtPath: ns(path)])
    }
}

/// "1.2 MB", as `ByteCountFormatter` writes a file's size.
pub fn file_size_label(bytes: u64) -> String {
    unsafe {
        // NSByteCountFormatterCountStyleFile.
        string_of(msg_send![class!(NSByteCountFormatter), stringFromByteCount: bytes as i64 countStyle: 0i64])
    }
}

thread_local! {
    static TIME_FORMATTER: Cell<id> = const { Cell::new(nil) };
    static APP_ICON: RefCell<Option<Option<Arc<RenderImage>>>> = const { RefCell::new(None) };
}

/// A moment as the user's locale writes a short time: "14:20" or "2:20 PM".
pub fn short_time(secs: f64) -> String {
    let formatter = TIME_FORMATTER.with(|cell| {
        if cell.get() == nil {
            unsafe {
                let formatter: id = msg_send![class!(NSDateFormatter), new];
                // NSDateFormatterNoStyle, NSDateFormatterShortStyle.
                let _: () = msg_send![formatter, setDateStyle: 0u64];
                let _: () = msg_send![formatter, setTimeStyle: 1u64];
                cell.set(formatter);
            }
        }
        cell.get()
    });
    unsafe {
        let date: id = msg_send![class!(NSDate), dateWithTimeIntervalSince1970: secs];
        string_of(msg_send![formatter, stringFromDate: date])
    }
}

/// The app's icon, at 64 points.
pub fn app_icon() -> Option<Arc<RenderImage>> {
    APP_ICON.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(|| unsafe {
                let image: id = msg_send![app(), applicationIconImage];
                if image == nil {
                    None
                } else {
                    let size = NSSize::new(64., 64.);
                    let _: () = msg_send![image, setSize: size];
                    fumi_ui::native::draw_to_render_image(image, size, 2.)
                }
            })
            .clone()
    })
}
