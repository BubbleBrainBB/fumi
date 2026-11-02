import Foundation

// Standalone test runner for XhsDeck: how a note becomes the Xiaohongshu
// card layout's blocks — sentences, image leads, title, caption, warnings.
//
//     swiftc Fumi/Services/XhsDeck.swift Fumi/Services/FrontmatterParser.swift Fumi/Services/TagExtractor.swift Tests/XhsDeckTests.swift -o /tmp/xhs-tests
//     /tmp/xhs-tests
//
// Exits non-zero if any assertion fails.

@main
enum XhsDeckTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    static func deck(_ markdown: String, file: String = "Note.md") -> XhsDeck {
        XhsDeck.make(markdown: markdown, notePath: "/tmp/notes/\(file)", fileName: file)
    }

    static func main() {
        // Sentences break only where nothing is open.
        expect(XhsDeck.sentences("一句。两句！三句？"), ["一句。", "两句！", "三句？"], "three sentences")
        expect(XhsDeck.sentences("**加粗。还在加粗**，结束。"), ["**加粗。还在加粗**，结束。"], "no break inside bold")
        expect(XhsDeck.sentences("他说：“好。”然后走了。"), ["他说：“好。”", "然后走了。"], "closing quote stays")
        expect(XhsDeck.sentences("用 `a.b()。` 调用。好。"), ["用 `a.b()。` 调用。", "好。"], "no break inside code")
        expect(XhsDeck.sentences("*斜体。*后面。"), ["*斜体。*", "后面。"], "italic closes with its sentence")

        // Inline Markdown.
        expect(XhsDeck.inline("等到了**Kimi K3.1 Pro**。"), "等到了<b>Kimi K3.1 Pro</b>。", "bold")
        expect(XhsDeck.inline("在<u>幻觉测试</u>上"), "在<u>幻觉测试</u>上", "underline kept")
        expect(XhsDeck.inline("a <script>x</script> b"), "a x b", "other HTML dropped")
        expect(XhsDeck.inline("看 [这里](https://x.y) 和 `**不加粗**`"),
               "看 <span class=\"link\">这里</span> 和 <code>**不加粗**</code>", "links and code")

        // The title comes from the leading heading, which leaves the cards:
        // they start with the note's first words, short or not.
        let titled = deck("# 标题\n\n第一段很短。\n\n第二段。")
        expect(titled.title, "标题", "title from H1")
        expect(titled.blocks.map(\.kind), [.paragraph, .paragraph], "H1 leaves the blocks; the first paragraph stays")

        // The title falls back to the file name; front matter wins over both.
        expect(deck("正文。", file: "刚刚.md").title, "刚刚", "title from file name")
        let fm = deck("---\ntitle: 前言\nxhs_title: 短标题\ntags: [AI/新模型]\n---\n\n# 不是这个\n\n正文。")
        expect(fm.title, "短标题", "xhs_title for the post")
        expect(deck("---\ntitle: 前言\n---\n\n正文。").title, "前言", "front matter title")
        expect(fm.caption.hasSuffix("#新模型"), true, "tags end the caption as topics")

        // An image takes the colon sentence before it, and its alt as caption.
        let figure = deck("开头很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的一段。\n\n基于 K3 做的后训练。下面是官方放出的 Benchmark：\n\n![官方 Benchmark](https://e.com/a.png)\n## 代码\n必须要提一下。")
        expect(figure.blocks.map(\.kind), [.paragraph, .paragraph, .figure, .heading, .paragraph], "blocks")
        expect(figure.blocks[1].sentences, ["基于 K3 做的后训练。"], "colon sentence left the paragraph")
        expect(figure.blocks[2].lead, "下面是官方放出的 Benchmark：", "lead")
        expect(figure.blocks[2].caption, "官方 Benchmark", "caption")
        expect(figure.blocks[2].src, "https://e.com/a.png", "web image as is")

        // A one-sentence bold paragraph before an image is its lead too.
        let bold = deck("前面一段很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的话。\n\n**除了性能之外，还有一件事儿是成本。**\n\n![](https://e.com/b.png)")
        expect(bold.blocks.map(\.kind), [.paragraph, .figure], "bold lead joined the figure")
        expect(bold.blocks[1].lead, "<b>除了性能之外，还有一件事儿是成本。</b>", "bold lead")

        // Videos and missing files leave the cards, with a warning.
        let media = deck("长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n![](attachments/a.mp4)\n\n![](attachments/b.mp4)\n\n![](missing.png)")
        expect(media.blocks.map(\.kind), [.paragraph], "media left out")
        expect(media.warnings, ["2 videos can't go on cards; add them in Xiaohongshu.", "Image not found: missing.png"], "warnings")

        // Rules are page breaks; lists, quotes, code and tables are blocks.
        let mixed = deck("长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n---\n\n- 一\n- 二\n\n> 金句。\n\n```js\nx()\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |")
        expect(mixed.blocks.map(\.kind), [.paragraph, .pageBreak, .list, .quote, .code, .table], "kinds")
        expect(mixed.blocks[2].items, ["一", "二"], "list items")

        // Lines of a Chinese paragraph join without a space.
        expect(deck("长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n第一行\n第二行").blocks.last?.sentences,
               ["第一行第二行"], "CJK lines join")

        // Leave Out of Cards hides a block by its id, which survives other edits.
        let full = deck("长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n别放这段。")
        let hidden = XhsDeck.make(markdown: "改了开头长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n别放这段。",
                                  notePath: "/tmp/notes/Note.md", fileName: "Note.md",
                                  leftOut: [full.blocks[1].id])
        expect(hidden.blocks.count, 1, "left out")
        expect(hidden.leftOutCount, 1, "left out count")

        let kimi = deck("# 刚刚，月之暗面发布Kimi K3.1 Pro。\n\n等啊等，终于等到了**Kimi K3.1 Pro**。\n\n基于**K3**做的。")
        expect(kimi.caption, "等啊等，终于等到了Kimi K3.1 Pro。\n\n基于K3做的。", "caption")

        // The caption is the whole note: headings, paragraphs, lists, quotes; no images or code.
        let whole = deck("# 标题\n\n很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的第一段。\n\n![](https://e.com/a.png)\n\n## 代码\n\n- 一\n- 二\n\n> 金句。\n\n```\nx()\n```\n\n最后一段。")
        expect(whole.caption, "很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的第一段。\n\n代码\n\n• 一\n• 二\n\n金句。\n\n最后一段。", "whole note caption")

        // The caption is never cut short, however long the opening is.
        let long = String(repeating: "很长的开头", count: 300) + "。"
        expect(deck("# 标题\n\n\(long)").caption, long, "long caption kept whole")

        // The real note, if it is on this Mac.
        let real = NSString(string: "~/Documents/My Personal Digital Garden/刚刚，月之暗面发布Kimi K3.1 Pro。.md").expandingTildeInPath
        if let text = try? String(contentsOfFile: real, encoding: .utf8) {
            let deck = XhsDeck.make(markdown: text, notePath: real, fileName: (real as NSString).lastPathComponent)
            print("Kimi note: \(deck.blocks.count) blocks, warnings: \(deck.warnings)")
            expect(deck.blocks.first?.sentences, ["等啊等，终于等到了<b>Kimi K3.1 Pro</b>。"], "real note starts with its first words")
        }

        print(failures == 0 ? "All \(checks) XhsDeck checks passed." : "\(failures) of \(checks) checks failed.")
        exit(failures == 0 ? 0 : 1)
    }
}
