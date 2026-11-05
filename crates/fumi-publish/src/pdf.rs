//! Exports a note as a PDF. The note is laid out line by line by the editor's own
//! engine (`display::build_line`), with no caret, so no syntax shows and the text
//! copied out of the PDF has no Markdown marks. AppKit's text stack sets those
//! lines, so the PDF holds real text. Only the paper is new: A4 with margins, a
//! running foot and page numbers, painted in the light palette.

use crate::mac::{self, bytes_of, ns};
use block::ConcreteBlock;
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRange, NSRect, NSSize};
use fumi_core::frontmatter::{Frontmatter, Value};
use fumi_core::markdown::*;
use fumi_core::wiki_link::WikiLinkIndex;
use fumi_editor::blocks::{column_widths, latex_of};
use fumi_editor::display::*;
use fumi_editor::editor::{ItemKind, build_items};
use fumi_editor::element::callout_symbol;
use fumi_editor::media::{self, InlineMedia, Lookup, MediaInfo, MediaKind, MediaLookup};
use fumi_ui::native::{load_framework, weight};
use fumi_ui::settings;
use fumi_ui::theme::{self, Tokens, hex, t, text_size};
use gpui::{Font, FontStyle, Hsla, Rgba};
use objc::runtime::{BOOL, YES};
use objc::{class, msg_send, sel, sel_impl};
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    static kCGPDFContextTitle: id;
    static kCGPDFContextCreator: id;
    fn CGDataConsumerCreateWithCFData(data: id) -> *mut c_void;
    fn CGDataConsumerRelease(consumer: *mut c_void);
    fn CGPDFContextCreate(consumer: *mut c_void, media_box: *const NSRect, info: id) -> *mut c_void;
    fn CGPDFContextBeginPage(context: *mut c_void, info: id);
    fn CGPDFContextEndPage(context: *mut c_void);
    fn CGPDFContextClose(context: *mut c_void);
    fn CGContextRelease(context: *mut c_void);
    fn CGContextSaveGState(context: *mut c_void);
    fn CGContextRestoreGState(context: *mut c_void);
    fn CGContextTranslateCTM(context: *mut c_void, x: f64, y: f64);
    fn CGContextScaleCTM(context: *mut c_void, x: f64, y: f64);
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSFontAttributeName: id;
    static NSForegroundColorAttributeName: id;
    static NSParagraphStyleAttributeName: id;
    static NSUnderlineStyleAttributeName: id;
    static NSUnderlineColorAttributeName: id;
    static NSStrikethroughStyleAttributeName: id;
    static NSStrikethroughColorAttributeName: id;
    static NSKernAttributeName: id;
    static NSBackgroundColorAttributeName: id;
    static NSFontDescriptorSystemDesignSerif: id;
}

/// The sheet and the type it is set in. The type follows the editor's own settings,
/// so the PDF reads like the note on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// A4, in points.
    pub page: (f64, f64),
    /// Top, left, bottom, right.
    pub margin: (f64, f64, f64, f64),
    pub family: String,
    pub size: f32,
    pub line_height: f32,
    /// The note's name, in the foot of every page and the document's title.
    pub title: String,
}

impl Config {
    pub fn new(family: String, size: f32, line_height: f32, title: String) -> Config {
        Config { page: (595.28, 841.89), margin: (64., 60., 60., 60.), family, size, line_height, title }
    }

    pub fn from_settings(title: String) -> Config {
        Config::new(settings::editor_font(), settings::font_size() as f32, settings::line_height() as f32, title)
    }

    fn content(&self) -> (f64, f64) {
        (self.page.0 - self.margin.1 - self.margin.3, self.page.1 - self.margin.0 - self.margin.2)
    }
}

// Pagination ---------------------------------------------------------------------

/// Breaks laid-out rows into pages of `page_height` points: indexes into `rows`,
/// each `(top, bottom)`. A row is never split: one that does not fit goes to the
/// next page whole. An empty note still gets one sheet.
pub fn paginate(rows: &[(f64, f64)], page_height: f64) -> Vec<Vec<usize>> {
    let mut pages = Vec::new();
    let mut page = Vec::new();
    let mut top = 0.;
    for (i, (min_y, max_y)) in rows.iter().enumerate() {
        if !page.is_empty() && max_y - top > page_height {
            pages.push(std::mem::take(&mut page));
            top = *min_y;
        }
        page.push(i);
    }
    pages.push(page);
    pages
}

/// `(width, height)` shrunk to fit, keeping its shape.
pub fn fit_size(width: f64, height: f64, max_width: f64, max_height: f64) -> (f64, f64) {
    let factor = (max_width / width.max(1.)).min(max_height / height.max(1.)).min(1.);
    (width * factor, height * factor)
}

/// The UTF-16 length, which is what AppKit counts in.
fn utf16_len(text: &str) -> u64 {
    text.encode_utf16().count() as u64
}

// Fonts and colors ------------------------------------------------------------------

fn color(c: Hsla) -> id {
    let c: Rgba = c.into();
    unsafe {
        msg_send![class!(NSColor), colorWithSRGBRed: c.r as f64 green: c.g as f64 blue: c.b as f64 alpha: c.a as f64]
    }
}

/// `NSFont.Weight` for a CSS weight.
fn ns_weight(css: f32) -> f64 {
    match css as i32 {
        ..=150 => -0.8,
        151..=250 => -0.6,
        251..=350 => -0.4,
        351..=450 => 0.,
        451..=550 => 0.23,
        551..=650 => 0.3,
        651..=750 => 0.4,
        751..=850 => 0.56,
        _ => 0.62,
    }
}

/// `NSFontManager`'s 0 to 15 for a CSS weight.
fn manager_weight(css: f32) -> i64 {
    match css as i32 {
        ..=350 => 3,
        351..=450 => 5,
        451..=550 => 6,
        551..=650 => 8,
        651..=750 => 9,
        _ => 11,
    }
}

#[derive(Default)]
struct Fonts {
    cache: HashMap<(String, i32, bool, u32), usize>,
}

impl Fonts {
    fn get(&mut self, font: &Font, size: f32) -> id {
        let italic = matches!(font.style, FontStyle::Italic | FontStyle::Oblique);
        let key = (font.family.to_string(), font.weight.0 as i32, italic, size.to_bits());
        if let Some(found) = self.cache.get(&key) {
            return *found as id;
        }
        let made = Self::make(&font.family, font.weight.0, italic, size as f64);
        let retained: id = unsafe { msg_send![made, retain] };
        self.cache.insert(key, retained as usize);
        retained
    }

