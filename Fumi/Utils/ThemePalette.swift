import Foundation

/// The color themes (design board ㊳, plus White). Each has a light and a
/// dark palette; the appearance mode picks which one shows.
enum AppTheme: String, CaseIterable, Identifiable {
    case white, paper, glass, linen, mist

    var id: String { rawValue }

    var title: String {
        switch self {
        case .white: return "White"
        case .paper: return "Paper"
        case .glass: return "Glass"
        case .linen: return "Linen"
        case .mist: return "Mist"
        }
    }
}

/// Light, dark, or whatever macOS is set to.
enum AppearanceMode: String, CaseIterable, Identifiable {
    case light, dark, auto

    var id: String { rawValue }

    var title: String {
        switch self {
        case .light: return "Light"
        case .dark: return "Dark"
        case .auto: return "Auto"
        }
    }
}

/// One theme in one appearance. Values are sRGB hex (0xRRGGBB).
///
/// Foundation-only so the table can be checked by a standalone test
/// (`Tests/ThemePaletteTests.swift`).
struct ThemePalette: Equatable {
    // Window
    /// The window's ground: the sidebar sits on it, the page floats over it.
    var ground: UInt32
    /// The selected sidebar row, on the ground.
    var groundSelected: UInt32
    /// The page behind the text, and raised surfaces (popovers, cards).
    var page: UInt32
    /// A group of settings, on the page.
    var card: UInt32
    var sidebar: UInt32
    /// Selected rows on the page (Settings sections).
    var selected: UInt32
    var border: UInt32
    var controlBorder: UInt32

    // Text and icons
    var text: UInt32
    var text2: UInt32
    var text3: UInt32
    var faintText: UInt32
    var icon: UInt32
    var faintIcon: UInt32
    /// Bold text when the accent is Ink: a step stronger than body text.
    var strongInk: UInt32

    // Markdown
    var marker: UInt32
    var rule: UInt32
    var codeBlock: UInt32
    var inlineCode: UInt32
    var linkUnderline: UInt32
    var tableBorder: UInt32
    var highlight: UInt32

    static func palette(_ theme: AppTheme, dark: Bool) -> ThemePalette {
        switch (theme, dark) {
        case (.white, false): return .whiteLight
        case (.white, true): return .whiteDark
        case (.paper, false): return .paperLight
        case (.paper, true): return .paperDark
        case (.glass, false): return .glassLight
        case (.glass, true): return .glassDark
        case (.linen, false): return .linenLight
        case (.linen, true): return .linenDark
        case (.mist, false): return .mistLight
        case (.mist, true): return .mistDark
        }
    }

    /// A white page on a near-white neutral ground (sidebar and toolbar),
    /// told apart by the step in gray and the page's hairline edge, no
    /// shadow. Neutral grays, as in Glass.
    static let whiteLight = ThemePalette(
        ground: 0xF6F6F7, groundSelected: 0xEAEAEC, page: 0xFFFFFF, card: 0xF5F5F7, sidebar: 0xF6F6F7,
        selected: 0xEFEFF1, border: 0xEAEAEC, controlBorder: 0xE0E0E3,
        text: 0x1D1D1F, text2: 0x3C3C43, text3: 0x6E6E73, faintText: 0x98989D, icon: 0x86868B, faintIcon: 0xAEAEB2,
        strongInk: 0x000000,
        marker: 0xC7C7CC, rule: 0xEAEAEC, codeBlock: 0xF5F5F7, inlineCode: 0xF0F0F2, linkUnderline: 0xD1D1D6,
        tableBorder: 0xE5E5EA, highlight: 0xFBF3DB)

    /// White's dark side: a graphite page on a deeper neutral ground.
    static let whiteDark = ThemePalette(
        ground: 0x161618, groundSelected: 0x2A2A2D, page: 0x1F1F21, card: 0x262628, sidebar: 0x161618,
        selected: 0x2C2C2E, border: 0x2C2C2E, controlBorder: 0x38383A,
        text: 0xF5F5F7, text2: 0xD1D1D6, text3: 0x98989F, faintText: 0x7C7C80, icon: 0x98989F, faintIcon: 0x636366,
        strongInk: 0xFFFFFF,
        marker: 0x545458, rule: 0x2E2E30, codeBlock: 0x242426, inlineCode: 0x2C2C2E, linkUnderline: 0x545458,
        tableBorder: 0x38383A, highlight: 0x3F3820)

    /// 拾页 as it was: warm-gray ground, white page.
    static let paperLight = ThemePalette(
        ground: 0xEFEEEA, groundSelected: 0xE2E1DC, page: 0xFFFFFF, card: 0xF6F5F2, sidebar: 0xF7F7F5,
        selected: 0xEBEBE9, border: 0xEBEBE9, controlBorder: 0xE3E3E0,
        text: 0x37352F, text2: 0x55534E, text3: 0x6B6A66, faintText: 0x9B9A97, icon: 0x8F8E8A, faintIcon: 0xB4B3AF,
        strongInk: 0x111110,
        marker: 0xC7C6C2, rule: 0xEDEDEB, codeBlock: 0xF7F7F5, inlineCode: 0xF1F1EF, linkUnderline: 0xD3D3CF,
        tableBorder: 0xE9E9E7, highlight: 0xFBF3DB)

    static let paperDark = ThemePalette(
        ground: 0x1C1B19, groundSelected: 0x2F2E2B, page: 0x262523, card: 0x2F2E2B, sidebar: 0x1C1B19,
        selected: 0x353431, border: 0x34332F, controlBorder: 0x3D3C38,
        text: 0xE9E7E2, text2: 0xC3C0B9, text3: 0x9A978F, faintText: 0x7A7771, icon: 0x8F8C85, faintIcon: 0x6E6C66,
        strongInk: 0xFFFFFF,
        marker: 0x5E5C57, rule: 0x383733, codeBlock: 0x2D2C29, inlineCode: 0x34332F, linkUnderline: 0x5E5C57,
        tableBorder: 0x3D3C38, highlight: 0x4A4020)

