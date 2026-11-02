import AppKit

// Standalone layout test: blank lines and line positions must not depend on
// where the caret is, and ↩ must always move the text by one full line.
//
//     swiftc -o /tmp/blank-line-tests \
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
//       Tests/BlankLineLayoutTests.swift
//     /tmp/blank-line-tests
//
// Exits non-zero if any assertion fails.

@main
enum BlankLineLayoutTests {
    static var failures = 0
    static var checks = 0

    static func expect(_ condition: Bool, _ message: String) {
        checks += 1
        if !condition {
            failures += 1
            print("FAIL: \(message)")
        }
    }

    static let style = WysiwygEngine.makeStyle(fontSize: 16, lineHeight: 1.75, fontName: "New York")
    static let engine = WysiwygEngine(style: style)

    /// Styles `text` with the caret at `caret`, lays it out, and returns the
    /// top of every line fragment (one per paragraph here: lines are short).
    static func lineTops(_ text: String, caret: Int) -> [CGFloat] {
        let storage = NSTextStorage(string: text)
        engine.applyStyle(to: storage, cursorLocation: caret, maxWidth: 640)
        let layout = NSLayoutManager()
        let container = NSTextContainer(size: NSSize(width: 640, height: CGFloat.greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        layout.addTextContainer(container)
        storage.addLayoutManager(layout)
        layout.ensureLayout(for: container)
        var tops: [CGFloat] = []
        var glyph = 0
        while glyph < layout.numberOfGlyphs {
            var range = NSRange()
            let rect = layout.lineFragmentRect(forGlyphAt: glyph, effectiveRange: &range)
            tops.append(rect.minY)
            glyph = NSMaxRange(range)
        }
        if !layout.extraLineFragmentRect.isEmpty { tops.append(layout.extraLineFragmentRect.minY) }
        return tops
    }

    static func main() {
        let line = style.lineHeight
        // The note from the bug report: runs of blank lines between paragraphs.
        let doc = "第一段文字，\n\n\n\n因为需要\n\n\n\n\n"
        let length = (doc as NSString).length

        // ── Caret position never moves anything ──
        let reference = lineTops(doc, caret: 0)
        for caret in 0...length {
            let tops = lineTops(doc, caret: caret)
            expect(tops == reference, "layout with caret at \(caret) matches caret at 0: \(tops) vs \(reference)")
        }

        // ── Every blank line is one full body line ──
        let gaps = zip(reference.dropFirst(), reference).map { $0 - $1 }
        expect(gaps.allSatisfy { abs($0 - line) < 0.5 }, "every line advances by \(line): \(gaps)")

        // ── ↩ at the end of the text adds exactly one line ──
        var text = "因为需要"
        var previous = lineTops(text, caret: 4)
        for press in 1...4 {
            text += "\n"
            let caret = (text as NSString).length
            let tops = lineTops(text, caret: caret)
            expect(tops.count == previous.count + 1, "press \(press): one more line")
            expect(abs((tops.last ?? 0) - (previous.last ?? 0) - line) < 0.5,
                   "press \(press): caret line moves down by one line")
            previous = tops
        }

        // ── ↩ at the start of a line pushes it down one line each time ──
        var pushed = "第一段文字，\n因为需要"
        var start = 7 // start of 因为需要
        var before = lineTops(pushed, caret: start).last ?? 0
        for press in 1...3 {
            pushed = (pushed as NSString).replacingCharacters(in: NSRange(location: start, length: 0), with: "\n")
            start += 1
            let after = lineTops(pushed, caret: start).last ?? 0
            expect(abs(after - before - line) < 0.5, "line-start press \(press): text pushed down one line (\(after - before))")
            before = after
        }

        // ── No sub-pixel lines after a heading either ──
        let heading = "## 标题\n\n\n正文"
        let headingTops = lineTops(heading, caret: 0)
        let headingGaps = zip(headingTops.dropFirst(), headingTops).map { $0 - $1 }
        expect(headingGaps.allSatisfy { $0 >= line - 0.5 }, "no collapsed line under a heading: \(headingGaps)")
        expect(lineTops(heading, caret: 0) == lineTops(heading, caret: 7), "heading layout ignores the caret on a blank line")

        if failures == 0 {
            print("All \(checks) blank-line layout checks passed.")
        } else {
            print("\(failures) of \(checks) blank-line layout checks failed.")
            exit(1)
        }
    }
}
