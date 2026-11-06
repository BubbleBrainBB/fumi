//! `WKWebView` over a GPUI layout slot, as the spike proved it (spikes/REPORT.md,
//! section 2). AppKit draws it above all of GPUI, so it can't be clipped by GPUI's
//! masks: it sits in a container the size of what shows, which clips it, and which
//! takes the sheet's rounded corners.

use crate::mac::{self, error_text, from_ns, ns};
use crate::xhs::{self, Asset};
use block::{Block, ConcreteBlock};
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use futures::channel::{mpsc, oneshot};
use fumi_ui::native::load_framework;
use gpui::*;
use objc::declare::ClassDecl;
use objc::runtime::{BOOL, Class, Object, Sel, YES};
use objc::{class, msg_send, sel, sel_impl};
use std::cell::Cell;

pub const SCHEME: &str = "fumi-xhs";
pub const CARDS_PAGE_URL: &str = "fumi-xhs://app/cards.html";

/// What a web view tells the pane.
#[derive(Clone, Debug, PartialEq)]
pub enum WebEvent {
    Finished,
    Message(serde_json::Value),
}

type Sink = mpsc::UnboundedSender<WebEvent>;

fn sink_of(this: &Object) -> Option<&Sink> {
    let raw: usize = unsafe { *this.get_ivar("sink") };
    (raw != 0).then(|| unsafe { &*(raw as *const Sink) })
}

/// A script message's body as JSON: a string, or what `NSJSONSerialization` takes.
fn json_of(body: id) -> Option<serde_json::Value> {
    if let Some(text) = from_ns(body) {
        return Some(serde_json::Value::String(text));
    }
    if body == nil {
        return None;
    }
    unsafe {
        // `dataWithJSONObject:` raises on anything else, and that would abort.
        let valid: bool = msg_send![class!(NSJSONSerialization), isValidJSONObject: body];
        if !valid {
            return None;
        }
        let error: *mut id = std::ptr::null_mut();
        let data: id = msg_send![class!(NSJSONSerialization), dataWithJSONObject: body options: 0u64 error: error];
        serde_json::from_slice(&mac::bytes_of(data)).ok()
    }
}

extern "C" fn did_receive(this: &Object, _: Sel, _: id, message: id) {
    let body: id = unsafe { msg_send![message, body] };
    if let (Some(sink), Some(value)) = (sink_of(this), json_of(body)) {
        let _ = sink.unbounded_send(WebEvent::Message(value));
    }
}

/// `WKNavigationTypeLinkActivated`, `WKNavigationActionPolicyCancel`, `…Allow`.
const LINK_ACTIVATED: i64 = 0;
const POLICY_CANCEL: i64 = 0;
const POLICY_ALLOW: i64 = 1;

/// Links open in the browser (the article) or not at all (the cards), never inside
/// the preview.
extern "C" fn decide_policy(this: &Object, _: Sel, _: id, action: id, handler: id) {
    let handler = handler as *mut Block<(i64,), ()>;
    unsafe {
        let kind: i64 = msg_send![action, navigationType];
        if kind == LINK_ACTIVATED {
            let opens: usize = *this.get_ivar("opens_links");
            if opens != 0 {
                let request: id = msg_send![action, request];
                let url: id = msg_send![request, URL];
                if url != nil {
                    let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
                    let _: bool = msg_send![workspace, openURL: url];
                }
            }
            (*handler).call((POLICY_CANCEL,));
        } else {
            (*handler).call((POLICY_ALLOW,));
        }
    }
}

extern "C" fn did_finish(this: &Object, _: Sel, _: id, _: id) {
    if let Some(sink) = sink_of(this) {
        let _ = sink.unbounded_send(WebEvent::Finished);
    }
}

fn delegate_class() -> &'static Class {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let mut decl = ClassDecl::new("FumiPublishWebDelegate", class!(NSObject)).expect("class is new");
        decl.add_ivar::<usize>("sink");
        decl.add_ivar::<usize>("opens_links");
        unsafe {
            decl.add_method(
                sel!(userContentController:didReceiveScriptMessage:),
                did_receive as extern "C" fn(&Object, Sel, id, id),
            );
            decl.add_method(
                sel!(webView:decidePolicyForNavigationAction:decisionHandler:),
                decide_policy as extern "C" fn(&Object, Sel, id, id, id),
            );
            decl.add_method(sel!(webView:didFinishNavigation:), did_finish as extern "C" fn(&Object, Sel, id, id));
        }
        decl.register();
    });
    Class::get("FumiPublishWebDelegate").expect("registered")
}

