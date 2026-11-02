import AppKit

/// An attachment whose visual representation stands in for a span of raw
/// Markdown. `markdownSource` is the exact text it replaces, so the document's
/// raw Markdown can always be reconstructed (see `WysiwygTextView.markdownString`).
protocol MarkdownBackedAttachment: AnyObject {
    var markdownSource: String { get }
}

/// Parsed structure of a GFM-style Markdown table.
struct MarkdownTable: Equatable {
    enum Alignment: Equatable { case left, center, right }
    var headers: [String]
    var alignments: [Alignment]
    var rows: [[String]]
    var columnCount: Int { headers.count }
}

/// One table found in a document: its raw character range plus the parsed table.
struct ParsedMarkdownTable {
    let range: NSRange
    let table: MarkdownTable
}

/// Detects GFM pipe tables in raw Markdown. A table is a row containing `|`
/// immediately followed by a delimiter row (`| --- | :--: |`), then zero or more
/// rows. Tables inside fenced code blocks are ignored.
enum MarkdownTableParser {

    static func tables(in text: NSString) -> [ParsedMarkdownTable] {
        var lines: [(range: NSRange, text: String)] = []
        text.enumerateSubstrings(in: NSRange(location: 0, length: text.length), options: [.byLines]) { sub, range, _, _ in
            lines.append((range, sub ?? ""))
        }

        // Mark which line indices are inside a fenced code block.
        var inFence = [Bool](repeating: false, count: lines.count)
        var fenced = false
        for (i, line) in lines.enumerated() {
            let t = line.text.trimmingCharacters(in: .whitespaces)
            if t.hasPrefix("```") || t.hasPrefix("~~~") {
                inFence[i] = true   // the fence line itself is "in" a block
                fenced.toggle()
                continue
            }
            inFence[i] = fenced
        }

        var results: [ParsedMarkdownTable] = []
        var i = 0
        while i < lines.count {
            guard !inFence[i] else { i += 1; continue }
            let header = lines[i].text
            if isTableRow(header), i + 1 < lines.count, !inFence[i + 1],
               isDelimiterRow(lines[i + 1].text),
               lines[i + 1].text.contains("|") || splitCells(header).count == 1 {
                var j = i + 2
                while j < lines.count, !inFence[j], isTableRow(lines[j].text) { j += 1 }
                let start = lines[i].range.location
                let lastLine = lines[j - 1].range
                let blockRange = NSRange(location: start, length: (lastLine.location + lastLine.length) - start)
                if let table = parse(header: header,
                                     delimiter: lines[i + 1].text,
                                     body: (i + 2..<j).map { lines[$0].text }) {
                    results.append(ParsedMarkdownTable(range: blockRange, table: table))
                }
                i = j
            } else {
                i += 1
            }
        }
        return results
    }

    static func isTableRow(_ line: String) -> Bool {
        let t = line.trimmingCharacters(in: .whitespaces)
        guard t.contains("|") else { return false }
        if t.hasPrefix("```") || t.hasPrefix("~~~") { return false }
        return true
    }

    static func isDelimiterRow(_ line: String) -> Bool {
        let t = line.trimmingCharacters(in: .whitespaces)
        guard t.contains("-") else { return false }
        return t.range(of: #"^\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?$"#, options: .regularExpression) != nil
    }

    static func splitCells(_ line: String) -> [String] {
        var t = line.trimmingCharacters(in: .whitespaces)
        if t.hasPrefix("|") { t.removeFirst() }
        if t.hasSuffix("|") { t.removeLast() }
        return t.components(separatedBy: "|").map { $0.trimmingCharacters(in: .whitespaces) }
    }

    static func parse(header: String, delimiter: String, body: [String]) -> MarkdownTable? {
        let headers = splitCells(header)
        guard !headers.isEmpty else { return nil }
        var alignments = splitCells(delimiter).map { cell -> MarkdownTable.Alignment in
            let left = cell.hasPrefix(":"), right = cell.hasSuffix(":")
            if left && right { return .center }
            if right { return .right }
            return .left
        }
        while alignments.count < headers.count { alignments.append(.left) }
        if alignments.count > headers.count { alignments = Array(alignments.prefix(headers.count)) }

        let rows = body.map { line -> [String] in
            var cells = splitCells(line)
            while cells.count < headers.count { cells.append("") }
            if cells.count > headers.count { cells = Array(cells.prefix(headers.count)) }
            return cells
        }
        return MarkdownTable(headers: headers, alignments: alignments, rows: rows)
    }
}

/// A cell's inline Markdown, rendered the way body text renders it: `**bold**`
/// in the bold color, `*italic*`, `` `code` `` monospaced on a rounded fill,
/// `[links](…)` underlined, `~~strike~~` — with the markers gone, so a cell
/// is measured and wrapped by what it shows.
enum TableCellText {
    /// On inline code, for the rounded fill drawn behind it.
    static let codeKey = NSAttributedString.Key("fumi.tableCellCode")

