//! Words, as the system's tokenizer finds them: dictionary-based in Chinese and
//! Japanese, where Unicode's word boundaries would stop at every character.

use std::ops::Range;
use std::os::raw::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CFRange {
    location: isize,
    length: isize,
}

const UTF8: u32 = 0x0800_0100;
/// `kCFStringTokenizerUnitWord`: no tokens for the space between words.
const UNIT_WORD: usize = 0;
/// `kCFStringTokenizerUnitWordBoundary`: the space between is a token too.
const UNIT_WORD_BOUNDARY: usize = 4;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithBytes(alloc: *const c_void, bytes: *const u8, count: isize, encoding: u32, external: u8) -> *const c_void;
    fn CFStringTokenizerCreate(alloc: *const c_void, string: *const c_void, range: CFRange, options: usize, locale: *const c_void) -> *mut c_void;
    fn CFStringTokenizerAdvanceToNextToken(tokenizer: *mut c_void) -> usize;
    fn CFStringTokenizerGetCurrentTokenRange(tokenizer: *mut c_void) -> CFRange;
    fn CFRelease(object: *const c_void);
}

fn tokens(text: &str, unit: usize) -> Vec<Range<usize>> {
    if text.is_empty() {
        return Vec::new();
    }
    let utf16_len: usize = text.chars().map(char::len_utf16).sum();
    let mut units: Vec<Range<usize>> = Vec::new();
    unsafe {
        let string = CFStringCreateWithBytes(std::ptr::null(), text.as_ptr(), text.len() as isize, UTF8, 0);
        if string.is_null() {
            return Vec::new();
        }
        let range = CFRange { location: 0, length: utf16_len as isize };
        let tokenizer = CFStringTokenizerCreate(std::ptr::null(), string, range, unit, std::ptr::null());
        if !tokenizer.is_null() {
            while CFStringTokenizerAdvanceToNextToken(tokenizer) != 0 {
                let r = CFStringTokenizerGetCurrentTokenRange(tokenizer);
                units.push(r.location as usize..(r.location + r.length) as usize);
            }
            CFRelease(tokenizer);
        }
        CFRelease(string);
    }
    bytes_of_utf16(text, units)
}

/// Moves ascending UTF-16 ranges to byte ranges, in one walk over the text.
fn bytes_of_utf16(text: &str, ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut out = Vec::with_capacity(ranges.len());
    let mut chars = text.char_indices();
    let (mut byte, mut unit) = (0, 0);
    let mut seek = |target: usize| {
        while unit < target {
            let Some((at, c)) = chars.next() else {
                return text.len();
            };
            unit += c.len_utf16();
            byte = at + c.len_utf8();
        }
        byte
    };
    for r in ranges {
        let a = seek(r.start);
        let b = seek(r.end);
        out.push(a..b);
    }
    out
}

/// The words in one line.
pub fn words(text: &str) -> Vec<Range<usize>> {
    tokens(text, UNIT_WORD)
}

/// Option-→: the end of the first word ending after `at`.
pub fn end_after(words: &[Range<usize>], at: usize) -> Option<usize> {
    words.iter().find(|w| w.end > at).map(|w| w.end)
}

/// Option-←: the start of the last word starting before `at`.
pub fn start_before(words: &[Range<usize>], at: usize) -> Option<usize> {
    words.iter().rev().find(|w| w.start < at).map(|w| w.start)
}

/// What a double-click at `at` selects: the word there, or the space or
/// punctuation between words.
pub fn selection_at(text: &str, at: usize) -> Option<Range<usize>> {
    let parts = tokens(text, UNIT_WORD_BOUNDARY);
    parts.iter().find(|w| w.start <= at && at < w.end).or(parts.last().filter(|w| w.end == at)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn english_goes_by_words_and_skips_punctuation() {
        let t = "foo, bar's baz";
        let w = words(t);
        assert_eq!(w, [0..3, 5..10, 11..14]);
        assert_eq!(end_after(&w, 0), Some(3));
        assert_eq!(end_after(&w, 3), Some(10));
        assert_eq!(start_before(&w, 14), Some(11));
        assert_eq!(start_before(&w, 11), Some(5));
        assert_eq!(start_before(&w, 0), None);
        assert_eq!(end_after(&w, 14), None);
    }

    #[core::prelude::v1::test]
    fn chinese_moves_by_more_than_a_character() {
        let t = "今天天气很好，我们去公园";
        let w = words(t);
        assert!(w.len() >= 3, "{w:?}");
        assert!(w.iter().any(|r| r.len() > 3), "some word has two characters: {w:?}");
        for r in &w {
            assert!(t.is_char_boundary(r.start) && t.is_char_boundary(r.end));
        }
        assert!(end_after(&w, 0).unwrap() >= 6);
    }

    #[core::prelude::v1::test]
    fn offsets_survive_astral_characters() {
        let t = "😀 hello 𠀀世界";
        let w = words(t);
        assert!(w.iter().any(|r| &t[r.clone()] == "hello"), "{w:?}");
        for r in &w {
            assert!(t.is_char_boundary(r.start) && t.is_char_boundary(r.end));
        }
    }

    #[core::prelude::v1::test]
    fn a_double_click_takes_the_word_or_the_gap() {
        let t = "foo  bar";
        assert_eq!(selection_at(t, 1), Some(0..3));
        assert_eq!(selection_at(t, 3).map(|r| t[r].trim().is_empty()), Some(true));
        assert_eq!(selection_at(t, 8), Some(5..8));
        assert_eq!(selection_at("", 0), None);
    }
}
