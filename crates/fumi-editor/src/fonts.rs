//! What the system knows about font families, for pairing a Latin family with a
//! CJK one. Asked rarely (when Settings change) and remembered.

use cocoa::base::{id, nil};
use fumi_ui::native::ns;
use objc::{class, msg_send, sel, sel_impl};
use std::collections::HashSet;
use std::ffi::CStr;
use std::sync::OnceLock;

fn installed() -> &'static HashSet<String> {
    static FAMILIES: OnceLock<HashSet<String>> = OnceLock::new();
    FAMILIES.get_or_init(|| unsafe {
        let manager: id = msg_send![class!(NSFontManager), sharedFontManager];
        let families: id = msg_send![manager, availableFontFamilies];
        let count: usize = msg_send![families, count];
        (0..count)
            .filter_map(|i| {
                let name: id = msg_send![families, objectAtIndex: i];
                let utf8: *const std::os::raw::c_char = msg_send![name, UTF8String];
                (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())
            })
            .collect()
    })
}

pub fn is_installed(family: &str) -> bool {
    installed().contains(family)
}

/// Whether a family named in Settings can be used: the names the app maps to
/// system fonts always can.
pub fn is_available(family: &str) -> bool {
    matches!(family, "" | "System" | "SF Mono" | "New York") || is_installed(family)
}

/// Whether the family has 一 of its own, the probe for covering Han.
pub fn covers_han(family: &str) -> bool {
    unsafe {
        let manager: id = msg_send![class!(NSFontManager), sharedFontManager];
        let font: id = msg_send![manager, fontWithFamily: ns(family) traits: 0usize weight: 5isize size: 12f64];
        if font == nil {
            return false;
        }
        let covered: id = msg_send![font, coveredCharacterSet];
        if covered == nil {
            return false;
        }
        let has: objc::runtime::BOOL = msg_send![covered, characterIsMember: 0x4E00u16];
        has == objc::runtime::YES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn knows_a_cjk_family_from_a_latin_one() {
        assert!(covers_han("PingFang SC"));
        assert!(!covers_han("Georgia"));
        assert!(!covers_han("No Such Family"));
        assert!(is_installed("Georgia"));
        assert!(!is_available("No Such Family"));
        assert!(is_available("New York"));
    }
}
