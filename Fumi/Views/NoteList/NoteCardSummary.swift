import Foundation

/// Computes the one-line preview shown under a note's title in the list.
///
/// The title now shows the file name, so the preview must show the note's first
/// line of *content* — but a note commonly opens with a YAML frontmatter block
/// (`--- … ---`), whose delimiter is not content. This skips a leading
/// frontmatter block and returns the first non-empty line after it.
///
/// Extracted from `NoteCardView` as a pure function so it can be unit tested
/// without SwiftUI. Returns `nil` when there is no displayable content.
enum NoteCardSummary {
    static func firstContentLine(of content: String) -> String? {
        let lines = content.components(separatedBy: "\n")
        var index = firstNonBlank(in: lines, from: 0)

        // Skip a leading YAML frontmatter block, but only when it is actually
        // closed by a second `---`. An unterminated leading `---` is treated as
        // ordinary content (e.g. a stray horizontal rule).
        if index < lines.count, isDelimiter(lines[index]) {
            var j = index + 1
            while j < lines.count {
                if isDelimiter(lines[j]) {
                    index = firstNonBlank(in: lines, from: j + 1)
                    break
                }
                j += 1
            }
        }

        guard index < lines.count else { return nil }
        return lines[index].trimmingCharacters(in: .whitespaces)
    }

    private static func isDelimiter(_ line: String) -> Bool {
        line.trimmingCharacters(in: .whitespaces) == "---"
    }

    /// Index of the first line that is not blank, starting at `start`; returns
    /// `lines.count` when every line from `start` on is blank.
    private static func firstNonBlank(in lines: [String], from start: Int) -> Int {
        var i = start
        while i < lines.count, lines[i].trimmingCharacters(in: .whitespaces).isEmpty {
            i += 1
        }
        return i
    }
}