    private static let code = try! NSRegularExpression(pattern: #"`([^`\n]+)`"#)
    private static let link = try! NSRegularExpression(pattern: #"\[([^\]\n]+)\]\(([^)\s]+)\)"#)
    private static let bold = try! NSRegularExpression(pattern: #"\*\*(?=\S)(.+?)(?<=\S)\*\*|__(?=\S)(.+?)(?<=\S)__"#)
    private static let italic = try! NSRegularExpression(
        pattern: #"(?<![*\w])\*(?=[^\s*])(.+?)(?<=[^\s*])\*(?![*\w])|(?<![_\w])_(?=[^\s_])(.+?)(?<=[^\s_])_(?![_\w])"#)
    private static let strike = try! NSRegularExpression(pattern: #"~~(?=\S)(.+?)(?<=\S)~~"#)

    static func attributed(_ text: String, font: NSFont, color: NSColor, strongColor: NSColor,
                           alignment: NSTextAlignment = .left) -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = alignment
        paragraph.lineBreakMode = .byWordWrapping
        let result = NSMutableAttributedString(string: text, attributes: [
            .font: font, .foregroundColor: color, .paragraphStyle: paragraph,
        ])
        let mono = NSFont.monospacedSystemFont(ofSize: WysiwygEngine.smallSize(font.pointSize), weight: .regular)

        // Code first: nothing inside it is Markdown.
        replace(code, in: result) { content in
            content.addAttributes([.font: mono, .foregroundColor: MarkdownPalette.inlineCode, codeKey: true],
                                  range: NSRange(location: 0, length: content.length))
        }
        replace(link, in: result) { content in
            content.addAttributes([.underlineStyle: NSUnderlineStyle.single.rawValue,
                                   .underlineColor: MarkdownPalette.linkUnderline],
                                  range: NSRange(location: 0, length: content.length))
        }
        replace(bold, in: result) { content in
            convert(content, to: .boldFontMask)
            content.addAttribute(.foregroundColor, value: strongColor, range: NSRange(location: 0, length: content.length))
        }
        replace(italic, in: result) { content in convert(content, to: .italicFontMask) }
        replace(strike, in: result) { content in
            content.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue,
                                 range: NSRange(location: 0, length: content.length))
        }
        return result
    }

    /// The widest run that can't wrap (a Latin word, a path): a column
    /// narrower than this breaks it mid-word. Han and kana wrap anywhere.
    static func longestUnbreakableWidth(_ text: NSAttributedString) -> CGFloat {
        let word = try! NSRegularExpression(pattern: #"[^\s\p{Han}\p{Hiragana}\p{Katakana}\p{Hangul}，。、；：！？（）《》“”]+"#)
        return word.matches(in: text.string, range: NSRange(location: 0, length: text.length))
            .map { ceil(text.attributedSubstring(from: $0.range).size().width) }
            .max() ?? 0
    }

    /// Replaces each match with its captured content, styled by `style`;
    /// matches that reach into inline code are left as they are.
    private static func replace(_ pattern: NSRegularExpression, in text: NSMutableAttributedString,
                                style: (NSMutableAttributedString) -> Void) {
        let matches = pattern.matches(in: text.string, range: NSRange(location: 0, length: text.length))
        for match in matches.reversed() {
            if pattern !== code {
                var touchesCode = false
                text.enumerateAttribute(codeKey, in: match.range) { value, _, stop in
                    if value != nil { touchesCode = true; stop.pointee = true }
                }
                if touchesCode { continue }
            }
            guard let group = (1..<match.numberOfRanges).map(match.range(at:)).first(where: { $0.location != NSNotFound })
            else { continue }
            let content = NSMutableAttributedString(attributedString: text.attributedSubstring(from: group))
            style(content)
            text.replaceCharacters(in: match.range, with: content)
        }
    }

    private static func convert(_ text: NSMutableAttributedString, to trait: NSFontTraitMask) {
        text.enumerateAttribute(.font, in: NSRange(location: 0, length: text.length)) { value, range, _ in
            guard let font = value as? NSFont else { return }
            text.addAttribute(.font, value: NSFontManager.shared.convert(font, toHaveTrait: trait), range: range)
        }
    }
}

/// A non-editable text attachment that draws a Markdown table as a bordered
/// grid. Double-clicking it (see `WysiwygTextView`) reveals the raw Markdown for
/// editing; `markdownSource` lets the raw document be reconstructed.
final class TableAttachment: NSTextAttachment, MarkdownBackedAttachment {
    let markdownSource: String

    init(table: MarkdownTable, markdownSource: String, font: NSFont, maxWidth: CGFloat,
         strongColor: NSColor = MarkdownPalette.strong) {
        self.markdownSource = markdownSource
        super.init(data: nil, ofType: nil)
        let rendered = TableAttachment.render(table: table, font: font, maxWidth: maxWidth, strongColor: strongColor)
        image = rendered
        bounds = NSRect(origin: .zero, size: rendered.size)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    /// Text widths for the columns: as wide as their content when it all
    /// fits; otherwise every column keeps room for its longest word (when the
    /// words fit at all) and the rest of the width is shared by how much
    /// more each column wants.
    static func columnWidths(natural: [CGFloat], minimum: [CGFloat], available: CGFloat) -> [CGFloat] {
        guard natural.reduce(0, +) > available else { return natural }
        let floor30: CGFloat = 30
        let mins = zip(natural, minimum).map { min($0, max($1, floor30)) }
        let minTotal = mins.reduce(0, +)
        if minTotal >= available {
            let scale = available / max(1, minTotal)
            return mins.map { max(floor30, ($0 * scale).rounded(.down)) }
        }
        let extra = available - minTotal
        let wants = zip(natural, mins).map { $0 - $1 }
        let wantTotal = max(1, wants.reduce(0, +))
        return zip(mins, wants).map { ($0 + $1 * extra / wantTotal).rounded(.down) }
    }

    static func render(table: MarkdownTable, font baseFont: NSFont, maxWidth: CGFloat,
                       strongColor: NSColor = MarkdownPalette.strong) -> NSImage {
        let cols = max(1, table.columnCount)
        let headerFont = NSFont.systemFont(ofSize: baseFont.pointSize, weight: .semibold)
        let numberFont = NSFont.monospacedDigitSystemFont(ofSize: baseFont.pointSize, weight: .regular)
        let border: CGFloat = 1
        let padH: CGFloat = 12
        let padV: CGFloat = 8
        let radius: CGFloat = 8

        let borderColor = MarkdownPalette.tableBorder
        let headerBg = MarkdownPalette.codeBlockBg

        let allRows: [[String]] = [table.headers] + table.rows
        func cell(_ r: Int, _ c: Int) -> String { c < allRows[r].count ? allRows[r][c] : "" }

        // Left-aligned columns (the parser's default for a plain `---`) whose
        // body cells are all numbers read better right-aligned, tabular digits.
        func isNumeric(_ s: String) -> Bool {
            let t = s.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: ",", with: "")
            return !t.isEmpty && Double(t.trimmingCharacters(in: CharacterSet(charactersIn: "%$¥€+-"))) != nil
        }
        let numericColumn: [Bool] = (0..<cols).map { c in
            !table.rows.isEmpty && table.rows.allSatisfy { isNumeric(c < $0.count ? $0[c] : "") }
        }
        func alignment(_ c: Int) -> NSTextAlignment {
            if table.alignments.indices.contains(c) {
                switch table.alignments[c] {
                case .center: return .center
                case .right: return .right
                case .left: break
                }
            }
            return numericColumn[c] ? .right : .left
        }
        func font(_ r: Int, _ c: Int) -> NSFont { r == 0 ? headerFont : (numericColumn[c] ? numberFont : baseFont) }
        func color(forRow r: Int) -> NSColor { r == 0 ? MarkdownPalette.secondary : MarkdownPalette.primary }

        // Every cell as it will be drawn, markers gone.
        let texts: [[NSAttributedString]] = (0..<allRows.count).map { r in
            (0..<cols).map { c in
                TableCellText.attributed(cell(r, c), font: font(r, c), color: color(forRow: r),
                                         strongColor: strongColor, alignment: alignment(c))
            }
        }

        var natural = [CGFloat](repeating: 8, count: cols)
        var minimum = [CGFloat](repeating: 0, count: cols)
        for row in texts {
            for c in 0..<cols {
                natural[c] = max(natural[c], ceil(row[c].size().width))
                minimum[c] = max(minimum[c], TableCellText.longestUnbreakableWidth(row[c]))
            }
        }
        let chrome = border * CGFloat(cols + 1) + padH * 2 * CGFloat(cols)
        let availText = max(CGFloat(cols) * 30, maxWidth - chrome)
        let colW = columnWidths(natural: natural, minimum: minimum, available: availText)
        let totalWidth = ceil(colW.reduce(0, +) + chrome)
        // `colW` is each column's text width; a cell spans that plus padding.
        let cellW = colW.map { $0 + padH * 2 }

        // Each cell laid out once, for its height and then for drawing.
        let layouts: [[CellLayout]] = texts.map { row in
            (0..<cols).map { c in CellLayout(row[c], width: max(1, colW[c])) }
        }
        let rowH = layouts.map { row in (row.map(\.height).max() ?? 0) + padV * 2 }
        let totalHeight = ceil(rowH.reduce(0, +) + border * CGFloat(allRows.count + 1))
        let size = NSSize(width: max(totalWidth, 1), height: max(totalHeight, 1))

        return NSImage(size: size, flipped: true) { _ in
            let outline = NSBezierPath(roundedRect: NSRect(origin: .zero, size: size).insetBy(dx: border / 2, dy: border / 2),
                                       xRadius: radius, yRadius: radius)
            NSGraphicsContext.saveGraphicsState()
            outline.addClip()

            // Header background.
            headerBg.setFill()
            NSRect(x: 0, y: 0, width: size.width, height: rowH[0] + border).fill()

            // Inner grid lines (the rounded outline is stroked separately).
            borderColor.setFill()
            var yc: CGFloat = 0
            for r in 0..<(allRows.count - 1) {
                yc += rowH[r] + border
                NSRect(x: 0, y: yc, width: size.width, height: border).fill()
            }
            var xc: CGFloat = 0
            for c in 0..<(cols - 1) {
                xc += cellW[c] + border
                NSRect(x: xc, y: 0, width: border, height: size.height).fill()
            }
            NSGraphicsContext.restoreGraphicsState()

            outline.lineWidth = border
            borderColor.setStroke()
            outline.stroke()

            // Cell text.
            var rowY: CGFloat = border
            for r in 0..<allRows.count {
                var colX: CGFloat = border
                for c in 0..<cols {
                    layouts[r][c].draw(at: NSPoint(x: colX + padH, y: rowY + padV))
                    colX += cellW[c] + border
                }
                rowY += rowH[r] + border
            }
            return true
        }
    }
}

/// One table cell set in TextKit: wraps at word boundaries in its column,
/// and draws inline code on a rounded fill like the body's.
private final class CellLayout {
    private let storage: NSTextStorage
    private let layoutManager = NSLayoutManager()
    private let container: NSTextContainer
    let height: CGFloat

    init(_ text: NSAttributedString, width: CGFloat) {
        storage = NSTextStorage(attributedString: text.length > 0 ? text : NSAttributedString(string: " "))
        container = NSTextContainer(size: NSSize(width: width, height: .greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        layoutManager.addTextContainer(container)
        storage.addLayoutManager(layoutManager)
        layoutManager.ensureLayout(for: container)
        height = ceil(layoutManager.usedRect(for: container).height)
    }

    func draw(at origin: NSPoint) {
        let full = NSRange(location: 0, length: storage.length)
        storage.enumerateAttribute(TableCellText.codeKey, in: full) { value, range, _ in
            guard value != nil else { return }
            let glyphs = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            layoutManager.enumerateEnclosingRects(forGlyphRange: glyphs,
                                                  withinSelectedGlyphRange: NSRange(location: NSNotFound, length: 0),
                                                  in: container) { rect, _ in
                let pill = rect.insetBy(dx: -WysiwygEngine.pillPadding, dy: 1)
                    .offsetBy(dx: origin.x, dy: origin.y)
                MarkdownPalette.inlineBg.setFill()
                NSBezierPath(roundedRect: pill, xRadius: 4, yRadius: 4).fill()
            }
        }
        layoutManager.drawGlyphs(forGlyphRange: layoutManager.glyphRange(for: container), at: origin)
    }
}
