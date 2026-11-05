//! How the editor reads `![alt](dest "title")` and `[text](dest)`, shared by
//! rendering and by `link_rewrite` so both see the same links. Port of
//! `Services/InlineLink.swift`.
//!
//! The destination follows CommonMark: `<…>`-wrapped, or bare with balanced
//! parentheses (two levels deep), so `image%20(8)%20copy.png` and
//! `截图 (1).png` stay whole. Unlike CommonMark a bare destination may hold
//! unencoded spaces, as Obsidian allows. An optional title (`"…"`, `'…'` or
//! `(…)`) follows after whitespace.
//!
//! The patterns are the Swift ones: they use no lookaround, and the `regex`
//! crate's leftmost-first matching picks the same match and groups as ICU.

use regex::{Captures, Regex};
use std::ops::Range;
use std::sync::LazyLock;

/// The text between `(` and `)`: whitespace, destination, optional title.
fn inside() -> String {
    let plain = r"[^()\n]";
    let level2 = format!(r"\({plain}*\)");
    let level1 = format!(r"\((?:{plain}|{level2})*\)");
    let bare = format!("(?:{plain}|{level1})+?");
    let angled = r"<([^<>\n]*)>";
    let title = r#"("[^"\n]*"|'[^'\n]*'|\([^()\n]*\))"#;
    format!(r"\([ \t]*(?:{angled}|({bare}))(?:[ \t]+{title})?[ \t]*\)")
}

static IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"!\[([^\]\n]*)\]{}", inside())).expect("image pattern"));
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\[([^\]\n]+)\]{}", inside())).expect("link pattern"));
static ANY: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"!?\[([^\]\n]*)\]{}", inside())).expect("any pattern"));

/// `![alt](dest)`. Groups: 1 alt, 2 `<…>` destination (without the brackets),
/// 3 bare destination, 4 title (with its quotes).
pub fn image_pattern() -> &'static Regex {
    &IMAGE
}

/// `[text](dest)`, text not empty. Same groups, 1 being the text.
pub fn link_pattern() -> &'static Regex {
    &LINK
}

/// Both, `!` optional, alt or text possibly empty. Same groups.
pub fn any_pattern() -> &'static Regex {
    &ANY
}

/// The byte range of the destination of a match of one of the patterns above,
/// without `<>`.
pub fn destination_range(captures: &Captures) -> Range<usize> {
    captures.get(2).or_else(|| captures.get(3)).map_or(0..0, |m| m.range())
}

/// Whether the destination was written `<…>`.
pub fn is_angled(captures: &Captures) -> bool {
    captures.get(2).is_some()
}

#[cfg(test)]
mod tests {
    //! Ported from `Tests/InlineLinkTests.swift`.
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Found {
        whole: String,
        alt: String,
        destination: String,
        title: Option<String>,
        angled: bool,
    }

    fn matches(text: &str, pattern: &Regex) -> Vec<Found> {
        pattern
            .captures_iter(text)
            .map(|c| Found {
                whole: c[0].to_string(),
                alt: c[1].to_string(),
                destination: text[destination_range(&c)].to_string(),
                title: c.get(4).map(|m| m.as_str().to_string()),
                angled: is_angled(&c),
            })
            .collect()
    }

    fn images(text: &str) -> Vec<Found> {
        matches(text, image_pattern())
    }

    fn destinations(text: &str) -> Vec<String> {
        destinations_with(text, image_pattern())
    }

    fn destinations_with(text: &str, pattern: &Regex) -> Vec<String> {
        matches(text, pattern).into_iter().map(|f| f.destination).collect()
    }

    fn title(text: &str) -> Option<String> {
        images(text).into_iter().next().and_then(|f| f.title)
    }

