import Foundation

// Standalone test runner for InlineLink. No Xcode test target exists; it is
// Foundation-only, so this compiles directly with the source file:
//
//     swiftc Fumi/Services/InlineLink.swift Tests/InlineLinkTests.swift -o /tmp/inlinelink-tests
//     /tmp/inlinelink-tests
//
// Exits non-zero if any assertion fails.

@main
enum InlineLinkTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    struct Found: Equatable, CustomStringConvertible {
        let whole: String, alt: String, destination: String, title: String?, angled: Bool
        var description: String { "\(whole) | alt=\(alt) dest=\(destination) title=\(title ?? "nil") angled=\(angled)" }
    }

    static func matches(_ text: String, _ pattern: NSRegularExpression = InlineLink.imagePattern) -> [Found] {
        let ns = text as NSString
        return pattern.matches(in: text, range: NSRange(location: 0, length: ns.length)).map { match in
            let title = match.range(at: 4)
            return Found(whole: ns.substring(with: match.range),
                         alt: ns.substring(with: match.range(at: 1)),
                         destination: ns.substring(with: InlineLink.destinationRange(of: match)),
                         title: title.location == NSNotFound ? nil : ns.substring(with: title),
                         angled: InlineLink.isAngled(match))
        }
    }

    static func destinations(_ text: String, _ pattern: NSRegularExpression = InlineLink.imagePattern) -> [String] {
        matches(text, pattern).map(\.destination)
    }

    static func main() {
        // Plain links, as before.
        expect(destinations("![](a.png)"), ["a.png"], "plain local image")
        expect(destinations("![](attachments/a.png)"), ["attachments/a.png"], "image in a folder")
        expect(destinations("![cat](https://example.com/cat.jpg)"), ["https://example.com/cat.jpg"], "remote image")
        expect(matches("![cat](a.png)").first?.alt, "cat", "alt text")
        expect(destinations("![]()"), [], "empty destination is not an image")

        // Parentheses inside the destination.
        let pixmax = "![](https://image-1325800846.cos.ap-nanjing.myqcloud.com/image%20(8)%20copy.png)"
        expect(matches(pixmax), [Found(whole: pixmax, alt: "",
                                       destination: "https://image-1325800846.cos.ap-nanjing.myqcloud.com/image%20(8)%20copy.png",
                                       title: nil, angled: false)], "remote URL with parentheses")
        expect(destinations("![](截图 (1).png)"), ["截图 (1).png"], "local name with parentheses and a space")
        expect(destinations("![](attachments/视频生成节点-1 (1).mp4)"), ["attachments/视频生成节点-1 (1).mp4"],
               "video with parentheses")
        expect(destinations("![](docs/report (final).pdf)"), ["docs/report (final).pdf"], "PDF with parentheses")
        expect(destinations("![](a(b(c))d.png)"), ["a(b(c))d.png"], "two levels of nesting")
        expect(destinations("![](a((b)).png)"), ["a((b)).png"], "two levels, back to back")
        expect(destinations("![](a(b.png)"), [], "an unbalanced parenthesis is not a destination")
        expect(destinations("x ![](a.png)) y"), ["a.png"], "a stray ) after the link stays outside")

        // Unencoded spaces in local paths still work.
        expect(destinations("![](my image.png)"), ["my image.png"], "space in a local path")
        expect(destinations("![](  padded.png  )"), ["padded.png"], "whitespace around the destination")

        // Titles.
        expect(matches(#"![a](a.png "A title")"#).first?.title, #""A title""#, "double-quoted title")
        expect(destinations(#"![a](a.png "A title")"#), ["a.png"], "title is not part of the destination")
        expect(matches("![](a.png 'single')").first?.title, "'single'", "single-quoted title")
        expect(matches("![](a.png (paren))").first?.title, "(paren)", "parenthesised title")
        expect(destinations(#"![](my image (1).png "Shot")"#), ["my image (1).png"], "spaces, parentheses and a title")
        expect(matches(#"![](my image (1).png "Shot")"#).first?.title, #""Shot""#, "title after a path with parentheses")
        expect(destinations(#"![](https://x.com/a_(b).png "t")"#), ["https://x.com/a_(b).png"], "remote with parens and title")

        // `<…>` destinations.
        let angled = matches("![](<my file (2).png>)")
        expect(angled.map(\.destination), ["my file (2).png"], "angled destination, brackets dropped")
        expect(angled.first?.angled, true, "angled is reported")
        expect(matches("![](a.png)").first?.angled, false, "bare is not angled")
        expect(destinations("![](<a).png>)"), ["a).png"], "unbalanced paren is fine inside <…>")
        expect(matches(#"![](<a b.png> "T")"#).first?.title, #""T""#, "angled with a title")

        // Several on one line.
        expect(destinations("![](a.png) and ![](b.png)"), ["a.png", "b.png"], "two images on one line")
        expect(destinations("![](a (1).png) ![](b (2).png)"), ["a (1).png", "b (2).png"],
               "two images with parentheses on one line")
        expect(destinations("![](a.png)![](https://x.com/b(1).png)"), ["a.png", "https://x.com/b(1).png"],
               "two images back to back")
        expect(destinations("![](a.png)\n![](b.png)"), ["a.png", "b.png"], "a link never spans lines")
        expect(destinations("![](a.png\nb.png)"), [], "a destination never spans lines")

        // Not images.
        expect(destinations("![[embed.png]]"), [], "Obsidian embed is not a Markdown image")
        expect(destinations("[text](a.png)"), [], "a link is not an image")

        // Links share the grammar.
        expect(destinations("[Foo](https://en.wikipedia.org/wiki/Foo_(bar))", InlineLink.linkPattern),
               ["https://en.wikipedia.org/wiki/Foo_(bar)"], "link with parentheses")
        expect(destinations("[](a.png)", InlineLink.linkPattern), [], "a link needs text")
        expect(destinations("see [a](x (1).md) and ![](y (2).png)", InlineLink.anyPattern),
               ["x (1).md", "y (2).png"], "anyPattern finds links and images")
        expect(matches("![](y.png)", InlineLink.anyPattern).first?.whole, "![](y.png)", "anyPattern keeps the !")

        if failures == 0 {
            print("OK: \(checks) checks passed")
        } else {
            print("\(failures) of \(checks) checks failed")
            exit(1)
        }
    }
}
