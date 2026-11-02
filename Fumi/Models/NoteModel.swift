import Foundation
import SwiftData

/// SwiftData entity representing a single Markdown note.
///
/// Tags are stored as a newline-delimited string in `tagsRaw` for efficient
/// SwiftData predicate queries. The tag tree is dynamically built by
/// aggregating all notes' tags at view time.
@Model
final class NoteModel {
    var id: UUID
    var title: String
    var content: String
    private var tagsRaw: String
    var pinned: Bool
    var trashed: Bool
    var trashedAt: Date?
    var createdAt: Date
    var updatedAt: Date

    init(content: String = "") {
        self.id = UUID()
        self.title = ""
        self.content = content
        self.tagsRaw = ""
        self.pinned = false
        self.trashed = false
        self.trashedAt = nil
        self.createdAt = Date()
        self.updatedAt = Date()
    }

    var tags: [String] {
        get { tagsRaw.split(separator: "\n").map(String.init) }
        set { tagsRaw = newValue.joined(separator: "\n") }
    }
}
