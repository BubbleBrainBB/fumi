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

    /// WebKit wants the main thread: `cargo test --lib -p fumi-publish -- --ignored
    /// --test-threads=1`. `FUMI_CARDS_SAMPLE` names a folder to leave the PNGs in.
    #[test]
    #[ignore]
    fn offscreen_cards_are_1080_wide_pngs() {
        unsafe extern "C" {
            fn pthread_main_np() -> i32;
        }
        assert_eq!(unsafe { pthread_main_np() }, 1, "run with --test-threads=1");
        unsafe {
            let _: id = msg_send![class!(NSApplication), sharedApplication];
        }
        let note = "# A Title\n\nThe first paragraph of the note, with **bold** words and enough text to fill a card.\n\n## A Heading\n\n- one\n- two\n\n> A quote.\n\n".repeat(6);
        let choices = xhs::NoteChoices::default();
        let deck = xhs::deck_for(&note, "/tmp/note.md", &choices);
        let input = Input {
            deck,
            template: "memo".into(),
            ratio: "3:4".into(),
            page_numbers: true,
            author: xhs::Author { name: "Fumi".into(), handle: "@fumi".into(), avatar: None },
            end_card: xhs::EndCard { on: false, line: String::new() },
            accent: "#1D6FC2".into(),
            mode: "preview".into(),
        };
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
