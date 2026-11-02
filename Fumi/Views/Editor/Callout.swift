import AppKit

extension NSAttributedString.Key {
    /// Marks a range as a callout block: a `:::` admonition, or a blockquote
    /// that opens with an emoji. The value is the raw type string (e.g. "note").
    /// `CodeBlockLayoutManager` draws a tinted rounded box behind such ranges,
    /// colored by `CalloutKind`.
    static let calloutType = NSAttributedString.Key("fumi.calloutType")
}

/// The kind of callout, which determines its tint and icon (design board
/// 57). Both Obsidian's and GitHub's type names map onto six colors; a type
/// it doesn't know is drawn gray under its own name.
enum CalloutKind {
    case note, tip, important, warning, danger, plain

    /// Raw type used for a blockquote that starts with an emoji (`> ⚠️ …`):
    /// drawn as a gray box with the emoji standing in for the icon.
    static let emojiQuote = "quote"

    init(_ raw: String) {
        switch raw.lowercased() {
        case "note", "info", "abstract", "summary", "tldr", "todo": self = .note
        case "tip", "hint", "success", "check", "done": self = .tip
        case "important": self = .important
        case "warning", "attention", "question", "help", "faq": self = .warning
        case "caution", "danger", "error", "failure", "fail", "missing", "bug": self = .danger
        default: self = .plain
        }
    }

    /// The title a callout without one shows: its type, capitalized
    /// (`note` → "Note", `IMPORTANT` → "Important").
    static func title(for raw: String) -> String {
        raw.prefix(1).uppercased() + raw.dropFirst().lowercased()
    }

    /// Title and icon color.
    var color: NSColor {
        switch self {
        case .note:      return MarkdownPalette.adaptive(0x2A6FB8, 0x6FA8E6)
        case .tip:       return MarkdownPalette.adaptive(0x448361, 0x6FB08A)
        case .important: return MarkdownPalette.adaptive(0x9065B0, 0xB48FD6)
        case .warning:   return MarkdownPalette.adaptive(0x9A6B12, 0xD1A04A)
        case .danger:    return MarkdownPalette.adaptive(0xC4402F, 0xE5735F)
        case .plain:     return MarkdownPalette.adaptive(0x55534E, 0xC3C0B9)
        }
    }

    /// Box fill. On Glass the page is see-through, so the fill is the type
    /// color, faint, rather than a solid patch.
    var background: NSColor {
        if ThemeManager.activeTheme == .glass {
            return color.withAlphaComponent(self == .plain ? 0.1 : 0.15)
        }
        switch self {
        case .note:      return MarkdownPalette.adaptive(0xE8F1FB, 0x1E2A38)
        case .tip:       return MarkdownPalette.adaptive(0xE7F5EC, 0x1E2E25)
        case .important: return MarkdownPalette.adaptive(0xF3EEF8, 0x2B2436)
        case .warning:   return MarkdownPalette.adaptive(0xFBF3DB, 0x3A3220)
        case .danger:    return MarkdownPalette.adaptive(0xFDECEE, 0x3A2224)
        case .plain:     return MarkdownPalette.adaptive(0xF1F1EF, 0x2E2D2A)
        }
    }

    /// The icon for a raw type: its kind's, or a closer one for some names.
    static func symbolName(for raw: String) -> String {
        switch raw.lowercased() {
        case "success", "check", "done": return "checkmark.circle"
        case "question", "help", "faq": return "questionmark.circle"
        case "todo": return "checklist"
        case "bug": return "ladybug"
        default: return CalloutKind(raw).symbolName
        }
    }

    var symbolName: String {
        switch self {
        case .note:      return "info.circle"
        case .tip:       return "lightbulb"
        case .important: return "exclamationmark.bubble"
        case .warning:   return "exclamationmark.triangle"
        case .danger:    return "xmark.octagon"
        case .plain:     return "quote.opening"
        }
    }
}
