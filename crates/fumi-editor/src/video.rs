//! Local video: the poster card (`VideoAttachment`) and the inline player that
//! plays over it. Frames come from `AVPlayerItemVideoOutput` and are painted as
//! a surface in GPUI's own scene, so a video scrolls and clips with the text and
//! the wheel over it scrolls the note. The controls are ours; full screen hands
//! the same `AVPlayer` to an `AVPlayerView` in a window of its own.

use crate::display::ui_font;
use crate::media::{self, InlineMedia, MediaGeom, MediaKind};
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use fumi_ui::native::{self, SymbolCache};
use fumi_ui::theme::t;
use gpui::*;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use objc2_core_foundation::CFRetained;
use objc2_core_video::{CVPixelBuffer, CVPixelBufferGetHeight, CVPixelBufferGetWidth};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Once;
use std::time::{Duration, Instant};

const RADIUS: f32 = 8.;
const PLAY_DIAMETER: f32 = 52.;
/// A playing video's controls leave this long after the pointer stops.
const CONTROLS_LINGER: Duration = Duration::from_millis(2500);

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

impl CMTime {
    fn seconds(self) -> f64 {
        // kCMTimeFlags_Valid, and not indefinite or an infinity.
        if self.flags & 1 == 0 || self.flags & 0b11100 != 0 || self.timescale == 0 {
            0.
        } else {
            self.value as f64 / self.timescale as f64
        }
    }

    fn at(seconds: f64) -> Self {
        CMTime { value: (seconds.max(0.) * 600.).round() as i64, timescale: 600, flags: 1, epoch: 0 }
    }

    const ZERO: CMTime = CMTime { value: 0, timescale: 1, flags: 1, epoch: 0 };
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AffineTransform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    fn CACurrentMediaTime() -> f64;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGImageRelease(image: *mut std::ffi::c_void);
}

fn frameworks() {
    static LOAD: Once = Once::new();
    LOAD.call_once(|| {
        native::load_framework("AVFoundation");
        native::load_framework("AVKit");
    });
}

// The poster ------------------------------------------------------------------

/// `min(0.5 s, duration / 2)`.
pub fn poster_time(duration: f64) -> f64 {
    (duration / 2.).max(0.).min(0.5)
}

/// What a card shows of a video before it plays.
pub struct VideoInfo {
    pub poster: Option<Arc<RenderImage>>,
    pub poster_size: Option<(f32, f32)>,
    pub duration: f64,
    /// Its track is turned, and the video output needs telling.
    pub turned: bool,
}

/// Reads a video's poster and duration, with the track's orientation applied.
/// Blocks; on a worker thread.
pub fn read_video_info(path: &Path) -> VideoInfo {
    frameworks();
    let mut info = VideoInfo { poster: None, poster_size: None, duration: 0., turned: false };
    let Some(asset_class) = Class::get("AVURLAsset") else { return info };
    let Some(generator_class) = Class::get("AVAssetImageGenerator") else { return info };
    media::with_pool(|| unsafe {
        let url: id = msg_send![class!(NSURL), fileURLWithPath: native::ns(&path.to_string_lossy())];
        let asset: id = msg_send![asset_class, URLAssetWithURL: url options: nil];
        let duration: CMTime = msg_send![asset, duration];
        let seconds = duration.seconds();
        info.duration = if seconds.is_finite() { seconds } else { 0. };

        let tracks: id = msg_send![asset, tracksWithMediaType: native::ns("vide")];
        let first: id = msg_send![tracks, firstObject];
        if first != nil {
            let turn: AffineTransform = msg_send![first, preferredTransform];
            info.turned = turn.b != 0. || turn.c != 0. || turn.a < 0. || turn.d < 0.;
            let _ = (turn.tx, turn.ty);
        }

        let generator: id = msg_send![generator_class, assetImageGeneratorWithAsset: asset];
        let _: () = msg_send![generator, setAppliesPreferredTrackTransform: true];
        let _: () = msg_send![generator, setMaximumSize: NSSize::new(1400., 1400.)];
        let at = CMTime::at(poster_time(info.duration));
        let none: *mut id = std::ptr::null_mut();
        let actual: *mut CMTime = std::ptr::null_mut();
        let image: *mut std::ffi::c_void = msg_send![generator, copyCGImageAtTime: at actualTime: actual error: none];
        if !image.is_null() {
            if let Some(frame) = media::bgra_of(image) {
                let (w, h) = frame.buffer().dimensions();
                info.poster_size = Some((w as f32, h as f32));
                info.poster = Some(Arc::new(RenderImage::new(smallvec::smallvec![frame])));
            }
            CGImageRelease(image);
        }
    });
    info
}