// The cards' web view -----------------------------------------------------------

/// The cards' own menu replaces the web view's.
extern "C" fn will_open_menu(this: &Object, _: Sel, menu: id, event: id) {
    unsafe {
        let forward: usize = *this.get_ivar("forward_scroll");
        if forward != 0 {
            let _: () = msg_send![menu, removeAllItems];
        } else {
            let superclass = Class::get("WKWebView").expect("WebKit");
            let _: () = msg_send![super(this, superclass), willOpenMenu: menu withEvent: event];
        }
    }
}

/// The view is as tall as the cards; the pane around it scrolls, and that's GPUI's.
extern "C" fn scroll_wheel(this: &Object, _: Sel, event: id) {
    unsafe {
        let forward: usize = *this.get_ivar("forward_scroll");
        if forward != 0 {
            let _: () = msg_send![forward as id, scrollWheel: event];
        } else {
            let superclass = Class::get("WKWebView").expect("WebKit");
            let _: () = msg_send![super(this, superclass), scrollWheel: event];
        }
    }
}

/// The page took the keyboard, by a click or otherwise: the pane hears of it as it
/// hears of a press in the page, and takes GPUI's focus off the editor, so the keys GPUI
/// still sees (key equivalents reach the GPUI view first) find no editor binding.
extern "C" fn become_first_responder(this: &Object, _: Sel) -> BOOL {
    unsafe {
        let superclass = Class::get("WKWebView").expect("WebKit");
        let became: BOOL = msg_send![super(this, superclass), becomeFirstResponder];
        if became == YES {
            let delegate: id = msg_send![this, navigationDelegate];
            if delegate != nil
                && let Some(sink) = sink_of(&*delegate)
            {
                let _ = sink.unbounded_send(WebEvent::Message(serde_json::Value::String("pointer".into())));
            }
        }
        became
    }
}

fn web_view_class() -> &'static Class {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let superclass = Class::get("WKWebView").expect("WebKit");
        let mut decl = ClassDecl::new("FumiPublishWebView", superclass).expect("class is new");
        decl.add_ivar::<usize>("forward_scroll");
        unsafe {
            decl.add_method(sel!(willOpenMenu:withEvent:), will_open_menu as extern "C" fn(&Object, Sel, id, id));
            decl.add_method(sel!(scrollWheel:), scroll_wheel as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(becomeFirstResponder), become_first_responder as extern "C" fn(&Object, Sel) -> BOOL);
        }
        decl.register();
    });
    Class::get("FumiPublishWebView").expect("registered")
}

// fumi-xhs:// --------------------------------------------------------------------

extern "C" fn start_task(_: &Object, _: Sel, _: id, task: id) {
    unsafe {
        let request: id = msg_send![task, request];
        let url: id = msg_send![request, URL];
        if url == nil {
            return;
        }
        let host = from_ns(msg_send![url, host]).unwrap_or_default();
        let path: id = msg_send![url, path];
        let decoded: id = msg_send![path, stringByRemovingPercentEncoding];
        let path = from_ns(if decoded != nil { decoded } else { path }).unwrap_or_default();
        let loaded = xhs::asset_for(&host, &path).and_then(|asset| match asset {
            Asset::Embedded(text, mime) => Some((text.as_bytes().to_vec(), mime)),
            Asset::File(file, mime) => std::fs::read(file).ok().map(|data| (data, mime)),
        });
        let Some((data, mime)) = loaded else {
            // NSURLErrorFileDoesNotExist.
            let error: id = msg_send![class!(NSError), errorWithDomain: ns("NSURLErrorDomain") code: -1100i64 userInfo: nil];
            let _: () = msg_send![task, didFailWithError: error];
            return;
        };
        let response: id = msg_send![class!(NSURLResponse), alloc];
        let response: id = msg_send![response,
            initWithURL: url
            MIMEType: ns(mime)
            expectedContentLength: data.len() as i64
            textEncodingName: nil];
        let _: () = msg_send![task, didReceiveResponse: response];
        let _: () = msg_send![response, release];
        let _: () = msg_send![task, didReceiveData: mac::ns_data(&data)];
        let _: () = msg_send![task, didFinish];
    }
}