    fn make(family: &str, css: f32, italic: bool, size: f64) -> id {
        unsafe {
            let system: id = msg_send![class!(NSFont), systemFontOfSize: size weight: ns_weight(css)];
            let plain: id = match family {
                ".SystemUIFont" => system,
                ".AppleSystemUIFontSerif" => {
                    let descriptor: id = msg_send![system, fontDescriptor];
                    let serif: id = msg_send![descriptor, fontDescriptorWithDesign: NSFontDescriptorSystemDesignSerif];
                    if serif == nil {
                        system
                    } else {
                        let font: id = msg_send![class!(NSFont), fontWithDescriptor: serif size: size];
                        if font == nil { system } else { font }
                    }
                }
                other => {
                    let manager: id = msg_send![class!(NSFontManager), sharedFontManager];
                    let found: id = msg_send![manager, fontWithFamily: ns(other) traits: 0u64 weight: manager_weight(css) size: size];
                    if found != nil {
                        found
                    } else {
                        let named: id = msg_send![class!(NSFont), fontWithName: ns(other) size: size];
                        if named == nil { system } else { named }
                    }
                }
            };
            if !italic {
                return plain;
            }
            let manager: id = msg_send![class!(NSFontManager), sharedFontManager];
            msg_send![manager, convertFont: plain toHaveTrait: 1u64] // NSItalicFontMask
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        for font in self.cache.values() {
            unsafe {
                let _: () = msg_send![*font as id, release];
            }
        }
    }
}

fn paragraph(min: f64, max: f64, spacing: f64, center: bool, clip: bool) -> id {
    unsafe {
        let style: id = msg_send![class!(NSMutableParagraphStyle), new];
        let _: () = msg_send![style, setMinimumLineHeight: min];
        let _: () = msg_send![style, setMaximumLineHeight: max];
        let _: () = msg_send![style, setLineSpacing: spacing];
        if center {
            let _: () = msg_send![style, setAlignment: ALIGN_CENTER];
        }
        if clip {
            let _: () = msg_send![style, setLineBreakMode: 2u64]; // NSLineBreakByClipping
        }
        msg_send![style, autorelease]
    }
}

/// `NSTextAlignment`: its values differ between Intel and Apple silicon.
const ALIGN_CENTER: i64 = if cfg!(target_arch = "x86_64") { 2 } else { 1 };
const ALIGN_RIGHT: i64 = if cfg!(target_arch = "x86_64") { 1 } else { 2 };

fn attributes(font: id, ink: id, style: Option<id>) -> id {
    unsafe {
        let dict: id = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![dict, setObject: font forKey: NSFontAttributeName];
        let _: () = msg_send![dict, setObject: ink forKey: NSForegroundColorAttributeName];
        if let Some(style) = style {
            let _: () = msg_send![dict, setObject: style forKey: NSParagraphStyleAttributeName];
        }
        dict
    }
}

fn number(value: f64) -> id {
    unsafe { msg_send![class!(NSNumber), numberWithDouble: value] }
}

fn int(value: i64) -> id {
    unsafe { msg_send![class!(NSNumber), numberWithLong: value] }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

/// Adds a span's look to its attributes.
fn add_span_look(dict: id, span: &Span) {
    unsafe {
        if let Some(underline) = span.underline {
            // NSUnderlineStyleSingle, NSUnderlineStylePatternDash.
            let style = if underline.dashed { 0x01 | 0x200 } else { 0x01 };
            let _: () = msg_send![dict, setObject: int(style) forKey: NSUnderlineStyleAttributeName];
            let _: () = msg_send![dict, setObject: color(underline.color) forKey: NSUnderlineColorAttributeName];
        }
        if let Some(strike) = span.strike {
            let _: () = msg_send![dict, setObject: int(1) forKey: NSStrikethroughStyleAttributeName];
            let _: () = msg_send![dict, setObject: color(strike) forKey: NSStrikethroughColorAttributeName];
        }
    }
}

fn append(total: id, text: &str, dict: id) {
    unsafe {
        let piece: id = msg_send![class!(NSAttributedString), alloc];
        let piece: id = msg_send![piece, initWithString: ns(text) attributes: dict];
        let _: () = msg_send![total, appendAttributedString: piece];
        let _: () = msg_send![piece, release];
    }
}

// Pictures ----------------------------------------------------------------------------

/// An image drawn at its box's top left, with its corners rounded and its caption
/// under it. Drawn straight into the PDF, so nothing is resampled.
fn picture_image(source: id, geom: media::MediaGeom, radius: f64, caption: Option<String>, ground: Option<Hsla>) -> id {
    let source = source as usize;
    let tokens = t();
    let handler = ConcreteBlock::new(move |_: NSRect| -> BOOL {
        unsafe {
            let area = rect(((geom.box_width - geom.width) / 2.) as f64, 0., geom.width as f64, geom.height as f64);
            let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
            let _: () = msg_send![mac::rounded_path(area, radius), addClip];
            if let Some(ground) = ground {
                let _: () = msg_send![color(ground), setFill];
                let _: () = msg_send![class!(NSBezierPath), fillRect: area];
            }
            if source != 0 {
                let zero = rect(0., 0., 0., 0.);
                let hints: id = nil;
                let _: () = msg_send![source as id,
                    drawInRect: area
                    fromRect: zero
                    operation: 2u64 // NSCompositingOperationSourceOver
                    fraction: 1f64
                    respectFlipped: true
                    hints: hints];
            }
            let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];
            if let Some(caption) = &caption {
                let font: id = msg_send![class!(NSFont), systemFontOfSize: 12f64];
                let dict = attributes(font, color(tokens.text3), None);
                let text = ns(caption);
                let size: NSSize = msg_send![text, sizeWithAttributes: dict];
                let x = area.origin.x + ((area.size.width - size.width) / 2.).max(0.);
                let at = NSPoint::new(x, area.size.height + media::CAPTION_GAP as f64);
                let _: () = msg_send![text, drawAtPoint: at withAttributes: dict];
            }
        }
        YES
    })
    .copy();
    unsafe {
        let size = NSSize::new(geom.box_width as f64, geom.box_height as f64);
        msg_send![class!(NSImage), imageWithSize: size flipped: true drawingHandler: &*handler]
    }
}

/// A gpui image as an `NSImage`, autoreleased, `w` x `h` points.
fn image_of_render(image: &gpui::RenderImage, w: f64, h: f64) -> id {
    let bytes = image.as_bytes(0).unwrap_or(&[]);
    let pixel = image.size(0);
    let (pw, ph) = (pixel.width.0 as i64, pixel.height.0 as i64);
    if bytes.is_empty() || pw <= 0 || ph <= 0 {
        return nil;
    }
    unsafe {
        let rep: id = msg_send![class!(NSBitmapImageRep), alloc];
        let planes: *mut *mut u8 = std::ptr::null_mut();
        let rep: id = msg_send![rep,
            initWithBitmapDataPlanes: planes
            pixelsWide: pw
            pixelsHigh: ph
            bitsPerSample: 8i64
            samplesPerPixel: 4i64
            hasAlpha: true
            isPlanar: false
            colorSpaceName: ns("NSDeviceRGBColorSpace")
            bytesPerRow: pw * 4
            bitsPerPixel: 32i64];
        if rep == nil {
            return nil;
        }
        let out: *mut u8 = msg_send![rep, bitmapData];
        let target = std::slice::from_raw_parts_mut(out, (pw * ph * 4) as usize);
        // BGRA to premultiplied RGBA.
        for (to, from) in target.chunks_exact_mut(4).zip(bytes.chunks_exact(4)) {
            let a = from[3] as u32;
            to[0] = (from[2] as u32 * a / 255) as u8;
            to[1] = (from[1] as u32 * a / 255) as u8;
            to[2] = (from[0] as u32 * a / 255) as u8;
            to[3] = from[3];
        }
        let _: () = msg_send![rep, setSize: NSSize::new(w, h)];
        let ns_image: id = msg_send![class!(NSImage), alloc];
        let ns_image: id = msg_send![ns_image, initWithSize: NSSize::new(w, h)];
        let _: () = msg_send![ns_image, addRepresentation: rep];
        let _: () = msg_send![rep, release];
        msg_send![ns_image, autorelease]
    }
}

fn attachment(image: id, x: f64, y: f64, w: f64, h: f64) -> id {
    unsafe {
        let attachment: id = msg_send![class!(NSTextAttachment), new];
        let _: () = msg_send![attachment, setImage: image];
        let _: () = msg_send![attachment, setBounds: rect(x, y, w, h)];
        let string: id = msg_send![class!(NSAttributedString), attributedStringWithAttachment: attachment];
        let _: () = msg_send![attachment, release];
        string
    }
}

// Media --------------------------------------------------------------------------------

/// What the editor's lookup does, without its loading queue: everything's on disk
/// by the time the sheet is laid out.
struct PaperMedia {
    base: Option<PathBuf>,
    library: Option<PathBuf>,
    index: std::cell::OnceCell<HashMap<String, PathBuf>>,
}

impl PaperMedia {
    fn find(&self, dest: &str, embed: bool) -> Option<PathBuf> {
        let name = if embed { media::embed_name(dest)? } else { dest };
        let exists = |p: &Path| p.exists();
        if let Some(path) = media::resolve_local_path(name, self.base.as_deref(), self.library.as_deref(), &exists).filter(|p| p.exists()) {
            return Some(path);
        }
        if !embed {
            return None;
        }
        let root = self.library.as_deref()?;
        let index = self.index.get_or_init(|| media::build_embed_index(root));
        index.get(&Path::new(name).file_name()?.to_string_lossy().to_lowercase()).cloned()
    }
}

impl MediaLookup for PaperMedia {
    fn lookup(&self, dest: &str, embed: bool) -> Lookup {
        if !embed && media::is_remote(dest) {
            let key = media::normalized_key(dest);
            if media::is_video_name(&key) {
                return Lookup::Missing;
            }
            let file = media::cache_file(&media::cache_dir(), &key);
            return match media::probe_image(&file) {
                Some(probe) => ready(&file, probe),
                None => Lookup::Missing,
            };
        }
        let Some(path) = self.find(dest, embed) else { return Lookup::Missing };
        if (embed || media::is_video_name(dest)) && media::is_video_name(&path.to_string_lossy()) {
            return Lookup::Ready(MediaInfo {
                key: path.to_string_lossy().into(),
                kind: MediaKind::Video { path },
                natural: None,
                settling: false,
            });
        }
        match media::probe_image(&path) {
            Some(probe) => ready(&path, probe),
            None => Lookup::Missing,
        }
    }
}

fn ready(path: &Path, probe: media::Probe) -> Lookup {
    Lookup::Ready(MediaInfo {
        key: path.to_string_lossy().into(),
        kind: MediaKind::Image { animated: probe.animated },
        natural: Some(probe.natural),
        settling: false,
    })
}

/// The note's remote images, to fetch before laying it out.
pub fn remote_images(markdown: &str) -> Vec<String> {
    lf_lines(markdown)
        .flat_map(media_spans)
        .filter(|span| !span.embed && media::is_remote(&span.dest))
        .map(|span| media::normalized_key(&span.dest))
        .collect()
}

/// Fetches each into the editor's cache, waiting at most 20 s for it. What doesn't
/// arrive shows as its link.
pub fn fetch_remote_images(urls: &[String]) {
    let dir = media::cache_dir();
    let _ = std::fs::create_dir_all(&dir);
    for url in urls {
        let file = media::cache_file(&dir, url);
        if file.exists() {
            continue;
        }
        let temp = file.with_extension("pdf-fetch");
        let args = media::curl_args(&media::encode_url(url), &temp, None);
        let child = std::process::Command::new("/usr/bin/curl").args(args).stdout(std::process::Stdio::null()).spawn();
        let Ok(mut child) = child else { continue };
        let start = Instant::now();
        let done = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.success(),
                Ok(None) if start.elapsed() < Duration::from_secs(20) => std::thread::sleep(Duration::from_millis(100)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break false;
                }
            }
        };
        if done && media::probe_image(&temp).is_some() {
            let _ = std::fs::rename(&temp, &file);
        } else {
            let _ = std::fs::remove_file(&temp);
        }
    }
}

