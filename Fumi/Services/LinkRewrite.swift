import Foundation

/// Keeps a moved note's links on the same files. Pasted images and videos are
/// linked relative to the note's folder (`../attachments/…` from a subfolder),
/// so a note that changes folder — moved alone, or with a folder whose links
/// lead out of it — can lose them. Foundation-only (whether a file is there is
/// asked of `exists`), so Tests/LinkRewriteTests.swift runs it directly.
/// Links are found by `InlineLink`, the grammar the editor renders with.
///
/// A link is read the way the editor's `ImageLoader` reads it: relative to the
/// note's folder first, then to the library. Only links that found a file
/// before the move and would now find another one (or none) are rewritten.
/// URLs, absolute and `~` paths, broken links, bare-name embeds (`![[a.png]]`,
/// found by name anywhere in the library) and anything in code stay as typed.
enum LinkRewrite {
    /// `content` of the note at `notePath` after the item at `movedFrom` — the
    /// note itself or a folder holding it — moved to `movedTo`. `exists` sees
    /// the library after the move. Returns `content` itself when no link
    /// needs to change, or when the move doesn't touch the note.
    static func rewrite(_ content: String, noteAt notePath: String, movedFrom: String, to movedTo: String,
                        library: String, exists: @escaping (String) -> Bool) -> String {
        guard let newNote = LibraryMove.remap(notePath, from: movedFrom, to: movedTo), newNote != notePath,
              content.contains("](") || content.contains("![[") else { return content }
        let context = Context(oldFolder: parent(notePath), newFolder: parent(newNote), library: library,
                              movedFrom: movedFrom, movedTo: movedTo, exists: exists)

        let ns = content as NSString
        let skipped = codeRanges(in: ns)
        func inCode(_ range: NSRange) -> Bool {
            skipped.contains { NSLocationInRange(range.location, $0) }
        }
        var edits: [(range: NSRange, text: String)] = []
        let all = NSRange(location: 0, length: ns.length)
        InlineLink.anyPattern.enumerateMatches(in: content, range: all) { match, _, _ in
            guard let match, !inCode(match.range) else { return }
            let isImage = ns.character(at: match.range.location) == 0x21 // "!"
            let destination = InlineLink.destinationRange(of: match)
            if let text = rewrittenTarget(ns.substring(with: destination), isImage: isImage,
                                          angled: InlineLink.isAngled(match), context) {
                edits.append((destination, text))
            }
        }
        embedPattern.enumerateMatches(in: content, range: all) { match, _, _ in
            guard let match, !inCode(match.range) else { return }
            let inner = match.range(at: 1)
            if let text = rewrittenEmbed(ns.substring(with: inner), context) {
                edits.append((inner, text))
            }
        }
        guard !edits.isEmpty else { return content }

        let result = NSMutableString(string: content)
        for edit in edits.sorted(by: { $0.range.location > $1.range.location }) {
            result.replaceCharacters(in: edit.range, with: edit.text)
        }
        return result as String
    }

    /// `file`'s path from `directory`, with `../` where needed. Both absolute.
    static func relativePath(from directory: String, to file: String) -> String {
        let from = components(directory), to = components(file)
        var shared = 0
        while shared < min(from.count, to.count), from[shared] == to[shared] { shared += 1 }
        let ups = Array(repeating: "..", count: from.count - shared)
        return (ups + to[shared...]).joined(separator: "/")
    }

    /// `relative` from `base`, with `.` and `..` worked out.
    static func join(_ base: String, _ relative: String) -> String {
        var parts = components(base)
        for part in relative.split(separator: "/", omittingEmptySubsequences: true).map(String.init) {
            switch part {
            case ".": continue
            case "..": if !parts.isEmpty { parts.removeLast() }
            default: parts.append(part)
            }
        }
        return "/" + parts.joined(separator: "/")
    }

    // MARK: - Links

