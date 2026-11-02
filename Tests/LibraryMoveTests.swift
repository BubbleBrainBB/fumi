import Foundation

// Standalone test runner for LibraryMove. No Xcode test target exists; it is
// Foundation-only, so this compiles directly with the source file:
//
//     swiftc Fumi/Services/LibraryMove.swift Tests/LibraryMoveTests.swift -o /tmp/librarymove-tests
//     /tmp/librarymove-tests
//
// Exits non-zero if any assertion fails.

@main
enum LibraryMoveTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    static let library = URL(fileURLWithPath: "/ws", isDirectory: true)

    static func url(_ path: String) -> URL { URL(fileURLWithPath: "/ws" + path) }

    /// The destination's path, or the refusal, as one comparable value.
    static func move(_ item: String, into folderId: String, taken: Set<String> = []) -> String {
        let folder = LibraryMove.folderURL(forId: folderId, in: library)
        switch LibraryMove.destination(moving: url(item), into: folder, library: library,
                                       exists: { taken.contains($0.path) }) {
        case .success(let target): return target.path
        case .failure(let refusal): return "\(refusal)"
        }
    }

    static func main() {
        // ── Folder ids ↔ URLs ──
        expect(LibraryMove.folderURL(forId: "/", in: library).path, "/ws", "root id is the library")
        expect(LibraryMove.folderURL(forId: "/产品思考/草稿", in: library).path, "/ws/产品思考/草稿", "nested id")
        expect(LibraryMove.folderId(of: url(""), in: library), "/", "library is the root id")
        expect(LibraryMove.folderId(of: url("/Travel/Kyoto"), in: library), "/Travel/Kyoto", "nested folder id")
        expect(LibraryMove.folderId(of: URL(fileURLWithPath: "/ws/Travel/", isDirectory: true), in: library),
               "/Travel", "trailing slash ignored")
        expect(LibraryMove.folderId(of: URL(fileURLWithPath: "/wsx/Travel"), in: library), nil,
               "sibling with a shared prefix is outside")

        // ── Notes and files ──
        expect(move("/Inbox.md", into: "/Travel"), "/ws/Travel/Inbox.md", "root note into a folder")
        expect(move("/Travel/Kyoto.md", into: "/"), "/ws/Kyoto.md", "note back out to the root")
        expect(move("/Travel/cover.jpg", into: "/Archive/2025"), "/ws/Archive/2025/cover.jpg", "file keeps its extension")
        expect(move("/Travel/Kyoto.md", into: "/Travel"), "alreadyThere", "same folder")
        expect(move("/Kyoto.md", into: "/"), "alreadyThere", "root note dropped on the root")
        expect(move("/Inbox.md", into: "/Travel", taken: ["/ws/Travel/Inbox.md"]), "nameTaken(\"Inbox.md\")",
               "name collision")
        expect(move("/Inbox.md", into: "/Travel", taken: ["/ws/Travel/Other.md"]), "/ws/Travel/Inbox.md",
               "other names don't collide")

        // ── Folders ──
        expect(move("/Travel", into: "/Archive"), "/ws/Archive/Travel", "folder into a folder")
        expect(move("/Archive/Travel", into: "/"), "/ws/Travel", "folder back out to the root")
        expect(move("/Travel", into: "/Travel"), "intoItself", "folder onto itself")
        expect(move("/Travel", into: "/Travel/Japan/Kyoto"), "intoItself", "folder into its own descendant")
        expect(move("/Travel", into: "/Travel Notes"), "/ws/Travel Notes/Travel", "shared name prefix is not a descendant")
        expect(move("/Archive/Travel", into: "/Archive"), "alreadyThere", "folder already in that folder")
        expect(move("/Travel", into: "/Archive", taken: ["/ws/Archive/Travel"]), "nameTaken(\"Travel\")",
               "folder name collision")

        // ── Outside the library ──
        let outside = LibraryMove.destination(moving: URL(fileURLWithPath: "/Users/me/Desktop/a.md"),
                                              into: library, library: library, exists: { _ in false })
        expect(outside.failureValue, .outsideLibrary, "item from outside the library")
        let intoOutside = LibraryMove.destination(moving: url("/a.md"), into: URL(fileURLWithPath: "/tmp"),
                                                  library: library, exists: { _ in false })
        expect(intoOutside.failureValue, .outsideLibrary, "folder outside the library")
        let library = LibraryMove.destination(moving: library, into: url("/Travel"),
                                              library: library, exists: { _ in false })
        expect(library.failureValue, .outsideLibrary, "the library itself can't move")

        // ── Following a move ──
        expect(LibraryMove.remap("/ws/Travel/Kyoto.md", from: "/ws/Travel/Kyoto.md", to: "/ws/Kyoto.md"),
               "/ws/Kyoto.md", "the moved item")
        expect(LibraryMove.remap("/ws/Travel/Japan/Kyoto.md", from: "/ws/Travel", to: "/ws/Archive/Travel"),
               "/ws/Archive/Travel/Japan/Kyoto.md", "an item inside a moved folder")
        expect(LibraryMove.remap("/ws/Travel Notes/a.md", from: "/ws/Travel", to: "/ws/Archive/Travel"),
               nil, "shared name prefix is untouched")
        expect(LibraryMove.remap("/Travel/Japan", from: "/Travel", to: "/Archive/Travel"),
               "/Archive/Travel/Japan", "folder ids remap too")
        expect(LibraryMove.remap("/Other", from: "/Travel", to: "/Archive/Travel"), nil, "unrelated id")

        if failures == 0 {
            print("All \(checks) LibraryMove checks passed.")
        } else {
            print("\(failures) of \(checks) LibraryMove checks failed.")
            exit(1)
        }
    }
}

extension Result {
    var failureValue: Failure? {
        if case .failure(let error) = self { return error }
        return nil
    }
}