// Lines ---------------------------------------------------------------------------------

struct Row {
    /// In the line's text box.
    y: f64,
    height: f64,
    glyphs: NSRange,
    baseline: f64,
}

struct SpanRange {
    span: usize,
    chars: NSRange,
}

struct LaidLine {
    display: DisplayLine,
    storage: id,
    layout: id,
    container: id,
    rows: Vec<Row>,
    spans: Vec<SpanRange>,
    height: f64,
    /// The base font's ascent and descent, and its cap height.
    metrics: (f64, f64, f64),
}

impl Drop for LaidLine {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.layout, release];
            let _: () = msg_send![self.container, release];
            let _: () = msg_send![self.storage, release];
        }
    }
}

fn font_metrics(font: id) -> (f64, f64, f64) {
    unsafe {
        let ascent: f64 = msg_send![font, ascender];
        let descent: f64 = msg_send![font, descender];
        let cap: f64 = msg_send![font, capHeight];
        (ascent, descent.abs(), cap)
    }
}

fn lay_out_line(mut d: DisplayLine, fonts: &mut Fonts, column: f64, page_height: f64) -> LaidLine {
    let base = fonts.get(&d.base_font, f32::from(d.font_size));
    let metrics = font_metrics(base);
    let width = (column - f32::from(d.indent) as f64 - f32::from(d.tail) as f64).max(40.);
    let has_box = d.spans.iter().any(|s| s.media.is_some() || s.math.is_some());
    let style = match d.metric {
        // A taller picture or formula makes its row taller.
        LineMetric::Fixed(h) => paragraph(h as f64, if has_box { 0. } else { h as f64 }, 0., d.centered, !d.wrap),
        LineMetric::Natural { spacing } => paragraph(0., 0., spacing as f64, d.centered, !d.wrap),
    };
    let text = d.text.to_string();
    let mut spans = Vec::new();
    unsafe {
        let total: id = msg_send![class!(NSMutableAttributedString), new];
        let mut at = 0u64;
        let mut cursor = 0usize;
        let plain = attributes(base, color(t().text), Some(style));
        for (index, span) in d.spans.iter_mut().enumerate() {
            if span.range.start > cursor {
                if let Some(gap) = text.get(cursor..span.range.start) {
                    append(total, gap, plain);
                    at += utf16_len(gap);
                }
            }
            cursor = span.range.end.max(cursor);
            let font = fonts.get(&span.font, f32::from(span.size));
            let dict = attributes(font, color(span.color), Some(style));
            add_span_look(dict, span);
            if let Some(picture) = &span.media {
                let string = media_string(picture, width, page_height, metrics.1);
                let _: () = msg_send![total, appendAttributedString: string];
                let length: u64 = msg_send![total, length];
                let _: () = msg_send![total, addAttribute: NSParagraphStyleAttributeName value: style range: NSRange::new(at, 1)];
                spans.push(SpanRange { span: index, chars: NSRange::new(at, length - at) });
                at = length;
                continue;
            }
            if let Some(math) = &span.math {
                let size = f32::from(span.size);
                if let Some(rendered) = ratex_gpui::render::render_latex(&math.latex, size, 3., t().text) {
                    let (w, h) = fit_size(rendered.width as f64, rendered.height as f64, width, page_height);
                    let image = image_of_render(&rendered.image, w, h);
                    let cap = font_metrics(font).2;
                    let string = attachment(image, 0., (cap - h) / 2., w, h);
                    let _: () = msg_send![total, appendAttributedString: string];
                    let length: u64 = msg_send![total, length];
                    let _: () = msg_send![total, addAttribute: NSParagraphStyleAttributeName value: style range: NSRange::new(at, length - at)];
                    spans.push(SpanRange { span: index, chars: NSRange::new(at, length - at) });
                    at = length;
                    continue;
                }
            }
            let Some(piece) = text.get(span.range.clone()) else { continue };
            append(total, piece, dict);
            let length = utf16_len(piece);
            spans.push(SpanRange { span: index, chars: NSRange::new(at, length) });
            at += length;
            let kern = f32::from(span.kern_after) as f64;
            if kern != 0. && length > 0 {
                let _: () = msg_send![total, addAttribute: NSKernAttributeName value: number(kern) range: NSRange::new(at - 1, 1)];
            }
        }
        if let Some(rest) = text.get(cursor..).filter(|r| !r.is_empty()) {
            append(total, rest, plain);
        }

        let storage: id = msg_send![class!(NSTextStorage), alloc];
        let storage: id = msg_send![storage, initWithAttributedString: total];
        let _: () = msg_send![total, release];
        let layout: id = msg_send![class!(NSLayoutManager), new];
        let container: id = msg_send![class!(NSTextContainer), alloc];
        let container: id = msg_send![container, initWithContainerSize: NSSize::new(width, 10_000_000.)];
        let _: () = msg_send![container, setLineFragmentPadding: 0f64];
        let _: () = msg_send![layout, addTextContainer: container];
        let _: () = msg_send![storage, addLayoutManager: layout];
        let _: () = msg_send![layout, ensureLayoutForTextContainer: container];

        let glyphs: NSRange = msg_send![layout, glyphRangeForTextContainer: container];
        let mut rows = Vec::new();
        let mut index = glyphs.location;
        let end = glyphs.location + glyphs.length;
        while index < end {
            let mut range = NSRange::new(0, 0);
            let fragment: NSRect = msg_send![layout, lineFragmentRectForGlyphAtIndex: index effectiveRange: &mut range];
            let location: NSPoint = msg_send![layout, locationForGlyphAtIndex: index];
            rows.push(Row { y: fragment.origin.y, height: fragment.size.height, glyphs: range, baseline: fragment.origin.y + location.y });
            let next = range.location + range.length;
            if next <= index {
                break;
            }
            index = next;
        }
        if rows.is_empty() {
            let height = match d.metric {
                LineMetric::Fixed(h) => h as f64,
                LineMetric::Natural { spacing } => metrics.0 + metrics.1 + spacing as f64,
            };
            rows.push(Row { y: 0., height, glyphs: NSRange::new(0, 0), baseline: height - metrics.1 });
        }
        let height = rows.last().map_or(0., |r| r.y + r.height);
        LaidLine { display: d, storage, layout, container, rows, spans, height, metrics }
    }
}

