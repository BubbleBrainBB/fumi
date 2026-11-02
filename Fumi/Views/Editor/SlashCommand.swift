import Foundation

/// The slash menu's items and the plain-text rules behind it (design board
/// 56): when a typed `/` opens the menu, how the text after it filters the
/// items, and what picking one does to the line. Foundation-only, so the
/// rules are tested without a text view (Tests/SlashCommandTests.swift).
struct SlashCommand: Identifiable, Equatable {
    enum Group: String, CaseIterable {
        case basic = "Basic", blocks = "Blocks", insert = "Insert", format = "Format"
    }

    /// What the item's tile shows: a few letters, or an SF Symbol.
    enum Icon: Equatable {
        case letters(String)
        case serifLetters(String)
        case symbol(String)
    }

    enum Action: Equatable {
        /// Makes the whole line this kind of block: replaces the Markdown
        /// prefix the line already has (`# `, `- `, `> `…) with this one.
        case linePrefix(String)
        /// A block of its own lines; `selection` is where the caret lands
        /// (or what is selected) within `text`.
        case block(String, selection: NSRange)
        /// Wraps the caret: `prefix|suffix`.
        case inline(prefix: String, suffix: String)
        /// Asks for files, which arrive as on a drop.
        case chooseImage
    }

    let id: String
    let title: String
    let group: Group
    let icon: Icon
    /// The gray hint on the right: the syntax it inserts, or its shortcut.
    let hint: String
    /// Also matched when filtering: English synonyms, the Chinese name and
    /// its pinyin initials.
    let keywords: [String]
    let action: Action

    static let all: [SlashCommand] = [
        SlashCommand(id: "text", title: "Text", group: .basic, icon: .serifLetters("Aa"), hint: "",
                     keywords: ["p", "plain", "paragraph", "正文", "zw"], action: .linePrefix("")),
        SlashCommand(id: "h1", title: "Heading 1", group: .basic, icon: .letters("H1"), hint: "#",
                     keywords: ["h1", "title", "标题", "bt"], action: .linePrefix("# ")),
        SlashCommand(id: "h2", title: "Heading 2", group: .basic, icon: .letters("H2"), hint: "##",
                     keywords: ["h2", "标题", "bt"], action: .linePrefix("## ")),
        SlashCommand(id: "h3", title: "Heading 3", group: .basic, icon: .letters("H3"), hint: "###",
                     keywords: ["h3", "标题", "bt"], action: .linePrefix("### ")),
        SlashCommand(id: "bullets", title: "Bulleted List", group: .basic, icon: .symbol("list.bullet"), hint: "-",
                     keywords: ["ul", "bullet", "列表", "lb"], action: .linePrefix("- ")),
        SlashCommand(id: "numbers", title: "Numbered List", group: .basic, icon: .letters("1."), hint: "1.",
                     keywords: ["ol", "number", "编号", "bh"], action: .linePrefix("1. ")),
        SlashCommand(id: "todo", title: "To-do", group: .basic, icon: .symbol("checkmark.square"), hint: "[ ]",
                     keywords: ["todo", "task", "check", "待办", "db"], action: .linePrefix("- [ ] ")),
        SlashCommand(id: "quote", title: "Quote", group: .basic, icon: .symbol("text.quote"), hint: ">",
                     keywords: ["blockquote", "引用", "yy"], action: .linePrefix("> ")),

        SlashCommand(id: "callout", title: "Callout", group: .blocks, icon: .symbol("info.square"), hint: "[!]",
                     keywords: ["note", "tip", "warning", "提示", "ts"], action: .linePrefix("> [!note] ")),
        SlashCommand(id: "code", title: "Code Block", group: .blocks,
                     icon: .symbol("chevron.left.forwardslash.chevron.right"), hint: "```",
                     keywords: ["code", "fence", "代码", "dm"],
                     action: .block("```\n\n```", selection: NSRange(location: 4, length: 0))),
        SlashCommand(id: "math", title: "Math Block", group: .blocks, icon: .serifLetters("∑"), hint: "$$",
                     keywords: ["latex", "formula", "公式", "gs"],
                     action: .block("$$\n\n$$", selection: NSRange(location: 3, length: 0))),
        SlashCommand(id: "table", title: "Table", group: .blocks, icon: .symbol("tablecells"), hint: "|",
                     keywords: ["grid", "表格", "bg"],
                     action: .block("| Column 1 | Column 2 |\n| --- | --- |\n|  |  |",
                                    selection: NSRange(location: 2, length: 8))),
        SlashCommand(id: "divider", title: "Divider", group: .blocks, icon: .symbol("minus"), hint: "---",
                     keywords: ["hr", "line", "rule", "分割线", "fgx"],
                     action: .block("---\n", selection: NSRange(location: 4, length: 0))),

        SlashCommand(id: "image", title: "Image…", group: .insert, icon: .symbol("photo"), hint: "",
                     keywords: ["photo", "picture", "video", "图片", "tp"], action: .chooseImage),
        SlashCommand(id: "link", title: "Link", group: .insert, icon: .symbol("link"), hint: "⌘K",
                     keywords: ["url", "链接", "lj"], action: .inline(prefix: "[", suffix: "](url)")),

        SlashCommand(id: "bold", title: "Bold", group: .format, icon: .symbol("bold"), hint: "⌘B",
                     keywords: ["b", "strong", "粗体", "ct"], action: .inline(prefix: "**", suffix: "**")),
        SlashCommand(id: "italic", title: "Italic", group: .format, icon: .symbol("italic"), hint: "⌘I",
                     keywords: ["i", "em", "斜体", "xt"], action: .inline(prefix: "*", suffix: "*")),
        SlashCommand(id: "strike", title: "Strikethrough", group: .format, icon: .symbol("strikethrough"), hint: "~~",
                     keywords: ["del", "strike", "删除线", "scx"], action: .inline(prefix: "~~", suffix: "~~")),
        SlashCommand(id: "highlight", title: "Highlight", group: .format, icon: .symbol("highlighter"), hint: "==",
                     keywords: ["mark", "高亮", "gl"], action: .inline(prefix: "==", suffix: "==")),
        SlashCommand(id: "inlineCode", title: "Inline Code", group: .format, icon: .letters("`"), hint: "`",
                     keywords: ["code", "行内代码", "hndm"], action: .inline(prefix: "`", suffix: "`")),
    ]

