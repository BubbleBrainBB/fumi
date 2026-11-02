import SwiftUI
import AppKit
import Observation

// MARK: - Palette

/// The app's surface and text colors. Each one follows the chosen theme
/// (Settings › Appearance) and resolves light or dark at draw time, so the
/// whole app — editor included — flips with the appearance.
enum Palette {
    static var appBg: Color { themed(\.page) }
    static var sidebarBg: Color { themed(\.sidebar) }
    /// The window's ground (design board ㉗ D): the sidebar sits on it and
    /// the editor floats above it as a page.
    static var windowGround: Color { themed(\.ground) }
    /// Fill behind the selected sidebar row, on the window ground.
    static var groundSelectedBg: Color { themed(\.groundSelected) }
    static var editorBg: Color { themed(\.page) }
    static var primaryText: Color { themed(\.text) }
    static var secondaryText: Color { themed(\.text2) }
    static var tertiaryText: Color { themed(\.text3) }
    /// Default (unselected) stroke color for sidebar and toolbar icons.
    static var iconMuted: Color { themed(\.icon) }
    /// Pane dividers (sidebar edge, editor header rule).
    static var border: Color { themed(\.border) }
    static var codeBlockBg: Color { themed(\.codeBlock) }
    /// Elevated surface for cards, popovers, selected segments.
    static var surface: Color { themed(\.page) }
    /// Fill behind a selected row on the page.
    static var selectedBg: Color { themed(\.selected) }
    /// Hairline border for raised controls (the sidebar's New Note button).
    static var controlBorder: Color { themed(\.controlBorder) }

    static func themed(_ key: KeyPath<ThemePalette, UInt32>) -> Color {
        Color(nsColor: .themed(key))
    }
}

/// Holds the color theme, light/dark mode and accent. Changing the theme or
/// accent reskins the whole app (the root view rebuilds via `.id` on
/// `renderKey`); changing the mode only swaps the app's appearance, which
/// every themed color follows on its own.
@Observable
final class ThemeManager {
    static let shared = ThemeManager()

    /// The theme the dynamic colors draw with. They resolve while drawing,
    /// outside observation, so they read this mirror of `theme`.
    static var activeTheme: AppTheme = AppSettings.colorTheme

    var accentHex: String
    var theme: AppTheme
    var mode: AppearanceMode

    private init() {
        // One-time migration from the old multi-theme build: its accents were
        // tied to the removed Paper/Mineral/Bookish/Inky themes, so reset to
        // the new default instead of carrying amber over.
        if UserDefaults.standard.object(forKey: AppSettings.appThemeKey) != nil {
            UserDefaults.standard.removeObject(forKey: AppSettings.appThemeKey)
            AppSettings.accentColorHex = AppSettings.defaultAccentHex
        }
        accentHex = AppSettings.accentColorHex
        theme = AppSettings.colorTheme
        mode = AppSettings.appearanceMode
        Self.activeTheme = theme
    }

    /// The Ink accent is the theme's own ink, so it stays visible in dark.
    static let inkAccentHex = "#37352F"

    var accentNSColor: NSColor {
        if accentHex.caseInsensitiveCompare(Self.inkAccentHex) == .orderedSame { return .themed(\.text) }
        return NSColor.hex(Color.hexValue(accentHex) ?? 0x1D6FC2)
    }

    var accentColor: Color { Color(nsColor: accentNSColor) }

    /// A stable key that changes whenever the visible palette changes.
    var renderKey: String { accentHex + "|" + theme.rawValue }

    var isGlass: Bool { theme == .glass }
    /// The page is the ground's own color (Glass's see-through sheets), so
    /// what sits on the page — the active tab — needs the page's hairline
    /// edge to read.
    var pageMatchesGround: Bool { theme == .glass }

    func setAccent(_ hex: String) {
        accentHex = hex
        AppSettings.accentColorHex = hex
    }

    func setTheme(_ newTheme: AppTheme) {
        guard newTheme != theme else { return }
        Self.activeTheme = newTheme
        theme = newTheme
        AppSettings.colorTheme = newTheme
    }

    func setMode(_ newMode: AppearanceMode) {
        mode = newMode
        AppSettings.appearanceMode = newMode
        applyAppearance()
    }

    func applyAppearance() {
        switch mode {
        case .light: NSApp?.appearance = NSAppearance(named: .aqua)
        case .dark: NSApp?.appearance = NSAppearance(named: .darkAqua)
        case .auto: NSApp?.appearance = nil
        }
    }
}

// MARK: - Dynamic colors

extension NSAppearance {
    var isDark: Bool { bestMatch(from: [.aqua, .darkAqua]) == .darkAqua }
}

extension NSColor {
    static func hex(_ value: UInt32, alpha: CGFloat = 1) -> NSColor {
        NSColor(srgbRed: CGFloat((value >> 16) & 0xFF) / 255,
                green: CGFloat((value >> 8) & 0xFF) / 255,
                blue: CGFloat(value & 0xFF) / 255,
                alpha: alpha)
    }

    /// A color from the active theme's palette, light or dark by the
    /// appearance it is drawn in.
    static func themed(_ key: KeyPath<ThemePalette, UInt32>) -> NSColor {
        NSColor(name: nil) { appearance in
            .hex(ThemePalette.palette(ThemeManager.activeTheme, dark: appearance.isDark)[keyPath: key])
        }
    }

