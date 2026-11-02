import AppKit
import Foundation

/// Represents a Markdown inline rule for the WYSIWYG engine.
struct WysiwygInlineRule {
    enum Kind { case plain, code, hashtag }

    let pattern: NSRegularExpression
    /// Returns the marker ranges (to be hidden/faded) and content ranges (to be styled).
    let analyze: (NSTextCheckingResult, NSString) -> (markers: [NSRange], content: [NSRange])
    let contentAttributes: [NSAttributedString.Key: Any]
    var kind: Kind = .plain
}

/// Fonts and attributes for every Markdown element, after the 拾页 Markdown
/// spec (design canvas board ⑩): body 16 / line height 1.75 in ink, headings
/// in the same ink, quiet markers, tinted blocks.
struct MarkdownStyle {
    let fontSize: CGFloat
    /// Fixed height of a body line (font size × the line-height setting).
    /// Every body line gets exactly this, whether it holds CJK or Latin text,
    /// so mixed paragraphs keep an even rhythm (board ⑲ ①).
    let lineHeight: CGFloat
    let bodyFont: NSFont
    /// Pairs the body family with a matching CJK face when the family is
    /// Latin-only (New York → Songti SC, Helvetica → PingFang SC …), the
    /// native take on css-composite-font's `unicode-range` dispatch. Inactive
    /// for System / SF Mono / families that already cover Han, where the
    /// system cascade already pairs the two scripts.
    let compositeFont: CompositeFont

    /// Also the style of every empty line: a blank line is a full body line
    /// wherever the caret is, so ↩ always moves down one line and nothing
    /// shifts when the caret moves.
    let baseParagraph: [NSAttributedString.Key: Any]

    /// The note's first `#` heading, shown as its title.
    let title: [NSAttributedString.Key: Any]
    /// `#`…`######` (index 0 = a later `#`).
    let headings: [[NSAttributedString.Key: Any]]

    let bold: [NSAttributedString.Key: Any]
    let boldItalic: [NSAttributedString.Key: Any]
    let italic: [NSAttributedString.Key: Any]
    let strikethrough: [NSAttributedString.Key: Any]
    let inlineCode: [NSAttributedString.Key: Any]
    let link: [NSAttributedString.Key: Any]
    let highlight: [NSAttributedString.Key: Any]
    let hashtag: [NSAttributedString.Key: Any]

    let codeBlock: [NSAttributedString.Key: Any]
    let calloutTitleFont: NSFont
    let calloutBodyFont: NSFont
    let quoteAttributionFont: NSFont
    let rawSourceFont: NSFont
}

/// The WYSIWYG engine that applies Markdown styling and marker hiding.
///
/// Key principle: the underlying text storage always contains the raw Markdown.
/// Styling is applied via temporary attributes. Syntax characters are collapsed
/// to zero width; inline and heading syntax is revealed (in the marker color)
/// only on the paragraph holding the caret. Bullets, checkboxes, quote bars and
/// rules are drawn by `CodeBlockLayoutManager` in place of their syntax.
final class WysiwygEngine {
    private let style: MarkdownStyle
    private let inlineRules: [WysiwygInlineRule]
    /// Rendered table grids cached by "width|source" so re-collapsing tables on
    /// every keystroke does not re-render their images.
    private var tableCache: [String: TableAttachment] = [:]
    /// Rendered math cached by "display|width|source" for the same reason.
    private var mathCache: [String: MathAttachment] = [:]
    /// Video cards cached by "width|path|source" so an inline player keeps
    /// tracking the same attachment object across restyles. Rebuilt once the
    /// poster arrives.
    private var videoCache: [String: (attachment: VideoAttachment, hasPoster: Bool)] = [:]
    /// Images and the frontmatter panel, reused while unchanged so a restyle
    /// keeps the same attachment objects: nothing to re-measure or redraw,
    /// and `applyChanges` sees those runs as unchanged. Image keys count
    /// repeats of the same image within a pass.
    private var imageCache: [String: ImageTextAttachment] = [:]
    private var imageOccurrences: [String: Int] = [:]
    private var frontmatterCache: (key: String, attachment: FrontmatterAttachment)?

