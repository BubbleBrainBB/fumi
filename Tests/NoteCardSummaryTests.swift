import Foundation

// Standalone test runner for NoteCardSummary. No Xcode test target exists, and
// NoteCardSummary has no SwiftUI dependency, so this compiles directly with the
// source file:
//
//     swiftc Fumi/Views/NoteList/NoteCardSummary.swift Tests/NoteCardSummaryTests.swift -o /tmp/summary-tests
//     /tmp/summary-tests
//
// Exits non-zero if any assertion fails.

@main
enum NoteCardSummaryTests {
    static var failures = 0
    static var checks = 0

    static func expect(_ actual: String?, _ expected: String?, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            let a = actual.map { "\"\($0)\"" } ?? "nil"
            let e = expected.map { "\"\($0)\"" } ?? "nil"
            FileHandle.standardError.write("FAIL: \(message) — expected \(e), got \(a)\n".data(using: .utf8)!)
        }
    }

    static func main() {
        // Plain content: first non-empty line.
        expect(NoteCardSummary.firstContentLine(of: "Hello\nWorld"), "Hello",
               "plain content returns first line")

        // The first line is no longer skipped (the title moved to the file name).
        expect(NoteCardSummary.firstContentLine(of: "First line\nSecond line"), "First line",
               "does not skip the first content line")

        // Leading blank lines are skipped.
        expect(NoteCardSummary.firstContentLine(of: "\n\n  \nReal start"), "Real start",
               "leading blank lines skipped")

        // YAML frontmatter block is skipped; preview shows real content.
        let fm = "---\ntitle: \"2026-03-08\"\ntags: [ai]\n---\n今天是 2026 年 3 月 8 日"
        expect(NoteCardSummary.firstContentLine(of: fm), "今天是 2026 年 3 月 8 日",
               "leading frontmatter block is skipped")

        // Frontmatter followed by blank lines before content.
        let fmBlank = "---\ntitle: x\n---\n\n\nBody after blanks"
        expect(NoteCardSummary.firstContentLine(of: fmBlank), "Body after blanks",
               "blank lines after frontmatter are skipped too")

        // A note that is ONLY frontmatter has no displayable content.
        expect(NoteCardSummary.firstContentLine(of: "---\ntitle: x\ntags: [a]\n---\n"), nil,
               "frontmatter-only note has no content line")

        // An unterminated leading `---` is treated as ordinary content, not
        // frontmatter (so we don't swallow the whole note).
        expect(NoteCardSummary.firstContentLine(of: "---\nstill content"), "---",
               "unterminated leading --- is treated as content")

        // A Markdown heading is shown as-is (we only changed frontmatter handling).
        expect(NoteCardSummary.firstContentLine(of: "# Heading\nbody"), "# Heading",
               "markdown heading shown as first line")

        // Empty / whitespace-only content yields nil.
        expect(NoteCardSummary.firstContentLine(of: ""), nil, "empty content -> nil")
        expect(NoteCardSummary.firstContentLine(of: "   \n\n"), nil, "blank-only content -> nil")

        if failures == 0 {
            print("OK — all \(checks) NoteCardSummary checks passed")
            exit(0)
        } else {
            FileHandle.standardError.write("\(failures)/\(checks) NoteCardSummary checks FAILED\n".data(using: .utf8)!)
            exit(1)
        }
    }
}
