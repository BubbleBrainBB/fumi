import Foundation

/// Three-way merge of a note, line by line: what the editor has (`mine`) and
/// what is now on disk (`theirs`), both edited from the same `base`. A
/// Markdown paragraph is one line, so this merges paragraph by paragraph.
/// Foundation-only, so Tests/Merge3Tests.swift runs it directly.
enum Merge3 {
    struct Result: Equatable {
        /// The merged text. Where both sides changed the same lines
        /// differently, `mine` is kept.
        var text: String
        /// How many such places there were.
        var conflicts: Int
    }

    static func merge(base: String, mine: String, theirs: String) -> Result {
        if mine == theirs || theirs == base { return Result(text: mine, conflicts: 0) }
        if mine == base { return Result(text: theirs, conflicts: 0) }

        let b = lines(base), m = lines(mine), t = lines(theirs)
        let toMine = matches(from: b, to: m)
        let toTheirs = matches(from: b, to: t)

        // diff3: base lines kept by both sides are fixed points; between two
        // of them each side either left the base alone or changed it.
        var out: [Substring] = []
        var conflicts = 0
        var (lastB, lastM, lastT) = (-1, -1, -1)
        for i in 0...b.count {
            let fixed: (m: Int, t: Int)?
            if i == b.count {
                fixed = (m.count, t.count)
            } else if let mi = toMine[i], let ti = toTheirs[i] {
                fixed = (mi, ti)
            } else {
                fixed = nil
            }
            guard let fixed else { continue }
            let baseChunk = b[(lastB + 1)..<i]
            let mineChunk = m[(lastM + 1)..<fixed.m]
            let theirsChunk = t[(lastT + 1)..<fixed.t]
            if mineChunk.elementsEqual(baseChunk) {
                out += theirsChunk
            } else if theirsChunk.elementsEqual(baseChunk) || theirsChunk.elementsEqual(mineChunk) {
                out += mineChunk
            } else {
                out += mineChunk
                conflicts += 1
            }
            if i < b.count { out.append(b[i]) }
            (lastB, lastM, lastT) = (i, fixed.m, fixed.t)
        }
        return Result(text: out.joined(separator: "\n"), conflicts: conflicts)
    }

    /// Where `location` (UTF-16, in `old`) lands in `new`: lines both share
    /// keep their offset within the line; a location inside lines that
    /// changed lands at the same distance into what replaced them, at most
    /// its end. Keeps the caret on the text it was on when a merge is put
    /// into the editor.
    static func mapLocation(_ location: Int, from old: String, to new: String) -> Int {
        let o = lines(old), n = lines(new)
        let pairs = matches(from: o, to: n)
        var oldStart = 0, newStart = 0
        var (lastO, lastN) = (-1, -1)
        for i in 0...o.count {
            guard i == o.count || pairs[i] != nil else { continue }
            let j = i == o.count ? n.count : pairs[i]!
            // The changed run before this kept line, then the line itself. A
            // location at the start of the kept line stays with that line.
            let oldRun = length(o[(lastO + 1)..<i]), newRun = length(n[(lastN + 1)..<j])
            if location < oldStart + oldRun {
                // At most the end of the replacement's last line, not past its break.
                return newStart + min(location - oldStart, max(0, newRun - 1))
            }
            oldStart += oldRun
            newStart += newRun
            guard i < o.count else { break }
            let lineLength = o[i].utf16.count + 1
            if location < oldStart + lineLength {
                return newStart + (location - oldStart)
            }
            oldStart += lineLength
            newStart += lineLength
            (lastO, lastN) = (i, j)
        }
        return (new as NSString).length
    }

    private static func lines(_ text: String) -> [Substring] {
        text.split(separator: "\n", omittingEmptySubsequences: false)
    }

    /// UTF-16 length of `lines` with the line break after each.
    private static func length(_ lines: ArraySlice<Substring>) -> Int {
        lines.reduce(0) { $0 + $1.utf16.count + 1 }
    }

    /// For each line of `a`, the line of `b` it stays as, or nil when it was
    /// removed or changed. Increasing, from a shortest diff.
    private static func matches(from a: [Substring], to b: [Substring]) -> [Int?] {
        let diff = b.difference(from: a)
        var removed = Set<Int>(), inserted = Set<Int>()
        for change in diff {
            switch change {
            case .remove(let offset, _, _): removed.insert(offset)
            case .insert(let offset, _, _): inserted.insert(offset)
            }
        }
        var result = [Int?](repeating: nil, count: a.count)
        var j = 0
        for i in a.indices where !removed.contains(i) {
            while inserted.contains(j) { j += 1 }
            result[i] = j
            j += 1
        }
        return result
    }
}
