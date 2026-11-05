//! Fumi's own line glyphs, and SF Symbols, as images any element can show. The
//! glyphs are `ShiyeIcon.swift`'s paths on its 24-unit grid, stroked at 1.7 with
//! round caps and joins by Core Graphics, so they match the Swift app's exactly.

use crate::native::SymbolCache;
use gpui::prelude::*;
use gpui::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Glyph {
    Sidebar,
    NewNote,
    Note,
    Library,
    Tag,
    Memo,
    Image,
    Video,
    File,
    Info,
    More,
    Phone,
    Back,
    Forward,
    /// The tree's line-drawn folders, stroked thinner than the rest (1.15).
    FolderClosed,
    FolderOpen,
}

type Ctx = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    w: f64,
    h: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    o: CGPoint,
    s: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(s: *mut c_void);
    fn CGBitmapContextCreate(
        data: *mut c_void,
        w: usize,
        h: usize,
        bits: usize,
        row: usize,
        space: *mut c_void,
        info: u32,
    ) -> Ctx;
    fn CGContextRelease(c: Ctx);
    fn CGContextScaleCTM(c: Ctx, x: f64, y: f64);
    fn CGContextTranslateCTM(c: Ctx, x: f64, y: f64);
    fn CGContextSetRGBStrokeColor(c: Ctx, r: f64, g: f64, b: f64, a: f64);
    fn CGContextSetLineWidth(c: Ctx, w: f64);
    fn CGContextSetLineCap(c: Ctx, cap: i32);
    fn CGContextSetLineJoin(c: Ctx, join: i32);
    fn CGContextMoveToPoint(c: Ctx, x: f64, y: f64);
    fn CGContextAddLineToPoint(c: Ctx, x: f64, y: f64);
    fn CGContextAddQuadCurveToPoint(c: Ctx, cx: f64, cy: f64, x: f64, y: f64);
    fn CGContextClosePath(c: Ctx);
    fn CGContextAddEllipseInRect(c: Ctx, r: CGRect);
    fn CGContextAddPath(c: Ctx, p: *const c_void);
    fn CGContextStrokePath(c: Ctx);
    fn CGPathCreateWithRoundedRect(r: CGRect, w: f64, h: f64, t: *const c_void) -> *const c_void;
    fn CGPathRelease(p: *const c_void);
}

struct Pen {
    c: Ctx,
    k: f64,
}

impl Pen {
    fn m(&self, x: f64, y: f64) {
        unsafe { CGContextMoveToPoint(self.c, x * self.k, y * self.k) }
    }
    fn l(&self, x: f64, y: f64) {
        unsafe { CGContextAddLineToPoint(self.c, x * self.k, y * self.k) }
    }
    fn q(&self, x: f64, y: f64, cx: f64, cy: f64) {
        unsafe { CGContextAddQuadCurveToPoint(self.c, cx * self.k, cy * self.k, x * self.k, y * self.k) }
    }
    fn z(&self) {
        unsafe { CGContextClosePath(self.c) }
    }
    fn line(&self, pts: &[(f64, f64)]) {
        self.m(pts[0].0, pts[0].1);
        for p in &pts[1..] {
            self.l(p.0, p.1);
        }
    }
    fn rrect(&self, x: f64, y: f64, w: f64, h: f64, r: f64) {
        let k = self.k;
        let rect = CGRect { o: CGPoint { x: x * k, y: y * k }, s: CGSize { w: w * k, h: h * k } };
        unsafe {
            let p = CGPathCreateWithRoundedRect(rect, r * k, r * k, std::ptr::null());
            CGContextAddPath(self.c, p);
            CGPathRelease(p);
        }
    }
    fn oval(&self, x: f64, y: f64, w: f64, h: f64) {
        let k = self.k;
        let rect = CGRect { o: CGPoint { x: x * k, y: y * k }, s: CGSize { w: w * k, h: h * k } };
        unsafe { CGContextAddEllipseInRect(self.c, rect) }
    }
    fn page(&self) {
        self.m(14., 3.5);
        self.l(7., 3.5);
        self.q(5., 5.5, 5., 3.5);
        self.l(5., 18.5);
        self.q(7., 20.5, 5., 20.5);
        self.l(17., 20.5);
        self.q(19., 18.5, 19., 20.5);
        self.l(19., 8.5);
        self.z();
        self.line(&[(14., 3.5), (14., 8.5), (19., 8.5)]);
    }
}

