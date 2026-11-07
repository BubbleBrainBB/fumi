//! The AppKit this crate calls: strings, the pasteboard, panels, the alert, small
//! images. Everything an unrecognized selector could abort is listed in
//! `tests::every_selector_exists`, so a wrong name fails `cargo test`, not the app.

use crate::wechat::Theme;
use block::ConcreteBlock;
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize, NSString};
use fumi_ui::native::{class_named, load_framework, path_of_url};
use gpui::Window;
use objc::runtime::{BOOL, YES};
use objc::{class, msg_send, sel, sel_impl};
use std::path::{Path, PathBuf};

/// An autoreleased string, for a call now.
pub fn ns(s: &str) -> id {
    unsafe {
        let string = NSString::alloc(nil).init_str(s);
        msg_send![string, autorelease]
    }
}

pub fn from_ns(string: id) -> Option<String> {
    if string == nil {
        return None;
    }
    unsafe {
        let is_string: bool = msg_send![string, isKindOfClass: class!(NSString)];
        if !is_string {
            return None;
        }
        let utf8: *const std::os::raw::c_char = msg_send![string, UTF8String];
        (!utf8.is_null()).then(|| std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }
}

pub fn ns_data(bytes: &[u8]) -> id {
    unsafe { msg_send![class!(NSData), dataWithBytes: bytes.as_ptr() length: bytes.len() as u64] }
}

pub fn bytes_of(data: id) -> Vec<u8> {
    if data == nil {
        return Vec::new();
    }
    unsafe {
        let length: u64 = msg_send![data, length];
        let bytes: *const u8 = msg_send![data, bytes];
        if bytes.is_null() || length == 0 {
            return Vec::new();
        }
        std::slice::from_raw_parts(bytes, length as usize).to_vec()
    }
}

pub fn file_url(path: &Path) -> id {
    unsafe { msg_send![class!(NSURL), fileURLWithPath: ns(&path.to_string_lossy())] }
}

/// `error.localizedDescription`.
pub fn error_text(error: id) -> String {
    if error == nil {
        return String::new();
    }
    unsafe { from_ns(msg_send![error, localizedDescription]).unwrap_or_default() }
}

pub fn gpui_view_of(window: &Window) -> id {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else { return nil };
    match handle.as_raw() {
        RawWindowHandle::AppKit(h) => h.ns_view.as_ptr() as id,
        _ => nil,
    }
}

/// Makes `view` the window's first responder, if it isn't already: GPUI's view when GPUI is
/// clicked (AppKit would leave the keyboard with a web view), or a web view when it is.
pub fn claim_first_responder(ns_window: id, view: id) {
    if ns_window == nil || view == nil {
        return;
    }
    unsafe {
        let current: id = msg_send![ns_window, firstResponder];
        if current != view {
            let _: bool = msg_send![ns_window, makeFirstResponder: view];
        }
    }
}

/// Whether the press AppKit is handling now lands in `view` (a web view), going by what
/// AppKit's own hit testing finds there: GPUI's overlay views only answer inside GPUI's
/// hitboxes, so Search or the Info card over the page count as GPUI's.
pub fn press_lands_in(ns_window: id, view: id) -> bool {
    if ns_window == nil || view == nil {
        return false;
    }
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let event: id = msg_send![app, currentEvent];
        if event == nil {
            return false;
        }
        let event_window: id = msg_send![event, window];
        if event_window != ns_window {
            return false;
        }
        let point: cocoa::foundation::NSPoint = msg_send![event, locationInWindow];
        let content: id = msg_send![ns_window, contentView];
        let frame_view: id = msg_send![content, superview];
        let root = if frame_view != nil { frame_view } else { content };
        let hit: id = msg_send![root, hitTest: point];
        if hit == nil {
            return false;
        }
        let inside: bool = msg_send![hit, isDescendantOf: view];
        inside
    }
}

// The pasteboard ---------------------------------------------------------------

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSPasteboardTypeString: id;
    static NSPasteboardTypeHTML: id;
    static NSPasteboardTypePNG: id;
    static NSPasteboardTypeTIFF: id;
}