    // MARK: - Filtering

    /// One item that matches the typed query, with the offsets of the
    /// matched characters in its title (drawn bold).
    struct Result: Equatable {
        let command: SlashCommand
        let titleOffsets: [Int]
    }

    /// The items for `query` (the text typed after the slash), best first.
    /// An empty query lists everything in menu order. A title that starts
    /// with the query ranks first, then a keyword typed in full (`db`,
    /// `待办`), then a fuzzy match in the title, then a keyword's start;
    /// ties keep menu order.
    static func results(for query: String, in commands: [SlashCommand] = all) -> [Result] {
        let query = query.replacingOccurrences(of: "'", with: "")
        guard !query.isEmpty else { return commands.map { Result(command: $0, titleOffsets: []) } }
        let lowered = query.lowercased()
        var scored: [(score: Int, index: Int, result: Result)] = []
        for (index, command) in commands.enumerated() {
            if command.title.lowercased().hasPrefix(lowered) {
                let offsets = Array(0..<(query as NSString).length)
                scored.append((3000, index, Result(command: command, titleOffsets: offsets)))
            } else if command.keywords.contains(where: { $0.lowercased() == lowered }) {
                scored.append((2000, index, Result(command: command, titleOffsets: [])))
            } else if let match = FuzzyMatch.match(query, in: command.title) {
                scored.append((1000 + match.score, index, Result(command: command, titleOffsets: match.offsets)))
            } else if command.keywords.contains(where: { $0.lowercased().hasPrefix(lowered) }) {
                scored.append((500, index, Result(command: command, titleOffsets: [])))
            }
        }
        return scored.sorted { $0.score != $1.score ? $0.score > $1.score : $0.index < $1.index }.map(\.result)
    }

    // MARK: - When the menu opens

    /// The characters that open the menu: `/`, and `、`, which a Chinese
    /// input method types for the same key.
    static func isTrigger(_ character: unichar) -> Bool {
        character == 0x2F || character == 0x3001
    }

