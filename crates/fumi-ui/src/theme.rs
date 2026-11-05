//! The app's colors. A port of `ThemePalette.swift` and `ColorTokens.swift`:
//! five themes, each with a light and a dark palette, plus the user's accent.
//!
//! Colors are read while drawing, on the main thread, from a thread-local copy
//! of the resolved tokens (the Swift app reads `ThemeManager.activeTheme` the
//! same way), so text builders don't need a context threaded through them.

use gpui::*;
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppTheme {
    White,
    Paper,
    Glass,
    Linen,
    Mist,
}

impl AppTheme {
    pub const ALL: [AppTheme; 5] = [Self::White, Self::Paper, Self::Glass, Self::Linen, Self::Mist];

    pub fn raw(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Paper => "paper",
            Self::Glass => "glass",
            Self::Linen => "linen",
            Self::Mist => "mist",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Paper => "Paper",
            Self::Glass => "Glass",
            Self::Linen => "Linen",
            Self::Mist => "Mist",
        }
    }

    pub fn from_raw(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.raw() == raw)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppearanceMode {
    Light,
    Dark,
    Auto,
}

impl AppearanceMode {
    pub const ALL: [AppearanceMode; 3] = [Self::Light, Self::Dark, Self::Auto];

    pub fn raw(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Auto => "auto",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Auto => "Auto",
        }
    }

    pub fn from_raw(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.raw() == raw)
    }
}

/// One theme in one appearance. Values are sRGB hex.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ThemePalette {
    pub ground: u32,
    pub ground_selected: u32,
    pub page: u32,
    pub card: u32,
    pub sidebar: u32,
    pub selected: u32,
    pub border: u32,
    pub control_border: u32,
    pub text: u32,
    pub text2: u32,
    pub text3: u32,
    pub faint_text: u32,
    pub icon: u32,
    pub faint_icon: u32,
    pub strong_ink: u32,
    pub marker: u32,
    pub rule: u32,
    pub code_block: u32,
    pub inline_code: u32,
    pub link_underline: u32,
    pub table_border: u32,
    pub highlight: u32,
}

pub const WHITE_LIGHT: ThemePalette = ThemePalette {
    ground: 0xF6F6F7,
    ground_selected: 0xEAEAEC,
    page: 0xFFFFFF,
    card: 0xF5F5F7,
    sidebar: 0xF6F6F7,
    selected: 0xEFEFF1,
    border: 0xEAEAEC,
    control_border: 0xE0E0E3,
    text: 0x1D1D1F,
    text2: 0x3C3C43,
    text3: 0x6E6E73,
    faint_text: 0x98989D,
    icon: 0x86868B,
    faint_icon: 0xAEAEB2,
    strong_ink: 0x000000,
    marker: 0xC7C7CC,
    rule: 0xEAEAEC,
    code_block: 0xF5F5F7,
    inline_code: 0xF0F0F2,
    link_underline: 0xD1D1D6,
    table_border: 0xE5E5EA,
    highlight: 0xFBF3DB,
};

pub const WHITE_DARK: ThemePalette = ThemePalette {
    ground: 0x161618,
    ground_selected: 0x2A2A2D,
    page: 0x1F1F21,
    card: 0x262628,
    sidebar: 0x161618,
    selected: 0x2C2C2E,
    border: 0x2C2C2E,
    control_border: 0x38383A,
    text: 0xF5F5F7,
    text2: 0xD1D1D6,
    text3: 0x98989F,
    faint_text: 0x7C7C80,
    icon: 0x98989F,
    faint_icon: 0x636366,
    strong_ink: 0xFFFFFF,
    marker: 0x545458,
    rule: 0x2E2E30,
    code_block: 0x242426,
    inline_code: 0x2C2C2E,
    link_underline: 0x545458,
    table_border: 0x38383A,
    highlight: 0x3F3820,
};

pub const PAPER_LIGHT: ThemePalette = ThemePalette {
    ground: 0xEFEEEA,
    ground_selected: 0xE2E1DC,
    page: 0xFFFFFF,
    card: 0xF6F5F2,
    sidebar: 0xF7F7F5,
    selected: 0xEBEBE9,
    border: 0xEBEBE9,
    control_border: 0xE3E3E0,
    text: 0x37352F,
    text2: 0x55534E,
    text3: 0x6B6A66,
    faint_text: 0x9B9A97,
    icon: 0x8F8E8A,
    faint_icon: 0xB4B3AF,
    strong_ink: 0x111110,
    marker: 0xC7C6C2,
    rule: 0xEDEDEB,
    code_block: 0xF7F7F5,
    inline_code: 0xF1F1EF,
    link_underline: 0xD3D3CF,
    table_border: 0xE9E9E7,
    highlight: 0xFBF3DB,
};

