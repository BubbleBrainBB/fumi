//! Spell checking for the editor's lines, `fumi.spellCheck`: what
//! `NSSpellChecker` finds, marked the way `NSTextView` marks it.

use cocoa::base::{id, nil};
use gpui::{Hsla, rgb};
use objc::{class, msg_send, sel, sel_impl};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;

/// One dot of the mark, and from one dot to the next.
pub const DOT: f32 = 2.5;
pub const DOT_STEP: f32 = 4.;

/// `systemRed`.
pub fn color(dark: bool) -> Hsla {
    if dark { rgb(0xFF453A).into() } else { rgb(0xFF3B30).into() }
}

/// The misspelled words of `text`, by byte. Lines are checked again each time
/// they're drawn, so what a line came to is remembered.
pub fn misspelled(text: &str) -> Vec<Range<usize>> {
    thread_local! {
        static KNOWN: RefCell<HashMap<String, Vec<Range<usize>>>> = RefCell::default();
    }
    if !text.chars().any(char::is_alphabetic) {
        return Vec::new();
    }
    if let Some(found) = KNOWN.with(|k| k.borrow().get(text).cloned()) {
        return found;
    }
    let found = check(text);
    KNOWN.with(|k| {
        let mut k = k.borrow_mut();
        if k.len() > 4096 {
            k.clear();
        }
        k.insert(text.to_string(), found.clone());
    });
    found
}

/// The word the caret is in or just after isn't marked while it's being typed.
pub fn without_typing(ranges: Vec<Range<usize>>, caret: Option<usize>) -> Vec<Range<usize>> {
    match caret {
        Some(at) => ranges.into_iter().filter(|r| !(r.start <= at && at <= r.end)).collect(),
        None => ranges,
    }
}

fn check(text: &str) -> Vec<Range<usize>> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRange {
        location: u64,
        length: u64,
    }
    let mut ranges = Vec::new();
    let units: Vec<u16> = text.encode_utf16().collect();
    let to_byte = |unit: usize| String::from_utf16_lossy(&units[..unit.min(units.len())]).len();
    unsafe {
        let checker: id = msg_send![class!(NSSpellChecker), sharedSpellChecker];
        if checker == nil {
            return ranges;
        }
        let string = fumi_ui::native::ns(text);
        let mut from = 0u64;
        while (from as usize) < units.len() {
            let range: NSRange = msg_send![checker, checkSpellingOfString: string startingAt: from as i64];
            if range.length == 0 || range.location >= units.len() as u64 || range.location < from {
                break;
            }
            let end = range.location + range.length;
            ranges.push(to_byte(range.location as usize)..to_byte(end as usize));
            from = end;
        }
        let _: () = msg_send![string, release];
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn the_word_being_typed_is_left_alone() {
        let ranges = vec![0..4, 10..14];
        assert_eq!(without_typing(ranges.clone(), Some(14)), [0..4]);
        assert_eq!(without_typing(ranges.clone(), Some(7)), ranges);
        assert_eq!(without_typing(ranges.clone(), None), ranges);
    }

    #[core::prelude::v1::test]
    fn finds_a_misspelling_by_byte() {
        let found = misspelled("写 helllo wrold");
        assert!(found.iter().all(|r| r.start >= 4), "{found:?}");
        assert!(misspelled("1234 ---").is_empty());
    }
}
