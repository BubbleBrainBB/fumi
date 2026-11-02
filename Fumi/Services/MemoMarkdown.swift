import Foundation

/// A memo's Markdown on the Memos page: its blocks (paragraphs, list items,
/// quotes, code), drawn once a memo is saved, and its syntax spans, which
/// the composer colors while typing.
enum MemoMarkdown {
    enum Marker: Equatable {
        case bullet
        /// "1." or "1)", as written.
        case ordered(String)
        case task(checked: Bool)
    }

    enum Block: Equatable {
        /// Consecutive lines, their line breaks kept.
        case paragraph(String)
        /// `line` is the item's line in the memo, for ticking a to-do.
        case item(level: Int, marker: Marker, text: String, line: Int)
        /// Consecutive `>` lines without their `>`.
        case quote(String)
        case code(language: String, text: String)
    }

    private static let listPrefix = try! NSRegularExpression(pattern: #"^([ \t]*)([-*+]|\d+[.)])[ \t]+(\[[ xX]\](?:[ \t]+|$))?"#)
    private static let quotePrefix = try! NSRegularExpression(pattern: #"^ {0,3}((?:>[ \t]?)+)"#)

    private static func lines(_ text: String) -> [String] {
        text.components(separatedBy: "\n")
    }

    /// The fence a line opens or closes (```` ``` ```` or `~~~`), or nil.
    private static func fence(_ line: String) -> String? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("```") { return "```" }
        if trimmed.hasPrefix("~~~") { return "~~~" }
        return nil
    }

    private static func match(_ regex: NSRegularExpression, _ line: String) -> NSTextCheckingResult? {
        regex.firstMatch(in: line, range: NSRange(location: 0, length: (line as NSString).length))
    }

    private static func item(_ line: String, at index: Int) -> Block? {
        guard let m = match(listPrefix, line) else { return nil }
        let ns = line as NSString
        let columns = ns.substring(with: m.range(at: 1)).reduce(0) { $0 + ($1 == "\t" ? 4 : 1) }
        let token = ns.substring(with: m.range(at: 2))
        let marker: Marker
        if m.range(at: 3).location != NSNotFound {
            marker = .task(checked: ns.substring(with: m.range(at: 3)).lowercased().contains("x"))
        } else if token.first?.isNumber == true {
            marker = .ordered(token)
        } else {
            marker = .bullet
        }
        return .item(level: min(columns / 2, 4), marker: marker,
                     text: ns.substring(from: NSMaxRange(m.range)), line: index)
    }

    static func blocks(_ text: String) -> [Block] {
        var blocks: [Block] = []
        var paragraph: [String] = []
        var quote: [String] = []
        var code: (fence: String, language: String, lines: [String])?

        func flush() {
            if !paragraph.isEmpty { blocks.append(.paragraph(paragraph.joined(separator: "\n"))) }
            if !quote.isEmpty { blocks.append(.quote(quote.joined(separator: "\n"))) }
            paragraph = []
            quote = []
        }

        for (index, line) in lines(text).enumerated() {
            if let open = code {
                if fence(line) == open.fence {
                    blocks.append(.code(language: open.language, text: open.lines.joined(separator: "\n")))
                    code = nil
                } else {
                    code?.lines.append(line)
                }
                continue
            }
            if let mark = fence(line) {
                flush()
                let info = line.trimmingCharacters(in: .whitespaces).dropFirst(3)
                code = (mark, info.trimmingCharacters(in: .whitespaces), [])
            } else if line.trimmingCharacters(in: .whitespaces).isEmpty {
                flush()
            } else if let m = match(quotePrefix, line) {
                if !paragraph.isEmpty { flush() }
                quote.append((line as NSString).substring(from: NSMaxRange(m.range)))
            } else if let item = item(line, at: index) {
                flush()
                blocks.append(item)
            } else {
                if !quote.isEmpty { flush() }
                paragraph.append(line)
            }
        }
        // An unclosed fence still shows as code.
        if let open = code {
            blocks.append(.code(language: open.language, text: open.lines.joined(separator: "\n")))
        }
        flush()
        return blocks
    }

    /// `text` with the to-do on `line` ticked or unticked.
    static func togglingTask(line: Int, in text: String) -> String {
        var all = lines(text)
        guard all.indices.contains(line), let m = match(listPrefix, all[line]),
              m.range(at: 3).location != NSNotFound else { return text }
        let ns = all[line] as NSString
        let box = NSRange(location: m.range(at: 3).location + 1, length: 1)
        let checked = ns.substring(with: box).lowercased() == "x"
        all[line] = ns.replacingCharacters(in: box, with: checked ? " " : "x")
        return all.joined(separator: "\n")
    }

    // MARK: Syntax spans

    struct Span: Equatable {
        enum Kind: Equatable {
            /// Syntax characters: `**`, `- `, `> `, fences…
            case marker
            case strong, emphasis, strike, code, link, tag
            /// The text of a quote line.
            case quote
            /// The lines inside a fence.
            case codeBlock
            /// The text of a ticked to-do.
            case done
        }

        let kind: Kind
        let range: NSRange
    }

    private struct InlineRule {
        let kind: Span.Kind
        let regex: NSRegularExpression
        /// Delimiter length on each side; a link's are uneven and found by group.
        let delimiter: Int
    }

    private static func rule(_ kind: Span.Kind, _ pattern: String, _ delimiter: Int) -> InlineRule {
        InlineRule(kind: kind, regex: try! NSRegularExpression(pattern: pattern), delimiter: delimiter)
    }

    // In precedence order: code shields what's inside it, strong claims its
    // `**` before emphasis looks for `*`.
    private static let inlineRules = [
        rule(.code, #"`[^`\n]+`"#, 1),
        rule(.link, #"\[([^\]\n]+)\]\(([^)\s]+)\)"#, 0),
        rule(.strong, #"\*\*(?!\s)(.+?)(?<!\s)\*\*"#, 2),
        rule(.strong, #"(?<!\w)__(?!\s)(.+?)(?<!\s)__(?!\w)"#, 2),
        rule(.strike, #"~~(?!\s)(.+?)(?<!\s)~~"#, 2),
        rule(.emphasis, #"(?<!\*)\*(?![\s*])(.+?)(?<![\s*])\*(?!\*)"#, 1),
        rule(.emphasis, #"(?<![_\w])_(?![\s_])(.+?)(?<![\s_])_(?![_\w])"#, 1),
    ]

    /// Where the Markdown is in `text`, by kind; ranges are UTF-16.
    static func spans(in text: String) -> [Span] {
        var spans: [Span] = []
        var location = 0
        var openFence: String?
        for line in lines(text) {
            let length = (line as NSString).length
            let range = NSRange(location: location, length: length)
            if let open = openFence {
                if fence(line) == open {
                    spans.append(Span(kind: .marker, range: range))
                    openFence = nil
                } else if length > 0 {
                    spans.append(Span(kind: .codeBlock, range: range))
                }
            } else if let mark = fence(line) {
                spans.append(Span(kind: .marker, range: range))
                openFence = mark
            } else {
                lineSpans(line, at: location, into: &spans)
            }
            location += length + 1
        }
        return spans
    }

    private static func lineSpans(_ line: String, at offset: Int, into spans: inout [Span]) {
        let ns = line as NSString
        var start = 0
        var contentKind: Span.Kind?
        if let m = match(quotePrefix, line) {
            start = NSMaxRange(m.range)
            spans.append(Span(kind: .marker, range: NSRange(location: offset, length: start)))
            contentKind = .quote
        }
        let rest = ns.substring(from: start)
        if let m = match(listPrefix, rest) {
            spans.append(Span(kind: .marker, range: NSRange(location: offset + start, length: m.range.length)))
            if m.range(at: 3).location != NSNotFound,
               (rest as NSString).substring(with: m.range(at: 3)).lowercased().contains("x") {
                contentKind = .done
            }
            start += m.range.length
        }
        let content = NSRange(location: offset + start, length: ns.length - start)
        if let contentKind, content.length > 0 { spans.append(Span(kind: contentKind, range: content)) }
        inlineSpans(ns.substring(from: start), at: content.location, into: &spans)
    }

    private static func inlineSpans(_ text: String, at offset: Int, into spans: inout [Span]) {
        let ns = text as NSString
        let whole = NSRange(location: 0, length: ns.length)
        // Characters already spoken for: delimiters, and the inside of code.
        var taken = IndexSet()
        func isFree(_ range: NSRange) -> Bool {
            !taken.intersects(integersIn: range.location..<NSMaxRange(range))
        }
        func add(_ kind: Span.Kind, _ range: NSRange) {
            guard range.length > 0 else { return }
            spans.append(Span(kind: kind, range: NSRange(location: offset + range.location, length: range.length)))
        }

        for rule in inlineRules {
            for m in rule.regex.matches(in: text, range: whole) {
                let r = m.range
                let open: NSRange, inner: NSRange, close: NSRange
                if rule.kind == .link {
                    open = NSRange(location: r.location, length: 1)
                    inner = m.range(at: 1)
                    close = NSRange(location: NSMaxRange(inner), length: NSMaxRange(r) - NSMaxRange(inner))
                } else {
                    open = NSRange(location: r.location, length: rule.delimiter)
                    close = NSRange(location: NSMaxRange(r) - rule.delimiter, length: rule.delimiter)
                    inner = NSRange(location: NSMaxRange(open), length: close.location - NSMaxRange(open))
                }
                guard isFree(open), isFree(close), rule.kind != .code || isFree(r) else { continue }
                add(.marker, open)
                add(.marker, close)
                add(rule.kind, inner)
                taken.insert(integersIn: open.location..<NSMaxRange(open))
                taken.insert(integersIn: close.location..<NSMaxRange(close))
                if rule.kind == .code { taken.insert(integersIn: r.location..<NSMaxRange(r)) }
            }
        }
        for tag in MemoFormat.tagRanges(in: text) where isFree(tag.range) {
            add(.tag, tag.range)
        }
    }
}