fn pasteboard() -> id {
    unsafe {
        let board: id = msg_send![class!(NSPasteboard), generalPasteboard];
        let _: i64 = msg_send![board, clearContents];
        board
    }
}

/// The article as rich text for the WeChat editor, with the Markdown as the
/// plain-text fallback.
pub fn copy_article(html: &str, markdown: &str) {
    let board = pasteboard();
    unsafe {
        let _: bool = msg_send![board, setString: ns(html) forType: NSPasteboardTypeHTML];
        let _: bool = msg_send![board, setString: ns(markdown) forType: NSPasteboardTypeString];
    }
}

pub fn copy_text(text: &str) {
    let board = pasteboard();
    unsafe {
        let _: bool = msg_send![board, setString: ns(text) forType: NSPasteboardTypeString];
    }
}

/// PNG, and TIFF for what doesn't read PNG.
pub fn copy_image(png: &[u8]) {
    let board = pasteboard();
    unsafe {
        let data = ns_data(png);
        let _: bool = msg_send![board, setData: data forType: NSPasteboardTypePNG];
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithData: data];
        if image != nil {
            let tiff: id = msg_send![image, TIFFRepresentation];
            if tiff != nil {
                let _: bool = msg_send![board, setData: tiff forType: NSPasteboardTypeTIFF];
            }
            let _: () = msg_send![image, release];
        }
    }
}

// Panels: modal, so call these from a spawned task -------------------------------

fn content_type(identifier: &str) -> id {
    load_framework("UniformTypeIdentifiers");
    match class_named("UTType") {
        Some(class) => unsafe { msg_send![class, typeWithIdentifier: ns(identifier)] },
        None => nil,
    }
}

pub struct SavePanel<'a> {
    pub title: Option<&'a str>,
    pub prompt: Option<&'a str>,
    pub message: Option<&'a str>,
    pub name: &'a str,
    pub directory: Option<&'a Path>,
    /// A uniform type identifier.
    pub content_type: Option<&'a str>,
}

pub fn run_save_panel(options: SavePanel) -> Option<PathBuf> {
    unsafe {
        let panel: id = msg_send![class!(NSSavePanel), savePanel];
        if panel == nil {
            return None;
        }
        if let Some(title) = options.title {
            let _: () = msg_send![panel, setTitle: ns(title)];
        }
        if let Some(prompt) = options.prompt {
            let _: () = msg_send![panel, setPrompt: ns(prompt)];
        }
        if let Some(message) = options.message {
            let _: () = msg_send![panel, setMessage: ns(message)];
        }
        let _: () = msg_send![panel, setCanCreateDirectories: true];
        let _: () = msg_send![panel, setNameFieldStringValue: ns(options.name)];
        if let Some(directory) = options.directory {
            let _: () = msg_send![panel, setDirectoryURL: file_url(directory)];
        }
        if let Some(kind) = options.content_type {
            let kind = content_type(kind);
            if kind != nil {
                let types: id = msg_send![class!(NSArray), arrayWithObject: kind];
                let _: () = msg_send![panel, setAllowedContentTypes: types];
            }
        }
        let answer: i64 = msg_send![panel, runModal];
        if answer != 1 {
            return None;
        }
        path_of_url(msg_send![panel, URL])
    }
}

/// File › Export All Notes…'s folder.
pub fn choose_export_folder() -> Option<PathBuf> {
    unsafe {
        let panel: id = msg_send![class!(NSOpenPanel), openPanel];
        if panel == nil {
            return None;
        }
        let _: () = msg_send![panel, setTitle: ns("Choose Export Folder")];
        let _: () = msg_send![panel, setCanChooseDirectories: true];
        let _: () = msg_send![panel, setCanChooseFiles: false];
        let _: () = msg_send![panel, setCanCreateDirectories: true];
        let _: () = msg_send![panel, setPrompt: ns("Export")];
        let answer: i64 = msg_send![panel, runModal];
        if answer != 1 {
            return None;
        }
        path_of_url(msg_send![panel, URL])
    }
}