// The player ------------------------------------------------------------------

pub struct VideoPlayer {
    /// The line it stands on; kept right through edits.
    pub line: usize,
    pub path: PathBuf,
    player: id,
    output: id,
    latest: Option<CFRetained<CVPixelBuffer>>,
    full_screen: Option<(id, id)>,
    /// Keeps drawing until this moment, for a seek to show while paused.
    awake_until: Option<Instant>,
    /// Moving the scrubber: whether to go on playing afterwards.
    pub resume: Option<bool>,
}

impl VideoPlayer {
    pub fn start(line: usize, path: &Path, turned: bool) -> Option<Self> {
        frameworks();
        let player_class = Class::get("AVPlayer")?;
        let output_class = Class::get("AVPlayerItemVideoOutput")?;
        unsafe {
            let url: id = msg_send![class!(NSURL), fileURLWithPath: native::ns(&path.to_string_lossy())];
            let player: id = msg_send![player_class, playerWithURL: url];
            if player == nil {
                return None;
            }
            let player: id = msg_send![player, retain];
            let _: () = msg_send![player, setActionAtItemEnd: 1i64]; // pause
            let item: id = msg_send![player, currentItem];
            // Video output hands over frames as they're stored; a composition
            // turns them the way the track says first.
            if turned && let Some(composition) = Class::get("AVVideoComposition") {
                let asset: id = msg_send![item, asset];
                let made: id = msg_send![composition, videoCompositionWithPropertiesOfAsset: asset];
                if made != nil {
                    let _: () = msg_send![item, setVideoComposition: made];
                }
            }
            let settings: id = msg_send![class!(NSMutableDictionary), new];
            // 420f, the one format GPUI's surface shader takes.
            let format: id = msg_send![class!(NSNumber), numberWithUnsignedInt: 875704422u32];
            let _: () = msg_send![settings, setObject: format forKey: native::ns("PixelFormatType")];
            let empty: id = msg_send![class!(NSDictionary), dictionary];
            let _: () = msg_send![settings, setObject: empty forKey: native::ns("IOSurfaceProperties")];
            let yes: id = msg_send![class!(NSNumber), numberWithBool: true as objc::runtime::BOOL];
            let _: () = msg_send![settings, setObject: yes forKey: native::ns("MetalCompatibility")];
            let output: id = msg_send![output_class, alloc];
            let output: id = msg_send![output, initWithPixelBufferAttributes: settings];
            let _: () = msg_send![settings, release];
            let _: () = msg_send![item, addOutput: output];
            let _: () = msg_send![player, play];
            Some(VideoPlayer {
                line,
                path: path.to_path_buf(),
                player,
                output,
                latest: None,
                full_screen: None,
                awake_until: None,
                resume: None,
            })
        }
    }

    pub fn is_playing(&self) -> bool {
        let rate: f32 = unsafe { msg_send![self.player, rate] };
        rate != 0.
    }

    /// Whether the next frame may look different.
    pub fn is_live(&self) -> bool {
        self.is_playing() || self.awake_until.is_some_and(|until| Instant::now() < until)
    }

    fn wake(&mut self) {
        self.awake_until = Some(Instant::now() + Duration::from_millis(600));
    }

    pub fn time(&self) -> f64 {
        let time: CMTime = unsafe { msg_send![self.player, currentTime] };
        time.seconds()
    }