extern "C" fn stop_task(_: &Object, _: Sel, _: id, _: id) {}

fn scheme_handler() -> id {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let mut decl = ClassDecl::new("FumiPublishAssets", class!(NSObject)).expect("class is new");
        unsafe {
            decl.add_method(sel!(webView:startURLSchemeTask:), start_task as extern "C" fn(&Object, Sel, id, id));
            decl.add_method(sel!(webView:stopURLSchemeTask:), stop_task as extern "C" fn(&Object, Sel, id, id));
        }
        decl.register();
    });
    let class = Class::get("FumiPublishAssets").expect("registered");
    unsafe { msg_send![class, new] }
}

// A web view ---------------------------------------------------------------------

/// A rectangle in points, top-left origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn of(b: Bounds<Pixels>) -> Rect {
        Rect {
            x: f32::from(b.origin.x) as f64,
            y: f32::from(b.origin.y) as f64,
            w: f32::from(b.size.width) as f64,
            h: f32::from(b.size.height) as f64,
        }
    }

    fn bottom(&self) -> f64 {
        self.y + self.h
    }

    fn right(&self) -> f64 {
        self.x + self.w
    }

    fn intersect(&self, other: &Rect) -> Option<Rect> {
        let (x0, y0) = (self.x.max(other.x), self.y.max(other.y));
        let (x1, y1) = (self.right().min(other.right()), self.bottom().min(other.bottom()));
        (x1 > x0 && y1 > y0).then_some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
    }
}

/// Where the container goes in its unflipped parent, where the web view goes in the
/// unflipped container, and whether what shows reaches the clip's bottom edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frames {
    pub container: (f64, f64, f64, f64),
    pub web: (f64, f64, f64, f64),
    pub at_bottom: bool,
}

/// `None` when none of the slot shows.
pub fn frames_for(slot: Rect, clip: Rect, parent_height: f64) -> Option<Frames> {
    let shown = slot.intersect(&clip)?;
    Some(Frames {
        container: (shown.x, parent_height - shown.bottom(), shown.w, shown.h),
        web: (slot.x - shown.x, shown.bottom() - slot.bottom(), slot.w, slot.h),
        at_bottom: (shown.bottom() - clip.bottom()).abs() < 0.5,
    })
}

fn ns_rect((x, y, w, h): (f64, f64, f64, f64)) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

pub struct WebConfig<'a> {
    pub handler: &'a str,
    /// Clicked links go to the browser.
    pub opens_links: bool,
    /// Serve `fumi-xhs://`.
    pub assets: bool,
    /// Wheel events go to GPUI's view, and the web view's own menu is empty.
    pub forwards_to: Option<id>,
}

/// One web view, its delegate and the container that clips it.
pub struct WebHost {
    pub web: id,
    delegate: id,
    sink: *mut Sink,
    handler: String,
    container: id,
    ns_window: id,
    gpui_view: id,
    last: Option<Frames>,
    last_corner: Option<bool>,
    last_alpha: f64,
    hidden: bool,
    last_width: f64,
    radius: f64,
    /// The window composition surface the container lives in: GPUI's root view draws
    /// below it and its deferred draws (Search, Settings, menus, the Info card) above.
    surface: WindowCompositionSurface,
}

/// The composition surface is a subview of GPUI's view, so a key the page leaves alone
/// would climb the responder chain into it and land in the editor. The chain skips GPUI's
/// view instead, going on to its superview, as it did when the web view was its sibling.
fn keep_keys_out_of_gpui(container: id, gpui_view: id) {
    unsafe {
        let parent: id = msg_send![gpui_view, superview];
        let next: id = msg_send![container, nextResponder];
        if parent != nil && next != parent {
            let _: () = msg_send![container, setNextResponder: parent];
        }
    }
}

