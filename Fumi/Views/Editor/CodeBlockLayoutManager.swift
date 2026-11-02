import AppKit
import SwiftUI

/// Editor colors from the 拾页 Markdown spec (design canvas board ⑩). Each
/// follows the theme and resolves light or dark when drawn (ColorTokens).
enum MarkdownPalette {
    static func hex(_ value: UInt32) -> NSColor { .hex(value) }

    /// One color for light, another for dark, the same in every theme.
    static func adaptive(_ light: UInt32, _ dark: UInt32) -> NSColor { .adaptive(light: light, dark: dark) }

    static let primary = NSColor.themed(\.text)
    /// Bold text when the accent is too close to the body ink to tell apart
    /// (the Ink accent): a step stronger, so **strong** still reads at a glance.
    static let strong = NSColor.themed(\.strongInk)

    /// Bold text in the user's accent color; the stronger ink for the Ink accent.
    static func strong(accentHex: String) -> NSColor {
        let digits = accentHex.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
        guard let value = UInt32(digits, radix: 16), value != 0x37352F else { return strong }
        return hex(value)
    }
    static let secondary = NSColor.themed(\.text2)
    static let tertiary = NSColor.themed(\.text3)
    static let faint = NSColor.themed(\.faintText)
    /// Markdown syntax revealed on the caret's paragraph. Glass lets what's
    /// behind the window through the page, where the fixed light gray
    /// matched a darker backdrop and the syntax vanished; there it is the
    /// ink at a quarter strength instead — the same gray on white, and
    /// always a shade darker than whatever shows through.
    static let marker = NSColor(name: nil) { appearance in
        let palette = ThemePalette.palette(ThemeManager.activeTheme, dark: appearance.isDark)
        guard ThemeManager.activeTheme == .glass else { return .hex(palette.marker) }
        return .hex(palette.text, alpha: 0.26)
    }
    static let rule = NSColor.themed(\.rule)
    static let codeBlockBg = NSColor.themed(\.codeBlock)
    static let inlineBg = NSColor.themed(\.inlineCode)
    static let inlineCode = adaptive(0xC4402F, 0xE5735F)
    static let linkUnderline = NSColor.themed(\.linkUnderline)
    static let highlight = NSColor.themed(\.highlight)
    static let tableBorder = NSColor.themed(\.tableBorder)
    static let checkboxBorder = NSColor.themed(\.faintIcon)
    static let nestedQuoteBar = NSColor.themed(\.linkUnderline)

    static let syntaxKeyword = adaptive(0x9065B0, 0xB794D6)
    static let syntaxString = adaptive(0x448361, 0x6FB08A)
    static let syntaxNumber = adaptive(0x9A6B12, 0xD1A04A)

    /// The user's accent (Settings › Appearance), for checked to-dos.
    static var accent: NSColor { ThemeManager.shared.accentNSColor }
}

extension NSAttributedString.Key {
    /// Marks a range as belonging to a fenced code block. `CodeBlockLayoutManager`
    /// paints a single rounded rectangle behind each such range. Set instead of
    /// `.backgroundColor`, which TextKit paints per-glyph-run (ragged width, no
    /// padding, no corners) rather than as one uniform box.
    static let codeBlockBackground = NSAttributedString.Key("fumi.codeBlockBackground")
    /// The fenced block's language (its info string), drawn top-right in the box.
    static let codeBlockLanguage = NSAttributedString.Key("fumi.codeBlockLanguage")

    /// Marks the hidden prefix of a list item. The value is a `ListMarker`
    /// describing the bullet, number or checkbox to draw in the indent.
    static let listMarker = NSAttributedString.Key("fumi.listMarker")

    /// Blockquote nesting depth (Int) on each quote line; bars are drawn in the
    /// indent, one per level.
    static let blockquoteDepth = NSAttributedString.Key("fumi.blockquoteDepth")

    /// A rounded fill behind an inline span (inline code, hashtags). The value
    /// is the fill `NSColor`.
    static let inlinePill = NSAttributedString.Key("fumi.inlinePill")

    /// A text highlight (`==text==`): a fill hugging the glyphs, without the
    /// padding of `.inlinePill`. The value is the fill `NSColor`.
    static let textHighlight = NSAttributedString.Key("fumi.textHighlight")

    /// A horizontal rule line, drawn across the column.
    static let horizontalRule = NSAttributedString.Key("fumi.horizontalRule")

