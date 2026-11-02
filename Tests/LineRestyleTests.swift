import AppKit

// Differential test for the typing fast path: a line that
// `WysiwygEngine.canStyleAlone` accepts, styled by itself (`styledLine`),
// must come out exactly as the whole-note pass styles it in place —
// for every such line, with the caret on it and elsewhere.
//
//     swiftc -o /tmp/line-restyle-tests \
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
//       Tests/LineRestyleTests.swift
//     /tmp/line-restyle-tests
//
// Exits non-zero if any line differs.

@main
enum LineRestyleTests {
    static var failures = 0
    static var checks = 0

    static let fixture = """
    ---
    tags: [notes]
    ---

    # 京都看红叶

    开头一段，带 **粗体**、*斜体*、`code`、[链接](https://example.com) 和 #标签。
    中文和English混排，数字123也在里面。

    ## 第二个标题 with *emphasis*

    - 列表一，**重点**
    - 列表二
      - 嵌套 ~~删除~~
    1. 有序一
    2. 有序二 ==高亮==
    - [ ] 待办
    - [x] 完成了

    紧挨代码块的前一行
    ```swift
    let x = 1
    ```
    紧挨代码块的后一行

    > 引用一行
    > 引用两行

    ::: tip 提示
    提示正文
    :::

    普通段落 <u>下划线</u> 和 ***粗斜体***。

    ![图片](https://example.com/a.png)

    | a | b |
    |---|---|
    | 1 | 2 |

    行内公式 $x^2$ 后面的文字。

    ---

    ### 三级标题

    最后一段，没有结尾换行
    """

