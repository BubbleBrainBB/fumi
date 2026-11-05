//! What the general pasteboard holds, for pasting media, and how imported files
//! go into a note. `WysiwygTextView.pastedImage` and `insertImported`.

use crate::media::with_pool;
use cocoa::base::{id, nil};
use fumi_ui::native::ns;
use objc::{class, msg_send, sel, sel_impl};
use std::path::PathBuf;

/// The pasteboard's contents, as far as paste cares.
#[derive(Default)]
pub struct PastedMedia {
    /// Whether it holds file URLs at all, media or not.
    pub has_files: bool,
    pub files: Vec<PathBuf>,
    /// Image data in its own format, with the extension to save it under. None
    /// when there's text too: Word and Pages put a picture of copied text there.
    pub image: Option<(Vec<u8>, &'static str)>,
}

fn data_of(pasteboard: id, kind: &str) -> Option<Vec<u8>> {
    unsafe {
        let data: id = msg_send![pasteboard, dataForType: ns(kind)];
        if data == nil {
            return None;
        }
        let length: usize = msg_send![data, length];
        let bytes: *const u8 = msg_send![data, bytes];
        (!bytes.is_null() && length > 0).then(|| std::slice::from_raw_parts(bytes, length).to_vec())
    }
}

fn tiff_to_png(tiff: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let data: id = msg_send![class!(NSData), dataWithBytes: tiff.as_ptr() length: tiff.len()];
        let rep: id = msg_send![class!(NSBitmapImageRep), imageRepWithData: data];
        if rep == nil {
            return None;
        }
        let empty: id = msg_send![class!(NSDictionary), dictionary];
        // NSBitmapImageFileTypePNG.
        let png: id = msg_send![rep, representationUsingType: 4u64 properties: empty];
        if png == nil {
            return None;
        }
        let length: usize = msg_send![png, length];
        let bytes: *const u8 = msg_send![png, bytes];
        Some(std::slice::from_raw_parts(bytes, length).to_vec())
    }
}

pub fn read_general() -> PastedMedia {
    with_pool(|| {
        let pasteboard: id = unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
        read_from(pasteboard)
    })
}

/// A private pasteboard by name, for trying this without touching the user's.
pub fn named_pasteboard(name: &str) -> id {
    unsafe { msg_send![class!(NSPasteboard), pasteboardWithName: ns(name)] }
}

pub fn read_from(pasteboard: id) -> PastedMedia {
    unsafe {
        let classes: id = msg_send![class!(NSArray), arrayWithObject: class!(NSURL)];
        let options: id = msg_send![class!(NSMutableDictionary), dictionary];
        let yes: id = msg_send![class!(NSNumber), numberWithBool: true as objc::runtime::BOOL];
        let _: () = msg_send![options, setObject: yes forKey: ns("NSPasteboardURLReadingFileURLsOnlyKey")];
        let mut out = PastedMedia::default();
        let can: bool = msg_send![pasteboard, canReadObjectForClasses: classes options: options];
        out.has_files = can;
        if can {
            let urls: id = msg_send![pasteboard, readObjectsForClasses: classes options: options];
            let count: usize = if urls == nil { 0 } else { msg_send![urls, count] };
            for i in 0..count {
                let url: id = msg_send![urls, objectAtIndex: i];
                let path: id = msg_send![url, path];
                if path != nil {
                    let text: *const std::os::raw::c_char = msg_send![path, UTF8String];
                    out.files.push(PathBuf::from(std::ffi::CStr::from_ptr(text).to_string_lossy().into_owned()));
                }
            }
            return out;
        }
        let string: id = msg_send![pasteboard, stringForType: ns("public.utf8-plain-text")];
        if string != nil {
            return out;
        }
        for (kind, ext) in [("public.png", "png"), ("public.jpeg", "jpg"), ("com.compuserve.gif", "gif")] {
            if let Some(data) = data_of(pasteboard, kind) {
                out.image = Some((data, ext));
                return out;
            }
        }
        if let Some(png) = data_of(pasteboard, "public.tiff").and_then(|tiff| tiff_to_png(&tiff)) {
            out.image = Some((png, "png"));
        }
        out
    }
}

/// The text that links `links` in: one per line, starting a new line unless the
/// insert point is at one, ending the line if text follows.
pub fn embed_text(links: &[String], before: Option<u8>, after: Option<u8>) -> String {
    let mut text = links.join("\n");
    if before.is_some_and(|b| b != b'\n') {
        text.insert(0, '\n');
    }
    if after.is_some_and(|b| b != b'\n') {
        text.push('\n');
    }
    text
}

