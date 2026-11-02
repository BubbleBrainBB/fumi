import AppKit
import Foundation
import UniformTypeIdentifiers

/// Exports a note as a PDF.
///
/// The note is laid out by the editor's own engine (`WysiwygEngine`) and the
/// editor's own layout manager (`CodeBlockLayoutManager`), so the sheet shows
/// what the editor shows: the same fonts, the same spacing, the same
/// headings, lists, quotes, code blocks, callouts, tables, math and images.
/// Only the paper is new — A4 with margins, a running foot and page numbers,
/// painted in the light palette so it reads well on white.
enum PDFExporter {

    // MARK: - Configuration

    /// The sheet and the type it is set in. The type follows the editor's own
    /// settings, so the PDF reads like the note on screen.
    struct Configuration {
        /// A4, in points.
        var pageSize = CGSize(width: 595.28, height: 841.89)
        var margin = NSEdgeInsets(top: 64, left: 60, bottom: 60, right: 60)
        var fontName = AppSettings.editorFont
        var fontSize = CGFloat(AppSettings.editorFontSize)
        var lineHeight = CGFloat(AppSettings.editorLineHeight)
        /// The note's name, shown in the foot of every page.
        var title = ""
    }

    // MARK: - Rendering

    /// Lays `markdown` out and returns the PDF, or nil when the sheet is too
    /// small to hold anything.
    ///
    /// - Parameter baseURL: The note's folder, so relative image paths in the
    ///   note resolve to files next to it.
    static func render(markdown: String,
                       configuration: Configuration = Configuration(),
                       baseURL: URL? = nil) -> Data? {
        let pageSize = configuration.pageSize
        let margin = configuration.margin
        let contentWidth = pageSize.width - margin.left - margin.right
        let contentHeight = pageSize.height - margin.top - margin.bottom
        guard contentWidth > 60, contentHeight > 60 else { return nil }

        let previousBase = ImageLoader.shared.baseURL
        if let baseURL { ImageLoader.shared.setBaseURL(baseURL) }
        defer { ImageLoader.shared.setBaseURL(previousBase) }

        // The editor's own text stack, at the width of the sheet's column.
        let storage = NSTextStorage(string: markdown)
        let layoutManager = CodeBlockLayoutManager()
        let container = NSTextContainer(containerSize: CGSize(width: contentWidth,
                                                             height: CGFloat.greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        layoutManager.addTextContainer(container)
        storage.addLayoutManager(layoutManager)

        // Styled exactly as the editor styles it — but with no caret, so no
        // raw Markdown marker is revealed and every block is drawn as it is
        // meant to be read.
        let style = WysiwygEngine.makeStyle(fontSize: configuration.fontSize,
                                             lineHeight: configuration.lineHeight,
                                             fontName: configuration.fontName,
                                             strongColor: MarkdownPalette.strong(accentHex: AppSettings.accentColorHex))
        let engine = WysiwygEngine(style: style)
        engine.applyStyle(to: storage, cursorLocation: -1, maxWidth: contentWidth)

        // The engine keeps the Markdown marks in the text — shrunk and
        // transparent. On screen that is how it hides them; on paper they
        // would still be picked up when the text is copied, so they go.
        stripHiddenSyntax(from: storage)

        // Nothing may run off the sheet: an image or a table taller than the
        // page is shrunk to fit instead of being cut in half.
        fitAttachments(in: storage, layoutManager: layoutManager, container: container, toHeight: contentHeight)

        layoutManager.ensureLayout(for: container)
        let pages = paginate(layoutManager: layoutManager, container: container, pageHeight: contentHeight)

        let data = NSMutableData()
        guard let consumer = CGDataConsumer(data: data as CFMutableData) else { return nil }
        var mediaBox = CGRect(origin: .zero, size: pageSize)
        let info: [String: Any] = [
            kCGPDFContextTitle as String: configuration.title,
            kCGPDFContextCreator as String: "Fumi",
        ]
        guard let context = CGContext(consumer: consumer, mediaBox: &mediaBox,
                                      info as CFDictionary) else { return nil }

        draw(pages: pages, layoutManager: layoutManager, configuration: configuration, into: context)
        context.closePDF()
        return data as Data
    }

    // MARK: - Pages

    /// One laid-out line: where it sits and which glyphs it holds.
    private struct Line {
        var rect: NSRect
        var glyphs: NSRange
    }

    /// Breaks the laid-out text into pages of `pageHeight` points. A line is
    /// never split: one that does not fit goes to the next page whole.
    private static func paginate(layoutManager: NSLayoutManager,
                                 container: NSTextContainer,
                                 pageHeight: CGFloat) -> [[Line]] {
        var lines: [Line] = []
        layoutManager.enumerateLineFragments(forGlyphRange: layoutManager.glyphRange(for: container)) { rect, _, _, glyphs, _ in
            lines.append(Line(rect: rect, glyphs: glyphs))
        }

        var pages: [[Line]] = []
        var page: [Line] = []
        var top: CGFloat = 0
        for line in lines {
            if !page.isEmpty && line.rect.maxY - top > pageHeight {
                pages.append(page)
                page = []
                top = line.rect.minY
            }
            page.append(line)
        }
        // An empty note still gets one sheet, foot and all.
        pages.append(page)
        return pages
    }

    /// Draws the pages into the PDF context.
    private static func draw(pages: [[Line]],
                             layoutManager: CodeBlockLayoutManager,
                             configuration: Configuration,
                             into context: CGContext) {
        let pageSize = configuration.pageSize
        let margin = configuration.margin
        let total = pages.count

        for (index, page) in pages.enumerated() {
            context.beginPDFPage(nil)
            context.saveGState()
            // The sheet's top-left corner at the origin, so the text is laid
            // out the way it reads on screen.
            context.translateBy(x: 0, y: pageSize.height)
            context.scaleBy(x: 1, y: -1)

            let nsContext = NSGraphicsContext(cgContext: context, flipped: true)

            // Paper is white: every themed color takes its light value.
            NSAppearance(named: .aqua)?.performAsCurrentDrawingAppearance {
                NSGraphicsContext.saveGraphicsState()
                NSGraphicsContext.current = nsContext

                if let first = page.first, let last = page.last {
                    let origin = NSPoint(x: margin.left, y: margin.top - first.rect.minY)
                    let glyphs = NSRange(location: first.glyphs.location,
                                         length: NSMaxRange(last.glyphs) - first.glyphs.location)
                    layoutManager.drawBackground(forGlyphRange: glyphs, at: origin)
                    layoutManager.drawGlyphs(forGlyphRange: glyphs, at: origin)
                }
                drawFoot(page: index + 1, of: total, configuration: configuration)

                NSGraphicsContext.restoreGraphicsState()
            }

            context.restoreGState()
            context.endPDFPage()
        }
    }

    /// The foot of every page: the note's name on the left, the page number
    /// on the right.
    private static func drawFoot(page: Int, of total: Int, configuration: Configuration) {
        let pageSize = configuration.pageSize
        let margin = configuration.margin
        let y = pageSize.height - margin.bottom + 22
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 9),
            .foregroundColor: MarkdownPalette.faint,
        ]
        if !configuration.title.isEmpty {
            (configuration.title as NSString).draw(at: NSPoint(x: margin.left, y: y),
                                                   withAttributes: attributes)
        }
        let number = "\(page) / \(total)"
        let width = (number as NSString).size(withAttributes: attributes).width
        (number as NSString).draw(at: NSPoint(x: pageSize.width - margin.right - width, y: y),
                                  withAttributes: attributes)
    }