/// A picture, sized for the column and the page.
fn media_string(picture: &InlineMedia, width: f64, page_height: f64, descent: f64) -> id {
    let mut geom = picture.geom(width as f32);
    // A line takes more room than the picture on it.
    let budget = page_height - descent - 2.;
    if geom.box_height as f64 > budget {
        let caption = geom.box_height - geom.height;
        let (w, h) = fit_size(geom.width as f64, geom.height as f64, width, budget - caption as f64);
        geom.width = w as f32;
        geom.height = h as f32;
        geom.box_width = w as f32;
        geom.box_height = h as f32 + caption;
    }
    let caption = picture.has_caption().then(|| {
        let trimmed = picture.caption.trim();
        match &picture.info.kind {
            MediaKind::Video { path } if trimmed.is_empty() => path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            _ => trimmed.to_string(),
        }
    });
    let image = match &picture.info.kind {
        MediaKind::Image { animated } => unsafe {
            let source: id = msg_send![class!(NSImage), alloc];
            let source: id = msg_send![source, initWithContentsOfFile: ns(&picture.info.key)];
            let source: id = msg_send![source, autorelease];
            let radius = if *animated { 0. } else { media::IMAGE_RADIUS as f64 };
            picture_image(source, geom, radius, caption, None)
        },
        MediaKind::Video { .. } => picture_image(nil, geom, media::IMAGE_RADIUS as f64, caption, Some(t().video_ground)),
    };
    attachment(image, 0., 0., geom.box_width as f64, geom.box_height as f64)
}

// Tables ----------------------------------------------------------------------------------

const TABLE_PAD_H: f64 = 12.;
const TABLE_PAD_V: f64 = 8.;
const TABLE_RADIUS: f64 = 8.;
const TABLE_LINE: f64 = 19.;

struct LaidTable {
    widths: Vec<f64>,
    rows: Vec<(Vec<id>, f64)>,
    width: f64,
    height: f64,
    scale: f64,
}

impl Drop for LaidTable {
    fn drop(&mut self) {
        for (cells, _) in &self.rows {
            for cell in cells {
                unsafe {
                    let _: () = msg_send![*cell, release];
                }
            }
        }
    }
}

fn split_cells(line: &str) -> Vec<&str> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(str::trim).collect()
}

fn is_numeric(s: &str) -> bool {
    let t: String = s.trim().chars().filter(|c| *c != ',').collect();
    let t = t.trim_matches(['%', '$', '¥', '€', '+', '-']);
    !t.is_empty() && t.parse::<f64>().is_ok()
}

fn lay_out_table(src: &str, typo: &Typography, fonts: &mut Fonts, column: f64, page_height: f64) -> LaidTable {
    let lines: Vec<&str> = lf_lines(src).collect();
    let headers = split_cells(lines[0]);
    let cols = headers.len().max(1);
    let mut aligns: Vec<i64> = split_cells(lines.get(1).copied().unwrap_or(""))
        .into_iter()
        .map(|c| match (c.starts_with(':'), c.ends_with(':')) {
            (true, true) => ALIGN_CENTER,
            (false, true) => ALIGN_RIGHT,
            _ => 0,
        })
        .collect();
    aligns.resize(cols, 0);
    let body: Vec<Vec<&str>> = lines.iter().skip(2).map(|l| split_cells(l)).collect();
    let numeric: Vec<bool> = (0..cols).map(|c| !body.is_empty() && body.iter().all(|r| is_numeric(r.get(c).copied().unwrap_or("")))).collect();

    let mut natural = vec![8f32; cols];
    let mut minimum = vec![0f32; cols];
    let mut built: Vec<Vec<id>> = Vec::new();
    for (r, cells) in std::iter::once(&headers).chain(body.iter()).enumerate() {
        let mut row = Vec::new();
        for c in 0..cols {
            let (text, spans) = cell_spans(cells.get(c).copied().unwrap_or(""), r == 0, numeric[c], typo);
            let align = if aligns[c] == 0 && numeric[c] { ALIGN_RIGHT } else { aligns[c] };
            unsafe {
                let style: id = msg_send![class!(NSMutableParagraphStyle), new];
                let _: () = msg_send![style, setMinimumLineHeight: TABLE_LINE];
                let _: () = msg_send![style, setMaximumLineHeight: TABLE_LINE];
                let _: () = msg_send![style, setAlignment: align];
                let total: id = msg_send![class!(NSMutableAttributedString), new];
                let mut longest = 0f64;
                for span in &spans {
                    let Some(piece) = text.get(span.range.clone()) else { continue };
                    let font = fonts.get(&span.font, f32::from(span.size));
                    let dict = attributes(font, color(span.color), Some(style));
                    add_span_look(dict, span);
                    if let Some(fill) = span.fill {
                        let _: () = msg_send![dict, setObject: color(fill.color) forKey: NSBackgroundColorAttributeName];
                    }
                    append(total, piece, dict);
                    for word in piece.split(|ch: char| ch.is_whitespace() || fumi_editor::textlayout::is_cjk(ch)) {
                        if !word.is_empty() {
                            let size: NSSize = msg_send![ns(word), sizeWithAttributes: dict];
                            longest = longest.max(size.width.ceil());
                        }
                    }
                }
                let _: () = msg_send![style, release];
                let size: NSSize = msg_send![total, size];
                natural[c] = natural[c].max(size.width.ceil() as f32);
                minimum[c] = minimum[c].max(longest as f32);
                row.push(total);
            }
        }
        built.push(row);
    }
    let chrome = (cols + 1) as f64 + TABLE_PAD_H * 2. * cols as f64;
    let available = (cols as f64 * 30.).max(column - chrome);
    let widths: Vec<f64> = column_widths(&natural, &minimum, available as f32).into_iter().map(|w| w as f64).collect();
    let mut rows = Vec::new();
    let mut height = 1.;
    for row in built {
        let mut tallest = TABLE_LINE;
        for (cell, width) in row.iter().zip(&widths) {
            let bounds: NSRect = unsafe {
                msg_send![*cell, boundingRectWithSize: NSSize::new(*width, 10_000_000.) options: 1u64]
            };
            tallest = tallest.max(bounds.size.height.ceil());
        }
        let row_height = tallest + TABLE_PAD_V * 2. + 1.;
        height += row_height;
        rows.push((row, row_height));
    }
    let width = widths.iter().sum::<f64>() + chrome;
    let scale = (page_height / (height + 18.)).min(1.);
    LaidTable { widths, rows, width, height, scale }
}

