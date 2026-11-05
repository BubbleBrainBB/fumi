//! Images and video files in a note: where a destination points, how big it
//! shows, and everything loaded off the main thread. Port of `ImageLoader`,
//! with ImageIO doing the reading as it does there. Remote files come through
//! `/usr/bin/curl`, into a disk cache that plays the part of `URLCache`.
//!
//! The size of an image is read from its header on first sight, so a line's
//! height is known before any pixel is decoded and never changes when they
//! arrive. Only a remote image not yet on disk has no size, and shows its source
//! meanwhile, as in Fumi.

use crate::editor::Editor;
use crate::video::{self, VideoInfo};
use cocoa::base::{id, nil};
use cocoa::foundation::NSString;
use futures::channel::oneshot;
use gpui::{App, AsyncApp, RenderImage, WeakEntity};
use objc::{class, msg_send, sel, sel_impl};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The longest side a decoded image may have, in pixels. `decodeMaxPixelSize`.
pub const DECODE_MAX_PIXELS: f32 = 1200.;
pub const IMAGE_RADIUS: f32 = 8.;
pub const CAPTION_HEIGHT: f32 = 26.;
/// From the image's foot to the caption's top.
pub const CAPTION_GAP: f32 = 7.;
const DISK_CAP: u64 = 512 * 1024 * 1024;
/// Decoded remote images kept in memory, `URLCache`'s memory capacity. What's
/// let go is decoded again from disk when it's next drawn.
const MEMORY_CAP: usize = 32 * 1024 * 1024;
/// Drawn this recently: on screen, so not let go whatever the cap says.
const IN_USE: Duration = Duration::from_secs(2);
/// How long a path that didn't exist stays unchecked.
const MISSING_RECHECK: Duration = Duration::from_secs(2);
const MAX_DECODES: usize = 4;
const MAX_FETCHES: usize = 8;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
const ACCEPT: &str = "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";

// Destinations ----------------------------------------------------------------

/// `removingPercentEncoding`: `None` when a `%` starts no escape or the bytes
/// aren't UTF-8.
fn percent_decoded(s: &str) -> Option<String> {
    if !s.contains('%') {
        return Some(s.to_string());
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn unwrap_angle(path: &str) -> &str {
    let t = path.trim();
    t.strip_prefix('<').and_then(|t| t.strip_suffix('>')).unwrap_or(t)
}

/// `cleanPath`: trimmed, `<…>` stripped, percent-decoded.
pub fn clean_path(path: &str) -> String {
    let value = unwrap_angle(path);
    percent_decoded(value).unwrap_or_else(|| value.to_string())
}

/// `normalizedKey`: what a remote image is known by. Pasted URLs bring `&amp;`
/// and stray spaces with them.
pub fn normalized_key(path: &str) -> String {
    let mut value = unwrap_angle(path).replace("&amp;", "&").replace('\u{a0}', "");
    let lower = value.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        value.retain(|c| !c.is_whitespace());
    }
    value
}

pub fn is_remote(path: &str) -> bool {
    let value = normalized_key(path).to_lowercase();
    value.starts_with("http://") || value.starts_with("https://")
}

/// `makeRemoteURL`: leaves a valid URL alone and percent-encodes what isn't.
pub fn encode_url(url: &str) -> String {
    let allowed = |b: u8| b.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&'()*+,;=%".contains(&b);
    let mut out = String::with_capacity(url.len());
    for b in url.bytes() {
        if allowed(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    host.split(':').next().unwrap_or("").to_lowercase()
}

/// `resolveLocalPath`: a `file://` URL; absolute; `~`; relative to the note's
/// folder if it's there; else relative to the library.
pub fn resolve_local_path(raw: &str, base: Option<&Path>, workspace: Option<&Path>, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    let path = clean_path(raw);
    if path.to_lowercase().starts_with("file://") {
        let rest = &path[7..];
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        return Some(PathBuf::from(rest));
    }
    if path.starts_with('/') {
        return Some(PathBuf::from(path));
    }
    if path == "~" || path.starts_with("~/") {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        return Some(home.join(path.trim_start_matches('~').trim_start_matches('/')));
    }
    if let Some(base) = base {
        let candidate = base.join(&path);
        if exists(&candidate) {
            return Some(candidate);
        }
    }
    workspace.map(|w| w.join(&path))
}

/// `![[a.png|300]]` and `![[a.png#x]]` are `a.png`.
pub fn embed_name(target: &str) -> Option<&str> {
    let name = target.split(['|', '#']).next().unwrap_or("");
    let name = name.trim_matches(' ');
    (!name.is_empty()).then_some(name)
}

pub fn is_video_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_lowercase().as_str(), "mp4" | "mov" | "m4v" | "webm"))
}

/// File name, lowercased, to file, for `![[name]]`: hidden files, packages and
/// notes are left out, and the shallowest of several wins.
pub fn build_embed_index(root: &Path) -> HashMap<String, PathBuf> {
    let mut index: HashMap<String, (usize, PathBuf)> = HashMap::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if kind.is_dir() {
                let package = matches!(
                    ext.as_str(),
                    "app" | "bundle" | "framework" | "plugin" | "kext" | "pkg" | "rtfd" | "key" | "pages" | "numbers" | "photoslibrary" | "xcodeproj" | "xcworkspace"
                );
                if !package {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if ext == "md" {
                continue;
            }
            let key = name.to_lowercase();
            match index.get(&key) {
                Some((d, _)) if *d <= depth => {}
                _ => {
                    index.insert(key, (depth, path));
                }
            }
        }
    }
    index.into_iter().map(|(k, (_, p))| (k, p)).collect()
}

// Sizes -----------------------------------------------------------------------

/// The size a decoded image comes out at: its longest side at most 1200 pixels.
/// That pixel size is its size in points.
pub fn downsampled_size(width: f32, height: f32) -> (f32, f32) {
    let long = width.max(height);
    if long <= DECODE_MAX_PIXELS || long <= 0. {
        return (width, height);
    }
    let scale = DECODE_MAX_PIXELS / long;
    ((width * scale).round().max(1.), (height * scale).round().max(1.))
}

/// `displaySize`: capped to the column, keeping the aspect.
pub fn display_size(natural: (f32, f32), max_width: f32) -> (f32, f32) {
    let (w, h) = natural;
    if w <= 0. || h <= 0. {
        return natural;
    }
    let width = w.min(max_width);
    (width, width * (h / w))
}

