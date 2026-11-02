import Foundation

// Standalone test runner for AttachmentImport (pasting / dropping media):
//
//     swiftc Fumi/Models/FileNote.swift Fumi/Views/Editor/AttachmentImport.swift Tests/AttachmentImportTests.swift -o /tmp/attachment-tests
//     /tmp/attachment-tests
//
// Exits non-zero if any assertion fails.

@main
enum AttachmentImportTests {
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
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent("attachment-tests-\(UUID().uuidString)")
        let library = root.appendingPathComponent("Library")
        let outside = root.appendingPathComponent("Desktop")
        try! fm.createDirectory(at: library.appendingPathComponent("Notes/Deep"), withIntermediateDirectories: true)
        try! fm.createDirectory(at: outside, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }

        // ── Media types ──
        expect(AttachmentImport.isMedia(URL(fileURLWithPath: "/a/clip.MOV")), true, "video, any case")
        expect(AttachmentImport.isMedia(URL(fileURLWithPath: "/a/shot.png")), true, "image")
        expect(AttachmentImport.isMedia(URL(fileURLWithPath: "/a/doc.pdf")), false, "not media")

        // ── Copying in ──
        let clip = outside.appendingPathComponent("my clip.mp4")
        try! Data("v1".utf8).write(to: clip)
        let first = try! AttachmentImport.importFile(clip, into: library)
        expect(first.lastPathComponent, "my clip.mp4", "copied under its own name")
        expect(first.deletingLastPathComponent().lastPathComponent, "attachments", "into attachments/")
        expect(fm.fileExists(atPath: clip.path), true, "original left in place")
        let second = try! AttachmentImport.importFile(clip, into: library)
        expect(second.lastPathComponent, "my clip 2.mp4", "name taken: numbered")

        // A file already in the library is linked, not copied.
        let inLibrary = library.appendingPathComponent("Notes/shot.png")
        try! Data("p".utf8).write(to: inLibrary)
        expect(try! AttachmentImport.importFile(inLibrary, into: library).path, inLibrary.standardizedFileURL.path,
               "file inside the library is used as is")

        // Pasted image data.
        let date = DateComponents(calendar: Calendar(identifier: .gregorian), year: 2026, month: 10, day: 17,
                                  hour: 3, minute: 30, second: 5).date!
        let pasted = try! AttachmentImport.saveImage(Data("img".utf8), ext: "png", into: library, date: date)
        expect(pasted.lastPathComponent, "Pasted image 20261017033005.png", "pasted image name")

        // ── Paths ──
        let attachments = library.appendingPathComponent("attachments")
        let file = attachments.appendingPathComponent("my clip.mp4")
        expect(AttachmentImport.relativePath(from: library, to: file), "attachments/my clip.mp4", "note at the root")
        expect(AttachmentImport.relativePath(from: library.appendingPathComponent("Notes/Deep"), to: file),
               "../../attachments/my clip.mp4", "note two folders down")
        expect(AttachmentImport.markdown(forPath: "../attachments/my clip (1).mp4"),
               "![](../attachments/my%20clip%20%281%29.mp4)", "spaces and parentheses encoded")
        expect(AttachmentImport.markdown(forPath: "attachments/截图.png"),
               "![](attachments/截图.png)", "Chinese names stay readable")

        if failures == 0 {
            print("All \(checks) AttachmentImport checks passed.")
        } else {
            print("\(failures) of \(checks) AttachmentImport checks failed.")
            exit(1)
        }
    }
}