pub const PAPER_DARK: ThemePalette = ThemePalette {
    ground: 0x1C1B19,
    ground_selected: 0x2F2E2B,
    page: 0x262523,
    card: 0x2F2E2B,
    sidebar: 0x1C1B19,
    selected: 0x353431,
    border: 0x34332F,
    control_border: 0x3D3C38,
    text: 0xE9E7E2,
    text2: 0xC3C0B9,
    text3: 0x9A978F,
    faint_text: 0x7A7771,
    icon: 0x8F8C85,
    faint_icon: 0x6E6C66,
    strong_ink: 0xFFFFFF,
    marker: 0x5E5C57,
    rule: 0x383733,
    code_block: 0x2D2C29,
    inline_code: 0x34332F,
    link_underline: 0x5E5C57,
    table_border: 0x3D3C38,
    highlight: 0x4A4020,
};

pub const GLASS_LIGHT: ThemePalette = ThemePalette {
    ground: 0xF5F5F7,
    ground_selected: 0xFFFFFF,
    page: 0xFFFFFF,
    card: 0xF2F2F4,
    sidebar: 0xF5F5F7,
    selected: 0xE8E8ED,
    border: 0xE5E5EA,
    control_border: 0xDCDCE0,
    text: 0x1D1D1F,
    text2: 0x3C3C43,
    text3: 0x6E6E73,
    faint_text: 0x98989D,
    icon: 0x7C7C82,
    faint_icon: 0xAEAEB2,
    strong_ink: 0x000000,
    marker: 0xC7C7CC,
    rule: 0xE5E5EA,
    code_block: 0xF5F5F7,
    inline_code: 0xEFEFF2,
    link_underline: 0xD1D1D6,
    table_border: 0xE5E5EA,
    highlight: 0xFBF3DB,
};

pub const GLASS_DARK: ThemePalette = ThemePalette {
    ground: 0x262628,
    ground_selected: 0x3A3A3C,
    page: 0x1C1C1E,
    card: 0x2C2C2E,
    sidebar: 0x262628,
    selected: 0x3A3A3C,
    border: 0x2E2E30,
    control_border: 0x3A3A3C,
    text: 0xF5F5F7,
    text2: 0xD1D1D6,
    text3: 0x98989F,
    faint_text: 0x7C7C80,
    icon: 0x98989F,
    faint_icon: 0x636366,
    strong_ink: 0xFFFFFF,
    marker: 0x545458,
    rule: 0x333335,
    code_block: 0x242426,
    inline_code: 0x2C2C2E,
    link_underline: 0x545458,
    table_border: 0x38383A,
    highlight: 0x3F3820,
};

pub const LINEN_LIGHT: ThemePalette = ThemePalette {
    ground: 0xF1EEE7,
    ground_selected: 0xE6E1D6,
    page: 0xFFFEFC,
    card: 0xF7F4EE,
    sidebar: 0xF1EEE7,
    selected: 0xECE8DF,
    border: 0xECE8DF,
    control_border: 0xE4DFD4,
    text: 0x3A342B,
    text2: 0x5C574D,
    text3: 0x716A5E,
    faint_text: 0xA39C8F,
    icon: 0x958E80,
    faint_icon: 0xBDB5A6,
    strong_ink: 0x15120E,
    marker: 0xCBC4B6,
    rule: 0xEDE9E0,
    code_block: 0xF7F4EE,
    inline_code: 0xF2EEE6,
    link_underline: 0xD6CFC1,
    table_border: 0xE9E4DA,
    highlight: 0xFAF0D2,
};

pub const LINEN_DARK: ThemePalette = ThemePalette {
    ground: 0x1D1A16,
    ground_selected: 0x332E27,
    page: 0x27231E,
    card: 0x302B25,
    sidebar: 0x1D1A16,
    selected: 0x38322A,
    border: 0x37312A,
    control_border: 0x3D372F,
    text: 0xECE5D8,
    text2: 0xC8BFAF,
    text3: 0x9E9586,
    faint_text: 0x7F7769,
    icon: 0x978E7F,
    faint_icon: 0x6E665A,
    strong_ink: 0xFFFBF2,
    marker: 0x655D51,
    rule: 0x3A342C,
    code_block: 0x2E2A24,
    inline_code: 0x332E27,
    link_underline: 0x655D51,
    table_border: 0x3D372F,
    highlight: 0x4A3E22,
};

