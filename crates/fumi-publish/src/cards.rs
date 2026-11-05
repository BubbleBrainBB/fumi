//! Renders cards to PNG off screen: the layout at full size in a web view in a
//! window nobody sees, snapshotted card by card at 1080 pixels wide. And where the
//! files go: Photos and AirDrop. A port of `XhsRenderer` and the export half of
//! `XhsCards`.

use crate::mac::{self, ns};
use crate::web::{self, WebConfig, WebEvent};
use crate::xhs::{self, EXPORT_WIDTH, Input};
use block::ConcreteBlock;
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize, NSUInteger};
use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use futures::lock::Mutex;
use fumi_ui::native::{class_named, load_framework};
use gpui::AsyncApp;
use objc::runtime::BOOL;
use objc::{class, msg_send, sel, sel_impl};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

pub const LAYOUT_SCRIPT: &str = "return JSON.stringify(await FumiXhs.layout(JSON.parse(json)))";

const COULD_NOT_LAY_OUT: &str = "The cards couldn't be laid out.";
const COULD_NOT_DRAW: &str = "The card couldn't be drawn.";
const COULD_NOT_SAVE: &str = "The card couldn't be saved as PNG.";
pub const PHOTOS_DENIED: &str = "Fumi can't add to Photos. Allow it in System Settings › Privacy & Security › Photos.";
pub const USAGE_KEY: &str = "NSPhotoLibraryAddUsageDescription";

struct Offscreen {
    window: id,
    web: id,
    events: RefCell<mpsc::UnboundedReceiver<WebEvent>>,
    loaded: Cell<bool>,
    /// One render at a time: they share the one page.
    turn: Mutex<()>,
}

