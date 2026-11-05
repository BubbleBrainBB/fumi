//! A note's display title. Port of `Services/TitleExtractor.swift`.
//!
//! 1. A leading YAML frontmatter block is skipped.
//! 2. A first content line starting with `# ` gives the heading text.
//! 3. Otherwise the first line with text left once Markdown markers are
//!    stripped gives it, so an image-only line is skipped.
//! 4. If nothing qualifies the result is `""`; callers use the file name.
//! 5. Titles are cut to 80 grapheme clusters, with `...` after.

use crate::frontmatter::text;
use regex::Regex;
use std::sync::LazyLock;
use unicode_segmentation::UnicodeSegmentation;

/// The title of a new note with none yet.
pub const UNTITLED: &str = "Untitled";

const MAX_TITLE: usize = 80;

/// The rules applied in order. ICU's `.` stops at more line endings than
/// Rust's, and its `$` also matches before a final line ending (which stays).
static SANITIZE_RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    const DOT: &str = r"[^\n\x0B\x0C\r\x{85}\x{2028}\x{2029}]";
    const LAST_ENDING: &str = r"(\r\n|[\n\x0B\x0C\r\x{85}\x{2028}\x{2029}])?$";
    [
        (r"!\[[^\]]*\]\([^)]*\)".to_string(), ""),
        (r"^#+\s*".to_string(), ""),
        (r"^\*\s+".to_string(), ""),
        (r"^-\s+".to_string(), ""),
        (r"^\d+\.\s+".to_string(), ""),
        (r"^>\s*".to_string(), ""),
        (format!("^[-*_]{{3,}}{LAST_ENDING}"), "$1"),
        (format!(r"\*\*({DOT}+?)\*\*"), "$1"),
        (format!(r"\*({DOT}+?)\*"), "$1"),
        (format!(r"~~({DOT}+?)~~"), "$1"),
        (r"`([^`]+)`".to_string(), "$1"),
        (r"\[([^\]]+)\]\([^)]+\)".to_string(), "$1"),
        (r"#\S+".to_string(), ""),
    ]
    .into_iter()
    .map(|(pattern, replacement)| (Regex::new(&pattern).expect("title rule"), replacement))
    .collect()
});

/// The display title of `content`, or `""` when it has none.
pub fn extract(content: &str) -> String {
    let lines: Vec<&str> = text::lf_lines(content).collect();
    let mut start = 0;
    if text::trim_ws(lines[0]) == "---" {
        let close = (1..lines.len()).find(|&i| {
            let t = text::trim_ws(lines[i]);
            t == "---" || t == "..."
        });
        if let Some(close) = close {
            start = close + 1;
        }
    }
    for line in &lines[start..] {
        let trimmed = text::trim_ws(line);
        if trimmed.is_empty() || trimmed == "---" {
            continue;
        }
        let candidate = if text::has_prefix(trimmed, "# ") { text::trim_ws(&trimmed[2..]) } else { trimmed };
        let title = sanitize(candidate);
        if !title.is_empty() {
            return title;
        }
    }
    String::new()
}

fn sanitize(input: &str) -> String {
    let mut result = input.to_string();
    for (regex, replacement) in SANITIZE_RULES.iter() {
        if let std::borrow::Cow::Owned(replaced) = regex.replace_all(&result, *replacement) {
            result = replaced;
        }
    }
    let result = text::trim_ws(&result);
    match result.grapheme_indices(true).nth(MAX_TITLE) {
        Some((cut, _)) => format!("{}...", &result[..cut]),
        None => result.to_string(),
    }
}

#[cfg(test)]
mod tests {
    //! Ported from `Tests/FrontmatterTitleTests.swift` (its titles half; the
    //! frontmatter is in `frontmatter.rs`).
    use super::*;

    #[test]
    fn titles() {
        assert_eq!(extract("# Hello\n\nBody"), "Hello", "H1 title");
        assert_eq!(extract("---\ntags: [a]\n---\n# Real Title"), "Real Title", "frontmatter is skipped");
        assert_eq!(
            extract("![](https://image-1325.cos/x.png)\n\nThe actual first line"),
            "The actual first line",
            "image-only line is not a title"
        );
        assert_eq!(extract("# ![](a.png)\nNext"), "Next", "heading made only of an image is skipped");
        assert_eq!(extract("See ![chart](c.png) for **details**"), "See  for details", "inline image removed, markers stripped");
        assert_eq!(extract("![](only.png)"), "", "nothing usable → empty, caller uses file name");
        assert_eq!(extract(""), "", "empty note");
    }
}

