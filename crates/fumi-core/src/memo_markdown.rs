//! A memo's Markdown on the Memos page: its blocks (paragraphs, list items,
//! quotes, code), drawn once a memo is saved, and its syntax spans, which the
//! composer colors while typing. Port of `Services/MemoMarkdown.swift`.
//!
//! Lines split on every `\n`, as `components(separatedBy:)` does, so a line of
//! a CRLF memo keeps its `\r`. Span ranges are UTF-8 byte offsets.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use crate::fuzzy_search::foundation::{
    has_prefix, is_regex_end, is_regex_line_terminator, is_word_char, trim_whitespaces,
};
use crate::memo_format;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    Bullet,
    /// "1." or "1)", as written.
    Ordered(String),
    Task { checked: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Consecutive lines, their line breaks kept.
    Paragraph(String),
    /// `line` is the item's line in the memo, for ticking a to-do.
    Item { level: usize, marker: Marker, text: String, line: usize },
    /// Consecutive `>` lines without their `>`.
    Quote(String),
    Code { language: String, text: String },
}

static LIST_HEAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([ \t]*)([-*+]|\d+[.)])[ \t]+").expect("valid list pattern"));
static QUOTE_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ {0,3}((?:>[ \t]?)+)").expect("valid quote pattern"));

/// `^([ \t]*)([-*+]|\d+[.)])[ \t]+(\[[ xX]\](?:[ \t]+|$))?`, by hand for ICU's `$`.
struct ListPrefix<'a> {
    indent: &'a str,
    token: &'a str,
    /// The byte range of the `[ ]` group, trailing blanks included.
    task: Option<Range<usize>>,
    end: usize,
}

fn list_prefix(line: &str) -> Option<ListPrefix<'_>> {
    let captures = LIST_HEAD.captures(line)?;
    let head = captures.get(0)?.end();
    let mut prefix = ListPrefix {
        indent: captures.get(1)?.as_str(),
        token: captures.get(2)?.as_str(),
        task: None,
        end: head,
    };
    let rest = line[head..].as_bytes();
    if rest.len() >= 3 && rest[0] == b'[' && matches!(rest[1], b' ' | b'x' | b'X') && rest[2] == b']' {
        let after = &line[head + 3..];
        let blanks = after.len() - after.trim_start_matches([' ', '\t']).len();
        if blanks > 0 || is_regex_end(after) {
            prefix.task = Some(head..head + 3 + blanks);
            prefix.end = head + 3 + blanks;
        }
    }
    Some(prefix)
}

/// The fence a line opens or closes, or `None`.
fn fence(line: &str) -> Option<&'static str> {
    let trimmed = trim_whitespaces(line);
    if has_prefix(trimmed, "```") {
        Some("```")
    } else if has_prefix(trimmed, "~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn item(line: &str, index: usize) -> Option<Block> {
    let prefix = list_prefix(line)?;
    let columns: usize = prefix.indent.bytes().map(|b| if b == b'\t' { 4 } else { 1 }).sum();
    let marker = if let Some(task) = &prefix.task {
        Marker::Task { checked: line[task.clone()].contains(['x', 'X']) }
    } else if matches!(prefix.token, "-" | "*" | "+") {
        Marker::Bullet
    } else {
        Marker::Ordered(prefix.token.to_string())
    };
    Some(Block::Item { level: (columns / 2).min(4), marker, text: line[prefix.end..].to_string(), line: index })
}