    pub fn duration(&self) -> f64 {
        unsafe {
            let item: id = msg_send![self.player, currentItem];
            if item == nil {
                return 0.;
            }
            let duration: CMTime = msg_send![item, duration];
            duration.seconds()
        }
    }

    pub fn toggle(&mut self) {
        if self.is_playing() {
            unsafe {
                let _: () = msg_send![self.player, pause];
            }
        } else {
            self.play();
        }
        self.wake();
    }

    pub fn play(&mut self) {
        // At the end, playing means from the beginning.
        let duration = self.duration();
        if duration > 0. && self.time() >= duration - 0.05 {
            self.seek(0., true);
        }
        unsafe {
            let _: () = msg_send![self.player, play];
        }
    }

    pub fn pause(&mut self) {
        unsafe {
            let _: () = msg_send![self.player, pause];
        }
    }

    pub fn seek(&mut self, seconds: f64, exact: bool) {
        unsafe {
            let tolerance = if exact { CMTime::ZERO } else { CMTime::at(0.1) };
            let _: () = msg_send![self.player, seekToTime: CMTime::at(seconds) toleranceBefore: tolerance toleranceAfter: tolerance];
        }
        self.wake();
    }

    pub fn is_muted(&self) -> bool {
        unsafe { msg_send![self.player, isMuted] }
    }

    pub fn toggle_muted(&mut self) {
        let muted = self.is_muted();
        unsafe {
            let _: () = msg_send![self.player, setMuted: !muted];
        }
    }

    pub fn volume(&self) -> f32 {
        unsafe { msg_send![self.player, volume] }
    }

    pub fn set_volume(&mut self, volume: f32) {
        unsafe {
            let _: () = msg_send![self.player, setVolume: volume.clamp(0., 1.)];
            let _: () = msg_send![self.player, setMuted: false];
        }
    }

    fn has_new_frame(&self) -> bool {
        // A paused video being sought keeps drawing until it settles.
        if !self.is_playing() {
            return true;
        }
        unsafe {
            let now: CMTime = msg_send![self.output, itemTimeForHostTime: CACurrentMediaTime()];
            msg_send![self.output, hasNewPixelBufferForItemTime: now]
        }
    }

    /// The newest frame due by now.
    fn pull(&mut self) -> Option<CFRetained<CVPixelBuffer>> {
        unsafe {
            let now: CMTime = msg_send![self.output, itemTimeForHostTime: CACurrentMediaTime()];
            let fresh: bool = msg_send![self.output, hasNewPixelBufferForItemTime: now];
            if fresh {
                let none: *mut CMTime = std::ptr::null_mut();
                let raw: *mut CVPixelBuffer = msg_send![self.output, copyPixelBufferForItemTime: now itemTimeForDisplay: none];
                if let Some(ptr) = std::ptr::NonNull::new(raw) {
                    self.latest = Some(CFRetained::from_raw(ptr));
                }
            }
        }
        self.latest.clone()
    }

    /// Plays on in a full-screen window of its own; leaving it comes back here.
    pub fn enter_full_screen(&mut self) {
        self.close_full_screen();
        let Some(view_class) = Class::get("AVPlayerView") else { return };
        unsafe {
            let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(960., 540.));
            let view: id = msg_send![view_class, alloc];
            let view: id = msg_send![view, initWithFrame: frame];
            let _: () = msg_send![view, setPlayer: self.player];
            let _: () = msg_send![view, setControlsStyle: 1i64]; // inline
            // Titled, closable, miniaturizable, resizable, full-size content.
            let style: u64 = 1 | 2 | 4 | 8 | (1 << 15);
            let window: id = msg_send![class!(NSWindow), alloc];
            let window: id = msg_send![window, initWithContentRect: frame styleMask: style backing: 2u64 defer: false];
            let _: () = msg_send![window, setReleasedWhenClosed: false];
            let _: () = msg_send![window, setCollectionBehavior: 1u64 << 7]; // fullScreenPrimary
            let _: () = msg_send![window, setContentView: view];
            let _: () = msg_send![view, release];
            let delegate: id = msg_send![full_screen_delegate(), new];
            let _: () = msg_send![window, setDelegate: delegate];
            let _: () = msg_send![window, center];
            let _: () = msg_send![window, makeKeyAndOrderFront: nil];
            let _: () = msg_send![window, toggleFullScreen: nil];
            self.full_screen = Some((window, delegate));
        }
    }

    fn close_full_screen(&mut self) {
        if let Some((window, delegate)) = self.full_screen.take() {
            unsafe {
                let _: () = msg_send![window, setDelegate: nil];
                let _: () = msg_send![window, close];
                let _: () = msg_send![window, release];
                let _: () = msg_send![delegate, release];
            }
        }
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        self.close_full_screen();
        unsafe {
            let _: () = msg_send![self.player, pause];
            let item: id = msg_send![self.player, currentItem];
            if item != nil {
                let _: () = msg_send![item, removeOutput: self.output];
            }
            let _: () = msg_send![self.output, release];
            let _: () = msg_send![self.player, release];
        }
    }
}

