import Foundation
import Observation

/// One library on the sidebar's first level (design board 60 B): a folder,
/// with the name and icon Fumi shows for it.
struct LibraryEntry: Codable, Equatable, Identifiable {
    var path: String
    /// Set by Rename…: only what Fumi shows, the folder keeps its name.
    var name: String?
    /// One character or emoji, set by Change Icon…; without one the row
    /// shows the plain library icon (design board 61 A).
    var icon: String?

    var id: String { path }
    var url: URL { URL(fileURLWithPath: path) }
    var displayName: String { name ?? url.lastPathComponent }

    /// Which of the six callout tints a chosen icon sits on. Follows the path, so
    /// renaming keeps the color.
    var tint: Int {
        let hash = path.utf8.reduce(UInt32(5381)) { ($0 &<< 5) &+ $0 &+ UInt32($1) }
        return Int(hash % 6)
    }
}

/// The libraries the sidebar lists, in the order the user dragged them
/// into — which is also the order of ⌃⌘1–9. Every folder opened joins it;
/// only Remove from List takes one out.
@Observable
final class LibraryList {
    static let key = "fumi.libraries"

    private(set) var entries: [LibraryEntry]
    @ObservationIgnored private let defaults: UserDefaults

    init(defaults: UserDefaults, seed: () -> [String]) {
        self.defaults = defaults
        if let data = defaults.data(forKey: Self.key),
           let saved = try? JSONDecoder().decode([LibraryEntry].self, from: data) {
            entries = saved
        } else {
            var paths: [String] = []
            for path in seed() where !paths.contains(path) { paths.append(path) }
            entries = paths.map { LibraryEntry(path: $0) }
        }
    }

    func entry(for url: URL?) -> LibraryEntry? {
        guard let path = url?.standardizedFileURL.path else { return nil }
        return entries.first { $0.path == path }
    }

    /// A folder just opened: last in the list, unless it's already there.
    func add(_ url: URL) {
        let path = url.standardizedFileURL.path
        guard !entries.contains(where: { $0.path == path }) else { return }
        entries.append(LibraryEntry(path: path))
        save()
    }

    func remove(_ path: String) {
        entries.removeAll { $0.path == path }
        save()
    }

    /// Drops `path` where `target` is, the rows between shifting over.
    func move(_ path: String, to target: String) {
        guard path != target,
              let from = entries.firstIndex(where: { $0.path == path }),
              let to = entries.firstIndex(where: { $0.path == target }) else { return }
        let entry = entries.remove(at: from)
        entries.insert(entry, at: to)
        save()
    }

    /// An empty name goes back to the folder's own.
    func rename(_ path: String, to name: String) {
        update(path) { $0.name = Self.trimmed(name) }
    }

    /// Keeps only the first character (an emoji counts as one); empty goes
    /// back to the plain library icon.
    func setIcon(_ path: String, to icon: String) {
        update(path) { $0.icon = Self.trimmed(icon).map { String($0.prefix(1)) } }
    }

    private func update(_ path: String, _ change: (inout LibraryEntry) -> Void) {
        guard let index = entries.firstIndex(where: { $0.path == path }) else { return }
        change(&entries[index])
        save()
    }

    private func save() {
        guard let data = try? JSONEncoder().encode(entries) else { return }
        defaults.set(data, forKey: Self.key)
    }

    private static func trimmed(_ text: String) -> String? {
        let value = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }

    // MARK: - Describing a library

    /// Where the folder lives, short: "~/Documents", "iCloud Drive",
    /// "T7/Notes" for an external disk.
    static func location(of url: URL, home: String = NSHomeDirectory()) -> String {
        let parent = url.standardizedFileURL.deletingLastPathComponent().path
        let iCloud = home + "/Library/Mobile Documents/com~apple~CloudDocs"
        if parent == iCloud { return "iCloud Drive" }
        if parent.hasPrefix(iCloud + "/") { return "iCloud Drive/" + parent.dropFirst(iCloud.count + 1) }
        if parent == home { return "~" }
        if parent.hasPrefix(home + "/") { return "~/" + parent.dropFirst(home.count + 1) }
        if parent.hasPrefix("/Volumes/") { return String(parent.dropFirst("/Volumes/".count)) }
        return parent
    }

    /// Why a library can't be opened, or nil when its folder is there.
    static func missingReason(_ url: URL, exists: (String) -> Bool = { path in
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory) && isDirectory.boolValue
    }) -> String? {
        let path = url.standardizedFileURL.path
        guard !exists(path) else { return nil }
        let parts = url.standardizedFileURL.pathComponents
        if parts.count >= 3, parts[1] == "Volumes", !exists("/Volumes/" + parts[2]) {
            return "Disk “\(parts[2])” isn’t connected"
        }
        return "Moved or deleted"
    }
}