pub fn blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut quote: Vec<&str> = Vec::new();
    let mut code: Option<(&str, String, Vec<&str>)> = None;

    fn flush(blocks: &mut Vec<Block>, paragraph: &mut Vec<&str>, quote: &mut Vec<&str>) {
        if !paragraph.is_empty() {
            blocks.push(Block::Paragraph(paragraph.join("\n")));
        }
        if !quote.is_empty() {
            blocks.push(Block::Quote(quote.join("\n")));
        }
        paragraph.clear();
        quote.clear();
    }

    for (index, line) in text.split('\n').enumerate() {
        if let Some((open, language, lines)) = &mut code {
            if fence(line) == Some(*open) {
                blocks.push(Block::Code { language: std::mem::take(language), text: lines.join("\n") });
                code = None;
            } else {
                lines.push(line);
            }
            continue;
        }
        if let Some(mark) = fence(line) {
            flush(&mut blocks, &mut paragraph, &mut quote);
            let info = &trim_whitespaces(line)[3..];
            code = Some((mark, trim_whitespaces(info).to_string(), Vec::new()));
        } else if trim_whitespaces(line).is_empty() {
            flush(&mut blocks, &mut paragraph, &mut quote);
        } else if let Some(m) = QUOTE_PREFIX.find(line) {
            if !paragraph.is_empty() {
                flush(&mut blocks, &mut paragraph, &mut quote);
            }
            quote.push(&line[m.end()..]);
        } else if let Some(item) = item(line, index) {
            flush(&mut blocks, &mut paragraph, &mut quote);
            blocks.push(item);
        } else {
            if !quote.is_empty() {
                flush(&mut blocks, &mut paragraph, &mut quote);
            }
            paragraph.push(line);
        }
    }
    // An unclosed fence still shows as code.
    if let Some((_, language, lines)) = code {
        blocks.push(Block::Code { language, text: lines.join("\n") });
    }
    flush(&mut blocks, &mut paragraph, &mut quote);
    blocks
}

/// `text` with the to-do on `line` ticked or unticked. One byte changes.
pub fn toggling_task(line: usize, text: &str) -> String {
    let mut start = 0;
    let mut found = None;
    for (index, current) in text.split('\n').enumerate() {
        if index == line {
            found = Some(current);
            break;
        }
        start += current.len() + 1;
    }
    let Some(current) = found else {
        return text.to_string();
    };
    let Some(task) = list_prefix(current).and_then(|prefix| prefix.task) else {
        return text.to_string();
    };
    let at = start + task.start + 1;
    let replacement = if matches!(text.as_bytes()[at], b'x' | b'X') { " " } else { "x" };
    let mut result = text.to_string();
    result.replace_range(at..at + 1, replacement);
    result
}

// Syntax spans.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpanKind {
    /// Syntax characters: `**`, `- `, `> `, fences...
    Marker,
    Strong,
    Emphasis,
    Strike,
    Code,
    Link,
    Tag,
    /// The text of a quote line.
    Quote,
    /// The lines inside a fence.
    CodeBlock,
    /// The text of a ticked to-do.
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub kind: SpanKind,
    pub range: Range<usize>,
}

/// Where the Markdown is in `text`, by kind.
pub fn spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut location = 0;
    let mut open_fence = None;
    for line in text.split('\n') {
        let range = location..location + line.len();
        if let Some(open) = open_fence {
            if fence(line) == Some(open) {
                spans.push(Span { kind: SpanKind::Marker, range });
                open_fence = None;
            } else if !line.is_empty() {
                spans.push(Span { kind: SpanKind::CodeBlock, range });
            }
        } else if let Some(mark) = fence(line) {
            spans.push(Span { kind: SpanKind::Marker, range });
            open_fence = Some(mark);
        } else {
            line_spans(line, location, &mut spans);
        }
        location += line.len() + 1;
    }
    spans
}

fn line_spans(line: &str, offset: usize, spans: &mut Vec<Span>) {
    let mut start = 0;
    let mut content_kind = None;
    if let Some(m) = QUOTE_PREFIX.find(line) {
        start = m.end();
        spans.push(Span { kind: SpanKind::Marker, range: offset..offset + start });
        content_kind = Some(SpanKind::Quote);
    }
    let rest = &line[start..];
    if let Some(prefix) = list_prefix(rest) {
        spans.push(Span { kind: SpanKind::Marker, range: offset + start..offset + start + prefix.end });
        if prefix.task.as_ref().is_some_and(|task| rest[task.clone()].contains(['x', 'X'])) {
            content_kind = Some(SpanKind::Done);
        }
        start += prefix.end;
    }
    if let Some(kind) = content_kind
        && start < line.len()
    {
        spans.push(Span { kind, range: offset + start..offset + line.len() });
    }
    inline_spans(&line[start..], offset + start, spans);
}