extern "C" fn did_exit_full_screen(_: &Object, _: Sel, note: id) {
    unsafe {
        let window: id = msg_send![note, object];
        let _: () = msg_send![window, orderOut: nil];
        let view: id = msg_send![window, contentView];
        let _: () = msg_send![view, setPlayer: nil];
    }
}

/// Esc leaves full screen.
extern "C" fn cancel_operation(this: &Object, _: Sel, sender: id) {
    unsafe {
        let window: id = *this.get_ivar("window");
        if window != nil {
            let _: () = msg_send![window, toggleFullScreen: sender];
        }
    }
}

extern "C" fn did_enter_full_screen(this: &mut Object, _: Sel, note: id) {
    unsafe {
        let window: id = msg_send![note, object];
        this.set_ivar("window", window);
    }
}

fn full_screen_delegate() -> &'static Class {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let mut decl = ClassDecl::new("FumiVideoWindowDelegate", class!(NSObject)).expect("class name free");
        decl.add_ivar::<id>("window");
        unsafe {
            decl.add_method(
                sel!(windowDidExitFullScreen:),
                did_exit_full_screen as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(windowDidEnterFullScreen:),
                did_enter_full_screen as extern "C" fn(&mut Object, Sel, id),
            );
            decl.add_method(sel!(cancelOperation:), cancel_operation as extern "C" fn(&Object, Sel, id));
        }
        decl.register();
    });
    Class::get("FumiVideoWindowDelegate").expect("registered")
}

/// The players of one editor, and what they need from its pointer.
pub struct VideoUi {
    pub players: Vec<VideoPlayer>,
    /// The player with the pointer over it.
    pub hovered: Option<usize>,
    pub moved_at: Instant,
    /// Set by a paint: something is playing on screen.
    pub live: Cell<bool>,
    /// Something's already watching for that video's next frame.
    pub watching: Cell<bool>,
    /// Set by a paint: an animated image's next frame is due by then.
    pub next_frame: Cell<Option<Instant>>,
}

impl Default for VideoUi {
    fn default() -> Self {
        VideoUi {
            players: Vec::new(),
            hovered: None,
            moved_at: Instant::now(),
            live: Cell::new(false),
            watching: Cell::new(false),
            next_frame: Cell::new(None),
        }
    }
}

impl VideoUi {
    /// Whether a playing video has something new to draw, or stopped playing.
    pub fn frame_due(&mut self) -> bool {
        self.players.iter().all(|p| !p.is_live()) || self.players.iter().any(|p| p.is_live() && p.has_new_frame())
    }

    pub fn player_on(&self, line: usize, path: &Path) -> Option<usize> {
        self.players.iter().position(|p| p.line == line && p.path == path)
    }

    /// A paused player keeps its controls; a playing one shows them while the
    /// pointer's on it and moving.
    pub fn controls_visible(&self, index: usize) -> bool {
        let Some(player) = self.players.get(index) else { return false };
        !player.is_playing()
            || player.resume.is_some()
            || (self.hovered == Some(index) && self.moved_at.elapsed() < CONTROLS_LINGER)
    }
}

