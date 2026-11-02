import Foundation

/// The Xiaohongshu Cards extension's input (design boards ㊾, 51, 52): a note
/// broken into the blocks the card layout paginates
/// (Resources/Xiaohongshu/cards.js), plus the post's title and caption, and
/// what can't go on cards. The title is the post's, not a card's: the cards
/// start with the note's first words.
///
/// Foundation-only, so Tests/XhsDeckTests.swift runs it directly.
struct XhsDeck: Codable, Equatable {
    struct Block: Codable, Equatable {
        enum Kind: String, Codable {
            case heading, paragraph, figure, quote, list, code, table, pageBreak
        }

        /// Stable across edits elsewhere in the note: Leave Out of Cards
        /// remembers blocks by it.
        var id: String
        var kind: Kind
        /// Headings.
        var level: Int?
        /// Headings, quotes and tables, as inline HTML.
        var html: String?
        /// Paragraphs, one inline-HTML string per sentence: a paragraph may
        /// break between two cards only there.
        var sentences: [String]?
        /// Lists.
        var items: [String]?
        var ordered: Bool?
        /// Figures: the image, its caption (the alt text), and the short
        /// sentence before it that introduces it, which stays on its card.
        var src: String?
        var caption: String?
        var lead: String?
        /// Code blocks.
        var text: String?
    }

    var blocks: [Block]
    /// The post's title: `xhs_title` in the front matter, else the note's.
    var title: String
    /// The post's text: the whole note as plain text, and its tags.
    var caption: String
    /// What can't go on cards, in English.
    var warnings: [String]
    /// Blocks hidden with Leave Out of Cards.
    var leftOutCount: Int

    static let cardLimit = 18

    // MARK: - Building

    static func make(markdown: String, notePath: String, fileName: String,
                     leftOut: Set<String> = []) -> XhsDeck {
        let source = markdown as NSString
        let frontmatter = Frontmatter.parse(source)
        let body = frontmatter.map { source.substring(from: $0.range.length) } ?? markdown
        func field(_ key: String) -> String? {
            guard case .text(let value)? = frontmatter?.fields.first(where: { $0.key == key })?.value else { return nil }
            let trimmed = value.trimmingCharacters(in: .whitespaces)
            return trimmed.isEmpty ? nil : trimmed
        }

        var raw = rawBlocks(body)
        var warnings: [String] = []

        // The title: the front matter's, else a leading # heading (left
        // off the cards: it is the post's title), else the file name.
        var title = field("title")
        if let first = raw.first, first.kind == .heading, first.level == 1 {
            if title == nil { title = plain(first.markdown) }
            raw.removeFirst()
        }
        let noteTitle = title ?? (fileName as NSString).deletingPathExtension

        // Images resolve against the note's folder; videos can't go on cards.
        let folder = (notePath as NSString).deletingLastPathComponent
        var videos = 0
        var missing: [String] = []
        raw = raw.compactMap { block in
            guard block.kind == .figure, let target = block.src else { return block }
            if isVideo(target) { videos += 1; return nil }
            if let resolved = resolveImage(target, folder: folder) {
                var block = block
                block.src = resolved
                return block
            }
            missing.append(target)
            return nil
        }
        if videos > 0 {
            warnings.append(videos == 1 ? "1 video can't go on a card; add it in Xiaohongshu."
                                        : "\(videos) videos can't go on cards; add them in Xiaohongshu.")
        }
        for target in missing { warnings.append("Image not found: \(target)") }
        if raw.contains(where: { $0.markdown.contains("$$") }) { warnings.append("Math stays as plain text.") }

        // Stable ids, and what Leave Out of Cards hid.
        var seen: [String: Int] = [:]
        for index in raw.indices {
            let base = fingerprint("\(raw[index].kind.rawValue)\u{1}\(raw[index].markdown)")
            let count = seen[base, default: 0]
            seen[base] = count + 1
            raw[index].id = count == 0 ? base : "\(base)-\(count)"
        }
        let leftOutCount = raw.filter { leftOut.contains($0.id) }.count
        raw.removeAll { leftOut.contains($0.id) }

        let tags = TagExtractor.frontmatterTags(from: markdown).isEmpty
            ? TagExtractor.extract(from: body) : TagExtractor.frontmatterTags(from: markdown)
        return XhsDeck(blocks: withLeads(raw), title: field("xhs_title") ?? noteTitle,
                       caption: caption(raw, tags: tags), warnings: warnings, leftOutCount: leftOutCount)
    }

