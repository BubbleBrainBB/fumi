import Foundation

// Standalone test runner for LinkRewrite. No Xcode test target exists; it is
// Foundation-only, so this compiles directly with the source files:
//
//     swiftc Fumi/Services/InlineLink.swift Fumi/Services/LibraryMove.swift Fumi/Services/LinkRewrite.swift Tests/LinkRewriteTests.swift -o /tmp/linkrewrite-tests
//     /tmp/linkrewrite-tests
//
// Exits non-zero if any assertion fails.

@main
enum LinkRewriteTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    /// The library before any move.
    static let files: Set<String> = [
        "/ws/attachments/a.png",
        "/ws/attachments/b c.png",
        "/ws/attachments/clip.mov",
        "/ws/doc.pdf",
        "/ws/Notes.md",
        "/ws/notes/x.png",
        "/ws/Travel/Kyoto.md",
        "/ws/Travel/cover.jpg",
        "/ws/Travel/my pic.png",
        "/ws/Travel/a#1.png",
        "/ws/Travel/截图 (1).png",
        "/ws/attachments/视频生成节点-1 (1).mp4",
        "/ws/Travel/attachments/local.png",
        "/ws/Travel/notes/x.png",
    ]

    /// `content` of the note at `note` after `from` moved to `to`, with the
    /// library's files moved too.
    static func rewrite(_ content: String, note: String = "/ws/Travel/Kyoto.md",
                        from: String = "/ws/Travel/Kyoto.md", to: String = "/ws/Kyoto.md") -> String {
        let after = Set(files.map { LibraryMove.remap($0, from: from, to: to) ?? $0 })
        return LinkRewrite.rewrite(content, noteAt: note, movedFrom: from, to: to, library: "/ws",
                                   exists: { after.contains($0) })
    }

    static func main() {
        // ── Paths ──
        expect(LinkRewrite.relativePath(from: "/ws/Travel", to: "/ws/attachments/a.png"), "../attachments/a.png",
               "up and over")
        expect(LinkRewrite.relativePath(from: "/ws", to: "/ws/Travel/cover.jpg"), "Travel/cover.jpg", "down")
        expect(LinkRewrite.relativePath(from: "/ws/Travel", to: "/ws/Travel/cover.jpg"), "cover.jpg", "same folder")
        expect(LinkRewrite.join("/ws/Travel", "../attachments/./a.png"), "/ws/attachments/a.png", "dots worked out")

        // ── A note moved up to the root ──
        expect(rewrite("![](../attachments/a.png)"), "![](attachments/a.png)", "pasted image from a subfolder")
        expect(rewrite("![](cover.jpg)"), "![](Travel/cover.jpg)", "image beside the note stays behind")
        expect(rewrite("![](attachments/local.png)"), "![](Travel/attachments/local.png)",
               "note-relative attachments folder")
        expect(rewrite("![](attachments/a.png)"), "![](attachments/a.png)",
               "library-relative link still resolves")
        expect(rewrite("![](notes/x.png)"), "![](Travel/notes/x.png)",
               "same path at the new folder is another file")
        expect(rewrite("![](../attachments/b%20c.png)"), "![](attachments/b%20c.png)", "encoded space kept encoded")
        expect(rewrite("![](<../attachments/b c.png>)"), "![](<attachments/b c.png>)", "angle brackets kept")
        expect(rewrite("![](my pic.png)"), "![](Travel/my%20pic.png)", "unencoded space")
        expect(rewrite("![](a#1.png)"), "![](Travel/a%231.png)", "an image's # is part of its name")
        expect(rewrite("![Cover](cover.jpg \"The cover\")"), "![Cover](Travel/cover.jpg \"The cover\")",
               "alt and title kept")
        expect(rewrite("![](../attachments/clip.mov)"), "![](attachments/clip.mov)", "video")
        expect(rewrite("![](截图 (1).png)"), "![](Travel/截图%20%281%29.png)", "parentheses in the name")
        expect(rewrite("![](../attachments/视频生成节点-1 (1).mp4 \"Clip\")"),
               "![](attachments/视频生成节点-1%20%281%29.mp4 \"Clip\")", "video with parentheses and a title")
        expect(rewrite("![](<截图 (1).png>)"), "![](<Travel/截图 (1).png>)", "parentheses inside <…>")
        expect(rewrite("![](cover.jpg) ![](截图 (1).png)"), "![](Travel/cover.jpg) ![](Travel/截图%20%281%29.png)",
               "two images on one line")
        expect(rewrite("[Report](../doc.pdf)"), "[Report](doc.pdf)", "PDF link")
        expect(rewrite("See [notes](../Notes.md#todo)."), "See [notes](Notes.md#todo).", "link keeps its #section")
        expect(rewrite("[top](Kyoto.md#start)"), "[top](Kyoto.md#start)", "link to itself still resolves")
        expect(rewrite("![[../attachments/a.png|300]]"), "![[attachments/a.png|300]]", "embed with a folder")
        expect(rewrite("![[cover.jpg]]"), "![[cover.jpg]]", "bare-name embed is found by name")

        // ── Left as written ──
        let untouched = [
            "![](https://example.com/a.png)",
            "[Site](https://example.com/page)",
            "[Mail](mailto:me@example.com)",
            "![](file:///Users/me/a.png)",
            "![](/Users/me/Pictures/a.png)",
            "![](~/Pictures/a.png)",
            "[Jump](#heading)",
            "![](missing.png)",
            "[[Notes]]",
            "No links here.",
        ]
        for text in untouched {
            expect(rewrite(text), text, "untouched: \(text)")
        }
        expect(rewrite("Use `![](cover.jpg)` to embed."), "Use `![](cover.jpg)` to embed.", "inline code")
        let fenced = "```md\n![](cover.jpg)\n```\n![](cover.jpg)"
        expect(rewrite(fenced), "```md\n![](cover.jpg)\n```\n![](Travel/cover.jpg)", "fenced code")

        // ── A whole document ──
        let doc = "# Kyoto\n\n![](../attachments/a.png)\n\nText with [a link](https://kyoto.jp) and ![](cover.jpg).\n"
        expect(rewrite(doc), "# Kyoto\n\n![](attachments/a.png)\n\nText with [a link](https://kyoto.jp) and ![](Travel/cover.jpg).\n",
               "several links in one note")

        // ── Other moves ──
        expect(rewrite("![](attachments/a.png) ![](doc.pdf)", note: "/ws/Kyoto.md",
                       from: "/ws/Kyoto.md", to: "/ws/Archive/2025/Kyoto.md"),
               "![](attachments/a.png) ![](doc.pdf)", "moved deeper: library-relative still resolves")
        expect(rewrite("![](../attachments/a.png)", note: "/ws/Travel/Kyoto.md",
                       from: "/ws/Travel/Kyoto.md", to: "/ws/Archive/Kyoto.md"),
               "![](../attachments/a.png)", "same depth elsewhere still resolves")
        expect(rewrite("![](cover.jpg) ![](../attachments/a.png)", note: "/ws/Travel/Kyoto.md",
                       from: "/ws/Travel", to: "/ws/Archive/Travel"),
               "![](cover.jpg) ![](../../attachments/a.png)",
               "folder move: what moved along stays, what's outside follows")
        expect(rewrite("![](cover.jpg)", note: "/ws/Notes.md"), "![](cover.jpg)", "a note the move doesn't touch")

        if failures == 0 {
            print("All \(checks) LinkRewrite checks passed.")
        } else {
            print("\(failures) of \(checks) LinkRewrite checks failed.")
            exit(1)
        }
    }
}
