import SwiftUI
import AppKit

// Test-only shim for Fumi/Utils/ColorTokens.swift, which cannot compile in
// this standalone harness (`@Observable final class ThemeManager` needs the
// Observation macro). Everything the engine dependency graph references is
// reproduced here verbatim; ThemeManager is dropped and `Color.accent` points
// at the default accent hex.

enum Palette {
    static let appBg = Color.fromHex("#FFFFFF")
    static let sidebarBg = Color.fromHex("#F7F7F5")
    static let editorBg = Color.fromHex("#FFFFFF")
    static let primaryText = Color.fromHex("#37352F")
    static let secondaryText = Color.fromHex("#55534E")
    static let tertiaryText = Color.fromHex("#73726E")
    static let iconMuted = Color.fromHex("#8F8E8A")
    static let border = Color.fromHex("#EBEBE9")
    static let codeBlockBg = Color.fromHex("#F7F7F5")
    static let surface = Color.fromHex("#FFFFFF")
    static let selectedBg = Color.fromHex("#EBEBE9")
    static let controlBorder = Color.fromHex("#E3E3E0")
}

extension Color {
    static func fromHex(_ hex: String) -> Color {
        var cleanHex = hex
        if cleanHex.hasPrefix("#") { cleanHex.removeFirst() }
        guard let value = UInt32(cleanHex, radix: 16) else { return .gray }
        let r = Double((value >> 16) & 0xFF) / 255
        let g = Double((value >> 8) & 0xFF) / 255
        let b = Double(value & 0xFF) / 255
        return Color(red: r, green: g, blue: b)
    }

    static var appBg: Color { Palette.appBg }
    static var sidebarBg: Color { Palette.sidebarBg }
    static var editorBg: Color { Palette.editorBg }
    static var primaryText: Color { Palette.primaryText }
    static var secondaryText: Color { Palette.secondaryText }
    static var tertiaryText: Color { Palette.tertiaryText }
    static var iconMuted: Color { Palette.iconMuted }
    static var accent: Color { Color.fromHex("#1D6FC2") }
    static var borderColor: Color { Palette.border }
    static var codeBlockBg: Color { Palette.codeBlockBg }
    static var surface: Color { Palette.surface }
    static var selectedBg: Color { Palette.selectedBg }
    static var controlBorder: Color { Palette.controlBorder }
    static var faintText: Color { Color.fromHex("#9B9A97") }
    static var faintIcon: Color { Color.fromHex("#B4B3AF") }

    static var accentHover: Color { Color.accent.opacity(0.10) }
    static var accentSelected: Color { Color.accent.opacity(0.06) }
    static var cardHover: Color { Color.primaryText.opacity(0.04) }
    static var searchFieldBg: Color { Color.primaryText.opacity(0.03) }
    static var quietSurface: Color { Color.primaryText.opacity(0.025) }
}

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

// Declared in WysiwygTextView.swift, which the standalone harness leaves out;
// the engine and image attachments post them.
extension Notification.Name {
    static let imageDidLoad = Notification.Name("fumi.imageDidLoad")
    static let animatedImageFrameDidChange = Notification.Name("fumi.animatedImageFrameDidChange")
}