    // MARK: - Blocks

    /// A block as written, before its sentences and leads are worked out.
    private struct RawBlock {
        var id = ""
        var kind: Block.Kind
        var level: Int?
        /// The block's Markdown, for its text and its id.
        var markdown: String
        var items: [String]?
        var ordered: Bool?
        var src: String?
        var caption: String?
        var lang: String?
    }

    private static let headingLine = try! NSRegularExpression(pattern: #"^(#{1,6})[ \t]+(.*?)[ \t#]*$"#)
    private static let ruleLine = try! NSRegularExpression(pattern: #"^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$"#)
    private static let listLine = try! NSRegularExpression(pattern: #"^[ \t]*([-*+]|(\d+)[.)])[ \t]+(?:\[[ xX]\][ \t]+)?(.*)$"#)
    private static let imageLine = try! NSRegularExpression(pattern: #"^!\[([^\]\n]*)\]\(<?([^)>\n]*?)>?(?:[ \t]+"[^"\n]*")?\)$"#)
    private static let tableSeparator = try! NSRegularExpression(pattern: #"^[ \t]*\|?[ \t]*:?-{2,}:?[ \t]*(\|[ \t]*:?-{2,}:?[ \t]*)*\|?[ \t]*$"#)

    private static func groups(_ regex: NSRegularExpression, _ line: String) -> [String]? {
        let ns = line as NSString
        guard let match = regex.firstMatch(in: line, range: NSRange(location: 0, length: ns.length)) else { return nil }
        return (0..<match.numberOfRanges).map {
            let range = match.range(at: $0)
            return range.location == NSNotFound ? "" : ns.substring(with: range)
        }
    }

    private static func rawBlocks(_ body: String) -> [RawBlock] {
        let lines = body.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")
        var blocks: [RawBlock] = []
        var paragraph: [String] = []
        var index = 0

        func flushParagraph() {
            guard !paragraph.isEmpty else { return }
            blocks.append(RawBlock(kind: .paragraph, markdown: joinLines(paragraph)))
            paragraph = []
        }

        while index < lines.count {
            let line = lines[index]
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            if trimmed.isEmpty || (trimmed.hasPrefix("<!--") && trimmed.hasSuffix("-->")) {
                flushParagraph()
                index += 1
                continue
            }
            // Fenced code.
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                flushParagraph()
                let fence = String(trimmed.prefix(3))
                let lang = String(trimmed.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                var code: [String] = []
                index += 1
                while index < lines.count, !lines[index].trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
                    code.append(lines[index])
                    index += 1
                }
                index += 1
                blocks.append(RawBlock(kind: .code, markdown: code.joined(separator: "\n"), lang: lang))
                continue
            }
            if let parts = groups(headingLine, trimmed) {
                flushParagraph()
                blocks.append(RawBlock(kind: .heading, level: parts[1].count, markdown: parts[2]))
                index += 1
                continue
            }
            if groups(ruleLine, line) != nil {
                flushParagraph()
                blocks.append(RawBlock(kind: .pageBreak, markdown: "---"))
                index += 1
                continue
            }
            if let parts = groups(imageLine, trimmed) {
                flushParagraph()
                let target = parts[2].trimmingCharacters(in: .whitespaces)
                blocks.append(RawBlock(kind: .figure, markdown: trimmed, src: target,
                                       caption: parts[1].trimmingCharacters(in: .whitespaces)))
                index += 1
                continue
            }
            // Obsidian embeds (![[file.png]]) count as images too.
            if trimmed.hasPrefix("![["), trimmed.hasSuffix("]]") {
                flushParagraph()
                let target = String(trimmed.dropFirst(3).dropLast(2)).components(separatedBy: "|")[0]
                blocks.append(RawBlock(kind: .figure, markdown: trimmed, src: target, caption: ""))
                index += 1
                continue
            }
            if trimmed.hasPrefix(">") {
                flushParagraph()
                var quote: [String] = []
                while index < lines.count {
                    let next = lines[index].trimmingCharacters(in: .whitespaces)
                    guard next.hasPrefix(">") else { break }
                    var text = String(next.dropFirst())
                    if text.hasPrefix(" ") { text.removeFirst() }
                    // A callout's [!type] line is its label, not its words.
                    if quote.isEmpty, text.hasPrefix("[!") {
                        let label = text.components(separatedBy: "]").dropFirst().joined(separator: "]")
                            .trimmingCharacters(in: CharacterSet(charactersIn: "-+ "))
                        if !label.isEmpty { quote.append("**\(label)**") }
                    } else {
                        quote.append(text)
                    }
                    index += 1
                }
                blocks.append(RawBlock(kind: .quote, markdown: joinLines(quote)))
                continue
            }
            if let first = groups(listLine, line) {
                flushParagraph()
                var items: [String] = []
                let ordered = !first[2].isEmpty
                while index < lines.count {
                    let next = lines[index]
                    if let parts = groups(listLine, next) {
                        items.append(parts[3])
                    } else if !next.trimmingCharacters(in: .whitespaces).isEmpty, next.hasPrefix(" ") || next.hasPrefix("\t"),
                              !items.isEmpty {
                        items[items.count - 1] = joinLines([items[items.count - 1], next.trimmingCharacters(in: .whitespaces)])
                    } else {
                        break
                    }
                    index += 1
                }
                blocks.append(RawBlock(kind: .list, markdown: items.joined(separator: "\n"), items: items, ordered: ordered))
                continue
            }
            if trimmed.contains("|"), index + 1 < lines.count, groups(tableSeparator, lines[index + 1]) != nil {
                flushParagraph()
                var rows: [String] = [line]
                index += 2
                while index < lines.count, lines[index].contains("|"),
                      !lines[index].trimmingCharacters(in: .whitespaces).isEmpty {
                    rows.append(lines[index])
                    index += 1
                }
                blocks.append(RawBlock(kind: .table, markdown: rows.joined(separator: "\n")))
                continue
            }
            paragraph.append(line)
            index += 1
        }
        flushParagraph()
        return blocks
    }

    /// Lines of one paragraph as one: no space where Chinese meets Chinese.
    private static func joinLines(_ lines: [String]) -> String {
        lines.map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }.reduce("") { joined, line in
            guard let last = joined.last, let first = line.first else { return joined + line }
            return joined + (isCJK(last) || isCJK(first) ? "" : " ") + line
        }
    }

    /// The blocks for the layout: paragraphs in sentences, and each image
    /// with the short sentence that introduces it.
    private static func withLeads(_ raw: [RawBlock]) -> [Block] {
        var blocks: [Block] = []
        for block in raw {
            switch block.kind {
            case .paragraph:
                blocks.append(Block(id: block.id, kind: .paragraph, sentences: sentences(block.markdown).map(inline)))
            case .figure:
                var lead: String?
                // A sentence ending in a colon, or a paragraph of one short
                // sentence, right before the image goes with it (board 52).
                if let last = blocks.last, last.kind == .paragraph, var sentences = last.sentences, let final = sentences.last {
                    let text = plainHTML(final)
                    let endsInColon = text.hasSuffix("：") || text.hasSuffix(":")
                    if endsInColon || (sentences.count == 1 && text.count <= 40) {
                        lead = final
                        sentences.removeLast()
                        if sentences.isEmpty { blocks.removeLast() } else { blocks[blocks.count - 1].sentences = sentences }
                    }
                }
                blocks.append(Block(id: block.id, kind: .figure, src: block.src,
                                    caption: (block.caption ?? "").isEmpty ? nil : escape(block.caption ?? ""), lead: lead))
            case .heading:
                blocks.append(Block(id: block.id, kind: .heading, level: block.level, html: inline(block.markdown)))
            case .quote:
                blocks.append(Block(id: block.id, kind: .quote, html: inline(block.markdown)))
            case .list:
                blocks.append(Block(id: block.id, kind: .list, items: (block.items ?? []).map(inline), ordered: block.ordered))
            case .code:
                blocks.append(Block(id: block.id, kind: .code, text: block.markdown))
            case .table:
                blocks.append(Block(id: block.id, kind: .table, html: table(block.markdown)))
            case .pageBreak:
                blocks.append(Block(id: block.id, kind: .pageBreak))
            }
        }
        return blocks
    }

    // MARK: - Sentences

    private static let sentenceEnds: Set<Character> = ["。", "！", "？", "!", "?", "；", "…"]
    private static let closers: Set<Character> = ["”", "’", "」", "』", "）", ")", "】", "\"", "'", "*", "_", "~", "=", "。", "…"]

    /// `markdown` in sentences, cut only where no bold, code or link is open,
    /// so each piece stands as Markdown of its own. A very long sentence is
    /// cut again at its commas.
    static func sentences(_ markdown: String) -> [String] {
        var pieces: [String] = []
        var current = ""
        var bold = false, italic = false, code = false, brackets = 0
        let characters = Array(markdown)
        var index = 0
        var balanced: Bool { !bold && !italic && !code && brackets == 0 }

        /// Appends the character at `index` (two for `**`), tracking what it opens or closes.
        func take() {
            let character = characters[index]
            current.append(character)
            if code {
                if character == "`" { code = false }
                return
            }
            switch character {
            case "`": code = true
            case "*":
                if index + 1 < characters.count, characters[index + 1] == "*" {
                    index += 1
                    current.append("*")
                    bold.toggle()
                } else {
                    italic.toggle()
                }
            case "[": brackets += 1
            case "]": brackets = max(0, brackets - 1)
            default: break
            }
        }

        while index < characters.count {
            let character = characters[index]
            take()
            if !code, sentenceEnds.contains(character) {
                // Take the closing marks and quotes that belong to it.
                while index + 1 < characters.count, closers.contains(characters[index + 1]) {
                    index += 1
                    take()
                }
                if balanced {
                    pieces.append(current)
                    current = ""
                }
            } else if balanced, character == "，", plain(current).count > 90 {
                pieces.append(current)
                current = ""
            }
            index += 1
        }
        if !current.trimmingCharacters(in: .whitespaces).isEmpty { pieces.append(current) }
        return pieces.map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }

    // MARK: - Inline Markdown

    static func escape(_ text: String) -> String {
        text.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;").replacingOccurrences(of: "\"", with: "&quot;")
    }

    private static let inlineRules: [(NSRegularExpression, String)] = [
        (#"\*\*(?=\S)(.+?)(?<=\S)\*\*"#, "<b>$1</b>"),
        (#"__(?=\S)(.+?)(?<=\S)__"#, "<b>$1</b>"),
        (#"(?<![\*\w])\*(?=\S)(.+?)(?<=\S)\*(?!\*)"#, "<i>$1</i>"),
        (#"~~(?=\S)(.+?)(?<=\S)~~"#, "<s>$1</s>"),
        (#"==(?=\S)(.+?)(?<=\S)=="#, "<mark>$1</mark>"),
        (#"&lt;u&gt;(.+?)&lt;/u&gt;"#, "<u>$1</u>"),
        (#"&lt;br\s*/?&gt;"#, " "),
        (#"!\[([^\]]*)\]\([^)]*\)"#, "$1"),
        (#"\[([^\]]+)\]\((?:[^)]*)\)"#, "<span class=\"link\">$1</span>"),
        (#"\[\[(?:[^\]|]*\|)?([^\]]+)\]\]"#, "$1"),
        (#"&lt;/?[A-Za-z][^&]*?&gt;"#, ""),
    ].map { (try! NSRegularExpression(pattern: $0.0), $0.1) }

    /// Inline Markdown as HTML: bold, italic, strike, highlight, underline,
    /// code and links (as text: a card can't be clicked). Other HTML is dropped.
    static func inline(_ markdown: String) -> String {
        // Code spans first, so nothing inside them is read as Markdown.
        var codes: [String] = []
        var text = ""
        var rest = Substring(markdown)
        while let open = rest.firstIndex(of: "`") {
            text += rest[..<open]
            let after = rest[rest.index(after: open)...]
            guard let close = after.firstIndex(of: "`") else { text += rest[open...]; rest = ""; break }
            codes.append(String(after[..<close]))
            text += "\u{0}\(codes.count - 1)\u{0}"
            rest = after[after.index(after: close)...]
        }
        text += rest
        text = text.replacingOccurrences(of: #"\\([\\`*_{}\[\]()#+\-.!~=|<>])"#, with: "$1", options: .regularExpression)
        var html = escape(text) as NSString
        for (regex, template) in inlineRules {
            html = regex.stringByReplacingMatches(in: html as String, range: NSRange(location: 0, length: html.length),
                                                  withTemplate: template) as NSString
        }
        var result = html as String
        for (index, code) in codes.enumerated() {
            result = result.replacingOccurrences(of: "\u{0}\(index)\u{0}", with: "<code>\(escape(code))</code>")
        }
        return result.trimmingCharacters(in: .whitespaces)
    }

    /// The words alone.
    static func plain(_ markdown: String) -> String {
        plainHTML(inline(markdown))
    }

    static func plainHTML(_ html: String) -> String {
        html.replacingOccurrences(of: "<[^>]+>", with: "", options: .regularExpression)
            .replacingOccurrences(of: "&lt;", with: "<").replacingOccurrences(of: "&gt;", with: ">")
            .replacingOccurrences(of: "&quot;", with: "\"").replacingOccurrences(of: "&amp;", with: "&")
            .trimmingCharacters(in: .whitespaces)
    }

    private static func table(_ markdown: String) -> String {
        func cells(_ row: String) -> [String] {
            var line = row.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("|") { line.removeFirst() }
            if line.hasSuffix("|") { line.removeLast() }
            return line.components(separatedBy: "|").map { inline($0.trimmingCharacters(in: .whitespaces)) }
        }
        let rows = markdown.components(separatedBy: "\n")
        guard let head = rows.first else { return "" }
        let header = cells(head).map { "<th>\($0)</th>" }.joined()
        let body = rows.dropFirst().map { "<tr>" + cells($0).map { "<td>\($0)</td>" }.joined() + "</tr>" }.joined()
        return "<table><thead><tr>\(header)</tr></thead><tbody>\(body)</tbody></table>"
    }

    // MARK: - Caption

    /// The whole note as plain text — headings, paragraphs, quotes and
    /// lists, in order — then the note's tags as topics. Images, code and
    /// tables are on the cards only.
    private static func caption(_ raw: [RawBlock], tags: [String]) -> String {
        var parts: [String] = []
        for block in raw {
            switch block.kind {
            case .heading, .paragraph, .quote:
                parts.append(plain(block.markdown))
            case .list:
                let items = block.items ?? []
                parts.append(items.enumerated().map { index, item in
                    (block.ordered == true ? "\(index + 1). " : "• ") + plain(item)
                }.joined(separator: "\n"))
            case .figure, .code, .table, .pageBreak:
                break
            }
        }
        let caption = parts.filter { !$0.isEmpty }.joined(separator: "\n\n")
        let topics = tags.map { "#" + ($0.split(separator: "/").last.map(String.init) ?? $0) }.joined(separator: " ")
        return topics.isEmpty ? caption : caption + "\n\n" + topics
    }

    // MARK: - Images

    private static let videoExtensions: Set<String> = ["mp4", "mov", "m4v", "webm", "avi", "mkv"]
    private static let imageExtensions: Set<String> = ["png", "jpg", "jpeg", "gif", "webp", "heic", "tif", "tiff", "bmp", "svg"]

    private static func isVideo(_ target: String) -> Bool {
        let path = target.components(separatedBy: "?")[0]
        return videoExtensions.contains((path as NSString).pathExtension.lowercased())
    }

    /// A web image as it is; a file next to the note through the card
    /// view's own scheme (XhsAssets); nil when the file isn't there.
    private static func resolveImage(_ target: String, folder: String) -> String? {
        if target.hasPrefix("http://") || target.hasPrefix("https://") || target.hasPrefix("data:image/") { return target }
        var path = target.removingPercentEncoding ?? target
        if path.hasPrefix("file://") { path = URL(string: target)?.path ?? path }
        let absolute = path.hasPrefix("/") ? path : (folder as NSString).appendingPathComponent(path)
        let standardized = (absolute as NSString).standardizingPath
        guard FileManager.default.fileExists(atPath: standardized),
              imageExtensions.contains((standardized as NSString).pathExtension.lowercased()) else { return nil }
        return XhsDeck.assetURL(forFile: standardized)
    }

    /// `fumi-xhs://local/<path>`: how the card view loads a file on this Mac.
    static func assetURL(forFile path: String) -> String {
        let encoded = path.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? path
        return "fumi-xhs://local" + (encoded.hasPrefix("/") ? encoded : "/" + encoded)
    }

    // MARK: - Characters

    static func isCJK(_ character: Character) -> Bool {
        character.unicodeScalars.contains { scalar in
            switch scalar.value {
            case 0x2E80...0x2FFF, 0x3040...0x9FFF, 0xF900...0xFAFF, 0x3000...0x303F, 0xFF00...0xFFEF: true
            default: false
            }
        }
    }


    /// FNV-1a, as hex: short and the same on every launch.
    private static func fingerprint(_ text: String) -> String {
        var hash: UInt64 = 0xcbf29ce484222325
        for byte in text.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x100000001b3
        }
        return String(hash, radix: 16)
    }
}