/// A warning alert with one OK button.
pub fn warning_alert(title: &str, message: &str) {
    unsafe {
        let alert: id = msg_send![class!(NSAlert), new];
        let _: () = msg_send![alert, setAlertStyle: 0i64]; // NSAlertStyleWarning
        let _: () = msg_send![alert, setMessageText: ns(title)];
        let _: () = msg_send![alert, setInformativeText: ns(message)];
        let _: id = msg_send![alert, addButtonWithTitle: ns("OK")];
        let _: i64 = msg_send![alert, runModal];
        let _: () = msg_send![alert, release];
    }
}

// Colors and drawing -----------------------------------------------------------

pub fn color(hex: u32, alpha: f64) -> id {
    let channel = |shift: u32| ((hex >> shift) & 0xFF) as f64 / 255.;
    unsafe {
        msg_send![class!(NSColor), colorWithSRGBRed: channel(16) green: channel(8) blue: channel(0) alpha: alpha]
    }
}

pub fn rounded_path(rect: NSRect, radius: f64) -> id {
    unsafe { msg_send![class!(NSBezierPath), bezierPathWithRoundedRect: rect xRadius: radius yRadius: radius] }
}

pub fn fill_rect(rect: NSRect, radius: f64, fill: id) {
    unsafe {
        let _: () = msg_send![fill, setFill];
        let _: () = msg_send![rounded_path(rect, radius), fill];
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

/// A page in miniature for the theme menu: an accent heading, two lines. Lives as
/// long as the app; there are four.
pub fn theme_swatch(theme: Theme) -> id {
    let accent = theme.accent();
    let handler = ConcreteBlock::new(move |bounds: NSRect| -> BOOL {
        unsafe {
            let inset = rect(bounds.origin.x + 0.5, bounds.origin.y + 0.5, bounds.size.width - 1., bounds.size.height - 1.);
            let page = rounded_path(inset, 3.);
            let _: () = msg_send![color(0xFFFFFF, 1.), setFill];
            let _: () = msg_send![page, fill];
            let _: () = msg_send![color(0, 0.2), setStroke];
            let _: () = msg_send![page, setLineWidth: 0.5f64];
            let _: () = msg_send![page, stroke];
            fill_rect(rect(3., 3., 10., 2.5), 1., color(accent, 1.));
            let line = color(0xC7C7C7, 1.);
            fill_rect(rect(3., 7., 14., 1.5), 0., line);
            fill_rect(rect(3., 10., 11., 1.5), 0., line);
        }
        YES
    })
    .copy();
    unsafe {
        let image: id = msg_send![class!(NSImage),
            imageWithSize: NSSize::new(20., 14.)
            flipped: true
            drawingHandler: &*handler];
        msg_send![image, retain]
    }
}

/// The swatches, made once.
pub fn theme_swatches() -> [id; Theme::ALL.len()] {
    thread_local! {
        static SWATCHES: [usize; Theme::ALL.len()] = Theme::ALL.map(|t| theme_swatch(t) as usize);
    }
    SWATCHES.with(|s| s.map(|i| i as id))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use objc::runtime::{Class, Sel};

    fn class_of(name: &str) -> &'static Class {
        Class::get(name).unwrap_or_else(|| panic!("no class {name}"))
    }

    /// Whether instances, or for `+` the class, answer the selector.
    pub(crate) fn check(class: &str, selector: &str) {
        let (is_class, name) = match selector.strip_prefix('+') {
            Some(name) => (true, name),
            None => (false, selector),
        };
        let c = class_of(class);
        let sel = Sel::register(name);
        let ok: bool = unsafe {
            if is_class {
                msg_send![c, respondsToSelector: sel]
            } else {
                let direct: bool = msg_send![c, instancesRespondToSelector: sel];
                // A class cluster's real instances are of a subclass.
                direct || {
                    let instance: id = msg_send![c, new];
                    instance != nil && msg_send![instance, respondsToSelector: sel]
                }
            }
        };
        assert!(ok, "{class} doesn't answer {selector}");
    }

    /// An ObjC exception unwinding through Rust aborts the app, so every selector
    /// sent by this crate is checked here against the running system.
    #[test]
    fn every_selector_exists() {
        for framework in ["AppKit", "WebKit", "Photos", "UniformTypeIdentifiers", "CoreGraphics"] {
            load_framework(framework);
        }
        let list: &[(&str, &[&str])] = &[
            ("NSPasteboard", &["+generalPasteboard", "clearContents", "setString:forType:", "setData:forType:"]),
            ("NSImage", &["+imageWithSize:flipped:drawingHandler:", "initWithData:", "initWithContentsOfFile:", "TIFFRepresentation",
                "drawInRect:", "drawInRect:fromRect:operation:fraction:respectFlipped:hints:", "size", "setSize:", "initWithSize:", "addRepresentation:",
                "+imageWithSystemSymbolName:accessibilityDescription:", "imageWithSymbolConfiguration:"]),
            ("NSData", &["+dataWithBytes:length:", "length", "bytes"]),
            ("NSURL", &["+fileURLWithPath:", "+URLWithString:", "host", "path"]),
            ("NSString", &["UTF8String", "stringByRemovingPercentEncoding", "drawAtPoint:withAttributes:", "drawInRect:withAttributes:", "sizeWithAttributes:"]),
            ("NSSavePanel", &["+savePanel", "setTitle:", "setPrompt:", "setMessage:", "setCanCreateDirectories:",
                "setNameFieldStringValue:", "setDirectoryURL:", "setAllowedContentTypes:", "runModal", "URL"]),
            ("NSOpenPanel", &["+openPanel", "setCanChooseDirectories:", "setCanChooseFiles:"]),
            ("UTType", &["+typeWithIdentifier:"]),
            ("NSAlert", &["setAlertStyle:", "setMessageText:", "setInformativeText:", "addButtonWithTitle:", "runModal"]),
            ("NSColor", &["+colorWithSRGBRed:green:blue:alpha:", "setFill", "setStroke", "alphaComponent"]),
            ("NSBezierPath", &["+bezierPathWithRoundedRect:xRadius:yRadius:", "+bezierPath", "+fillRect:", "+clipRect:", "fill", "stroke", "addClip",
                "setLineWidth:", "moveToPoint:", "lineToPoint:"]),
            ("NSWindow", &["firstResponder", "makeFirstResponder:", "contentView", "setContentView:",
                "initWithContentRect:styleMask:backing:defer:", "setReleasedWhenClosed:", "setIgnoresMouseEvents:",
                "setExcludedFromWindowsMenu:", "setCollectionBehavior:", "orderFrontRegardless", "orderOut:",
                "setContentSize:", "backingScaleFactor"]),
            ("NSView", &["addSubview:positioned:relativeTo:", "addSubview:", "removeFromSuperview", "setFrame:", "setHidden:",
                "setWantsLayer:", "layer", "setAlphaValue:", "superview", "isFlipped", "willOpenMenu:withEvent:", "scrollWheel:"]),
            ("CALayer", &["setMasksToBounds:", "setCornerRadius:", "setMaskedCorners:"]),
            ("NSMenu", &["removeAllItems"]),
            ("NSMenuItem", &["+sectionHeaderWithTitle:", "setKeyEquivalentModifierMask:"]),
            ("NSJSONSerialization", &["+isValidJSONObject:", "+dataWithJSONObject:options:error:"]),
            ("NSURLResponse", &["initWithURL:MIMEType:expectedContentLength:textEncodingName:"]),
            ("NSError", &["+errorWithDomain:code:userInfo:", "localizedDescription", "userInfo"]),
            ("NSDictionary", &["objectForKey:", "+dictionaryWithObject:forKey:"]),
            ("NSURLRequest", &["+requestWithURL:", "URL"]),
            ("NSWorkspace", &["+sharedWorkspace", "openURL:"]),
            ("NSNumber", &["+numberWithBool:", "+numberWithDouble:", "+numberWithLong:"]),
            ("NSBitmapImageRep", &["initWithBitmapDataPlanes:pixelsWide:pixelsHigh:bitsPerSample:samplesPerPixel:hasAlpha:isPlanar:colorSpaceName:bytesPerRow:bitsPerPixel:",
                "setSize:", "bitmapData", "bitmapImageRepByConvertingToColorSpace:renderingIntent:", "representationUsingType:properties:"]),
            ("NSColorSpace", &["+sRGBColorSpace"]),
            ("NSGraphicsContext", &["+graphicsContextWithBitmapImageRep:", "+graphicsContextWithCGContext:flipped:",
                "+saveGraphicsState", "+restoreGraphicsState", "+setCurrentContext:", "setImageInterpolation:"]),
            ("NSMutableDictionary", &["+dictionary", "setObject:forKey:"]),
            ("NSMutableArray", &["+arrayWithCapacity:", "addObject:"]),
            ("NSMutableData", &["+data"]),
            ("NSBundle", &["+mainBundle", "objectForInfoDictionaryKey:"]),
            ("NSDate", &["+date", "+dateWithTimeInterval:sinceDate:"]),
            ("NSSharingService", &["+sharingServiceNamed:", "canPerformWithItems:", "performWithItems:"]),
            ("PHPhotoLibrary", &["+sharedPhotoLibrary", "+requestAuthorizationForAccessLevel:handler:", "performChanges:completionHandler:"]),
            ("PHAssetCreationRequest", &["+creationRequestForAsset", "addResourceWithType:fileURL:options:", "setCreationDate:"]),
            ("WKWebViewConfiguration", &["userContentController", "setURLSchemeHandler:forURLScheme:", "setSuppressesIncrementalRendering:"]),
            ("WKUserContentController", &["addScriptMessageHandler:name:", "removeScriptMessageHandlerForName:", "addUserScript:"]),
            ("WKUserScript", &["initWithSource:injectionTime:forMainFrameOnly:"]),
            ("WKScriptMessage", &["body"]),
            ("WKNavigationAction", &["navigationType", "request"]),
            ("WKContentWorld", &["+pageWorld"]),
            ("WKSnapshotConfiguration", &["setRect:", "setSnapshotWidth:"]),
            ("WKWebView", &["initWithFrame:configuration:", "configuration", "setNavigationDelegate:", "loadHTMLString:baseURL:", "loadRequest:",
                "evaluateJavaScript:completionHandler:", "callAsyncJavaScript:arguments:inFrame:inContentWorld:completionHandler:",
                "takeSnapshotWithConfiguration:completionHandler:"]),
            ("NSLayoutManager", &["addTextContainer:", "ensureLayoutForTextContainer:", "glyphRangeForTextContainer:",
                "lineFragmentRectForGlyphAtIndex:effectiveRange:", "locationForGlyphAtIndex:", "drawGlyphsForGlyphRange:atPoint:",
                "glyphRangeForCharacterRange:actualCharacterRange:", "boundingRectForGlyphRange:inTextContainer:"]),
            ("NSTextStorage", &["initWithAttributedString:", "addLayoutManager:"]),
            ("NSTextContainer", &["initWithContainerSize:", "setLineFragmentPadding:", "setMaximumNumberOfLines:"]),
            ("NSMutableParagraphStyle", &["setMinimumLineHeight:", "setMaximumLineHeight:", "setLineSpacing:", "setAlignment:", "setLineBreakMode:"]),
            ("NSMutableAttributedString", &["appendAttributedString:", "addAttribute:value:range:", "length"]),
            ("NSAttributedString", &["+attributedStringWithAttachment:", "initWithString:attributes:", "size",
                "boundingRectWithSize:options:", "drawWithRect:options:"]),
            ("NSTextAttachment", &["setImage:", "setBounds:"]),
            ("NSFont", &["+systemFontOfSize:weight:", "+fontWithName:size:", "+fontWithDescriptor:size:", "fontDescriptor", "ascender", "descender", "capHeight",
                "+systemFontOfSize:", "+monospacedDigitSystemFontOfSize:weight:"]),
            ("NSFontDescriptor", &["fontDescriptorWithDesign:"]),
            ("NSFontManager", &["+sharedFontManager", "convertFont:toHaveTrait:", "fontWithFamily:traits:weight:size:"]),
            ("NSImageSymbolConfiguration", &["+configurationWithPointSize:weight:", "+configurationWithPaletteColors:", "configurationByApplyingConfiguration:"]),
        ];
        for (class, selectors) in list {
            for selector in *selectors {
                check(class, selector);
            }
        }
    }
}