    /// Pure-white glass: neutral, no wallpaper tint (the user's call).
    static let glassLight = ThemePalette(
        ground: 0xF5F5F7, groundSelected: 0xFFFFFF, page: 0xFFFFFF, card: 0xF2F2F4, sidebar: 0xF5F5F7,
        selected: 0xE8E8ED, border: 0xE5E5EA, controlBorder: 0xDCDCE0,
        text: 0x1D1D1F, text2: 0x3C3C43, text3: 0x6E6E73, faintText: 0x98989D, icon: 0x7C7C82, faintIcon: 0xAEAEB2,
        strongInk: 0x000000,
        marker: 0xC7C7CC, rule: 0xE5E5EA, codeBlock: 0xF5F5F7, inlineCode: 0xEFEFF2, linkUnderline: 0xD1D1D6,
        tableBorder: 0xE5E5EA, highlight: 0xFBF3DB)

    /// Neutral graphite glass.
    static let glassDark = ThemePalette(
        ground: 0x262628, groundSelected: 0x3A3A3C, page: 0x1C1C1E, card: 0x2C2C2E, sidebar: 0x262628,
        selected: 0x3A3A3C, border: 0x2E2E30, controlBorder: 0x3A3A3C,
        text: 0xF5F5F7, text2: 0xD1D1D6, text3: 0x98989F, faintText: 0x7C7C80, icon: 0x98989F, faintIcon: 0x636366,
        strongInk: 0xFFFFFF,
        marker: 0x545458, rule: 0x333335, codeBlock: 0x242426, inlineCode: 0x2C2C2E, linkUnderline: 0x545458,
        tableBorder: 0x38383A, highlight: 0x3F3820)

    /// Warm paper on a desk (board ㉘ D5).
    static let linenLight = ThemePalette(
        ground: 0xF1EEE7, groundSelected: 0xE6E1D6, page: 0xFFFEFC, card: 0xF7F4EE, sidebar: 0xF1EEE7,
        selected: 0xECE8DF, border: 0xECE8DF, controlBorder: 0xE4DFD4,
        text: 0x3A342B, text2: 0x5C574D, text3: 0x716A5E, faintText: 0xA39C8F, icon: 0x958E80, faintIcon: 0xBDB5A6,
        strongInk: 0x15120E,
        marker: 0xCBC4B6, rule: 0xEDE9E0, codeBlock: 0xF7F4EE, inlineCode: 0xF2EEE6, linkUnderline: 0xD6CFC1,
        tableBorder: 0xE9E4DA, highlight: 0xFAF0D2)

    static let linenDark = ThemePalette(
        ground: 0x1D1A16, groundSelected: 0x332E27, page: 0x27231E, card: 0x302B25, sidebar: 0x1D1A16,
        selected: 0x38322A, border: 0x37312A, controlBorder: 0x3D372F,
        text: 0xECE5D8, text2: 0xC8BFAF, text3: 0x9E9586, faintText: 0x7F7769, icon: 0x978E7F, faintIcon: 0x6E665A,
        strongInk: 0xFFFBF2,
        marker: 0x655D51, rule: 0x3A342C, codeBlock: 0x2E2A24, inlineCode: 0x332E27, linkUnderline: 0x655D51,
        tableBorder: 0x3D372F, highlight: 0x4A3E22)

    /// Cool slate.
    static let mistLight = ThemePalette(
        ground: 0xECEFF3, groundSelected: 0xDCE1E8, page: 0xFFFFFF, card: 0xF3F5F8, sidebar: 0xECEFF3,
        selected: 0xE3E7EC, border: 0xE6EAEF, controlBorder: 0xDEE3EA,
        text: 0x27303A, text2: 0x4A5562, text3: 0x616C7A, faintText: 0x97A1AD, icon: 0x8792A0, faintIcon: 0xB3BBC6,
        strongInk: 0x0E1318,
        marker: 0xC3CAD3, rule: 0xE8ECF1, codeBlock: 0xF4F6F9, inlineCode: 0xEEF1F5, linkUnderline: 0xCDD4DD,
        tableBorder: 0xE3E8EE, highlight: 0xFBF3DB)

    static let mistDark = ThemePalette(
        ground: 0x13171C, groundSelected: 0x29313B, page: 0x1B2027, card: 0x232931, sidebar: 0x13171C,
        selected: 0x2A313A, border: 0x2A313A, controlBorder: 0x303843,
        text: 0xE4E9EF, text2: 0xB8C2CE, text3: 0x8995A3, faintText: 0x6C7886, icon: 0x808C9A, faintIcon: 0x566170,
        strongInk: 0xFFFFFF,
        marker: 0x4E5866, rule: 0x2C333C, codeBlock: 0x222831, inlineCode: 0x262D36, linkUnderline: 0x4E5866,
        tableBorder: 0x303843, highlight: 0x3E3820)

    // MARK: Contrast

    /// WCAG relative luminance of an sRGB hex color.
    static func luminance(_ hex: UInt32) -> Double {
        func channel(_ v: UInt32) -> Double {
            let c = Double(v) / 255
            return c <= 0.03928 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * channel((hex >> 16) & 0xFF) + 0.7152 * channel((hex >> 8) & 0xFF) + 0.0722 * channel(hex & 0xFF)
    }

    /// WCAG contrast ratio between two colors (1…21).
    static func contrast(_ a: UInt32, _ b: UInt32) -> Double {
        let la = luminance(a), lb = luminance(b)
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }
}
