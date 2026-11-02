import Foundation

/// A folder (notebook) in the sidebar's notes tree. Folders mirror the
/// workspace's subdirectories; `id` is the folder's path relative to the
/// workspace root ("/" for the root, "/产品思考/草稿" for a nested one).
struct NoteFolder: Identifiable, Equatable {
    let id: String
    let name: String
    var folders: [NoteFolder] = []
    var notes: [FileNote] = []
    /// Images, videos and PDFs, listed after the notes, by name.
    var files: [LibraryFile] = []

    /// Notes in this folder and every folder below it.
    var totalCount: Int {
        notes.count + folders.reduce(0) { $0 + $1.totalCount }
    }

    /// Notes and files in this folder and every folder below it.
    var itemCount: Int {
        notes.count + files.count + folders.reduce(0) { $0 + $1.itemCount }
    }
}

enum NoteTreeBuilder {
    /// Groups notes into a folder tree by their `relativePath`. Notes keep the
    /// order they were passed in (the caller sorts them); folders are sorted by
    /// name the way Finder does.
    static func build(from notes: [FileNote], files: [LibraryFile] = []) -> NoteFolder {
        var root = NoteFolder(id: "/", name: "")
        for note in notes {
            let components = note.relativePath.split(separator: "/").map(String.init)
            insert(components[...], into: &root) { $0.notes.append(note) }
        }
        for file in files {
            let components = file.relativePath.split(separator: "/").map(String.init)
            insert(components[...], into: &root) { $0.files.append(file) }
        }
        sortFolders(&root)
        return root
    }

    private static func insert(_ path: ArraySlice<String>, into folder: inout NoteFolder,
                               _ add: (inout NoteFolder) -> Void) {
        guard let head = path.first else {
            add(&folder)
            return
        }
        let childId = (folder.id == "/" ? "" : folder.id) + "/" + head
        if let idx = folder.folders.firstIndex(where: { $0.id == childId }) {
            insert(path.dropFirst(), into: &folder.folders[idx], add)
        } else {
            var child = NoteFolder(id: childId, name: head)
            insert(path.dropFirst(), into: &child, add)
            folder.folders.append(child)
        }
    }

    private static func sortFolders(_ folder: inout NoteFolder) {
        folder.folders.sort { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        folder.files.sort { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        for i in folder.folders.indices {
            sortFolders(&folder.folders[i])
        }
    }

    /// Every folder id from the root down to (and including) the folder that
    /// holds a note at `relativePath`. Used to auto-expand the path to the
    /// selected note.
    static func ancestorIds(ofRelativePath relativePath: String) -> [String] {
        var ids: [String] = []
        var current = ""
        for component in relativePath.split(separator: "/") {
            current += "/" + component
            ids.append(current)
        }
        return ids
    }
}