pub const MIST_LIGHT: ThemePalette = ThemePalette {
    ground: 0xECEFF3,
    ground_selected: 0xDCE1E8,
    page: 0xFFFFFF,
    card: 0xF3F5F8,
    sidebar: 0xECEFF3,
    selected: 0xE3E7EC,
    border: 0xE6EAEF,
    control_border: 0xDEE3EA,
    text: 0x27303A,
    text2: 0x4A5562,
    text3: 0x616C7A,
    faint_text: 0x97A1AD,
    icon: 0x8792A0,
    faint_icon: 0xB3BBC6,
    strong_ink: 0x0E1318,
    marker: 0xC3CAD3,
    rule: 0xE8ECF1,
    code_block: 0xF4F6F9,
    inline_code: 0xEEF1F5,
    link_underline: 0xCDD4DD,
    table_border: 0xE3E8EE,
    highlight: 0xFBF3DB,
};

pub const MIST_DARK: ThemePalette = ThemePalette {
    ground: 0x13171C,
    ground_selected: 0x29313B,
    page: 0x1B2027,
    card: 0x232931,
    sidebar: 0x13171C,
    selected: 0x2A313A,
    border: 0x2A313A,
    control_border: 0x303843,
    text: 0xE4E9EF,
    text2: 0xB8C2CE,
    text3: 0x8995A3,
    faint_text: 0x6C7886,
    icon: 0x808C9A,
    faint_icon: 0x566170,
    strong_ink: 0xFFFFFF,
    marker: 0x4E5866,
    rule: 0x2C333C,
    code_block: 0x222831,
    inline_code: 0x262D36,
    link_underline: 0x4E5866,
    table_border: 0x303843,
    highlight: 0x3E3820,
};

pub fn palette(theme: AppTheme, dark: bool) -> ThemePalette {
    match (theme, dark) {
        (AppTheme::White, false) => WHITE_LIGHT,
        (AppTheme::White, true) => WHITE_DARK,
        (AppTheme::Paper, false) => PAPER_LIGHT,
        (AppTheme::Paper, true) => PAPER_DARK,
        (AppTheme::Glass, false) => GLASS_LIGHT,
        (AppTheme::Glass, true) => GLASS_DARK,
        (AppTheme::Linen, false) => LINEN_LIGHT,
        (AppTheme::Linen, true) => LINEN_DARK,
        (AppTheme::Mist, false) => MIST_LIGHT,
        (AppTheme::Mist, true) => MIST_DARK,
    }
}

pub fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

fn adaptive(dark: bool, light: u32, night: u32) -> Hsla {
    hex(if dark { night } else { light })
}

pub const DEFAULT_ACCENT: &str = "#1D6FC2";
pub const INK_ACCENT: &str = "#37352F";

/// The six callout colors of `CalloutKind`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CalloutKind {
    Note,
    Tip,
    Important,
    Warning,
    Danger,
    Plain,
}

impl CalloutKind {
    pub const EMOJI_QUOTE: &'static str = "quote";

    pub fn of(raw: &str) -> Self {
        match raw.to_lowercase().as_str() {
            "note" | "info" | "abstract" | "summary" | "tldr" | "todo" => Self::Note,
            "tip" | "hint" | "success" | "check" | "done" => Self::Tip,
            "important" => Self::Important,
            "warning" | "attention" | "question" | "help" | "faq" => Self::Warning,
            "caution" | "danger" | "error" | "failure" | "fail" | "missing" | "bug" => Self::Danger,
            _ => Self::Plain,
        }
    }

