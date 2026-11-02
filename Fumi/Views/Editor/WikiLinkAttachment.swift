import AppKit

extension NSAttributedString.Key {
    /// On a shown `[[link]]` (its text, or its attachment): the link's inner
    /// text, `target#heading|alias`. A click opens the note.
    static let wikiLink = NSAttributedString.Key("fumi.wikiLink")
    /// On a link's text, `[text](url)` or a bare URL: the address. A click
    /// opens it in the browser.
    static let webLink = NSAttributedString.Key("fumi.webLink")
}

/// A `[[link]]` whose shown text isn't in the file — the linked note's
/// front matter title, or `Title › Heading` — drawn in the link style in the
/// line's own font and color (design board 58). Stands for its source like
/// an image does, so the source returns as the caret reaches it.
final class WikiLinkAttachment: NSTextAttachment, MarkdownBackedAttachment {
    let markdownSource: String
    /// The link's inner text, `target#heading|alias`.
    let link: String
    private let plain: NSImage
    private let hovered: NSImage

    var isHovered = false {
        didSet { image = isHovered ? hovered : plain }
    }

    /// `text` carries its fonts (composite CJK runs included) and color.
    init(text: NSAttributedString, link: String, font: NSFont, markdownSource: String, maxWidth: CGFloat) {
        self.markdownSource = markdownSource
        self.link = link
        let size = NSSize(width: min(ceil(text.size().width) + 1, maxWidth),
                          height: ceil(font.ascender - font.descender))
        plain = Self.render(text, size: size, font: font, hovered: false)
        hovered = Self.render(text, size: size, font: font, hovered: true)
        super.init(data: nil, ofType: nil)
        image = plain
        // The baseline lines up with the text around it.
        bounds = NSRect(x: 0, y: font.descender, width: size.width, height: size.height)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    private static func render(_ text: NSAttributedString, size: NSSize, font: NSFont, hovered: Bool) -> NSImage {
        NSImage(size: size, flipped: true) { rect in
            if hovered {
                MarkdownPalette.primary.withAlphaComponent(0.07).setFill()
                NSBezierPath(roundedRect: rect, xRadius: 4, yRadius: 4).fill()
            }
            let baseline = font.ascender
            text.draw(with: NSRect(x: 0, y: 0, width: rect.width, height: rect.height),
                      options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
            // The same underline as a link in text (style.link), darker under the pointer.
            let underline = hovered ? MarkdownPalette.primary.withAlphaComponent(0.7) : MarkdownPalette.linkUnderline
            underline.setFill()
            NSRect(x: 0, y: (baseline + 3).rounded(), width: rect.width - 1, height: 1).fill()
            return true
        }
    }
}
