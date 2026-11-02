import AppKit

// Standalone integration test for the composite-font pass inside the real
// WysiwygEngine pipeline (WysiwygEngine.applyStyle). Compiles the engine and
// its dependency graph together with this file:
//
//     swiftc -o /tmp/engine-composite-tests \
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
//       Tests/EngineCompositeFontTests.swift
//     /tmp/engine-composite-tests
//
// Exits non-zero if any assertion fails.
//
// Why this exists on top of CompositeFontTests: the unit tests pin the pass in
// isolation; this one proves the wiring inside applyStyle — that the pass runs
// AFTER every other font attribute (so bold, headings, hashtags and callouts
// all inherit the pairing), that it never disturbs inline code / code blocks /
// collapsed Markdown syntax, and that a "System" body leaves the document on
// the native cascade.

@main
enum EngineCompositeFontTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message) — expected \(expected), got \(actual)\n".data(using: .utf8)!)
        }
    }

    static func expect(_ condition: Bool, _ message: String) {
        checks += 1
        if !condition {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message)\n".data(using: .utf8)!)
        }
    }

    /// A document that exercises body, heading, bold, inline code, code block
    /// and list in one pass. The caret sits inside the body paragraph so no
    /// line is in "live preview" (which would reveal raw syntax markers).
    static let document = """
    # 标题 Heading

    正文中文 mixed English 混排测试文字。

    **粗体中文** bold text.

    `code 代码` inline.

    ```swift
    let x = 1 // 中文注释
    ```

    - 列表项 item one

    他说 “**重点**” 结束。
    """

    static func main() {
        testSerifBodyThroughEngine()
        testSystemBodyStaysNative()

        if failures == 0 {
            print("EngineCompositeFontTests: \(checks) checks passed")
        } else {
            print("EngineCompositeFontTests: \(failures)/\(checks) checks FAILED")
            exit(1)
        }
    }

    /// Runs one full restyle over `document` with the given body family and
    /// hands the styled storage to `assertions` (read before the engine's
    /// endEditing fires font fixing — the same raw view the pass itself has).
    static func restyle(fontName: String, _ assertions: (NSTextStorage) -> Void) {
        let style = WysiwygEngine.makeStyle(fontSize: 17, lineHeight: 1.75, fontName: fontName)
        let engine = WysiwygEngine(style: style)
        let storage = NSTextStorage(string: document)
        // Caret inside the body paragraph (line 3), far from any syntax.
        let caret = (document as NSString).range(of: "mixed").location + 2
        storage.beginEditing()
        engine.applyStyle(to: storage, cursorLocation: caret, maxWidth: 640)
        assertions(storage)
        storage.endEditing()
    }

    static func fontName(at index: Int, in storage: NSTextStorage) -> String {
        (storage.attribute(.font, at: index, effectiveRange: nil) as? NSFont)?.fontName ?? "<none>"
    }

    static func pointSize(at index: Int, in storage: NSTextStorage) -> CGFloat {
        (storage.attribute(.font, at: index, effectiveRange: nil) as? NSFont)?.pointSize ?? -1
    }

    /// Index of the first character of `needle` in the document.
    static func index(of needle: String) -> Int {
        (document as NSString).range(of: needle).location
    }

    // MARK: - Serif body

    static func testSerifBodyThroughEngine() {
        restyle(fontName: "New York") { storage in
            // Body: Han takes Songti SC (scaled), Latin keeps New York.
            let han = index(of: "正文中文")
            expect(fontName(at: han, in: storage), "STSongti-SC-Regular", "Body Han → Songti SC")
            expect(pointSize(at: han, in: storage), 17 * 0.97, "Body Han scaled size")
            let latin = index(of: "mixed")
            expect(fontName(at: latin, in: storage), ".NewYork-Regular", "Body Latin keeps New York")
            expect(pointSize(at: latin, in: storage), 17, "Body Latin size")

            // Heading: semibold New York for Latin, semibold Songti for Han.
            let headingHan = index(of: "标题")
            expect(fontName(at: headingHan, in: storage).hasPrefix("STSongti-SC"), true, "Heading Han → Songti SC")
            expect(pointSize(at: headingHan, in: storage), 30 * 0.97, "Heading Han at title size × scale")

            // Quotes around bold Han: the collapsed `**` between a quote and
            // its text is skipped, so both quotes join the CJK face.
            let openQuote = index(of: "“**重点")
            expect(fontName(at: openQuote, in: storage), "STSongti-SC-Regular", "Opening quote before hidden ** → Songti")
            let closeQuote = index(of: "**” 结束") + 2
            expect(fontName(at: closeQuote, in: storage), "STSongti-SC-Regular", "Closing quote after hidden ** → Songti")

            // Bold run: the pairing inherits the run's weight.
            let boldHan = index(of: "粗体中文")
            expect(fontName(at: boldHan, in: storage), "STSongti-SC-Bold", "Bold Han → Songti SC Bold")

            // Inline code: the mono run is left to its own cascade.
            let codeHan = index(of: "代码")
            expect(fontName(at: codeHan, in: storage), ".AppleSystemUIFontMonospaced-Regular", "Inline code Han untouched")
            expect(pointSize(at: codeHan, in: storage), (17 * 0.85).rounded(), "Inline code size untouched")

            // Fenced code block: same, mono all the way through.
            let blockHan = index(of: "中文注释")
            expect(fontName(at: blockHan, in: storage), ".AppleSystemUIFontMonospaced-Regular", "Code block Han untouched")

            // List item body: paired like ordinary body text.
            let listHan = index(of: "列表项")
            expect(fontName(at: listHan, in: storage), "STSongti-SC-Regular", "List Han → Songti SC")

            // Collapsed heading marker (`## ` hidden): still the 0.1pt font.
            let marker = index(of: "# 标题")
            expect(pointSize(at: marker, in: storage), 0.1, "Collapsed heading marker untouched")
        }
    }

    // MARK: - System body

    static func testSystemBodyStaysNative() {
        restyle(fontName: "System") { storage in
            // No composite: the native SF ↔ PingFang cascade stays in charge.
            let han = index(of: "正文中文")
            expect(fontName(at: han, in: storage), ".AppleSystemUIFont", "System body Han stays native")
            expect(pointSize(at: han, in: storage), 17, "System body Han size unchanged")
            let latin = index(of: "mixed")
            expect(fontName(at: latin, in: storage), ".AppleSystemUIFont", "System body Latin unchanged")
        }
    }
}