/// A native composition surface and its `NSView`; `None` if the window can't compose.
fn native_surface(window: &Window) -> Option<(WindowCompositionSurface, id)> {
    let composition = window.enable_window_composition().map_err(|e| eprintln!("compose: {e:#}")).ok()?;
    let surface = composition.create_native_surface().map_err(|e| eprintln!("compose: {e:#}")).ok()?;
    let handle = surface.platform_surface().ok()?.platform_handle().ok()?;
    let view = *handle.downcast::<usize>().ok()? as id;
    Some((surface, view))
}

/// A script message handler told about each press in the page.
fn pointer_script(handler: &str) -> String {
    format!("addEventListener('mousedown',()=>window.webkit.messageHandlers.{handler}.postMessage('pointer'),true);")
}

/// The web view itself, retained, and what talks to it. `None` without WebKit.
pub fn make_web_view(config: &WebConfig) -> Option<(id, id, *mut Sink, mpsc::UnboundedReceiver<WebEvent>)> {
    load_framework("WebKit");
    Class::get("WKWebView")?;
    unsafe {
        let (send, receive) = mpsc::unbounded();
        let sink = Box::into_raw(Box::new(send));
        let delegate: id = msg_send![delegate_class(), new];
        (*delegate).set_ivar::<usize>("sink", sink as usize);
        (*delegate).set_ivar::<usize>("opens_links", config.opens_links as usize);

        let configuration: id = msg_send![Class::get("WKWebViewConfiguration")?, new];
        let controller: id = msg_send![configuration, userContentController];
        let _: () = msg_send![controller, addScriptMessageHandler: delegate name: ns(config.handler)];
        let script: id = msg_send![Class::get("WKUserScript")?, alloc];
        // WKUserScriptInjectionTimeAtDocumentEnd.
        let script: id = msg_send![script,
            initWithSource: ns(&pointer_script(config.handler))
            injectionTime: 1i64
            forMainFrameOnly: true];
        let _: () = msg_send![controller, addUserScript: script];
        let _: () = msg_send![script, release];
        if config.assets {
            let handler = scheme_handler();
            let _: () = msg_send![configuration, setURLSchemeHandler: handler forURLScheme: ns(SCHEME)];
            let _: () = msg_send![handler, release];
            let _: () = msg_send![configuration, setSuppressesIncrementalRendering: true];
        }
        let web: id = msg_send![web_view_class(), alloc];
        let web: id = msg_send![web, initWithFrame: ns_rect((0., 0., 300., 400.)) configuration: configuration];
        let _: () = msg_send![configuration, release];
        (*web).set_ivar::<usize>("forward_scroll", config.forwards_to.map_or(0, |v| v as usize));
        let no: id = msg_send![class!(NSNumber), numberWithBool: false];
        let _: () = msg_send![web, setValue: no forKey: ns("drawsBackground")];
        let _: () = msg_send![web, setNavigationDelegate: delegate];
        Some((web, delegate, sink, receive))
    }
}

/// What `make_web_view` made, taken apart.
pub fn release_web_view(web: id, delegate: id, sink: *mut Sink, handler: &str) {
    unsafe {
        let _: () = msg_send![web, setNavigationDelegate: nil];
        let configuration: id = msg_send![web, configuration];
        let controller: id = msg_send![configuration, userContentController];
        let _: () = msg_send![controller, removeScriptMessageHandlerForName: ns(handler)];
        (*delegate).set_ivar::<usize>("sink", 0);
        let _: () = msg_send![delegate, release];
        let _: () = msg_send![web, release];
        drop(Box::from_raw(sink));
    }
}

