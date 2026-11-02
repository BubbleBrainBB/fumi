import Foundation

// Standalone test runner for TagSearch. No Xcode test target exists; TagSearch
// and its TagNode/TagTreeBuilder deps are Foundation-only, so this compiles
// directly with the sources:
//
//     swiftc Fumi/Services/TagExtractor.swift Fumi/Views/Sidebar/TagSearch.swift Tests/TagSearchTests.swift -o /tmp/tagsearch-tests
//     /tmp/tagsearch-tests
//
// Exits non-zero if any assertion fails.

@main
enum TagSearchTests {
    static var failures = 0
    static var checks = 0

    static func expect(_ actual: [String], _ expected: [String], _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message)\n  expected \(expected)\n  got      \(actual)\n".data(using: .utf8)!)
        }
    }

    static func ids(_ tree: [TagNode], _ query: String) -> [String] {
        TagSearch.matches(in: tree, query: query).map(\.id)
    }

    static func main() {
        let tree = TagTreeBuilder.build(from: [
            "AI", "AI/编程", "AI/编程/Swift", "AI图像", "AI新闻",
            "Anthropic", "Bubble日报", "Claude",
        ])

        // Empty / whitespace query → no flat results (caller shows the tree).
        expect(ids(tree, ""), [], "empty query returns nothing")
        expect(ids(tree, "   "), [], "whitespace query returns nothing")

        // Substring match across the whole path, results sorted by full path.
        expect(ids(tree, "AI"), ["AI", "AI/编程", "AI/编程/Swift", "AI图像", "AI新闻"],
               "matches all paths containing AI, sorted")

        // A nested tag matches by its parent segment even when querying the parent.
        expect(ids(tree, "编程"), ["AI/编程", "AI/编程/Swift"],
               "nested matches found via path segment")

        // Deep leaf matches by its own name.
        expect(ids(tree, "Swift"), ["AI/编程/Swift"], "deep leaf matches by name")

        // Case-insensitive.
        expect(ids(tree, "claude"), ["Claude"], "case-insensitive match")
        expect(ids(tree, "anthropic"), ["Anthropic"], "case-insensitive match 2")

        // No match.
        expect(ids(tree, "zzz"), [], "no match returns empty")

        // Query is trimmed before matching.
        expect(ids(tree, "  Claude  "), ["Claude"], "query is trimmed")

        if failures == 0 {
            print("OK — all \(checks) TagSearch checks passed")
            exit(0)
        } else {
            FileHandle.standardError.write("\(failures)/\(checks) TagSearch checks FAILED\n".data(using: .utf8)!)
            exit(1)
        }
    }
}
