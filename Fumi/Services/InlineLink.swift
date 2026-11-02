import Foundation

/// How the editor reads `![alt](dest "title")` and `[text](dest)`, shared by
/// `WysiwygEngine` (rendering) and `LinkRewrite` (moving notes) so both see
/// the same links. Foundation-only, so Tests/InlineLinkTests.swift runs it
/// directly.
///
/// The destination follows CommonMark: `<…>`-wrapped, or bare with balanced
/// parentheses (two levels deep), so `image%20(8)%20copy.png` and
/// `截图 (1).png` stay whole. Unlike CommonMark a bare destination may hold
/// unencoded spaces, as Obsidian allows: `![](my image.png)`. An optional
/// title — `"…"`, `'…'` or `(…)` — follows after whitespace.
enum InlineLink {
    /// `![alt](dest)`. Groups: 1 alt, 2 `<…>` destination (without the
    /// brackets), 3 bare destination, 4 title (with its quotes).
    static let imagePattern = try! NSRegularExpression(pattern: #"!\[([^\]\n]*)\]"# + inside)
    /// `[text](dest)`, text not empty. Same groups as `imagePattern`, 1 being
    /// the text.
    static let linkPattern = try! NSRegularExpression(pattern: #"\[([^\]\n]+)\]"# + inside)
    /// Both, `!` optional, alt or text possibly empty. Same groups.
    static let anyPattern = try! NSRegularExpression(pattern: #"!?\[([^\]\n]*)\]"# + inside)

    /// The destination of a match of one of the patterns above, without `<>`.
    static func destinationRange(of match: NSTextCheckingResult) -> NSRange {
        let angled = match.range(at: 2)
        return angled.location != NSNotFound ? angled : match.range(at: 3)
    }

    /// Whether the destination was written `<…>`.
    static func isAngled(_ match: NSTextCheckingResult) -> Bool {
        match.range(at: 2).location != NSNotFound
    }

    /// The text between `(` and `)`: whitespace, destination, optional title.
    private static let inside: String = {
        let plain = #"[^()\n]"#
        let level2 = #"\("# + plain + #"*\)"#
        let level1 = #"\((?:"# + plain + "|" + level2 + #")*\)"#
        let bare = "(?:" + plain + "|" + level1 + ")+?"
        let angled = #"<([^<>\n]*)>"#
        let title = #"("[^"\n]*"|'[^'\n]*'|\([^()\n]*\))"#
        return #"\([ \t]*(?:"# + angled + "|(" + bare + "))" + #"(?:[ \t]+"# + title + #")?[ \t]*\)"#
    }()
}