    /// On a callout's first character: the `CalloutKind` raw type whose icon is
    /// drawn in the callout's top-left corner.
    static let calloutIcon = NSAttributedString.Key("fumi.calloutIcon")

    /// On a hidden `[!type]` word: the title drawn in its place ("Note" for
    /// `> [!note]`), for a callout that names no title of its own.
    static let calloutTitle = NSAttributedString.Key("fumi.calloutTitle")
}

/// What to draw in a list item's indent.
final class ListMarker: NSObject {
    enum Kind: Equatable {
        case bullet(level: Int)
        case ordered(label: String)
        case task(checked: Bool)
    }

    let kind: Kind
    /// The paragraph's head indent; the marker sits in the 24pt slot before it.
    let indent: CGFloat
    let font: NSFont

    init(kind: Kind, indent: CGFloat, font: NSFont) {
        self.kind = kind
        self.indent = indent
        self.font = font
    }

    // Equal by what they draw, so a restyle that rebuilds an unchanged list
    // leaves its lines alone (WysiwygEngine.applyChanges).
    override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? ListMarker else { return false }
        return kind == other.kind && indent == other.indent && font == other.font
    }

    override var hash: Int { indent.hashValue ^ font.hashValue }

    static let slotWidth: CGFloat = 24

    /// The checkbox's frame relative to the container's left edge and the
    /// line's baseline, shared by drawing and click hit-testing.
    func checkboxRect(baselineY: CGFloat) -> NSRect {
        let size: CGFloat = 16
        let centerY = baselineY - font.capHeight / 2
        return NSRect(x: indent - Self.slotWidth + 1, y: centerY - size / 2, width: size, height: size)
    }
}

/// Layout manager that draws the editor's non-text decorations: block boxes
/// (code, callouts), quote bars, list bullets / numbers / checkboxes, inline
/// pills, rules and callout icons. The Markdown
/// syntax these stand in for is collapsed to zero width by `WysiwygEngine`.
final class CodeBlockLayoutManager: NSLayoutManager {
    /// Extra height added above and below a block box's text.
    var verticalPadding: CGFloat = 10
    var cornerRadius: CGFloat = 8
    /// The link under the pointer (design board 58): a faint fill behind it.
    var hoveredLink: NSRange?

    override func drawBackground(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        drawCodeBlockBoxes(forGlyphRange: glyphsToShow, at: origin)
        drawCalloutBoxes(forGlyphRange: glyphsToShow, at: origin)
        drawHoveredLink(forGlyphRange: glyphsToShow, at: origin)
        drawInlineFills(.inlinePill, padding: WysiwygEngine.pillPadding, radius: 4,
                        forGlyphRange: glyphsToShow, at: origin)
        drawInlineFills(.textHighlight, padding: 0, radius: 3,
                        forGlyphRange: glyphsToShow, at: origin)
        // Draw text backgrounds (highlight, selection) on top of the boxes so
        // selection inside a block stays visible.
        super.drawBackground(forGlyphRange: glyphsToShow, at: origin)
    }

    override func drawGlyphs(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        super.drawGlyphs(forGlyphRange: glyphsToShow, at: origin)
        drawQuoteBars(forGlyphRange: glyphsToShow, at: origin)
        drawListMarkers(forGlyphRange: glyphsToShow, at: origin)
        drawRules(forGlyphRange: glyphsToShow, at: origin)
        drawCalloutIcons(forGlyphRange: glyphsToShow, at: origin)
        drawCalloutTitles(forGlyphRange: glyphsToShow, at: origin)
        drawCodeLanguages(forGlyphRange: glyphsToShow, at: origin)
    }

    // MARK: - Geometry helpers

    /// Baseline of the line holding `glyphIndex`, in view coordinates.
    private func baselineY(forGlyphAt glyphIndex: Int, origin: NSPoint) -> CGFloat {
        let fragRect = lineFragmentRect(forGlyphAt: glyphIndex, effectiveRange: nil)
        return origin.y + fragRect.minY + location(forGlyphAt: glyphIndex).y
    }

    /// Enumerates runs of `key` intersecting the glyphs being drawn.
    private func enumerate(_ key: NSAttributedString.Key, in glyphsToShow: NSRange,
                           _ body: (Any, NSRange, NSRange) -> Void) {
        guard let storage = textStorage else { return }
        let charRange = characterRange(forGlyphRange: glyphsToShow, actualGlyphRange: nil)
        storage.enumerateAttribute(key, in: charRange, options: []) { value, range, _ in
            guard let value else { return }
            let gRange = self.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            guard gRange.length > 0 else { return }
            body(value, range, gRange)
        }
    }

