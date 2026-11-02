import Foundation

// Standalone test runner for OutlineSectionPreview. Compile and run:
//
//     swiftc Fumi/Views/Editor/OutlineSectionPreview.swift Tests/OutlineSectionPreviewTests.swift -o /tmp/outline-preview-tests
//     /tmp/outline-preview-tests
//
// Exits non-zero if any assertion fails.

@main
enum OutlineSectionPreviewTests {
    static var failures = 0
    static var checks = 0

    static func expect(_ actual: String, _ expected: String, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message) — expected \"\(expected)\", got \"\(actual)\"\n".data(using: .utf8)!)
        }
    }

    static func main() {
        expect(OutlineSectionPreview.text(of: "\nFirst line.\n\nSecond line.\n"),
               "First line. Second line.", "joins paragraphs, skips blank lines")
        expect(OutlineSectionPreview.text(of: "- one\n* two\n3. three\n- [x] done"),
               "one two three done", "drops list and task markers")
        expect(OutlineSectionPreview.text(of: "> quoted\n> [!note] Callout title"),
               "quoted Callout title", "drops quote and callout markers")
        expect(OutlineSectionPreview.text(of: "A **bold** and `code` with [link](https://x.y) and [[Page|alias]]"),
               "A bold and code with link and alias", "strips inline syntax")
        expect(OutlineSectionPreview.text(of: "![img](a.png)\nAfter image"),
               "After image", "drops image-only lines")
        expect(OutlineSectionPreview.text(of: "Intro\n```swift\nlet x = 1\n```\n---\n| a | b |\nOutro"),
               "Intro Outro", "skips code blocks, rules and table rows")
        expect(OutlineSectionPreview.text(of: "snake_case_name stays"),
               "snake_case_name stays", "keeps single underscores")
        expect(OutlineSectionPreview.text(of: String(repeating: "word ", count: 100), limit: 20).count == 20 ? "ok" : "long",
               "ok", "truncates to the limit")
        expect(OutlineSectionPreview.text(of: ""), "", "empty body")

        print("\(checks - failures)/\(checks) checks passed")
        exit(failures == 0 ? 0 : 1)
    }
}