fn draw_table(table: &LaidTable, x: f64, y: f64, context: *mut c_void) {
    let c = t();
    unsafe {
        CGContextSaveGState(context);
        CGContextTranslateCTM(context, x, y + 8. * table.scale);
        CGContextScaleCTM(context, table.scale, table.scale);
        let outline = rect(0.5, 0.5, table.width - 1., table.height - 1.);
        let path = mac::rounded_path(outline, TABLE_RADIUS);
        let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
        let _: () = msg_send![path, addClip];
        if let Some((_, header)) = table.rows.first() {
            mac::fill_rect(rect(0., 0., table.width, *header + 1.), 0., color(c.code_block));
        }
        let mut top = 1.;
        for (r, (cells, height)) in table.rows.iter().enumerate() {
            if r > 0 {
                mac::fill_rect(rect(0., top - 1., table.width, 1.), 0., color(c.table_border));
            }
            let mut left = 1.;
            for (col, (cell, width)) in cells.iter().zip(&table.widths).enumerate() {
                if col > 0 {
                    mac::fill_rect(rect(left - 1., top, 1., *height), 0., color(c.table_border));
                }
                let area = rect(left + TABLE_PAD_H, top + TABLE_PAD_V, *width, *height);
                let _: () = msg_send![*cell, drawWithRect: area options: 1u64];
                left += width + TABLE_PAD_H * 2. + 1.;
            }
            top += height;
        }
        let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];
        let _: () = msg_send![color(c.table_border), setStroke];
        let _: () = msg_send![path, setLineWidth: 1f64];
        let _: () = msg_send![path, stroke];
        CGContextRestoreGState(context);
    }
}

// Front matter -----------------------------------------------------------------------------

const FRONT_ROW: f64 = 26.;
const FRONT_KEY: f64 = 96.;

fn pill_colors(index: usize) -> (u32, u32) {
    const LIGHT: [(u32, u32); 5] = [
        (0xFBF0D9, 0x7A5410),
        (0xE8F1FB, 0x1F4E80),
        (0xFDECEE, 0x8A2F36),
        (0xE7F5EC, 0x2F6B4A),
        (0xF3EEF8, 0x6B4A8A),
    ];
    LIGHT[index % 5]
}

fn draw_front(fields: &[fumi_core::frontmatter::Field], x: f64, y: f64, width: f64) {
    let c = t();
    unsafe {
        let font: id = msg_send![class!(NSFont), systemFontOfSize: text_size::BODY as f64];
        let (ascent, descent, _) = font_metrics(font);
        let text_y = |row_top: f64| row_top + (FRONT_ROW - (ascent + descent)) / 2.;
        let mut top = y;
        if fields.is_empty() {
            let _: () = msg_send![ns("Properties"), drawAtPoint: NSPoint::new(x, text_y(top)) withAttributes: attributes(font, color(c.text3), None)];
            top += FRONT_ROW;
        }
        for field in fields {
            let keys = attributes(font, color(c.text3), None);
            let _: () = msg_send![ns(&field.key), drawInRect: rect(x, text_y(top), FRONT_KEY - 8., ascent + descent) withAttributes: keys];
            let value_x = x + FRONT_KEY;
            match &field.value {
                Value::Text(text) => {
                    let dict = attributes(font, color(c.text), None);
                    let _: () = msg_send![ns(text), drawInRect: rect(value_x, text_y(top), width - FRONT_KEY, ascent + descent) withAttributes: dict];
                }
                Value::List(items) => {
                    let mut at = value_x;
                    for (index, item) in items.iter().enumerate() {
                        let (ground, ink) = pill_colors(index);
                        let dict = attributes(font, color(hex(ink)), None);
                        let size: NSSize = msg_send![ns(item), sizeWithAttributes: dict];
                        let pill = size.width.ceil() + 12.;
                        if at + pill > x + width {
                            break;
                        }
                        mac::fill_rect(rect(at, top + 3., pill, 20.), 4., color(hex(ground)));
                        let _: () = msg_send![ns(item), drawAtPoint: NSPoint::new(at + 6., text_y(top)) withAttributes: dict];
                        at += pill + 4.;
                    }
                }
            }
            top += FRONT_ROW;
        }
        mac::fill_rect(rect(x, top + 10., width, 1.), 0., color(c.rule));
    }
}

fn front_height(count: usize) -> f64 {
    FRONT_ROW * count.max(1) as f64 + 11. + 16.
}

// The document -------------------------------------------------------------------------------

enum Piece {
    Line(LaidLine),
    Table(LaidTable),
    /// The picture, retained, and its size.
    Math(usize, f64, f64),
    /// A formula that wouldn't typeset, as its source.
    Source(String, f64),
    Front(Vec<fumi_core::frontmatter::Field>),
}

/// One thing that goes on a page whole.
struct PageRow {
    piece: usize,
    row: usize,
    top: f64,
    bottom: f64,
    /// Where the piece's text box starts.
    origin: f64,
    /// Where the line's own space begins and ends, for boxes that reach into it.
    line_top: f64,
    line_bottom: f64,
}

/// The block's language, or callout type, holding `line`.
fn block_type(lines: &[LineInfo], source: &[&str], line: usize) -> String {
    let open = match lines[line].kind {
        Kind::FenceOpen | Kind::FenceBody | Kind::FenceClose => Kind::FenceOpen,
        Kind::CalloutOpen | Kind::CalloutBody | Kind::CalloutClose => Kind::CalloutOpen,
        Kind::QuoteCalloutTitle | Kind::QuoteCalloutBody => Kind::QuoteCalloutTitle,
        _ => return String::new(),
    };
    let at = (0..=line).rev().find(|l| lines[*l].kind == open).unwrap_or(line);
    let text = source[at];
    match open {
        Kind::FenceOpen => text.trim().trim_start_matches(['`', '~']).split(' ').next().unwrap_or("").to_string(),
        Kind::CalloutOpen => text.trim().trim_start_matches(':').trim().split(' ').next().filter(|s| !s.is_empty()).unwrap_or("note").to_string(),
        _ => callout_type(quote_content(text)).unwrap_or("note").to_string(),
    }
}

fn callout_closed(lines: &[LineInfo], line: usize) -> bool {
    (0..=line).rev().find(|l| lines[*l].kind == Kind::CalloutOpen).is_some_and(|l| lines[l].real)
}

/// Lays `markdown` out and returns the PDF, or `None` when the sheet is too small to
/// hold anything. `base` is the note's folder, so relative image paths resolve.
pub fn render(markdown: &str, config: &Config, base: Option<&Path>, library: Option<&Path>, links: Option<Rc<WikiLinkIndex>>) -> Option<Vec<u8>> {
    let (column, page_height) = config.content();
    if column <= 60. || page_height <= 60. {
        return None;
    }
    load_framework("AppKit");
    // Paper is white: every themed color takes its light value.
    let screen = t();
    let paper = Tokens::resolve(settings::color_theme(), false, &settings::accent_hex());
    theme::set_active(paper);
    let out = render_on_paper(markdown, config, base, library, links, column, page_height);
    theme::set_active(screen);
    out
}