// Controls --------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    PlayPause,
    Scrubber,
    Sound,
    Volume,
    FullScreen,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    pub bar: Bounds<Pixels>,
    pub play: Bounds<Pixels>,
    pub current: Option<Bounds<Pixels>>,
    pub track: Bounds<Pixels>,
    pub remaining: Option<Bounds<Pixels>>,
    pub sound: Bounds<Pixels>,
    pub volume: Option<Bounds<Pixels>>,
    pub full: Bounds<Pixels>,
}

const BAR_HEIGHT: f32 = 36.;
const BAR_INSET: f32 = 8.;
const BUTTON: f32 = 28.;

/// Where everything goes in a video this big; narrow ones lose the volume
/// slider first, then the times.
pub fn controls_layout(video: Bounds<Pixels>, long: bool) -> Controls {
    let width = f32::from(video.size.width);
    let bar = Bounds::new(
        point(video.left() + px(BAR_INSET), video.bottom() - px(BAR_INSET + BAR_HEIGHT)),
        size(video.size.width - px(BAR_INSET * 2.), px(BAR_HEIGHT)),
    );
    let y = bar.top() + px((BAR_HEIGHT - BUTTON) / 2.);
    let slot = |x: Pixels, w: f32| Bounds::new(point(x, y), size(px(w), px(BUTTON)));
    let label = if long { 62. } else { 44. };
    let show_times = width >= 240.;
    let show_volume = width >= 400.;

    let mut left = bar.left() + px(4.);
    let play = slot(left, BUTTON);
    left += px(BUTTON + 2.);
    let current = show_times.then(|| {
        let b = slot(left, label);
        left += px(label + 6.);
        b
    });

    let mut right = bar.right() - px(4.);
    right -= px(BUTTON);
    let full = slot(right, BUTTON);
    let volume = show_volume.then(|| {
        right -= px(60.);
        slot(right, 56.)
    });
    right -= px(BUTTON);
    let sound = slot(right, BUTTON);
    let remaining = show_times.then(|| {
        right -= px(label + 2.);
        slot(right, label)
    });
    right -= px(6.);
    let track = slot(left, f32::from((right - left).max(px(10.))));
    Controls { bar, play, current, track, remaining, sound, volume, full }
}

impl Controls {
    pub fn control_at(&self, p: Point<Pixels>) -> Option<Control> {
        if self.play.contains(&p) {
            Some(Control::PlayPause)
        } else if self.track.contains(&p) {
            Some(Control::Scrubber)
        } else if self.sound.contains(&p) {
            Some(Control::Sound)
        } else if self.volume.is_some_and(|b| b.contains(&p)) {
            Some(Control::Volume)
        } else if self.full.contains(&p) {
            Some(Control::FullScreen)
        } else {
            None
        }
    }
}

/// Where along a slider the pointer is, 0 to 1.
pub fn fraction_along(track: Bounds<Pixels>, x: Pixels) -> f32 {
    let width = f32::from(track.size.width);
    if width <= 0. {
        return 0.;
    }
    (f32::from(x - track.left()) / width).clamp(0., 1.)
}

// Painting --------------------------------------------------------------------

fn pill(bounds: Bounds<Pixels>, color: Hsla, radius: f32, window: &mut Window) {
    window.paint_quad(PaintQuad {
        bounds,
        corner_radii: Corners::all(px(radius)),
        background: color.into(),
        border_widths: Edges::default(),
        border_color: transparent_black(),
        border_style: BorderStyle::default(),
    });
}

fn digits_font(weight: FontWeight) -> Font {
    let mut f = ui_font();
    f.weight = weight;
    f.features = FontFeatures(std::sync::Arc::new(vec![("tnum".to_string(), 1)]));
    f
}

