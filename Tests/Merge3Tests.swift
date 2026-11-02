import Foundation

// Standalone test runner for Merge3. There is no Xcode test target in this
// project, and Merge3 is Foundation-only, so it compiles directly with its
// source file:
//
//     swiftc Fumi/Services/Merge3.swift Tests/Merge3Tests.swift -o /tmp/merge3-tests
//     /tmp/merge3-tests
//
// Exits non-zero if any assertion fails.

@main
enum Merge3Tests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message) — got \(actual), expected \(expected)")
        }
    }

    static func main() {
        testTrivialCases()
        testMergesDifferentParagraphs()
        testConflicts()
        testMapLocation()

        if failures == 0 {
            print("OK: all \(checks) checks passed")
        } else {
            print("\(failures)/\(checks) checks FAILED")
            exit(1)
        }
    }

    static func merge(_ base: String, _ mine: String, _ theirs: String) -> Merge3.Result {
        Merge3.merge(base: base, mine: mine, theirs: theirs)
    }

    // MARK: - merge

    static func testTrivialCases() {
        expect(merge("a", "a2", "a"), .init(text: "a2", conflicts: 0), "only mine changed → mine")
        expect(merge("a", "a", "a3"), .init(text: "a3", conflicts: 0), "only theirs changed → theirs")
        expect(merge("a", "same", "same"), .init(text: "same", conflicts: 0), "both made the same change → it")
    }

    static func testMergesDifferentParagraphs() {
        let base = "# W43\n\nDone\n\n## Next\n\n- find"
        // The agent edits the top while the user types at the bottom.
        let mine = "# W43\n\nDone\n\n## Next\n\n- find and replace"
        let theirs = "# W43\n\nDone, three templates\n\n## Next\n\n- find"
        expect(merge(base, mine, theirs),
               .init(text: "# W43\n\nDone, three templates\n\n## Next\n\n- find and replace", conflicts: 0),
               "edits in different paragraphs → both kept")

        // The agent adds a section; the user edits another line.
        let added = "# W43\n\nDone\n\n## Risks\n\nNo history yet\n\n## Next\n\n- find"
        expect(merge(base, mine, added),
               .init(text: "# W43\n\nDone\n\n## Risks\n\nNo history yet\n\n## Next\n\n- find and replace",
                     conflicts: 0),
               "a section added outside + a line edited here → both kept")

        // The agent removes lines the user didn't touch.
        let removed = "# W43\n\n## Next\n\n- find"
        expect(merge(base, mine, removed),
               .init(text: "# W43\n\n## Next\n\n- find and replace", conflicts: 0),
               "lines removed outside stay removed")

        // A trailing newline on both sides survives.
        expect(merge("a\n\nb\n", "a!\n\nb\n", "a\n\nb!\n"),
               .init(text: "a!\n\nb!\n", conflicts: 0), "trailing newline kept")

        // Chinese text, paragraph by paragraph.
        expect(merge("本周完成\n\n下周计划", "本周完成了三件事\n\n下周计划", "本周完成\n\n下周计划：查找替换"),
               .init(text: "本周完成了三件事\n\n下周计划：查找替换", conflicts: 0), "CJK paragraphs merge")
    }

    static func testConflicts() {
        expect(merge("a\n\nb", "a mine\n\nb", "a theirs\n\nb"),
               .init(text: "a mine\n\nb", conflicts: 1), "same paragraph changed both ways → mine, 1 conflict")

        expect(merge("a\n\nb\n\nc", "a1\n\nb\n\nc1", "a2\n\nb\n\nc2"),
               .init(text: "a1\n\nb\n\nc1", conflicts: 2), "two conflicting paragraphs counted apart")

        // Mine and theirs both insert different text at the same place.
        expect(merge("a\n\nb", "a\n\nmine\n\nb", "a\n\ntheirs\n\nb"),
               .init(text: "a\n\nmine\n\nb", conflicts: 1), "different inserts at one place → conflict")

        // A conflict next to a clean change: the clean one still comes through.
        expect(merge("a\n\nb\n\nc", "a1\n\nb\n\nc", "a2\n\nb\n\nc2"),
               .init(text: "a1\n\nb\n\nc2", conflicts: 1), "conflict doesn't block other changes")
    }

    // MARK: - mapLocation

    static func testMapLocation() {
        let old = "first\nsecond\nthird"
        // A line inserted above: the caret moves down with its line.
        let inserted = "zero\nfirst\nsecond\nthird"
        expect(Merge3.mapLocation(8, from: old, to: inserted), 13, "caret follows its line past an insert above")
        expect(Merge3.mapLocation(6, from: old, to: inserted), 11, "caret at a line start stays with that line")

        // A line changed below: the caret doesn't move.
        let changedBelow = "first\nsecond\nTHIRD!!"
        expect(Merge3.mapLocation(8, from: old, to: changedBelow), 8, "change below leaves the caret")

        // A line removed above.
        let removedAbove = "second\nthird"
        expect(Merge3.mapLocation(8, from: old, to: removedAbove), 2, "caret follows its line past a removal above")

        // The caret inside a changed line lands inside what replaced it.
        let changedHere = "first\nsec\nthird"
        expect(Merge3.mapLocation(10, from: old, to: changedHere), 9, "inside a shorter replacement → at its end")
        expect(Merge3.mapLocation(7, from: old, to: changedHere), 7, "inside a replacement → same distance in")

        // The end of the text.
        expect(Merge3.mapLocation(18, from: old, to: inserted), 23, "end stays at the end")
        expect(Merge3.mapLocation(0, from: "", to: "new"), 0, "empty old text")
    }
}