fn render_on_paper(
    markdown: &str,
    config: &Config,
    base: Option<&Path>,
    library: Option<&Path>,
    links: Option<Rc<WikiLinkIndex>>,
    column: f64,
    page_height: f64,
) -> Option<Vec<u8>> {
    let typo = Typography::new(&config.family, config.size, config.line_height);
    let source: Vec<&str> = lf_lines(markdown).collect();
    let lines = scan_all(markdown);
    let items = build_items(&lines, None);
    let title = find_title(&lines);
    let paper_media = PaperMedia {
        base: base.map(Path::to_path_buf),
        library: library.map(Path::to_path_buf),
        index: std::cell::OnceCell::new(),
    };
    let link_context = links.map(|index| LinkContext { index, folder: base.map(Path::to_path_buf) });
    let mut fonts = Fonts::default();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut rows: Vec<PageRow> = Vec::new();
    let mut y = 0.;

    let kind_at = |l: Option<usize>| l.and_then(|l| lines.get(l)).map_or(Kind::Blank, |i| i.kind);
    for item in &items {
        let start = item.start as usize;
        let end = start + item.len as usize;
        let piece = pieces.len();
        match item.kind {
            ItemKind::Line => {
                let info = lines[start];
                let kind = info.kind;
                let real = match kind {
                    Kind::CalloutBody | Kind::CalloutClose => callout_closed(&lines, start),
                    _ => info.real,
                };
                let kind_of_block = block_type(&lines, &source, start);
                let display = build_line(
                    source[start],
                    &LineCtx {
                        kind,
                        caret: None,
                        title: title == Some(start),
                        marked: None,
                        prev: kind_at(start.checked_sub(1)),
                        next: kind_at(Some(start + 1)),
                        block_type: &kind_of_block,
                        typo: &typo,
                        links: link_context.as_ref(),
                        real,
                        raw_block: false,
                        prev_has_caret: false,
                        media: Some(&paper_media),
                        reveal_media: false,
                    },
                );
                let laid = lay_out_line(display, &mut fonts, column, page_height);
                let top = f32::from(laid.display.top) as f64;
                let bottom = f32::from(laid.display.bottom) as f64;
                let origin = y + top;
                for (index, row) in laid.rows.iter().enumerate() {
                    rows.push(PageRow {
                        piece,
                        row: index,
                        top: origin + row.y,
                        bottom: origin + row.y + row.height,
                        origin,
                        line_top: y,
                        line_bottom: origin + laid.height + bottom,
                    });
                }
                y = origin + laid.height + bottom;
                pieces.push(Piece::Line(laid));
            }
            ItemKind::Table => {
                let src = source[start..end].join("\n");
                let table = lay_out_table(&src, &typo, &mut fonts, column, page_height);
                let height = (table.height + 18.) * table.scale;
                rows.push(PageRow { piece, row: 0, top: y, bottom: y + height, origin: y, line_top: y, line_bottom: y + height });
                y += height;
                pieces.push(Piece::Table(table));
            }
            ItemKind::Math => {
                let src = source[start..end].join("\n");
                let size = typo.size + 2.;
                let rendered = ratex_gpui::render::render_latex(latex_of(&src), size, 3., t().text);
                let height;
                match rendered {
                    Some(r) => {
                        let scale = (column / r.width as f64).min(1.);
                        let (w, h) = fit_size(r.width as f64 * scale, r.height as f64 * scale, column, page_height - 20.);
                        let image = image_of_render(&r.image, w, h);
                        let image: id = unsafe { msg_send![image, retain] };
                        height = h + 20.;
                        pieces.push(Piece::Math(image as usize, w, h));
                    }
                    None => {
                        height = 20. + 18.;
                        pieces.push(Piece::Source(src, 18.));
                    }
                }
                rows.push(PageRow { piece, row: 0, top: y, bottom: y + height, origin: y, line_top: y, line_bottom: y + height });
                y += height;
            }
            ItemKind::Front => {
                let src = source[start..end].join("\n");
                let fields = Frontmatter::parse(&src).map(|f| f.fields).unwrap_or_default();
                let height = front_height(fields.len());
                rows.push(PageRow { piece, row: 0, top: y, bottom: y + height, origin: y, line_top: y, line_bottom: y + height });
                y += height;
                pieces.push(Piece::Front(fields));
            }
        }
    }

    let spans: Vec<(f64, f64)> = rows.iter().map(|r| (r.top, r.bottom)).collect();
    let pages = paginate(&spans, page_height);
    let out = draw_document(&pieces, &rows, &pages, config, &mut fonts);
    for piece in &pieces {
        if let Piece::Math(image, _, _) = piece {
            unsafe {
                let _: () = msg_send![*image as id, release];
            }
        }
    }
    out
}

fn draw_document(pieces: &[Piece], rows: &[PageRow], pages: &[Vec<usize>], config: &Config, fonts: &mut Fonts) -> Option<Vec<u8>> {
    unsafe {
        let data: id = msg_send![class!(NSMutableData), data];
        let consumer = CGDataConsumerCreateWithCFData(data);
        if consumer.is_null() {
            return None;
        }
        let media_box = rect(0., 0., config.page.0, config.page.1);
        let info: id = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![info, setObject: ns(&config.title) forKey: kCGPDFContextTitle];
        let _: () = msg_send![info, setObject: ns("Fumi") forKey: kCGPDFContextCreator];
        let context = CGPDFContextCreate(consumer, &media_box, info);
        CGDataConsumerRelease(consumer);
        if context.is_null() {
            return None;
        }
        let total = pages.len();
        for (number, page) in pages.iter().enumerate() {
            CGPDFContextBeginPage(context, nil);
            CGContextSaveGState(context);
            // The sheet's top-left corner at the origin, so it's laid out as it reads.
            CGContextTranslateCTM(context, 0., config.page.1);
            CGContextScaleCTM(context, 1., -1.);
            let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
            let ns_context: id = msg_send![class!(NSGraphicsContext), graphicsContextWithCGContext: context flipped: true];
            let _: () = msg_send![class!(NSGraphicsContext), setCurrentContext: ns_context];

            if let Some(first) = page.first() {
                let shift = config.margin.0 - rows[*first].top;
                for index in page {
                    draw_behind(&rows[*index], pieces, config, shift);
                }
                for index in page {
                    draw_row(&rows[*index], pieces, config, shift, context, fonts);
                }
            }
            draw_foot(number + 1, total, config);

            let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];
            CGContextRestoreGState(context);
            CGPDFContextEndPage(context);
        }
        CGPDFContextClose(context);
        CGContextRelease(context);
        Some(bytes_of(data))
    }
}

/// The foot of every page: the note's name on the left, the page number on the right.
fn draw_foot(page: usize, total: usize, config: &Config) {
    unsafe {
        let font: id = msg_send![class!(NSFont), systemFontOfSize: 9f64];
        let dict = attributes(font, color(t().faint_text), None);
        let y = config.page.1 - config.margin.2 + 22.;
        if !config.title.is_empty() {
            let _: () = msg_send![ns(&config.title), drawAtPoint: NSPoint::new(config.margin.1, y) withAttributes: dict];
        }
        let number = ns(&format!("{page} / {total}"));
        let size: NSSize = msg_send![number, sizeWithAttributes: dict];
        let at = NSPoint::new(config.page.0 - config.margin.3 - size.width, y);
        let _: () = msg_send![number, drawAtPoint: at withAttributes: dict];
    }
}

/// A rectangle rounded only where asked.
fn fill_partly_rounded(area: NSRect, radius: f64, top: bool, bottom: bool, fill: id) {
    unsafe {
        if top && bottom {
            mac::fill_rect(area, radius, fill);
            return;
        }
        if !top && !bottom {
            mac::fill_rect(area, 0., fill);
            return;
        }
        let _: () = msg_send![class!(NSGraphicsContext), saveGraphicsState];
        let _: () = msg_send![class!(NSBezierPath), clipRect: area];
        let (y, h) = if top {
            (area.origin.y, area.size.height + radius * 2.)
        } else {
            (area.origin.y - radius * 2., area.size.height + radius * 2.)
        };
        mac::fill_rect(rect(area.origin.x, y, area.size.width, h), radius, fill);
        let _: () = msg_send![class!(NSGraphicsContext), restoreGraphicsState];
    }
}