    // MARK: - Attachments

    /// Takes the Markdown marks out of the text the reader copies out of the
    /// PDF.
    ///
    /// The engine hides a mark by shrinking it and painting it transparent,
    /// and hangs the decorations on it — the bullet of a list item, a rule, a
    /// callout's icon — so the mark cannot simply be deleted. It is replaced
    /// by a zero-width space instead: the line stays alive, the decoration
    /// stays on it, and the copied text reads "Lists" rather than "## Lists".
    private static func stripHiddenSyntax(from storage: NSTextStorage) {
        var hidden: [NSRange] = []
        storage.enumerateAttribute(.foregroundColor, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let color = value as? NSColor, color.alphaComponent == 0 else { return }
            // Only the engine's own hiding: a tiny font beside the clear color.
            guard let font = storage.attribute(.font, at: range.location, effectiveRange: nil) as? NSFont,
                  font.pointSize < 1 else { return }
            hidden.append(range)
        }
        // From the back, so the ranges found earlier stay valid.
        for range in hidden.reversed() {
            guard range.length > 0, NSMaxRange(range) <= storage.length else { continue }
            let text = storage.string
            if let stringRange = Range(range, in: text), text[stringRange].contains("\u{FFFC}") { continue }
            // One at a time, so each keeps the attributes it was drawn with.
            for offset in stride(from: range.length - 1, through: 0, by: -1) {
                let character = NSRange(location: range.location + offset, length: 1)
                let attributes = storage.attributes(at: character.location, effectiveRange: nil)
                storage.replaceCharacters(in: character, with: "\u{200B}")
                storage.setAttributes(attributes, range: character)
            }
        }
    }

