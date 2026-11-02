import Foundation

// Standalone test runner for FuzzySearch / FuzzyMatch. No Xcode test target
// exists; FuzzySearch is Foundation-only (it only reads FileNote), so this
// compiles directly with the sources:
//
//     swiftc Fumi/Models/FileNote.swift Fumi/Services/FuzzySearch.swift Tests/FuzzySearchTests.swift -o /tmp/fuzzysearch-tests
//     /tmp/fuzzysearch-tests
//
// Exits non-zero if any assertion fails.

@main
enum FuzzySearchTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    static func note(_ title: String, in folder: String = "/", tags: [String] = [],
                     body: String = "", updated: TimeInterval = 0, pinned: Bool = false) -> FileNote {
        let dir = folder == "/" ? "" : folder
        let url = URL(fileURLWithPath: "/ws\(dir)/\(title).md")
        let stamp = Date(timeIntervalSince1970: updated)
        return FileNote(id: url.path, url: url, title: title, content: body,
                        tags: tags, createdAt: stamp, updatedAt: stamp, pinned: pinned,
                        relativePath: folder)
    }

    /// Slices `haystack` down to the matched characters — asserts both that a
    /// match exists and that its offsets point at the right places.
    static func matchedText(_ query: String, in haystack: String) -> String? {
        guard let m = FuzzyMatch.match(query, in: haystack) else { return nil }
        let chars = Array(haystack)
        return String(m.offsets.map { chars[$0] })
    }

    static func main() {
        // ── Subsequence matching ──
        expect(matchedText("kyo", in: "Kyoto Trip"), "Kyo", "substring is a subsequence")
        expect(matchedText("kt", in: "Kyoto Trip"), "KT", "scattered characters match in order")
        expect(matchedText("KYO", in: "kyoto trip"), "kyo", "case-insensitive on both sides")
        expect(matchedText("bj", in: "CubeJournal"), "bJ", "works across camelCase words")
        expect(matchedText("cj", in: "CubeJournal"), "CJ", "case-insensitive across camelCase")
        expect(matchedText("xyz", in: "Kyoto"), nil, "non-subsequence does not match")
        expect(matchedText("kyotos", in: "Kyoto"), nil, "query longer than haystack does not match")
        expect(matchedText("", in: "Kyoto"), nil, "empty query does not match")
        expect(matchedText("京都", in: "Kyoto 京都 trip"), "京都", "matches CJK characters")

        // Offsets are ascending and point at the query's characters.
        if let m = FuzzyMatch.match("kt", in: "Kyoto Trip") {
            expect(m.offsets, [0, 6], "offsets of a scattered match")
        }

        // ── Scoring ──
        // A prefix match outranks the same characters scattered later.
        let prefix = FuzzyMatch.match("kyo", in: "Kyoto")!
        let scattered = FuzzyMatch.match("kyo", in: "Kimono Kyoto trip")!
        expect(prefix.score > scattered.score, true, "prefix match scores above scattered match")

        // Word-boundary matches beat mid-word ones.
        let boundary = FuzzyMatch.match("tr", in: "Kyoto Trip")!
        let midWord = FuzzyMatch.match("tr", in: "Kyoto petrol")!
        expect(boundary.score > midWord.score, true, "word-boundary match scores above mid-word")

        // Consecutive runs beat the same characters with a gap.
        let run = FuzzyMatch.match("oto", in: "oto")!
        let gapped = FuzzyMatch.match("oto", in: "otxo")!
        expect(run.score > gapped.score, true, "consecutive run scores above gapped run")

        // ranges(in:) merges consecutive offsets into single runs.
        let ranges = FuzzyMatch.match("kyo", in: "Kyoto")!.ranges(in: "Kyoto")
        expect(ranges.count, 1, "consecutive match is one range")
        expect(String("Kyoto"[ranges[0].start..<ranges[0].end]), "Kyo", "range slices the matched run")

        // ── Note ranking ──
        let byName = note("Kyoto guide", body: "nothing relevant here")
        let byBody = note("Random note", body: "we talked about Kyoto at length")
        var byTitle = note("random-file", body: "irrelevant", updated: 0)
        byTitle.title = "Kyoto in spring"   // clipped article: title differs from file name

        let ranked = FuzzySearch.hits([byBody, byTitle, byName], query: "kyoto")
        expect(ranked.map(\.note.title), ["Kyoto guide", "Kyoto in spring", "Random note"],
               "file name ranks first, then title, then body")

        // The body hit carries a windowed snippet with highlight ranges.
        let bodyHit = ranked.first { $0.note.id == byBody.id }!
        expect(bodyHit.field == .content, true, "body-only match is a content hit")
        expect(bodyHit.snippet?.text.contains("Kyoto"), true, "snippet contains the matched line")
        expect(bodyHit.snippet?.ranges.isEmpty == false, true, "snippet has highlight ranges")
        let snippet = bodyHit.snippet!
        expect(String(snippet.text[snippet.ranges[0].start..<snippet.ranges[0].end]), "Kyoto",
               "snippet highlight covers the match")

        // Folder path matches.
        let inFolder = note("guide", in: "/Kyoto")
        let folderHit = FuzzySearch.hits([inFolder], query: "kyoto").first!
        expect(folderHit.field == .folder, true, "folder path match")
        expect(folderHit.subtitle, "Kyoto", "folder hit shows the folder as subtitle")

        // Root notes display "Notes" as their folder.
        expect(FuzzySearch.hits([note("A")], query: "").isEmpty, true, "empty query returns no hits")
        expect(note("A").folderDisplayName, "Notes", "root folder display name")
        expect(note("A", in: "/产品思考").folderDisplayName, "产品思考", "nested folder display name")

        // ── Tag mode (# prefix) ──
        let tagged = [
            note("A", tags: ["ai"], updated: 2),
            note("B", tags: ["AI/编程"], updated: 1),
            note("C", tags: ["Anthropic"], updated: 0),
        ]
        let tagHits = FuzzySearch.hits(tagged, query: "#ai")
        expect(tagHits.map(\.note.title), ["A", "B", "C"], "tag mode matches every tag containing the subsequence")
        expect(tagHits.first?.field == .tag, true, "tag hit field")
        expect(tagHits.first?.subtitle, "ai", "tag hit shows the matched tag")
        expect(FuzzySearch.hits(tagged, query: "#编程").map(\.note.title), ["B"],
               "tag mode matches CJK tag segments")
        expect(FuzzySearch.hits(tagged, query: "#").isEmpty, true, "bare # returns nothing")
        expect(FuzzySearch.hits(tagged, query: "#zzz").isEmpty, true, "no tag match returns nothing")

        // ── Ranking tie-breakers ──
        let older = note("same", updated: 1_000)
        let newer = note("same", updated: 2_000)
        expect(FuzzySearch.hits([older, newer], query: "same").map(\.note.id),
               [newer.id, older.id], "equal score: recently updated first")
        let pinned = note("same", updated: 1_000, pinned: true)
        expect(FuzzySearch.hits([newer, pinned], query: "same").first?.note.pinned, true,
               "equal score: pinned first")

        // ── Limit ──
        let many = (0..<10).map { note("note \($0)") }
        expect(FuzzySearch.hits(many, query: "note", limit: 3).count, 3, "limit caps the result count")

        // ── Recent list (empty query) ──
        let recents = FuzzySearch.recent(
            [note("old", updated: 1), note("pinned old", updated: 2, pinned: true), note("new", updated: 3)],
            limit: 2)
        expect(recents.map(\.title), ["pinned old", "new"], "recent: pinned first, then by date")
        expect(FuzzySearch.recent(many, limit: 3).count, 3, "recent respects the limit")

        // ── Snippets ──
        let longLine = String(repeating: "filler text before the match ", count: 4) + "Kyoto" +
                       String(repeating: " trailing filler after", count: 6)
        if let m = FuzzyMatch.match("kyoto", in: longLine) {
            let s = FuzzySearch.snippet(from: longLine, match: m)
            expect(s.text.contains("…"), true, "long line is windowed with ellipses")
            expect(s.text.contains("Kyoto"), true, "window keeps the match")
            expect(s.text.count < longLine.count, true, "window is shorter than the line")
            expect(String(s.text[s.ranges[0].start..<s.ranges[0].end]), "Kyoto",
                   "snippet highlight points at the match after windowing")
        }

        let short = FuzzySearch.snippet(from: "  a quiet line about Kyoto  ",
                                        match: FuzzyMatch.match("kyoto", in: "  a quiet line about Kyoto  ")!)
        expect(short.text, "a quiet line about Kyoto", "short snippet: whitespace trimmed, no ellipses")
        expect(String(short.text[short.ranges[0].start..<short.ranges[0].end]), "Kyoto",
               "trimmed snippet highlight stays aligned")

        let heading = FuzzySearch.snippet(from: "## Kyoto notes", match: FuzzyMatch.match("kyoto", in: "## Kyoto notes")!)
        expect(heading.text, "Kyoto notes", "snippet drops leading markdown heading marks")
        expect(String(heading.text[heading.ranges[0].start..<heading.ranges[0].end]), "Kyoto",
               "heading-stripped snippet highlight stays aligned")

        let quoted = FuzzySearch.snippet(from: "> quoted Kyoto line", match: FuzzyMatch.match("kyoto", in: "> quoted Kyoto line")!)
        expect(quoted.text, "quoted Kyoto line", "snippet drops blockquote markers")

        if failures == 0 {
            print("OK — all \(checks) FuzzySearch checks passed")
            exit(0)
        } else {
            FileHandle.standardError.write("\(failures)/\(checks) FuzzySearch checks FAILED\n".data(using: .utf8)!)
            exit(1)
        }
    }
}
