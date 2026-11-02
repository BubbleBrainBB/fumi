import AppKit

// Inline Markdown inside table cells (TableCellText, TableAttachment):
//
//     swiftc -o /tmp/table-inline-tests \
//       Fumi/Views/Editor/CompositeFont.swift \
//       Fumi/Views/Editor/WysiwygEngine.swift \
//       Fumi/Views/Editor/CodeBlockLayoutManager.swift \
//       Fumi/Views/Editor/Callout.swift \
//       Fumi/Views/Editor/ImageAttachment.swift \
//       Fumi/Views/Editor/VideoAttachment.swift \
//       Fumi/Views/Editor/FrontmatterAttachment.swift \
//       Fumi/Views/Editor/MarkdownTable.swift \
//       Fumi/Views/Editor/LatexMath.swift \
//       Tests/ColorTokensShim.swift \
//       Fumi/Services/FrontmatterParser.swift \
//       Tests/TableInlineTests.swift
//     /tmp/table-inline-tests [optional/path/to/render.png]
//
// Exits non-zero if any assertion fails.

@main
enum TableInlineTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    static let table = """
    | 层 | 目录 | 权限 | 职责 |
    |---|---|---|---|
    | **Schema** | `README.md`、`AGENTS.md` | 人类 + LLM 谨慎修改 | 组织方式、页面类型、工作流 |
    | **Raw 只读层** | `_raw/clippings/` `_raw/ai-news/` `_raw/weixin/` `_raw/papers/` | **LLM 只读，永不修改** | 原始事实来源（source of truth） |
    | **Wiki 可写层** | `_wiki/notes/` `_wiki/moc/` `_wiki/projects/` | LLM 全权维护 | 综述页、实体页、对比页、矛盾标注 |
    """

    static let font = NSFont.systemFont(ofSize: 14)
    static let strong = MarkdownPalette.hex(0x1D6FC2)

    static func cell(_ text: String) -> NSAttributedString {
        TableCellText.attributed(text, font: font, color: MarkdownPalette.primary, strongColor: strong)
    }

    static func isBold(_ text: NSAttributedString, _ substring: String) -> Bool {
        let range = (text.string as NSString).range(of: substring)
        guard range.location != NSNotFound, let f = text.attribute(.font, at: range.location, effectiveRange: nil) as? NSFont
        else { return false }
        return NSFontManager.shared.traits(of: f).contains(.boldFontMask)
    }

    static func attribute(_ key: NSAttributedString.Key, _ text: NSAttributedString, _ substring: String) -> Any? {
        let range = (text.string as NSString).range(of: substring)
        guard range.location != NSNotFound else { return nil }
        return text.attribute(key, at: range.location, effectiveRange: nil)
    }

    static func main() {
        // ── Markers are gone, styles are on the right text ──
        let schema = cell("**Schema**")
        expect(schema.string, "Schema", "bold markers removed")
        expect(isBold(schema, "Schema"), true, "bold")
        expect(attribute(.foregroundColor, schema, "Schema") as? NSColor, strong, "bold in the bold color")

        let files = cell("`README.md`、`AGENTS.md`")
        expect(files.string, "README.md、AGENTS.md", "backticks removed")
        expect((attribute(.font, files, "AGENTS.md") as? NSFont)?.isFixedPitch, true, "code is monospaced")
        expect(attribute(TableCellText.codeKey, files, "README.md") as? Bool, true, "code marked for its fill")
        expect(attribute(TableCellText.codeKey, files, "、") as? Bool, nil, "text between code spans is plain")

        let mixed = cell("**LLM 只读，永不修改**")
        expect(mixed.string, "LLM 只读，永不修改", "bold with Chinese")
        expect(isBold(mixed, "只读"), true, "Chinese bold")

        let inline = cell("看 [文档](https://example.com)，*斜体*，~~删掉~~，`**不是粗体**`")
        expect(inline.string, "看 文档，斜体，删掉，**不是粗体**", "links, italic, strike; nothing parsed inside code")
        expect(attribute(.underlineStyle, inline, "文档") as? Int, NSUnderlineStyle.single.rawValue, "link underlined")
        expect(attribute(.strikethroughStyle, inline, "删掉") as? Int, NSUnderlineStyle.single.rawValue, "strike")
        expect(isBold(inline, "**不是粗体**"), false, "code stays code")

        expect(cell("a * b * c").string, "a * b * c", "lone asterisks stay")
        expect(cell("snake_case_name").string, "snake_case_name", "underscores inside words stay")
        expect(cell("").string, "", "empty cell")

        // ── Column widths: a word never breaks when the words fit ──
        let widths = TableAttachment.columnWidths(natural: [60, 400, 120, 300], minimum: [60, 140, 40, 30], available: 500)
        expect(widths[0] >= 60, true, "a column keeps its longest word")
        expect(widths[1] >= 140, true, "long code path keeps its width")
        expect(widths.reduce(0, +) <= 500, true, "columns fit the page")
        expect(TableAttachment.columnWidths(natural: [50, 60], minimum: [40, 50], available: 500), [50, 60],
               "a table that fits keeps its natural widths")
        let longest = TableCellText.longestUnbreakableWidth(cell("**Schema** 说明"))
        expect(longest >= ceil(cell("Schema").size().width), true, "longest word measured in its rendered font")

        // ── The whole table renders ──
        let parsed = MarkdownTableParser.tables(in: table as NSString)
        expect(parsed.count, 1, "table parsed")
        if let first = parsed.first {
            let image = TableAttachment.render(table: first.table, font: font, maxWidth: 680, strongColor: strong)
            expect(image.size.width <= 680, true, "fits the column")
            if CommandLine.arguments.count > 1,
               let tiff = image.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
               let png = rep.representation(using: .png, properties: [:]) {
                try? png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
            }
        }

        if failures == 0 {
            print("All \(checks) table inline checks passed.")
        } else {
            print("\(failures) of \(checks) table inline checks failed.")
            exit(1)
        }
    }
}