    static func main() {
        let style = WysiwygEngine.makeStyle(fontSize: 16, lineHeight: 1.75, fontName: "New York")
        let engine = WysiwygEngine(style: style)
        let raw = fixture as NSString

        // Caret positions: nowhere, and inside several lines.
        var carets = [-1, 0]
        for probe in ["粗体", "English", "emphasis", "列表一", "嵌套", "有序二", "待办", "前一行", "下划线", "三级", "没有结尾"] {
            let r = raw.range(of: probe)
            carets += [r.location, NSMaxRange(r)]
        }

        var eligibleLines = Set<String>()
        for caret in carets {
            let full = NSTextStorage(string: fixture)
            engine.applyStyle(to: full, cursorLocation: max(caret, 0), maxWidth: 640)
            let display = full.string as NSString
            var location = 0
            while location < display.length {
                let lineRange = display.lineRange(for: NSRange(location: location, length: 0))
                location = NSMaxRange(lineRange)
                guard WysiwygEngine.canStyleAlone(lineRange: lineRange, in: full) else { continue }
                let line = display.substring(with: lineRange)
                let content = line.trimmingCharacters(in: .newlines)
                guard !content.trimmingCharacters(in: .whitespaces).isEmpty else { continue }
                eligibleLines.insert(content)
                // The same line in the raw text, to place the caret in it.
                let rawStart = raw.range(of: line).location
                let rawLength = (content as NSString).length
                let cursorInLine = caret >= rawStart && caret <= rawStart + rawLength ? caret - rawStart : -1
                let alone = engine.styledLine(line, cursorInLine: cursorInLine, maxWidth: 640)
                compare(full, lineRange, alone, "caret \(caret), line \"\(content.prefix(16))\"")
            }
        }

        // The fast path must cover ordinary writing and refuse block lines.
        for must in ["开头一段，带 **粗体**、*斜体*、`code`、[链接](https://example.com) 和 #标签。",
                     "中文和English混排，数字123也在里面。", "## 第二个标题 with *emphasis*",
                     "- 列表一，**重点**", "  - 嵌套 ~~删除~~", "2. 有序二 ==高亮==", "- [ ] 待办",
                     "普通段落 <u>下划线</u> 和 ***粗斜体***。", "最后一段，没有结尾换行"] {
            checks += 1
            if !eligibleLines.contains(must) { failures += 1; print("FAIL: should style alone: \(must)") }
        }
        for mustNot in ["# 京都看红叶", "紧挨代码块的前一行", "紧挨代码块的后一行", "提示正文", "行内公式 $x^2$ 后面的文字。",
                        "> 引用一行", "tags: [notes]"] {
            checks += 1
            if eligibleLines.contains(mustNot) { failures += 1; print("FAIL: should not style alone: \(mustNot)") }
        }

        // ── Edits: the fast path on the edited document must equal a full pass ──
        var fastEdits = 0
        let edits: [(probe: String, offset: Int, removeLength: Int, insert: String)] = [
            ("粗体", 1, 0, "字"), ("English", 3, 0, "\n"), ("数字123也在里面。", 9, 0, "\n"),
            ("- 列表二", 4, 0, "\n"), ("有序二", 2, 0, "**"), ("普通段落", 2, 0, "\n"),
            ("最后一段", 0, 0, "\n"), ("三级标题", 4, 0, "\n"),
        ]
        for edit in edits {
            fastEdits += checkEdit(engine, probe: edit.probe, offset: edit.offset, removeLength: edit.removeLength,
                                   insert: edit.insert) ? 1 : 0
        }
        // ⌫ at the start of a line joins it to the one above.
        for probe in ["中文和English", "  - 嵌套", "2. 有序二"] {
            fastEdits += checkEdit(engine, probe: probe, offset: -1, removeLength: 1, insert: "") ? 1 : 0
        }
        // ── Writing a whole-note restyle back: only what changed ──
        let raw0 = fixture as NSString
        let scenarios: [(String, Int)] = [
            (fixture, raw0.range(of: "English").location),                                   // caret moves
            (fixture, raw0.range(of: "![图片]").location + 2),                                // into an image
            (fixture, raw0.range(of: "| 1 |").location + 2),                                  // into the table
            (fixture, raw0.range(of: "tags").location),                                       // into the frontmatter
            (raw0.replacingOccurrences(of: "提示正文", with: "提示正文\n多一行"), 0),            // edit in a callout
            (raw0.replacingOccurrences(of: "let x = 1", with: "let x = 1 // ok"), 0),          // edit in code
            (raw0.replacingOccurrences(of: "```swift", with: "``swift"), 0),                   // fence broken
            (raw0.replacingOccurrences(of: "![图片](https://example.com/a.png)\n", with: ""), 0),
            ("", 0), ("短\n", 1),
        ]
        var previous = NSTextStorage(string: fixture)
        engine.applyStyle(to: previous, cursorLocation: 0, maxWidth: 640)
        for (index, scenario) in scenarios.enumerated() {
            let styled = NSTextStorage(string: scenario.0)
            engine.applyStyle(to: styled, cursorLocation: scenario.1, maxWidth: 640)
            WysiwygEngine.applyChanges(from: styled, to: previous)
            checks += 1
            if previous.string != styled.string {
                failures += 1
                print("FAIL: applyChanges scenario \(index): text differs")
            } else {
                var same = true
                for i in 0..<styled.length where !(styled.attributes(at: i, effectiveRange: nil) as NSDictionary)
                    .isEqual(to: previous.attributes(at: i, effectiveRange: nil)) {
                    same = false
                    failures += 1
                    print("FAIL: applyChanges scenario \(index): attributes differ at \(i)")
                    break
                }
                _ = same
            }
            previous = styled.length > 0 ? previous : NSTextStorage(string: fixture)
        }

        checks += 1
        if fastEdits < 8 { failures += 1; print("FAIL: only \(fastEdits) edits took the fast path") }

        if failures == 0 {
            print("All \(checks) line-restyle checks passed (\(eligibleLines.count) lines styled alone, \(fastEdits) edits).")
        } else {
            print("\(failures) of \(checks) line-restyle checks failed.")
            exit(1)
        }
    }