fn text_line(text: String, font: Font, font_size: f32, color: Hsla, window: &mut Window) -> ShapedLine {
    let run = TextRun {
        len: text.len(),
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(text.into(), px(font_size), &[run], None)
}

/// A caption: 12 pt, centered in `width`, truncated if it doesn't fit, 7 pt under
/// what it's under.
pub fn paint_caption(text: &str, origin: Point<Pixels>, width: Pixels, from: TruncateFrom, window: &mut Window, cx: &mut App) {
    let c = t();
    let run = TextRun {
        len: text.len(),
        font: ui_font(),
        color: c.text3,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let (shown, runs) = window.text_system().line_wrapper(ui_font(), px(12.)).truncate_line(
        SharedString::from(text.to_string()),
        width,
        "…",
        std::slice::from_ref(&run),
        from,
    );
    let line = window.text_system().shape_line(shown, px(12.), &runs, None);
    let x = origin.x + (width - line.width) / 2.;
    let height = px(media::CAPTION_HEIGHT - media::CAPTION_GAP);
    let _ = line.paint(point(x, origin.y), height, TextAlign::Left, None, window, cx);
}

/// The white triangle on the play button.
fn paint_triangle(center: Point<Pixels>, scale: f32, window: &mut Window) {
    let mut path = PathBuilder::fill();
    path.move_to(point(center.x - px(7. * scale), center.y - px(11. * scale)));
    path.line_to(point(center.x + px(12. * scale), center.y));
    path.line_to(point(center.x - px(7. * scale), center.y + px(11. * scale)));
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, t().video_ink);
    }
}

