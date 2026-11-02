import Foundation

/// One memo (design board ㉛): a short, untitled entry. Memos live one file
/// per day — `Memos/2026-10-16.md` — each under a `## 09:31` heading, so the
/// day's file reads as a plain journal in any Markdown app.
struct Memo: Identifiable, Hashable {
    /// The day's file name without extension, "2026-10-16".
    let day: String
    /// Position among the memos of that day's file.
    let index: Int
    /// "09:31".
    let time: String
    let text: String

    var id: String { "\(day)#\(index)" }

    /// `#tags` in the text, without the `#`.
    var tags: [String] { MemoFormat.tags(in: text) }
}

enum MemoFormat {
    /// The folder, inside the notes library, that holds the day files.
    static let folderName = "Memos"

    // MARK: Days and times

    private static func formatter(_ format: String) -> DateFormatter {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.timeZone = .current
        formatter.dateFormat = format
        return formatter
    }

    private static let dayFormatter = formatter("yyyy-MM-dd")
    private static let timeFormatter = formatter("HH:mm")

    static func day(of date: Date) -> String { dayFormatter.string(from: date) }
    static func time(of date: Date) -> String { timeFormatter.string(from: date) }
    static func date(ofDay day: String) -> Date? { dayFormatter.date(from: day) }

    /// Whether a file name (without extension) names a day file.
    static func isDay(_ name: String) -> Bool {
        name.range(of: #"^\d{4}-\d{2}-\d{2}$"#, options: .regularExpression) != nil
    }

    // MARK: Parsing

    private static let headingPattern = try! NSRegularExpression(pattern: #"^##[ \t]+(\d{1,2}:\d{2})[ \t]*$"#)

    /// The time in a memo heading line (`## 09:31`), or nil.
    private static func headingTime(_ line: Substring) -> String? {
        let s = String(line)
        let range = NSRange(location: 0, length: (s as NSString).length)
        guard let match = headingPattern.firstMatch(in: s, range: range) else { return nil }
        return (s as NSString).substring(with: match.range(at: 1))
    }

    /// Line spans of each memo: its heading line and the body lines after it.
    private struct Block {
        let time: String
        let heading: Int
        let bodyStart: Int
        /// One past the last line (exclusive).
        let end: Int
    }

    private static func blocks(in lines: [Substring]) -> [Block] {
        var heads: [(line: Int, time: String)] = []
        var inFence = false
        for (i, line) in lines.enumerated() {
            if line.hasPrefix("```") || line.hasPrefix("~~~") { inFence.toggle() }
            if !inFence, let time = headingTime(line) { heads.append((i, time)) }
        }
        return heads.enumerated().map { n, head in
            Block(time: head.time, heading: head.line, bodyStart: head.line + 1,
                  end: n + 1 < heads.count ? heads[n + 1].line : lines.count)
        }
    }

    private static func lines(_ content: String) -> [Substring] {
        content.split(separator: "\n", omittingEmptySubsequences: false)
    }

    private static func trimmedBody(_ body: ArraySlice<Substring>) -> String {
        body.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// The memos in a day file, in file order. Text before the first memo
    /// heading (a daily note's own content) is not a memo.
    static func parse(_ content: String, day: String) -> [Memo] {
        let lines = lines(content)
        return blocks(in: lines).enumerated().compactMap { index, block in
            let text = trimmedBody(lines[block.bodyStart..<block.end])
            guard !text.isEmpty else { return nil }
            return Memo(day: day, index: index, time: block.time, text: text)
        }
    }

    // MARK: Writing

    /// `content` with a new memo added at the end.
    static func appending(_ text: String, at time: String, to content: String) -> String {
        let entry = "## \(time)\n\(text.trimmingCharacters(in: .whitespacesAndNewlines))\n"
        let existing = content.trimmingCharacters(in: .newlines)
        return existing.isEmpty ? entry : existing + "\n\n" + entry
    }

    /// `content` with memo number `index` given new text.
    static func replacing(memo index: Int, with text: String, in content: String) -> String {
        var lines = lines(content)
        let all = blocks(in: lines)
        guard all.indices.contains(index) else { return content }
        let block = all[index]
        var body = text.trimmingCharacters(in: .whitespacesAndNewlines)
            .split(separator: "\n", omittingEmptySubsequences: false)
        // Keep the gap before the next memo, or the file's final newline.
        if block.end < lines.count || content.hasSuffix("\n") { body.append("") }
        lines.replaceSubrange(block.bodyStart..<block.end, with: body)
        return lines.joined(separator: "\n")
    }

    /// `content` without memo number `index`.
    static func removing(memo index: Int, from content: String) -> String {
        var lines = lines(content)
        let all = blocks(in: lines)
        guard all.indices.contains(index) else { return content }
        lines.removeSubrange(all[index].heading..<all[index].end)
        let joined = lines.joined(separator: "\n").trimmingCharacters(in: .newlines)
        return joined.isEmpty ? "" : joined + "\n"
    }

    // MARK: Tags

    private static let tagPattern = try! NSRegularExpression(pattern: #"(?<=^|[\s(\[])#([\p{L}][\p{L}\p{N}_/-]*)"#)

    /// `#tags` in `text`: names and their ranges (including the `#`).
    static func tagRanges(in text: String) -> [(name: String, range: NSRange)] {
        let ns = text as NSString
        return tagPattern.matches(in: text, range: NSRange(location: 0, length: ns.length)).map {
            (ns.substring(with: $0.range(at: 1)), $0.range)
        }
    }

    static func tags(in text: String) -> [String] { tagRanges(in: text).map(\.name) }
}

/// The heatmap's grid: weeks as columns (Monday first), days as rows.
enum MemoHeatmap {
    /// Shade for a day with `count` memos: 0 (none) … 4 (most).
    static func level(for count: Int) -> Int {
        switch count {
        case ..<1: return 0
        case 1: return 1
        case 2: return 2
        case 3...4: return 3
        default: return 4
        }
    }

    /// `weeks` columns of seven days, the last one holding `today`. Days
    /// after today are nil.
    static func grid(weeks: Int, endingAt today: Date, calendar: Calendar = .current) -> [[Date?]] {
        var calendar = calendar
        calendar.firstWeekday = 2 // Monday
        let start = calendar.startOfDay(for: today)
        let weekday = (calendar.component(.weekday, from: start) + 5) % 7 // Monday = 0
        guard let monday = calendar.date(byAdding: .day, value: -weekday, to: start),
              let first = calendar.date(byAdding: .weekOfYear, value: -(weeks - 1), to: monday) else { return [] }
        return (0..<weeks).map { week in
            (0..<7).map { day in
                guard let date = calendar.date(byAdding: .day, value: week * 7 + day, to: first) else { return nil }
                return date > start ? nil : date
            }
        }
    }

    /// The month holding `date` as weeks of seven days, Monday first; days
    /// outside the month are nil.
    static func month(containing date: Date, calendar: Calendar = .current) -> [[Date?]] {
        var calendar = calendar
        calendar.firstWeekday = 2 // Monday
        guard let interval = calendar.dateInterval(of: .month, for: date),
              let days = calendar.range(of: .day, in: .month, for: date) else { return [] }
        let lead = (calendar.component(.weekday, from: interval.start) + 5) % 7 // Monday = 0
        var cells: [Date?] = Array(repeating: nil, count: lead)
        for offset in 0..<days.count {
            cells.append(calendar.date(byAdding: .day, value: offset, to: interval.start))
        }
        while cells.count % 7 != 0 { cells.append(nil) }
        return stride(from: 0, to: cells.count, by: 7).map { Array(cells[$0..<$0 + 7]) }
    }
}

/// File access for memos, shared by the app window and the quick-capture
/// panel (which writes without a window or store).
enum MemoFile {
    static func folder(in workspace: URL) -> URL {
        workspace.appendingPathComponent(MemoFormat.folderName, isDirectory: true)
    }

    static func url(forDay day: String, in workspace: URL) -> URL {
        folder(in: workspace).appendingPathComponent(day + ".md")
    }

    /// Adds a memo to the day file for `date`, creating the folder and file
    /// as needed. Returns the file it wrote.
    @discardableResult
    static func append(_ text: String, at date: Date = Date(), in workspace: URL) throws -> URL {
        let url = url(forDay: MemoFormat.day(of: date), in: workspace)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let existing = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
        try MemoFormat.appending(text, at: MemoFormat.time(of: date), to: existing)
            .write(to: url, atomically: true, encoding: .utf8)
        return url
    }
}