/// Seconds from UTC here, for naming a pasted image in local time.
pub fn utc_offset(now: i64) -> i32 {
    unsafe {
        let mut parts: libc::tm = std::mem::zeroed();
        let time = now as libc::time_t;
        libc::localtime_r(&time, &mut parts);
        parts.tm_gmtoff as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(name: &str) -> id {
        let board = named_pasteboard(name);
        unsafe {
            let _: isize = msg_send![board, clearContents];
        }
        board
    }

    fn put(board: id, kind: &str, data: &[u8]) {
        unsafe {
            let types: id = msg_send![class!(NSArray), arrayWithObject: ns(kind)];
            let _: isize = msg_send![board, addTypes: types owner: nil];
            let data: id = msg_send![class!(NSData), dataWithBytes: data.as_ptr() length: data.len()];
            let _: bool = msg_send![board, setData: data forType: ns(kind)];
        }
    }

    #[core::prelude::v1::test]
    fn pasteboards_are_read_in_the_right_order() {
        let png = {
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::RgbaImage::new(4, 4).write_to(&mut bytes, image::ImageFormat::Png).unwrap();
            bytes.into_inner()
        };
        with_pool(|| {
            let b = board("fumi.test.png");
            put(b, "public.png", &png);
            put(b, "public.jpeg", b"jpeg");
            let read = read_from(b);
            assert_eq!(read.image.map(|i| (i.0, i.1)), Some((png.clone(), "png")));
            assert!(!read.has_files);

            // Text alongside means the picture is of that text.
            let b = board("fumi.test.text");
            put(b, "public.png", &png);
            put(b, "public.utf8-plain-text", b"words");
            assert!(read_from(b).image.is_none());

            let b = board("fumi.test.gif");
            put(b, "com.compuserve.gif", b"GIF89a");
            assert_eq!(read_from(b).image.unwrap().1, "gif");

            // File URLs go first, images among them or not.
            let b = board("fumi.test.files");
            put(b, "public.file-url", b"file:///tmp/a.png");
            put(b, "public.png", &png);
            let read = read_from(b);
            assert!(read.has_files && read.image.is_none());
            assert_eq!(read.files, [PathBuf::from("/tmp/a.png")]);

            for name in ["fumi.test.png", "fumi.test.text", "fumi.test.gif", "fumi.test.files"] {
                let _: () = unsafe { msg_send![named_pasteboard(name), releaseGlobally] };
            }
        });
    }

    #[core::prelude::v1::test]
    fn tiff_comes_out_as_png() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        // Written by AppKit, since the image crate here has no TIFF.
        with_pool(|| unsafe {
            let png = {
                let mut p = std::io::Cursor::new(Vec::new());
                image::RgbaImage::new(4, 4).write_to(&mut p, image::ImageFormat::Png).unwrap();
                p.into_inner()
            };
            let data: id = msg_send![class!(NSData), dataWithBytes: png.as_ptr() length: png.len()];
            let rep: id = msg_send![class!(NSBitmapImageRep), imageRepWithData: data];
            let tiff: id = msg_send![rep, TIFFRepresentation];
            let length: usize = msg_send![tiff, length];
            let ptr: *const u8 = msg_send![tiff, bytes];
            std::io::Write::write_all(&mut bytes, std::slice::from_raw_parts(ptr, length)).unwrap();
            let b = board("fumi.test.tiff");
            put(b, "public.tiff", bytes.get_ref());
            let (out, ext) = read_from(b).image.unwrap();
            assert_eq!(ext, "png");
            assert_eq!(&out[1..4], b"PNG");
            let _: () = msg_send![named_pasteboard("fumi.test.tiff"), releaseGlobally];
        });
    }

    #[core::prelude::v1::test]
    fn each_embed_gets_its_own_line() {
        let one = ["![](a.png)".to_string()];
        let two = ["![](a.png)".to_string(), "![](b.mov)".to_string()];
        assert_eq!(embed_text(&one, None, None), "![](a.png)");
        assert_eq!(embed_text(&one, Some(b'\n'), Some(b'\n')), "![](a.png)");
        assert_eq!(embed_text(&one, Some(b'x'), None), "\n![](a.png)");
        assert_eq!(embed_text(&one, None, Some(b'x')), "![](a.png)\n");
        assert_eq!(embed_text(&two, Some(b'x'), Some(b'y')), "\n![](a.png)\n![](b.mov)\n");
    }
}