/// A delimited inline run the crate's `regex` can't express, for want of
/// lookaround: an opening delimiter, a lazy `.+?` and a closing one.
pub(crate) struct Delimited {
    pub open: &'static str,
    pub close: &'static str,
    /// Refuses the character before the opener, as `(?<!...)`.
    pub refuse_before_open: fn(char) -> bool,
    /// Refuses the first inner character, as `(?!...)` or `(?=\S)`.
    pub refuse_first: fn(char) -> bool,
    /// Refuses the last inner character, as `(?<!...)` or `(?<=\S)`.
    pub refuse_last: fn(char) -> bool,
    /// Refuses the character after the closer, as `(?!...)`.
    pub refuse_after_close: fn(char) -> bool,
}

pub(crate) fn never(_: char) -> bool {
    false
}

impl Delimited {
    /// Non-overlapping matches, leftmost and shortest, as an ICU regex finds
    /// them: `(start, end)` of each whole run, delimiters included.
    pub fn matches(&self, text: &str) -> Vec<(usize, usize)> {
        let mut found = Vec::new();
        let Some(last_close) = text.rfind(self.close) else {
            return found;
        };
        let mut position = 0;
        while let Some(relative) = text.get(position..).and_then(|rest| rest.find(self.open)) {
            let start = position + relative;
            // A run needs a character between its delimiters.
            if start + self.open.len() >= last_close {
                break;
            }
            match self.match_at(text, start) {
                Some(end) => {
                    found.push((start, end));
                    position = end;
                }
                None => position = start + 1,
            }
        }
        found
    }

    fn match_at(&self, text: &str, start: usize) -> Option<usize> {
        if text[..start].chars().next_back().is_some_and(self.refuse_before_open) {
            return None;
        }
        let inner = start + self.open.len();
        let mut chars = text[inner..].char_indices();
        let (_, first) = chars.next()?;
        if is_regex_line_terminator(first) || (self.refuse_first)(first) {
            return None;
        }
        let mut current = first;
        let mut end = inner + first.len_utf8();
        loop {
            if !(self.refuse_last)(current) && text[end..].starts_with(self.close) {
                let close_end = end + self.close.len();
                if !text[close_end..].chars().next().is_some_and(self.refuse_after_close) {
                    return Some(close_end);
                }
            }
            let (_, next) = chars.next()?;
            if is_regex_line_terminator(next) {
                return None;
            }
            current = next;
            end += next.len_utf8();
        }
    }
}

static CODE_RULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^`\n]+`").expect("valid code pattern"));
static LINK_RULE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]\n]+)\]\(([^)\s]+)\)").expect("valid link pattern"));

fn word_or_underscore(c: char) -> bool {
    c == '_' || is_word_char(c)
}
fn space_or_star(c: char) -> bool {
    c.is_whitespace() || c == '*'
}
fn space_or_underscore(c: char) -> bool {
    c.is_whitespace() || c == '_'
}
fn star(c: char) -> bool {
    c == '*'
}

static STRONG_STARS: Delimited = Delimited {
    open: "**",
    close: "**",
    refuse_before_open: never,
    refuse_first: char::is_whitespace,
    refuse_last: char::is_whitespace,
    refuse_after_close: never,
};
static STRONG_UNDERSCORES: Delimited = Delimited {
    open: "__",
    close: "__",
    refuse_before_open: is_word_char,
    refuse_first: char::is_whitespace,
    refuse_last: char::is_whitespace,
    refuse_after_close: is_word_char,
};
static STRIKE: Delimited = Delimited {
    open: "~~",
    close: "~~",
    refuse_before_open: never,
    refuse_first: char::is_whitespace,
    refuse_last: char::is_whitespace,
    refuse_after_close: never,
};
static EMPHASIS_STAR: Delimited = Delimited {
    open: "*",
    close: "*",
    refuse_before_open: star,
    refuse_first: space_or_star,
    refuse_last: space_or_star,
    refuse_after_close: star,
};
static EMPHASIS_UNDERSCORE: Delimited = Delimited {
    open: "_",
    close: "_",
    refuse_before_open: word_or_underscore,
    refuse_first: space_or_underscore,
    refuse_last: space_or_underscore,
    refuse_after_close: word_or_underscore,
};