    #[test]
    fn plain_links() {
        assert_eq!(destinations("![](a.png)"), ["a.png"], "plain local image");
        assert_eq!(destinations("![](attachments/a.png)"), ["attachments/a.png"], "image in a folder");
        assert_eq!(destinations("![cat](https://example.com/cat.jpg)"), ["https://example.com/cat.jpg"], "remote image");
        assert_eq!(images("![cat](a.png)").first().map(|f| f.alt.as_str()), Some("cat"), "alt text");
        assert_eq!(destinations("![]()"), Vec::<String>::new(), "empty destination is not an image");
    }

    #[test]
    fn parentheses_inside_the_destination() {
        let pixmax = "![](https://image-1325800846.cos.ap-nanjing.myqcloud.com/image%20(8)%20copy.png)";
        assert_eq!(
            images(pixmax),
            [Found {
                whole: pixmax.to_string(),
                alt: String::new(),
                destination: "https://image-1325800846.cos.ap-nanjing.myqcloud.com/image%20(8)%20copy.png".to_string(),
                title: None,
                angled: false,
            }],
            "remote URL with parentheses"
        );
        assert_eq!(destinations("![](截图 (1).png)"), ["截图 (1).png"], "local name with parentheses and a space");
        assert_eq!(destinations("![](attachments/视频生成节点-1 (1).mp4)"), ["attachments/视频生成节点-1 (1).mp4"], "video with parentheses");
        assert_eq!(destinations("![](docs/report (final).pdf)"), ["docs/report (final).pdf"], "PDF with parentheses");
        assert_eq!(destinations("![](a(b(c))d.png)"), ["a(b(c))d.png"], "two levels of nesting");
        assert_eq!(destinations("![](a((b)).png)"), ["a((b)).png"], "two levels, back to back");
        assert_eq!(destinations("![](a(b.png)"), Vec::<String>::new(), "an unbalanced parenthesis is not a destination");
        assert_eq!(destinations("x ![](a.png)) y"), ["a.png"], "a stray ) after the link stays outside");
    }

    #[test]
    fn unencoded_spaces() {
        assert_eq!(destinations("![](my image.png)"), ["my image.png"], "space in a local path");
        assert_eq!(destinations("![](  padded.png  )"), ["padded.png"], "whitespace around the destination");
    }

