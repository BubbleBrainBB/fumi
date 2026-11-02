import Foundation

/// Extracts a display title from note content.
///
/// Rules:
/// 1. A leading YAML frontmatter block is skipped
/// 2. If the first content line starts with "# ", use the heading text
/// 3. Otherwise use the first line that still has text once Markdown markers
///    are stripped (so an image-only line like `![](https://…)` is skipped)
/// 4. If nothing qualifies, return "" — callers fall back to the file name
/// 5. Truncate to 80 characters
enum TitleExtractor {
    private static let sanitizeRules: [(NSRegularExpression, String)] = [
        (#"!\[[^\]]*\]\([^)]*\)"#, ""),
        (#"^#+\s*"#, ""),
        (#"^\*\s+"#, ""),
        (#"^-\s+"#, ""),
        (#"^\d+\.\s+"#, ""),
        (#"^>\s*"#, ""),
        (#"^[-*_]{3,}$"#, ""),
        (#"\*\*(.+?)\*\*"#, "$1"),
        (#"\*(.+?)\*"#, "$1"),
        (#"~~(.+?)~~"#, "$1"),
        (#"`([^`]+)`"#, "$1"),
        (#"\[([^\]]+)\]\([^)]+\)"#, "$1"),
        (#"#\S+"#, ""),
    ].compactMap { pattern, replacement in
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return nil }
        return (regex, replacement)
    }

    static func extract(from content: String) -> String {
        let lines = content.split(separator: "\n", omittingEmptySubsequences: false)
        var start = 0
        // Skip a frontmatter block (`---` on the first line … closing `---`).
        if let first = lines.first, first.trimmingCharacters(in: .whitespaces) == "---",
           let close = lines.indices.dropFirst().first(where: {
               let t = lines[$0].trimmingCharacters(in: .whitespaces)
               return t == "---" || t == "..."
           }) {
            start = close + 1
        }

        for line in lines[start...] {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty || trimmed == "---" { continue }

            let candidate = trimmed.hasPrefix("# ")
                ? String(trimmed.dropFirst(2)).trimmingCharacters(in: .whitespaces)
                : trimmed
            let title = sanitize(candidate)
            if !title.isEmpty { return title }
        }
        return ""
    }

    static var untitled: String { "Untitled" }

    private static func sanitize(_ text: String) -> String {
        var result = text

        for (regex, replacement) in sanitizeRules {
            let range = NSRange(location: 0, length: (result as NSString).length)
            result = regex.stringByReplacingMatches(in: result, range: range, withTemplate: replacement)
        }

        result = result.trimmingCharacters(in: .whitespaces)

        // Truncate to 80 characters
        if result.count > 80 {
            let endIndex = result.index(result.startIndex, offsetBy: 80)
            result = String(result[..<endIndex]) + "..."
        }

        return result
    }
}