    /// Shrinks what is taller than the page so that nothing is cut off at the
    /// sheet's edge.
    ///
    /// A line takes more room than the picture on it — the paragraph's own
    /// spacing above and below — so the text is laid out first and the space
    /// around the picture is measured, before the picture is sized to what is
    /// left of the page.
    private static func fitAttachments(in storage: NSTextStorage,
                                       layoutManager: CodeBlockLayoutManager,
                                       container: NSTextContainer,
                                       toHeight maxHeight: CGFloat) {
        layoutManager.ensureLayout(for: container)

        var replacements: [(NSRange, NSTextAttachment)] = []
        storage.enumerateAttribute(.attachment, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let attachment = value as? NSTextAttachment,
                  attachment.bounds.height > maxHeight else { return }
            // The line the attachment sits on, and what it takes besides it.
            let glyphs = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            guard glyphs.length > 0 else { return }
            let line = layoutManager.lineFragmentRect(forGlyphAt: glyphs.location, effectiveRange: nil)
            let around = max(0, line.height - attachment.bounds.height)
            let budget = maxHeight - around - 1
            guard budget > 40 else { return }
            // An image is re-made at the smaller size, so it stays sharp; the
            // rest is drawn smaller as it is.
            let fitted = (attachment as? ImageTextAttachment)?.scaled(toFitHeight: budget)
                ?? ScaledAttachment(wrapping: attachment, toHeight: budget)
            replacements.append((range, fitted))
        }
        guard !replacements.isEmpty else { return }
        for (range, attachment) in replacements {
            storage.addAttribute(.attachment, value: attachment, range: range)
        }
        layoutManager.ensureLayout(for: container)
    }

    // MARK: - Remote images

    /// Fetches the note's remote images so the sheet shows them instead of
    /// their links. Local images are already on disk.
    static func preloadRemoteImages(in markdown: String) async {
        let ns = markdown as NSString
        var urls: [String] = []
        InlineLink.imagePattern.enumerateMatches(in: markdown,
                                                 range: NSRange(location: 0, length: ns.length)) { match, _, _ in
            guard let match else { return }
            let range = InlineLink.destinationRange(of: match)
            guard range.location != NSNotFound, range.length > 0 else { return }
            let url = ns.substring(with: range).lowercased()
            if url.hasPrefix("http://") || url.hasPrefix("https://") {
                urls.append(ns.substring(with: range))
            }
        }
        for url in urls {
            await loadImage(url)
        }
    }