fn draw(glyph: Glyph, p: &Pen) {
    match glyph {
        Glyph::Sidebar => {
            p.rrect(3., 4.5, 18., 15., 3.);
            p.line(&[(9.5, 4.5), (9.5, 19.5)]);
        }
        Glyph::NewNote => {
            p.m(12., 20.5);
            p.l(7., 20.5);
            p.q(5., 18.5, 5., 20.5);
            p.l(5., 5.5);
            p.q(7., 3.5, 5., 3.5);
            p.l(13., 3.5);
            p.l(18., 8.5);
            p.l(18., 12.);
            p.line(&[(13., 3.5), (13., 8.5), (18., 8.5)]);
            p.line(&[(18., 15.5), (18., 20.5)]);
            p.line(&[(15.5, 18.), (20.5, 18.)]);
        }
        Glyph::Note => {
            p.page();
            p.line(&[(8.5, 13.), (15.5, 13.)]);
            p.line(&[(8.5, 16.5), (13., 16.5)]);
        }
        Glyph::Library => {
            p.m(9., 3.5);
            p.l(17.5, 3.5);
            p.q(19.5, 5.5, 19.5, 3.5);
            p.l(19.5, 16.);
            p.rrect(4.5, 6.5, 12., 14., 2.);
            p.line(&[(7.5, 11.5), (13.5, 11.5)]);
            p.line(&[(7.5, 15.), (11.5, 15.)]);
        }
        Glyph::Memo => {
            p.m(7., 4.5);
            p.l(17., 4.5);
            p.q(19., 6.5, 19., 4.5);
            p.l(19., 14.5);
            p.q(17., 16.5, 19., 16.5);
            p.l(11., 16.5);
            p.l(7., 19.5);
            p.l(7., 16.5);
            p.q(5., 14.5, 5., 16.5);
            p.l(5., 6.5);
            p.q(7., 4.5, 5., 4.5);
            p.z();
            p.line(&[(8.5, 9.), (15.5, 9.)]);
            p.line(&[(8.5, 12.), (13., 12.)]);
        }
        Glyph::Tag => {
            p.line(&[(9.5, 4.), (8., 20.)]);
            p.line(&[(16., 4.), (14.5, 20.)]);
            p.line(&[(4.5, 9.), (19.5, 9.)]);
            p.line(&[(4., 15.), (19., 15.)]);
        }
        Glyph::Image => {
            p.rrect(3.5, 5., 17., 14., 2.5);
            p.line(&[(3.5, 16.5), (8.5, 11.5), (13., 16.)]);
            p.line(&[(11.5, 14.5), (14., 12.), (20.5, 18.)]);
            p.oval(14.6, 7.6, 2.4, 2.4);
        }
        Glyph::Video => {
            p.rrect(3.5, 5., 17., 14., 2.5);
            p.m(10.5, 9.2);
            p.l(15., 12.);
            p.l(10.5, 14.8);
            p.z();
        }
        Glyph::File => p.page(),
        Glyph::Info => {
            p.oval(3., 3., 18., 18.);
            p.line(&[(12., 11.), (12., 16.5)]);
            p.line(&[(12., 7.6), (12., 7.65)]);
        }
        Glyph::Phone => {
            p.rrect(6.5, 2.5, 11., 19., 2.5);
            p.line(&[(10.5, 18.5), (13.5, 18.5)]);
        }
        Glyph::Back => p.line(&[(14.5, 6.), (8.5, 12.), (14.5, 18.)]),
        Glyph::Forward => p.line(&[(9.5, 6.), (15.5, 12.), (9.5, 18.)]),
        Glyph::FolderClosed => {
            p.m(3., 7.);
            p.q(5., 5., 3., 5.);
            p.l(9., 5.);
            p.l(11., 7.);
            p.l(19., 7.);
            p.q(21., 9., 21., 7.);
            p.l(21., 17.);
            p.q(19., 19., 21., 19.);
            p.l(5., 19.);
            p.q(3., 17., 3., 19.);
            p.z();
        }
        Glyph::FolderOpen => {
            // The back panel with its tab...
            p.m(3., 19.);
            p.l(3., 7.);
            p.q(5., 5., 3., 5.);
            p.l(9., 5.);
            p.l(11., 7.);
            p.l(18., 7.);
            p.q(20., 9., 20., 7.);
            p.l(20., 10.);
            // ...and the front flap, tilted open.
            p.m(3., 19.);
            p.l(5.6, 11.4);
            p.q(7.5, 10., 6.1, 10.);
            p.l(21., 10.);
            p.l(18.6, 17.6);
            p.q(16.7, 19., 18.2, 19.);
            p.z();
        }
        Glyph::More => {
            for x in [5.5, 12., 18.5] {
                p.oval(x - 0.6, 11.4, 1.2, 1.2);
            }
        }
    }
}

const SCALE: f64 = 2.;

