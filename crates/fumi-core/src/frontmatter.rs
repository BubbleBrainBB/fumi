//! A note's leading YAML frontmatter block (`---` … `---`), parsed just enough
//! to show it as a properties panel. Port of `Services/FrontmatterParser.swift`.
//!
//! Only the flat shapes notes actually use are understood: `key: value`,
//! `key: [a, b]` and `key:` followed by `- item` lines.
//!
//! This file also holds [`text`], the Foundation and Swift `String` behavior
//! that every module of this group needs.

use std::ops::Range;

/// A field's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    List(Vec<String>),
}

/// One `key: value` of the block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub value: Value,
}

/// A parsed frontmatter block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    /// Byte range of the whole block in the source, from the opening `---`
    /// through the end of the closing line, its line ending excluded.
    pub range: Range<usize>,
    pub fields: Vec<Field>,
}

/// The Swift app stops looking for the closing fence after this many lines
/// have been read past the first.
const MAX_LINE_INDEX: usize = 200;

impl Frontmatter {
    /// Finds and parses a frontmatter block at the very start of `string`. A
    /// BOM or any other text before the opening `---` means there is none.
    pub fn parse(string: &str) -> Option<Frontmatter> {
        if !string.starts_with("---") {
            return None;
        }
        let lines: Vec<text::Line> = text::lines(string).take(MAX_LINE_INDEX + 1).collect();
        if text::trim_ws(&string[lines.first()?.content.clone()]) != "---" {
            return None;
        }
        let close = (1..lines.len()).find(|&i| {
            let t = text::trim_ws(&string[lines[i].content.clone()]);
            t == "---" || t == "..."
        })?;
        let body: Vec<&str> = lines[1..close].iter().map(|l| &string[l.content.clone()]).collect();
        Some(Frontmatter { range: 0..lines[close].content.end, fields: Frontmatter::parse_fields(&body) })
    }

    /// Reads the lines between the fences into fields, in order.
    pub fn parse_fields(lines: &[&str]) -> Vec<Field> {
        let mut fields = Vec::new();
        let mut pending_key: Option<String> = None;
        let mut pending_items: Vec<String> = Vec::new();

        fn flush(fields: &mut Vec<Field>, key: &mut Option<String>, items: &mut Vec<String>) {
            if let Some(key) = key.take() {
                let value = if items.is_empty() { Value::Text(String::new()) } else { Value::List(std::mem::take(items)) };
                fields.push(Field { key, value });
            }
        }

        for raw in lines {
            let trimmed = text::trim_ws(raw);
            if trimmed.is_empty() || text::has_prefix(trimmed, "#") {
                continue;
            }
            if pending_key.is_some() && (text::has_prefix(trimmed, "- ") || trimmed == "-") {
                let item = text::unquote(text::trim_ws(text::drop_first(trimmed, 1)));
                if !item.is_empty() {
                    pending_items.push(item.to_string());
                }
                continue;
            }
            flush(&mut fields, &mut pending_key, &mut pending_items);

            let Some(colon) = text::find_char(trimmed, ":") else { continue };
            let key = text::trim_ws(&trimmed[..colon]);
            let value = text::trim_ws(&trimmed[colon + 1..]);
            if key.is_empty() {
                continue;
            }
            if value.is_empty() {
                pending_key = Some(key.to_string());
            } else if text::has_prefix(value, "[") && text::has_suffix(value, "]") {
                let items = text::split_on(&value[1..value.len() - 1], ",", false)
                    .into_iter()
                    .map(|item| text::unquote(text::trim_ws(item)).to_string())
                    .filter(|item| !item.is_empty())
                    .collect();
                fields.push(Field { key: key.to_string(), value: Value::List(items) });
            } else {
                fields.push(Field { key: key.to_string(), value: Value::Text(text::unquote(value).to_string()) });
            }
        }
        flush(&mut fields, &mut pending_key, &mut pending_items);
        fields
    }
}

/// What `String`, `NSString` and `CharacterSet` do that `str` does not, for
/// the modules ported from them. A Swift `Character` is a grapheme cluster
/// compared by canonical equivalence, so `"#\u{FE0F}\u{20E3}"` (the keycap
/// emoji) does not start with `#`, while plain `starts_with` says it does.
pub(crate) mod text {
    use std::ops::Range;
    use unicode_normalization::UnicodeNormalization;
    use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

