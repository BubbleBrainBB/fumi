import Foundation

// Standalone test runner for MemoFormat / MemoHeatmap (design board ㉛):
//
//     swiftc Fumi/Services/MemoFormat.swift Tests/MemoFormatTests.swift -o /tmp/memo-tests
//     /tmp/memo-tests
//
// Exits non-zero if any assertion fails.

@main
enum MemoFormatTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    static let day = """
    ## 09:31
    早上散步想到：随手记不该有标题，
    打开就写。 #idea

    ## 14:02
    搜索正文改成精确匹配之后，结果干净了。 #fumi #search

    """

    static func main() {
        // ── Parsing ──
        let memos = MemoFormat.parse(day, day: "2026-10-16")
        expect(memos.count, 2, "two memos")
        expect(memos.map(\.time), ["09:31", "14:02"], "times in file order")
        expect(memos[0].text, "早上散步想到：随手记不该有标题，\n打开就写。 #idea", "multi-line body, trimmed")
        expect(memos[1].tags, ["fumi", "search"], "tags")
        expect(memos[0].id, "2026-10-16#0", "id is day + index")

        // A daily note's own text before the first memo is not a memo.
        let daily = "# 2026-10-16\n今天的计划……\n\n## 08:00\n起床\n"
        expect(MemoFormat.parse(daily, day: "d").map(\.text), ["起床"], "preamble ignored")

        // Other headings stay inside a memo; a heading in a code block is not a memo.
        let mixed = "## 10:00\n## Not a time\n```\n## 11:00\n```\n"
        let parsed = MemoFormat.parse(mixed, day: "d")
        expect(parsed.count, 1, "only time headings outside code start memos")
        expect(parsed.first?.text, "## Not a time\n```\n## 11:00\n```", "body keeps other headings and code")

        // Empty memos are skipped.
        expect(MemoFormat.parse("## 10:00\n\n## 11:00\nhi\n", day: "d").map(\.text), ["hi"], "empty memo skipped")

        // ── Appending ──
        expect(MemoFormat.appending("  hello  ", at: "09:00", to: ""), "## 09:00\nhello\n", "first memo of the day")
        let two = MemoFormat.appending("second", at: "10:00", to: "## 09:00\nfirst\n")
        expect(two, "## 09:00\nfirst\n\n## 10:00\nsecond\n", "blank line between memos")
        expect(MemoFormat.parse(two, day: "d").map(\.text), ["first", "second"], "appended memo parses back")

        // ── Replacing ──
        let edited = MemoFormat.replacing(memo: 0, with: "changed\ntwo lines", in: two)
        expect(edited, "## 09:00\nchanged\ntwo lines\n\n## 10:00\nsecond\n", "replace first keeps the gap")
        expect(MemoFormat.replacing(memo: 1, with: "last", in: two), "## 09:00\nfirst\n\n## 10:00\nlast\n",
               "replace last keeps the final newline")
        expect(MemoFormat.replacing(memo: 5, with: "x", in: two), two, "out-of-range replace is a no-op")

        // ── Removing ──
        expect(MemoFormat.removing(memo: 0, from: two), "## 10:00\nsecond\n", "remove first")
        expect(MemoFormat.removing(memo: 1, from: two), "## 09:00\nfirst\n", "remove last")
        expect(MemoFormat.removing(memo: 0, from: "## 09:00\nonly\n"), "", "remove the only memo")
        expect(MemoFormat.removing(memo: 0, from: daily), "# 2026-10-16\n今天的计划……\n", "remove keeps the preamble")

        // ── Days ──
        expect(MemoFormat.isDay("2026-10-16"), true, "day file name")
        expect(MemoFormat.isDay("Untitled"), false, "other file name")
        let date = MemoFormat.date(ofDay: "2026-10-16")!
        expect(MemoFormat.day(of: date), "2026-10-16", "day round trip")

        // ── Tags ──
        expect(MemoFormat.tags(in: "a #one, b#no #二 (#three)"), ["one", "二", "three"], "tag boundaries")

        // ── Heatmap ──
        expect([0, 1, 2, 3, 4, 5, 9].map(MemoHeatmap.level(for:)), [0, 1, 2, 3, 3, 4, 4], "levels")
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .current
        let thursday = calendar.date(from: DateComponents(year: 2026, month: 10, day: 15))! // a Thursday
        let grid = MemoHeatmap.grid(weeks: 3, endingAt: thursday, calendar: calendar)
        expect(grid.count, 3, "three weeks")
        expect(grid.allSatisfy { $0.count == 7 }, true, "seven days a week")
        expect(grid[2][3].map { MemoFormat.day(of: $0) }, "2026-10-15", "today is the last column's Thursday")
        expect(grid[2][4] == nil, true, "days after today are empty")
        expect(grid[0][0].map { MemoFormat.day(of: $0) }, "2026-09-28", "first cell is the Monday two weeks back")

        let october = MemoHeatmap.month(containing: thursday, calendar: calendar)
        expect(october.count, 5, "October 2026 spans five Monday-first weeks")
        expect(october[0][2] == nil && october[0][3] != nil, true, "October 1 2026 is a Thursday")
        expect(october[0][3].map { MemoFormat.day(of: $0) }, "2026-10-01", "first day")
        expect(october[4][5].map { MemoFormat.day(of: $0) }, "2026-10-31", "last day on Saturday")
        expect(october[4][6] == nil, true, "padding after the last day")
        let february = MemoHeatmap.month(containing: calendar.date(from: DateComponents(year: 2027, month: 2, day: 10))!,
                                         calendar: calendar)
        expect(february.count, 4, "February 2027 starts on Monday and fills four weeks")

        if failures == 0 {
            print("All \(checks) MemoFormat checks passed.")
        } else {
            print("\(failures) of \(checks) MemoFormat checks failed.")
            exit(1)
        }
    }
}
