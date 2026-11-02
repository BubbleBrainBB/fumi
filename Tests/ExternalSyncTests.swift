import Foundation

// Standalone test runner for ExternalSync. There is no Xcode test target in this
// project, and ExternalSync has no AppKit dependency, so it compiles directly
// with its source file:
//
//     swiftc Fumi/Services/ExternalSync.swift Tests/ExternalSyncTests.swift -o /tmp/externalsync-tests
//     /tmp/externalsync-tests
//
// Exits non-zero if any assertion fails.

@main
enum ExternalSyncTests {
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
        testResolve()
        testHasExternalChange()
        testVisiblePaths()

        if failures == 0 {
            print("OK: all \(checks) checks passed")
        } else {
            print("\(failures)/\(checks) checks FAILED")
            exit(1)
        }
    }

    // MARK: - resolve(disk:displayed:lastSynced:)

    static func testResolve() {
        // Disk equals what's shown — our own save echo / already in sync.
        expect(ExternalSync.resolve(disk: "a", displayed: "a", lastSynced: "a"),
               .ignore, "in sync → ignore")
        expect(ExternalSync.resolve(disk: "a", displayed: "a", lastSynced: "old"),
               .ignore, "disk == displayed even with stale lastSynced → ignore")

        // Clean editor (displayed == lastSynced) but disk moved → adopt.
        expect(ExternalSync.resolve(disk: "new", displayed: "old", lastSynced: "old"),
               .adopt, "clean editor + external change → adopt")

        // Editor has unsaved local edits and the file changed outside → merge.
        expect(ExternalSync.resolve(disk: "external", displayed: "myedit", lastSynced: "old"),
               .merge, "local edits + external change → merge")

        // Disk still holds what the editor last saved (our own save's echo, the
        // user typing on since) → nothing from outside.
        expect(ExternalSync.resolve(disk: "saved", displayed: "saved and more", lastSynced: "saved"),
               .ignore, "own save echo while typing on → ignore")

        // Local edits that happen to match disk are caught by the first rule.
        expect(ExternalSync.resolve(disk: "same", displayed: "same", lastSynced: "old"),
               .ignore, "local edit converged with disk → ignore")
    }

    // MARK: - visiblePaths(_:in:)

    static func testVisiblePaths() {
        let lib = "/w/Notes"
        expect(ExternalSync.visiblePaths(["/w/Notes/.git/index", "/w/Notes/.obsidian/workspace.json", "/w/Notes/.DS_Store",
                                          "/w/Notes/sub/.hidden/a.md"], in: lib),
               [], "inside hidden folders, or hidden themselves → dropped")
        expect(ExternalSync.visiblePaths(["/w/Notes/a.md", "/w/Notes/sub/b.png", "/w/Notes/sub", "/w/Notes"], in: lib),
               ["/w/Notes/a.md", "/w/Notes/sub/b.png", "/w/Notes/sub", "/w/Notes"], "visible files and folders → kept")
        expect(ExternalSync.visiblePaths(["/w/.vaults/Notes/a.md"], in: "/w/.vaults/Notes"),
               ["/w/.vaults/Notes/a.md"], "a hidden folder above the library doesn't count")
        expect(ExternalSync.visiblePaths(["/private/w/.git/x"], in: "/w"),
               ["/private/w/.git/x"], "outside the library as we name it (a symlink) → kept, to be safe")
    }

    // MARK: - hasExternalChange(eventPaths:ownWrites:read:)

    static func testHasExternalChange() {
        let disk = ["/w/a.md": "mine", "/w/b.md": "theirs", "/w/a.markdown": "mine"]
        let read: (String) -> String? = { disk[$0] }

        // A note we just saved ourselves, still as we wrote it → not external.
        expect(ExternalSync.hasExternalChange(eventPaths: ["/w/a.md"], ownWrites: ["/w/a.md": "mine"], read: read),
               false, "only our own save → not external")

        // We saved it, but something wrote over it since → external.
        expect(ExternalSync.hasExternalChange(eventPaths: ["/w/b.md"], ownWrites: ["/w/b.md": "mine"], read: read),
               true, "written over right after our save → external")

        // A note changed that we did not write → external.
        expect(ExternalSync.hasExternalChange(eventPaths: ["/w/a.md"], ownWrites: [:], read: read),
               true, "unknown md change → external")

        // Mixed: one ours, one external → external (don't suppress).
        expect(ExternalSync.hasExternalChange(eventPaths: ["/w/a.md", "/w/b.md"], ownWrites: ["/w/a.md": "mine"],
                                              read: read),
               true, "one external among ours → external")

        // Our file deleted since → external.
        expect(ExternalSync.hasExternalChange(eventPaths: ["/w/gone.md"], ownWrites: ["/w/gone.md": "x"], read: read),
               true, "our file gone → external")

        // Non-note files only → conservatively treat as external (covers dir
        // events for create/delete/rename we should reflect).
        expect(ExternalSync.hasExternalChange(eventPaths: ["/w/.git/index"], ownWrites: [:], read: read),
               true, "no md paths → reload to be safe")

        // .markdown extension is recognized too.
        expect(ExternalSync.hasExternalChange(eventPaths: ["/w/a.markdown"], ownWrites: ["/w/a.markdown": "mine"],
                                              read: read),
               false, ".markdown self-save → not external")
    }
}
