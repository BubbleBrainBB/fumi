//! The one-line preview under a note's title in the list: the first non-empty
//! line of content, after a leading frontmatter block if it is closed. Port of
//! `Views/NoteList/NoteCardSummary.swift`.

use crate::frontmatter::text;

/// The first line of content, trimmed, or `None` when there is none. Only
/// `---` closes the block here (not `...`), as in the Swift code.
pub fn first_content_line(content: &str) -> Option<String> {
    let lines: Vec<&str> = text::lf_lines(content).collect();
    let mut index = first_non_blank(&lines, 0);

    // An unterminated leading `---` is ordinary content (a stray rule).
    if index < lines.len() && is_delimiter(lines[index]) {
        if let Some(close) = (index + 1..lines.len()).find(|&j| is_delimiter(lines[j])) {
            index = first_non_blank(&lines, close + 1);
        }
    }
    lines.get(index).map(|line| text::trim_ws(line).to_string())
}

fn is_delimiter(line: &str) -> bool {
    text::trim_ws(line) == "---"
}

fn first_non_blank(lines: &[&str], start: usize) -> usize {
    (start..lines.len()).find(|&i| !text::trim_ws(lines[i]).is_empty()).unwrap_or(lines.len())
}

#[cfg(test)]
mod tests {
    //! Ported from `Tests/NoteCardSummaryTests.swift`.
    use super::*;

    fn check(content: &str, expected: Option<&str>, message: &str) {
        assert_eq!(first_content_line(content).as_deref(), expected, "{message}");
    }

    #[test]
    fn plain_content_returns_first_line() {
        check("Hello\nWorld", Some("Hello"), "plain content returns first line");
    }

    #[test]
    fn first_line_is_not_skipped() {
        check("First line\nSecond line", Some("First line"), "does not skip the first content line");
    }

    #[test]
    fn leading_blanks_are_skipped() {
        check("\n\n  \nReal start", Some("Real start"), "leading blank lines skipped");
    }

    #[test]
    fn frontmatter_is_skipped() {
        let fm = "---\ntitle: \"2026-03-08\"\ntags: [ai]\n---\n今天是 2026 年 3 月 8 日";
        check(fm, Some("今天是 2026 年 3 月 8 日"), "leading frontmatter block is skipped");
    }

    #[test]
    fn blanks_after_frontmatter_are_skipped() {
        check("---\ntitle: x\n---\n\n\nBody after blanks", Some("Body after blanks"), "blank lines after frontmatter are skipped too");
    }

    #[test]
    fn frontmatter_only_has_no_content() {
        check("---\ntitle: x\ntags: [a]\n---\n", None, "frontmatter-only note has no content line");
    }

    #[test]
    fn unterminated_fence_is_content() {
        check("---\nstill content", Some("---"), "unterminated leading --- is treated as content");
    }

    #[test]
    fn heading_is_shown_as_is() {
        check("# Heading\nbody", Some("# Heading"), "markdown heading shown as first line");
    }

    #[test]
    fn empty_content_is_none() {
        check("", None, "empty content -> nil");
        check("   \n\n", None, "blank-only content -> nil");
    }
}

#[cfg(test)]
mod extra_tests {
    //! Cases the Swift suite lacks. Expected values were checked against the
    //! Swift code unless a comment says the port differs on purpose.
    use super::*;

    fn check(content: &str, expected: Option<&str>) {
        assert_eq!(first_content_line(content).as_deref(), expected, "{content:?}");
    }

    /// The Swift code differs here on purpose: it cuts at `\n` and keeps the
    /// `\r`, so a CRLF line ends in `\r`, `---\r` is no fence and `\r` isn't
    /// blank, and the card shows `---`.
    #[test]
    fn crlf_notes() {
        check("甲\r\n乙\r\n", Some("甲"));
        check("\r\n\r\n  \r\n正文", Some("正文"));
        check("---\r\ntitle: x\r\n---\r\n\r\n正文\r\n", Some("正文"));
        check("---\r\ntitle: x\r\n---\r\n", None);
    }

    #[test]
    fn frontmatter_at_file_boundaries() {
        check("---", Some("---"));
        check("---\n", Some("---"));
        check("---\n---", None);
        check("---\na: 1\n---", None);
        check("---\na: 1\n...\n正文", Some("---"));
        check("\n\n---\na: 1\n---\n正文", Some("正文"));
        check("\u{FEFF}---\na: 1\n---\n正文", Some("\u{FEFF}---"));
    }

    #[test]
    fn text_stays_whole() {
        check("\u{3000}全角缩进\u{3000}\n", Some("全角缩进"));
        check("  😀🇨🇳 e\u{301}  \n", Some("😀🇨🇳 e\u{301}"));
        check("a\u{2028}b\nc", Some("a\u{2028}b"));
    }
}