fn inline_spans(text: &str, offset: usize, spans: &mut Vec<Span>) {
    // Bytes already spoken for: delimiters, and the inside of code.
    let mut taken = vec![false; text.len()];
    fn is_free(taken: &[bool], range: &Range<usize>) -> bool {
        !taken[range.clone()].iter().any(|t| *t)
    }
    fn take(taken: &mut [bool], range: &Range<usize>) {
        taken[range.clone()].fill(true);
    }
    let mut add = |kind: SpanKind, range: &Range<usize>| {
        if !range.is_empty() {
            spans.push(Span { kind, range: offset + range.start..offset + range.end });
        }
    };

    // In precedence order: code shields what's inside it, strong claims its
    // `**` before emphasis looks for `*`.
    for m in CODE_RULE.find_iter(text) {
        let (open, close) = (m.start()..m.start() + 1, m.end() - 1..m.end());
        if is_free(&taken, &m.range()) {
            add(SpanKind::Marker, &open);
            add(SpanKind::Marker, &close);
            add(SpanKind::Code, &(open.end..close.start));
            take(&mut taken, &m.range());
        }
    }
    for captures in LINK_RULE.captures_iter(text) {
        let (Some(whole), Some(inner)) = (captures.get(0), captures.get(1)) else {
            continue;
        };
        let (open, close) = (whole.start()..whole.start() + 1, inner.end()..whole.end());
        if is_free(&taken, &open) && is_free(&taken, &close) {
            add(SpanKind::Marker, &open);
            add(SpanKind::Marker, &close);
            add(SpanKind::Link, &inner.range());
            take(&mut taken, &open);
            take(&mut taken, &close);
        }
    }
    let rules: [(&Delimited, SpanKind); 5] = [
        (&STRONG_STARS, SpanKind::Strong),
        (&STRONG_UNDERSCORES, SpanKind::Strong),
        (&STRIKE, SpanKind::Strike),
        (&EMPHASIS_STAR, SpanKind::Emphasis),
        (&EMPHASIS_UNDERSCORE, SpanKind::Emphasis),
    ];
    for (rule, kind) in rules {
        for (start, end) in rule.matches(text) {
            let open = start..start + rule.open.len();
            let close = end - rule.close.len()..end;
            if is_free(&taken, &open) && is_free(&taken, &close) {
                add(SpanKind::Marker, &open);
                add(SpanKind::Marker, &close);
                add(kind, &(open.end..close.start));
                take(&mut taken, &open);
                take(&mut taken, &close);
            }
        }
    }
    for (_, range) in memo_format::tag_ranges(text) {
        if is_free(&taken, &range) {
            add(SpanKind::Tag, &range);
        }
    }
}

#[cfg(test)]
mod tests {
    // Ported from Tests/MemoMarkdownTests.swift.
    use super::*;