thread_local! {
    static SHARED: RefCell<Option<Rc<Offscreen>>> = const { RefCell::new(None) };
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

fn shared() -> Result<Rc<Offscreen>, String> {
    if let Some(existing) = SHARED.with(|s| s.borrow().clone()) {
        return Ok(existing);
    }
    let config = WebConfig { handler: "xhsOffscreen", opens_links: false, assets: true, forwards_to: None };
    let (web, _delegate, _sink, events) = web::make_web_view(&config).ok_or_else(|| COULD_NOT_LAY_OUT.to_string())?;
    let window = unsafe {
        let window: id = msg_send![class!(NSWindow), alloc];
        let window: id = msg_send![window,
            initWithContentRect: rect(-20000., -20000., 300., 400.)
            styleMask: 0u64 // NSWindowStyleMaskBorderless
            backing: 2u64 // NSBackingStoreBuffered
            defer: false];
        let _: () = msg_send![window, setReleasedWhenClosed: false];
        let _: () = msg_send![window, setIgnoresMouseEvents: true];
        let _: () = msg_send![window, setExcludedFromWindowsMenu: true];
        // Transient, stationary, ignores the window cycle.
        let _: () = msg_send![window, setCollectionBehavior: (1u64 << 3) | (1 << 4) | (1 << 6)];
        let _: () = msg_send![window, setContentView: web];
        let url: id = msg_send![class!(NSURL), URLWithString: ns(web::CARDS_PAGE_URL)];
        let request: id = msg_send![class!(NSURLRequest), requestWithURL: url];
        let _: id = msg_send![web, loadRequest: request];
        window
    };
    let made = Rc::new(Offscreen { window, web, events: RefCell::new(events), loaded: Cell::new(false), turn: Mutex::new(()) });
    SHARED.with(|s| *s.borrow_mut() = Some(made.clone()));
    Ok(made)
}

/// The rectangle to snapshot, in the web view's own coordinates.
pub fn snapshot_rect(flipped: bool, content_height: f64, x: f64, y: f64, width: f64, height: f64) -> (f64, f64, f64, f64) {
    (x, if flipped { y } else { content_height - (y + height) }, width, height)
}

/// Pixels high, for a card `width` x `height` exported 1080 wide.
pub fn export_height(width: f64, height: f64) -> i64 {
    (EXPORT_WIDTH * height / width).round() as i64
}

/// Every card, or only the one at `only`, as PNG data.
pub async fn render(input: &Input, only: Option<usize>, cx: &AsyncApp) -> Result<Vec<Vec<u8>>, String> {
    render_with(input, only, &|wait| cx.background_executor().timer(wait)).await
}

async fn render_with<F: Future<Output = ()>>(input: &Input, only: Option<usize>, sleep: &impl Fn(Duration) -> F) -> Result<Vec<Vec<u8>>, String> {
    let page = shared()?;
    let _turn = page.turn.lock().await;
    while !page.loaded.get() {
        let event = {
            let mut events = page.events.borrow_mut();
            events.next().await
        };
        match event {
            Some(WebEvent::Finished) => page.loaded.set(true),
            Some(_) => {}
            None => return Err(COULD_NOT_LAY_OUT.to_string()),
        }
    }
    let mut input = input.clone();
    input.mode = "export".to_string();
    unsafe {
        let _: () = msg_send![page.window, orderFrontRegardless];
    }
    let result = render_shown(&page, &input, only, sleep).await;
    unsafe {
        let nothing: id = nil;
        let _: () = msg_send![page.window, orderOut: nothing];
    }
    result
}

async fn render_shown<F: Future<Output = ()>>(page: &Offscreen, input: &Input, only: Option<usize>, sleep: &impl Fn(Duration) -> F) -> Result<Vec<Vec<u8>>, String> {
    let answer = web::call_async(page.web, LAYOUT_SCRIPT, &input.json()).await.map_err(|_| COULD_NOT_LAY_OUT.to_string())??;
    let value: serde_json::Value = serde_json::from_str(&answer).map_err(|_| COULD_NOT_LAY_OUT.to_string())?;
    let (Some(rects), Some(height)) = (value["rects"].as_array(), value["height"].as_f64()) else {
        return Err(COULD_NOT_LAY_OUT.to_string());
    };
    let content_height = height.ceil().max(400.);
    unsafe {
        let _: () = msg_send![page.window, setContentSize: NSSize::new(300., content_height)];
        let _: () = msg_send![page.web, setFrame: rect(0., 0., 300., content_height)];
    }
    // Let the resized page paint before the snapshots.
    sleep(Duration::from_millis(120)).await;

    let mut images = Vec::new();
    for (position, card) in rects.iter().enumerate() {
        if only.is_some_and(|i| i != position) {
            continue;
        }
        let number = |key: &str, default: f64| card[key].as_f64().unwrap_or(default);
        let (x, y, width, height) = (number("x", 0.), number("y", 0.), number("width", 300.), number("height", 400.));
        let (configuration, scale_width) = unsafe {
            let flipped: bool = msg_send![page.web, isFlipped];
            let (rx, ry, rw, rh) = snapshot_rect(flipped, content_height, x, y, width, height);
            let scale: f64 = msg_send![page.window, backingScaleFactor];
            let configuration: id = msg_send![class_named("WKSnapshotConfiguration").ok_or(COULD_NOT_DRAW)?, new];
            let _: () = msg_send![configuration, setRect: rect(rx, ry, rw, rh)];
            (configuration, EXPORT_WIDTH / scale)
        };
        unsafe {
            let number: id = msg_send![class!(NSNumber), numberWithDouble: scale_width];
            let _: () = msg_send![configuration, setSnapshotWidth: number];
        }
        let shot = web::snapshot(page.web, configuration).await;
        unsafe {
            let _: () = msg_send![configuration, release];
        }
        let image = shot.map_err(|_| COULD_NOT_DRAW.to_string())?? as id;
        let png = png_of(image, EXPORT_WIDTH as i64, export_height(width, height));
        unsafe {
            let _: () = msg_send![image, release];
        }
        images.push(png?);
    }
    Ok(images)
}

/// `image` redrawn at exactly `width` x `height` pixels, in sRGB.
fn png_of(image: id, width: i64, height: i64) -> Result<Vec<u8>, String> {
    unsafe {
        let bitmap: id = msg_send![class!(NSBitmapImageRep), alloc];
        let planes: *mut *mut u8 = std::ptr::null_mut();
        let bitmap: id = msg_send![bitmap,
            initWithBitmapDataPlanes: planes
            pixelsWide: width
            pixelsHigh: height
            bitsPerSample: 8i64
            samplesPerPixel: 4i64
            hasAlpha: true
            isPlanar: false
            colorSpaceName: ns("NSDeviceRGBColorSpace")
            bytesPerRow: 0i64
            bitsPerPixel: 0i64];
        if bitmap == nil {
            return Err(COULD_NOT_DRAW.to_string());
        }
        let _: () = msg_send![bitmap, setSize: NSSize::new(width as f64, height as f64)];
        let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
        let context: id = msg_send![class!(NSGraphicsContext), graphicsContextWithBitmapImageRep: bitmap];
        let _: () = msg_send![class!(NSGraphicsContext), setCurrentContext: context];
        let _: () = msg_send![context, setImageInterpolation: 3u64]; // NSImageInterpolationHigh
        let _: () = msg_send![image, drawInRect: rect(0., 0., width as f64, height as f64)];
        let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];
        let space: id = msg_send![class!(NSColorSpace), sRGBColorSpace];
        let srgb: id = msg_send![bitmap, bitmapImageRepByConvertingToColorSpace: space renderingIntent: 0i64];
        let data: id = if srgb != nil {
            let empty: id = msg_send![class!(NSDictionary), dictionary];
            msg_send![srgb, representationUsingType: 4u64 properties: empty] // NSBitmapImageFileTypePNG
        } else {
            nil
        };
        let out = mac::bytes_of(data);
        let _: () = msg_send![bitmap, release];
        if out.is_empty() { Err(COULD_NOT_SAVE.to_string()) } else { Ok(out) }
    }
}