    /// The same in every theme, one value for light and one for dark.
    static func adaptive(light: UInt32, dark: UInt32, alpha: CGFloat = 1) -> NSColor {
        NSColor(name: nil) { appearance in .hex(appearance.isDark ? dark : light, alpha: alpha) }
    }
}

// MARK: - Color tokens

extension Color {
    static var appBg: Color { Palette.appBg }
    static var sidebarBg: Color { Palette.sidebarBg }
    static var windowGround: Color { Palette.windowGround }
    static var groundSelectedBg: Color { Palette.groundSelectedBg }
    static var editorBg: Color { Palette.editorBg }
    static var primaryText: Color { Palette.primaryText }
    static var secondaryText: Color { Palette.secondaryText }
    static var tertiaryText: Color { Palette.tertiaryText }
    static var iconMuted: Color { Palette.iconMuted }
    static var accent: Color { ThemeManager.shared.accentColor }
    static var borderColor: Color { Palette.border }
    static var codeBlockBg: Color { Palette.codeBlockBg }
    /// Elevated surface for cards, popovers, selected segments.
    static var surface: Color { Palette.surface }
    static var selectedBg: Color { Palette.selectedBg }
    static var controlBorder: Color { Palette.controlBorder }
    /// A group of rows on the page (Settings cards).
    static var card: Color { Palette.themed(\.card) }
    /// Quietest text: counts beside sidebar folders.
    static var faintText: Color { Palette.themed(\.faintText) }
    /// Quietest icon: documents in the sidebar tree.
    static var faintIcon: Color { Palette.themed(\.faintIcon) }
    /// Inline code and other small tinted runs.
    static var inlineFill: Color { Palette.themed(\.inlineCode) }

    static var accentHover: Color { Color.accent.opacity(0.10) }
    static var accentSelected: Color { Color.accent.opacity(0.06) }
    static var cardHover: Color { Color.primaryText.opacity(0.05) }
    static var searchFieldBg: Color { Color.primaryText.opacity(0.04) }
    static var quietSurface: Color { Color.primaryText.opacity(0.03) }
    /// Fill for small controls: segmented tracks, steppers, key caps.
    static var controlFill: Color { Color(nsColor: .adaptive(light: 0x767680, dark: 0x767680, alpha: 1)).opacity(0.13) }

    /// The page's hairline edge: dark on light, light on dark.
    static var pageEdge: Color {
        Color(nsColor: NSColor(name: nil) { $0.isDark ? NSColor.white.withAlphaComponent(0.07) : NSColor.black.withAlphaComponent(0.06) })
    }

    /// Behind a card that floats over the window (Settings, search).
    static var scrim: Color {
        Color(nsColor: NSColor(name: nil) { NSColor.black.withAlphaComponent(scrimAlpha($0)) })
    }

    /// The scrim at `strength` as a color multiplier: what lies under the
    /// scrim comes out this many times as bright.
    static func scrimShade(_ strength: Double) -> Color {
        Color(nsColor: NSColor(name: nil) { NSColor(white: 1 - scrimAlpha($0) * strength, alpha: 1) })
    }

    private static func scrimAlpha(_ appearance: NSAppearance) -> CGFloat {
        appearance.isDark ? 0.45 : 0.16
    }

    /// The window's ground as painted. Glass: a light white veil over the
    /// behind-window blur, so the desktop shows through it most.
    static var groundFill: Color {
        ThemeManager.shared.isGlass ? Palette.windowGround.opacity(0.12) : Palette.windowGround
    }

    /// The page sheet under the text. Glass: translucent white (graphite)
    /// over the same blur — clearer than paper, still easy to read on.
    static var pageSheet: Color {
        ThemeManager.shared.isGlass ? Palette.editorBg.opacity(0.5) : Palette.editorBg
    }

    /// What views inside the page paint behind themselves: nothing in Glass,
    /// where the page sheet alone carries the color, else the page color.
    static var pageContentBg: Color {
        ThemeManager.shared.isGlass ? Color.clear : Palette.editorBg
    }

    /// The soft lift under a selected sidebar row in the Glass theme, where
    /// the selection is a raised white pill rather than a tint.
    static var selectedRowShadow: Color {
        ThemeManager.shared.isGlass ? Color.black.opacity(0.10) : Color.clear
    }
}

/// The app's only text sizes: notes and secondary lines, body and controls,
/// emphasis, page titles. Nothing in the interface goes under 12.
enum TextSize: CGFloat {
    case caption = 12
    case body = 13
    case emphasis = 15
    case title = 20
}

extension Font {
    static func fumi(_ size: TextSize, weight: Font.Weight = .regular, design: Font.Design = .default) -> Font {
        .system(size: size.rawValue, weight: weight, design: design)
    }
}

// MARK: - Hex colors

extension Color {
    static func hexValue(_ hex: String) -> UInt32? {
        var cleanHex = hex
        if cleanHex.hasPrefix("#") { cleanHex.removeFirst() }
        return UInt32(cleanHex, radix: 16)
    }

    static func fromHex(_ hex: String) -> Color {
        guard let value = hexValue(hex) else { return .gray }
        let r = Double((value >> 16) & 0xFF) / 255
        let g = Double((value >> 8) & 0xFF) / 255
        let b = Double(value & 0xFF) / 255
        return Color(red: r, green: g, blue: b)
    }
}