impl WebHost {
    pub fn new(config: WebConfig, window: &Window) -> Option<(WebHost, mpsc::UnboundedReceiver<WebEvent>)> {
        let (web, delegate, sink, receive) = make_web_view(&config)?;
        let gpui_view = mac::gpui_view_of(window);
        let ns_window = fumi_ui::native::ns_window_of(window);
        let Some((surface, surface_view)) = native_surface(window) else {
            release_web_view(web, delegate, sink, config.handler);
            return None;
        };
        unsafe {
            let container: id = msg_send![class!(NSView), alloc];
            let container: id = msg_send![container, initWithFrame: ns_rect((0., 0., 10., 10.))];
            let _: () = msg_send![container, setWantsLayer: true];
            let layer: id = msg_send![container, layer];
            let _: () = msg_send![layer, setMasksToBounds: true];
            let _: () = msg_send![container, setHidden: true];
            let _: () = msg_send![container, addSubview: web];
            let _: () = msg_send![surface_view, addSubview: container];
            keep_keys_out_of_gpui(container, gpui_view);
            Some((
                WebHost {
                    web,
                    delegate,
                    sink,
                    handler: config.handler.to_string(),
                    container,
                    ns_window,
                    gpui_view,
                    last: None,
                    last_corner: None,
                    last_alpha: 1.,
                    hidden: true,
                    last_width: 0.,
                    radius: 0.,
                    surface,
                },
                receive,
            ))
        }
    }

    pub fn window_handles(&self) -> (usize, usize) {
        (self.ns_window as usize, self.gpui_view as usize)
    }

    pub fn set_corner_radius(&mut self, radius: f64) {
        if self.radius != radius {
            self.radius = radius;
            self.last_corner = None;
        }
    }

    /// Puts the web view on `slot`, showing what `clip` lets through. True when its
    /// width changed.
    pub fn place(&mut self, slot: Rect, clip: Rect, alpha: f64) -> bool {
        let parent_height: f64 = unsafe {
            let parent: id = msg_send![self.gpui_view, superview];
            let frame: NSRect = msg_send![parent, frame];
            frame.size.height
        };
        let Some(frames) = frames_for(slot, clip, parent_height) else {
            self.hide();
            return false;
        };
        unsafe {
            if self.last != Some(frames) {
                self.last = Some(frames);
                keep_keys_out_of_gpui(self.container, self.gpui_view);
                // The surface goes where the container would, in top-left device pixels,
                // and the container fills it.
                let (x, y, w, h) = frames.container;
                if let Ok(platform) = self.surface.platform_surface() {
                    let scale: f64 = msg_send![self.ns_window, backingScaleFactor];
                    let top = parent_height - y - h;
                    let d = |v: f64| DevicePixels((v * scale).round() as i32);
                    let _ = platform.set_bounds(Bounds { origin: point(d(x), d(top)), size: size(d(w), d(h)) });
                }
                let _: () = msg_send![self.container, setFrame: ns_rect((0., 0., w, h))];
                let _: () = msg_send![self.web, setFrame: ns_rect(frames.web)];
            }
            if self.last_corner != Some(frames.at_bottom) {
                self.last_corner = Some(frames.at_bottom);
                let layer: id = msg_send![self.container, layer];
                let _: () = msg_send![layer, setCornerRadius: if frames.at_bottom { self.radius } else { 0. }];
                // The unflipped layer's two bottom corners.
                let _: () = msg_send![layer, setMaskedCorners: 3u64];
            }
            if self.last_alpha != alpha {
                self.last_alpha = alpha;
                let _: () = msg_send![self.container, setAlphaValue: alpha];
            }
            if self.hidden {
                self.hidden = false;
                if let Ok(platform) = self.surface.platform_surface() {
                    let _ = platform.set_visible(true);
                }
                let _: () = msg_send![self.container, setHidden: false];
            }
        }
        let changed = (slot.w - self.last_width).abs() > 0.5;
        self.last_width = slot.w;
        changed
    }

    pub fn hide(&mut self) {
        if !self.hidden {
            self.hidden = true;
            unsafe {
                let _: () = msg_send![self.container, setHidden: true];
            }
            if let Ok(platform) = self.surface.platform_surface() {
                let _ = platform.set_visible(false);
            }
            mac::claim_first_responder(self.ns_window, self.gpui_view);
        }
    }

    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    pub fn eval(&self, script: &str) {
        evaluate(self.web, script);
    }
}

impl Drop for WebHost {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.container, removeFromSuperview];
            let _: () = msg_send![self.web, removeFromSuperview];
            let _: () = msg_send![self.container, release];
        }
        release_web_view(self.web, self.delegate, self.sink, &self.handler);
        mac::claim_first_responder(self.ns_window, self.gpui_view);
    }
}