#[cfg(test)]
mod extra_tests {
    //! Cases the Swift suite lacks. Expected values were checked against the
    //! Swift code unless a comment says the port differs on purpose.
    use super::*;

    #[test]
    fn cjk_titles() {
        assert_eq!(extract("# 今天的笔记\n\n正文"), "今天的笔记");
        assert_eq!(extract("---\n标题: 甲\n---\n\n**重要**的事 #标签 还有 `代码`"), "重要的事  还有 代码");
        assert_eq!(extract("\u{3000}# 全角缩进\u{3000}"), "全角缩进", "U+3000 is whitespace");
    }

    #[test]
    fn cut_at_80_grapheme_clusters() {
        let cjk = "中".repeat(80);
        assert_eq!(extract(&cjk), cjk, "exactly 80 stays whole");
        assert_eq!(extract(&"中".repeat(81)), format!("{cjk}..."));
        let family = "👨\u{200D}👩\u{200D}👧";
        assert_eq!(extract(&family.repeat(81)), format!("{}...", family.repeat(80)), "a ZWJ family is one");
        let flag = "🇨🇳";
        assert_eq!(extract(&flag.repeat(81)), format!("{}...", flag.repeat(80)), "a flag is one");
        let marked = "e\u{301}";
        assert_eq!(extract(&marked.repeat(81)), format!("{}...", marked.repeat(80)), "a combining mark stays on");
    }

    /// The Swift code differs here on purpose: its `split(separator: "\n")`
    /// doesn't split at `\r\n`, so the whole CRLF note became the title.
    #[test]
    fn crlf_notes_have_lines() {
        assert_eq!(extract("# 标题\r\n正文\r\n"), "标题");
        assert_eq!(extract("---\r\ntags: [a]\r\n---\r\n\r\n# 标题\r\n"), "标题");
        assert_eq!(extract("\r\n\r\n第一行\r\n第二行"), "第一行");
        assert_eq!(extract("![](a.png)\r\n下一行"), "下一行");
    }

    #[test]
    fn frontmatter_at_file_boundaries() {
        assert_eq!(extract("---\ntitle: x\n---"), "", "only frontmatter, no final newline");
        assert_eq!(extract("---\ntitle: x\n---\n"), "");
        assert_eq!(extract("---\ntitle: x\n"), "title: x", "no closing fence: its lines are content");
        assert_eq!(extract("---"), "");
        assert_eq!(extract("---\n...\nAfter"), "After", "`...` closes too");
        assert_eq!(extract("\u{FEFF}---\na: 1\n---\n# T"), "\u{FEFF}---", "a BOM hides the fence");
        assert_eq!(extract("\u{FEFF}# T"), "\u{FEFF}# T", "a BOM hides the heading marker");
    }

    #[test]
    fn emoji_are_not_markers() {
        let keycap = "#\u{FE0F}\u{20E3}";
        assert_eq!(extract(&format!("{keycap} 话题")), "\u{FE0F}\u{20E3} 话题", "not `# `, but the rule `^#+` reads scalars");
        assert_eq!(extract("😀 **粗** 🎉"), "😀 粗 🎉");
    }

    #[test]
    fn line_kinds() {
        assert_eq!(extract("***\n正文"), "正文", "a rule has no text");
        assert_eq!(extract("- 项目"), "项目");
        assert_eq!(extract("12. 项目"), "项目");
        assert_eq!(extract("> 引用"), "引用");
        assert_eq!(extract("[文字](https://x.y)"), "文字");
        assert_eq!(extract("## 二级"), "二级");
        assert_eq!(extract("#标签 正文"), "标签 正文", "`^#+` goes first, so a leading hashtag stays as a word");
        assert_eq!(extract("正文 #标签 尾"), "正文  尾");
        assert_eq!(extract("***\u{2028}"), "\u{2028}", "ICU's `$` stops before a final U+2028, which stays");
        assert_eq!(extract("*a\u{2028}b*"), "*a\u{2028}b*", "ICU's `.` doesn't cross U+2028");
    }

    #[test]
    fn untitled_constant() {
        assert_eq!(UNTITLED, "Untitled");
    }
}