/// The cards as PNG files in a folder of their own under the temporary directory,
/// named 01.png, 02.png, …
pub async fn render_files(input: &Input, name: &str, cx: &AsyncApp) -> Result<Vec<PathBuf>, String> {
    let images = render(input, None, cx).await?;
    let folder = std::env::temp_dir().join("Fumi Xiaohongshu").join(xhs::safe_name(name));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    images
        .iter()
        .enumerate()
        .map(|(i, data)| {
            let file = folder.join(xhs::card_file_name(i));
            std::fs::write(&file, data).map_err(|e| e.to_string())?;
            Ok(file)
        })
        .collect()
}

/// Copies the rendered files into a folder, over what has the same name.
pub fn copy_to_folder(files: &[PathBuf], folder: &Path) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    for file in files {
        let Some(name) = file.file_name() else { continue };
        let target = folder.join(name);
        let _ = std::fs::remove_file(&target);
        std::fs::copy(file, target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Whether this build says why it wants Photos. Without the usage string the
/// system kills the app on the request, rather than refusing it.
fn has_usage_string() -> bool {
    unsafe {
        let bundle: id = msg_send![class!(NSBundle), mainBundle];
        let value: id = msg_send![bundle, objectForInfoDictionaryKey: ns(USAGE_KEY)];
        value != nil
    }
}

/// `PHAuthorizationStatusAuthorized`, `…Limited`.
pub fn may_add(status: i64) -> bool {
    status == 3 || status == 4
}

/// Adds the cards to Photos in order, one second apart, so they sort the way they
/// read. Asks only to add: Fumi never sees the library.
pub async fn add_to_photos(files: &[PathBuf]) -> Result<(), String> {
    load_framework("Photos");
    let (Some(library), Some(request_class)) = (class_named("PHPhotoLibrary"), class_named("PHAssetCreationRequest")) else {
        return Err(PHOTOS_DENIED.to_string());
    };
    if !has_usage_string() {
        return Err(PHOTOS_DENIED.to_string());
    }
    let (send, answer) = oneshot::channel::<i64>();
    let send = Cell::new(Some(send));
    let handler = ConcreteBlock::new(move |status: i64| {
        if let Some(send) = send.take() {
            let _ = send.send(status);
        }
    })
    .copy();
    unsafe {
        // PHAccessLevelAddOnly.
        let _: () = msg_send![library, requestAuthorizationForAccessLevel: 1i64 handler: &*handler];
    }
    if !may_add(answer.await.unwrap_or(0)) {
        return Err(PHOTOS_DENIED.to_string());
    }

    let urls: Vec<usize> = files
        .iter()
        .map(|file| unsafe {
            let url: id = msg_send![mac::file_url(file), retain];
            url as usize
        })
        .collect();
    let start: usize = unsafe {
        let date: id = msg_send![class!(NSDate), date];
        let date: id = msg_send![date, retain];
        date as usize
    };
    let request_class = request_class as *const _ as usize;
    let changes = ConcreteBlock::new(move || unsafe {
        for (index, url) in urls.iter().enumerate() {
            let class = request_class as *const objc::runtime::Class;
            let request: id = msg_send![&*class, creationRequestForAsset];
            let date: id = msg_send![class!(NSDate), dateWithTimeInterval: index as f64 sinceDate: start as id];
            let _: () = msg_send![request, setCreationDate: date];
            let options: id = nil;
            // PHAssetResourceTypePhoto.
            let _: () = msg_send![request, addResourceWithType: 1i64 fileURL: *url as id options: options];
        }
    })
    .copy();
    let (send, answer) = oneshot::channel::<Result<(), String>>();
    let send = Cell::new(Some(send));
    let done = ConcreteBlock::new(move |ok: BOOL, error: id| {
        if let Some(send) = send.take() {
            let _ = send.send(if ok { Ok(()) } else { Err(mac::error_text(error)) });
        }
    })
    .copy();
    unsafe {
        let shared: id = msg_send![library, sharedPhotoLibrary];
        let _: () = msg_send![shared, performChanges: &*changes completionHandler: &*done];
    }
    answer.await.unwrap_or_else(|_| Err(PHOTOS_DENIED.to_string()))
}

/// The system share. False when AirDrop isn't available on this Mac.
pub fn air_drop(files: &[PathBuf]) -> bool {
    unsafe {
        let items: id = msg_send![class!(NSMutableArray), arrayWithCapacity: files.len() as NSUInteger];
        for file in files {
            let _: () = msg_send![items, addObject: mac::file_url(file)];
        }
        // NSSharingServiceNameSendViaAirDrop.
        let service: id = msg_send![class!(NSSharingService), sharingServiceNamed: ns("com.apple.share.AirDrop.send")];
        if service == nil {
            return false;
        }
        let can: bool = msg_send![service, canPerformWithItems: items];
        if !can {
            return false;
        }
        // It has to outlive the sending.
        let service: id = msg_send![service, retain];
        let _: () = msg_send![service, performWithItems: items];
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_follow_the_card() {
        assert_eq!(export_height(300., 400.), 1440);
        assert_eq!(export_height(300., 300.), 1080);
    }

    #[test]
    fn snapshots_read_from_the_right_edge() {
        assert_eq!(snapshot_rect(true, 1000., 0., 410., 300., 400.), (0., 410., 300., 400.));
        assert_eq!(snapshot_rect(false, 1000., 0., 410., 300., 400.), (0., 190., 300., 400.));
    }

    #[test]
    fn only_added_access_counts() {
        assert!(may_add(3));
        assert!(may_add(4));
        assert!(!may_add(0));
        assert!(!may_add(2));
    }

    /// Runs `future` while the main run loop turns, as WebKit's callbacks need.
    fn on_run_loop<T>(future: impl Future<Output = T>) -> T {
        unsafe extern "C" {
            static kCFRunLoopDefaultMode: id;
            fn CFRunLoopRunInMode(mode: id, seconds: f64, return_after_source: bool) -> i32;
        }
        let mut future = Box::pin(future);
        let mut context = std::task::Context::from_waker(futures::task::noop_waker_ref());
        let start = std::time::Instant::now();
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
            assert!(start.elapsed() < Duration::from_secs(60), "timed out");
            unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.01, false) };
        }
    }

    async fn pause(wait: Duration) {
        let start = std::time::Instant::now();
        std::future::poll_fn(|_| if start.elapsed() >= wait { std::task::Poll::Ready(()) } else { std::task::Poll::Pending }).await
    }

    /// WebKit wants the main thread and libtest doesn't run tests there, so this
    /// hook, run before `main`, does the real checks when asked to. The test below
    /// asks by running this binary again. Nothing appears on screen: the window is
    /// far off it and the process has no Dock icon.
    #[used]
    #[unsafe(link_section = "__DATA,__mod_init_func")]
    static MAIN_THREAD_HOOK: extern "C" fn() = {
        extern "C" fn hook() {
            if std::env::var_os("FUMI_WEBKIT_CHECK").is_some() {
                webkit_checks();
                std::process::exit(0);
            }
        }
        hook
    };

    /// `cargo test --lib -p fumi-publish -- --ignored`. `FUMI_CARDS_SAMPLE` names a
    /// folder to leave the PNGs in.
    #[test]
    #[ignore]
    fn webkit_on_the_main_thread() {
        let output = std::process::Command::new(std::env::current_exe().unwrap()).env("FUMI_WEBKIT_CHECK", "1").output().unwrap();
        let (out, err) = (String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success(), "{out}\n{err}");
        assert!(out.contains("webkit checks passed"), "{out}\n{err}");
    }

    fn next_event(events: &mut mpsc::UnboundedReceiver<WebEvent>) -> WebEvent {
        on_run_loop(events.next()).expect("the web view is still there")
    }

    fn sample_input() -> Input {
        let note = "# A Title\n\nThe first paragraph of the note, with **bold** words and enough text to fill a card.\n\n## A Heading\n\n- one\n- two\n\n> A quote.\n\n".repeat(6);
        Input {
            deck: xhs::deck_for(&note, "/tmp/note.md", &xhs::NoteChoices::default()),
            template: "memo".into(),
            ratio: "3:4".into(),
            page_numbers: true,
            author: xhs::Author { name: "Fumi".into(), handle: "@fumi".into(), avatar: None },
            end_card: xhs::EndCard { on: false, line: String::new() },
            accent: "#1D6FC2".into(),
            mode: "preview".into(),
        }
    }

    fn webkit_checks() {
        unsafe {
            let app: id = msg_send![class!(NSApplication), sharedApplication];
            let _: bool = msg_send![app, setActivationPolicy: 2i64]; // NSApplicationActivationPolicyProhibited
        }
        load_framework("WebKit");

        // The article: page, script messages, in-place updates, link policy.
        let config = WebConfig { handler: "wechat", opens_links: false, assets: false, forwards_to: None };
        let (web_view, delegate, sink, mut events) = web::make_web_view(&config).expect("WebKit");
        unsafe {
            let base: id = nil;
            let _: id = msg_send![web_view, loadHTMLString: ns(crate::wechat::ARTICLE_PAGE) baseURL: base];
        }
        assert_eq!(next_event(&mut events), WebEvent::Finished);
        let shown = crate::wechat::Shown { html: "<p>Body <a href='https://example.com/x' id='l'>link</a></p>".into(), title: "T".into(), is_dark: true };
        web::evaluate(web_view, &crate::wechat::update_script(None, &shown));
        let answer = on_run_loop(web::call_async(
            web_view,
            "return JSON.stringify([document.getElementById('title').textContent, document.body.className, document.getElementById('article').textContent, document.getElementById('head').className])",
            "{}",
        ));
        assert_eq!(answer.unwrap().unwrap(), r#"["T","dark","Body link",""]"#);
        web::evaluate(web_view, "document.getElementById('copy').click()");
        assert_eq!(next_event(&mut events), WebEvent::Message(serde_json::Value::String("copyTitle".into())));
        web::evaluate(web_view, "dispatchEvent(new MouseEvent('mousedown'))");
        assert_eq!(next_event(&mut events), WebEvent::Message(serde_json::Value::String("pointer".into())));
        // A clicked link is refused, so the page stays: no second load finishes.
        web::evaluate(web_view, "document.getElementById('l').click()");
        on_run_loop(pause(Duration::from_millis(300)));
        let still = on_run_loop(web::call_async(web_view, "return location.href", "{}")).unwrap().unwrap();
        assert_eq!(still, "about:blank");
        // An exception comes back as its message.
        let failed = on_run_loop(web::call_async(web_view, "throw new Error('boom')", "{}")).unwrap().unwrap_err();
        assert!(failed.contains("boom"), "{failed}");
        release_web_view_for_test(web_view, delegate, sink, "wechat");

        // The cards in the preview: fumi-xhs://, layout, the menu message.
        let config = WebConfig { handler: "xhs", opens_links: false, assets: true, forwards_to: None };
        let (web_view, delegate, sink, mut events) = web::make_web_view(&config).expect("WebKit");
        unsafe {
            let window: id = msg_send![class!(NSWindow), alloc];
            let window: id = msg_send![window, initWithContentRect: rect(-20000., -20000., 420., 800.) styleMask: 0u64 backing: 2u64 defer: false];
            let _: () = msg_send![web_view, setFrame: rect(0., 0., 420., 800.)];
            let _: () = msg_send![window, setContentView: web_view];
            let _: () = msg_send![window, orderFrontRegardless];
            let url: id = msg_send![class!(NSURL), URLWithString: ns(web::CARDS_PAGE_URL)];
            let request: id = msg_send![class!(NSURLRequest), requestWithURL: url];
            let _: id = msg_send![web_view, loadRequest: request];
        }
        assert_eq!(next_event(&mut events), WebEvent::Finished);
        let input = sample_input();
        let answer = on_run_loop(web::call_async(web_view, LAYOUT_SCRIPT, &input.json())).unwrap().unwrap();
        let value: serde_json::Value = serde_json::from_str(&answer).unwrap();
        assert!(value["count"].as_u64().unwrap() >= 2, "{answer}");
        assert!(value["height"].as_f64().unwrap() > 100., "{answer}");
        web::evaluate(
            web_view,
            "document.querySelector('.cell').dispatchEvent(new MouseEvent('contextmenu', {bubbles:true, clientX:12, clientY:34}))",
        );
        match next_event(&mut events) {
            WebEvent::Message(menu) => {
                assert_eq!(menu["type"], "menu");
                assert_eq!(menu["index"], 0);
                assert_eq!(menu["x"], 12);
                assert_eq!(menu["y"], 34);
                assert!(menu["ids"].is_array());
            }
            other => panic!("{other:?}"),
        }
        web::evaluate(web_view, "FumiXhs.menuClosed()");
        // A still for when something floats above it.
        let shot = on_run_loop(web::snapshot(web_view, nil)).unwrap().unwrap() as id;
        let size: NSSize = unsafe { msg_send![shot, size] };
        assert_eq!((size.width, size.height), (420., 800.));
        release_web_view_for_test(web_view, delegate, sink, "xhs");

        // Export: exact pixels, one at a time or all.
        let images = on_run_loop(render_with(&input, None, &|wait| pause(wait))).expect("rendered");
        assert!(images.len() >= 2, "{} cards", images.len());
        for (index, png) in images.iter().enumerate() {
            assert_eq!(&png[1..4], b"PNG");
            unsafe {
                let rep: id = msg_send![class!(NSBitmapImageRep), imageRepWithData: mac::ns_data(png)];
                let wide: i64 = msg_send![rep, pixelsWide];
                let high: i64 = msg_send![rep, pixelsHigh];
                assert_eq!((wide, high), (1080, 1440));
            }
            if let Ok(folder) = std::env::var("FUMI_CARDS_SAMPLE") {
                std::fs::create_dir_all(&folder).unwrap();
                std::fs::write(Path::new(&folder).join(xhs::card_file_name(index)), png).unwrap();
            }
        }
        let one = on_run_loop(render_with(&input, Some(1), &|wait| pause(wait))).expect("rendered one");
        assert_eq!(one.len(), 1);
        println!("webkit checks passed");
    }

    fn release_web_view_for_test(web_view: id, delegate: id, sink: *mut mpsc::UnboundedSender<WebEvent>, handler: &str) {
        web::release_web_view(web_view, delegate, sink, handler);
    }

    #[test]
    fn folder_copies_overwrite() {
        let dir = std::env::temp_dir().join(format!("fumi-publish-cards-{}", std::process::id()));
        let source = dir.join("src");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("01.png"), b"new").unwrap();
        let target = dir.join("out/deep");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("01.png"), b"old").unwrap();
        copy_to_folder(&[source.join("01.png")], &target).unwrap();
        assert_eq!(std::fs::read(target.join("01.png")).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
