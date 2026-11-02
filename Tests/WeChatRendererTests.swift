import Foundation

// Standalone test runner for WeChatRenderer: the bundled renderer, run in
// JavaScriptCore, must write exactly what its source's own
// `node Tools/wechat-renderer/src/scripts/render.mjs` writes, in every
// theme — and, its one addition, accept bold beside Chinese punctuation.
//
//     swiftc Fumi/Services/WeChatRenderer.swift Tests/WeChatRendererTests.swift -o /tmp/wechat-tests
//     /tmp/wechat-tests
//
// Needs node; without it the comparison is skipped.
// Exits non-zero if any assertion fails.

@main
enum WeChatRendererTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(String(describing: expected).prefix(300))\n  actual:   \(String(describing: actual).prefix(300))")
        }
    }

    static func run(_ arguments: [String]) -> Int32 {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice
        try? process.run()
        process.waitUntilExit()
        return process.terminationStatus
    }

    static func main() throws {
        let repo = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        let renderer = try WeChatRenderer(folder: repo.appendingPathComponent("Fumi/Resources/WeChat"))

        // ── Basics ──
        let article = try renderer.render(markdown: "# Title\n\nHello **world**.\n\n## Part\n\n> quote",
                                          notePath: "/tmp/note.md", theme: .blackRed)
        expect(article.title, "Title", "leading H1 becomes the title")
        expect(article.html.contains("<h1"), false, "the title is not in the body")
        expect(article.html.contains("color:#8f1d22;font-weight:700;\">world</strong>"), true, "bold in the theme's red")
        expect(article.html.contains("class="), false, "no classes: WeChat drops them")
        expect(try renderer.render(markdown: "Body", notePath: "/tmp/My Note.md", theme: .blueprint).title,
               "My Note", "no H1: the file name")
        do {
            _ = try renderer.render(markdown: "![x](missing.png)", notePath: "/tmp/note.md", theme: .blackRed)
            expect(false, true, "a missing local image is an error")
        } catch {
            expect(error.localizedDescription.contains("图片文件不存在"), true, "the renderer's own message")
        }

        // ── Bold beside Chinese punctuation (markdown-it-cjk-friendly) ──
        for markdown in ["这是**“重点”**内容", "这是**重点。**然后", "**最后统一决定。**", "这是**重点**然后", "（**括号里**）"] {
            let html = try renderer.render(markdown: markdown, notePath: "/tmp/note.md", theme: .blueprint).html
            expect(html.contains("<strong"), true, "bold: \(markdown)")
        }
        let spaced = try renderer.render(markdown: "**最后统一决定。 **", notePath: "/tmp/note.md", theme: .blueprint).html
        expect(spaced.contains("<strong"), false, "a space before the closing ** is still not bold")
        expect(try renderer.render(markdown: "2 * 3 * 4", notePath: "/tmp/note.md", theme: .blueprint).html.contains("<em"),
               false, "spaced asterisks stay literal")

        // ── A video or image alone on its line, text right under it ──
        expect(WeChatRenderer.separatingEmbeds("Intro:\n![](a.mp4)\nNext line"), "Intro:\n\n![](a.mp4)\n\nNext line",
               "blank lines around a lone embed")
        expect(WeChatRenderer.separatingEmbeds("![](a.mp4)\n\nNext"), "![](a.mp4)\n\nNext", "already apart: unchanged")
        expect(WeChatRenderer.separatingEmbeds("Text ![](a.png) inline\nmore"), "Text ![](a.png) inline\nmore",
               "an image inside a sentence stays inline")
        expect(WeChatRenderer.separatingEmbeds("```\n![](a.mp4)\nx\n```"), "```\n![](a.mp4)\nx\n```", "code stays as is")
        expect(WeChatRenderer.separatingEmbeds("- item\n  ![](a.png)\n  more"), "- item\n  ![](a.png)\n  more",
               "inside a list stays as is")
        let video = try renderer.render(markdown: "还有一个：\n![](https://example.com/turbofan.mp4)\n还有一个是台灯：",
                                        notePath: "/tmp/note.md", theme: .blueprint)
        expect(video.html.contains("在此插入视频"), true, "the video becomes its placeholder card")
        expect(video.html.contains("还有一个是台灯"), true, "the next line stays")

        // ── Obsidian [[links]] and ![[embeds]] ──
        let found = ["10月14日.mp4": "/tmp/notes/attachments/10月14日.mp4", "a b.png": "/tmp/notes/a b.png"]
        func wiki(_ markdown: String) -> String { WeChatRenderer.resolvingWikiLinks(markdown) { found[$0] } }
        expect(wiki("效果是这样：\n\n![[10月14日.mp4]]"), "效果是这样：\n\n![](</tmp/notes/attachments/10月14日.mp4>)", "video embed resolves")
        expect(wiki("![[a b.png|300]]"), "![](</tmp/notes/a b.png>)", "image embed, size dropped")
        expect(wiki("![[missing.mp4]]"), "![](<missing.mp4>)", "unfound video keeps its name")
        expect(wiki("见 [[另一篇笔记]] 和 [[笔记|这里]]。"), "见 另一篇笔记 和 这里。", "links keep their words")
        expect(wiki("[[笔记#小节]]、[[#小节]]、![[笔记]]"), "笔记 › 小节、小节、笔记", "headings and note embeds")
        expect(wiki("`[[不动]]`\n```\n[[也不动]]\n```"), "`[[不动]]`\n```\n[[也不动]]\n```", "code stays as is")
        let embedded = try renderer.render(markdown: wiki("效果是这样：\n![[10月14日.mp4]]\n它会根据[[食材]]生成。"),
                                           notePath: "/tmp/notes/note.md", theme: .blueprint)
        expect(embedded.html.contains("在此插入视频"), true, "the embedded video becomes its placeholder card")
        expect(embedded.html.contains("[["), false, "no brackets left")
        expect(embedded.warnings.contains { $0.hasPrefix("Obsidian Wiki") }, false, "no wiki warning")

        // ── Same output as the source ──
        let source = repo.appendingPathComponent("Tools/wechat-renderer/src")
        let examples = ["assets/example.md", "assets/blueprint-example.md", "assets/songti-example.md",
                        "assets/motion-journal-example.md"]
        if FileManager.default.fileExists(atPath: source.appendingPathComponent("scripts/render.mjs").path) {
            let scratch = FileManager.default.temporaryDirectory.appendingPathComponent("wechat-tests-\(UUID().uuidString)")
            defer { try? FileManager.default.removeItem(at: scratch) }
            for theme in WeChatRenderer.Theme.allCases {
                for example in examples {
                    let input = source.appendingPathComponent(example)
                    let out = scratch.appendingPathComponent("\(theme.rawValue)-\(input.lastPathComponent)")
                    let status = run(["node", source.appendingPathComponent("scripts/render.mjs").path,
                                      "--input", input.path, "--out-dir", out.path, "--theme", theme.rawValue])
                    guard status == 0 else { expect(status, 0, "node render \(example) \(theme)"); continue }
                    let expected = try String(contentsOf: out.appendingPathComponent("article.html"), encoding: .utf8)
                    let title = try String(contentsOf: out.appendingPathComponent("title.txt"), encoding: .utf8)
                    let markdown = try String(contentsOf: input, encoding: .utf8)
                    let ours = try renderer.render(markdown: markdown, notePath: input.resolvingSymlinksInPath().path, theme: theme)
                    expect(ours.html, expected, "\(example) in \(theme.rawValue) matches render.mjs")
                    expect(ours.title + "\n", title, "\(example) title in \(theme.rawValue)")
                }
            }
        } else {
            print("Skipped the render.mjs comparison: no source at \(source.path)")
        }

        if failures == 0 {
            print("All \(checks) WeChatRenderer checks passed.")
        } else {
            print("\(failures) of \(checks) WeChatRenderer checks failed.")
            exit(1)
        }
    }
}
