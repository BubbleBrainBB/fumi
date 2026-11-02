import Foundation

/// Images and videos pasted or dropped into a note: copied into the library's
/// `attachments/` folder and linked with `![](…)` relative to the note, so the
/// files travel with the library and other Markdown apps find them too.
enum AttachmentImport {
    static let folderName = "attachments"

    /// Images and videos; other files (PDFs too) paste and drop as before.
    static func isMedia(_ url: URL) -> Bool {
        let kind = LibraryFile.kind(of: url)
        return kind == .image || kind == .video
    }

    /// The file's place in the library: the file itself when it already lives
    /// inside it, otherwise a copy in `attachments/`.
    static func importFile(_ file: URL, into library: URL, fileManager: FileManager = .default) throws -> URL {
        let file = file.standardizedFileURL
        if isInside(file, library) { return file }
        let folder = try attachmentsFolder(in: library, fileManager: fileManager)
        let target = uniqueURL(for: file.lastPathComponent, in: folder, fileManager: fileManager)
        try fileManager.copyItem(at: file, to: target)
        return target
    }

    /// Saves pasted image data (a screenshot, an image copied from a browser).
    static func saveImage(_ data: Data, ext: String, into library: URL, date: Date = Date(),
                          fileManager: FileManager = .default) throws -> URL {
        let folder = try attachmentsFolder(in: library, fileManager: fileManager)
        let target = uniqueURL(for: "Pasted image \(stamp(date)).\(ext)", in: folder, fileManager: fileManager)
        try data.write(to: target, options: .atomic)
        return target
    }

    /// `name`, or `name 2`, `name 3` … — the first that isn't taken.
    static func uniqueURL(for name: String, in folder: URL, fileManager: FileManager = .default) -> URL {
        let base = (name as NSString).deletingPathExtension
        let ext = (name as NSString).pathExtension
        var candidate = folder.appendingPathComponent(name)
        var n = 2
        while fileManager.fileExists(atPath: candidate.path) {
            let numbered = ext.isEmpty ? "\(base) \(n)" : "\(base) \(n).\(ext)"
            candidate = folder.appendingPathComponent(numbered)
            n += 1
        }
        return candidate
    }

    /// `file`'s path from `directory`, with `../` where needed.
    static func relativePath(from directory: URL, to file: URL) -> String {
        let from = directory.standardizedFileURL.pathComponents
        let to = file.standardizedFileURL.pathComponents
        var shared = 0
        while shared < min(from.count, to.count), from[shared] == to[shared] { shared += 1 }
        let ups = Array(repeating: "..", count: from.count - shared)
        return (ups + to[shared...]).joined(separator: "/")
    }

    /// `![](path)`. Only what would break the link is encoded (spaces,
    /// parentheses, `%`); other names, Chinese ones too, stay readable.
    static func markdown(forPath path: String) -> String {
        var encoded = ""
        for character in path {
            switch character {
            case "%": encoded += "%25"
            case " ": encoded += "%20"
            case "(": encoded += "%28"
            case ")": encoded += "%29"
            default: encoded.append(character)
            }
        }
        return "![](\(encoded))"
    }

    // MARK: Helpers

    private static func attachmentsFolder(in library: URL, fileManager: FileManager) throws -> URL {
        let folder = library.appendingPathComponent(folderName, isDirectory: true)
        try fileManager.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    private static func isInside(_ file: URL, _ folder: URL) -> Bool {
        let parent = folder.standardizedFileURL.pathComponents
        let child = file.pathComponents
        return child.count > parent.count && Array(child.prefix(parent.count)) == parent
    }

    private static func stamp(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMddHHmmss"
        return formatter.string(from: date)
    }
}