    /// The bottom of a run's last line, minus that line's trailing line
    /// spacing, so a bar or box ends at the text rather than in the gap below.
    private func textBottom(ofCharacterRange range: NSRange, glyphRange gRange: NSRange) -> CGFloat {
        let lastGlyph = NSMaxRange(gRange) - 1
        let frag = lineFragmentRect(forGlyphAt: lastGlyph, effectiveRange: nil)
        let lastChar = max(range.location, NSMaxRange(range) - 1)
        let spacing = (textStorage?.attribute(.paragraphStyle, at: lastChar, effectiveRange: nil) as? NSParagraphStyle)?.lineSpacing ?? 0
        return frag.maxY - spacing
    }

    // MARK: - Blocks

    private func drawCodeBlockBoxes(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        enumerateBlockBoxes(.codeBlockBackground, forGlyphRange: glyphsToShow, at: origin) { rect, value, _ in
            guard (value as? Bool) == true else { return }
            MarkdownPalette.codeBlockBg.setFill()
            NSBezierPath(roundedRect: rect, xRadius: self.cornerRadius, yRadius: self.cornerRadius).fill()
        }
    }

    private func drawCalloutBoxes(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        enumerateBlockBoxes(.calloutType, forGlyphRange: glyphsToShow, at: origin) { rect, value, _ in
            guard let raw = value as? String else { return }
            CalloutKind(raw).background.setFill()
            NSBezierPath(roundedRect: rect, xRadius: self.cornerRadius, yRadius: self.cornerRadius).fill()
        }
    }

    private func drawCodeLanguages(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        enumerateBlockBoxes(.codeBlockBackground, forGlyphRange: glyphsToShow, at: origin) { rect, _, range in
            guard let language = self.textStorage?.attribute(.codeBlockLanguage, at: range.location, effectiveRange: nil) as? String,
                  !language.isEmpty else { return }
            let attrs: [NSAttributedString.Key: Any] = [
                .font: NSFont.systemFont(ofSize: TextSize.caption.rawValue),
                .foregroundColor: MarkdownPalette.tertiary,
            ]
            let size = (language as NSString).size(withAttributes: attrs)
            (language as NSString).draw(at: NSPoint(x: rect.maxX - 12 - size.width, y: rect.minY + 7), withAttributes: attrs)
        }
    }

    private func drawCalloutIcons(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        guard let storage = textStorage else { return }
        enumerate(.calloutIcon, in: glyphsToShow) { value, range, gRange in
            guard let raw = value as? String else { return }
            let kind = CalloutKind(raw)
            let config = NSImage.SymbolConfiguration(pointSize: 13, weight: .semibold)
                .applying(NSImage.SymbolConfiguration(paletteColors: [kind.color]))
            guard let image = NSImage(systemSymbolName: CalloutKind.symbolName(for: raw), accessibilityDescription: nil)?
                .withSymbolConfiguration(config) else { return }
            // The tagged character is collapsed syntax (`:::`, `> [!`), so
            // center on the title font's cap height: the line's largest font.
            let text = storage.string as NSString
            let line = text.paragraphRange(for: range)
            var font = NSFont.systemFont(ofSize: 14, weight: .semibold)
            var largest: CGFloat = 0
            storage.enumerateAttribute(.font, in: line) { value, _, _ in
                if let lineFont = value as? NSFont, lineFont.pointSize > largest {
                    largest = lineFont.pointSize
                    font = lineFont
                }
            }
            let baseline = self.baselineY(forGlyphAt: gRange.location, origin: origin)
            let centerY = baseline - font.capHeight / 2
            let size = image.size
            image.draw(in: NSRect(x: origin.x + 14 + (16 - size.width) / 2, y: centerY - size.height / 2,
                                  width: size.width, height: size.height))
        }
    }

    /// The title of a callout that names none, drawn over its hidden type
    /// word in the title's font and the callout's color.
    private func drawCalloutTitles(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        guard let storage = textStorage else { return }
        enumerate(.calloutTitle, in: glyphsToShow) { value, range, gRange in
            guard let title = value as? String, gRange.length > 0,
                  let font = storage.attribute(.font, at: range.location, effectiveRange: nil) as? NSFont else { return }
            let type = storage.attribute(.calloutType, at: range.location, effectiveRange: nil) as? String ?? "note"
            let fragment = lineFragmentRect(forGlyphAt: gRange.location, effectiveRange: nil)
            let x = origin.x + fragment.minX + location(forGlyphAt: gRange.location).x
            let baseline = baselineY(forGlyphAt: gRange.location, origin: origin)
            (title as NSString).draw(at: NSPoint(x: x, y: baseline - font.ascender),
                                     withAttributes: [.font: font, .foregroundColor: CalloutKind(type).color])
        }
    }

