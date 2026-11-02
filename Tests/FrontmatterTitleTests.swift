import Foundation

// Standalone test runner for the frontmatter parser and TitleExtractor. No
// Xcode test target exists; both are Foundation-only, so this compiles
// directly with the sources:
//
//     swiftc Fumi/Services/FrontmatterParser.swift Fumi/Services/TitleExtractor.swift Tests/FrontmatterTitleTests.swift -o /tmp/frontmatter-title-tests
//     /tmp/frontmatter-title-tests
//
// Exits non-zero if any assertion fails.

@main
enum FrontmatterTitleTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    static func main() {
        // ── Frontmatter ──
        let doc = """
        ---
        tags: [规则, "llm-wiki"]
        aliases:
          - CLAUDE
          - 'agent rules'
        status: draft
        # a comment
        ---
        # CLAUDE.md
        """
        let fm = Frontmatter.parse(doc as NSString)
        expect(fm?.fields.map(\.key), ["tags", "aliases", "status"], "keys in order, comments skipped")
        expect(fm?.fields.first?.value, .list(["规则", "llm-wiki"]), "flow list, quotes stripped")
        expect(fm?.fields[1].value, .list(["CLAUDE", "agent rules"]), "block list")
        expect(fm?.fields[2].value, .text("draft"), "scalar")
        let closeEnd = (doc as NSString).range(of: "---", options: .backwards)
        expect(fm?.range, NSRange(location: 0, length: NSMaxRange(closeEnd)), "range ends at the closing fence, newline excluded")

        expect(Frontmatter.parse("# Title\n---\nnot: frontmatter\n---" as NSString) == nil, true, "must start the document")
        expect(Frontmatter.parse("---\ntags: [a]\n" as NSString) == nil, true, "unclosed block is not frontmatter")
        expect(Frontmatter.parse("---\nempty:\n---" as NSString)?.fields, [Frontmatter.Field(key: "empty", value: .text(""))], "key with no value")

        // ── Titles ──
        expect(TitleExtractor.extract(from: "# Hello\n\nBody"), "Hello", "H1 title")
        expect(TitleExtractor.extract(from: "---\ntags: [a]\n---\n# Real Title"), "Real Title", "frontmatter is skipped")
        expect(TitleExtractor.extract(from: "![](https://image-1325.cos/x.png)\n\nThe actual first line"),
               "The actual first line", "image-only line is not a title")
        expect(TitleExtractor.extract(from: "# ![](a.png)\nNext"), "Next", "heading made only of an image is skipped")
        expect(TitleExtractor.extract(from: "See ![chart](c.png) for **details**"), "See  for details", "inline image removed, markers stripped")
        expect(TitleExtractor.extract(from: "![](only.png)"), "", "nothing usable → empty, caller uses file name")
        expect(TitleExtractor.extract(from: ""), "", "empty note")

        if failures == 0 {
            print("All \(checks) Frontmatter/Title checks passed.")
        } else {
            print("\(failures) of \(checks) Frontmatter/Title checks failed.")
            exit(1)
        }
    }
}