    #[test]
    fn titles() {
        assert_eq!(title(r#"![a](a.png "A title")"#).as_deref(), Some(r#""A title""#), "double-quoted title");
        assert_eq!(destinations(r#"![a](a.png "A title")"#), ["a.png"], "title is not part of the destination");
        assert_eq!(title("![](a.png 'single')").as_deref(), Some("'single'"), "single-quoted title");
        assert_eq!(title("![](a.png (paren))").as_deref(), Some("(paren)"), "parenthesised title");
        assert_eq!(destinations(r#"![](my image (1).png "Shot")"#), ["my image (1).png"], "spaces, parentheses and a title");
        assert_eq!(title(r#"![](my image (1).png "Shot")"#).as_deref(), Some(r#""Shot""#), "title after a path with parentheses");
        assert_eq!(destinations(r#"![](https://x.com/a_(b).png "t")"#), ["https://x.com/a_(b).png"], "remote with parens and title");
    }

    #[test]
    fn angled_destinations() {
        let angled = images("![](<my file (2).png>)");
        assert_eq!(angled.iter().map(|f| f.destination.as_str()).collect::<Vec<_>>(), ["my file (2).png"], "angled destination, brackets dropped");
        assert_eq!(angled.first().map(|f| f.angled), Some(true), "angled is reported");
        assert_eq!(images("![](a.png)").first().map(|f| f.angled), Some(false), "bare is not angled");
        assert_eq!(destinations("![](<a).png>)"), ["a).png"], "unbalanced paren is fine inside <…>");
        assert_eq!(title(r#"![](<a b.png> "T")"#).as_deref(), Some(r#""T""#), "angled with a title");
    }

    #[test]
    fn several_on_one_line() {
        assert_eq!(destinations("![](a.png) and ![](b.png)"), ["a.png", "b.png"], "two images on one line");
        assert_eq!(destinations("![](a (1).png) ![](b (2).png)"), ["a (1).png", "b (2).png"], "two images with parentheses on one line");
        assert_eq!(destinations("![](a.png)![](https://x.com/b(1).png)"), ["a.png", "https://x.com/b(1).png"], "two images back to back");
        assert_eq!(destinations("![](a.png)\n![](b.png)"), ["a.png", "b.png"], "a link never spans lines");
        assert_eq!(destinations("![](a.png\nb.png)"), Vec::<String>::new(), "a destination never spans lines");
    }

    #[test]
    fn not_images() {
        assert_eq!(destinations("![[embed.png]]"), Vec::<String>::new(), "Obsidian embed is not a Markdown image");
        assert_eq!(destinations("[text](a.png)"), Vec::<String>::new(), "a link is not an image");
    }

    #[test]
    fn links_share_the_grammar() {
        assert_eq!(
            destinations_with("[Foo](https://en.wikipedia.org/wiki/Foo_(bar))", link_pattern()),
            ["https://en.wikipedia.org/wiki/Foo_(bar)"],
            "link with parentheses"
        );
        assert_eq!(destinations_with("[](a.png)", link_pattern()), Vec::<String>::new(), "a link needs text");
        assert_eq!(destinations_with("see [a](x (1).md) and ![](y (2).png)", any_pattern()), ["x (1).md", "y (2).png"], "anyPattern finds links and images");
        assert_eq!(matches("![](y.png)", any_pattern()).first().map(|f| f.whole.as_str()), Some("![](y.png)"), "anyPattern keeps the !");
    }
}

#[cfg(test)]
mod extra_tests {
    //! Cases the Swift suite lacks. Expected values were checked against the
    //! Swift code.
    use super::*;

    fn destinations(text: &str) -> Vec<&str> {
        any_pattern().captures_iter(text).map(|c| &text[destination_range(&c)]).collect()
    }

    #[test]
    fn offsets_are_bytes_on_char_boundaries() {
        let text = "中文😀 ![图🇨🇳](截图 (1)😀.png \"题\") 后文";
        let c = image_pattern().captures(text).unwrap();
        let range = destination_range(&c);
        assert_eq!(range.start, "中文😀 ![图🇨🇳](".len());
        assert_eq!(&text[range], "截图 (1)😀.png");
        assert_eq!(&c[1], "图🇨🇳");
        assert_eq!(&c[4], "\"题\"");
    }

    #[test]
    fn crlf_lines() {
        assert_eq!(destinations("![](a.png)\r\n[b](b.md)\r\n"), ["a.png", "b.md"]);
        assert_eq!(destinations("![](a.png\r\nb.png)"), Vec::<&str>::new(), "never spans a CRLF");
        assert_eq!(destinations("![a\r\nb](c.png)"), Vec::<&str>::new());
    }

    #[test]
    fn other_line_endings_do_not_stop_a_destination() {
        assert_eq!(destinations("![](a\u{2028}b.png)"), ["a\u{2028}b.png"]);
        assert_eq!(destinations("![](a\rb.png)"), ["a\rb.png"]);
    }

    #[test]
    fn whitespace_only_destinations() {
        assert_eq!(destinations("![]( )"), [" "], "a lone space is a bare destination");
        assert_eq!(destinations("![](<>)"), [""], "empty angled destination");
        assert!(is_angled(&image_pattern().captures("![](<>)").unwrap()));
    }

    #[test]
    fn nesting_past_two_levels_is_no_link() {
        assert_eq!(destinations("![](a(b(c(d))).png)"), Vec::<&str>::new());
    }

    #[test]
    fn full_width_parentheses_are_text() {
        assert_eq!(destinations("![](截图（1）.png)"), ["截图（1）.png"]);
    }
}