    /// The title a callout without one shows: `note` → "Note".
    pub fn title(raw: &str) -> String {
        let mut chars = raw.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
            None => String::new(),
        }
    }

    pub fn color(self, dark: bool) -> Hsla {
        match self {
            Self::Note => adaptive(dark, 0x2A6FB8, 0x6FA8E6),
            Self::Tip => adaptive(dark, 0x448361, 0x6FB08A),
            Self::Important => adaptive(dark, 0x9065B0, 0xB48FD6),
            Self::Warning => adaptive(dark, 0x9A6B12, 0xD1A04A),
            Self::Danger => adaptive(dark, 0xC4402F, 0xE5735F),
            Self::Plain => adaptive(dark, 0x55534E, 0xC3C0B9),
        }
    }

    /// Box fill. On Glass the page is see-through, so the fill is the type
    /// color, faint, rather than a solid patch.
    pub fn background(self, dark: bool, glass: bool) -> Hsla {
        if glass {
            return self.color(dark).opacity(if self == Self::Plain { 0.1 } else { 0.15 });
        }
        match self {
            Self::Note => adaptive(dark, 0xE8F1FB, 0x1E2A38),
            Self::Tip => adaptive(dark, 0xE7F5EC, 0x1E2E25),
            Self::Important => adaptive(dark, 0xF3EEF8, 0x2B2436),
            Self::Warning => adaptive(dark, 0xFBF3DB, 0x3A3220),
            Self::Danger => adaptive(dark, 0xFDECEE, 0x3A2224),
            Self::Plain => adaptive(dark, 0xF1F1EF, 0x2E2D2A),
        }
    }
}

/// Every color the interface draws with, resolved for the active theme, mode
/// and accent. Field names follow `ColorTokens.swift` and `MarkdownPalette`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tokens {
    pub theme: AppTheme,
    pub dark: bool,
    pub glass: bool,
    // Surfaces
    pub window_ground: Hsla,
    pub ground_selected: Hsla,
    pub page: Hsla,
    pub card: Hsla,
    pub sidebar: Hsla,
    pub selected: Hsla,
    pub border: Hsla,
    pub control_border: Hsla,
    /// The window's ground as painted: a light veil on Glass.
    pub ground_fill: Hsla,
    /// The page sheet under the text: translucent on Glass.
    pub page_sheet: Hsla,
    /// What views inside the page paint behind themselves.
    pub page_content: Hsla,
    pub page_edge: Hsla,
    pub scrim: Hsla,
    pub control_fill: Hsla,
    pub card_hover: Hsla,
    pub search_field: Hsla,
    pub quiet_surface: Hsla,
    pub selected_row_shadow: Hsla,
    // Text and icons
    pub text: Hsla,
    pub text2: Hsla,
    pub text3: Hsla,
    pub faint_text: Hsla,
    pub icon: Hsla,
    pub faint_icon: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_selected: Hsla,
    // Markdown
    pub strong: Hsla,
    pub marker: Hsla,
    pub rule: Hsla,
    pub code_block: Hsla,
    pub inline_bg: Hsla,
    pub inline_code: Hsla,
    pub link_underline: Hsla,
    pub highlight: Hsla,
    pub table_border: Hsla,
    pub checkbox_border: Hsla,
    pub nested_quote_bar: Hsla,
    pub syntax_keyword: Hsla,
    pub syntax_string: Hsla,
    pub syntax_number: Hsla,
    pub selection: Hsla,
}

pub fn parse_hex(hex: &str) -> Option<u32> {
    u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()
}

impl Tokens {
    pub fn resolve(theme: AppTheme, dark: bool, accent_hex: &str) -> Self {
        let p = palette(theme, dark);
        let glass = theme == AppTheme::Glass;
        let is_ink = accent_hex.eq_ignore_ascii_case(INK_ACCENT);
        let accent = if is_ink { hex(p.text) } else { hex(parse_hex(accent_hex).unwrap_or(0x1D6FC2)) };
        let text = hex(p.text);
        let page = hex(p.page);
        let scrim_alpha = if dark { 0.45 } else { 0.16 };
        Tokens {
            theme,
            dark,
            glass,
            window_ground: hex(p.ground),
            ground_selected: hex(p.ground_selected),
            page,
            card: hex(p.card),
            sidebar: hex(p.sidebar),
            selected: hex(p.selected),
            border: hex(p.border),
            control_border: hex(p.control_border),
            ground_fill: if glass { hex(p.ground).opacity(0.12) } else { hex(p.ground) },
            page_sheet: if glass { page.opacity(0.5) } else { page },
            page_content: if glass { transparent_black() } else { page },
            page_edge: if dark { hsla(0., 0., 1., 0.07) } else { hsla(0., 0., 0., 0.06) },
            scrim: hsla(0., 0., 0., scrim_alpha),
            control_fill: hex(0x767680).opacity(0.13),
            card_hover: text.opacity(0.05),
            search_field: text.opacity(0.04),
            quiet_surface: text.opacity(0.03),
            selected_row_shadow: if glass { hsla(0., 0., 0., 0.10) } else { transparent_black() },
            text,
            text2: hex(p.text2),
            text3: hex(p.text3),
            faint_text: hex(p.faint_text),
            icon: hex(p.icon),
            faint_icon: hex(p.faint_icon),
            accent,
            accent_hover: accent.opacity(0.10),
            accent_selected: accent.opacity(0.06),
            strong: if is_ink { hex(p.strong_ink) } else { accent },
            marker: if glass { text.opacity(0.26) } else { hex(p.marker) },
            rule: hex(p.rule),
            code_block: hex(p.code_block),
            inline_bg: hex(p.inline_code),
            inline_code: adaptive(dark, 0xC4402F, 0xE5735F),
            link_underline: hex(p.link_underline),
            highlight: hex(p.highlight),
            table_border: hex(p.table_border),
            checkbox_border: hex(p.faint_icon),
            nested_quote_bar: hex(p.link_underline),
            syntax_keyword: adaptive(dark, 0x9065B0, 0xB794D6),
            syntax_string: adaptive(dark, 0x448361, 0x6FB08A),
            syntax_number: adaptive(dark, 0x9A6B12, 0xD1A04A),
            
            selection: accent.opacity(0.18),
        }
    }
}