/// A card's video area: the column wide, as tall as the poster's aspect, capped
/// at 1.25, or 9/16 until there's a poster.
pub fn video_size(poster: Option<(f32, f32)>, max_width: f32) -> (f32, f32) {
    let aspect = match poster {
        Some((w, h)) if w > 0. && h > 0. => (h / w).min(1.25),
        _ => 9. / 16.,
    };
    let width = max_width.max(1.);
    (width, (width * aspect).round())
}

/// `aspectFit`: the rectangle `size` takes inside `area`, relative to its corner.
pub fn aspect_fit(size: (f32, f32), area: (f32, f32)) -> (f32, f32, f32, f32) {
    let (w, h) = size;
    if w <= 0. || h <= 0. {
        return (0., 0., area.0, area.1);
    }
    let scale = (area.0 / w).min(area.1 / h);
    let (fit_w, fit_h) = (w * scale, h * scale);
    ((area.0 - fit_w) / 2., (area.1 - fit_h) / 2., fit_w, fit_h)
}

/// `m:ss`, or `h:mm:ss` over an hour.
pub fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.).round() as u64;
    if total >= 3600 {
        format!("{}:{:02}:{:02}", total / 3600, total % 3600 / 60, total % 60)
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

// What a line asks for --------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum MediaKind {
    Image { animated: bool },
    Video { path: PathBuf },
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaInfo {
    /// What the frames are stored under.
    pub key: Rc<str>,
    pub kind: MediaKind,
    /// In points; for a video, the poster's size if it has one.
    pub natural: Option<(f32, f32)>,
    /// A video whose poster is still coming, and its height may change with it.
    pub settling: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lookup {
    /// Nothing there: the source shows.
    Missing,
    /// Not yet known: the source shows, and the line changes when it is.
    Pending,
    Ready(MediaInfo),
}

pub trait MediaLookup {
    fn lookup(&self, dest: &str, embed: bool) -> Lookup;
}

/// Where a note's relative paths lead from.
pub struct MediaContext {
    pub base: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
}

impl MediaLookup for MediaContext {
    fn lookup(&self, dest: &str, embed: bool) -> Lookup {
        with_store(|s| s.lookup(self, dest, embed))
    }
}

/// An image or video standing in a line.
#[derive(Clone, Debug)]
pub struct InlineMedia {
    pub info: MediaInfo,
    pub caption: String,
    /// Shown under its source: it takes a row for itself.
    pub fills_line: bool,
    /// The source's bytes in the line.
    pub src: std::ops::Range<usize>,
}

/// How a box divides up, for a column `max_width` wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediaGeom {
    pub box_width: f32,
    /// Everything: the picture and the caption's room.
    pub box_height: f32,
    pub width: f32,
    pub height: f32,
}

impl InlineMedia {
    pub fn has_caption(&self) -> bool {
        match self.info.kind {
            MediaKind::Image { animated } => !animated && !self.caption.trim().is_empty(),
            MediaKind::Video { .. } => true,
        }
    }

    pub fn geom(&self, max_width: f32) -> MediaGeom {
        let (width, height) = match self.info.kind {
            MediaKind::Image { .. } => display_size(self.info.natural.unwrap_or((1., 1.)), max_width),
            MediaKind::Video { .. } => video_size(self.info.natural, max_width),
        };
        let caption = if self.has_caption() { CAPTION_HEIGHT } else { 0. };
        MediaGeom {
            box_width: if self.fills_line { width.max(max_width) } else { width },
            box_height: height + caption,
            width,
            height,
        }
    }
}

// Failures and retries --------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FetchFailure {
    /// curl's exit code.
    Transport(i32),
    Http(u32),
    /// Arrived, and isn't an image.
    Undecodable,
    BadUrl,
}

/// When to ask again: soon after a dropped connection, later after a busy
/// server, and not for five minutes after anything else.
pub fn retry_delay(failure: FetchFailure) -> Duration {
    let seconds = match failure {
        // Timed out, couldn't resolve, couldn't connect, connection lost.
        FetchFailure::Transport(5 | 6 | 7 | 18 | 28 | 52 | 55 | 56) => 15,
        FetchFailure::Http(408 | 429 | 500..=599) => 30,
        _ => 300,
    };
    Duration::from_secs(seconds)
}

/// Everything curl is told. 30 s between bytes, 120 s in all.
pub fn curl_args(url: &str, out: &Path, proxy: Option<&ProxySettings>) -> Vec<String> {
    let mut args: Vec<String> = [
        "--silent", "--location", "--globoff",
        "--proto", "=http,https", "--proto-redir", "=http,https",
        "--compressed",
        "--connect-timeout", "30",
        "--speed-time", "30", "--speed-limit", "1",
        "--max-time", "120",
        "--write-out", "%{http_code}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.extend(["--user-agent".into(), USER_AGENT.into()]);
    args.extend(["--header".into(), format!("Accept: {ACCEPT}")]);
    // Feishu and Lark want their own Referer; other image hosts may refuse it.
    let host = host_of(url);
    if ["feishu", "larksuite", "larkoffice"].iter().any(|h| host.contains(h)) {
        args.extend(["--referer".into(), "https://www.feishu.cn/".into()]);
    }
    if let Some(p) = proxy {
        args.extend([p.flag.into(), p.address.clone()]);
        if !p.exceptions.is_empty() {
            args.extend(["--noproxy".into(), p.exceptions.clone()]);
        }
    }
    args.extend(["--output".into(), out.to_string_lossy().into_owned()]);
    args.extend(["--url".into(), url.to_string()]);
    args
}

/// The system's proxy, which URLSession would use and curl doesn't look at.
pub struct ProxySettings {
    flag: &'static str,
    address: String,
    exceptions: String,
}

#[link(name = "CFNetwork", kind = "framework")]
unsafe extern "C" {
    fn CFNetworkCopySystemProxySettings() -> *const c_void;
}

fn system_proxy() -> Option<ProxySettings> {
    unsafe {
        let dict = CFNetworkCopySystemProxySettings() as id;
        if dict == nil {
            return None;
        }
        let read = |key: &str| -> Option<String> {
            let value: id = msg_send![dict, objectForKey: ns(key)];
            (value != nil).then(|| {
                let text: id = msg_send![value, description];
                from_ns(text)
            })
        };
        let enabled = |key: &str| read(key).is_some_and(|v| v != "0");
        let found = if enabled("HTTPSEnable") {
            read("HTTPSProxy").map(|h| ("--proxy", h, read("HTTPSPort")))
        } else if enabled("HTTPEnable") {
            read("HTTPProxy").map(|h| ("--proxy", h, read("HTTPPort")))
        } else if enabled("SOCKSEnable") {
            read("SOCKSProxy").map(|h| ("--socks5-hostname", h, read("SOCKSPort")))
        } else {
            None
        };
        let exceptions: id = msg_send![dict, objectForKey: ns("ExceptionsList")];
        let exceptions = if exceptions == nil {
            String::new()
        } else {
            let joined: id = msg_send![exceptions, componentsJoinedByString: ns(",")];
            from_ns(joined)
        };
        let settings = found.map(|(flag, host, port)| ProxySettings {
            flag,
            address: match port {
                Some(port) => format!("{host}:{port}"),
                None => host,
            },
            exceptions,
        });
        let _: () = msg_send![dict, release];
        settings
    }
}

fn ns(s: &str) -> id {
    unsafe {
        let string = NSString::alloc(nil).init_str(s);
        msg_send![string, autorelease]
    }
}

fn from_ns(string: id) -> String {
    unsafe {
        if string == nil {
            return String::new();
        }
        let bytes = NSString::UTF8String(string);
        if bytes.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(bytes).to_string_lossy().into_owned()
    }
}

/// Runs `f` with its own autorelease pool: worker threads have none.
pub fn with_pool<R>(f: impl FnOnce() -> R) -> R {
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let result = f();
        let _: () = msg_send![pool, drain];
        result
    }
}

