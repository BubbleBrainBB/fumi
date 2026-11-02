import Foundation

// Standalone test runner for TabHistory (Back / Forward in a tab):
//
//     swiftc Fumi/App/NoteTabs.swift Tests/TabHistoryTests.swift -o /tmp/tab-history-tests
//     /tmp/tab-history-tests
//
// Exits non-zero if any assertion fails.

@main
enum TabHistoryTests {
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
        let a = NoteTab.Content.note("/a.md"), b = NoteTab.Content.note("/b.md"), c = NoteTab.Content.note("/c.md")

        // ── Visiting ──
        var history = TabHistory()
        history.visit(leaving: .draft)
        expect(history.back, [], "a blank draft isn't kept")
        history.visit(leaving: a)
        history.visit(leaving: .memos)
        expect(history.back, [a, .memos], "pages left, oldest first")

        // ── Back and forward ──
        expect(history.back(from: b), .memos, "back to Memos")
        expect(history.back(from: .memos), a, "back again")
        expect(history.back(from: a), nil, "nothing before the first page")
        expect(history.forward, [b, .memos], "both ahead")
        expect(history.forward(from: a), .memos, "forward")
        expect(history.forward(from: .memos), b, "forward again")
        expect(history.forward(from: b), nil, "nothing ahead")

        _ = history.back(from: b)
        history.visit(leaving: .memos)
        expect(history.forward, [], "a new page clears what was ahead")

        // ── No repeats ──
        var pingPong = TabHistory()
        for _ in 0..<5 { pingPong.visit(leaving: a); pingPong.visit(leaving: b) }
        expect(pingPong.back, [a, b], "going back and forth keeps each page once")

        var long = TabHistory()
        for i in 0..<80 { long.visit(leaving: .note("/\(i).md")) }
        expect(long.back.count, TabHistory.limit, "capped")
        expect(long.back.first, .note("/30.md"), "oldest dropped")

        // ── Renames and deletes ──
        var moved = TabHistory(back: [a, c], forward: [b])
        moved.remap { $0 == a ? .note("/x/a.md") : $0 }
        expect(moved.back, [.note("/x/a.md"), c], "renamed pages follow")
        moved.remove(b)
        expect(moved.forward, [], "deleted pages drop out")

        var fromDraft = TabHistory(back: [a])
        expect(fromDraft.back(from: .draft), a, "back from a blank draft")
        expect(fromDraft.forward, [], "the draft isn't kept ahead")

        if failures == 0 {
            print("All \(checks) TabHistory checks passed.")
        } else {
            print("\(failures) of \(checks) TabHistory checks failed.")
            exit(1)
        }
    }
}
