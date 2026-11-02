import Foundation

/// A note's leading YAML frontmatter block (`---` … `---`), parsed just enough
/// to render it as a properties panel. Only the flat shapes notes actually use
/// are understood: `key: value`, `key: [a, b]` and `key:` followed by `- item`
/// lines; anything else is shown as its raw text.
struct Frontmatter: Equatable {
    enum Value: Equatable {
        case text(String)
        case list([String])
    }

    struct Field: Equatable {
        let key: String
        let value: Value
    }

    /// Character range of the whole block in the source, from the opening
    /// `---` through the end of the closing `---` line (its newline excluded).
    let range: NSRange
    let fields: [Field]

    /// Finds and parses a frontmatter block at the very start of `string`.
    static func parse(_ string: NSString) -> Frontmatter? {
        guard string.length >= 3, string.hasPrefix("---") else { return nil }
        var lines: [(range: NSRange, text: String)] = []
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length), options: [.byLines]) { sub, range, _, stop in
            lines.append((range, sub ?? ""))
            if lines.count > 200 { stop.pointee = true }
        }
        guard let first = lines.first, first.text.trimmingCharacters(in: .whitespaces) == "---" else { return nil }
        guard let closeIndex = lines.indices.dropFirst().first(where: {
            let t = lines[$0].text.trimmingCharacters(in: .whitespaces)
            return t == "---" || t == "..."
        }) else { return nil }

        let body = lines[1..<closeIndex].map(\.text)
        let range = NSRange(location: 0, length: NSMaxRange(lines[closeIndex].range))
        return Frontmatter(range: range, fields: parseFields(body))
    }

    static func parseFields(_ lines: [String]) -> [Field] {
        var fields: [Field] = []
        var pendingKey: String? = nil
        var pendingItems: [String] = []

        func flushPending() {
            guard let key = pendingKey else { return }
            fields.append(Field(key: key, value: pendingItems.isEmpty ? .text("") : .list(pendingItems)))
            pendingKey = nil
            pendingItems = []
        }

        for raw in lines {
            let trimmed = raw.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty || trimmed.hasPrefix("#") { continue }

            if pendingKey != nil, trimmed.hasPrefix("- ") || trimmed == "-" {
                let item = unquote(String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces))
                if !item.isEmpty { pendingItems.append(item) }
                continue
            }
            flushPending()

            guard let colon = trimmed.firstIndex(of: ":") else { continue }
            let key = String(trimmed[..<colon]).trimmingCharacters(in: .whitespaces)
            let value = String(trimmed[trimmed.index(after: colon)...]).trimmingCharacters(in: .whitespaces)
            guard !key.isEmpty else { continue }

            if value.isEmpty {
                pendingKey = key
            } else if value.hasPrefix("["), value.hasSuffix("]") {
                let items = value.dropFirst().dropLast()
                    .split(separator: ",")
                    .map { unquote($0.trimmingCharacters(in: .whitespaces)) }
                    .filter { !$0.isEmpty }
                fields.append(Field(key: key, value: .list(items)))
            } else {
                fields.append(Field(key: key, value: .text(unquote(value))))
            }
        }
        flushPending()
        return fields
    }

    private static func unquote(_ s: String) -> String {
        guard s.count >= 2, let f = s.first, let l = s.last, f == l, f == "\"" || f == "'" else { return s }
        return String(s.dropFirst().dropLast())
    }
}
