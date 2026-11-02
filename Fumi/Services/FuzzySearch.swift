import Foundation

// MARK: - Single-string fuzzy matching

/// One contiguous run of matched characters, expressed in `String.Index` so
/// callers can slice and highlight without re-deriving offsets.
struct FuzzyRange: Equatable {
    let start: String.Index
    let end: String.Index
}

/// A fuzzy match of a query inside one string: the query's characters appear
/// in order (a subsequence), case-insensitively, anywhere in the haystack.
struct FuzzyMatch {
    /// Higher is better. Early, word-boundary and consecutive matches score
    /// above scattered late ones.
    let score: Int
    /// Offsets (into the haystack) of the matched characters, ascending.
    let offsets: [Int]

    /// Matches `query` against `haystack` as a case-insensitive subsequence,
    /// or returns nil when some query character is missing.
    ///
    /// Greedy in both directions — each query character takes the earliest
    /// (forward) or latest (backward) haystack character that can still
    /// complete the query — and the better-scoring alignment wins. Forward
    /// greedy alone can split an obvious run (" talked" + "Kyoto" for
    /// "kyoto"); the backward pass anchors late matches tightly.
    static func match(_ query: String, in haystack: String) -> FuzzyMatch? {
        let q = Array(query.lowercased())
        let h = Array(haystack.lowercased())
        guard !q.isEmpty, q.count <= h.count else { return nil }

        guard let forward = greedyMatch(q, in: h) else { return nil }
        if let backward = reverseGreedyMatch(q, in: h), backward.score > forward.score {
            return backward
        }
        return forward
    }

    /// Forward greedy: earliest completion. Biased toward matches that start
    /// early — what "fuzzy" search usually wants.
    private static func greedyMatch(_ q: [Character], in h: [Character]) -> FuzzyMatch? {
        var offsets: [Int] = []
        var hi = 0
        for qc in q {
            var found = false
            while hi < h.count {
                if h[hi] == qc {
                    offsets.append(hi)
                    hi += 1
                    found = true
                    break
                }
                hi += 1
            }
            guard found else { return nil }
        }
        return FuzzyMatch(score: score(of: offsets, in: h), offsets: offsets)
    }

    /// Backward greedy: latest completion. Finds the tight alignment when the
    /// match sits late in the string. Offsets come out descending; reversed
    /// before scoring so callers always see ascending order.
    private static func reverseGreedyMatch(_ q: [Character], in h: [Character]) -> FuzzyMatch? {
        var offsets: [Int] = []
        var hi = h.count - 1
        for qc in q.reversed() {
            var found = false
            while hi >= 0 {
                if h[hi] == qc {
                    offsets.append(hi)
                    hi -= 1
                    found = true
                    break
                }
                hi -= 1
            }
            guard found else { return nil }
        }
        let ascending = Array(offsets.reversed())
        return FuzzyMatch(score: score(of: ascending, in: h), offsets: ascending)
    }

    /// A literal (contiguous) match of `length` characters at `start`,
    /// scored like a fuzzy one.
    static func contiguous(at start: Int, length: Int, in haystack: String) -> FuzzyMatch {
        let offsets = Array(start..<(start + length))
        return FuzzyMatch(score: score(of: offsets, in: Array(haystack.lowercased())), offsets: offsets)
    }

    /// The matched offsets as sliceable ranges of `haystack`.
    func ranges(in haystack: String) -> [FuzzyRange] {
        var result: [FuzzyRange] = []
        var i = 0
        while i < offsets.count {
            var j = i
            while j + 1 < offsets.count, offsets[j + 1] == offsets[j] + 1 { j += 1 }
            let start = haystack.index(haystack.startIndex, offsetBy: offsets[i])
            let end = haystack.index(haystack.startIndex, offsetBy: offsets[j] + 1)
            result.append(FuzzyRange(start: start, end: end))
            i = j + 1
        }
        return result
    }

