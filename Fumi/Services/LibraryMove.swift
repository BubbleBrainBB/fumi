import Foundation

/// The path rules for moving a note, file or folder to another folder of the
/// library — dragged in the sidebar or picked from Move To. Foundation-only
/// (no file system access: whether a name is taken is asked of `exists`), so
/// Tests/LibraryMoveTests.swift runs it directly.
enum LibraryMove {
    enum Refusal: Error, Equatable {
        /// The item is already in that folder.
        case alreadyThere
        /// A folder can't go into itself or a folder inside it.
        case intoItself
        /// The item or the folder isn't inside the library (or is the library).
        case outsideLibrary
        /// The folder already holds an item with this name.
        case nameTaken(String)
        /// The move itself failed (the file system's message).
        case failed(String)
    }

    /// The folder a sidebar folder id stands for: "/" is the library itself,
    /// "/Projects/Drafts" a folder inside it.
    static func folderURL(forId id: String, in library: URL) -> URL {
        id.split(separator: "/").reduce(library) { $0.appendingPathComponent(String($1), isDirectory: true) }
    }

    /// The sidebar folder id of a folder inside the library ("/" for the
    /// library itself), or nil for one outside it.
    static func folderId(of folder: URL, in library: URL) -> String? {
        let path = normalizedPath(folder), root = normalizedPath(library)
        if path == root { return "/" }
        return remap(path, from: root, to: "")
    }

    /// Where moving `item` into `folder` puts it, or why it can't go there.
    static func destination(moving item: URL, into folder: URL, library: URL,
                            exists: (URL) -> Bool) -> Result<URL, Refusal> {
        let itemPath = normalizedPath(item)
        let folderPath = normalizedPath(folder)
        let root = normalizedPath(library)
        guard itemPath != root, isSameOrInside(itemPath, root), isSameOrInside(folderPath, root) else {
            return .failure(.outsideLibrary)
        }
        if isSameOrInside(folderPath, itemPath) { return .failure(.intoItself) }
        if normalizedPath(item.deletingLastPathComponent()) == folderPath { return .failure(.alreadyThere) }
        let name = item.lastPathComponent
        let target = URL(fileURLWithPath: folderPath).appendingPathComponent(name)
        if exists(target) { return .failure(.nameTaken(name)) }
        return .success(target)
    }

    /// `path` after the item at `old` moved to `new`: `new` itself, or the
    /// same place under `new` for a path inside a moved folder; nil when the
    /// move doesn't touch it. Works on file paths and folder ids alike.
    static func remap(_ path: String, from old: String, to new: String) -> String? {
        if path == old { return new }
        guard path.hasPrefix(old + "/") else { return nil }
        return new + path.dropFirst(old.count)
    }

    /// Whether `path` is `ancestor` or lies inside it.
    static func isSameOrInside(_ path: String, _ ancestor: String) -> Bool {
        path == ancestor || path.hasPrefix(ancestor == "/" ? "/" : ancestor + "/")
    }

    private static func normalizedPath(_ url: URL) -> String {
        var path = url.standardizedFileURL.path
        while path.count > 1, path.hasSuffix("/") { path.removeLast() }
        return path
    }
}
