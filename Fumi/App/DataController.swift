import Foundation
import SwiftData

/// Manages the SwiftData ModelContainer and provides note CRUD operations.
///
/// All data operations go through this controller so the UI layers
/// don't need to manage ModelContext directly.
@Observable
final class DataController {
    let container: ModelContainer
    let context: ModelContext

    init() throws {
        let schema = Schema([NoteModel.self])
        let config = ModelConfiguration(schema: schema, isStoredInMemoryOnly: false)
        container = try ModelContainer(for: schema, configurations: [config])
        context = ModelContext(container)
    }

    // MARK: - Create

    @discardableResult
    func createNote() -> NoteModel {
        let note = NoteModel()
        context.insert(note)
        save()
        return note
    }

    // MARK: - Read

    func fetchAllNotes(includeTrashed: Bool = false) -> [NoteModel] {
        let descriptor: FetchDescriptor<NoteModel>
        if includeTrashed {
            descriptor = FetchDescriptor<NoteModel>()
        } else {
            descriptor = FetchDescriptor<NoteModel>(
                predicate: #Predicate { $0.trashed == false }
            )
        }
        return (try? context.fetch(descriptor)) ?? []
    }

    func fetchTrashed() -> [NoteModel] {
        let descriptor = FetchDescriptor<NoteModel>(
            predicate: #Predicate { $0.trashed == true }
        )
        return (try? context.fetch(descriptor)) ?? []
    }

    func fetchNote(byId id: UUID) -> NoteModel? {
        let descriptor = FetchDescriptor<NoteModel>(
            predicate: #Predicate { $0.id == id }
        )
        return try? context.fetch(descriptor).first
    }

    func searchNotes(query: String, trashed: Bool = false) -> [NoteModel] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            return fetchAllNotes(includeTrashed: false)
        }
        // SwiftData #Predicate doesn't support localizedCaseInsensitiveContains,
        // so fetch all matching trashed state and filter in memory.
        let all = fetchAllNotes(includeTrashed: trashed)
        return all.filter { note in
            (note.trashed == trashed) && (
                note.title.localizedCaseInsensitiveContains(trimmed) ||
                note.content.localizedCaseInsensitiveContains(trimmed)
            )
        }
    }

    // MARK: - Update

    func save() {
        do {
            try context.save()
        } catch {
            print("DataController save error: \(error)")
        }
    }

    func updateNote(_ note: NoteModel, content: String) {
        note.content = content
        note.title = TitleExtractor.extract(from: content)
        note.tags = TagExtractor.frontmatterTags(from: content)
        note.updatedAt = Date()
        save()
    }

    func togglePin(_ note: NoteModel) {
        note.pinned.toggle()
        save()
    }

    // MARK: - Delete (Trash)

    func trashNote(_ note: NoteModel) {
        note.trashed = true
        note.trashedAt = Date()
        save()
    }

    func restoreNote(_ note: NoteModel) {
        note.trashed = false
        note.trashedAt = nil
        save()
    }

    func permanentlyDelete(_ note: NoteModel) {
        context.delete(note)
        save()
    }

    /// Auto-cleanup: permanently delete notes in Trash older than 30 days.
    func cleanupOldTrash() {
        let cutoff = Calendar.current.date(byAdding: .day, value: -30, to: Date()) ?? Date()
        let descriptor = FetchDescriptor<NoteModel>(
            predicate: #Predicate { $0.trashed == true && $0.trashedAt != nil && $0.trashedAt! < cutoff }
        )
        if let oldNotes = try? context.fetch(descriptor) {
            for note in oldNotes {
                context.delete(note)
            }
            if !oldNotes.isEmpty { save() }
        }
    }
}