    /// Sublime-style scoring: a point per matched character, a bonus for
    /// starting the string or a word, a bigger bonus for consecutive runs,
    /// and a capped penalty for the gap skipped since the previous match.
    private static func score(of offsets: [Int], in h: [Character]) -> Int {
        var score = 0
        for (j, i) in offsets.enumerated() {
            score += 1
            if i == 0 {
                score += 8                                  // starts the string
            } else if isWordBoundary(h[i - 1], h[i]) {
                score += 6                                  // starts a word
            }
            if j > 0 {
                if offsets[j - 1] == i - 1 { score += 5 }   // consecutive run
                score -= min(i - offsets[j - 1] - 1, 6)     // gap penalty, capped
            }
        }
        return score
    }

    /// Separators, plus case transitions (camelCase / PascalCase).
    private static func isWordBoundary(_ prev: Character, _ cur: Character) -> Bool {
        if separators.contains(prev) { return true }
        return prev.isLowercase && cur.isUppercase
    }

    private static let separators: Set<Character> = [" ", "_", "-", ".", "/", "(", "[", "{", "#", "、", "，"]
}

// MARK: - Note-level search

/// Which field of a note produced a hit — decides what the result row shows
/// as its second line.
enum NoteMatchField {
    case fileName
    case title
    case folder
    case tag
    case content
    /// Synthesized for the popover's empty-query "recent notes" list.
    case recent
}

/// A short excerpt of a note body, windowed around the match.
struct Snippet {
    let text: String
    let ranges: [FuzzyRange]
}

/// One search result: the note, where it matched, and what to highlight.
struct NoteSearchHit {
    let note: FileNote
    let score: Int
    let field: NoteMatchField
    /// Ranges to highlight in the row's first line (the file name).
    let titleRanges: [FuzzyRange]
    /// The row's second line: the matched title/tag, or the folder path.
    let subtitle: String
    /// Ranges to highlight in `subtitle`.
    let subtitleRanges: [FuzzyRange]
    /// Body excerpt, when the match is in the note's content.
    let snippet: Snippet?

    /// A hit for the empty-query "recent notes" list: no highlights.
    static func recent(_ note: FileNote) -> NoteSearchHit {
        NoteSearchHit(note: note, score: 0, field: .recent, titleRanges: [],
                      subtitle: note.folderDisplayName, subtitleRanges: [], snippet: nil)
    }
}

extension FileNote {
    /// Folder path for display: "Notes" at the workspace root.
    var folderDisplayName: String {
        relativePath == "/" ? "Notes" : String(relativePath.dropFirst())
    }
}

/// Note bodies prepared once for fast body search: folded for case and
/// width, as `NSString`s so each lookup is a plain UTF-16 scan. Building it
/// takes ~50 ms for a 4M-character vault; a search over it, under 10 ms.
/// Matching the raw Swift strings case-insensitively took 400–700 ms.
final class SearchCorpus {
    let notes: [FileNote]
    fileprivate let bodies: [NSString]
    fileprivate let foldedBodies: [NSString]

    init(_ notes: [FileNote]) {
        self.notes = notes
        bodies = notes.map { NSString(string: $0.content) }
        foldedBodies = notes.map { NSString(string: FuzzySearch.fold($0.content)) }
    }
}

