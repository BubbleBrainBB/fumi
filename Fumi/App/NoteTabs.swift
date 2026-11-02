import Foundation

/// One open page in a window (design board ㊴ B): a note, a blank draft,
/// the Memos page or a file preview. A window holds one or more.
struct NoteTab: Identifiable, Equatable, Codable {
    enum Content: Equatable, Codable {
        /// A blank page not yet on disk (board ㉝ E).
        case draft
        /// A note, by its id (the file path).
        case note(String)
        case memos
        /// An image, video or PDF, by its id (the file path).
        case file(String)

        /// The note or file path this tab shows, if any.
        var path: String? {
            switch self {
            case .note(let id), .file(let id): id
            case .draft, .memos: nil
            }
        }
    }

    var id = UUID()
    var content: Content
    /// Pinned tabs show only their icon and stay at the leading end.
    var isPinned = false
}

/// The tabs of a library, kept between launches: one set per library
/// folder, so reopening a folder brings its tabs back.
enum SavedTabs {
    private static let key = "fumi.tabs"

    private struct Entry: Codable {
        var tabs: [NoteTab]
        var active: Int
    }

    static func load(workspace: String) -> (tabs: [NoteTab], active: Int)? {
        guard let all = UserDefaults.standard.dictionary(forKey: key),
              let data = all[workspace] as? Data,
              let entry = try? JSONDecoder().decode(Entry.self, from: data),
              !entry.tabs.isEmpty else { return nil }
        return (entry.tabs, min(max(entry.active, 0), entry.tabs.count - 1))
    }

    static func save(_ tabs: [NoteTab], active: Int, workspace: String) {
        guard let data = try? JSONEncoder().encode(Entry(tabs: tabs, active: active)) else { return }
        var all = UserDefaults.standard.dictionary(forKey: key) ?? [:]
        all[workspace] = data
        UserDefaults.standard.set(all, forKey: key)
    }
}

/// Where a tab has been, as in a browser: each tab keeps its own.
struct TabHistory {
    var back: [NoteTab.Content] = []
    var forward: [NoteTab.Content] = []

    static let limit = 50

    /// A new page replaces whatever was ahead. A blank draft isn't a
    /// place to come back to.
    mutating func visit(leaving old: NoteTab.Content) {
        forward = []
        guard old != .draft else { return }
        back.removeAll { $0 == old }
        back.append(old)
        if back.count > Self.limit { back.removeFirst() }
    }

    /// The page before `current`, which goes ahead of it; nil if none.
    mutating func back(from current: NoteTab.Content) -> NoteTab.Content? {
        guard let previous = back.popLast() else { return nil }
        if current != .draft { forward.append(current) }
        return previous
    }

    /// The page Back left, `current` going behind it; nil if none.
    mutating func forward(from current: NoteTab.Content) -> NoteTab.Content? {
        guard let next = forward.popLast() else { return nil }
        if current != .draft { back.append(current) }
        return next
    }

    mutating func remap(_ transform: (NoteTab.Content) -> NoteTab.Content) {
        back = back.map(transform)
        forward = forward.map(transform)
    }

    mutating func remove(_ content: NoteTab.Content) {
        back.removeAll { $0 == content }
        forward.removeAll { $0 == content }
    }
}
