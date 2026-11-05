//! The families Settings names, as GPUI fonts. `WysiwygEngine.resolveFont`'s pins:
//! the system faces CoreText doesn't list under their own names, then whatever is
//! installed.

use gpui::{Font, FontFallbacks, font};

const CJK_FALLBACK: &str = "PingFang SC";
/// Han beside a serif, as `CompositeFont` pairs it.
const SERIF_CJK_FALLBACK: &str = "Songti SC";

/// The family GPUI looks up for a name Settings stores.
pub fn lookup_name(family: &str) -> &str {
    match family {
        "System" | "" => ".SystemUIFont",
        "SF Mono" => ".AppleSystemUIFontMonospaced",
        "New York" => ".AppleSystemUIFontSerif",
        other => other,
    }
}

/// A family by the name Settings stores, with Chinese falling back to its partner.
pub fn family_font(family: &str) -> Font {
    let mut f = font(lookup_name(family).to_string());
    let fallbacks = match family {
        "New York" => vec![SERIF_CJK_FALLBACK.to_string(), CJK_FALLBACK.to_string()],
        _ => vec![CJK_FALLBACK.to_string()],
    };
    f.fallbacks = Some(FontFallbacks::from_fonts(fallbacks));
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn the_pinned_names_map_to_system_faces() {
        assert_eq!(family_font("System").family.as_ref(), ".SystemUIFont");
        assert_eq!(family_font("").family.as_ref(), ".SystemUIFont");
        assert_eq!(family_font("SF Mono").family.as_ref(), ".AppleSystemUIFontMonospaced");
        assert_eq!(family_font("New York").family.as_ref(), ".AppleSystemUIFontSerif");
        assert_eq!(family_font("Avenir").family.as_ref(), "Avenir");
    }

    #[core::prelude::v1::test]
    fn every_family_has_the_cjk_fallback() {
        for family in ["System", "SF Mono", "New York", "Avenir"] {
            let fallbacks = family_font(family).fallbacks.expect(family);
            assert!(fallbacks.fallback_list().iter().any(|f| f == CJK_FALLBACK), "{family}");
        }
    }
}