    /// Obsidian embed: `![[file.ext]]`, `![[folder/file|300]]`.
    private static let embedPattern = try! NSRegularExpression(pattern: #"!\[\[([^\]\n]+)\]\]"#)
    /// A URL scheme: `https:`, `mailto:`, `file:`.
    private static let schemePattern = try! NSRegularExpression(pattern: #"^[A-Za-z][A-Za-z0-9+.\-]*:"#)

    private struct Context {
        let oldFolder: String, newFolder: String, library: String
        let movedFrom: String, movedTo: String
        let exists: (String) -> Bool

        /// Whether a file was at `path` before the move.
        func existedBefore(_ path: String) -> Bool {
            if let now = LibraryMove.remap(path, from: movedFrom, to: movedTo) { return exists(now) }
            // Moves never overwrite, so nothing was where the item landed.
            if LibraryMove.isSameOrInside(path, movedTo) { return false }
            return exists(path)
        }

        /// Where the file once at `path` is now.
        func now(_ path: String) -> String {
            LibraryMove.remap(path, from: movedFrom, to: movedTo) ?? path
        }

        /// The file `relative` points at now, if it moved, when it no longer
        /// reaches it from the note's new folder.
        func newTarget(for relative: String) -> String? {
            let before = [join(oldFolder, relative), join(library, relative)].first(where: existedBefore)
            guard let before else { return nil }
            let target = now(before)
            let after = [join(newFolder, relative), join(library, relative)].first(where: exists)
            return after == target ? nil : target
        }
    }

    /// The new destination of `[…](destination)` (inside the brackets when
    /// written `<…>`), or nil to leave it.
    private static func rewrittenTarget(_ destination: String, isImage: Bool, angled: Bool,
                                        _ context: Context) -> String? {
        var path = Substring(destination)
        // A link's `#section` is not part of the file; an image's whole target
        // is a path, as `ImageLoader` reads it.
        var fragment = ""
        if !isImage, let hash = path.firstIndex(of: "#") {
            fragment = String(path[hash...])
            path = path[..<hash]
        }
        let raw = String(path)
        guard isRelativeLocal(raw) else { return nil }
        let decoded = raw.removingPercentEncoding ?? raw
        guard let target = context.newTarget(for: decoded) else { return nil }

        let relative = relativePath(from: context.newFolder, to: target)
        return (angled ? relative : encoded(relative)) + fragment
    }

    /// The new inside of `![[ … ]]`, or nil to leave it. Only embeds written
    /// with a folder change; a bare name is found wherever the file is.
    private static func rewrittenEmbed(_ inner: String, _ context: Context) -> String? {
        let cut = inner.firstIndex(where: { $0 == "|" || $0 == "#" }) ?? inner.endIndex
        let name = inner[..<cut].trimmingCharacters(in: .whitespaces)
        guard name.contains("/"), isRelativeLocal(name),
              let target = context.newTarget(for: name) else { return nil }
        return relativePath(from: context.newFolder, to: target) + inner[cut...]
    }

    private static func isRelativeLocal(_ path: String) -> Bool {
        guard !path.isEmpty, !path.hasPrefix("/"), !path.hasPrefix("~"), !path.hasPrefix("#") else { return false }
        let range = NSRange(location: 0, length: (path as NSString).length)
        return schemePattern.firstMatch(in: path, range: range) == nil
    }

    /// Only what would break the link is encoded, as pasting writes it.
    private static func encoded(_ path: String) -> String {
        var result = ""
        for character in path {
            switch character {
            case "%": result += "%25"
            case " ": result += "%20"
            case "(": result += "%28"
            case ")": result += "%29"
            case "#": result += "%23"
            default: result.append(character)
            }
        }
        return result
    }

    // MARK: - Helpers

    /// Fenced code blocks and inline code spans, where links are only text.
    private static func codeRanges(in ns: NSString) -> [NSRange] {
        var ranges: [NSRange] = []
        var fenceStart: Int?
        var fence = ""
        var location = 0
        while location < ns.length {
            let line = ns.lineRange(for: NSRange(location: location, length: 0))
            let text = ns.substring(with: line).trimmingCharacters(in: .whitespaces)
            if let start = fenceStart {
                if text.hasPrefix(fence) {
                    ranges.append(NSRange(location: start, length: NSMaxRange(line) - start))
                    fenceStart = nil
                }
            } else if text.hasPrefix("```") || text.hasPrefix("~~~") {
                fenceStart = line.location
                fence = String(text.prefix(3))
            }
            location = NSMaxRange(line)
        }
        if let start = fenceStart { ranges.append(NSRange(location: start, length: ns.length - start)) }
        inlineCode.enumerateMatches(in: ns as String, range: NSRange(location: 0, length: ns.length)) { match, _, _ in
            if let match { ranges.append(match.range) }
        }
        return ranges
    }

    private static let inlineCode = try! NSRegularExpression(pattern: #"`[^`\n]+`"#)

    private static func parent(_ path: String) -> String {
        (path as NSString).deletingLastPathComponent
    }

    private static func components(_ path: String) -> [String] {
        path.split(separator: "/", omittingEmptySubsequences: true).map(String.init)
    }
}