/// Boxes and pills, under every line's text.
fn draw_behind(row: &PageRow, pieces: &[Piece], config: &Config, shift: f64) {
    let Piece::Line(laid) = &pieces[row.piece] else { return };
    let d = &laid.display;
    let column_x = config.margin.1;
    let (column, _) = config.content();
    let is_first = row.row == 0;
    let is_last = row.row + 1 == laid.rows.len();
    if let Some(b) = d.boxed {
        let pad = BLOCK_PADDING as f64;
        let text_top = row.origin;
        let text_bottom = row.origin + laid.height;
        let y0 = match (is_first, b.first) {
            (true, true) => text_top - pad,
            (true, false) => row.line_top,
            _ => row.top,
        };
        let y1 = match (is_last, b.last) {
            (true, true) => text_bottom + pad,
            (true, false) => row.line_bottom,
            _ => row.bottom,
        };
        let area = rect(column_x, y0 + shift, column, y1 - y0);
        fill_partly_rounded(area, BLOCK_RADIUS as f64, is_first && b.first, is_last && b.last, color(b.fill));
    }
    let this = &laid.rows[row.row];
    let text_x = column_x + f32::from(d.indent) as f64;
    for span_range in &laid.spans {
        let span = &d.spans[span_range.span];
        let Some(fill) = span.fill else { continue };
        unsafe {
            let actual: *mut NSRange = std::ptr::null_mut();
            let glyphs: NSRange = msg_send![laid.layout, glyphRangeForCharacterRange: span_range.chars actualCharacterRange: actual];
            let start = glyphs.location.max(this.glyphs.location);
            let end = (glyphs.location + glyphs.length).min(this.glyphs.location + this.glyphs.length);
            if start >= end {
                continue;
            }
            let bounds: NSRect = msg_send![laid.layout, boundingRectForGlyphRange: NSRange::new(start, end - start) inTextContainer: laid.container];
            let mut fonts = Fonts::default();
            let (ascent, descent, _) = font_metrics(fonts.get(&span.font, f32::from(span.size)));
            let base = row.origin + this.baseline + shift;
            let x0 = text_x + bounds.origin.x - fill.pad as f64;
            let x1 = text_x + bounds.origin.x + bounds.size.width;
            let area = rect(x0, base - ascent - fill.inset as f64, x1 - x0, ascent + descent + fill.inset as f64 * 2.);
            mac::fill_rect(area, fill.radius as f64, color(fill.color));
        }
    }
}

fn draw_row(row: &PageRow, pieces: &[Piece], config: &Config, shift: f64, context: *mut c_void, fonts: &mut Fonts) {
    let column_x = config.margin.1;
    let (column, _) = config.content();
    let y = row.top + shift;
    match &pieces[row.piece] {
        Piece::Line(laid) => draw_line(row, laid, column_x, column, shift, fonts),
        Piece::Table(table) => draw_table(table, column_x, y, context),
        Piece::Math(image, w, h) => unsafe {
            let area = rect(column_x + (column - w) / 2., y + 10., *w, *h);
            let zero = rect(0., 0., 0., 0.);
            let hints: id = nil;
            let _: () = msg_send![*image as id,
                drawInRect: area
                fromRect: zero
                operation: 2u64
                fraction: 1f64
                respectFlipped: true
                hints: hints];
        },
        Piece::Source(source, _) => unsafe {
            let font: id = fonts.get(&mono_font(), 12.);
            let dict = attributes(font, color(t().text3), None);
            let _: () = msg_send![ns(source), drawAtPoint: NSPoint::new(column_x, y + 10.) withAttributes: dict];
        },
        Piece::Front(fields) => draw_front(fields, column_x, y, column),
    }
}

