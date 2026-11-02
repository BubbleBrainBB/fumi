import Foundation

/// Extracts hashtag tags from Markdown content.
///
/// Matches patterns like #work, #work/project, #life/reading.
/// Tags must be preceded by start-of-line, whitespace, or certain
/// punctuation to avoid false positives (e.g., C# code, URLs).
enum TagExtractor {
    /// Regular expression for matching hashtags.
    /// - Must start with # followed by word characters and optional / paths
    /// - Must be preceded by line start, space, or certain punctuation
    static let pattern = #"(?<=^|[\s(])#([\w/-]+)"#

    private static let regex: NSRegularExpression = {
        // A tag must be preceded by start-of-line or whitespace ONLY. Allowing
        // `(` / `[` previously turned SVG/CSS references like `url(#filter0_d…)`
        // and `(#FFFFFF)` into tags.
        try! NSRegularExpression(
            pattern: #"(?m)(?<=^|\s)(#[A-Za-z][\w/-]*)"#,
            options: []
        )
    }()

    /// Extract raw tag strings (including #) from content.
    static func extractRaw(from content: String) -> [String] {
        // Blank out code blocks / inline code first so hex colours and other
        // `#…` tokens inside ```fences``` or `code` are not picked up as tags.
        let masked = maskingCode(content)
        let nsContent = masked as NSString
        let range = NSRange(location: 0, length: nsContent.length)
        var tags: [String] = []

        regex.enumerateMatches(in: masked, range: range) { match, _, _ in
            guard let match = match else { return }
            let tag = nsContent.substring(with: match.range(at: 1))
            // Ignore hex colour codes (e.g. #C7C7C7, #FFFFFF) — these are not tags.
            if tag.isEmpty || isHexColor(tag) { return }
            tags.append(tag)
        }

        // Deduplicate while preserving order
        var seen = Set<String>()
        return tags.filter { seen.insert($0).inserted }
    }

    // MARK: - Frontmatter tags

    /// Tags declared in the note's YAML frontmatter `tags:` field — the sole
    /// source of a note's tags. Inline `#hashtags` in the body are intentionally
    /// ignored (they are often quoted/embedded content, not the author's tags).
    /// Supports block sequences (`- a`), flow lists (`[a, b]`), and comma or
    /// single scalar values, with optional quotes and a leading `#`.
    static func frontmatterTags(from content: String) -> [String] {
        guard let body = frontmatterBody(content) else { return [] }
        var seen = Set<String>()
        return tagValues(inFrontmatterBody: body)
            .map(cleanTag)
            .filter { !$0.isEmpty && seen.insert($0).inserted }
    }

    /// The text between the opening `---` and the next `---`/`...`, or nil if the
    /// content has no leading frontmatter block.
    private static func frontmatterBody(_ content: String) -> String? {
        let ns = content as NSString
        var lines: [String] = []
        ns.enumerateSubstrings(in: NSRange(location: 0, length: ns.length), options: [.byLines]) { sub, _, _, _ in
            lines.append(sub ?? "")
        }
        guard let first = lines.first, first.trimmingCharacters(in: .whitespaces) == "---" else { return nil }
        for i in 1..<lines.count {
            let trimmed = lines[i].trimmingCharacters(in: .whitespaces)
            if trimmed == "---" || trimmed == "..." {
                return lines[1..<i].joined(separator: "\n")
            }
        }
        return nil
    }