// ImageIO ---------------------------------------------------------------------

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    static kCGImageSourceCreateThumbnailFromImageAlways: id;
    static kCGImageSourceCreateThumbnailWithTransform: id;
    static kCGImageSourceShouldCacheImmediately: id;
    static kCGImageSourceThumbnailMaxPixelSize: id;
    static kCGImagePropertyPixelWidth: id;
    static kCGImagePropertyPixelHeight: id;
    static kCGImagePropertyOrientation: id;
    static kCGImagePropertyGIFDictionary: id;
    static kCGImagePropertyGIFUnclampedDelayTime: id;
    static kCGImagePropertyGIFDelayTime: id;
    fn CGImageSourceCreateWithURL(url: id, options: id) -> *mut c_void;
    fn CGImageSourceGetCount(source: *mut c_void) -> usize;
    fn CGImageSourceCopyPropertiesAtIndex(source: *mut c_void, index: usize, options: id) -> id;
    fn CGImageSourceCreateThumbnailAtIndex(source: *mut c_void, index: usize, options: id) -> *mut c_void;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    static kCGColorSpaceSRGB: id;
    fn CGColorSpaceCreateWithName(name: id) -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGImageGetWidth(image: *mut c_void) -> usize;
    fn CGImageGetHeight(image: *mut c_void) -> usize;
    fn CGImageRelease(image: *mut c_void);
    fn CGBitmapContextCreate(data: *mut c_void, width: usize, height: usize, bits: usize, row: usize, space: *mut c_void, info: u32) -> *mut c_void;
    fn CGContextDrawImage(context: *mut c_void, rect: cocoa::foundation::NSRect, image: *mut c_void);
    fn CGContextRelease(context: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
}

fn file_url(path: &Path) -> id {
    unsafe { msg_send![class!(NSURL), fileURLWithPath: ns(&path.to_string_lossy())] }
}

/// What a file's header says, without decoding anything.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    /// Size in points, once downsampled and turned upright.
    pub natural: (f32, f32),
    pub animated: bool,
}

/// `None` for what isn't an image.
pub fn probe_image(path: &Path) -> Option<Probe> {
    with_pool(|| unsafe {
        let source = CGImageSourceCreateWithURL(file_url(path), nil);
        if source.is_null() {
            return None;
        }
        let count = CGImageSourceGetCount(source);
        let props = if count > 0 { CGImageSourceCopyPropertiesAtIndex(source, 0, nil) } else { nil };
        let probe = (props != nil)
            .then(|| {
                let number = |key: id| -> i64 {
                    let value: id = msg_send![props, objectForKey: key];
                    if value == nil { 0 } else { msg_send![value, longLongValue] }
                };
                let (mut w, mut h) = (number(kCGImagePropertyPixelWidth), number(kCGImagePropertyPixelHeight));
                // Orientations 5 to 8 stand the picture on its side.
                if number(kCGImagePropertyOrientation) >= 5 {
                    std::mem::swap(&mut w, &mut h);
                }
                let _: () = msg_send![props, release];
                (w > 0 && h > 0).then(|| Probe {
                    natural: downsampled_size(w as f32, h as f32),
                    animated: count > 1,
                })
            })
            .flatten();
        CFRelease(source);
        probe
    })
}

pub struct Decoded {
    pub image: Arc<RenderImage>,
    /// Each frame's time on screen; one entry for a still image.
    pub delays: Vec<f32>,
    pub total: f32,
    pub started: Instant,
}

/// `frameDuration`: under 0.02 s or missing is 0.1 s.
pub fn gif_delay(unclamped: Option<f64>, clamped: Option<f64>) -> f32 {
    let delay = unclamped.or(clamped).unwrap_or(0.1);
    if delay < 0.02 { 0.1 } else { delay as f32 }
}

/// Which frame shows `elapsed` seconds in, and for how much longer.
pub fn frame_at(delays: &[f32], total: f32, elapsed: f32) -> (usize, f32) {
    if delays.len() < 2 || total <= 0. {
        return (0, f32::MAX);
    }
    let mut t = elapsed.rem_euclid(total);
    for (i, delay) in delays.iter().enumerate() {
        if t < *delay {
            return (i, *delay - t);
        }
        t -= *delay;
    }
    (0, delays[0])
}

