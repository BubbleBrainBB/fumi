import Foundation

// Standalone test runner for WikiLink and WikiLinkIndex (links between
// notes, design board 58). Foundation-only, so it compiles on its own:
//
//   swiftc Fumi/Services/WikiLink.swift Fumi/Models/FileNote.swift Tests/WikiLinkTests.swift -o /tmp/wikilink-tests && /tmp/wikilink-tests

@main
enum WikiLinkTests {
    static var failures = 0

    static func check(_ condition: Bool, _ message: String, line: Int = #line) {
        if !condition {
            failures += 1
            print("FAIL (line \(line)): \(message)")
        }
    }

    static func main() {
        // MARK: Parsing
        let plain = WikiLink("智谱 GLM-4.5v")
        check(plain?.target == "智谱 GLM-4.5v" && plain?.heading == nil && plain?.alias == nil, "plain target")
        let full = WikiLink("folder/Note.md#效果有多好？|上次那篇")
        check(full?.target == "folder/Note" && full?.heading == "效果有多好？" && full?.alias == "上次那篇",
              "path, .md, heading and alias: \(String(describing: full))")
        check(WikiLink("  ") == nil && WikiLink("#Heading") == nil, "nothing to link to")

        // MARK: What a link shows
        check(plain?.displayText(title: "GLM 那篇") == "GLM 那篇", "the note's title")
        check(full?.displayText(title: "Note") == "上次那篇", "the alias wins")
        check(WikiLink("Note#Part")?.displayText(title: "A Note") == "A Note › Part", "title › heading")
        check(WikiLink("a/b/Missing")?.displayText(title: nil) == "Missing", "a missing note shows its name")

        // MARK: Titles
        check(WikiLinkIndex.title(of: "---\ntitle: \"2025年7月4日\"\ndate: x\n---\n# Other\n", fileName: "f") == "2025年7月4日",
              "front matter title first")
        check(WikiLinkIndex.title(of: "---\ntags: [a]\n---\n\n# 标题\n正文", fileName: "f") == "标题", "then a first-line heading")
        check(WikiLinkIndex.title(of: "正文第一行\n\n# 中间的标题\n", fileName: "文件名") == "文件名",
              "a heading further down isn't the title")
        check(WikiLinkIndex.title(of: "", fileName: "文件名") == "文件名", "else the file name")

        // MARK: Resolving
        let root = URL(fileURLWithPath: "/lib")
        let index = WikiLinkIndex()
        index.update(notes: [
            ("/lib/a/Note.md", URL(fileURLWithPath: "/lib/a/Note.md"), "# In A"),
            ("/lib/b/c/Note.md", URL(fileURLWithPath: "/lib/b/c/Note.md"), "# In C"),
            ("/lib/Top.md", URL(fileURLWithPath: "/lib/Top.md"), "text"),
        ], library: root)
        check(index.resolve("top", from: nil)?.title == "Top", "by name, any case")
        check(index.resolve("Note", from: URL(fileURLWithPath: "/lib/b/c"))?.title == "In C", "same folder first")
        check(index.resolve("Note", from: URL(fileURLWithPath: "/lib/x"))?.title == "In A", "else nearest the top")
        check(index.resolve("b/c/Note", from: nil)?.title == "In C", "by library path")
        check(index.resolve("Gone", from: nil) == nil, "missing")

        let revision = index.revision
        index.update(notes: [
            ("/lib/Top.md", URL(fileURLWithPath: "/lib/Top.md"), "text"),
        ], library: root)
        index.update(notes: [
            ("/lib/Top.md", URL(fileURLWithPath: "/lib/Top.md"), "text"),
        ], library: root)
        check(index.revision == revision + 1, "an unchanged library isn't a change")

        // MARK: What a save changes for links
        func note(_ path: String, _ content: String) -> FileNote {
            FileNote(id: path, url: URL(fileURLWithPath: path), title: "", content: content, tags: [],
                     createdAt: Date(), updatedAt: Date(), pinned: false, relativePath: "/")
        }
        let titled = note("/lib/A.md", "# Title\nbody")
        check(WikiLinkIndex.showsSame(titled, note("/lib/A.md", "# Title\nbody and more")), "typing in the body")
        check(!WikiLinkIndex.showsSame(titled, note("/lib/A.md", "# New title\nbody")), "a new heading")
        check(!WikiLinkIndex.showsSame(titled, note("/lib/A.md", "---\ntitle: T\n---\n# Title\nbody")), "a front matter title")
        check(!WikiLinkIndex.showsSame(titled, note("/lib/A.md", "body")), "no title left: the file name shows")
        check(!WikiLinkIndex.showsSame(titled, note("/lib/B.md", "# Title\nbody")), "moved or renamed")

        // MARK: Keeping what a rescan found unchanged
        let saved = note("/lib/A.md", "# 标题\n正文")
        let other = note("/lib/B.md", "正文")
        var rescannedA = note("/lib/A.md", "# 标题\n" + "正文")
        rescannedA.createdAt = saved.createdAt; rescannedA.updatedAt = saved.updatedAt; rescannedA.title = saved.title
        let edited = note("/lib/B.md", "外部改过")
        let added = note("/lib/C.md", "新的")
        let merged = FileNote.keepingUnchanged([saved, other], in: [added, edited, rescannedA])
        check(merged.map(\.id) == [added.id, edited.id, saved.id], "in the scan's order, without what's gone")
        check(merged[0] == added && merged[1] == edited, "what's new or changed comes from the scan")
        check(merged[2] == saved, "what's unchanged stays")
        check(FileNote.keepingUnchanged([saved], in: [saved]) == [saved], "nothing changed, nothing to do")
        let touched = note("/lib/A.md", saved.content)
        check(FileNote.keepingUnchanged([saved], in: [touched])[0].updatedAt == touched.updatedAt, "a new date still lands")
        let nfd = note("/lib/D.md", "e\u{301}")
        check(FileNote.keepingUnchanged([note("/lib/D.md", "\u{e9}")], in: [nfd])[0].content.utf8.count == 3,
              "the same text in another normalization is a change on disk")

        if failures == 0 {
            print("All WikiLink tests passed")
        } else {
            print("\(failures) failure(s)")
            exit(1)
        }
    }
}
