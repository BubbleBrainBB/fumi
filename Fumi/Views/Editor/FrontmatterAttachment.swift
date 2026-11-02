import AppKit

/// The frontmatter drawn as a properties panel (key column + values, list
/// values as tag pills, a rule underneath). Collapses the raw YAML like
/// `TableAttachment` does, so it re-expands to source while being edited.
final class FrontmatterAttachment: NSTextAttachment, MarkdownBackedAttachment {
    let markdownSource: String

    static let rowHeight: CGFloat = 26
    static let keyColumnWidth: CGFloat = 96
    static let bottomPadding: CGFloat = 10

    init(frontmatter: Frontmatter, markdownSource: String, maxWidth: CGFloat) {
        self.markdownSource = markdownSource
        super.init(data: nil, ofType: nil)
        let rendered = Self.render(frontmatter.fields, width: maxWidth)
        image = rendered
        bounds = NSRect(origin: .zero, size: rendered.size)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    /// Tag pill colors from the design comps, cycled across list items.
    private static let pillColors: [(bg: NSColor, fg: NSColor)] = [
        (MarkdownPalette.adaptive(0xFBF0D9, 0x3A3220), MarkdownPalette.adaptive(0x7A5410, 0xE8C77A)),
        (MarkdownPalette.adaptive(0xE8F1FB, 0x1E2A38), MarkdownPalette.adaptive(0x1F4E80, 0x9CC4EE)),
        (MarkdownPalette.adaptive(0xFDECEE, 0x3A2224), MarkdownPalette.adaptive(0x8A2F36, 0xEBA0A6)),
        (MarkdownPalette.adaptive(0xE7F5EC, 0x1E2E25), MarkdownPalette.adaptive(0x2F6B4A, 0x8FCBA6)),
        (MarkdownPalette.adaptive(0xF3EEF8, 0x2E2638), MarkdownPalette.adaptive(0x6B4A8A, 0xC6A9E0)),
    ]

    static func render(_ fields: [Frontmatter.Field], width: CGFloat) -> NSImage {
        let rows = max(fields.count, 1)
        let size = NSSize(width: max(width, 1), height: CGFloat(rows) * rowHeight + bottomPadding + 1)
        let font = NSFont.systemFont(ofSize: TextSize.body.rawValue)

        return NSImage(size: size, flipped: true) { _ in
            let keyAttrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: MarkdownPalette.tertiary]
            let valueAttrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: MarkdownPalette.primary]
            let textY = (rowHeight - ceil(font.ascender - font.descender)) / 2

            if fields.isEmpty {
                ("Properties" as NSString).draw(at: NSPoint(x: 0, y: textY), withAttributes: keyAttrs)
            }

            for (i, field) in fields.enumerated() {
                let y = CGFloat(i) * rowHeight
                (field.key as NSString).draw(
                    in: NSRect(x: 0, y: y + textY, width: keyColumnWidth - 8, height: rowHeight),
                    withAttributes: keyAttrs.merging([.paragraphStyle: truncating()]) { $1 })

                let valueX = keyColumnWidth
                let valueWidth = size.width - valueX
                switch field.value {
                case .text(let text):
                    (text as NSString).draw(
                        in: NSRect(x: valueX, y: y + textY, width: valueWidth, height: rowHeight),
                        withAttributes: valueAttrs.merging([.paragraphStyle: truncating()]) { $1 })
                case .list(let items):
                    var x = valueX
                    let pillHeight: CGFloat = 20
                    for (j, item) in items.enumerated() {
                        let colors = pillColors[j % pillColors.count]
                        let attrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: colors.fg]
                        let textWidth = ceil((item as NSString).size(withAttributes: attrs).width)
                        let pill = NSRect(x: x, y: y + (rowHeight - pillHeight) / 2, width: textWidth + 12, height: pillHeight)
                        guard pill.maxX <= size.width else { break }
                        colors.bg.setFill()
                        NSBezierPath(roundedRect: pill, xRadius: 4, yRadius: 4).fill()
                        (item as NSString).draw(at: NSPoint(x: pill.minX + 6, y: y + textY), withAttributes: attrs)
                        x = pill.maxX + 4
                    }
                }
            }

            MarkdownPalette.rule.setFill()
            NSRect(x: 0, y: size.height - 1, width: size.width, height: 1).fill()
            return true
        }
    }

    private static func truncating() -> NSParagraphStyle {
        let p = NSMutableParagraphStyle()
        p.lineBreakMode = .byTruncatingTail
        return p
    }
}