fn rasterize(glyph: Glyph, size: f32, color: Hsla) -> Option<Arc<RenderImage>> {
    let px_size = (size as f64 * SCALE).ceil() as usize;
    let mut pixels = vec![0u8; px_size * px_size * 4];
    let c = Rgba::from(color);
    unsafe {
        let space = CGColorSpaceCreateDeviceRGB();
        // RGBA premultiplied.
        let ctx = CGBitmapContextCreate(pixels.as_mut_ptr() as *mut c_void, px_size, px_size, 8, px_size * 4, space, 1);
        CGColorSpaceRelease(space);
        if ctx.is_null() {
            return None;
        }
        // Top-left origin, like the Swift path code.
        CGContextTranslateCTM(ctx, 0., px_size as f64);
        CGContextScaleCTM(ctx, SCALE, -SCALE);
        let k = size as f64 / 24.;
        CGContextSetRGBStrokeColor(ctx, c.r as f64, c.g as f64, c.b as f64, c.a as f64);
        let folder = matches!(glyph, Glyph::FolderClosed | Glyph::FolderOpen);
        CGContextSetLineWidth(ctx, if folder { 1.15 } else { 1.7 * k });
        CGContextSetLineCap(ctx, if folder { 0 } else { 1 });
        CGContextSetLineJoin(ctx, 1);
        draw(glyph, &Pen { c: ctx, k });
        CGContextStrokePath(ctx);
        CGContextRelease(ctx);
    }
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(px_size as u32, px_size as u32, pixels)?;
    Some(Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(buffer)])))
}

/// A Fumi glyph as a template `NSImage`, for the menu bar: AppKit tints it.
/// The caller owns the image.
pub fn template_image(g: Glyph, size: f32) -> cocoa::base::id {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSSize;
    use objc::{class, msg_send, sel, sel_impl};
    let px_size = (size as f64 * SCALE).ceil() as usize;
    let mut pixels = vec![0u8; px_size * px_size * 4];
    unsafe {
        let space = CGColorSpaceCreateDeviceRGB();
        let ctx = CGBitmapContextCreate(pixels.as_mut_ptr() as *mut c_void, px_size, px_size, 8, px_size * 4, space, 1);
        CGColorSpaceRelease(space);
        if ctx.is_null() {
            return nil;
        }
        CGContextTranslateCTM(ctx, 0., px_size as f64);
        CGContextScaleCTM(ctx, SCALE, -SCALE);
        let k = size as f64 / 24.;
        CGContextSetRGBStrokeColor(ctx, 0., 0., 0., 1.);
        CGContextSetLineWidth(ctx, 1.7 * k);
        CGContextSetLineCap(ctx, 1);
        CGContextSetLineJoin(ctx, 1);
        draw(g, &Pen { c: ctx, k });
        CGContextStrokePath(ctx);
        CGContextRelease(ctx);

        let rep: id = msg_send![class!(NSBitmapImageRep), alloc];
        let null: *mut *mut u8 = std::ptr::null_mut();
        let rep: id = msg_send![rep,
            initWithBitmapDataPlanes: null
            pixelsWide: px_size as i64
            pixelsHigh: px_size as i64
            bitsPerSample: 8i64
            samplesPerPixel: 4i64
            hasAlpha: true
            isPlanar: false
            colorSpaceName: crate::native::ns("NSDeviceRGBColorSpace")
            bytesPerRow: (px_size * 4) as i64
            bitsPerPixel: 32i64];
        if rep == nil {
            return nil;
        }
        let data: *mut u8 = msg_send![rep, bitmapData];
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), data, pixels.len());
        let points = NSSize::new(size as f64, size as f64);
        let _: () = msg_send![rep, setSize: points];
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithSize: points];
        let _: () = msg_send![image, addRepresentation: rep];
        let _: () = msg_send![rep, release];
        let _: () = msg_send![image, setTemplate: true];
        image
    }
}

thread_local! {
    static GLYPHS: RefCell<HashMap<(Glyph, u32, [u32; 4]), Option<Arc<RenderImage>>>> = RefCell::new(HashMap::new());
    static SYMBOLS: RefCell<SymbolCache> = RefCell::new(SymbolCache::default());
}

/// A Fumi glyph at `size` points, in `color`.
pub fn glyph(g: Glyph, size: f32, color: Hsla) -> AnyElement {
    let key = (
        g,
        size.to_bits(),
        [color.h.to_bits(), color.s.to_bits(), color.l.to_bits(), color.a.to_bits()],
    );
    let image = GLYPHS.with(|m| m.borrow_mut().entry(key).or_insert_with(|| rasterize(g, size, color)).clone());
    match image {
        Some(image) => img(image).w(px(size)).h(px(size)).flex_none().into_any_element(),
        None => div().w(px(size)).h(px(size)).into_any_element(),
    }
}

/// An SF Symbol by name, in `color`. Sized by the symbol itself at `size` points.
pub fn symbol(name: &str, size: f32, weight: f64, color: Hsla) -> AnyElement {
    let image = SYMBOLS.with(|m| m.borrow_mut().get(name, size, weight, color, 2.));
    match image {
        Some((image, (w, h))) => img(image).w(px(w)).h(px(h)).flex_none().into_any_element(),
        None => div().w(px(size)).h(px(size)).into_any_element(),
    }
}