    /// Applies an edit near `probe` (offset from its start; -1 is the line
    /// break before it) to a fully styled document, restyles the affected
    /// lines alone as the editor does, and compares the whole document with a
    /// full pass over the edited text. Returns whether the fast path applied.
    static func checkEdit(_ engine: WysiwygEngine, probe: String, offset: Int, removeLength: Int, insert: String) -> Bool {
        let raw = fixture as NSString
        let rawAt = raw.range(of: probe).location + offset
        let edited = raw.replacingCharacters(in: NSRange(location: rawAt, length: removeLength), with: insert)
        let caretAfter = rawAt + (insert as NSString).length

        // The editor: styled for the caret before the edit, then edited.
        let fast = NSTextStorage(string: fixture)
        engine.applyStyle(to: fast, cursorLocation: rawAt, maxWidth: 640)
        let shift = (fast.string as NSString).range(of: probe).location - raw.range(of: probe).location
        let displayAt = rawAt + shift
        fast.replaceCharacters(in: NSRange(location: displayAt, length: removeLength), with: insert)
        let caret = displayAt + (insert as NSString).length
        let ns = fast.string as NSString
        var lines = [ns.lineRange(for: NSRange(location: caret, length: 0))]
        if insert == "\n" { lines.append(ns.lineRange(for: NSRange(location: caret - 1, length: 0))) }
        guard lines.allSatisfy({ WysiwygEngine.canStyleAlone(lineRange: $0, in: fast) }) else { return false }
        let styled = lines.map { line -> NSAttributedString in
            let text = ns.substring(with: line)
            let end = line.location + (text.trimmingCharacters(in: .newlines) as NSString).length
            let cursor = caret >= line.location && caret <= end ? caret - line.location : -1
            return engine.styledLine(text, cursorInLine: cursor, maxWidth: 640)
        }
        for (line, attributed) in zip(lines, styled) {
            attributed.enumerateAttributes(in: NSRange(location: 0, length: attributed.length)) { attributes, range, _ in
                fast.setAttributes(attributes, range: NSRange(location: line.location + range.location, length: range.length))
            }
        }

        // The oracle: a full pass over the edited text.
        let full = NSTextStorage(string: edited)
        engine.applyStyle(to: full, cursorLocation: caretAfter, maxWidth: 640)
        checks += 1
        guard fast.string == full.string else {
            failures += 1
            print("FAIL: edit at \(probe): text differs")
            return true
        }
        compare(full, NSRange(location: 0, length: full.length), fast, "edit \"\(insert)\" at \(probe)")
        return true
    }

    static func compare(_ full: NSAttributedString, _ lineRange: NSRange, _ alone: NSAttributedString, _ label: String) {
        checks += 1
        for i in 0..<lineRange.length {
            let a = full.attributes(at: lineRange.location + i, effectiveRange: nil)
            let b = alone.attributes(at: alone.length == full.length ? lineRange.location + i : i, effectiveRange: nil)
            if normalized(a) != normalized(b) {
                failures += 1
                print("FAIL: \(label) differs at offset \(i)\n  full:  \(normalized(a))\n  alone: \(normalized(b))")
                return
            }
        }
    }

    /// Attribute values as comparable text; drawing marks by what they draw.
    static func normalized(_ attributes: [NSAttributedString.Key: Any]) -> [String: String] {
        var result: [String: String] = [:]
        for (key, value) in attributes {
            switch value {
            case let marker as ListMarker: result[key.rawValue] = "\(marker.kind) \(marker.indent) \(marker.font)"
            case let attachment as NSTextAttachment:
                result[key.rawValue] = "\(type(of: attachment)) \((attachment as? MarkdownBackedAttachment)?.markdownSource ?? "")"
            case let object as NSObject: result[key.rawValue] = object.description
            default: result[key.rawValue] = "\(value)"
            }
        }
        return result
    }
}