    /// Whether a slash typed at `location` in `text` (raw Markdown) opens
    /// the menu: at the start of a line or after a space, and not inside
    /// code, math, a table's source or the front matter.
    static func opensMenu(at location: Int, in text: NSString) -> Bool {
        guard location < text.length, isTrigger(text.character(at: location)) else { return false }
        let lineRange = text.lineRange(for: NSRange(location: location, length: 0))
        if location > lineRange.location {
            let previous = text.character(at: location - 1)
            guard previous == 0x20 || previous == 0x09 || previous == 0x3000 else { return false }
        }
        let line = text.substring(with: lineRange)
        let beforeSlash = (line as NSString).substring(to: location - lineRange.location)
        if line.trimmingCharacters(in: .whitespaces).hasPrefix("|") { return false }
        // An open inline code span: an odd number of backticks before it.
        if beforeSlash.filter({ $0 == "`" }).count % 2 == 1 { return false }
        return !isInsideBlock(at: lineRange.location, in: text)
    }

    /// Whether the line starting at `lineStart` is inside a fenced code
    /// block, a `$$` math block or the front matter.
    static func isInsideBlock(at lineStart: Int, in text: NSString) -> Bool {
        var fence: String?
        var inMath = false
        var inFrontmatter = false
        var location = 0
        var lineIndex = 0
        while location < lineStart {
            let range = text.lineRange(for: NSRange(location: location, length: 0))
            let trimmed = text.substring(with: range).trimmingCharacters(in: .whitespacesAndNewlines)
            if lineIndex == 0, trimmed == "---" {
                inFrontmatter = true
            } else if inFrontmatter {
                if trimmed == "---" || trimmed == "..." { inFrontmatter = false }
            } else if let open = fence {
                if trimmed.hasPrefix(open) { fence = nil }
            } else if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                fence = String(trimmed.prefix(3))
            } else if trimmed == "$$" {
                inMath.toggle()
            } else if trimmed.hasPrefix("$$"), !(trimmed.count > 2 && trimmed.hasSuffix("$$")) {
                inMath.toggle()
            } else if inMath, trimmed.hasSuffix("$$") {
                inMath = false
            }
            location = NSMaxRange(range)
            lineIndex += 1
        }
        return fence != nil || inMath || inFrontmatter
    }

    // MARK: - Picking an item

    /// What picking `self` does to one line: `line` is the line's text
    /// (no line break) and `query` the range of the slash and the text typed
    /// after it, both in the line. Returns the line's new text — which may
    /// run over several lines — and the selection in it.
    func edit(line: String, query: NSRange) -> (text: String, selection: NSRange) {
        let ns = line as NSString
        let before = ns.substring(to: query.location)
        let after = ns.substring(from: NSMaxRange(query))
        let rest = before + after
        let caret = (before as NSString).length

        switch action {
        case .linePrefix(let prefix):
            let (indent, oldPrefix) = Self.blockPrefix(of: rest)
            let contentStart = indent + oldPrefix
            let content = (rest as NSString).substring(from: contentStart)
            let text = (rest as NSString).substring(to: indent) + prefix + content
            let caretInContent = max(0, caret - contentStart)
            return (text, NSRange(location: indent + (prefix as NSString).length + caretInContent, length: 0))

        case .block(let block, let selection):
            if rest.trimmingCharacters(in: .whitespaces).isEmpty {
                return (block, selection)
            }
            // Below the line's own text, after a blank line, so a `---`
            // under a paragraph doesn't turn it into a heading.
            let lead = rest + "\n\n"
            let offset = (lead as NSString).length
            return (lead + block, NSRange(location: offset + selection.location, length: selection.length))

        case .inline(let prefix, let suffix):
            let text = before + prefix + suffix + after
            return (text, NSRange(location: caret + (prefix as NSString).length, length: 0))

        case .chooseImage:
            return (rest, NSRange(location: caret, length: 0))
        }
    }

    /// The length of a line's leading indentation and of the block marker
    /// after it (`### `, `- [ ] `, `1. `, `> [!tip] `…), in UTF-16 units.
    static func blockPrefix(of line: String) -> (indent: Int, prefix: Int) {
        let ns = line as NSString
        var indent = 0
        while indent < ns.length, ns.character(at: indent) == 0x20 || ns.character(at: indent) == 0x09 { indent += 1 }
        let body = ns.substring(from: indent)
        let patterns = [
            #"^#{1,6} "#,
            #"^> \[![A-Za-z-]+\][+-]? ?"#,
            #"^[-*+] \[[ xX]\] "#,
            #"^[-*+] "#,
            #"^\d+[.)] "#,
            #"^> ?"#,
        ]
        for pattern in patterns {
            if let range = body.range(of: pattern, options: .regularExpression) {
                return (indent, (String(body[range]) as NSString).length)
            }
        }
        return (indent, 0)
    }
}