    /// Obsidian embed: `![[file.ext]]`, `![[file|300]]`.
    private static let embedPattern = try! NSRegularExpression(pattern: #"!\[\[([^\]\n]+)\]\]"#)

    /// Indent per blockquote level: 3pt bar + 14pt gap.
    static let quoteIndent: CGFloat = 17
    /// Horizontal padding inside inline pills (inline code, hashtags).
    static let pillPadding: CGFloat = 4
    static let calloutTextIndent: CGFloat = 40
    static let codeTextIndent: CGFloat = 16
    static let listIndentStep: CGFloat = ListMarker.slotWidth

    /// Sizes in a note as multiples of the body size (Settings › Writing ›
    /// Size), so the title and headings stay above the body at any size:
    /// 30 / 26 / 21 / 18 at the default 16. Code, tags, callout titles and
    /// other small runs take one step under the body, never under 12.
    static let titleScale: CGFloat = 1.875
    static let headingScales: [CGFloat] = [1.625, 1.3125, 1.125]

    static func smallSize(_ body: CGFloat) -> CGFloat {
        max(TextSize.caption.rawValue, (body * 0.875).rounded())
    }

    init(style: MarkdownStyle) {
        self.style = style
        self.inlineRules = WysiwygEngine.buildInlineRules(style: style)
    }

    // MARK: - Style factory

    /// Resolve a font for the editor body in the user's chosen family, at the
    /// given size and weight. "System" and "SF Mono" use the system font APIs
    /// (which honor weight directly); named families match via font descriptor,
    /// falling back to the system font if the family is unavailable.
    static func resolveFont(name: String, size: CGFloat, weight: NSFont.Weight = .regular) -> NSFont {
        switch name {
        case "System", "":
            return NSFont.systemFont(ofSize: size, weight: weight)
        case "SF Mono":
            return NSFont.monospacedSystemFont(ofSize: size, weight: weight)
        case "New York":
            // New York is the system serif font; it is not exposed as a regular
            // font family by NSFontManager, so resolve it through the system
            // font descriptor's serif design. Falls back to the default system
            // font if the serif design is unavailable.
            let descriptor = NSFont.systemFont(ofSize: size, weight: weight)
                .fontDescriptor.withDesign(.serif)
            return descriptor.flatMap { NSFont(descriptor: $0, size: size) }
                ?? NSFont.systemFont(ofSize: size, weight: weight)
        default:
            let descriptor = NSFontDescriptor(fontAttributes: [
                .family: name,
                .traits: [NSFontDescriptor.TraitKey.weight: weight.rawValue]
            ])
            return NSFont(descriptor: descriptor, size: size) ?? NSFont.systemFont(ofSize: size, weight: weight)
        }
    }

    static func paragraph(indent: CGFloat = 0, tail: CGFloat = 0, lineSpacing: CGFloat = 0,
                          lineHeight: CGFloat? = nil,
                          before: CGFloat = 0, after: CGFloat = 0) -> NSMutableParagraphStyle {
        let p = NSMutableParagraphStyle()
        p.firstLineHeadIndent = indent
        p.headIndent = indent
        p.tailIndent = tail
        p.lineSpacing = lineSpacing
        if let lineHeight {
            p.minimumLineHeight = lineHeight
            p.maximumLineHeight = lineHeight
        }
        p.paragraphSpacingBefore = before
        p.paragraphSpacing = after
        return p
    }

    /// - Parameter strongColor: Bold text color — the accent (Settings ›
    ///   Appearance), passed in by the text view.
    static func makeStyle(fontSize: CGFloat = 16, lineHeight: CGFloat = 1.75, fontName: String = "System",
                          strongColor: NSColor = MarkdownPalette.strong) -> MarkdownStyle {
        let ink = MarkdownPalette.primary
        let bodyLineHeight = (fontSize * lineHeight).rounded()

        // Body typeface follows the user's Editor Font choice. Bold/italic are
        // derived from the same family so they stay consistent.
        let bodyFont = resolveFont(name: fontName, size: fontSize)
        // Bold, not Semibold: a Semibold Latin face (New York, SF) sits too
        // close to Regular to stand out.
        let boldFont = resolveFont(name: fontName, size: fontSize, weight: .bold)
        let italicFont = NSFontManager.shared.convert(bodyFont, toHaveTrait: .italicFontMask)
        let boldItalicFont = NSFontManager.shared.convert(boldFont, toHaveTrait: .italicFontMask)

        // Headings wrap at 1.3× their size, never tighter than a body line,
        // so a body-size #### keeps the body's rhythm.
        func heading(size: CGFloat, weight: NSFont.Weight, before: CGFloat, after: CGFloat,
                     color: NSColor = ink) -> [NSAttributedString.Key: Any] {
            [
                .font: resolveFont(name: fontName, size: size, weight: weight),
                .foregroundColor: color,
                .paragraphStyle: paragraph(lineHeight: max((size * 1.3).rounded(), bodyLineHeight),
                                           before: before, after: after),
            ]
        }

        func scaled(_ ratio: CGFloat) -> CGFloat { (fontSize * ratio).rounded() }
        let small = smallSize(fontSize)
        let mono = NSFont.monospacedSystemFont(ofSize: small, weight: .regular)

        return MarkdownStyle(
            fontSize: fontSize,
            lineHeight: bodyLineHeight,
            bodyFont: bodyFont,
            compositeFont: CompositeFont(baseFont: bodyFont, familyName: fontName),
            baseParagraph: [
                .font: bodyFont,
                .foregroundColor: ink,
                .paragraphStyle: paragraph(lineHeight: bodyLineHeight),
            ],
            // PingFang's Bold reads heavy in Chinese headings: Semibold, a
            // touch smaller, sitting closer to its paragraph (board ⑲ ④).
            title: heading(size: scaled(titleScale), weight: .semibold, before: 0, after: 10),
            headings: [
                heading(size: scaled(headingScales[0]), weight: .semibold, before: 24, after: 8),
                heading(size: scaled(headingScales[1]), weight: .semibold, before: 16, after: 8),
                heading(size: scaled(headingScales[2]), weight: .semibold, before: 12, after: 4),
                heading(size: fontSize, weight: .semibold, before: 8, after: 2),
                heading(size: fontSize, weight: .semibold, before: 8, after: 2),
                heading(size: fontSize, weight: .semibold, before: 8, after: 2, color: MarkdownPalette.tertiary),
            ],
            bold: [.font: boldFont, .foregroundColor: strongColor],
            boldItalic: [.font: boldItalicFont, .foregroundColor: strongColor],
            italic: [.font: italicFont],
            strikethrough: [
                .foregroundColor: MarkdownPalette.faint,
                .strikethroughStyle: NSUnderlineStyle.single.rawValue,
                .strikethroughColor: MarkdownPalette.faint,
            ],
            inlineCode: [
                .font: mono,
                .foregroundColor: MarkdownPalette.inlineCode,
            ],
            link: [
                .foregroundColor: ink,
                .underlineStyle: NSUnderlineStyle.single.rawValue,
                .underlineColor: MarkdownPalette.linkUnderline,
            ],
            // Drawn by CodeBlockLayoutManager around the glyphs only;
            // .backgroundColor would also paint the line spacing below.
            highlight: [.textHighlight: MarkdownPalette.highlight],
            hashtag: [
                .font: resolveFont(name: fontName, size: small),
                .foregroundColor: MarkdownPalette.secondary,
            ],
            codeBlock: [
                .font: mono,
                .foregroundColor: ink,
                // The block background is drawn as one rounded box by
                // CodeBlockLayoutManager (see the .codeBlockBackground marker
                // applied in applyStyle), not via .backgroundColor which paints
                // a ragged per-line fill.
                .paragraphStyle: paragraph(indent: codeTextIndent, tail: -codeTextIndent, lineSpacing: 6),
            ],
            calloutTitleFont: NSFont.systemFont(ofSize: small, weight: .semibold),
            calloutBodyFont: bodyFont,
            quoteAttributionFont: resolveFont(name: fontName, size: small),
            rawSourceFont: mono
        )
    }

    // MARK: - Inline rules

    private static func buildInlineRules(style: MarkdownStyle) -> [WysiwygInlineRule] {
        func wrapped(_ pattern: String, markerLength: Int, attrs: [NSAttributedString.Key: Any],
                     kind: WysiwygInlineRule.Kind = .plain) -> WysiwygInlineRule {
            WysiwygInlineRule(
                pattern: try! NSRegularExpression(pattern: pattern),
                analyze: { match, _ in
                    let full = match.range
                    let content = match.range(at: 1)
                    return ([NSRange(location: full.location, length: markerLength),
                             NSRange(location: NSMaxRange(content), length: markerLength)], [content])
                },
                contentAttributes: attrs,
                kind: kind
            )
        }

        return [
            // Images: ![alt](url), the destination read by `InlineLink`
            WysiwygInlineRule(
                pattern: InlineLink.imagePattern,
                analyze: { match, str in
                    let full = match.range
                    let altRange = match.range(at: 1)
                    var markers: [NSRange] = []
                    if altRange.location != NSNotFound && altRange.length > 0 {
                        markers.append(NSRange(location: full.location, length: altRange.location - full.location))
                        markers.append(NSRange(location: altRange.location + altRange.length, length: full.location + full.length - (altRange.location + altRange.length)))
                    } else {
                        markers.append(full)
                    }
                    return (markers, altRange.length > 0 ? [altRange] : [])
                },
                contentAttributes: [.foregroundColor: MarkdownPalette.tertiary]
            ),

            // Links: [text](url)
            WysiwygInlineRule(
                pattern: InlineLink.linkPattern,
                analyze: { match, str in
                    let full = match.range
                    let textRange = match.range(at: 1)
                    var markers: [NSRange] = []
                    markers.append(NSRange(location: full.location, length: textRange.location - full.location))
                    markers.append(NSRange(location: textRange.location + textRange.length, length: full.location + full.length - (textRange.location + textRange.length)))
                    return (markers, [textRange])
                },
                contentAttributes: style.link
            ),

            wrapped(#"\*{3}(.+?)\*{3}"#, markerLength: 3, attrs: style.boldItalic),
            wrapped(#"\*{2}(.+?)\*{2}"#, markerLength: 2, attrs: style.bold),
            wrapped(#"(?<!\*)\*(?!\s)([^*]+?)(?<!\s)\*(?!\*)"#, markerLength: 1, attrs: style.italic),
            wrapped(#"~~(.+?)~~"#, markerLength: 2, attrs: style.strikethrough),
            wrapped(#"==(?![\s=])(.+?)(?<!\s)=="#, markerLength: 2, attrs: style.highlight),

            // Underline: <u>text</u> (HTML, as pasted from web clippings)
            WysiwygInlineRule(
                pattern: try! NSRegularExpression(pattern: #"<u>(.+?)</u>"#),
                analyze: { match, _ in
                    let content = match.range(at: 1)
                    return ([NSRange(location: match.range.location, length: 3),
                             NSRange(location: NSMaxRange(content), length: 4)], [content])
                },
                contentAttributes: [
                    .underlineStyle: NSUnderlineStyle.single.rawValue,
                    .underlineColor: MarkdownPalette.primary,
                ]
            ),
            wrapped(#"`([^`]+)`"#, markerLength: 1, attrs: style.inlineCode, kind: .code),

            // Hashtags: #word/path (any script, so #决策 works too)
            WysiwygInlineRule(
                pattern: try! NSRegularExpression(pattern: #"(?m)(?<=^|[\s(\[])#[\p{L}][\p{L}\p{N}_/-]*"#),
                analyze: { match, _ in ([], [match.range]) },
                contentAttributes: style.hashtag,
                kind: .hashtag
            ),
        ]
    }

    // MARK: - Apply styling

    /// Applies WYSIWYG styling to the text storage.
    /// - Parameters:
    ///   - storage: The text storage to style. Holds raw Markdown on entry.
    ///   - cursorLocation: The caret, in raw-Markdown coordinates.
    ///   - maxWidth: The text column width, for images, tables, math and the
    ///     properties panel.
    func applyStyle(to storage: NSTextStorage, cursorLocation: Int,
                    activeTableIndex: Int = -1, activeMathIndex: Int = -1,
                    maxWidth: CGFloat = 640) {
        let fullRange = NSRange(location: 0, length: storage.length)
        let nsString = storage.string as NSString

        storage.beginEditing()
        imageOccurrences.removeAll(keepingCapacity: true)
        wikiOccurrences.removeAll(keepingCapacity: true)

        // Start from the body style everywhere: one pass that also drops
        // every mark from the last styling.
        storage.setAttributes(style.baseParagraph, range: fullRange)

        // Lines, with and without their terminators. Block decorations go on
        // the enclosing range so consecutive lines form one continuous run.
        var lines: [(range: NSRange, enclosing: NSRange, text: String)] = []
        nsString.enumerateSubstrings(in: fullRange, options: [.byLines]) { substring, range, enclosing, _ in
            lines.append((range, enclosing, substring ?? ""))
        }

        let frontmatter = Frontmatter.parse(nsString)
        let frontmatterActive = frontmatter.map {
            cursorLocation > 0 && cursorLocation <= NSMaxRange($0.range)
        } ?? false

        // Line-start offsets of fences that belong to a *closed* code block.
        // A stray or not-yet-closed fence is absent here, so it is processed as
        // an ordinary line instead of turning the rest of the document into code.
        let realFenceLocations = scanCodeBlocks(in: nsString).fenceLocations
        let quoteGroups = Self.quoteGroups(lines.map(\.text))

        var inCodeBlock = false
        var codeOpen: (range: NSRange, language: String) = (NSRange(), "")
        var codeBlockRanges: [NSRange] = []

        var inCallout = false
        var calloutStart = 0
        var calloutType = "note"

        // The first heading before any other content is the note's title.
        var seenContent = false
        // Code blocks and callouts get extra space around them, applied after
        // the loop so the neighbors' own paragraph styles don't overwrite it.
        var spacedBlocks: [NSRange] = []

        for (index, line) in lines.enumerated() {
            let lineRange = line.range
            let trimmed = line.text.trimmingCharacters(in: .whitespaces)
            let isActive = NSLocationInRange(cursorLocation, lineRange) || cursorLocation == NSMaxRange(lineRange)

            // Frontmatter: replaced by the properties panel at the end, or shown
            // as quiet raw source while the caret is inside it.
            if let frontmatter, NSLocationInRange(lineRange.location, frontmatter.range) {
                if frontmatterActive {
                    let isLast = NSMaxRange(lineRange) == NSMaxRange(frontmatter.range)
                    storage.addAttributes([
                        .font: style.rawSourceFont,
                        .foregroundColor: MarkdownPalette.tertiary,
                        .paragraphStyle: Self.paragraph(lineSpacing: 4, after: isLast ? 16 : 0),
                    ], range: line.enclosing)
                }
                continue
            }

            // Code fence detection (only fences that pair into a closed block).
            if realFenceLocations.contains(lineRange.location) {
                if !inCodeBlock {
                    inCodeBlock = true
                    let info = trimmed.drop(while: { $0 == "`" || $0 == "~" })
                    codeOpen = (lineRange, info.split(separator: " ").first.map(String.init) ?? "")
                } else {
                    inCodeBlock = false
                    seenContent = true
                    let blockRange = NSRange(location: codeOpen.range.location,
                                             length: NSMaxRange(lineRange) - codeOpen.range.location)
                    codeBlockRanges.append(blockRange)
                    storage.addAttributes(style.codeBlock, range: blockRange)
                    storage.addAttribute(.codeBlockBackground, value: true, range: blockRange)
                    storage.addAttribute(.codeBlockLanguage, value: codeOpen.language, range: blockRange)
                    let bodyStart = NSMaxRange(codeOpen.range) + 1
                    if lineRange.location > bodyStart {
                        highlightSyntax(storage, in: NSRange(location: bodyStart, length: lineRange.location - bodyStart),
                                        language: codeOpen.language)
                    }
                    for fence in [codeOpen.range, lineRange] {
                        if NSLocationInRange(cursorLocation, fence) || cursorLocation == NSMaxRange(fence) {
                            applyMarkerStyle(storage, range: fence)
                        } else {
                            collapse(storage, fence)
                        }
                    }
                    spacedBlocks.append(blockRange)
                }
                continue
            }

            if inCodeBlock { continue }

            // Callout fence: ::: [type] [title] … :::
            if trimmed.hasPrefix(":::") {
                if !inCallout {
                    inCallout = true
                    seenContent = true
                    calloutStart = lineRange.location
                    let after = trimmed.dropFirst(3).trimmingCharacters(in: .whitespaces)
                    calloutType = after.split(whereSeparator: { $0 == " " }).first.map(String.init) ?? "note"
                    styleCalloutOpeningLine(storage, line: line.text, lineRange: lineRange, type: calloutType, isActive: isActive)
                } else {
                    inCallout = false
                    let blockRange = NSRange(location: calloutStart, length: NSMaxRange(lineRange) - calloutStart)
                    storage.addAttribute(.calloutType, value: calloutType, range: blockRange)
                    storage.addAttribute(.paragraphStyle, value: calloutParagraph(), range: lineRange)
                    // Closing ::: line: collapsed, shown faded only when the caret
                    // is on it, so it stays editable.
                    if isActive {
                        applyMarkerStyle(storage, range: lineRange)
                    } else {
                        collapse(storage, lineRange)
                    }
                    spacedBlocks.append(blockRange)
                }
                continue
            }

            if inCallout {
                // Body line. Inline rules (bold/italic/links) still run afterwards.
                storage.addAttributes([.paragraphStyle: calloutParagraph(), .font: style.calloutBodyFont], range: lineRange)
                continue
            }

            // Empty lines keep the body style: one full line each, never
            // collapsed. Collapsing them (or only the ones away from the
            // caret) made ↩ look dead and made text jump as the caret moved.
            if trimmed.isEmpty { continue }

            // Headings
            if let heading = parseHeading(line: line.text) {
                let isTitle = heading.level == 1 && !seenContent
                seenContent = true
                let attrs = isTitle ? style.title : style.headings[heading.level - 1]
                storage.addAttributes(attrs, range: lineRange)
                let marker = NSRange(location: lineRange.location + heading.markerRange.location,
                                     length: heading.markerRange.length)
                // Live preview: `## ` shows inline on the caret's line only.
                if isActive {
                    applyMarkerStyle(storage, range: marker)
                } else {
                    collapse(storage, marker)
                }
                continue
            }
            seenContent = true

            // A `> [!type]` callout: its first line is the title, the rest
            // its body, in one tinted box.
            if let group = quoteGroups[index], let type = group.calloutType {
                if index == group.start {
                    styleCalloutTitleLine(storage, line: line.text, lineRange: lineRange, type: type, isActive: isActive)
                } else {
                    styleQuoteLine(storage, line: line.text, lineRange: lineRange, enclosing: line.enclosing,
                                   calloutIndent: Self.calloutTextIndent, isActive: isActive)
                }
                if index == group.end {
                    let start = lines[group.start].range.location
                    let blockRange = NSRange(location: start, length: NSMaxRange(lineRange) - start)
                    storage.addAttribute(.calloutType, value: type, range: blockRange)
                    spacedBlocks.append(blockRange)
                }
                continue
            }

            // Blockquote (or, when it opens with an emoji, a gray callout).
            if let group = quoteGroups[index] {
                styleQuoteLine(storage, line: line.text, lineRange: lineRange, enclosing: line.enclosing,
                               calloutIndent: group.isEmoji ? 14 : nil, isActive: isActive)
                if group.isEmoji, index == group.end {
                    let start = lines[group.start].range.location
                    let blockRange = NSRange(location: start, length: NSMaxRange(lineRange) - start)
                    storage.addAttribute(.calloutType, value: CalloutKind.emojiQuote, range: blockRange)
                    spacedBlocks.append(blockRange)
                }
                continue
            }

            // Horizontal rule
            if isHorizontalRule(trimmed) {
                if isActive {
                    applyMarkerStyle(storage, range: lineRange)
                } else {
                    collapse(storage, lineRange)
                    storage.addAttribute(.horizontalRule, value: true, range: lineRange)
                }
                storage.addAttribute(.paragraphStyle,
                                     value: Self.paragraph(lineHeight: style.lineHeight, before: 4, after: 4),
                                     range: lineRange)
                continue
            }

            // List items (bullets, numbers, to-dos); anything else is a paragraph.
            if !styleListItem(storage, line: line.text, lineRange: lineRange, isActive: isActive) {
                hideLeadingSpaces(storage, line: line.text, lineRange: lineRange)
            }
        }

        for block in spacedBlocks {
            addBlockSpacing(storage, blockRange: block)
        }

        // Collapse images and Obsidian embeds (`![](…)`, `![[…]]`) into
        // attachments: loaded images, and local videos as poster cards.
        // Live preview: while the caret touches one, its source stays visible
        // and the image is shown on the line below it instead of replacing it.
        // That attachment has an empty `markdownSource`, so it adds nothing to
        // the raw document.
        var replacements: [(range: NSRange, attachment: NSTextAttachment, revealed: Bool)] = []
        func isInCode(_ location: Int) -> Bool {
            codeBlockRanges.contains(where: { NSLocationInRange(location, $0) })
        }
        func touchesCaret(_ range: NSRange) -> Bool {
            cursorLocation >= range.location && cursorLocation <= NSMaxRange(range)
        }
        if let imageRule = inlineRules.first {
            imageRule.pattern.enumerateMatches(in: storage.string, range: fullRange) { match, _, _ in
                guard let match, !isInCode(match.range.location) else { return }
                let urlString = nsString.substring(with: InlineLink.destinationRange(of: match))
                let revealed = touchesCaret(match.range)
                let markdown = revealed ? "" : nsString.substring(with: match.range)
                let alt = nsString.substring(with: match.range(at: 1))

                if VideoFile.isVideo(urlString), !isRemoteImageURL(urlString),
                   let url = ImageLoader.shared.resolveEmbed(urlString) {
                    replacements.append((match.range, videoAttachment(url: url, source: markdown, maxWidth: maxWidth), revealed))
                } else if let content = ImageLoader.shared.loadSync(urlString: urlString) {
                    replacements.append((match.range, imageAttachment(content: content, source: markdown, url: urlString,
                                                                      caption: alt, maxWidth: maxWidth,
                                                                      revealed: revealed), revealed))
                } else if isRemoteImageURL(urlString) {
                    ImageLoader.shared.loadAsync(urlString: urlString) { image in
                        if image != nil {
                            NotificationCenter.default.post(name: .imageDidLoad, object: nil)
                        }
                    }
                }
            }
        }
        Self.embedPattern.enumerateMatches(in: storage.string, range: fullRange) { match, _, _ in
            guard let match, !isInCode(match.range.location) else { return }
            let target = nsString.substring(with: match.range(at: 1))
            let revealed = touchesCaret(match.range)
            let markdown = revealed ? "" : nsString.substring(with: match.range)
            guard let url = ImageLoader.shared.resolveEmbed(target) else { return }
            if VideoFile.isVideo(url.path) {
                replacements.append((match.range, videoAttachment(url: url, source: markdown, maxWidth: maxWidth), revealed))
            } else if let content = ImageLoader.shared.loadSync(urlString: url.path) {
                replacements.append((match.range, imageAttachment(content: content, source: markdown, url: url.path,
                                                                  caption: "", maxWidth: maxWidth, revealed: revealed),
                                     revealed))
            }
        }
        // Links to notes whose shown text isn't in the file — the note's
        // front matter title, `Title › Heading` — become attachments that
        // draw it (design board 58); the rest are styled in place below.
        WikiLink.pattern.enumerateMatches(in: storage.string, range: fullRange) { match, _, _ in
            guard let match, !isInCode(match.range.location), !touchesCaret(match.range) else { return }
            if let frontmatter, NSLocationInRange(match.range.location, frontmatter.range) { return }
            let inner = nsString.substring(with: match.range(at: 1))
            guard let link = WikiLink(inner) else { return }
            let shown = shownWikiText(link, inner: inner)
            guard shown.inText == nil else { return }
            replacements.append((match.range, wikiAttachment(shown.text, link: inner, at: match.range.location, in: storage,
                                                             source: nsString.substring(with: match.range),
                                                             maxWidth: maxWidth), false))
        }
        replacements.sort { $0.range.location < $1.range.location }

        // Inline styling below runs on the collapsed text, so map the caret
        // into it: each collapsed attachment before the caret shortens the text
        // by its source length minus one; a revealed one before it adds one.
        var inlineCursor = cursorLocation
        for item in replacements {
            if item.revealed {
                if NSMaxRange(item.range) < cursorLocation { inlineCursor += 1 }
            } else if NSMaxRange(item.range) <= cursorLocation {
                inlineCursor -= item.range.length - 1
            } else if item.range.location < cursorLocation {
                inlineCursor -= cursorLocation - item.range.location
            }
        }

        let revealedParagraph = Self.paragraph(lineSpacing: 10, before: 8, after: 8)
        for item in replacements.reversed() {
            if let link = item.attachment as? WikiLinkAttachment {
                // Inline, keeping its line's paragraph and font: only the link changes.
                var attributes = storage.attributes(at: item.range.location, effectiveRange: nil)
                attributes[.attachment] = link
                attributes[.wikiLink] = link.link
                storage.replaceCharacters(in: item.range, with: NSAttributedString(string: "\u{FFFC}", attributes: attributes))
                continue
            }
            let replacement = NSMutableAttributedString(attachment: item.attachment)
            replacement.addAttributes(style.baseParagraph, range: NSRange(location: 0, length: replacement.length))
            replacement.addAttribute(.paragraphStyle,
                                     value: item.revealed ? revealedParagraph : Self.paragraph(lineSpacing: 4, before: 8, after: 8),
                                     range: NSRange(location: 0, length: replacement.length))
            if item.revealed {
                // Source in the quiet syntax color, then the image.
                storage.addAttribute(.foregroundColor, value: MarkdownPalette.faint, range: item.range)
                storage.insert(replacement, at: NSMaxRange(item.range))
                let line = (storage.string as NSString).lineRange(for: item.range)
                storage.addAttribute(.paragraphStyle, value: revealedParagraph, range: line)
            } else {
                storage.replaceCharacters(in: item.range, with: replacement)
            }
        }

        let styledFullRange = NSRange(location: 0, length: storage.length)
        let styledString = storage.string as NSString
        let styledCodeBlockRanges = findCodeBlockRanges(in: styledString)
        let styledFrontmatter = frontmatterActive ? nil : frontmatter?.range

        // Collect unresolved image ranges so normal link styling does not split ![alt](url).
        var imageRanges: [NSRange] = []
        if let imageRule = inlineRules.first {
            imageRule.pattern.enumerateMatches(in: storage.string, range: styledFullRange) { match, _, _ in
                guard let match = match else { return }
                imageRanges.append(match.range)
                let (markers, contentRanges) = imageRule.analyze(match, styledString)
                for contentRange in contentRanges {
                    storage.addAttributes(imageRule.contentAttributes, range: contentRange)
                }
                for markerRange in markers {
                    storage.addAttribute(.foregroundColor, value: MarkdownPalette.faint, range: markerRange)
                }
            }
        }

        for (ruleIndex, rule) in inlineRules.enumerated() {
            if ruleIndex == 0 { continue }

            rule.pattern.enumerateMatches(in: storage.string, range: styledFullRange) { match, _, _ in
                guard let match = match else { return }

                if styledCodeBlockRanges.contains(where: { NSLocationInRange(match.range.location, $0) }) { return }
                if imageRanges.contains(where: { NSLocationInRange(match.range.location, $0) }) { return }
                if let fm = styledFrontmatter, NSLocationInRange(match.range.location, fm) { return }

                let (markers, contentRanges) = rule.analyze(match, styledString)
                for contentRange in contentRanges {
                    // Bold in a callout keeps the ink around it (body or
                    // title): the accent reads poorly on a tinted box.
                    let ink = isStrong(rule) && contentRange.location < storage.length
                        && storage.attribute(.calloutType, at: contentRange.location, effectiveRange: nil) != nil
                        ? storage.attribute(.foregroundColor, at: contentRange.location, effectiveRange: nil)
                        : nil
                    storage.addAttributes(rule.contentAttributes, range: contentRange)
                    if let ink { storage.addAttribute(.foregroundColor, value: ink, range: contentRange) }
                }

                // Reveal markers on the caret's span; collapse them elsewhere.
                let isActive = NSLocationInRange(inlineCursor, match.range) || inlineCursor == NSMaxRange(match.range)
                for markerRange in markers {
                    if isActive {
                        applyMarkerStyle(storage, range: markerRange)
                    } else {
                        collapse(storage, markerRange)
                    }
                }

                switch rule.kind {
                case .plain:
                    break
                case .code:
                    guard let content = contentRanges.first, content.length > 0 else { break }
                    addPill(storage, content: content, color: MarkdownPalette.inlineBg,
                            leadingSpacer: markers.first.map { NSRange(location: NSMaxRange($0) - 1, length: 1) })
                case .hashtag:
                    let range = match.range
                    storage.addAttribute(.foregroundColor, value: MarkdownPalette.faint,
                                         range: NSRange(location: range.location, length: 1))
                    let before = range.location - 1
                    let spacer = before >= 0 && styledString.character(at: before) == 0x20
                        ? NSRange(location: before, length: 1) : nil
                    addPill(storage, content: range, color: MarkdownPalette.inlineBg, leadingSpacer: spacer)
                }
            }
        }

        styleLinks(storage, cursor: inlineCursor) { location in
            styledCodeBlockRanges.contains { NSLocationInRange(location, $0) }
                || styledFrontmatter.map { NSLocationInRange(location, $0) } ?? false
        }

        // Composite fonts last: every run now carries its final size and
        // weight, so CJK characters can be re-set in the paired face without
        // disturbing bold, headings, callouts, hashtags or inline code. The
        // pass skips collapsed syntax, monospaced runs and attachments.
        style.compositeFont.apply(to: storage)

        applyCJKSpacing(storage, codeBlocks: styledCodeBlockRanges)

        // Math first, then tables: collapse every span except the one being
        // edited into a rendered attachment. Done after inline styling, in
        // reverse order so earlier ranges stay valid as we mutate the storage.
        applyMath(to: storage, activeIndex: activeMathIndex, maxWidth: maxWidth)
        applyTables(to: storage, activeIndex: activeTableIndex, maxWidth: maxWidth)

        // The frontmatter sits at offset 0, so replacing it last leaves every
        // range computed above valid.
        if let frontmatter, !frontmatterActive, NSMaxRange(frontmatter.range) <= storage.length {
            let source = (storage.string as NSString).substring(with: frontmatter.range)
            let key = "\(Int(maxWidth))|\(source)"
            let attachment: FrontmatterAttachment
            if let cached = frontmatterCache, cached.key == key {
                attachment = cached.attachment
            } else {
                attachment = FrontmatterAttachment(frontmatter: frontmatter, markdownSource: source, maxWidth: maxWidth)
                frontmatterCache = (key, attachment)
            }
            let replacement = NSMutableAttributedString(attachment: attachment)
            replacement.addAttribute(.paragraphStyle, value: Self.paragraph(lineSpacing: 0, after: 16),
                                     range: NSRange(location: 0, length: replacement.length))
            storage.replaceCharacters(in: frontmatter.range, with: replacement)
        }

        storage.endEditing()
    }

    // MARK: - Writing a restyle back

    /// Makes `storage` equal to `styled` — text and attributes — touching
    /// only what differs: the changed stretch of text is replaced, and in the
    /// rest only runs whose attributes changed are set. Layout stays valid
    /// wherever nothing changed, so a whole-note restyle costs a whole-note
    /// relayout only when the whole note really looks different.
    static func applyChanges(from styled: NSAttributedString, to storage: NSTextStorage) {
        let old = storage.string as NSString
        let new = styled.string as NSString
        let oldLength = old.length, newLength = new.length
        var oldChars = [unichar](repeating: 0, count: oldLength)
        var newChars = [unichar](repeating: 0, count: newLength)
        old.getCharacters(&oldChars, range: NSRange(location: 0, length: oldLength))
        new.getCharacters(&newChars, range: NSRange(location: 0, length: newLength))

        let shorter = min(oldLength, newLength)
        var prefix = 0
        while prefix < shorter, oldChars[prefix] == newChars[prefix] { prefix += 1 }
        var suffix = 0
        while suffix < shorter - prefix, oldChars[oldLength - 1 - suffix] == newChars[newLength - 1 - suffix] { suffix += 1 }

        storage.beginEditing()
        let oldMiddle = NSRange(location: prefix, length: oldLength - prefix - suffix)
        let newMiddle = NSRange(location: prefix, length: newLength - prefix - suffix)
        if oldMiddle.length > 0 || newMiddle.length > 0 {
            storage.replaceCharacters(in: oldMiddle, with: styled.attributedSubstring(from: newMiddle))
        }
        // The text now matches; bring the attributes of the rest in line.
        for range in [NSRange(location: 0, length: prefix), NSRange(location: newLength - suffix, length: suffix)]
        where range.length > 0 {
            styled.enumerateAttributes(in: range) { wanted, wantedRange, _ in
                let wantedDictionary = wanted as NSDictionary
                storage.enumerateAttributes(in: wantedRange) { current, currentRange, _ in
                    if !wantedDictionary.isEqual(to: current) {
                        storage.setAttributes(wanted, range: currentRange)
                    }
                }
            }
        }
        storage.endEditing()
    }

    // MARK: - One line on its own

    /// Whether the line at `lineRange` (with its break) in `storage` — styled
    /// text, as the editor shows it — can be restyled by itself: its look
    /// depends on nothing around it, so typing in it needn't restyle the
    /// whole note. Lines that are, could turn into, or sit next to a block
    /// (fences, callouts, quotes, tables, math, rules, the frontmatter), lines
    /// with images and a possible title take the whole-note pass instead.
    static func canStyleAlone(lineRange: NSRange, in storage: NSAttributedString) -> Bool {
        let ns = storage.string as NSString
        guard NSMaxRange(lineRange) <= ns.length else { return false }
        let line = ns.substring(with: lineRange).trimmingCharacters(in: .newlines)
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        for prefix in ["```", "~~~", ":::", ">", "|", "$$", "#", "<", "===", "[^"] where trimmed.hasPrefix(prefix) {
            // Headings are fine, except a possible title (`# `).
            if prefix == "#", trimmed.hasPrefix("##") { continue }
            return false
        }
        if isHorizontalRuleText(trimmed) { return false }
        // `[[` too: a link to a note may become an attachment, which a
        // line restyled on its own can't take.
        for marker in ["![", "[[", "$", "|", "\u{FFFC}"] where line.contains(marker) { return false }
        // Inside the frontmatter (shown as source while the caret is in it).
        if ns.hasPrefix("---"), let frontmatter = Frontmatter.parse(ns),
           lineRange.location < NSMaxRange(frontmatter.range) { return false }

        // Not inside a block, nor right before or after one (block spacing
        // reaches into the neighbors' paragraph styles).
        var around = lineRange
        if around.location > 0 {
            around = NSUnionRange(around, ns.lineRange(for: NSRange(location: around.location - 1, length: 0)))
        }
        if NSMaxRange(around) < ns.length {
            around = NSUnionRange(around, ns.lineRange(for: NSRange(location: NSMaxRange(around), length: 0)))
        }
        var inBlock = false
        for key: NSAttributedString.Key in [.codeBlockBackground, .calloutType, .blockquoteDepth, .horizontalRule] {
            storage.enumerateAttribute(key, in: around) { value, _, stop in
                if value != nil { inBlock = true; stop.pointee = true }
            }
            if inBlock { return false }
        }
        return true
    }

    private static func isHorizontalRuleText(_ trimmed: String) -> Bool {
        let compact = trimmed.replacingOccurrences(of: " ", with: "")
        guard compact.count >= 3, let first = compact.first, "-*_".contains(first) else { return false }
        return compact.allSatisfy { $0 == first }
    }

    /// `line` (with its break) styled exactly as the whole-note pass styles
    /// it in place — for a line that `canStyleAlone`. `cursorInLine` is the
    /// caret's offset in the line, or -1 when it is elsewhere.
    func styledLine(_ line: String, cursorInLine: Int, maxWidth: CGFloat) -> NSAttributedString {
        let storage = NSTextStorage(string: line)
        applyStyle(to: storage, cursorLocation: cursorInLine, maxWidth: maxWidth)
        return storage
    }

    private func imageAttachment(content: LoadedMarkdownImage, source: String, url: String, caption: String,
                                 maxWidth: CGFloat, revealed: Bool) -> ImageTextAttachment {
        let base = "\(Int(maxWidth))|\(revealed)|\(url)|\(caption)|\(source)"
        let occurrence = imageOccurrences[base, default: 0]
        imageOccurrences[base] = occurrence + 1
        let key = "\(base)#\(occurrence)"
        if let cached = imageCache[key] { return cached }
        if imageCache.count > 600 { imageCache.removeAll() }
        let attachment = ImageTextAttachment(content: content, markdownSource: source, caption: caption,
                                             maxWidth: maxWidth, fillsLine: revealed)
        imageCache[key] = attachment
        return attachment
    }

    private func videoAttachment(url: URL, source: String, maxWidth: CGFloat) -> VideoAttachment {
        let key = "\(Int(maxWidth))|\(url.path)|\(source)"
        let hasPoster = VideoPosterCache.shared.info(for: url) != nil
        if let cached = videoCache[key], cached.hasPoster || !hasPoster { return cached.attachment }
        let attachment = VideoAttachment(fileURL: url, markdownSource: source, maxWidth: maxWidth)
        videoCache[key] = (attachment, hasPoster)
        return attachment
    }

    // MARK: - Block helpers

    /// Callout bodies are body text: the body's line height.
    private func calloutParagraph() -> NSParagraphStyle {
        Self.paragraph(indent: Self.calloutTextIndent, tail: -14, lineHeight: style.lineHeight)
    }

    /// Consecutive `>` lines grouped into blocks. A block whose first line
    /// is `[!type]` (Obsidian / GitHub) is a callout of that type; one whose
    /// first line opens with an emoji is drawn as a gray callout.
    struct QuoteGroup: Equatable {
        let start: Int
        let end: Int
        let isEmoji: Bool
        var calloutType: String? = nil
    }

    static func quoteGroups(_ lines: [String]) -> [Int: QuoteGroup] {
        var result: [Int: QuoteGroup] = [:]
        var i = 0
        while i < lines.count {
            guard lines[i].trimmingCharacters(in: .whitespaces).hasPrefix(">") else { i += 1; continue }
            var j = i
            while j + 1 < lines.count, lines[j + 1].trimmingCharacters(in: .whitespaces).hasPrefix(">") { j += 1 }
            let first = quoteContent(of: lines[i])
            let group = QuoteGroup(start: i, end: j, isEmoji: startsWithEmoji(first),
                                   calloutType: calloutType(ofQuoteContent: first))
            for k in i...j { result[k] = group }
            i = j + 1
        }
        return result
    }

    /// The type of a quote line that opens a callout: `note` for `[!note]`,
    /// `[!note]-` or `[!note] Title`; nil for any other quote.
    static func calloutType(ofQuoteContent content: String) -> String? {
        let ns = content as NSString
        guard let match = calloutTypePattern.firstMatch(in: content, range: NSRange(location: 0, length: ns.length))
        else { return nil }
        return ns.substring(with: match.range(at: 1))
    }

    private static let calloutTypePattern = try! NSRegularExpression(pattern: #"^\[!([^\]\s]+)\][+-]?(?:\s|$)"#)

    /// A callout's opening line up to its title: the `>` markers, `[!type]`,
    /// a fold sign and the spaces after. Group 1 is the type.
    private static let calloutHeader = try! NSRegularExpression(pattern: #"^\s*(?:>\s*)+\[!([^\]\s]+)\][+-]?[ \t]*"#)

    /// A quote line's text after its `>` markers.
    static func quoteContent(of line: String) -> String {
        String(line.drop(while: { $0 == " " || $0 == "\t" || $0 == ">" }))
    }

    /// True for a pictographic first character (⚠️, ✅, 💡 …), not for digits or
    /// `#`/`*`, which are technically emoji-capable but read as text.
    static func startsWithEmoji(_ text: String) -> Bool {
        guard let first = text.first, let scalar = first.unicodeScalars.first else { return false }
        if scalar.properties.isEmojiPresentation { return true }
        return scalar.properties.isEmoji && first.unicodeScalars.count > 1 && scalar.value > 0x7F
    }

    private static let quotePrefix = try! NSRegularExpression(pattern: #"^\s*((?:>\s?)+)"#)

    /// A quote line, or with `calloutIndent` a callout's body line: in the
    /// callout's body font, its text at that indent, no quote bar.
    private func styleQuoteLine(_ storage: NSTextStorage, line: String, lineRange: NSRange,
                                enclosing: NSRange, calloutIndent: CGFloat?, isActive: Bool) {
        let nsLine = line as NSString
        guard let match = Self.quotePrefix.firstMatch(in: line, range: NSRange(location: 0, length: nsLine.length)) else { return }
        let prefix = NSRange(location: lineRange.location, length: match.range.length)
        let depth = nsLine.substring(with: match.range(at: 1)).filter { $0 == ">" }.count
        let content = nsLine.substring(from: match.range.length)

        let asCallout = calloutIndent != nil
        let indent = calloutIndent ?? Self.quoteIndent * CGFloat(depth)
        let paragraph = asCallout
            ? Self.paragraph(indent: indent, tail: -14, lineHeight: style.lineHeight)
            : Self.paragraph(indent: indent, lineHeight: style.lineHeight)
        if isActive {
            // Live preview: the raw `> ` shows on the caret's line, hung just
            // before the text so the text itself doesn't move.
            paragraph.firstLineHeadIndent = max(0, indent - prefixWidth(nsLine.substring(with: match.range)))
        }

        if asCallout {
            storage.addAttributes([.paragraphStyle: paragraph, .font: style.calloutBodyFont], range: lineRange)
        } else {
            storage.addAttribute(.paragraphStyle, value: paragraph, range: lineRange)
            storage.addAttribute(.blockquoteDepth, value: depth, range: enclosing)
            if content.hasPrefix("— ") || content.hasPrefix("——") || content.hasPrefix("-- ") {
                storage.addAttributes([.font: style.quoteAttributionFont, .foregroundColor: MarkdownPalette.tertiary],
                                      range: lineRange)
            }
        }
        if isActive {
            storage.addAttributes([.font: style.bodyFont, .foregroundColor: MarkdownPalette.marker], range: prefix)
        } else {
            collapse(storage, prefix)
        }
    }

    /// Rendered width of raw syntax shown on the caret's line.
    private func prefixWidth(_ prefix: String) -> CGFloat {
        ceil((prefix as NSString).size(withAttributes: [.font: style.bodyFont]).width)
    }

    private static let listPrefix = try! NSRegularExpression(pattern: #"^([ \t]*)([-*+]|\d+[.)])[ \t]+(\[[ xX]\][ \t]*)?"#)

    /// Markdown ignores up to three spaces before a paragraph; so does the
    /// page, on the caret's line too, so the text never shifts.
    private func hideLeadingSpaces(_ storage: NSTextStorage, line: String, lineRange: NSRange) {
        let spaces = line.prefix(while: { $0 == " " }).count
        guard (1...3).contains(spaces) else { return }
        collapse(storage, NSRange(location: lineRange.location, length: spaces))
    }

    /// Styles the line as a list item; false when it isn't one.
    @discardableResult
    private func styleListItem(_ storage: NSTextStorage, line: String, lineRange: NSRange, isActive: Bool) -> Bool {
        let nsLine = line as NSString
        guard let match = Self.listPrefix.firstMatch(in: line, range: NSRange(location: 0, length: nsLine.length)) else { return false }

        let leading = nsLine.substring(with: match.range(at: 1))
        let columns = leading.reduce(0) { $0 + ($1 == "\t" ? 4 : 1) }
        let level = min(columns / 2, 6)
        let indent = Self.listIndentStep * CGFloat(level + 1)
        let token = nsLine.substring(with: match.range(at: 2))
        let checkboxRange = match.range(at: 3)

        let kind: ListMarker.Kind
        if checkboxRange.location != NSNotFound {
            let box = nsLine.substring(with: checkboxRange)
            kind = .task(checked: box.contains("x") || box.contains("X"))
        } else if let number = Int(token.dropLast()) {
            kind = .ordered(label: level % 2 == 1 ? Self.letterLabel(number) + "." : token)
        } else {
            kind = .bullet(level: level)
        }

        // Items keep the body's line height (an empty item, all collapsed
        // syntax, stays one line tall too), a little air between them.
        let paragraph = Self.paragraph(indent: indent, lineHeight: style.lineHeight, after: 4)
        let prefix = NSRange(location: lineRange.location, length: match.range.length)
        if isActive {
            // Live preview: on the caret's line the raw `- ` / `1. ` / `- [ ] `
            // replaces the drawn marker, hung in the marker slot so the item
            // text stays put.
            let raw = nsLine.substring(with: match.range).trimmingCharacters(in: .whitespaces) + " "
            paragraph.firstLineHeadIndent = max(0, indent - prefixWidth(raw))
            storage.addAttributes([.font: style.bodyFont, .foregroundColor: MarkdownPalette.marker], range: prefix)
            let leading = match.range(at: 1)
            if leading.length > 0 {
                collapse(storage, NSRange(location: lineRange.location, length: leading.length))
            }
        } else {
            collapse(storage, prefix)
            storage.addAttribute(.listMarker,
                                 value: ListMarker(kind: kind, indent: indent, font: style.bodyFont),
                                 range: NSRange(location: lineRange.location, length: 1))
        }
        storage.addAttribute(.paragraphStyle, value: paragraph, range: lineRange)

        if case .task(true) = kind, NSMaxRange(prefix) < NSMaxRange(lineRange) {
            let text = NSRange(location: NSMaxRange(prefix), length: NSMaxRange(lineRange) - NSMaxRange(prefix))
            storage.addAttributes(style.strikethrough, range: text)
        }
        return true
    }

    /// 1 → "a", 26 → "z", 27 → "aa".
    static func letterLabel(_ number: Int) -> String {
        var n = max(1, number)
        var result = ""
        while n > 0 {
            n -= 1
            result = String(UnicodeScalar(UInt8(97 + n % 26))) + result
            n /= 26
        }
        return result
    }

    /// Rounded fill behind `content`, with room made on both sides: kerning on
    /// the character before the span (`leadingSpacer`) and on its last
    /// character, so the pill doesn't touch neighboring text.
    private func addPill(_ storage: NSTextStorage, content: NSRange, color: NSColor, leadingSpacer: NSRange?) {
        storage.addAttribute(.inlinePill, value: color, range: content)
        storage.addAttribute(.kern, value: Self.pillPadding, range: NSRange(location: NSMaxRange(content) - 1, length: 1))
        if let spacer = leadingSpacer, spacer.location >= 0 {
            let existing = (storage.attribute(.kern, at: spacer.location, effectiveRange: nil) as? CGFloat) ?? 0
            storage.addAttribute(.kern, value: existing + Self.pillPadding, range: spacer)
        }
    }

    // MARK: - CJK / Latin spacing (board ⑲ ②③)

    /// Display-only spacing for Chinese text: a gap of about ¼ em where Han
    /// characters meet Latin letters or digits ("用 WorkBuddy 做"), and a hair
    /// of tracking on Han characters. Done with kerning, so the file never
    /// gets spaces inserted. Collapsed syntax and code are skipped over.
    private func applyCJKSpacing(_ storage: NSTextStorage, codeBlocks: [NSRange]) {
        let string = storage.string as NSString
        let length = string.length
        guard length > 1 else { return }
        let boundaryGap = (style.fontSize * 0.22).rounded()

        enum Kind { case han, latin, other }
        func kind(_ c: unichar) -> Kind {
            switch c {
            case 0x4E00...0x9FFF, 0x3400...0x4DBF, 0xF900...0xFAFF: return .han
            case 0x30...0x39, 0x41...0x5A, 0x61...0x7A: return .latin
            default: return .other
            }
        }

        // One pass over font runs (not a lookup per character): collapsed
        // syntax and monospaced runs are skipped.
        var hidden: [NSRange] = []
        storage.enumerateAttribute(.font, in: NSRange(location: 0, length: length)) { value, range, _ in
            if let font = value as? NSFont, font.pointSize < 1 || font.isFixedPitch { hidden.append(range) }
        }
        var chars = [unichar](repeating: 0, count: length)
        string.getCharacters(&chars, range: NSRange(location: 0, length: length))

        var gaps: [Int] = []
        var codeIndex = 0
        var hiddenIndex = 0
        var previous: (index: Int, kind: Kind)? = nil
        for i in 0..<length {
            while codeIndex < codeBlocks.count, NSMaxRange(codeBlocks[codeIndex]) <= i { codeIndex += 1 }
            if codeIndex < codeBlocks.count, NSLocationInRange(i, codeBlocks[codeIndex]) {
                previous = nil
                continue
            }
            let c = chars[i]
            if c == 0x0A || c == 0x2028 || c == 0x2029 { previous = nil; continue }
            while hiddenIndex < hidden.count, NSMaxRange(hidden[hiddenIndex]) <= i { hiddenIndex += 1 }
            if hiddenIndex < hidden.count, NSLocationInRange(i, hidden[hiddenIndex]) { continue }
            let k = kind(c)
            if let prev = previous,
               (prev.kind == .han && k == .latin) || (prev.kind == .latin && k == .han) {
                gaps.append(prev.index)
            }
            previous = (i, k)
        }
        for i in gaps {
            let existing = (storage.attribute(.kern, at: i, effectiveRange: nil) as? CGFloat) ?? 0
            storage.addAttribute(.kern, value: existing + boundaryGap, range: NSRange(location: i, length: 1))
        }
    }

    // MARK: - Code syntax colors

    private static let keywordPattern = try! NSRegularExpression(pattern: #"\b(func|let|var|if|else|for|while|return|guard|import|class|struct|enum|protocol|extension|def|function|const|true|false|nil|null|None|True|False|self|this|in|case|switch|break|continue|try|catch|throw|throws|async|await|public|private|static|new|from|as|interface|type|extends|implements|package|go|fn|pub|mut|impl|use|where|yield|lambda|with|and|or|not|elif|end|do|then|fi|echo|export|select|insert|update|delete|create|table|into|values)\b"#)
    private static let numberPattern = try! NSRegularExpression(pattern: #"\b\d+(?:\.\d+)?\b"#)
    private static let stringPattern = try! NSRegularExpression(pattern: #""(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'"#)
    private static let slashComment = try! NSRegularExpression(pattern: #"//[^\n]*|/\*[\s\S]*?\*/"#)
    private static let hashComment = try! NSRegularExpression(pattern: #"(?m)(?:^|\s)#[^\n]*"#)
    private static let hashCommentLanguages: Set<String> = ["python", "py", "sh", "bash", "zsh", "shell", "yaml", "yml", "ruby", "rb", "toml", "r", "perl", "makefile", "dockerfile"]

    /// Four quiet colors only: keywords, strings, numbers, comments.
    private func highlightSyntax(_ storage: NSTextStorage, in range: NSRange, language: String) {
        let string = storage.string
        func paint(_ regex: NSRegularExpression, _ color: NSColor) {
            regex.enumerateMatches(in: string, range: range) { match, _, _ in
                guard let match else { return }
                storage.addAttribute(.foregroundColor, value: color, range: match.range)
            }
        }
        paint(Self.keywordPattern, MarkdownPalette.syntaxKeyword)
        paint(Self.numberPattern, MarkdownPalette.syntaxNumber)
        paint(Self.stringPattern, MarkdownPalette.syntaxString)
        if Self.hashCommentLanguages.contains(language.lowercased()) {
            paint(Self.hashComment, MarkdownPalette.faint)
        } else {
            paint(Self.slashComment, MarkdownPalette.faint)
        }
    }

    // MARK: - Math

    private func applyMath(to storage: NSTextStorage, activeIndex: Int, maxWidth: CGFloat) {
        let nsString = storage.string as NSString
        let tableRanges = MarkdownTableParser.tables(in: nsString).map { $0.range }
        let spans = LatexMath.mathSpans(in: nsString, excludingTableRanges: tableRanges)
        guard !spans.isEmpty else { return }

        let bodyFont = (style.baseParagraph[.font] as? NSFont) ?? NSFont.systemFont(ofSize: 17)
        let color = MarkdownPalette.primary
        let mono = style.rawSourceFont

        for (index, span) in spans.enumerated().reversed() {
            guard NSMaxRange(span.range) <= storage.length else { continue }
            if index == activeIndex {
                storage.addAttribute(.font, value: mono, range: span.range)
                continue
            }
            let source = nsString.substring(with: span.range)
            let attachment = mathAttachment(latex: span.latex, display: span.display, source: source,
                                            font: bodyFont, color: color, maxWidth: maxWidth)
            let replacement = NSMutableAttributedString(attachment: attachment)
            if span.display {
                let paragraph = NSMutableParagraphStyle()
                paragraph.alignment = .center
                paragraph.paragraphSpacingBefore = 10
                paragraph.paragraphSpacing = 10
                replacement.addAttribute(.paragraphStyle, value: paragraph,
                                         range: NSRange(location: 0, length: replacement.length))
            }
            storage.replaceCharacters(in: span.range, with: replacement)
        }
    }

    private func mathAttachment(latex: String, display: Bool, source: String, font: NSFont, color: NSColor, maxWidth: CGFloat) -> MathAttachment {
        let key = "\(display ? "D" : "I")|\(Int(maxWidth))|\(source)"
        if let cached = mathCache[key] { return cached }
        let attachment = MathAttachment(latex: latex, display: display, markdownSource: source,
                                        font: font, color: color, maxWidth: maxWidth)
        mathCache[key] = attachment
        return attachment
    }

    // MARK: - Tables

    private func applyTables(to storage: NSTextStorage, activeIndex: Int, maxWidth: CGFloat) {
        let nsString = storage.string as NSString
        let tables = MarkdownTableParser.tables(in: nsString)
        guard !tables.isEmpty else { return }

        let bodyFont = (style.baseParagraph[.font] as? NSFont) ?? NSFont.systemFont(ofSize: 16)
        let cellFont = NSFont.systemFont(ofSize: Self.smallSize(bodyFont.pointSize))
        let mono = style.rawSourceFont

        for (index, item) in tables.enumerated().reversed() {
            guard NSMaxRange(item.range) <= storage.length else { continue }
            if index == activeIndex {
                // Being edited: show the raw source, monospaced so the pipes line
                // up while typing.
                storage.addAttribute(.font, value: mono, range: item.range)
                continue
            }
            let source = nsString.substring(with: item.range)
            let attachment = tableAttachment(for: item.table, source: source, font: cellFont, maxWidth: maxWidth)
            let paragraph = NSMutableParagraphStyle()
            paragraph.paragraphSpacingBefore = 8
            paragraph.paragraphSpacing = 10
            let replacement = NSMutableAttributedString(attachment: attachment)
            replacement.addAttribute(.paragraphStyle, value: paragraph,
                                     range: NSRange(location: 0, length: replacement.length))
            storage.replaceCharacters(in: item.range, with: replacement)
        }
    }

    private func tableAttachment(for table: MarkdownTable, source: String, font: NSFont, maxWidth: CGFloat) -> TableAttachment {
        let key = "\(Int(maxWidth))|\(source)"
        if let cached = tableCache[key] { return cached }
        let strong = style.bold[.foregroundColor] as? NSColor ?? MarkdownPalette.strong
        let attachment = TableAttachment(table: table, markdownSource: source, font: font, maxWidth: maxWidth,
                                         strongColor: strong)
        tableCache[key] = attachment
        return attachment
    }

    // MARK: - Parsing helpers

    private struct HeadingInfo {
        let level: Int
        let markerRange: NSRange  // "## " (with up to 3 leading spaces), relative to the line
    }

    private static let headingPattern = try! NSRegularExpression(pattern: #"^ {0,3}(#{1,6})(?:[ \t]+|$)"#)

    private func parseHeading(line: String) -> HeadingInfo? {
        let nsLine = line as NSString
        guard let match = Self.headingPattern.firstMatch(in: line, range: NSRange(location: 0, length: nsLine.length)) else { return nil }
        return HeadingInfo(level: match.range(at: 1).length, markerRange: match.range)
    }

    private func isHorizontalRule(_ line: String) -> Bool {
        let trimmed = line.replacingOccurrences(of: " ", with: "")
        return (trimmed == "---" || trimmed == "***" || trimmed == "___")
    }

    private func isRemoteImageURL(_ value: String) -> Bool {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return trimmed.hasPrefix("http://") || trimmed.hasPrefix("https://") ||
               trimmed.hasPrefix("<http://") || trimmed.hasPrefix("<https://")
    }

    private func findCodeBlockRanges(in string: NSString) -> [NSRange] {
        scanCodeBlocks(in: string).blocks
    }

    /// A line that looks like a code fence: an optionally indented run of three
    /// or more backticks or tildes.
    private struct ScannedFence {
        let lineRange: NSRange
        let marker: Character   // "`" or "~"
        let length: Int         // number of fence characters
        let hasInfo: Bool       // a trailing info string (e.g. ```swift)
    }

    private func scanFences(in string: NSString) -> [ScannedFence] {
        var fences: [ScannedFence] = []
        let fullRange = NSRange(location: 0, length: string.length)
        string.enumerateSubstrings(in: fullRange, options: [.byLines]) { substring, range, _, _ in
            guard let line = substring else { return }
            var body = Substring(line)
            while let f = body.first, f == " " || f == "\t" { body = body.dropFirst() }
            guard let marker = body.first, marker == "`" || marker == "~" else { return }
            var length = 0
            for ch in body { if ch == marker { length += 1 } else { break } }
            guard length >= 3 else { return }
            let info = String(body.dropFirst(length)).trimmingCharacters(in: .whitespaces)
            fences.append(ScannedFence(lineRange: range, marker: marker, length: length, hasInfo: !info.isEmpty))
        }
        return fences
    }

    /// Pairs fences into closed code blocks, CommonMark-style:
    ///  - the closing fence uses the same character, is at least as long as the
    ///    opener, and carries no info string;
    ///  - an opening fence with no matching closer is left unpaired, so a stray
    ///    or half-typed fence cannot turn the rest of the document into code.
    /// Returns each block's character range plus the line-start offsets of the
    /// fences that bound a closed block.
    private func scanCodeBlocks(in string: NSString) -> (blocks: [NSRange], fenceLocations: Set<Int>) {
        let fences = scanFences(in: string)
        var blocks: [NSRange] = []
        var fenceLocations = Set<Int>()
        var i = 0
        while i < fences.count {
            let open = fences[i]
            var closeIndex: Int? = nil
            var j = i + 1
            while j < fences.count {
                let f = fences[j]
                if f.marker == open.marker, f.length >= open.length, !f.hasInfo { closeIndex = j; break }
                j += 1
            }
            guard let close = closeIndex else { i += 1; continue }
            let openRange = open.lineRange
            let closeRange = fences[close].lineRange
            blocks.append(NSRange(location: openRange.location,
                                  length: (closeRange.location + closeRange.length) - openRange.location))
            fenceLocations.insert(openRange.location)
            fenceLocations.insert(closeRange.location)
            i = close + 1
        }
        return (blocks, fenceLocations)
    }


    // MARK: - Marker hiding

    /// Shrinks syntax characters to (almost) zero width and makes them clear,
    /// so the rendered text sits exactly where it would without them.
    // MARK: - Links

    /// What a `[[link]]` shows, whether its note exists, and where that text
    /// already sits in the link's inner source (`target#heading|alias`) —
    /// then it is styled in place, else drawn as an attachment. A missing
    /// note's link always shows its text as written.
    private func shownWikiText(_ link: WikiLink, inner: String) -> (text: String, inText: NSRange?, exists: Bool) {
        let entry = WikiLinkIndex.shared.resolve(link.target, from: ImageLoader.shared.baseURL)
        let ns = inner as NSString
        // The alias, or the target's last path component.
        let bar = ns.range(of: "|")
        let slash = ns.range(of: "/", options: .backwards)
        var start = bar.location != NSNotFound ? NSMaxRange(bar) : (slash.location != NSNotFound ? NSMaxRange(slash) : 0)
        var end = ns.length
        while start < end, ns.character(at: start) == 0x20 { start += 1 }
        while end > start, ns.character(at: end - 1) == 0x20 { end -= 1 }
        let visible = NSRange(location: start, length: end - start)
        guard let entry else { return (ns.substring(with: visible), visible, false) }
        let text = link.displayText(title: entry.title)
        return (text, ns.substring(with: visible) == text ? visible : nil, true)
    }

    /// Repeats of the same link get attachments of their own.
    private var wikiOccurrences: [String: Int] = [:]
    private var wikiCache: [String: WikiLinkAttachment] = [:]

    private func wikiAttachment(_ text: String, link: String, at location: Int, in storage: NSTextStorage,
                                source: String, maxWidth: CGFloat) -> WikiLinkAttachment {
        let font = storage.attribute(.font, at: location, effectiveRange: nil) as? NSFont ?? style.bodyFont
        let color = storage.attribute(.foregroundColor, at: location, effectiveRange: nil) as? NSColor ?? MarkdownPalette.primary
        let base = "\(Int(maxWidth))|\(font.fontName)|\(font.pointSize)|\(ObjectIdentifier(color).hashValue)|\(text)|\(source)"
        let occurrence = wikiOccurrences[base, default: 0]
        wikiOccurrences[base] = occurrence + 1
        let key = "\(base)#\(occurrence)"
        if let cached = wikiCache[key] { return cached }
        if wikiCache.count > 400 { wikiCache.removeAll() }
        let drawn = NSTextStorage(string: text, attributes: [.font: font, .foregroundColor: color])
        style.compositeFont.apply(to: drawn)
        let attachment = WikiLinkAttachment(text: drawn, link: link, font: font, markdownSource: source, maxWidth: maxWidth)
        wikiCache[key] = attachment
        return attachment
    }

    /// Links in the text: `[[links]]` not drawn as attachments (the note's
    /// title is the text written), `[text](url)` and bare addresses, each
    /// marked with what a click opens. `skip` leaves code and the front
    /// matter alone.
    private func styleLinks(_ storage: NSTextStorage, cursor: Int, skip: (Int) -> Bool) {
        let string = storage.string as NSString
        let full = NSRange(location: 0, length: string.length)
        func isCode(_ location: Int) -> Bool {
            (storage.attribute(.font, at: location, effectiveRange: nil) as? NSFont)?.isFixedPitch ?? false
        }

        WikiLink.pattern.enumerateMatches(in: storage.string, range: full) { match, _, _ in
            guard let match, !skip(match.range.location), !isCode(match.range.location) else { return }
            let innerRange = match.range(at: 1)
            let inner = string.substring(with: innerRange)
            guard let link = WikiLink(inner) else { return }
            // `[[#Heading]]` and `[[a#b]]` hold no tags.
            storage.removeAttribute(.inlinePill, range: match.range)
            let open = NSRange(location: match.range.location, length: 2)
            let close = NSRange(location: NSMaxRange(match.range) - 2, length: 2)
            if NSLocationInRange(cursor, match.range) || cursor == NSMaxRange(match.range) {
                applyMarkerStyle(storage, range: open)
                applyMarkerStyle(storage, range: close)
                return
            }
            let shown = shownWikiText(link, inner: inner)
            guard let inText = shown.inText, inText.length > 0 else { return }
            let visible = NSRange(location: innerRange.location + inText.location, length: inText.length)
            collapse(storage, NSRange(location: match.range.location, length: visible.location - match.range.location))
            collapse(storage, NSRange(location: NSMaxRange(visible), length: NSMaxRange(match.range) - NSMaxRange(visible)))
            storage.addAttributes(shown.exists ? style.link : missingLink, range: visible)
            storage.addAttribute(.wikiLink, value: inner, range: visible)
        }

        var linked: [NSRange] = []
        InlineLink.linkPattern.enumerateMatches(in: storage.string, range: full) { match, _, _ in
            guard let match, !skip(match.range.location) else { return }
            linked.append(match.range)
            let text = match.range(at: 1)
            let address = string.substring(with: InlineLink.destinationRange(of: match))
            guard text.length > 0, Self.isWebAddress(address) else { return }
            storage.addAttribute(.webLink, value: address, range: text)
        }
        InlineLink.imagePattern.enumerateMatches(in: storage.string, range: full) { match, _, _ in
            if let match { linked.append(match.range) }
        }
        Self.bareAddress.enumerateMatches(in: storage.string, range: full) { match, _, _ in
            guard let match, !skip(match.range.location), !isCode(match.range.location),
                  !linked.contains(where: { NSIntersectionRange($0, match.range).length > 0 }) else { return }
            var range = match.range
            // Sentence punctuation after an address isn't part of it.
            while range.length > 0, ".,;:!?)".utf16.contains(string.character(at: NSMaxRange(range) - 1)) {
                range.length -= 1
            }
            storage.addAttributes(style.link, range: range)
            storage.addAttribute(.webLink, value: string.substring(with: range), range: range)
        }
    }

    /// A link to a note that doesn't exist: gray, dashed.
    private var missingLink: [NSAttributedString.Key: Any] {
        [.foregroundColor: MarkdownPalette.tertiary,
         .underlineStyle: NSUnderlineStyle.single.rawValue | NSUnderlineStyle.patternDash.rawValue,
         .underlineColor: MarkdownPalette.linkUnderline]
    }

    private static let bareAddress = try! NSRegularExpression(
        pattern: #"(?<![\w/(<])https?://[^\s<>\[\]()（）"'“”，。、；：！？]+"#)

    static func isWebAddress(_ address: String) -> Bool {
        let lowered = address.lowercased()
        return lowered.hasPrefix("http://") || lowered.hasPrefix("https://") || lowered.hasPrefix("mailto:")
    }

    /// Whether `rule` sets bold text in the bold color (`**…**`, `***…***`).
    private func isStrong(_ rule: WysiwygInlineRule) -> Bool {
        guard let color = rule.contentAttributes[.foregroundColor] as? NSColor else { return false }
        return color === style.bold[.foregroundColor] as? NSColor
    }

    private func collapse(_ storage: NSTextStorage, _ range: NSRange) {
        guard range.length > 0, NSMaxRange(range) <= storage.length else { return }
        storage.addAttributes([
            .foregroundColor: NSColor.clear,
            .font: NSFont.systemFont(ofSize: 0.1),
        ], range: range)
    }

    /// Reveals syntax on the caret's paragraph, in the quiet marker color.
    private func applyMarkerStyle(_ storage: NSTextStorage, range: NSRange) {
        storage.addAttribute(.foregroundColor, value: MarkdownPalette.marker, range: range)
    }

    // MARK: - Callouts

    /// Styles a callout's opening line (`::: warning 标题`): the title in the
    /// type color, with the type's icon drawn before it. `::: type` is hidden
    /// when a title follows; with no title the type name itself is the title.
    private func styleCalloutOpeningLine(_ storage: NSTextStorage, line: String, lineRange: NSRange, type: String, isActive: Bool) {
        let kind = CalloutKind(type)
        storage.addAttributes([
            .paragraphStyle: calloutParagraph(),
            .foregroundColor: kind.color,
            .font: style.calloutTitleFont,
        ], range: lineRange)
        storage.addAttribute(.calloutIcon, value: type, range: NSRange(location: lineRange.location, length: min(1, lineRange.length)))

        let nsLine = line as NSString
        let full = NSRange(location: 0, length: nsLine.length)
        let withTitle = try! NSRegularExpression(pattern: #"^\s*:::\s*\S+\s+(?=\S)"#)
        let bare = try! NSRegularExpression(pattern: #"^\s*:::\s*"#)
        guard let match = withTitle.firstMatch(in: line, range: full) ?? bare.firstMatch(in: line, range: full) else { return }
        let marker = NSRange(location: lineRange.location + match.range.location, length: match.range.length)
        if isActive {
            storage.addAttribute(.foregroundColor, value: kind.color.withAlphaComponent(0.4), range: marker)
        } else {
            collapse(storage, marker)
        }
    }

    /// Styles a `> [!type] Title` callout's first line: the title in the
    /// type color, the type's icon before it. `> [!type]` is hidden; with no
    /// title the type word stays (invisibly) and its name is drawn over it,
    /// capitalized. On the caret's line the syntax shows, faint.
    private func styleCalloutTitleLine(_ storage: NSTextStorage, line: String, lineRange: NSRange, type: String,
                                       isActive: Bool) {
        let kind = CalloutKind(type)
        storage.addAttributes([
            .paragraphStyle: calloutParagraph(),
            .foregroundColor: kind.color,
            .font: style.calloutTitleFont,
        ], range: lineRange)
        storage.addAttribute(.calloutIcon, value: type, range: NSRange(location: lineRange.location, length: min(1, lineRange.length)))

        let nsLine = line as NSString
        guard let match = Self.calloutHeader.firstMatch(in: line, range: NSRange(location: 0, length: nsLine.length)) else { return }
        let header = NSRange(location: lineRange.location + match.range.location, length: match.range.length)
        if isActive {
            storage.addAttribute(.foregroundColor, value: kind.color.withAlphaComponent(0.4), range: header)
            return
        }
        let title = nsLine.substring(from: NSMaxRange(match.range)).trimmingCharacters(in: .whitespaces)
        if !title.isEmpty {
            collapse(storage, header)
            return
        }
        let word = NSRange(location: lineRange.location + match.range(at: 1).location, length: match.range(at: 1).length)
        collapse(storage, NSRange(location: header.location, length: word.location - header.location))
        collapse(storage, NSRange(location: NSMaxRange(word), length: NSMaxRange(lineRange) - NSMaxRange(word)))
        storage.addAttributes([.foregroundColor: NSColor.clear, .calloutTitle: CalloutKind.title(for: type)], range: word)
    }

    /// Adds breathing room above and below a block (code block / callout) by
    /// bumping paragraph spacing on the adjacent lines; the box's own padding
    /// extends into that space.
    private func addBlockSpacing(_ storage: NSTextStorage, blockRange: NSRange) {
        let spacing: CGFloat = 14
        let blockStart = blockRange.location
        if blockStart > 0 {
            let prevLineEnd = blockStart - 1
            if prevLineEnd < storage.length,
               let prevPara = storage.attribute(.paragraphStyle, at: prevLineEnd, effectiveRange: nil) as? NSParagraphStyle {
                let modified = prevPara.mutableCopy() as! NSMutableParagraphStyle
                modified.paragraphSpacing = max(modified.paragraphSpacing, spacing)
                storage.addAttribute(.paragraphStyle, value: modified, range: NSRange(location: prevLineEnd, length: 1))
            }
        }
        let blockEnd = NSMaxRange(blockRange) + 1
        if blockEnd < storage.length,
           let endPara = storage.attribute(.paragraphStyle, at: blockEnd, effectiveRange: nil) as? NSParagraphStyle {
            let modified = endPara.mutableCopy() as! NSMutableParagraphStyle
            modified.paragraphSpacingBefore = max(modified.paragraphSpacingBefore, spacing)
            let lineEnd = (storage.string as NSString).lineRange(for: NSRange(location: blockEnd, length: 0))
            storage.addAttribute(.paragraphStyle, value: modified, range: lineEnd)
        }
    }
}