thread_local! {
    static ACTIVE: Cell<Tokens> = Cell::new(Tokens::resolve(AppTheme::White, false, DEFAULT_ACCENT));
}

/// The tokens to draw with right now.
pub fn t() -> Tokens {
    ACTIVE.with(Cell::get)
}

pub fn set_active(tokens: Tokens) {
    ACTIVE.with(|c| c.set(tokens));
}

/// Re-reads theme, mode and accent from Settings and makes them active.
/// `system_dark` is the window's appearance, used in Auto mode.
pub fn apply_from_settings(system_dark: bool) -> Tokens {
    use crate::settings as s;
    let dark = match s::appearance_mode() {
        AppearanceMode::Light => false,
        AppearanceMode::Dark => true,
        AppearanceMode::Auto => system_dark,
    };
    let tokens = Tokens::resolve(s::color_theme(), dark, &s::accent_hex());
    set_active(tokens);
    tokens
}

/// How strongly the window is dimmed behind a floating card, as a multiplier
/// for what must look dimmed but sits above the scrim.
pub fn scrim_shade(strength: f32, dark: bool) -> f32 {
    1. - (if dark { 0.45 } else { 0.16 }) * strength
}

/// The app's only interface text sizes.
pub mod text_size {
    pub const CAPTION: f32 = 12.;
    pub const BODY: f32 = 13.;
    pub const EMPHASIS: f32 = 15.;
    pub const TITLE: f32 = 20.;
}

/// WCAG contrast ratio between two sRGB colors (1…21).
pub fn contrast(a: u32, b: u32) -> f64 {
    fn lum(hex: u32) -> f64 {
        let ch = |v: u32| {
            let c = v as f64 / 255.;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch((hex >> 16) & 0xFF) + 0.7152 * ch((hex >> 8) & 0xFF) + 0.0722 * ch(hex & 0xFF)
    }
    let (la, lb) = (lum(a), lum(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn body_text_stays_readable_in_every_palette() {
        for theme in AppTheme::ALL {
            for dark in [false, true] {
                let p = palette(theme, dark);
                assert!(contrast(p.text, p.page) >= 7., "{theme:?} dark={dark}");
                assert!(contrast(p.text3, p.page) >= 3., "{theme:?} dark={dark}");
            }
        }
    }

    #[core::prelude::v1::test]
    fn ink_accent_follows_the_theme() {
        let light = Tokens::resolve(AppTheme::Paper, false, "#37352F");
        let dark = Tokens::resolve(AppTheme::Paper, true, "#37352f");
        assert_eq!(light.accent, hex(PAPER_LIGHT.text));
        assert_eq!(dark.accent, hex(PAPER_DARK.text));
        assert_eq!(dark.strong, hex(PAPER_DARK.strong_ink));
    }

    #[core::prelude::v1::test]
    fn callout_names_map_to_kinds() {
        assert_eq!(CalloutKind::of("TLDR"), CalloutKind::Note);
        assert_eq!(CalloutKind::of("bug"), CalloutKind::Danger);
        assert_eq!(CalloutKind::of("whatever"), CalloutKind::Plain);
        assert_eq!(CalloutKind::title("IMPORTANT"), "Important");
    }
}
