import Foundation

// Standalone test runner for NoteTreeBuilder. No Xcode test target exists;
// it is Foundation-only, so this compiles directly with the sources:
//
//     swiftc Fumi/Models/FileNote.swift Fumi/Views/Sidebar/NoteTree.swift Tests/NoteTreeTests.swift -o /tmp/notetree-tests
//     /tmp/notetree-tests
//
// Exits non-zero if any assertion fails.

@main
enum NoteTreeTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    static func note(_ title: String, in folder: String, tags: [String] = [], body: String = "") -> FileNote {
        let dir = folder == "/" ? "" : folder
        let url = URL(fileURLWithPath: "/ws\(dir)/\(title).md")
        return FileNote(id: url.path, url: url, title: title, content: "# \(title)\n\n\(body)",
                        tags: tags, createdAt: Date(), updatedAt: Date(), pinned: false,
                        relativePath: folder)
    }

    static func main() {
        // ── Tree building ──
        let notes = [
            note("Root B", in: "/"),
            note("Weekly", in: "/产品思考"),
            note("Draft", in: "/产品思考/草稿"),
            note("Root A", in: "/"),
            note("Kyoto", in: "/Travel"),
            note("Slow decisions", in: "/产品思考"),
        ]
        let root = NoteTreeBuilder.build(from: notes)

        expect(root.notes.map(\.title), ["Root B", "Root A"], "root notes keep input order")
        expect(root.folders.map(\.name), ["Travel", "产品思考"], "folders sorted by name")
        expect(root.folders.map(\.id), ["/Travel", "/产品思考"], "folder ids are relative paths")
        let product = root.folders[1]
        expect(product.notes.map(\.title), ["Weekly", "Slow decisions"], "folder notes keep input order")
        expect(product.folders.map(\.id), ["/产品思考/草稿"], "nested folder id")
        expect(product.totalCount, 3, "totalCount includes subfolders")
        expect(root.totalCount, 6, "root totalCount is every note")
        expect(NoteTreeBuilder.build(from: []).totalCount, 0, "empty workspace")

        // ── Images, videos, PDFs ──
        func file(_ name: String, in folder: String) -> LibraryFile {
            let dir = folder == "/" ? "" : folder
            let url = URL(fileURLWithPath: "/ws\(dir)/\(name)")
            return LibraryFile(id: url.path, url: url, kind: LibraryFile.kind(of: url)!, updatedAt: Date(),
                               relativePath: folder)
        }
        let withFiles = NoteTreeBuilder.build(from: notes, files: [
            file("z.mp4", in: "/attachments"), file("a 10.png", in: "/attachments"),
            file("a 9.png", in: "/attachments"), file("cover.jpg", in: "/Travel"),
        ])
        expect(withFiles.folders.map(\.name), ["attachments", "Travel", "产品思考"],
               "a folder holding only files is listed")
        expect(withFiles.folders[0].files.map(\.name), ["a 9.png", "a 10.png", "z.mp4"], "files sorted like Finder")
        expect(withFiles.folders[1].files.map(\.name), ["cover.jpg"], "file beside the notes of its folder")
        expect(withFiles.folders[1].notes.map(\.title), ["Kyoto"], "notes unchanged")
        expect(withFiles.totalCount, 6, "totalCount still counts notes only")
        expect(withFiles.itemCount, 10, "itemCount counts files too")

        expect(NoteTreeBuilder.ancestorIds(ofRelativePath: "/"), [], "root note has no ancestors")
        expect(NoteTreeBuilder.ancestorIds(ofRelativePath: "/产品思考/草稿"),
               ["/产品思考", "/产品思考/草稿"], "ancestor chain")

        if failures == 0 {
            print("All \(checks) NoteTree checks passed.")
        } else {
            print("\(failures) of \(checks) NoteTree checks failed.")
            exit(1)
        }
    }
}