    private static func tagValues(inFrontmatterBody body: String) -> [String] {
        let lines = body.components(separatedBy: "\n")
        var i = 0
        while i < lines.count {
            guard let inlineValue = topLevelTagsValue(lines[i]) else { i += 1; continue }
            let inline = inlineValue.trimmingCharacters(in: .whitespaces)
            if !inline.isEmpty {
                return parseFlowOrScalar(inline)
            }
            // Block sequence: subsequent `- item` lines.
            var items: [String] = []
            var j = i + 1
            while j < lines.count {
                let trimmed = lines[j].trimmingCharacters(in: .whitespaces)
                if trimmed.isEmpty { j += 1; continue }
                guard trimmed.hasPrefix("-") else { break }   // next key / dedent ends the list
                items.append(String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces))
                j += 1
            }
            return items
        }
        return []
    }

    /// If `line` is the top-level `tags:` key, return its inline value (possibly empty).
    private static func topLevelTagsValue(_ line: String) -> String? {
        guard let match = line.range(of: #"^tags[ \t]*:[ \t]?"#, options: .regularExpression) else { return nil }
        return String(line[match.upperBound...])
    }

    private static func parseFlowOrScalar(_ value: String) -> [String] {
        var v = value
        if v.hasPrefix("["), v.hasSuffix("]") { v = String(v.dropFirst().dropLast()) }
        if v.contains(",") { return v.components(separatedBy: ",") }
        return [v]
    }

    private static func cleanTag(_ raw: String) -> String {
        var s = raw.trimmingCharacters(in: .whitespaces)
        for quote in ["\"", "'"] where s.hasPrefix(quote) && s.hasSuffix(quote) && s.count >= 2 {
            s = String(s.dropFirst().dropLast())
        }
        if s.hasPrefix("#") { s = String(s.dropFirst()) }
        return s.trimmingCharacters(in: .whitespaces)
    }

    /// A `#RRGGBB` / `#RRGGBBAA` hex colour (6 or 8 hex digits). Lengths 3/4 are
    /// intentionally NOT treated as colours because short words like #ace or
    /// #cafe collide with them.
    static func isHexColor(_ tag: String) -> Bool {
        let body = tag.hasPrefix("#") ? String(tag.dropFirst()) : tag
        guard body.count == 6 || body.count == 8 else { return false }
        return body.allSatisfy(\.isHexDigit)
    }

    /// Returns `content` with fenced code blocks and inline code spans replaced
    /// by spaces (newlines preserved, so line/character offsets are unchanged).
    private static func maskingCode(_ content: String) -> String {
        let mutable = NSMutableString(string: content)
        let ns = content as NSString

        // Fenced code blocks (``` or ~~~), line by line.
        var fenceStart = -1
        var fencedRanges: [NSRange] = []
        ns.enumerateSubstrings(in: NSRange(location: 0, length: ns.length), options: [.byLines]) { sub, range, _, _ in
            let trimmed = (sub ?? "").trimmingCharacters(in: .whitespaces)
            guard trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") else { return }
            if fenceStart < 0 {
                fenceStart = range.location
            } else {
                fencedRanges.append(NSRange(location: fenceStart, length: NSMaxRange(range) - fenceStart))
                fenceStart = -1
            }
        }
        if fenceStart >= 0 {
            fencedRanges.append(NSRange(location: fenceStart, length: ns.length - fenceStart))
        }
        for range in fencedRanges.reversed() { blank(mutable, range) }

        // HTML/CSS/JS (web clippings embed raw SVG/HTML whose id refs — e.g.
        // url(#filter0_d…), href="#…", <style>#id{}</style> — must not become
        // tags) and inline code. Order matters: blank style/script bodies before
        // generic tags, inline code last.
        let maskPatterns = [
            #"(?is)<style\b[\s\S]*?</style>"#,
            #"(?is)<script\b[\s\S]*?</script>"#,
            #"<!--[\s\S]*?-->"#,
            #"</?[A-Za-z][^>]*>"#,
            #"`[^`\n]+`"#,
        ]
        for pattern in maskPatterns {
            guard let rx = try? NSRegularExpression(pattern: pattern) else { continue }
            let masked = mutable as String
            let matches = rx.matches(in: masked, range: NSRange(location: 0, length: mutable.length))
            for match in matches.reversed() { blank(mutable, match.range) }
        }

        return mutable as String
    }

    private static func blank(_ string: NSMutableString, _ range: NSRange) {
        let substring = string.substring(with: range)
        let replacement = String(substring.map { ($0 == "\n" || $0 == "\r") ? $0 : " " })
        string.replaceCharacters(in: range, with: replacement)
    }

    /// Extract tag paths (without # prefix) from content.
    static func extract(from content: String) -> [String] {
        extractRaw(from: content).map { String($0.dropFirst()) }
    }
}

/// Represents a node in the tag tree.
struct TagNode: Identifiable, Hashable {
    let id: String          // full path, e.g. "work/project"
    let name: String        // display name, e.g. "project"
    let level: Int          // depth in the tree
    var noteCount: Int      // direct note count for this exact tag
    var totalCount: Int     // cumulative count including children
    var children: [TagNode]
}

/// Builds a hierarchical tag tree from a flat list of tag paths.
enum TagTreeBuilder {
    /// Build tree from tag paths. Each path may contain "/" for nesting.
    static func build(from tagPaths: [String]) -> [TagNode] {
        var rootChildren: [String: TagNode] = [:]

        for path in tagPaths {
            let parts = path.split(separator: "/").map(String.init)
            buildNode(at: parts, into: &rootChildren)
        }

        return rootChildren.values.sorted { $0.name < $1.name }
    }

    private static func buildNode(at parts: [String], into siblings: inout [String: TagNode], parentPath: String = "") {
        guard let firstPart = parts.first else { return }

        let currentPath = parentPath.isEmpty ? firstPart : "\(parentPath)/\(firstPart)"
        let fullPath = parentPath.isEmpty ? firstPart : "\(parentPath)/\(firstPart)"

        var node = siblings[firstPart] ?? TagNode(
            id: fullPath,
            name: firstPart,
            level: parentPath.split(separator: "/").count,
            noteCount: 0,
            totalCount: 0,
            children: []
        )

        if parts.count == 1 {
            node.noteCount += 1
        }

        if parts.count > 1 {
            var childDict = Dictionary(uniqueKeysWithValues: node.children.map { ($0.name, $0) })
            buildNode(at: Array(parts.dropFirst()), into: &childDict, parentPath: currentPath)
            node.children = childDict.values.sorted { $0.name < $1.name }
        }

        node.totalCount += 1
        siblings[firstPart] = node
    }
}