    /// Shared geometry for block backgrounds. Enumerates the WHOLE storage (not
    /// just `glyphsToShow`) so a partial redraw still paints each block's box at
    /// its full size; skips boxes outside the dirty region. Calls `draw` with the
    /// box rect (full column width, vertical padding applied) in view
    /// coordinates, the attribute value and the run's character range.
    private func enumerateBlockBoxes(_ key: NSAttributedString.Key,
                                     forGlyphRange glyphsToShow: NSRange,
                                     at origin: NSPoint,
                                     draw: (NSRect, Any, NSRange) -> Void) {
        guard let storage = textStorage, let container = textContainers.first else { return }
        let dirtyRect = boundingRect(forGlyphRange: glyphsToShow, in: container)
            .offsetBy(dx: origin.x, dy: origin.y)
        let fullRange = NSRange(location: 0, length: storage.length)

        storage.enumerateAttribute(key, in: fullRange, options: []) { value, range, _ in
            guard let value else { return }
            let blockGlyphRange = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            guard blockGlyphRange.length > 0 else { return }
            var rect = boundingRect(forGlyphRange: blockGlyphRange, in: container)
            rect.size.height = textBottom(ofCharacterRange: range, glyphRange: blockGlyphRange) - rect.minY
            rect.origin.x = 0
            rect.size.width = container.size.width
            rect = rect.insetBy(dx: 0, dy: -verticalPadding)
            rect.origin.x += origin.x
            rect.origin.y += origin.y
            guard rect.intersects(dirtyRect) else { return }
            draw(rect, value, range)
        }
    }

    // MARK: - Quotes

    private func drawQuoteBars(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        enumerate(.blockquoteDepth, in: glyphsToShow) { value, range, gRange in
            guard let depth = value as? Int, depth > 0 else { return }
            let top = origin.y + lineFragmentRect(forGlyphAt: gRange.location, effectiveRange: nil).minY
            let bottom = origin.y + textBottom(ofCharacterRange: range, glyphRange: gRange)
            for level in 1...depth {
                let x = origin.x + CGFloat(level - 1) * WysiwygEngine.quoteIndent
                (level == 1 ? MarkdownPalette.primary : MarkdownPalette.nestedQuoteBar).setFill()
                NSRect(x: x, y: top + 2, width: 3, height: max(0, bottom - top - 2)).fill()
            }
        }
    }

    // MARK: - Lists

    private func drawListMarkers(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        enumerate(.listMarker, in: glyphsToShow) { value, _, gRange in
            guard let marker = value as? ListMarker else { return }
            let baseline = baselineY(forGlyphAt: gRange.location, origin: origin)
            let centerY = baseline - marker.font.capHeight / 2
            let slotLeft = origin.x + marker.indent - ListMarker.slotWidth

            switch marker.kind {
            case .bullet(let level):
                let centerX = slotLeft + 7
                MarkdownPalette.primary.set()
                switch level % 3 {
                case 0:
                    let d = max(5, (marker.font.pointSize * 0.33).rounded())
                    NSBezierPath(ovalIn: NSRect(x: centerX - d / 2, y: centerY - d / 2, width: d, height: d)).fill()
                case 1:
                    let d = max(5, (marker.font.pointSize * 0.33).rounded())
                    let ring = NSBezierPath(ovalIn: NSRect(x: centerX - d / 2 + 0.6, y: centerY - d / 2 + 0.6, width: d - 1.2, height: d - 1.2))
                    ring.lineWidth = 1.2
                    ring.stroke()
                default:
                    let s = max(4, (marker.font.pointSize * 0.28).rounded())
                    NSRect(x: centerX - s / 2, y: centerY - s / 2, width: s, height: s).fill()
                }

            case .ordered(let label):
                let attrs: [NSAttributedString.Key: Any] = [
                    .font: NSFont.monospacedDigitSystemFont(ofSize: marker.font.pointSize, weight: .regular),
                    .foregroundColor: MarkdownPalette.primary,
                ]
                let size = (label as NSString).size(withAttributes: attrs)
                let font = attrs[.font] as! NSFont
                (label as NSString).draw(at: NSPoint(x: origin.x + marker.indent - 6 - size.width,
                                                     y: baseline - font.ascender),
                                         withAttributes: attrs)

            case .task(let checked):
                var box = marker.checkboxRect(baselineY: baseline)
                box.origin.x += origin.x
                if checked {
                    MarkdownPalette.accent.setFill()
                    NSBezierPath(roundedRect: box, xRadius: 4, yRadius: 4).fill()
                    let check = NSBezierPath()
                    check.move(to: NSPoint(x: box.minX + 4, y: box.midY + 0.5))
                    check.line(to: NSPoint(x: box.minX + 7, y: box.midY + 3.5))
                    check.line(to: NSPoint(x: box.maxX - 3.5, y: box.minY + 4.5))
                    check.lineWidth = 2
                    check.lineCapStyle = .round
                    check.lineJoinStyle = .round
                    NSColor.white.setStroke()
                    check.stroke()
                } else {
                    let path = NSBezierPath(roundedRect: box.insetBy(dx: 0.75, dy: 0.75), xRadius: 4, yRadius: 4)
                    path.lineWidth = 1.5
                    MarkdownPalette.checkboxBorder.setStroke()
                    path.stroke()
                }
            }
        }
    }