pub fn evaluate(web: id, script: &str) {
    if script.is_empty() {
        return;
    }
    let none: *const std::ffi::c_void = std::ptr::null();
    unsafe {
        let _: () = msg_send![web, evaluateJavaScript: ns(script) completionHandler: none];
    }
}

/// The body's `return`, as text; or what went wrong, in the page's words if it can.
pub fn call_async(web: id, body: &str, json_argument: &str) -> oneshot::Receiver<Result<String, String>> {
    let (send, receive) = oneshot::channel();
    let send = Cell::new(Some(send));
    let handler = ConcreteBlock::new(move |result: id, error: id| {
        let Some(send) = send.take() else { return };
        let answer = if error != nil { Err(script_error(error)) } else { Ok(from_ns(result).unwrap_or_default()) };
        let _ = send.send(answer);
    })
    .copy();
    unsafe {
        let world: id = msg_send![Class::get("WKContentWorld").expect("WebKit"), pageWorld];
        let arguments: id = msg_send![class!(NSDictionary), dictionaryWithObject: ns(json_argument) forKey: ns("json")];
        let frame: id = nil;
        let _: () = msg_send![web,
            callAsyncJavaScript: ns(body)
            arguments: arguments
            inFrame: frame
            inContentWorld: world
            completionHandler: &*handler];
    }
    receive
}

/// `WKJavaScriptExceptionMessage`, or the error's description.
fn script_error(error: id) -> String {
    unsafe {
        let info: id = msg_send![error, userInfo];
        if info != nil {
            let message: id = msg_send![info, objectForKey: ns("WKJavaScriptExceptionMessage")];
            if let Some(message) = from_ns(message) {
                return message;
            }
        }
    }
    error_text(error)
}

/// What the view shows, as an `NSImage` retained for the receiver. Nil on failure.
pub fn snapshot(web: id, configuration: id) -> oneshot::Receiver<Result<usize, String>> {
    let (send, receive) = oneshot::channel();
    let send = Cell::new(Some(send));
    let handler = ConcreteBlock::new(move |image: id, error: id| {
        let Some(send) = send.take() else { return };
        let answer = if image != nil {
            let retained: id = unsafe { msg_send![image, retain] };
            Ok(retained as usize)
        } else {
            Err(error_text(error))
        };
        let _ = send.send(answer);
    })
    .copy();
    unsafe {
        let _: () = msg_send![web, takeSnapshotWithConfiguration: configuration completionHandler: &*handler];
    }
    receive
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[core::prelude::v1::test]
    fn a_slot_inside_the_clip_fills_its_container() {
        let f = frames_for(r(100., 100., 420., 300.), r(100., 50., 420., 800.), 1000.).unwrap();
        assert_eq!(f.container, (100., 600., 420., 300.));
        assert_eq!(f.web, (0., 0., 420., 300.));
        assert!(!f.at_bottom);
    }

    #[core::prelude::v1::test]
    fn scrolled_up_the_top_of_the_view_is_cut_off() {
        // The slot's top 100 is above the clip: the web view hangs out of the
        // container's top, in an unflipped container, by that much.
        let f = frames_for(r(100., 0., 420., 300.), r(100., 100., 420., 800.), 1000.).unwrap();
        assert_eq!(f.container, (100., 700., 420., 200.));
        assert_eq!(f.web, (0., 0., 420., 300.));
    }

    #[core::prelude::v1::test]
    fn scrolled_down_the_bottom_is_cut_off() {
        let f = frames_for(r(100., 700., 420., 300.), r(100., 100., 420., 800.), 1000.).unwrap();
        assert_eq!(f.container, (100., 100., 420., 200.));
        // The web view's bottom hangs 100 below the container's.
        assert_eq!(f.web, (0., -100., 420., 300.));
        assert!(f.at_bottom);
    }

    #[core::prelude::v1::test]
    fn out_of_the_clip_nothing_shows() {
        assert_eq!(frames_for(r(0., 0., 10., 10.), r(0., 20., 10., 10.), 100.), None);
    }

    #[core::prelude::v1::test]
    fn pointer_script_names_its_handler() {
        assert!(pointer_script("xhs").contains("messageHandlers.xhs.postMessage('pointer')"));
    }
}