/// A CGImage's pixels, as BGRA with straight alpha, the way GPUI wants them.
pub fn bgra_of(image: *mut c_void) -> Option<image::Frame> {
    unsafe {
        let (w, h) = (CGImageGetWidth(image), CGImageGetHeight(image));
        if w == 0 || h == 0 {
            return None;
        }
        let mut pixels = vec![0u8; w * h * 4];
        let space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        // kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little: B, G, R, A.
        let context = CGBitmapContextCreate(pixels.as_mut_ptr().cast(), w, h, 8, w * 4, space, 2 | (2 << 12));
        CGColorSpaceRelease(space);
        if context.is_null() {
            return None;
        }
        let rect = cocoa::foundation::NSRect::new(
            cocoa::foundation::NSPoint::new(0., 0.),
            cocoa::foundation::NSSize::new(w as f64, h as f64),
        );
        CGContextDrawImage(context, rect, image);
        CGContextRelease(context);
        for px in pixels.chunks_exact_mut(4) {
            let a = px[3] as u32;
            if a != 0 && a != 255 {
                px[0] = ((px[0] as u32 * 255 + a / 2) / a).min(255) as u8;
                px[1] = ((px[1] as u32 * 255 + a / 2) / a).min(255) as u8;
                px[2] = ((px[2] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
        let buffer = image::RgbaImage::from_raw(w as u32, h as u32, pixels)?;
        Some(image::Frame::new(buffer))
    }
}

/// Decodes and downsamples, in one ImageIO pass. Off the main thread.
pub fn decode_image(path: &Path) -> Option<Decoded> {
    with_pool(|| unsafe {
        let source = CGImageSourceCreateWithURL(file_url(path), nil);
        if source.is_null() {
            return None;
        }
        let options: id = msg_send![class!(NSMutableDictionary), new];
        let yes: id = msg_send![class!(NSNumber), numberWithBool: true as objc::runtime::BOOL];
        let _: () = msg_send![options, setObject: yes forKey: kCGImageSourceCreateThumbnailFromImageAlways];
        let _: () = msg_send![options, setObject: yes forKey: kCGImageSourceCreateThumbnailWithTransform];
        let _: () = msg_send![options, setObject: yes forKey: kCGImageSourceShouldCacheImmediately];
        let longest: id = msg_send![class!(NSNumber), numberWithInt: DECODE_MAX_PIXELS as i32];
        let _: () = msg_send![options, setObject: longest forKey: kCGImageSourceThumbnailMaxPixelSize];

        let count = CGImageSourceGetCount(source);
        let mut frames: smallvec::SmallVec<[image::Frame; 1]> = smallvec::SmallVec::new();
        let mut delays = Vec::with_capacity(count);
        for index in 0..count {
            let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, index, options);
            if thumbnail.is_null() {
                continue;
            }
            let frame = bgra_of(thumbnail);
            CGImageRelease(thumbnail);
            let Some(frame) = frame else { continue };
            frames.push(frame);
            delays.push(if count > 1 { frame_delay(source, index) } else { 0. });
        }
        let _: () = msg_send![options, release];
        CFRelease(source);
        if frames.is_empty() {
            return None;
        }
        let total = delays.iter().sum();
        Some(Decoded { image: Arc::new(RenderImage::new(frames)), delays, total, started: Instant::now() })
    })
}

unsafe fn frame_delay(source: *mut c_void, index: usize) -> f32 {
    unsafe {
        let props = CGImageSourceCopyPropertiesAtIndex(source, index, nil);
        if props == nil {
            return 0.1;
        }
        let gif: id = msg_send![props, objectForKey: kCGImagePropertyGIFDictionary];
        let read = |key: id| -> Option<f64> {
            if gif == nil {
                return None;
            }
            let value: id = msg_send![gif, objectForKey: key];
            if value == nil { None } else { Some(msg_send![value, doubleValue]) }
        };
        let delay = gif_delay(read(kCGImagePropertyGIFUnclampedDelayTime), read(kCGImagePropertyGIFDelayTime));
        let _: () = msg_send![props, release];
        delay
    }
}

// The disk cache --------------------------------------------------------------

fn bundle_id() -> String {
    unsafe {
        let bundle: id = msg_send![class!(NSBundle), mainBundle];
        let name: id = msg_send![bundle, bundleIdentifier];
        if name == nil { "com.fumi.gpui-prototype".to_string() } else { from_ns(name) }
    }
}

/// `~/Library/Caches/<bundle id>/FumiImageCache`.
pub fn cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join("Library/Caches").join(bundle_id()).join("FumiImageCache")
}

pub fn cache_file(dir: &Path, key: &str) -> PathBuf {
    let hash = Sha256::digest(key.as_bytes());
    let name: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    dir.join(name)
}

/// Drops the longest-unused files until the folder is under `cap` bytes.
pub fn trim_cache(dir: &Path, cap: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            meta.is_file().then(|| (meta.modified().unwrap_or(std::time::UNIX_EPOCH), meta.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= cap {
        return;
    }
    files.sort();
    for (_, len, path) in files {
        if total <= cap {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

fn touch(path: &Path) {
    if let Ok(c) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) {
        unsafe { libc::utimes(c.as_ptr(), std::ptr::null()) };
    }
}

/// Downloads to the cache. On the worker's thread, however long it takes.
fn fetch(key: &str, dir: &Path) -> Result<PathBuf, FetchFailure> {
    if key.contains(['\n', '\r', '\0']) {
        return Err(FetchFailure::BadUrl);
    }
    std::fs::create_dir_all(dir).map_err(|_| FetchFailure::Transport(23))?;
    let target = cache_file(dir, key);
    let temp = target.with_extension(format!("part{}", std::process::id()));
    let url = encode_url(key);
    let proxy = with_pool(system_proxy);
    let output = std::process::Command::new("/usr/bin/curl").args(curl_args(&url, &temp, proxy.as_ref())).output();
    let result = match output {
        Err(_) => Err(FetchFailure::Transport(-1)),
        Ok(out) => {
            let code = out.status.code().unwrap_or(-1);
            let status: u32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0);
            if code != 0 {
                Err(FetchFailure::Transport(code))
            } else if !(200..300).contains(&status) {
                Err(FetchFailure::Http(status))
            } else {
                Ok(())
            }
        }
    };
    match result {
        Ok(()) => {
            std::fs::rename(&temp, &target).map_err(|_| FetchFailure::Transport(23))?;
            trim_cache(dir, DISK_CAP);
            Ok(target)
        }
        Err(failure) => {
            let _ = std::fs::remove_file(&temp);
            Err(failure)
        }
    }
}

// The store -------------------------------------------------------------------

enum Frames {
    Waiting,
    Decoding,
    Ready(Rc<Decoded>),
    Failed,
}

struct ImageEntry {
    path: PathBuf,
    probe: Probe,
    frames: Frames,
    /// Came off the network: its pixels count against `MEMORY_CAP`.
    remote: bool,
    /// When its pixels were last asked for.
    used: Instant,
}

enum Remote {
    Loading,
    Failed { until: Instant },
    Done,
}

enum Index {
    Building,
    Ready(HashMap<String, PathBuf>),
}

enum Resolved {
    Found(PathBuf),
    /// Checked at this moment, and not there.
    Absent(Instant),
}

enum Job {
    Decode { key: String, path: PathBuf },
    Fetch { key: String, dir: PathBuf },
    Poster { path: PathBuf },
    Index { root: PathBuf },
}

enum Done {
    Decoded { key: String, decoded: Option<Decoded> },
    Fetched { key: String, result: Result<(PathBuf, Probe, Option<Decoded>), FetchFailure> },
    Poster { path: PathBuf, info: VideoInfo },
    Index { root: PathBuf, map: HashMap<String, PathBuf> },
}

impl Job {
    fn is_fetch(&self) -> bool {
        matches!(self, Job::Fetch { .. })
    }

    fn run(self) -> Done {
        match self {
            Job::Decode { key, path } => Done::Decoded { key, decoded: decode_image(&path) },
            Job::Fetch { key, dir } => {
                let result = fetch(&key, &dir).and_then(|path| match probe_image(&path) {
                    Some(probe) => Ok((path.clone(), probe, decode_image(&path))),
                    None => {
                        let _ = std::fs::remove_file(&path);
                        Err(FetchFailure::Undecodable)
                    }
                });
                Done::Fetched { key, result }
            }
            Job::Poster { path } => {
                let info = video::read_video_info(&path);
                Done::Poster { path, info }
            }
            Job::Index { root } => {
                let map = build_embed_index(&root);
                Done::Index { root, map }
            }
        }
    }
}

/// One to a process, like `ImageLoader.shared`: tabs on the same picture share
/// its frames.
#[derive(Default)]
pub struct MediaStore {
    resolved: HashMap<String, Resolved>,
    images: HashMap<String, ImageEntry>,
    /// Paths that turned out not to be images, when they were looked at.
    not_images: HashMap<String, Instant>,
    remotes: HashMap<String, Remote>,
    videos: HashMap<PathBuf, Option<Rc<VideoInfo>>>,
    indexes: HashMap<PathBuf, Index>,
    queue: Vec<Job>,
    decoding: usize,
    fetching: usize,
    listeners: Vec<WeakEntity<Editor>>,
}

thread_local! {
    static STORE: RefCell<MediaStore> = RefCell::new(MediaStore::default());
}

pub fn with_store<R>(f: impl FnOnce(&mut MediaStore) -> R) -> R {
    STORE.with(|s| f(&mut s.borrow_mut()))
}

impl MediaStore {
    pub fn add_listener(&mut self, editor: WeakEntity<Editor>) {
        self.listeners.retain(|l| l.is_upgradable());
        self.listeners.push(editor);
    }

    pub fn has_queued(&self) -> bool {
        !self.queue.is_empty()
    }

    pub fn lookup(&mut self, cx: &MediaContext, dest: &str, embed: bool) -> Lookup {
        if !embed && is_remote(dest) {
            let key = normalized_key(dest);
            // Remote video isn't shown at all, so it isn't worth downloading.
            return if is_video_name(&key) { Lookup::Missing } else { self.remote(key) };
        }
        let wants_video = embed || is_video_name(dest);
        let name = if embed { embed_name(dest) } else { Some(dest) };
        let Some(name) = name else { return Lookup::Missing };
        let path = self.resolve(cx, name, wants_video);
        let path = match path {
            Err(lookup) => return lookup,
            Ok(path) => path,
        };
        if wants_video && is_video_name(&path.to_string_lossy()) {
            return self.video(path);
        }
        self.local_image(path)
    }

    /// The file behind a path, by `resolveEmbed` when `search` says so.
    fn resolve(&mut self, cx: &MediaContext, name: &str, search: bool) -> Result<PathBuf, Lookup> {
        let key = format!(
            "{}\0{}\0{}\0{name}",
            cx.base.as_deref().map_or(std::borrow::Cow::Borrowed(""), Path::to_string_lossy),
            cx.workspace.as_deref().map_or(std::borrow::Cow::Borrowed(""), Path::to_string_lossy),
            search
        );
        match self.resolved.get(&key) {
            Some(Resolved::Found(path)) => return Ok(path.clone()),
            Some(Resolved::Absent(at)) if at.elapsed() < MISSING_RECHECK => return Err(Lookup::Missing),
            _ => {}
        }
        let exists = |p: &Path| p.exists();
        let direct = resolve_local_path(name, cx.base.as_deref(), cx.workspace.as_deref(), &exists);
        if let Some(path) = direct.filter(|p| p.exists()) {
            self.resolved.insert(key, Resolved::Found(path.clone()));
            return Ok(path);
        }
        if search && let Some(root) = &cx.workspace {
            match self.indexes.get(root) {
                None => {
                    self.indexes.insert(root.clone(), Index::Building);
                    self.queue.push(Job::Index { root: root.clone() });
                    return Err(Lookup::Pending);
                }
                Some(Index::Building) => return Err(Lookup::Pending),
                Some(Index::Ready(map)) => {
                    let file = Path::new(name).file_name().map(|n| n.to_string_lossy().to_lowercase());
                    if let Some(path) = file.and_then(|f| map.get(&f)) {
                        self.resolved.insert(key, Resolved::Found(path.clone()));
                        return Ok(path.clone());
                    }
                }
            }
        }
        self.resolved.insert(key, Resolved::Absent(Instant::now()));
        Err(Lookup::Missing)
    }

    fn local_image(&mut self, path: PathBuf) -> Lookup {
        let key = path.to_string_lossy().into_owned();
        if let Some(entry) = self.images.get(&key) {
            return entry.lookup(&key);
        }
        if let Some(at) = self.not_images.get(&key)
            && at.elapsed() < MISSING_RECHECK
        {
            return Lookup::Missing;
        }
        let Some(probe) = probe_image(&path) else {
            self.not_images.insert(key, Instant::now());
            return Lookup::Missing;
        };
        self.not_images.remove(&key);
        let entry = ImageEntry { path, probe, frames: Frames::Waiting, remote: false, used: Instant::now() };
        let lookup = entry.lookup(&key);
        self.images.insert(key, entry);
        lookup
    }

    fn remote(&mut self, key: String) -> Lookup {
        if let Some(entry) = self.images.get(&key) {
            return entry.lookup(&key);
        }
        match self.remotes.get(&key) {
            Some(Remote::Loading) | Some(Remote::Done) => return Lookup::Pending,
            Some(Remote::Failed { until }) if Instant::now() < *until => return Lookup::Pending,
            _ => {}
        }
        // Cache preferred: what's on disk is used without asking the network.
        let dir = cache_dir();
        let file = cache_file(&dir, &key);
        if file.exists() {
            if let Some(probe) = probe_image(&file) {
                touch(&file);
                let entry = ImageEntry { path: file, probe, frames: Frames::Waiting, remote: true, used: Instant::now() };
                let lookup = entry.lookup(&key);
                self.remotes.insert(key.clone(), Remote::Done);
                self.images.insert(key, entry);
                return lookup;
            }
            let _ = std::fs::remove_file(&file);
        }
        self.remotes.insert(key.clone(), Remote::Loading);
        self.queue.push(Job::Fetch { key, dir });
        Lookup::Pending
    }

    fn video(&mut self, path: PathBuf) -> Lookup {
        let key: Rc<str> = path.to_string_lossy().into_owned().into();
        let known = match self.videos.get(&path) {
            Some(known) => known.clone(),
            None => {
                self.videos.insert(path.clone(), None);
                self.queue.push(Job::Poster { path: path.clone() });
                None
            }
        };
        Lookup::Ready(MediaInfo {
            key,
            natural: known.as_ref().and_then(|i| i.poster_size),
            settling: known.is_none(),
            kind: MediaKind::Video { path },
        })
    }

    pub fn video_info(&self, path: &Path) -> Option<Rc<VideoInfo>> {
        self.videos.get(path).cloned().flatten()
    }

    /// The frame to draw now and how long it stays, starting the decode the
    /// first time it's asked for. `None` until there are pixels.
    pub fn frame(&mut self, key: &str) -> Option<(Arc<RenderImage>, usize, f32)> {
        let entry = self.images.get_mut(key)?;
        entry.used = Instant::now();
        match &entry.frames {
            Frames::Waiting => {
                entry.frames = Frames::Decoding;
                let path = entry.path.clone();
                self.queue.push(Job::Decode { key: key.to_string(), path });
                None
            }
            Frames::Ready(decoded) => {
                let (index, left) = frame_at(&decoded.delays, decoded.total, decoded.started.elapsed().as_secs_f32());
                Some((decoded.image.clone(), index, left))
            }
            _ => None,
        }
    }

    /// The most recently wanted first, so what's on screen goes ahead of what
    /// was scrolled past.
    fn next_jobs(&mut self) -> Vec<Job> {
        let mut out = Vec::new();
        let mut i = self.queue.len();
        while i > 0 {
            i -= 1;
            let fetch = self.queue[i].is_fetch();
            let room = if fetch { self.fetching < MAX_FETCHES } else { self.decoding < MAX_DECODES };
            if room {
                if fetch {
                    self.fetching += 1;
                } else {
                    self.decoding += 1;
                }
                out.push(self.queue.remove(i));
            }
        }
        out
    }

    /// Takes in a result. Returns when to wake up and try a failed one again.
    fn finish(&mut self, done: Done) -> Option<Duration> {
        let mut retry = None;
        match done {
            Done::Decoded { key, decoded } => {
                self.decoding -= 1;
                if let Some(entry) = self.images.get_mut(&key) {
                    entry.frames = match decoded {
                        Some(decoded) => Frames::Ready(Rc::new(decoded)),
                        None => Frames::Failed,
                    };
                }
            }
            Done::Fetched { key, result } => {
                self.fetching -= 1;
                match result {
                    Ok((path, probe, decoded)) => {
                        let frames = decoded.map_or(Frames::Failed, |d| Frames::Ready(Rc::new(d)));
                        let used = Instant::now();
                        self.images.insert(key.clone(), ImageEntry { path, probe, frames, remote: true, used });
                        self.remotes.insert(key, Remote::Done);
                    }
                    Err(failure) => {
                        let delay = retry_delay(failure);
                        retry = Some(delay);
                        self.remotes.insert(key, Remote::Failed { until: Instant::now() + delay });
                    }
                }
            }
            Done::Poster { path, info } => {
                self.decoding -= 1;
                self.videos.insert(path, Some(Rc::new(info)));
            }
            Done::Index { root, map } => {
                self.decoding -= 1;
                self.indexes.insert(root, Index::Ready(map));
                self.resolved.retain(|_, r| matches!(r, Resolved::Found(_)));
            }
        }
        retry
    }

    /// Lets go of the least recently drawn remote images until what's held fits
    /// the cap, never one still on screen. Returns their pixels, for the windows
    /// to drop too.
    fn trim_memory(&mut self) -> Vec<Arc<RenderImage>> {
        let mut held: Vec<(Instant, usize, &String)> = self
            .images
            .iter()
            .filter(|(_, e)| e.remote)
            .filter_map(|(k, e)| match &e.frames {
                Frames::Ready(d) => Some((e.used, decoded_bytes(d), k)),
                _ => None,
            })
            .collect();
        let now = Instant::now();
        let evict: Vec<String> = over_cap(&mut held, now).into_iter().cloned().collect();
        evict
            .into_iter()
            .filter_map(|k| {
                let entry = self.images.get_mut(&k)?;
                match std::mem::replace(&mut entry.frames, Frames::Waiting) {
                    Frames::Ready(d) => Some(d.image.clone()),
                    _ => None,
                }
            })
            .collect()
    }

    fn listeners(&mut self) -> Vec<WeakEntity<Editor>> {
        self.listeners.retain(|l| l.is_upgradable());
        self.listeners.clone()
    }
}

fn decoded_bytes(d: &Decoded) -> usize {
    (0..d.delays.len().max(1)).map(|i| d.image.as_bytes(i).map_or(0, <[u8]>::len)).sum()
}

/// Which of `held` (last drawn, bytes, key) go, oldest first, to get under
/// `MEMORY_CAP`, sparing what was drawn within `IN_USE` of `now`.
fn over_cap<'a, K>(held: &mut [(Instant, usize, &'a K)], now: Instant) -> Vec<&'a K> {
    let mut total: usize = held.iter().map(|h| h.1).sum();
    held.sort_by_key(|h| h.0);
    let mut out = Vec::new();
    for (used, bytes, key) in held.iter() {
        if total <= MEMORY_CAP {
            break;
        }
        if now.duration_since(*used) < IN_USE {
            continue;
        }
        total -= bytes;
        out.push(*key);
    }
    out
}

impl ImageEntry {
    fn lookup(&self, key: &str) -> Lookup {
        if matches!(self.frames, Frames::Failed) {
            return Lookup::Missing;
        }
        Lookup::Ready(MediaInfo {
            key: key.into(),
            kind: MediaKind::Image { animated: self.probe.animated },
            natural: Some(self.probe.natural),
            settling: false,
        })
    }
}

fn tell_listeners(cx: &mut AsyncApp) {
    for editor in with_store(|s| s.listeners()) {
        let _ = editor.update(cx, |e, cx| e.media_changed(cx));
    }
}

/// Starts whatever's queued. Each job has its own thread, since a download may
/// sit there for two minutes; the store's counters keep the number sensible.
pub fn pump(cx: &mut App) {
    for job in with_store(|s| s.next_jobs()) {
        let (tx, rx) = oneshot::channel();
        let spawned = std::thread::Builder::new().name("fumi-media".into()).spawn(move || {
            let _ = tx.send(job.run());
        });
        if spawned.is_err() {
            continue;
        }
        cx.spawn(async move |cx: &mut AsyncApp| {
            let Ok(done) = rx.await else { return };
            let (retry, dropped) = with_store(|s| (s.finish(done), s.trim_memory()));
            if !dropped.is_empty() {
                cx.update(|cx| {
                    for image in dropped {
                        cx.drop_image(image, None);
                    }
                });
            }
            tell_listeners(cx);
            cx.update(|cx| pump(cx));
            if let Some(delay) = retry {
                cx.background_executor().timer(delay + Duration::from_millis(50)).await;
                tell_listeners(cx);
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn remote_images_past_the_cap_go_oldest_first_but_not_on_screen() {
        let now = Instant::now();
        let ago = |s: u64| now - Duration::from_secs(s);
        let mb = 1024 * 1024;
        let (a, b, c, d) = ("a", "b", "c", "d");
        // 40 MB held: the oldest goes, and that's enough.
        let mut held = [(ago(30), 10 * mb, &a), (ago(60), 10 * mb, &b), (ago(10), 10 * mb, &c), (ago(5), 10 * mb, &d)];
        assert_eq!(over_cap(&mut held, now), [&b]);
        // Under the cap: nothing.
        let mut held = [(ago(30), 10 * mb, &a)];
        assert!(over_cap(&mut held, now).is_empty());
        // All on screen: kept, over the cap or not.
        let mut held = [(now, 30 * mb, &a), (now, 30 * mb, &b)];
        assert!(over_cap(&mut held, now).is_empty());
    }

    #[core::prelude::v1::test]
    fn paths_are_cleaned_before_resolving() {
        assert_eq!(clean_path(" <a%20b.png> "), "a b.png");
        assert_eq!(clean_path("100%.png"), "100%.png");
        assert_eq!(clean_path("%E5%9B%BE.png"), "图.png");
    }

    #[core::prelude::v1::test]
    fn local_resolution_order() {
        let (base, library) = (Path::new("/lib/notes"), Path::new("/lib"));
        let none = |_: &Path| false;
        let all = |_: &Path| true;
        assert_eq!(resolve_local_path("file:///x/a.png", Some(base), Some(library), &none), Some("/x/a.png".into()));
        assert_eq!(resolve_local_path("file:///x/a%20b.png", Some(base), None, &none), Some("/x/a b.png".into()));
        assert_eq!(resolve_local_path("/abs/a.png", Some(base), Some(library), &none), Some("/abs/a.png".into()));
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(resolve_local_path("~/a.png", Some(base), Some(library), &none), Some(home.join("a.png")));
        assert_eq!(resolve_local_path("a.png", Some(base), Some(library), &all), Some("/lib/notes/a.png".into()));
        assert_eq!(resolve_local_path("a.png", Some(base), Some(library), &none), Some("/lib/a.png".into()));
        assert_eq!(resolve_local_path("a.png", Some(base), None, &none), None);
    }

    #[core::prelude::v1::test]
    fn embed_targets() {
        assert_eq!(embed_name("a.png|300"), Some("a.png"));
        assert_eq!(embed_name("a.png#x"), Some("a.png"));
        assert_eq!(embed_name(" a.png "), Some("a.png"));
        assert_eq!(embed_name("|300"), None);
    }

    #[core::prelude::v1::test]
    fn embed_index_prefers_the_shallowest_and_skips_what_it_should() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for path in [
            "sub/deep/Pic.PNG",
            "sub/pic.png",
            ".hidden/only.png",
            "App.app/inner.png",
            "note.md",
            "sub/.dot.png",
        ] {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, b"x").unwrap();
        }
        let index = build_embed_index(root);
        assert_eq!(index.get("pic.png"), Some(&root.join("sub/pic.png")));
        for absent in ["only.png", "inner.png", "note.md", ".dot.png"] {
            assert!(!index.contains_key(absent), "{absent}");
        }
    }

    #[core::prelude::v1::test]
    fn remote_urls_are_cleaned() {
        assert_eq!(normalized_key("<https://a.com/x?a=1&amp;b=2>"), "https://a.com/x?a=1&b=2");
        assert_eq!(normalized_key("https://a.com/ x\u{a0}y\n.png"), "https://a.com/xy.png");
        // Only remote addresses lose their spaces.
        assert_eq!(normalized_key("my pic.png"), "my pic.png");
        assert!(is_remote(" HTTPS://a.com/x.png"));
        assert!(!is_remote("a.png"));
        assert_eq!(encode_url("https://a.com/图 (1).png?x=1#y"), "https://a.com/%E5%9B%BE%20(1).png?x=1#y");
        assert_eq!(encode_url("https://a.com/a%20b.png"), "https://a.com/a%20b.png");
    }

    #[core::prelude::v1::test]
    fn retry_delays_follow_the_failure() {
        for code in [6, 7, 28, 52, 56] {
            assert_eq!(retry_delay(FetchFailure::Transport(code)), Duration::from_secs(15), "{code}");
        }
        assert_eq!(retry_delay(FetchFailure::Transport(35)), Duration::from_secs(300));
        for status in [408, 429, 500, 503, 599] {
            assert_eq!(retry_delay(FetchFailure::Http(status)), Duration::from_secs(30), "{status}");
        }
        for status in [403, 404] {
            assert_eq!(retry_delay(FetchFailure::Http(status)), Duration::from_secs(300), "{status}");
        }
        assert_eq!(retry_delay(FetchFailure::Undecodable), Duration::from_secs(300));
        assert_eq!(retry_delay(FetchFailure::BadUrl), Duration::from_secs(300));
    }

    #[core::prelude::v1::test]
    fn requests_send_the_named_headers() {
        let args = curl_args("https://open.FEISHU.cn/x.png", Path::new("/tmp/o"), None).join(" ");
        assert!(args.contains("--referer https://www.feishu.cn/"));
        assert!(args.contains("Version/17.0 Safari"));
        assert!(args.contains("Accept: image/avif"));
        assert!(args.contains("--max-time 120"));
        assert!(args.contains("--speed-time 30"));
        for host in ["https://x.larksuite.com/a", "https://x.larkoffice.com:8080/a"] {
            assert!(curl_args(host, Path::new("/tmp/o"), None).join(" ").contains("--referer"), "{host}");
        }
        assert!(!curl_args("https://example.com/feishu.png", Path::new("/tmp/o"), None).join(" ").contains("--referer"));
    }

    #[core::prelude::v1::test]
    fn the_host_is_just_the_host() {
        assert_eq!(host_of("https://user:pw@Example.COM:443/a?b#c"), "example.com");
        assert_eq!(host_of("http://a.b"), "a.b");
    }

    #[core::prelude::v1::test]
    fn the_cache_keeps_to_its_cap() {
        let dir = tempfile::tempdir().unwrap();
        for (i, name) in ["old", "mid", "new"].iter().enumerate() {
            let path = dir.path().join(name);
            std::fs::write(&path, vec![0u8; 100]).unwrap();
            let when = libc::timeval { tv_sec: 1_000_000 + i as i64 * 1000, tv_usec: 0 };
            let c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
            unsafe { libc::utimes(c.as_ptr(), [when, when].as_ptr()) };
        }
        trim_cache(dir.path(), 300);
        assert!(dir.path().join("old").exists());
        trim_cache(dir.path(), 150);
        assert!(!dir.path().join("old").exists() && !dir.path().join("mid").exists());
        assert!(dir.path().join("new").exists());
        let a = cache_file(Path::new("/c"), "https://a");
        assert_ne!(a, cache_file(Path::new("/c"), "https://b"));
        assert_eq!(a.file_name().unwrap().len(), 64);
    }

    #[core::prelude::v1::test]
    fn sizes() {
        assert_eq!(downsampled_size(4000., 3000.), (1200., 900.));
        assert_eq!(downsampled_size(3000., 4000.), (900., 1200.));
        assert_eq!(downsampled_size(800., 600.), (800., 600.));
        assert_eq!(display_size((1200., 900.), 680.), (680., 510.));
        assert_eq!(display_size((300., 100.), 680.), (300., 100.));
        assert_eq!(video_size(None, 680.), (680., 383.));
        assert_eq!(video_size(Some((1920., 1080.)), 640.), (640., 360.));
        // A tall video stops at 1.25.
        assert_eq!(video_size(Some((1080., 1920.)), 400.), (400., 500.));
        assert_eq!(aspect_fit((100., 100.), (200., 100.)), (50., 0., 100., 100.));
        assert_eq!(format_time(65.), "1:05");
        assert_eq!(format_time(3725.), "1:02:05");
        assert_eq!(format_time(0.4), "0:00");
    }

    fn media(caption: &str, kind: MediaKind, fills_line: bool) -> InlineMedia {
        InlineMedia {
            info: MediaInfo { key: "k".into(), kind, natural: Some((1000., 500.)), settling: false },
            caption: caption.into(),
            fills_line,
            src: 0..0,
        }
    }

    #[core::prelude::v1::test]
    fn boxes_reserve_the_caption_and_the_line() {
        let still = MediaKind::Image { animated: false };
        let g = media("", still.clone(), false).geom(680.);
        assert_eq!((g.box_width, g.box_height), (680., 340.));
        let g = media("  a cat ", still.clone(), false).geom(680.);
        assert_eq!(g.box_height, 340. + CAPTION_HEIGHT);
        // Blank alt text is no caption, and an animated image never has one.
        assert_eq!(media("  ", still.clone(), false).geom(680.).box_height, 340.);
        assert_eq!(media("cat", MediaKind::Image { animated: true }, false).geom(680.).box_height, 340.);
        let mut small = media("", still, true);
        small.info.natural = Some((100., 50.));
        let g = small.geom(680.);
        assert_eq!((g.box_width, g.width, g.height), (680., 100., 50.));
        let g = media("", MediaKind::Video { path: "a.mp4".into() }, false).geom(640.);
        assert_eq!((g.width, g.height, g.box_height), (640., 320., 320. + CAPTION_HEIGHT));
    }

    #[core::prelude::v1::test]
    fn frame_timing() {
        assert_eq!(gif_delay(None, None), 0.1);
        assert_eq!(gif_delay(Some(0.01), Some(0.5)), 0.1);
        assert_eq!(gif_delay(None, Some(0.5)), 0.5);
        assert_eq!(gif_delay(Some(0.25), Some(0.5)), 0.25);
        let delays = [0.1, 0.2, 0.3];
        assert_eq!(frame_at(&delays, 0.6, 0.05).0, 0);
        assert_eq!(frame_at(&delays, 0.6, 0.15).0, 1);
        assert_eq!(frame_at(&delays, 0.6, 0.35).0, 2);
        // On a loop.
        assert_eq!(frame_at(&delays, 0.6, 0.65).0, 0);
        assert_eq!(frame_at(&[0.], 0., 5.).0, 0);
    }

    fn write_png(path: &Path, w: u32, h: u32) {
        let image = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 128]));
        image.save_with_format(path, image::ImageFormat::Png).unwrap();
    }

    #[core::prelude::v1::test]
    fn imageio_reads_the_header_and_downsamples() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big.png");
        write_png(&big, 2400, 1200);
        assert_eq!(probe_image(&big), Some(Probe { natural: (1200., 600.), animated: false }));
        let decoded = decode_image(&big).unwrap();
        assert_eq!(decoded.image.frame_count(), 1);
        let size = decoded.image.size(0);
        assert_eq!((size.width.0, size.height.0), (1200, 600));
        // BGRA with straight alpha.
        let px = &decoded.image.as_bytes(0).unwrap()[..4];
        assert!((px[2] as i32 - 10).abs() < 3 && (px[1] as i32 - 20).abs() < 3 && (px[0] as i32 - 30).abs() < 3 && px[3] == 128, "{px:?}");

        // Small images aren't blown up.
        let small = dir.path().join("small.png");
        write_png(&small, 64, 32);
        assert_eq!(probe_image(&small).unwrap().natural, (64., 32.));
        let size = decode_image(&small).unwrap().image.size(0);
        assert_eq!((size.width.0, size.height.0), (64, 32));

        let text = dir.path().join("a.png");
        std::fs::write(&text, "not an image").unwrap();
        assert_eq!(probe_image(&text), None);
    }

    #[core::prelude::v1::test]
    fn gifs_have_frames_and_delays() {
        use image::codecs::gif::GifEncoder;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.gif");
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = GifEncoder::new(file);
        for shade in [0u8, 255] {
            let frame = image::Frame::from_parts(
                image::RgbaImage::from_pixel(8, 8, image::Rgba([shade, shade, shade, 255])),
                0,
                0,
                image::Delay::from_numer_denom_ms(200, 1),
            );
            encoder.encode_frame(frame).unwrap();
        }
        drop(encoder);
        let probe = probe_image(&path).unwrap();
        assert!(probe.animated);
        let decoded = decode_image(&path).unwrap();
        assert_eq!(decoded.image.frame_count(), 2);
        assert_eq!(decoded.delays.len(), 2);
        assert!((decoded.delays[0] - 0.2).abs() < 0.001, "{:?}", decoded.delays);
    }
}