    /// Loads one image, giving up after a while: a load already in flight
    /// never calls back, so waiting for that alone would hang the export.
    private static func loadImage(_ url: String) async {
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            let done = Once()
            let finish = { done.run { continuation.resume() } }
            ImageLoader.shared.loadAsync(urlString: url) { _ in finish() }
            // The cache is watched as well, for a load that was already
            // running: the wait is over as soon as the image is there.
            Task { @MainActor in
                for _ in 0..<200 {
                    try? await Task.sleep(for: .milliseconds(100))
                    if ImageLoader.shared.loadSync(urlString: url) != nil { break }
                }
                finish()
            }
        }
    }

    /// A one-shot flag: the image may answer twice (loaded, then the timeout).
    private final class Once: @unchecked Sendable {
        private var fired = false
        func run(_ block: () -> Void) {
            guard !fired else { return }
            fired = true
            block()
        }
    }

    // MARK: - Saving

    /// Asks where to put the PDF and writes it there.
    @MainActor
    static func export(note: FileNote, markdown: String) async {
        let panel = NSSavePanel()
        panel.title = "Export as PDF"
        panel.prompt = "Export"
        panel.allowedContentTypes = [.pdf]
        panel.canCreateDirectories = true
        panel.nameFieldStringValue = "\(fileName(for: note.filename)).pdf"
        panel.directoryURL = note.url.deletingLastPathComponent()
        guard panel.runModal() == .OK, let url = panel.url else { return }

        var configuration = Configuration()
        configuration.title = note.filename

        // The sheet is drawn from the note as it is on screen, so the images
        // that are still on their way are fetched first.
        await preloadRemoteImages(in: markdown)
        guard let data = render(markdown: markdown, configuration: configuration,
                                baseURL: note.url.deletingLastPathComponent()) else {
            presentError("The note could not be laid out for printing.")
            return
        }
        do {
            try data.write(to: url, options: .atomic)
        } catch {
            presentError(error.localizedDescription)
        }
    }

    /// A file name that is safe to write: the note's name without the
    /// characters a file name may not hold.
    static func fileName(for title: String) -> String {
        let cleaned = title
            .replacingOccurrences(of: "/", with: "-")
            .replacingOccurrences(of: ":", with: "-")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return cleaned.isEmpty ? "Note" : cleaned
    }

    @MainActor
    private static func presentError(_ message: String) {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = "The PDF could not be exported"
        alert.informativeText = message
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }
}

/// An attachment drawn smaller so it fits the page. The PDF export uses it
/// for anything taller than the sheet that cannot be re-made at a smaller
/// size.
final class ScaledAttachment: NSTextAttachment {
    private let source: NSTextAttachment

    init(wrapping attachment: NSTextAttachment, toHeight maxHeight: CGFloat) {
        source = attachment
        super.init(data: nil, ofType: nil)
        let height = attachment.bounds.height
        let factor = height > 0 ? min(1, maxHeight / height) : 1
        bounds = NSRect(origin: .zero,
                        size: NSSize(width: attachment.bounds.width * factor,
                                     height: attachment.bounds.height * factor))
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func image(forBounds imageBounds: NSRect,
                        textContainer: NSTextContainer?,
                        characterIndex charIndex: Int) -> NSImage? {
        guard let image = source.image(forBounds: source.bounds,
                                       textContainer: textContainer,
                                       characterIndex: charIndex) else { return nil }
        let target = NSRect(origin: .zero, size: imageBounds.size)
        return NSImage(size: target.size, flipped: true) { _ in
            image.draw(in: target, from: .zero, operation: .sourceOver, fraction: 1,
                       respectFlipped: true, hints: nil)
            return true
        }
    }
}

extension ScaledAttachment: MarkdownBackedAttachment {
    var markdownSource: String {
        (source as? MarkdownBackedAttachment)?.markdownSource ?? ""
    }
}