/// A surface can't be rounded, so its corners are painted over with the page.
fn paint_corner_masks(b: Bounds<Pixels>, radius: f32, color: Hsla, window: &mut Window) {
    let r = px(radius);
    // How far the curve's control points sit from the corner.
    let k = r * 0.448;
    let corners = [
        (b.origin, px(1.), px(1.)),
        (point(b.right(), b.top()), px(-1.), px(1.)),
        (point(b.left(), b.bottom()), px(1.), px(-1.)),
        (point(b.right(), b.bottom()), px(-1.), px(-1.)),
    ];
    for (corner, sx, sy) in corners {
        let (dx, dy) = (f32::from(sx), f32::from(sy));
        let mut path = PathBuilder::fill();
        path.move_to(corner);
        path.line_to(point(corner.x + r * dx, corner.y));
        path.cubic_bezier_to(
            point(corner.x, corner.y + r * dy),
            point(corner.x + k * dx, corner.y),
            point(corner.x, corner.y + k * dy),
        );
        path.close();
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}

/// The card, as `VideoAttachment` draws it, or the player where one's open.
#[allow(clippy::too_many_arguments)]
pub fn paint_video(
    line: usize,
    media: &InlineMedia,
    geom: MediaGeom,
    origin: Point<Pixels>,
    ui: &std::cell::RefCell<VideoUi>,
    symbols: &std::cell::RefCell<SymbolCache>,
    window: &mut Window,
    cx: &mut App,
) {
    let MediaKind::Video { path } = &media.info.kind else { return };
    let c = t();
    let area = Bounds::new(origin, size(px(geom.width), px(geom.height)));
    let info = media::with_store(|s| s.video_info(path));

    pill(area, c.video_ground, RADIUS, window);
    if let Some(info) = &info
        && let (Some(poster), Some(natural)) = (&info.poster, info.poster_size)
    {
        let (x, y, w, h) = media::aspect_fit(natural, (geom.width, geom.height));
        let fit = Bounds::new(point(origin.x + px(x), origin.y + px(y)), size(px(w), px(h)));
        // Only a poster reaching the corners has any to round.
        let radius = if x < 1. && y < 1. { px(RADIUS) } else { px(0.) };
        let _ = window.paint_image(fit, fit, Corners::all(radius), poster.clone(), 0, false);
    }

    let open = ui.borrow().player_on(line, path);
    match open {
        Some(index) => paint_player(index, area, ui, symbols, window, cx),
        None => {
            pill(area, c.video_shade, RADIUS, window);
            let center = area.center();
            let half = px(PLAY_DIAMETER / 2.);
            pill(
                Bounds::new(point(center.x - half, center.y - half), size(half * 2., half * 2.)),
                c.video_button,
                PLAY_DIAMETER / 2.,
                window,
            );
            paint_triangle(center, 1., window);
            if let Some(duration) = info.as_ref().map(|i| i.duration).filter(|d| *d > 0.) {
                let line = text_line(media::format_time(duration), digits_font(FontWeight::MEDIUM), 12., c.video_ink, window);
                let (w, h) = (line.width + px(10.), px(19.));
                let b = Bounds::new(point(area.left() + px(10.), area.bottom() - px(10.) - h), size(w, h));
                pill(b, c.video_pill, 4., window);
                let _ = line.paint(point(b.left() + px(5.), b.top()), h, TextAlign::Left, None, window, cx);
            }
        }
    }

    // The file name, 7 pt under the poster.
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    paint_caption(
        &name,
        point(origin.x, origin.y + px(geom.height + media::CAPTION_GAP)),
        px(geom.width),
        TruncateFrom::Middle,
        window,
        cx,
    );
}

fn paint_player(
    index: usize,
    area: Bounds<Pixels>,
    ui: &std::cell::RefCell<VideoUi>,
    symbols: &std::cell::RefCell<SymbolCache>,
    window: &mut Window,
    cx: &mut App,
) {
    let c = t();
    let mut borrowed = ui.borrow_mut();
    let visible = borrowed.controls_visible(index);
    let player = &mut borrowed.players[index];
    if let Some(buffer) = player.pull() {
        let (w, h) = (CVPixelBufferGetWidth(&buffer) as f32, CVPixelBufferGetHeight(&buffer) as f32);
        let (x, y, fit_w, fit_h) = media::aspect_fit((w, h), (f32::from(area.size.width), f32::from(area.size.height)));
        // A rounded height shouldn't leave a hairline of the ground showing.
        let fit = if x < 1. && y < 1. {
            area
        } else {
            Bounds::new(point(area.left() + px(x), area.top() + px(y)), size(px(fit_w), px(fit_h)))
        };
        window.paint_surface(fit, buffer);
    }
    // Glass has no page color to paint with, so its video keeps square corners.
    if !c.glass {
        paint_corner_masks(area, RADIUS, c.page_content, window);
    }
    let live = player.is_live();
    let playing = player.is_playing();
    let (time, duration, muted, volume) = (player.time(), player.duration(), player.is_muted(), player.volume());
    // Something changes on the next frame: the picture, or the controls leaving.
    if live {
        borrowed.live.set(true);
    }
    drop(borrowed);
    if !visible {
        return;
    }

    let layout = controls_layout(area, duration >= 3600.);
    pill(layout.bar, c.video_pill, 10., window);

    let scale = window.scale_factor();
    let symbol = |name: &str, bounds: Bounds<Pixels>, point_size: f32, window: &mut Window| {
        let image = symbols.borrow_mut().get(name, point_size, native::weight::MEDIUM, c.video_ink, scale);
        if let Some((image, (w, h))) = image {
            let center = bounds.center();
            let b = Bounds::new(point(center.x - px(w / 2.), center.y - px(h / 2.)), size(px(w), px(h)));
            let _ = window.paint_image(b, b, Corners::default(), image, 0, false);
        }
    };
    symbol(if playing { "pause.fill" } else { "play.fill" }, layout.play, 14., window);
    symbol("arrow.up.left.and.arrow.down.right", layout.full, 13., window);
    symbol(
        if muted || volume == 0. { "speaker.slash.fill" } else { "speaker.wave.2.fill" },
        layout.sound,
        13.,
        window,
    );

    let label = |text: String, b: Bounds<Pixels>, right_align: bool, window: &mut Window, cx: &mut App| {
        let line = text_line(text, digits_font(FontWeight::MEDIUM), 12., c.video_ink, window);
        let x = if right_align { b.right() - line.width } else { b.left() };
        let _ = line.paint(point(x, b.top()), b.size.height, TextAlign::Left, None, window, cx);
    };
    if let Some(b) = layout.current {
        label(media::format_time(time), b, true, window, cx);
    }
    if let Some(b) = layout.remaining {
        label(format!("-{}", media::format_time((duration - time).max(0.))), b, false, window, cx);
    }

    let slider = |track: Bounds<Pixels>, value: f32, window: &mut Window| {
        let mid = track.center().y;
        let bar = Bounds::new(point(track.left(), mid - px(1.5)), size(track.size.width, px(3.)));
        pill(bar, c.video_track, 1.5, window);
        let done = track.size.width * value.clamp(0., 1.);
        pill(Bounds::new(bar.origin, size(done, px(3.))), c.video_ink, 1.5, window);
        let knob = Bounds::new(point(track.left() + done - px(5.), mid - px(5.)), size(px(10.), px(10.)));
        pill(knob, c.video_ink, 5., window);
    };
    let progress = if duration > 0. { (time / duration) as f32 } else { 0. };
    slider(layout.track, progress, window);
    if let Some(b) = layout.volume {
        slider(b, if muted { 0. } else { volume }, window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(100.), px(50.)), size(px(w), px(h)))
    }

    #[core::prelude::v1::test]
    fn poster_is_taken_early() {
        assert_eq!(poster_time(10.), 0.5);
        assert_eq!(poster_time(0.4), 0.2);
        assert_eq!(poster_time(0.), 0.);
    }

    #[core::prelude::v1::test]
    fn controls_fit_and_do_not_overlap() {
        for width in [120., 240., 400., 680.] {
            let c = controls_layout(video(width, 300.), false);
            let mut pieces = vec![c.play, c.track, c.sound, c.full];
            pieces.extend(c.current);
            pieces.extend(c.remaining);
            pieces.extend(c.volume);
            pieces.sort_by(|a, b| a.left().partial_cmp(&b.left()).unwrap());
            for pair in pieces.windows(2) {
                assert!(pair[0].right() <= pair[1].left() + px(0.5), "{width}: {pair:?}");
            }
            assert!(c.bar.bottom() < video(width, 300.).bottom());
            assert!(pieces.last().unwrap().right() <= c.bar.right());
        }
        let wide = controls_layout(video(680., 300.), false);
        assert!(wide.volume.is_some() && wide.current.is_some());
        let narrow = controls_layout(video(200., 300.), false);
        assert!(narrow.volume.is_none() && narrow.current.is_none());
    }

    #[core::prelude::v1::test]
    fn controls_answer_to_the_pointer() {
        let c = controls_layout(video(680., 300.), false);
        assert_eq!(c.control_at(c.play.center()), Some(Control::PlayPause));
        assert_eq!(c.control_at(c.track.center()), Some(Control::Scrubber));
        assert_eq!(c.control_at(c.sound.center()), Some(Control::Sound));
        assert_eq!(c.control_at(c.full.center()), Some(Control::FullScreen));
        assert_eq!(c.control_at(c.volume.unwrap().center()), Some(Control::Volume));
        assert_eq!(c.control_at(point(px(110.), px(60.))), None);
    }

    #[core::prelude::v1::test]
    fn sliders_clamp() {
        let track = Bounds::new(point(px(10.), px(0.)), size(px(100.), px(20.)));
        assert_eq!(fraction_along(track, px(60.)), 0.5);
        assert_eq!(fraction_along(track, px(0.)), 0.);
        assert_eq!(fraction_along(track, px(500.)), 1.);
    }

    #[core::prelude::v1::test]
    fn times_are_only_trusted_when_valid() {
        assert_eq!(CMTime { value: 600, timescale: 600, flags: 1, epoch: 0 }.seconds(), 1.);
        assert_eq!(CMTime { value: 600, timescale: 600, flags: 0, epoch: 0 }.seconds(), 0.);
        // Indefinite.
        assert_eq!(CMTime { value: 0, timescale: 0, flags: 17, epoch: 0 }.seconds(), 0.);
        assert_eq!(CMTime::at(1.5).seconds(), 1.5);
    }
}
