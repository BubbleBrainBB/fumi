import Foundation

// Standalone test runner for MemoMarkdown:
//
//     swiftc Fumi/Services/MemoFormat.swift Fumi/Services/MemoMarkdown.swift Tests/MemoMarkdownTests.swift -o /tmp/memo-markdown-tests
//     /tmp/memo-markdown-tests
//
// Exits non-zero if any assertion fails.

@main
enum MemoMarkdownTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    /// The text of each span of `kind`, in reading order.
    static func texts(_ kind: MemoMarkdown.Span.Kind, in text: String) -> [String] {
        MemoMarkdown.spans(in: text).filter { $0.kind == kind }
            .sorted { $0.range.location < $1.range.location }
            .map { (text as NSString).substring(with: $0.range) }
    }

    static func main() {
        // ── Blocks ──
        let memo = """
        想到两件事：
        先记下来

        - 买咖啡豆
          - 浅烘
        1. 第一
        - [ ] 回邮件 #work
        - [x] 交周报
        > 引用一行
        > 第二行
        ```swift
        let a = 1

        print(a)
        ```
        收尾
        """
        let blocks = MemoMarkdown.blocks(memo)
        expect(blocks, [
            .paragraph("想到两件事：\n先记下来"),
            .item(level: 0, marker: .bullet, text: "买咖啡豆", line: 3),
            .item(level: 1, marker: .bullet, text: "浅烘", line: 4),
            .item(level: 0, marker: .ordered("1."), text: "第一", line: 5),
            .item(level: 0, marker: .task(checked: false), text: "回邮件 #work", line: 6),
            .item(level: 0, marker: .task(checked: true), text: "交周报", line: 7),
            .quote("引用一行\n第二行"),
            .code(language: "swift", text: "let a = 1\n\nprint(a)"),
            .paragraph("收尾"),
        ], "blocks of a mixed memo")

        expect(MemoMarkdown.blocks("plain\nlines"), [.paragraph("plain\nlines")], "plain memo is one paragraph")
        expect(MemoMarkdown.blocks("```\nopen"), [.code(language: "", text: "open")], "unclosed fence is still code")
        expect(MemoMarkdown.blocks("#tag not a heading"), [.paragraph("#tag not a heading")], "a tag line stays a paragraph")
        expect(MemoMarkdown.blocks("-not a list\n2*3"), [.paragraph("-not a list\n2*3")], "markers need a space")
        expect(MemoMarkdown.blocks("- [ ]"), [.item(level: 0, marker: .task(checked: false), text: "", line: 0)],
               "empty to-do")

        // ── Ticking to-dos ──
        let todo = "list\n- [ ] one\n- [X] two"
        expect(MemoMarkdown.togglingTask(line: 1, in: todo), "list\n- [x] one\n- [X] two", "tick")
        expect(MemoMarkdown.togglingTask(line: 2, in: todo), "list\n- [ ] one\n- [ ] two", "untick")
        expect(MemoMarkdown.togglingTask(line: 0, in: todo), todo, "not a to-do: unchanged")
        expect(MemoMarkdown.togglingTask(line: 9, in: todo), todo, "out of range: unchanged")

        // ── Syntax spans ──
        let inline = "a **bold** and *it* or _it2_, `co*de*` ~~gone~~ [site](https://x.y/a_b_c) #tag"
        expect(texts(.strong, in: inline), ["bold"], "strong")
        expect(texts(.emphasis, in: inline), ["it", "it2"], "emphasis, not inside code or the URL")
        expect(texts(.code, in: inline), ["co*de*"], "inline code")
        expect(texts(.strike, in: inline), ["gone"], "strike")
        expect(texts(.link, in: inline), ["site"], "link text")
        expect(texts(.tag, in: inline), ["#tag"], "tag")
        expect(texts(.marker, in: inline), ["**", "**", "*", "*", "_", "_", "`", "`", "~~", "~~", "[", "](https://x.y/a_b_c)"],
               "delimiters")

        expect(texts(.marker, in: "2 * 3 * 4 and snake_case_name"), [], "no stray emphasis")
        expect(texts(.strong, in: "**粗体**中文"), ["粗体"], "strong beside CJK")

        let lines = "- [x] done *now*\n> - quoted\n```\n**raw**\n```"
        expect(texts(.done, in: lines), ["done *now*"], "ticked to-do text")
        expect(texts(.emphasis, in: lines), ["now"], "inline inside a to-do")
        expect(texts(.quote, in: lines), ["quoted"], "quote text after its list marker")
        expect(texts(.codeBlock, in: lines), ["**raw**"], "fenced lines are code")
        expect(texts(.strong, in: lines), [], "no inline styling in a fence")
        expect(texts(.marker, in: lines), ["- [x] ", "*", "*", "> ", "- ", "```", "```"], "line markers and fences")

        if failures == 0 {
            print("All \(checks) MemoMarkdown checks passed.")
        } else {
            print("\(failures) of \(checks) MemoMarkdown checks failed.")
            exit(1)
        }
    }
}