fn draw_line(row: &PageRow, laid: &LaidLine, column_x: f64, column: f64, shift: f64, fonts: &mut Fonts) {
    let d = &laid.display;
    let c = t();
    let this = &laid.rows[row.row];
    let text_x = column_x + f32::from(d.indent) as f64;
    let top_y = row.origin + shift;
    unsafe {
        if this.glyphs.length > 0 {
            let _: () = msg_send![laid.layout, drawGlyphsForGlyphRange: this.glyphs atPoint: NSPoint::new(text_x, top_y)];
        }
        // Quote bars, one per level.
        for level in 0..d.quote_depth {
            let x = column_x + level as f64 * QUOTE_INDENT as f64;
            let ink = if level == 0 { c.text } else { c.nested_quote_bar };
            let y0 = row.top + shift + if row.row == 0 { 2. } else { 0. };
            mac::fill_rect(rect(x, y0, 3., row.bottom + shift - y0), 0., color(ink));
        }
        if row.row != 0 {
            return;
        }
        // What stands in for a list item's syntax.
        if let Some(glyph) = &d.list {
            let base = top_y + this.baseline;
            let center_y = base - laid.metrics.2 / 2.;
            let slot = text_x - LIST_SLOT as f64;
            let body = f32::from(d.font_size) as f64;
            match glyph {
                ListGlyph::Bullet(shape) => {
                    let cx = slot + 7.;
                    let at = |w: f64| rect(cx - w / 2., center_y - w / 2., w, w);
                    match shape {
                        BulletShape::Disc => {
                            let w = (body * 0.33).round().max(5.);
                            mac::fill_rect(at(w), w / 2., color(c.text));
                        }
                        BulletShape::Ring => {
                            let w = (body * 0.33).round().max(5.);
                            let inset = rect(cx - w / 2. + 0.6, center_y - w / 2. + 0.6, w - 1.2, w - 1.2);
                            let path = mac::rounded_path(inset, (w - 1.2) / 2.);
                            let _: () = msg_send![color(c.text), setStroke];
                            let _: () = msg_send![path, setLineWidth: 1.2f64];
                            let _: () = msg_send![path, stroke];
                        }
                        BulletShape::Square => {
                            let w = (body * 0.28).round().max(4.);
                            mac::fill_rect(at(w), 0., color(c.text));
                        }
                    }
                }
                ListGlyph::Ordered(label) => {
                    let font: id = msg_send![class!(NSFont), monospacedDigitSystemFontOfSize: body weight: weight::REGULAR];
                    let dict = attributes(font, color(c.text), None);
                    let text = ns(label);
                    let size: NSSize = msg_send![text, sizeWithAttributes: dict];
                    let (ascent, _, _) = font_metrics(font);
                    let at = NSPoint::new(text_x - 6. - size.width, base - ascent);
                    let _: () = msg_send![text, drawAtPoint: at withAttributes: dict];
                }
                ListGlyph::Task { checked } => {
                    let area = rect(slot + 1., center_y - 8., 16., 16.);
                    if *checked {
                        mac::fill_rect(area, 4., color(c.accent));
                        let (left, right, top) = (area.origin.x, area.origin.x + 16., area.origin.y);
                        let mid = top + 8.;
                        let path: id = msg_send![class!(NSBezierPath), bezierPath];
                        let _: () = msg_send![path, moveToPoint: NSPoint::new(left + 4., mid - 0.5)];
                        let _: () = msg_send![path, lineToPoint: NSPoint::new(left + 7., mid + 2.5)];
                        let _: () = msg_send![path, lineToPoint: NSPoint::new(right - 3.5, top + 5.)];
                        let _: () = msg_send![path, setLineWidth: 2f64];
                        let _: () = msg_send![color(gpui::white()), setStroke];
                        let _: () = msg_send![path, stroke];
                    } else {
                        let inset = rect(area.origin.x + 0.75, area.origin.y + 0.75, 14.5, 14.5);
                        let path = mac::rounded_path(inset, 3.25);
                        let _: () = msg_send![color(c.checkbox_border), setStroke];
                        let _: () = msg_send![path, setLineWidth: 1.5f64];
                        let _: () = msg_send![path, stroke];
                    }
                }
            }
        }
        // A horizontal rule across the column.
        if d.rule {
            let y = (top_y + laid.height / 2.).round();
            mac::fill_rect(rect(column_x, y, column, 1.), 0., color(c.rule));
        }
        // Callout icon.
        if let Some((raw, ink)) = &d.callout_icon {
            let symbol: id = msg_send![class!(NSImage),
                imageWithSystemSymbolName: ns(callout_symbol(raw))
                accessibilityDescription: nil];
            if symbol != nil {
                let colors: id = msg_send![class!(NSArray), arrayWithObject: color(*ink)];
                let sized: id = msg_send![class!(NSImageSymbolConfiguration), configurationWithPointSize: 13f64 weight: weight::SEMIBOLD];
                let tinted: id = msg_send![class!(NSImageSymbolConfiguration), configurationWithPaletteColors: colors];
                let config: id = msg_send![sized, configurationByApplyingConfiguration: tinted];
                let image: id = msg_send![symbol, imageWithSymbolConfiguration: config];
                if image != nil {
                    let size: NSSize = msg_send![image, size];
                    let cy = top_y + this.baseline - laid.metrics.2 / 2.;
                    let area = rect(column_x + 14. + (16. - size.width) / 2., cy - size.height / 2., size.width, size.height);
                    let zero = rect(0., 0., 0., 0.);
                    let hints: id = nil;
                    let _: () = msg_send![image,
                        drawInRect: area
                        fromRect: zero
                        operation: 2u64
                        fraction: 1f64
                        respectFlipped: true
                        hints: hints];
                }
            }
        }
        // The code block's language, top right.
        if let Some((language, above)) = &d.code_language {
            let font = fonts.get(&ui_font(), 12.);
            let dict = attributes(font, color(c.text3), None);
            let text = ns(language);
            let size: NSSize = msg_send![text, sizeWithAttributes: dict];
            let at = NSPoint::new(column_x + column - 12. - size.width, top_y - *above as f64);
            let _: () = msg_send![text, drawAtPoint: at withAttributes: dict];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(title: &str) -> Config {
        Config::new("System".to_string(), 16., 1.75, title.to_string())
    }

    /// Reads the PDF back as PDFKit does: page count and each page's text.
    fn read(data: &[u8]) -> (usize, Vec<String>, String, String) {
        load_framework("PDFKit");
        unsafe {
            let doc: id = msg_send![objc::runtime::Class::get("PDFDocument").unwrap(), alloc];
            let doc: id = msg_send![doc, initWithData: mac::ns_data(data)];
            assert!(doc != nil, "PDFKit can't read it");
            let count: u64 = msg_send![doc, pageCount];
            let pages = (0..count)
                .map(|i| {
                    let page: id = msg_send![doc, pageAtIndex: i];
                    mac::from_ns(msg_send![page, string]).unwrap_or_default()
                })
                .collect();
            let attributes: id = msg_send![doc, documentAttributes];
            let title = mac::from_ns(msg_send![attributes, objectForKey: ns("Title")]).unwrap_or_default();
            let creator = mac::from_ns(msg_send![attributes, objectForKey: ns("Creator")]).unwrap_or_default();
            (count as usize, pages, title, creator)
        }
    }

    #[test]
    fn pagination_never_splits_a_row() {
        let rows: Vec<(f64, f64)> = (0..10).map(|i| (i as f64 * 30., (i + 1) as f64 * 30.)).collect();
        let pages = paginate(&rows, 100.);
        assert_eq!(pages[0], [0, 1, 2]);
        assert_eq!(pages[1], [3, 4, 5]);
        assert_eq!(pages.iter().flatten().count(), 10);
        // One taller than the page goes on a page of its own, whole.
        assert_eq!(paginate(&[(0., 10.), (10., 500.), (500., 510.)], 100.), [vec![0], vec![1], vec![2]]);
    }

    #[test]
    fn an_empty_note_still_gets_a_sheet() {
        assert_eq!(paginate(&[], 100.), [Vec::<usize>::new()]);
        let (count, pages, _, _) = read(&render("", &config("Empty"), None, None, None).unwrap());
        assert_eq!(count, 1);
        assert!(pages[0].contains("Empty"));
        assert!(pages[0].contains("1 / 1"));
    }

    #[test]
    fn shrinking_keeps_the_shape() {
        assert_eq!(fit_size(100., 200., 500., 100.), (50., 100.));
        assert_eq!(fit_size(100., 200., 500., 500.), (100., 200.));
    }

    #[test]
    fn a_too_small_sheet_is_refused() {
        let mut small = config("x");
        small.page = (100., 100.);
        assert!(render("x", &small, None, None, None).is_none());
    }

    #[test]
    fn the_text_has_no_markdown_marks() {
        let note = "# Title\n\n## Lists\n\n- **bold** item\n- [ ] a task\n\n1. first\n\n> quoted\n\n`code`\n\n~~gone~~\n\n---\n\n```rust\nlet x = 1;\n```\n\n> [!note] Heads up\n> inside\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n$$\nx^2\n$$\n";
        let (_, pages, title, creator) = read(&render(note, &config("My Note"), None, None, None).unwrap());
        assert_eq!((title.as_str(), creator.as_str()), ("My Note", "Fumi"));
        let text = &pages.join("\n");
        for word in ["Title", "Lists", "bold", "item", "a task", "first", "quoted", "code", "gone", "let x = 1;", "Heads up", "inside", "A", "My Note", "1 / "] {
            assert!(text.contains(word), "{word} is missing from {text:?}");
        }
        for mark in ["## ", "**", "- [ ]", "~~", "```", "$$", "[!note]"] {
            assert!(!text.contains(mark), "{mark} shows in {text:?}");
        }
    }

    #[test]
    fn a_long_note_breaks_into_numbered_pages_in_order() {
        let note: String = (0..400).map(|i| format!("Paragraph number {i}.\n\n")).collect();
        let (count, pages, _, _) = read(&render(&note, &config("Long"), None, None, None).unwrap());
        assert!(count > 3, "{count} pages");
        for (i, page) in pages.iter().enumerate() {
            assert!(page.contains(&format!("{} / {count}", i + 1)), "foot of page {i}");
        }
        let all = pages.join("\n");
        let mut at = 0;
        for i in 0..400 {
            let found = all[at..].find(&format!("Paragraph number {i}.")).unwrap_or_else(|| panic!("{i} dropped"));
            at += found;
        }
    }

    #[test]
    fn the_foot_and_the_type_survive_cjk() {
        let (_, pages, _, _) = read(&render("# 标题\n\n中文段落，含 **粗体**。", &config("笔记"), None, None, None).unwrap());
        // CoreGraphics maps some Han glyphs back to their compatibility forms.
        assert!(pages[0].contains("段落"), "{}", pages[0]);
        assert!(pages[0].contains("笔记"));
    }

    #[test]
    fn a_tall_image_is_scaled_to_the_page() {
        let dir = std::env::temp_dir().join(format!("fumi-publish-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A PNG 100 wide and 4000 tall.
        let mut rep_png = Vec::new();
        unsafe {
            let rep: id = msg_send![class!(NSBitmapImageRep), alloc];
            let planes: *mut *mut u8 = std::ptr::null_mut();
            let rep: id = msg_send![rep,
                initWithBitmapDataPlanes: planes pixelsWide: 100i64 pixelsHigh: 4000i64 bitsPerSample: 8i64 samplesPerPixel: 4i64
                hasAlpha: true isPlanar: false colorSpaceName: ns("NSDeviceRGBColorSpace") bytesPerRow: 0i64 bitsPerPixel: 0i64];
            let empty: id = msg_send![class!(NSDictionary), dictionary];
            let data: id = msg_send![rep, representationUsingType: 4u64 properties: empty];
            rep_png.extend(bytes_of(data));
        }
        std::fs::write(dir.join("tall.png"), &rep_png).unwrap();
        let (count, pages, _, _) = read(&render("before\n\n![](tall.png)\n\nafter", &config("T"), Some(&dir), None, None).unwrap());
        assert!(count <= 3, "{count} pages");
        assert!(pages.join("").contains("after"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_images_are_found() {
        assert_eq!(remote_images("a ![x](https://a.com/p.png) b\n![y](local.png)\n`![z](https://b.com/q.png)`"), ["https://a.com/p.png"]);
    }

    /// Writes a sample where `FUMI_PDF_SAMPLE` says, to look at.
    #[test]
    fn sample_for_a_look() {
        let Ok(path) = std::env::var("FUMI_PDF_SAMPLE") else { return };
        let note = "---\ntitle: Sample\ntags: [one, two]\n---\n\n# A Sample Note\n\nBody text with **bold**, *italic*, `inline code`, ~~struck~~ and a [link](https://example.com). 中文和 English 混排，看看行高。\n\n## Lists\n\n- first\n- second\n  - nested\n\n1. one\n2. two\n\n- [ ] open task\n- [x] done task\n\n> A quote\n> > nested\n\n> [!warning] Careful\n> Inside the callout.\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n---\n\n| Name | Count |\n| ---- | ----: |\n| a | 1 |\n| b | 22 |\n\nInline $x^2$ math.\n\n$$\n\\frac{a}{b}\n$$\n";
        std::fs::write(path, render(note, &config("Sample"), None, None, None).unwrap()).unwrap();
    }
}