    /// The text of each span of `kind`, in reading order.
    fn texts<'a>(kind: SpanKind, text: &'a str) -> Vec<&'a str> {
        let mut found: Vec<Span> = spans(text).into_iter().filter(|s| s.kind == kind).collect();
        found.sort_by_key(|s| s.range.start);
        found.into_iter().map(|s| &text[s.range]).collect()
    }

    fn item(level: usize, marker: Marker, text: &str, line: usize) -> Block {
        Block::Item { level, marker, text: text.to_string(), line }
    }

    #[test]
    fn blocks_of_memos() {
        let memo = "想到两件事：\n先记下来\n\n- 买咖啡豆\n  - 浅烘\n1. 第一\n- [ ] 回邮件 #work\n- [x] 交周报\n> 引用一行\n> 第二行\n```swift\nlet a = 1\n\nprint(a)\n```\n收尾";
        assert_eq!(
            blocks(memo),
            [
                Block::Paragraph("想到两件事：\n先记下来".into()),
                item(0, Marker::Bullet, "买咖啡豆", 3),
                item(1, Marker::Bullet, "浅烘", 4),
                item(0, Marker::Ordered("1.".into()), "第一", 5),
                item(0, Marker::Task { checked: false }, "回邮件 #work", 6),
                item(0, Marker::Task { checked: true }, "交周报", 7),
                Block::Quote("引用一行\n第二行".into()),
                Block::Code { language: "swift".into(), text: "let a = 1\n\nprint(a)".into() },
                Block::Paragraph("收尾".into()),
            ],
            "blocks of a mixed memo"
        );

        assert_eq!(blocks("plain\nlines"), [Block::Paragraph("plain\nlines".into())], "plain memo is one paragraph");
        assert_eq!(blocks("```\nopen"), [Block::Code { language: "".into(), text: "open".into() }], "unclosed fence is still code");
        assert_eq!(blocks("#tag not a heading"), [Block::Paragraph("#tag not a heading".into())], "a tag line stays a paragraph");
        assert_eq!(blocks("-not a list\n2*3"), [Block::Paragraph("-not a list\n2*3".into())], "markers need a space");
        assert_eq!(blocks("- [ ]"), [item(0, Marker::Task { checked: false }, "", 0)], "empty to-do");
    }

    #[test]
    fn ticking_to_dos() {
        let todo = "list\n- [ ] one\n- [X] two";
        assert_eq!(toggling_task(1, todo), "list\n- [x] one\n- [X] two", "tick");
        assert_eq!(toggling_task(2, todo), "list\n- [ ] one\n- [ ] two", "untick");
        assert_eq!(toggling_task(0, todo), todo, "not a to-do: unchanged");
        assert_eq!(toggling_task(9, todo), todo, "out of range: unchanged");
    }

    #[test]
    fn syntax_spans() {
        let inline = "a **bold** and *it* or _it2_, `co*de*` ~~gone~~ [site](https://x.y/a_b_c) #tag";
        assert_eq!(texts(SpanKind::Strong, inline), ["bold"], "strong");
        assert_eq!(texts(SpanKind::Emphasis, inline), ["it", "it2"], "emphasis, not inside code or the URL");
        assert_eq!(texts(SpanKind::Code, inline), ["co*de*"], "inline code");
        assert_eq!(texts(SpanKind::Strike, inline), ["gone"], "strike");
        assert_eq!(texts(SpanKind::Link, inline), ["site"], "link text");
        assert_eq!(texts(SpanKind::Tag, inline), ["#tag"], "tag");
        assert_eq!(
            texts(SpanKind::Marker, inline),
            ["**", "**", "*", "*", "_", "_", "`", "`", "~~", "~~", "[", "](https://x.y/a_b_c)"],
            "delimiters"
        );

        assert!(texts(SpanKind::Marker, "2 * 3 * 4 and snake_case_name").is_empty(), "no stray emphasis");
        assert_eq!(texts(SpanKind::Strong, "**粗体**中文"), ["粗体"], "strong beside CJK");

        let lines = "- [x] done *now*\n> - quoted\n```\n**raw**\n```";
        assert_eq!(texts(SpanKind::Done, lines), ["done *now*"], "ticked to-do text");
        assert_eq!(texts(SpanKind::Emphasis, lines), ["now"], "inline inside a to-do");
        assert_eq!(texts(SpanKind::Quote, lines), ["quoted"], "quote text after its list marker");
        assert_eq!(texts(SpanKind::CodeBlock, lines), ["**raw**"], "fenced lines are code");
        assert!(texts(SpanKind::Strong, lines).is_empty(), "no inline styling in a fence");
        assert_eq!(texts(SpanKind::Marker, lines), ["- [x] ", "*", "*", "> ", "- ", "```", "```"], "line markers and fences");
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    #[test]
    fn toggling_changes_one_byte_only() {
        let memo = "一\r\n- [ ] 二 😀\u{2028}\r\n\t* [X]\t三\r\n  结尾 ";
        let ticked = toggling_task(1, memo);
        assert_eq!(ticked, "一\r\n- [x] 二 😀\u{2028}\r\n\t* [X]\t三\r\n  结尾 ");
        assert_eq!(toggling_task(1, &ticked), memo, "and back");
        assert_eq!(toggling_task(2, memo), "一\r\n- [ ] 二 😀\u{2028}\r\n\t* [ ]\t三\r\n  结尾 ");
        assert_eq!(toggling_task(3, memo), memo, "not a to-do");
    }

    #[test]
    fn a_crlf_memo_keeps_its_cr_as_swift_does() {
        assert_eq!(blocks("a\r\nb"), [Block::Paragraph("a\r\nb".into())]);
        assert_eq!(blocks("- [ ]\r\nb"), [
            Block::Item { level: 0, marker: Marker::Task { checked: false }, text: "\r".into(), line: 0 },
            Block::Paragraph("b".into()),
        ], "ICU's $ matches before the CR");
        assert_eq!(blocks("\r\na"), [Block::Paragraph("\r\na".into())], "a lone CR line isn't blank");
        let found = spans("**粗**\r\n*斜*");
        let text = "**粗**\r\n*斜*";
        let strong = found.iter().find(|s| s.kind == SpanKind::Strong).unwrap();
        assert_eq!(&text[strong.range.clone()], "粗");
        let emphasis = found.iter().find(|s| s.kind == SpanKind::Emphasis).unwrap();
        assert_eq!(&text[emphasis.range.clone()], "斜");
    }

    #[test]
    fn levels_and_markers() {
        let level = |line: &str| match blocks(line).first() {
            Some(Block::Item { level, .. }) => *level,
            other => panic!("not an item: {other:?}"),
        };
        assert_eq!(level(" - a"), 0);
        assert_eq!(level("  - a"), 1);
        assert_eq!(level("\t- a"), 2);
        assert_eq!(level("                - a"), 4, "capped");
        assert_eq!(blocks("١) a"), [Block::Item { level: 0, marker: Marker::Ordered("١)".into()), text: "a".into(), line: 0 }]);
        assert_eq!(blocks("- [x]y"), [Block::Item { level: 0, marker: Marker::Bullet, text: "[x]y".into(), line: 0 }]);
    }

    #[test]
    fn quote_and_fence_edges() {
        assert_eq!(blocks("   > a"), [Block::Quote("a".into())]);
        assert_eq!(blocks("    > a"), [Block::Paragraph("    > a".into())]);
        assert_eq!(blocks(">> a\n>b"), [Block::Quote("a\nb".into())]);
        assert_eq!(blocks("``` rust \nx\n~~~\n```"), [Block::Code { language: "rust".into(), text: "x\n~~~".into() }]);
        assert_eq!(blocks("a\n> b\nc"), [
            Block::Paragraph("a".into()),
            Block::Quote("b".into()),
            Block::Paragraph("c".into()),
        ]);
    }

    fn texts<'a>(kind: SpanKind, text: &'a str) -> Vec<&'a str> {
        spans(text).into_iter().filter(|s| s.kind == kind).map(|s| &text[s.range]).collect()
    }

    #[test]
    fn span_ranges_are_bytes_on_character_boundaries() {
        let text = "😀 **粗体👨\u{200D}👩\u{200D}👧** _斜体_ `代码` ~~删~~ [链接](u) #标签\n> 引用 *中文*";
        for span in spans(text) {
            assert!(text.is_char_boundary(span.range.start) && text.is_char_boundary(span.range.end));
        }
        assert_eq!(texts(SpanKind::Strong, text), ["粗体👨\u{200D}👩\u{200D}👧"]);
        assert_eq!(texts(SpanKind::Emphasis, text), ["斜体", "中文"]);
        assert_eq!(texts(SpanKind::Code, text), ["代码"]);
        assert_eq!(texts(SpanKind::Strike, text), ["删"]);
        assert_eq!(texts(SpanKind::Link, text), ["链接"]);
        assert_eq!(texts(SpanKind::Tag, text), ["#标签"]);
        assert_eq!(texts(SpanKind::Quote, text), ["引用 *中文*"]);
    }

    #[test]
    fn delimiter_rules_as_the_regexes_have_them() {
        assert_eq!(texts(SpanKind::Strong, "***a**"), ["*a"]);
        assert_eq!(texts(SpanKind::Strong, "** a** **b **"), Vec::<&str>::new());
        assert_eq!(texts(SpanKind::Strong, "x__a__y __b__"), ["b"]);
        assert_eq!(texts(SpanKind::Strong, "__a__b__"), ["a__b"], "the closer can't be followed by a word");
        assert_eq!(texts(SpanKind::Emphasis, "中_文_字 _中文_"), ["中文"], "CJK counts as a word");
        assert_eq!(texts(SpanKind::Emphasis, "*a\u{2028}b*"), Vec::<&str>::new(), "`.` stops at a line break");
        assert_eq!(texts(SpanKind::Code, "`` a `b`"), [" a "], "from running the Swift code");
        assert_eq!(texts(SpanKind::Tag, "`#no` #yes"), ["#yes"]);
        assert_eq!(texts(SpanKind::Emphasis, "`*a` b*"), Vec::<&str>::new(), "an opener inside code is taken");
    }
}
