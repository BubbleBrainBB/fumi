import Foundation

struct FileNote: Identifiable, Hashable {
    let id: String          // file URL path as unique ID
    let url: URL
    var title: String
    var content: String
    var tags: [String]
    var createdAt: Date
    var updatedAt: Date
    var pinned: Bool
    var relativePath: String  // path relative to workspace root, for display

    var filename: String {
        url.deletingPathExtension().lastPathComponent
    }

    /// `scanned`, a fresh read of the library, holding on to the `known` notes
    /// it found unchanged. Two copies of a text compare slowly, a character
    /// at a time, and views compare notes on the main thread; one copy with
    /// itself compares at once.
    static func keepingUnchanged(_ known: [FileNote], in scanned: [FileNote]) -> [FileNote] {
        let byID = Dictionary(known.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        return scanned.map { note in
            guard let old = byID[note.id], old.content.utf8.elementsEqual(note.content.utf8) else { return note }
            var kept = note
            kept.content = old.content
            return kept == old ? old : kept
        }
    }
}

/// A non-note file in the library the app can show: an image, a video or a
/// PDF. Listed in the sidebar beside the notes of its folder and opened in a
/// preview page.
struct LibraryFile: Identifiable, Hashable {
    enum Kind: Hashable {
        case image, video, pdf
    }

    let id: String          // file URL path
    let url: URL
    let kind: Kind
    var updatedAt: Date
    var relativePath: String  // folder relative to the workspace root, like FileNote's

    /// File name with its extension (`cover.jpg`).
    var name: String { url.lastPathComponent }

    static let imageExtensions: Set<String> = ["png", "jpg", "jpeg", "gif", "webp", "heic", "tiff", "tif", "bmp", "svg"]
    static let videoExtensions: Set<String> = ["mp4", "mov", "m4v", "webm"]

    static func kind(of url: URL) -> Kind? {
        let ext = url.pathExtension.lowercased()
        if imageExtensions.contains(ext) { return .image }
        if videoExtensions.contains(ext) { return .video }
        if ext == "pdf" { return .pdf }
        return nil
    }
}