    // MARK: - Inline pills

    /// Rounded fills behind inline spans, sized to the glyphs (ascender to
    /// descender) rather than the whole line fragment, which includes the
    /// line spacing below the text.
    private func drawInlineFills(_ key: NSAttributedString.Key, padding: CGFloat, radius: CGFloat,
                                 forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        guard let storage = textStorage, let container = textContainers.first else { return }
        enumerate(key, in: glyphsToShow) { value, range, gRange in
            guard let color = value as? NSColor else { return }
            let font = (storage.attribute(.font, at: range.location, effectiveRange: nil) as? NSFont)
                ?? NSFont.systemFont(ofSize: 14)
            // One pill per line the span wraps across.
            enumerateLineFragments(forGlyphRange: gRange) { _, _, _, lineGlyphs, _ in
                let part = NSIntersectionRange(lineGlyphs, gRange)
                guard part.length > 0 else { return }
                let bounds = self.boundingRect(forGlyphRange: part, in: container)
                let baseline = self.baselineY(forGlyphAt: part.location, origin: origin)
                let inset: CGFloat = padding > 0 ? 2 : 1
                let pill = NSRect(x: origin.x + bounds.minX - padding,
                                  y: baseline - font.ascender - inset,
                                  width: bounds.width + padding,
                                  height: font.ascender - font.descender + inset * 2)
                color.setFill()
                NSBezierPath(roundedRect: pill, xRadius: radius, yRadius: radius).fill()
            }
        }
    }

    private func drawHoveredLink(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        guard let range = hoveredLink, let storage = textStorage, let container = textContainers.first,
              NSMaxRange(range) <= storage.length else { return }
        let gRange = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        guard NSIntersectionRange(gRange, glyphsToShow).length > 0 else { return }
        let font = (storage.attribute(.font, at: range.location, effectiveRange: nil) as? NSFont)
            ?? NSFont.systemFont(ofSize: 14)
        // One fill per line the link wraps across.
        enumerateLineFragments(forGlyphRange: gRange) { _, _, _, lineGlyphs, _ in
            let part = NSIntersectionRange(lineGlyphs, gRange)
            guard part.length > 0 else { return }
            let bounds = self.boundingRect(forGlyphRange: part, in: container)
            let baseline = self.baselineY(forGlyphAt: part.location, origin: origin)
            let fill = NSRect(x: origin.x + bounds.minX - 3, y: baseline - font.ascender - 2,
                              width: bounds.width + 6, height: font.ascender - font.descender + 4)
            MarkdownPalette.primary.withAlphaComponent(0.07).setFill()
            NSBezierPath(roundedRect: fill, xRadius: 4, yRadius: 4).fill()
        }
    }

    // MARK: - Rules

    private func drawRules(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        guard let container = textContainers.first else { return }
        enumerate(.horizontalRule, in: glyphsToShow) { _, range, gRange in
            let frag = lineFragmentRect(forGlyphAt: gRange.location, effectiveRange: nil)
            let bottom = textBottom(ofCharacterRange: range, glyphRange: gRange)
            let y = origin.y + ((frag.minY + bottom) / 2).rounded()
            MarkdownPalette.rule.setFill()
            NSRect(x: origin.x, y: y, width: container.size.width, height: 1).fill()
        }
    }
}
