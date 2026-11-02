import Foundation

/// The plain-text excerpt the outline card shows under a heading: the body
/// between that heading and the next one, with Markdown syntax dropped.
///
/// Pure (no SwiftUI) so it can be unit tested standalone; see
/// `Tests/OutlineSectionPreviewTests.swift`.
enum OutlineSectionPreview {
    static func text(of body: String, limit: Int = 240) -> String {
        var parts: [String] = []
        var length = 0
        var inCodeBlock = false
        for raw in body.split(separator: "\n", omittingEmptySubsequences: true) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("```") || line.hasPrefix("~~~") {
                inCodeBlock.toggle()
                continue
            }
            if inCodeBlock || line.isEmpty || line.hasPrefix("|") || isRule(line) { continue }
            let plain = inlinePlain(stripBlockPrefix(line))
            guard !plain.isEmpty else { continue }
            parts.append(plain)
            length += plain.count + 1
            if length >= limit { break }
        }
        let joined = parts.joined(separator: " ")
        return joined.count > limit ? String(joined.prefix(limit)) : joined
    }

    private static func isRule(_ line: String) -> Bool {
        let compact = line.replacingOccurrences(of: " ", with: "")
        return compact.count >= 3 && Set(compact).count == 1 && "-*_".contains(compact.first!)
    }

    private static let blockPrefix = try! NSRegularExpression(
        pattern: #"^(?:>\s*)*(?:\[![A-Za-z]+\][+-]?\s*)?(?:(?:[-*+]|\d+[.)])\s+)?(?:\[[ xX]\]\s+)?"#)
    private static let image = try! NSRegularExpression(pattern: #"!\[[^\]]*\]\([^)]*\)"#)
    private static let link = try! NSRegularExpression(pattern: #"\[([^\]]+)\]\([^)]*\)"#)
    private static let wikiLink = try! NSRegularExpression(pattern: #"\[\[(?:[^\]|]+\|)?([^\]]+)\]\]"#)
    private static let emphasis = try! NSRegularExpression(pattern: #"\*\*|__|~~|==|`|\*"#)
    private static let spaces = try! NSRegularExpression(pattern: #"\s{2,}"#)

    private static func stripBlockPrefix(_ line: String) -> String {
        replace(blockPrefix, in: line, with: "")
    }

    private static func inlinePlain(_ line: String) -> String {
        var s = replace(image, in: line, with: "")
        s = replace(wikiLink, in: s, with: "$1")
        s = replace(link, in: s, with: "$1")
        s = replace(emphasis, in: s, with: "")
        s = replace(spaces, in: s, with: " ")
        return s.trimmingCharacters(in: .whitespaces)
    }

    private static func replace(_ regex: NSRegularExpression, in s: String, with template: String) -> String {
        regex.stringByReplacingMatches(in: s, range: NSRange(s.startIndex..., in: s), withTemplate: template)
    }
}