enum FuzzySearch {
    /// Case and width folding for body search (Ａ→a, ＡＩ→ai).
    static func fold(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .widthInsensitive], locale: nil)
    }

    /// Ranks `notes` for `query`, best first.
    ///
    /// A query starting with `#` searches tags instead: a tag matches when
    /// the rest of the query is a subsequence of its full path (`#aisc` finds
    /// `AI/Swift`). Otherwise the query is matched against the file name,
    /// the extracted title, the folder path and the body — in that order of
    /// weight — and each note keeps only its best field.
    static func hits(_ notes: [FileNote], query: String, limit: Int = 100) -> [NoteSearchHit] {
        hits(in: SearchCorpus(notes), query: query, limit: limit)
    }

    /// `hits(_:query:limit:)` over a prepared corpus — reuse one corpus for
    /// every keystroke while the search panel is open.
    static func hits(in corpus: SearchCorpus, query: String, limit: Int = 100) -> [NoteSearchHit] {
        let notes = corpus.notes
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty else { return [] }

        if q.hasPrefix("#") {
            let tagQuery = q.dropFirst().trimmingCharacters(in: .whitespaces)
            guard !tagQuery.isEmpty else { return [] }
            var hits: [NoteSearchHit] = []
            for note in notes {
                if let (tag, match) = bestTagMatch(in: note.tags, query: tagQuery) {
                    hits.append(NoteSearchHit(note: note, score: match.score, field: .tag,
                                              titleRanges: [], subtitle: tag,
                                              subtitleRanges: match.ranges(in: tag), snippet: nil))
                }
            }
            return Array(sorted(hits).prefix(limit))
        }

        let foldedQuery = fold(q)
        var hits: [NoteSearchHit] = []
        for (index, note) in notes.enumerated() {
            if let hit = bestHit(for: note, query: q,
                                 body: { contentMatch(corpus.bodies[index], folded: corpus.foldedBodies[index],
                                                      query: q, foldedQuery: foldedQuery) }) {
                hits.append(hit)
            }
        }
        // Rank first, then cap — capping first dropped the best matches
        // whenever more than `limit` notes matched.
        return Array(sorted(hits).prefix(limit))
    }

    /// Notes for the popover's empty state: pinned first, then recently updated.
    static func recent(_ notes: [FileNote], limit: Int) -> [FileNote] {
        Array(notes.sorted { lhs, rhs in
            if lhs.pinned != rhs.pinned { return lhs.pinned }
            return lhs.updatedAt > rhs.updatedAt
        }.prefix(limit))
    }

    // MARK: Ranking

    /// Score dominates; pinned and recency only break ties.
    private static func sorted(_ hits: [NoteSearchHit]) -> [NoteSearchHit] {
        hits.sorted { lhs, rhs in
            if lhs.score != rhs.score { return lhs.score > rhs.score }
            if lhs.note.pinned != rhs.note.pinned { return lhs.note.pinned }
            return lhs.note.updatedAt > rhs.note.updatedAt
        }
    }

    /// Field weights: the file name is what users aim at, the title and tags
    /// are nearly as good, the folder path narrows by location, and a body
    /// match is the weakest signal.
    private static func bestHit(for note: FileNote, query: String,
                                body: () -> (line: String, match: FuzzyMatch)?) -> NoteSearchHit? {
        if let m = FuzzyMatch.match(query, in: note.filename) {
            return NoteSearchHit(note: note, score: Int(1.00 * Double(m.score)), field: .fileName,
                                 titleRanges: m.ranges(in: note.filename), subtitle: note.folderDisplayName,
                                 subtitleRanges: [], snippet: nil)
        }
        if !note.title.isEmpty, let m = FuzzyMatch.match(query, in: note.title) {
            return NoteSearchHit(note: note, score: Int(0.95 * Double(m.score)), field: .title,
                                 titleRanges: [], subtitle: note.title,
                                 subtitleRanges: m.ranges(in: note.title), snippet: nil)
        }
        if note.relativePath != "/", let m = FuzzyMatch.match(query, in: note.folderDisplayName) {
            return NoteSearchHit(note: note, score: Int(0.75 * Double(m.score)), field: .folder,
                                 titleRanges: [], subtitle: note.folderDisplayName,
                                 subtitleRanges: m.ranges(in: note.folderDisplayName), snippet: nil)
        }
        if let (line, m) = body() {
            return NoteSearchHit(note: note, score: Int(0.55 * Double(m.score)), field: .content,
                                 titleRanges: [], subtitle: note.folderDisplayName,
                                 subtitleRanges: [], snippet: snippet(from: line, match: m))
        }
        return nil
    }

    /// The best-scoring tag for `query`, or nil when no tag matches.
    private static func bestTagMatch(in tags: [String], query: String) -> (tag: String, match: FuzzyMatch)? {
        var best: (tag: String, match: FuzzyMatch)?
        for tag in tags {
            if let m = FuzzyMatch.match(query, in: tag), best == nil || m.score > best!.match.score {
                best = (tag, m)
            }
        }
        return best
    }

    /// The body line holding `query`, or nil.
    ///
    /// Bodies are matched literally (ignoring case and width), not as a
    /// subsequence: across a whole note nearly any short query is a
    /// subsequence of some line — "zqx" matched 67 of 499 notes.
    private static func contentMatch(_ body: NSString, folded: NSString, query: String,
                                      foldedQuery: String) -> (line: String, match: FuzzyMatch)? {
        var hit = folded.range(of: foldedQuery, options: .literal)
        guard hit.location != NSNotFound else { return nil }
        // Folding kept the length (the common case), so the offsets carry
        // over; otherwise locate the match in the original text directly.
        if folded.length != body.length {
            hit = body.range(of: query, options: [.caseInsensitive, .widthInsensitive])
            guard hit.location != NSNotFound else { return nil }
        }

        // The hit's line, cut to a window around it: a paragraph can run to
        // thousands of characters and the snippet shows a few dozen. The
        // margins exceed the snippet's own, so its ellipses still mark cuts.
        var line = body.lineRange(for: hit)
        while line.length > 0, NSMaxRange(line) > NSMaxRange(hit),
              let last = UnicodeScalar(body.character(at: NSMaxRange(line) - 1)),
              CharacterSet.newlines.contains(last) {
            line.length -= 1
        }
        let start = max(line.location, hit.location - 120)
        let end = min(NSMaxRange(line), NSMaxRange(hit) + 200)
        let window = body.rangeOfComposedCharacterSequences(for: NSRange(location: start, length: end - start))

        let text = body.substring(with: window)
        let offset = body.substring(with: NSRange(location: window.location, length: hit.location - window.location)).count
        let length = body.substring(with: hit).count
        return (text, FuzzyMatch.contiguous(at: offset, length: length, in: text))
    }

    // MARK: Snippets

    /// Wraps a matched body line into a short excerpt: leading Markdown noise
    /// (`#`, `>`, `- `) is dropped, and long lines are windowed around the
    /// match with ellipses at the cut.
    static func snippet(from line: String, match: FuzzyMatch) -> Snippet {
        let chars = Array(line)
        // Drop leading Markdown markers, but never cut into the match itself.
        let skip = min(markdownSkip(chars), match.offsets.first ?? chars.count)

        let shifted = match.offsets.map { $0 - skip }
        let body = Array(String(chars[skip...]))
        var start = max(0, (shifted.first ?? 0) - 24)
        var end = min(body.count, (shifted.last ?? 0) + 48)
        // Ellipses mark a real cut, not whitespace trimmed at the window's
        // edge — so decide before trimming.
        let cutAtStart = start > 0
        let cutAtEnd = end < body.count
        if start == 0 {
            while start < end, body[start].isWhitespace { start += 1 }
        }
        if end == body.count {
            while end > start, body[end - 1].isWhitespace { end -= 1 }
        }

        let prefix = cutAtStart ? "…" : ""
        let text = prefix + String(body[start..<end]) + (cutAtEnd ? "…" : "")

        // Offsets in `text` coordinates: past the window, then past the
        // ellipsis when one was prepended.
        let delta = start - (prefix.isEmpty ? 0 : 1)
        let offsets = shifted.map { $0 - delta }
        var ranges: [FuzzyRange] = []
        var i = 0
        while i < offsets.count {
            var j = i
            while j + 1 < offsets.count, offsets[j + 1] == offsets[j] + 1 { j += 1 }
            ranges.append(FuzzyRange(start: text.index(text.startIndex, offsetBy: offsets[i]),
                                     end: text.index(text.startIndex, offsetBy: offsets[j] + 1)))
            i = j + 1
        }
        return Snippet(text: text, ranges: ranges)
    }

    /// Leading Markdown markers to drop from a body line for display.
    private static func markdownSkip(_ chars: [Character]) -> Int {
        var i = 0
        while i < chars.count, chars[i] == "#" { i += 1 }
        if i > 0 {
            return i < chars.count && chars[i] == " " ? i + 1 : i
        }
        if chars.first == ">" {
            return chars.count > 1 && chars[1] == " " ? 2 : 1
        }
        if let first = chars.first, first == "-" || first == "*" || first == "+" {
            if chars.count > 1, chars[1] == " " { return 2 }
        }
        return 0
    }
}
