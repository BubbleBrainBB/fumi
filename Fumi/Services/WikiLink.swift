import Foundation

/// An Obsidian link between notes, `[[target#heading|alias]]` (design board
/// 58): shown as the linked note's title, opened by a click.
struct WikiLink: Equatable {
    /// The note's name or library path, without `.md`.
    let target: String
    let heading: String?
    let alias: String?

    /// Parses what sits between `[[` and `]]`; nil when it names nothing.
    init?(_ inner: String) {
        var rest = inner
        var alias: String?
        if let bar = rest.firstIndex(of: "|") {
            let text = rest[rest.index(after: bar)...].trimmingCharacters(in: .whitespaces)
            alias = text.isEmpty ? nil : text
            rest = String(rest[..<bar])
        }
        var heading: String?
        if let hash = rest.firstIndex(of: "#") {
            let text = rest[rest.index(after: hash)...].trimmingCharacters(in: .whitespaces)
            heading = text.isEmpty ? nil : text
            rest = String(rest[..<hash])
        }
        var target = rest.trimmingCharacters(in: .whitespaces)
        if target.lowercased().hasSuffix(".md") { target = String(target.dropLast(3)) }
        guard !target.isEmpty else { return nil }
        self.target = target
        self.heading = heading
        self.alias = alias
    }

    /// What the link shows: its alias, else the note's title, with the
    /// heading after it (`Title › Heading`). A missing note shows its target.
    func displayText(title: String?) -> String {
        if let alias { return alias }
        let name = title ?? (target as NSString).lastPathComponent
        guard let heading else { return name }
        return "\(name) › \(heading)"
    }

    /// `[[…]]` that isn't an embed (`![[…]]`), its inner text in group 1.
    static let pattern = try! NSRegularExpression(pattern: #"(?<!!)\[\[([^\[\]\n]+)\]\]"#)
}

/// The library's notes by the names links use, with their titles. Kept up
/// to date by `FileNoteStore`; read by the editor while it styles and when
/// a link is clicked.
final class WikiLinkIndex {
    static let shared = WikiLinkIndex()

    struct Entry: Equatable {
        let id: String
        let url: URL
        let title: String
    }

    /// Note file names and library paths (lowercased, without `.md`) → notes.
    private var byName: [String: [Entry]] = [:]
    private var byPath: [String: Entry] = [:]

    /// Bumped whenever a title, name or note changes, so styled links redraw.
    private(set) var revision = 0

    static let didChange = Notification.Name("fumi.wikiLinkIndexDidChange")

    /// Rebuilds from the library's notes: `(id, url, content)` each, with the
    /// library's folder to read paths against.
    func update(notes: [(id: String, url: URL, content: String)], library: URL?) {
        var names: [String: [Entry]] = [:]
        var paths: [String: Entry] = [:]
        let root = library?.standardizedFileURL.path
        for note in notes {
            let file = note.url.deletingPathExtension()
            let entry = Entry(id: note.id, url: note.url,
                              title: Self.title(of: note.content, fileName: file.lastPathComponent))
            names[file.lastPathComponent.lowercased(), default: []].append(entry)
            let path = file.standardizedFileURL.path
            if let root, path.hasPrefix(root + "/") {
                paths[String(path.dropFirst(root.count + 1)).lowercased()] = entry
            }
        }
        guard names != byName || paths != byPath else { return }
        byName = names
        byPath = paths
        revision += 1
        NotificationCenter.default.post(name: Self.didChange, object: self)
    }

    /// Whether links to `old` show the same once it's `new`: the same note in
    /// the same place, with the same title. Compares the body only when it
    /// changed, so an unchanged library costs next to nothing.
    static func showsSame(_ old: FileNote, _ new: FileNote) -> Bool {
        guard old.id == new.id, old.url == new.url else { return false }
        return old.content == new.content
            || title(of: old.content, fileName: new.filename) == title(of: new.content, fileName: new.filename)
    }

    /// The note `target` names: by library path when it has one, else by
    /// file name — one in `folder` (the linking note's) first, then the
    /// one nearest the library's top, as Obsidian picks.
    func resolve(_ target: String, from folder: URL?) -> Entry? {
        let key = target.lowercased()
        if key.contains("/"), let entry = byPath[key] { return entry }
        let name = (key as NSString).lastPathComponent
        guard let candidates = byName[name], !candidates.isEmpty else { return nil }
        if candidates.count == 1 { return candidates[0] }
        if let folder = folder?.standardizedFileURL.path,
           let near = candidates.first(where: { $0.url.deletingLastPathComponent().standardizedFileURL.path == folder }) {
            return near
        }
        return candidates.min { $0.url.pathComponents.count < $1.url.pathComponents.count }
    }

    /// A note's title as links show it: the front matter's `title`, else a
    /// `# ` heading on its first line, else its file name.
    static func title(of content: String, fileName: String) -> String {
        // Only the head of the note matters: don't split all of it.
        var lines = content.split(separator: "\n", maxSplits: 300, omittingEmptySubsequences: false)[...]
        if lines.first?.trimmingCharacters(in: .whitespaces) == "---" {
            lines = lines.dropFirst()
            while let line = lines.first {
                lines = lines.dropFirst()
                let trimmed = line.trimmingCharacters(in: .whitespaces)
                if trimmed == "---" || trimmed == "..." { break }
                if trimmed.hasPrefix("title:") {
                    let value = unquote(trimmed.dropFirst(6).trimmingCharacters(in: .whitespaces))
                    if !value.isEmpty { return value }
                }
            }
        }
        if let first = lines.first(where: { !$0.trimmingCharacters(in: .whitespaces).isEmpty }) {
            let trimmed = first.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("# ") {
                let heading = trimmed.dropFirst(2).trimmingCharacters(in: .whitespaces)
                if !heading.isEmpty { return heading }
            }
        }
        return fileName
    }

    private static func unquote(_ value: String) -> String {
        guard value.count >= 2, let first = value.first, first == value.last, first == "\"" || first == "'" else {
            return value
        }
        return String(value.dropFirst().dropLast())
    }
}
