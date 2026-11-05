//! The plain-text excerpt the outline card shows under a heading: the body
//! between that heading and the next one, with Markdown syntax dropped. Port
//! of `Views/Editor/OutlineSectionPreview.swift`.

use crate::frontmatter::text;
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;
use unicode_segmentation::UnicodeSegmentation;

/// The limit the app asks for.
pub const DEFAULT_LIMIT: usize = 240;

static BLOCK_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:>\s*)*(?:\[![A-Za-z]+\][+-]?\s*)?(?:(?:[-*+]|\d+[.)])\s+)?(?:\[[ xX]\]\s+)?").expect("block prefix")
});
static IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").expect("image"));
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)]*\)").expect("link"));
static WIKI_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[(?:[^\]|]+\|)?([^\]]+)\]\]").expect("wiki link"));
static EMPHASIS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*|__|~~|==|`|\*").expect("emphasis"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").expect("spaces"));

/// The excerpt of `body`, at most `limit` grapheme clusters long.
pub fn text(body: &str, limit: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut length = 0;
    let mut in_code_block = false;
    for raw in text::lf_lines(body) {
        let line = text::trim_ws(raw);
        if text::has_prefix(line, "```") || text::has_prefix(line, "~~~") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block || line.is_empty() || text::has_prefix(line, "|") || is_rule(line) {
            continue;
        }
        let plain = inline_plain(&BLOCK_PREFIX.replace_all(line, ""));
        if plain.is_empty() {
            continue;
        }
        length += text::count(&plain) + 1;
        parts.push(plain);
        if length >= limit {
            break;
        }
    }
    let joined = parts.join(" ");
    match joined.grapheme_indices(true).nth(limit) {
        Some((cut, _)) => joined[..cut].to_string(),
        None => joined,
    }
}

fn is_rule(line: &str) -> bool {
    let compact: Vec<&str> = line.graphemes(true).filter(|g| *g != " ").collect();
    compact.len() >= 3
        && compact.iter().collect::<HashSet<_>>().len() == 1
        && matches!(compact[0], "-" | "*" | "_")
}

fn inline_plain(line: &str) -> String {
    let s = IMAGE.replace_all(line, "");
    let s = WIKI_LINK.replace_all(&s, "$1");
    let s = LINK.replace_all(&s, "$1");
    let s = EMPHASIS.replace_all(&s, "");
    let s = SPACES.replace_all(&s, " ");
    text::trim_ws(&s).to_string()
}

#[cfg(test)]
mod tests {
    //! Ported from `Tests/OutlineSectionPreviewTests.swift`.
    use super::*;

    #[test]
    fn excerpts() {
        assert_eq!(text("\nFirst line.\n\nSecond line.\n", DEFAULT_LIMIT), "First line. Second line.", "joins paragraphs, skips blank lines");
        assert_eq!(text("- one\n* two\n3. three\n- [x] done", DEFAULT_LIMIT), "one two three done", "drops list and task markers");
        assert_eq!(text("> quoted\n> [!note] Callout title", DEFAULT_LIMIT), "quoted Callout title", "drops quote and callout markers");
        assert_eq!(
            text("A **bold** and `code` with [link](https://x.y) and [[Page|alias]]", DEFAULT_LIMIT),
            "A bold and code with link and alias",
            "strips inline syntax"
        );
        assert_eq!(text("![img](a.png)\nAfter image", DEFAULT_LIMIT), "After image", "drops image-only lines");
        assert_eq!(text("Intro\n```swift\nlet x = 1\n```\n---\n| a | b |\nOutro", DEFAULT_LIMIT), "Intro Outro", "skips code blocks, rules and table rows");
        assert_eq!(text("snake_case_name stays", DEFAULT_LIMIT), "snake_case_name stays", "keeps single underscores");
        assert_eq!(if text::count(&text(&"word ".repeat(100), 20)) == 20 { "ok" } else { "long" }, "ok", "truncates to the limit");
        assert_eq!(text("", DEFAULT_LIMIT), "", "empty body");
    }
}

#[cfg(test)]
mod extra_tests {
    //! Cases the Swift suite lacks. Expected values were checked against the
    //! Swift code unless a comment says the port differs on purpose.
    use super::*;

    #[test]
    fn cjk_and_emoji_are_counted_as_characters() {
        assert_eq!(text(&"中".repeat(30), 20), "中".repeat(20));
        let family = "👨\u{200D}👩\u{200D}👧";
        assert_eq!(text(&family.repeat(30), 20), family.repeat(20), "never cut inside a cluster");
        assert_eq!(text(&"🇨🇳".repeat(5), 3), "🇨🇳".repeat(3));
        assert_eq!(text(&"e\u{301}".repeat(5), 3), "e\u{301}".repeat(3));
        assert_eq!(text("- 第一项\n- [ ] 第二项\n> **引用** [链接](a.md)", DEFAULT_LIMIT), "第一项 第二项 引用 链接");
    }

    #[test]
    fn stops_reading_once_past_the_limit() {
        assert_eq!(text("aaaa\nbbbb\ncccc", 5), "aaaa");
        assert_eq!(text("aaaa\nbbbb\ncccc", 6), "aaaa b");
    }

    /// The Swift code differs here on purpose: its `split(separator: "\n")`
    /// doesn't split at `\r\n`, so a CRLF body was one line and a fence or a
    /// table on its first line hid everything.
    #[test]
    fn crlf_bodies_have_lines() {
        assert_eq!(text("\r\n甲\r\n\r\n乙\r\n", DEFAULT_LIMIT), "甲 乙");
        assert_eq!(text("- 一\r\n- 二", DEFAULT_LIMIT), "一 二");
        assert_eq!(text("```\r\ncode\r\n```\r\n后文", DEFAULT_LIMIT), "后文");
        assert_eq!(text("| a |\r\n后文", DEFAULT_LIMIT), "后文");
        assert_eq!(text("---\r\n后文", DEFAULT_LIMIT), "后文");
    }

    #[test]
    fn rules_and_fences() {
        assert_eq!(text("- - -\n* * *\n___\n后", DEFAULT_LIMIT), "后");
        assert_eq!(text("--\n后", DEFAULT_LIMIT), "-- 后", "two dashes are no rule");
        assert_eq!(text("~~~\nx\n```\n后", DEFAULT_LIMIT), "后", "any fence closes any fence");
        assert_eq!(text("```\n未闭合", DEFAULT_LIMIT), "");
        assert_eq!(text("[[目标]] 和 [[目标|别名]]", DEFAULT_LIMIT), "目标 和 别名");
        assert_eq!(text("a\u{3000}\u{3000}b", DEFAULT_LIMIT), "a b", "\\s covers U+3000");
    }
}