    /// `CharacterSet.whitespaces`, as dumped from Foundation: general category
    /// Zs, tab, and U+200B. No line endings, and no U+FEFF.
    pub fn is_ws(c: char) -> bool {
        matches!(c, '\t' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200B}' | '\u{202F}' | '\u{205F}' | '\u{3000}')
    }

    /// `trimmingCharacters(in: .whitespaces)`.
    pub fn trim_ws(s: &str) -> &str {
        s.trim_matches(is_ws)
    }

    /// A line as `NSString` sees one.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Line {
        /// The line without its ending.
        pub content: Range<usize>,
        /// The line with its ending.
        pub full: Range<usize>,
    }

    /// `enumerateSubstrings(options: .byLines)` and `lineRange(for:)`. Endings
    /// are `\n`, `\r`, `\r\n`, U+0085, U+2028 and U+2029; VT and FF are not.
    /// A final ending starts no extra line, and `""` has no lines.
    pub fn lines(s: &str) -> impl Iterator<Item = Line> + '_ {
        let bytes = s.as_bytes();
        let mut start = 0;
        std::iter::from_fn(move || {
            if start >= bytes.len() {
                return None;
            }
            let mut i = start;
            while i < bytes.len() {
                let ending = match bytes[i] {
                    b'\n' => 1,
                    b'\r' if bytes.get(i + 1) == Some(&b'\n') => 2,
                    b'\r' => 1,
                    0xC2 if bytes.get(i + 1) == Some(&0x85) => 2,
                    0xE2 if bytes.get(i + 1) == Some(&0x80) && matches!(bytes.get(i + 2), Some(&0xA8) | Some(&0xA9)) => 3,
                    _ => 0,
                };
                if ending > 0 {
                    let line = Line { content: start..i, full: start..i + ending };
                    start = i + ending;
                    return Some(line);
                }
                i += 1;
            }
            let line = Line { content: start..bytes.len(), full: start..bytes.len() };
            start = bytes.len();
            Some(line)
        })
    }

    /// Splits on `\n`, a `\r` before it going with the ending. This is what
    /// `split(separator: "\n")` was meant to do: in Swift `"\r\n"` is one
    /// `Character` that isn't `"\n"`, so a CRLF note reads as one long line.
    /// A `\r` not before a `\n` stays in its line.
    pub fn lf_lines(s: &str) -> impl Iterator<Item = &str> + '_ {
        lf_lines_limited(s, usize::MAX)
    }

    /// [`lf_lines`] cutting at most `max_splits` times; the last piece keeps
    /// the rest of the text.
    pub fn lf_lines_limited(s: &str, max_splits: usize) -> impl Iterator<Item = &str> + '_ {
        let mut rest = Some(s);
        let mut splits = 0;
        std::iter::from_fn(move || {
            let current = rest?;
            match current.find('\n').filter(|_| splits < max_splits) {
                Some(newline) => {
                    splits += 1;
                    rest = Some(&current[newline + 1..]);
                    let line = &current[..newline];
                    Some(line.strip_suffix('\r').unwrap_or(line))
                }
                None => {
                    rest = None;
                    Some(current)
                }
            }
        })
    }

    /// Whether `index` falls between two grapheme clusters of `s`.
    pub fn is_boundary(s: &str, index: usize) -> bool {
        if index == 0 || index == s.len() {
            return true;
        }
        // With the whole string as the chunk, the cursor never asks for more.
        GraphemeCursor::new(index, s.len(), true).is_boundary(s, 0).unwrap_or(false)
    }

    /// `String.hasPrefix`, for a `prefix` of ASCII text.
    pub fn has_prefix(s: &str, prefix: &str) -> bool {
        s.starts_with(prefix) && is_boundary(s, prefix.len())
    }

    /// `String.hasSuffix`, for a `suffix` of ASCII text.
    pub fn has_suffix(s: &str, suffix: &str) -> bool {
        s.ends_with(suffix) && is_boundary(s, s.len() - suffix.len())
    }

    /// `String.contains`, for a `needle` of ASCII text: it must start and end
    /// on grapheme boundaries.
    pub fn contains(s: &str, needle: &str) -> bool {
        s.match_indices(needle).any(|(i, _)| is_boundary(s, i) && is_boundary(s, i + needle.len()))
    }

    /// `s.dropFirst(n)`, counted in grapheme clusters.
    pub fn drop_first(s: &str, n: usize) -> &str {
        match s.grapheme_indices(true).nth(n) {
            Some((i, _)) => &s[i..],
            None => "",
        }
    }

    /// `s.dropLast(n)`, counted in grapheme clusters.
    pub fn drop_last(s: &str, n: usize) -> &str {
        if n == 0 {
            return s;
        }
        match s.grapheme_indices(true).rev().nth(n - 1) {
            Some((i, _)) => &s[..i],
            None => "",
        }
    }

    /// `s.count`.
    pub fn count(s: &str) -> usize {
        s.graphemes(true).count()
    }

    /// `s.firstIndex(of:)` for a one-character `c`, as a byte offset.
    pub fn find_char(s: &str, c: &str) -> Option<usize> {
        s.grapheme_indices(true).find(|(_, g)| *g == c).map(|(i, _)| i)
    }

    /// `s.split(separator:)` for a one-character separator.
    pub fn split_on<'a>(s: &'a str, separator: &str, keep_empty: bool) -> Vec<&'a str> {
        let mut parts = Vec::new();
        let mut start = 0;
        for (i, g) in s.grapheme_indices(true) {
            if g == separator {
                if keep_empty || i > start {
                    parts.push(&s[start..i]);
                }
                start = i + g.len();
            }
        }
        if keep_empty || s.len() > start {
            parts.push(&s[start..]);
        }
        parts
    }

    /// Removes one pair of matching `"` or `'` around `s`.
    pub fn unquote(s: &str) -> &str {
        let mut graphemes = s.graphemes(true);
        let (Some(first), Some(last)) = (graphemes.next(), graphemes.next_back()) else { return s };
        if first == last && (first == "\"" || first == "'") { &s[first.len()..s.len() - last.len()] } else { s }
    }

    /// `String.lowercased()`: each scalar on its own, so `Σ` is always `σ` and
    /// never the final `ς` that `str::to_lowercase` picks.
    pub fn lowercased(s: &str) -> String {
        s.chars().flat_map(char::to_lowercase).collect()
    }

    /// The form to compare or hash strings by. Swift's `==`, `<` and `Set` all
    /// go by canonical equivalence; `<` then orders by scalar, as bytes do.
    pub fn canonical(s: &str) -> String {
        if s.is_ascii() { s.to_string() } else { s.nfc().collect() }
    }

    /// Swift's `==` on `String`.
    pub fn canon_eq(a: &str, b: &str) -> bool {
        a == b || (!(a.is_ascii() && b.is_ascii()) && a.nfc().eq(b.nfc()))
    }

    /// What's left of `s` after `prefix`, compared `Character` by `Character`
    /// as `hasPrefix` does; the rest keeps its own bytes.
    pub fn strip_prefix_canon<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
        let mut graphemes = s.grapheme_indices(true);
        for wanted in prefix.graphemes(true) {
            let (_, g) = graphemes.next()?;
            if !canon_eq(g, wanted) {
                return None;
            }
        }
        Some(graphemes.next().map_or("", |(i, _)| &s[i..]))
    }

    /// `NSString.lastPathComponent`: `"a/b/"` is `b`, `"//"` is `/`.
    pub fn last_path_component(path: &str) -> &str {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() {
            return if path.is_empty() { "" } else { "/" };
        }
        trimmed.rsplit('/').next().unwrap_or(trimmed)
    }

    /// `NSString.deletingLastPathComponent`: `"/a"` is `/`, `"a"` is empty.
    /// Unlike it this leaves a `//` further up alone: every caller splits or
    /// standardizes the result.
    pub fn deleting_last_path_component(path: &str) -> &str {
        let trimmed = path.trim_end_matches('/');
        if trimmed.is_empty() {
            return if path.is_empty() { "" } else { "/" };
        }
        match trimmed.rfind('/') {
            None => "",
            Some(slash) => match trimmed[..slash].trim_end_matches('/') {
                "" => "/",
                head => head,
            },
        }
    }

    /// `URL.standardizedFileURL.path` for an absolute path: `.`, `..`, empty
    /// components and trailing slashes worked out, with no file system access.
    pub fn standardized_path(path: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(part),
            }
        }
        format!("/{}", parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    //! Ported from `Tests/FrontmatterTitleTests.swift` (its frontmatter half;
    //! the titles are in `title.rs`).
    use super::*;

    const DOC: &str = "---\ntags: [规则, \"llm-wiki\"]\naliases:\n  - CLAUDE\n  - 'agent rules'\nstatus: draft\n# a comment\n---\n# CLAUDE.md";

    fn list(items: &[&str]) -> Value {
        Value::List(items.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn parses_the_flat_shapes() {
        let fm = Frontmatter::parse(DOC).unwrap();
        let keys: Vec<&str> = fm.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["tags", "aliases", "status"], "keys in order, comments skipped");
        assert_eq!(fm.fields[0].value, list(&["规则", "llm-wiki"]), "flow list, quotes stripped");
        assert_eq!(fm.fields[1].value, list(&["CLAUDE", "agent rules"]), "block list");
        assert_eq!(fm.fields[2].value, Value::Text("draft".into()), "scalar");
        let close_end = DOC.rfind("---").unwrap() + 3;
        assert_eq!(fm.range, 0..close_end, "range ends at the closing fence, newline excluded");
    }

    #[test]
    fn what_is_not_frontmatter() {
        assert_eq!(Frontmatter::parse("# Title\n---\nnot: frontmatter\n---"), None, "must start the document");
        assert_eq!(Frontmatter::parse("---\ntags: [a]\n"), None, "unclosed block is not frontmatter");
        assert_eq!(
            Frontmatter::parse("---\nempty:\n---").map(|f| f.fields),
            Some(vec![Field { key: "empty".into(), value: Value::Text(String::new()) }]),
            "key with no value"
        );
    }
}

#[cfg(test)]
mod extra_tests {
    //! Cases the Swift suite lacks. Expected values were checked against the
    //! Swift code unless a comment says the port differs on purpose.
    use super::*;

    #[test]
    fn crlf_block_and_its_range() {
        let doc = "---\r\n标题: 笔记\r\ntags:\r\n  - 甲\r\n---\r\n正文\r\n";
        let fm = Frontmatter::parse(doc).unwrap();
        assert_eq!(fm.fields[0], Field { key: "标题".into(), value: Value::Text("笔记".into()) });
        assert_eq!(fm.fields[1].value, Value::List(vec!["甲".into()]));
        assert_eq!(&doc[fm.range.clone()], "---\r\n标题: 笔记\r\ntags:\r\n  - 甲\r\n---", "the CRLF after the fence is outside");
        assert_eq!(&doc[fm.range.end..], "\r\n正文\r\n");
    }

    #[test]
    fn range_is_in_bytes_and_on_char_boundaries() {
        let doc = "---\n😀: 中文 🇨🇳\n---\n后文";
        let fm = Frontmatter::parse(doc).unwrap();
        assert_eq!(fm.fields[0].value, Value::Text("中文 🇨🇳".into()));
        assert!(doc.is_char_boundary(fm.range.end));
        assert_eq!(&doc[fm.range.end..], "\n后文");
    }

    #[test]
    fn bom_before_the_fence_is_not_frontmatter() {
        assert_eq!(Frontmatter::parse("\u{FEFF}---\na: 1\n---\n"), None);
    }

    #[test]
    fn file_boundaries() {
        assert_eq!(Frontmatter::parse(""), None);
        assert_eq!(Frontmatter::parse("--"), None);
        assert_eq!(Frontmatter::parse("---"), None, "a lone fence closes nothing");
        assert_eq!(Frontmatter::parse("---\n"), None);
        assert_eq!(Frontmatter::parse("----\na: 1\n---"), None, "the first line must be just the fence");
        assert_eq!(Frontmatter::parse("  ---\na: 1\n---"), None, "indented fence: the document must start with it");
        let at_eof = Frontmatter::parse("---\na: 1\n---").unwrap();
        assert_eq!(at_eof.range, 0..12, "closing fence at the end of the file, no final newline");
        let dots = Frontmatter::parse("---\na: 1\n...\nb").unwrap();
        assert_eq!(dots.range, 0..12, "`...` closes too");
        let spaced = Frontmatter::parse("---  \na: 1\n ---  \nb").unwrap();
        assert_eq!(spaced.range, 0..17, "whitespace on the closing line is inside the range");
    }

    #[test]
    fn close_must_come_within_the_line_cap() {
        let doc = |lines: usize| format!("---\n{}---\n", "a: 1\n".repeat(lines));
        assert_eq!(Frontmatter::parse(&doc(199)).unwrap().fields.len(), 199, "fence on the 201st line");
        assert_eq!(Frontmatter::parse(&doc(200)), None, "fence on the 202nd line is too late");
    }

    #[test]
    fn other_line_endings_end_lines() {
        for ending in ["\r", "\u{85}", "\u{2028}", "\u{2029}"] {
            let doc = ["---", "a: 1", "---", "b"].join(ending);
            let fm = Frontmatter::parse(&doc).unwrap_or_else(|| panic!("{ending:?}"));
            assert_eq!(fm.fields[0].value, Value::Text("1".into()));
            assert_eq!(&doc[fm.range.end..fm.range.end + ending.len()], ending);
        }
        for not_an_ending in ["\u{B}", "\u{C}"] {
            let doc = ["---", "a: 1", "---", "b"].join(not_an_ending);
            assert_eq!(Frontmatter::parse(&doc), None, "{not_an_ending:?}");
        }
    }

    #[test]
    fn whitespace_is_the_foundation_set() {
        let fm = Frontmatter::parse("---\u{3000}\n\u{A0}a\u{2003}:\u{200B}b\u{3000}\n---").unwrap();
        assert_eq!(fm.fields, [Field { key: "a".into(), value: Value::Text("b".into()) }]);
        let fm = Frontmatter::parse("---\nk: \u{FEFF}v\n---").unwrap();
        assert_eq!(fm.fields[0].value, Value::Text("\u{FEFF}v".into()), "a BOM is not whitespace");
    }

    #[test]
    fn characters_are_grapheme_clusters() {
        let keycap = "#\u{FE0F}\u{20E3}";
        let fields = Frontmatter::parse_fields(&[&format!("{keycap}: 1")]);
        assert_eq!(fields[0].key, keycap, "a keycap emoji is not a comment");
        let fields = Frontmatter::parse_fields(&["a:\u{301}b: c"]);
        assert_eq!(fields[0].key, "a:\u{301}b", "a colon under a combining mark doesn't split");
        let fields = Frontmatter::parse_fields(&["k: \"\u{301}x\""]);
        assert_eq!(fields[0].value, Value::Text("\"\u{301}x\"".into()), "a marked quote isn't a quote");
        let fields = Frontmatter::parse_fields(&["k: [a,\u{301}b, c]"]);
        assert_eq!(fields[0].value, Value::List(vec!["a,\u{301}b".into(), "c".into()]));
    }

    #[test]
    fn field_shapes() {
        let fields = Frontmatter::parse_fields(&[
            "empty: []",
            "gaps: [a,,\"\", b]",
            "half: [a",
            "url: https://x.y/z",
            "one: \"",
            "list:",
            "- ",
            "- \"\"",
            "-",
            "- 项",
            "orphan",
            "- stray",
            ": no key",
        ]);
        assert_eq!(
            fields,
            [
                Field { key: "empty".into(), value: Value::List(vec![]) },
                Field { key: "gaps".into(), value: Value::List(vec!["a".into(), "b".into()]) },
                Field { key: "half".into(), value: Value::Text("[a".into()) },
                Field { key: "url".into(), value: Value::Text("https://x.y/z".into()) },
                Field { key: "one".into(), value: Value::Text("\"".into()) },
                Field { key: "list".into(), value: Value::List(vec!["项".into()]) },
            ]
        );
    }
}
