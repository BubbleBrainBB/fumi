//! Reading Markdown structure out of lines. Nothing here rewrites text: the
//! results are classifications and byte ranges over the source.

use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Blank,
    Para,
    Heading(u8),
    List { indent: u8, marker: u8 },
    Quote,
    Rule,
    FenceOpen,
    FenceBody,
    FenceClose,
    MathOpen,
    MathBody,
    MathClose,
    MathOne,
    TableHead,
    TableDelim,
    TableRow,
    FrontOpen,
    FrontBody,
    FrontClose,
    Image,
}

/// What the lines after this one are inside of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scan {
    Normal,
    Fence { ch: u8, len: u8 },
    Math,
    Front,
    Table,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineInfo {
    pub kind: Kind,
    pub after: Scan,
}

pub fn is_table_delim(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('-') || !t.contains('|') {
        return false;
    }
    t.bytes().all(|b| matches!(b, b'|' | b'-' | b':' | b' ' | b'\t'))
}

fn fence_of(line: &str) -> Option<(u8, u8)> {
    let t = line.trim_start_matches(' ');
    if line.len() - t.len() > 3 {
        return None;
    }
    let ch = *t.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = t.bytes().take_while(|b| *b == ch).count();
    (len >= 3).then_some((ch, len.min(255) as u8))
}

/// Classifies one line given the scan state before it and the next line, the
/// only lookahead a table needs.
pub fn classify(line: &str, next: Option<&str>, before: Scan, index: usize) -> LineInfo {
    let t = line.trim();
    let info = |kind, after| LineInfo { kind, after };
    match before {
        Scan::Front => {
            if t == "---" || t == "..." {
                return info(Kind::FrontClose, Scan::Normal);
            }
            return info(Kind::FrontBody, Scan::Front);
        }
        Scan::Fence { ch, len } => {
            if let Some((c, l)) = fence_of(line)
                && c == ch
                && l >= len
                && t.bytes().all(|b| b == ch)
            {
                return info(Kind::FenceClose, Scan::Normal);
            }
            return info(Kind::FenceBody, before);
        }
        Scan::Math => {
            if t == "$$" {
                return info(Kind::MathClose, Scan::Normal);
            }
            return info(Kind::MathBody, Scan::Math);
        }
        Scan::Table => {
            if !t.is_empty() && t.contains('|') {
                return info(Kind::TableRow, Scan::Table);
            }
        }
        Scan::Normal => {}
    }

    if index == 0 && t == "---" {
        return info(Kind::FrontOpen, Scan::Front);
    }
    if t.is_empty() {
        return info(Kind::Blank, Scan::Normal);
    }
    if let Some((ch, len)) = fence_of(line) {
        return info(Kind::FenceOpen, Scan::Fence { ch, len });
    }
    if t == "$$" {
        return info(Kind::MathOpen, Scan::Math);
    }
    if t.len() > 4 && t.starts_with("$$") && t.ends_with("$$") {
        return info(Kind::MathOne, Scan::Normal);
    }
    if before != Scan::Table && t.contains('|') && next.is_some_and(is_table_delim) {
        // The delimiter line is told apart below by its own text.
        return info(Kind::TableHead, Scan::Table);
    }
    let bytes = line.as_bytes();
    let indent = bytes.iter().take_while(|b| **b == b' ').count();
    let rest = &line[indent..];
    if indent < 4 {
        let hashes = rest.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&hashes) && rest.as_bytes().get(hashes).is_none_or(|b| *b == b' ') {
            return info(Kind::Heading(hashes as u8), Scan::Normal);
        }
        if is_rule(t) {
            return info(Kind::Rule, Scan::Normal);
        }
        if rest.starts_with('>') {
            return info(Kind::Quote, Scan::Normal);
        }
    }
    if let Some(marker) = list_marker(rest) {
        return info(
            Kind::List {
                indent: indent.min(255) as u8,
                marker: marker.min(255) as u8,
            },
            Scan::Normal,
        );
    }
    if t.starts_with("![") && t.ends_with(')') && image_target(t).is_some() {
        return info(Kind::Image, Scan::Normal);
    }
    info(Kind::Para, Scan::Normal)
}

fn is_rule(t: &str) -> bool {
    let Some(ch) = t.bytes().next() else { return false };
    matches!(ch, b'-' | b'*' | b'_')
        && t.bytes().filter(|b| *b == ch).count() >= 3
        && t.bytes().all(|b| b == ch || b == b' ')
}

/// Length of a list marker with its following space, or `None`.
pub fn list_marker(rest: &str) -> Option<usize> {
    let b = rest.as_bytes();
    if b.len() >= 2 && matches!(b[0], b'-' | b'*' | b'+') && b[1] == b' ' {
        return Some(2);
    }
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits)
        && matches!(b.get(digits), Some(b'.') | Some(b')'))
        && b.get(digits + 1) == Some(&b' ')
    {
        return Some(digits + 2);
    }
    None
}

/// The path of a line that is only an image, `![alt](path)`.
pub fn image_target(line: &str) -> Option<&str> {
    let t = line.trim();
    let rest = t.strip_prefix("![")?;
    let close = rest.find("](")?;
    let target = rest[close + 2..].strip_suffix(')')?;
    (!target.contains(')') && !target.is_empty()).then_some(target)
}

/// Fixes up table delimiter lines, which `classify` reports as rows.
pub fn table_kind(line: &str, prev: Kind, kind: Kind) -> Kind {
    if kind == Kind::TableRow && prev == Kind::TableHead && is_table_delim(line) {
        Kind::TableDelim
    } else {
        kind
    }
}

// Inline ------------------------------------------------------------------

pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;
pub const CODE: u8 = 4;
pub const STRIKE: u8 = 8;
pub const LINK: u8 = 16;
pub const WIKI: u8 = 32;
pub const MARK: u8 = 64;

/// A stretch of source with one style. `marker` stretches are syntax, hidden
/// off the caret's line and dimmed on it. Segments cover the input in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub range: Range<usize>,
    pub style: u8,
    pub marker: bool,
}

pub fn parse_inline(src: &str) -> Vec<Seg> {
    let mut out = Vec::with_capacity(8);
    walk(src, 0..src.len(), 0, &mut out);
    out
}

fn push(out: &mut Vec<Seg>, range: Range<usize>, style: u8, marker: bool) {
    if !range.is_empty() {
        out.push(Seg { range, style, marker });
    }
}

fn find_close(src: &str, from: usize, end: usize, pat: &str) -> Option<usize> {
    let mut i = from;
    while i < end {
        let hit = src[i..end].find(pat)? + i;
        if hit == 0 || src.as_bytes()[hit - 1] != b'\\' {
            return Some(hit);
        }
        i = hit + pat.len();
    }
    None
}

fn walk(src: &str, range: Range<usize>, style: u8, out: &mut Vec<Seg>) {
    let b = src.as_bytes();
    let end = range.end;
    let mut text_start = range.start;
    let mut i = range.start;
    macro_rules! flush {
        () => {
            push(out, text_start..i, style, false);
        };
    }
    while i < end {
        match b[i] {
            b'\\' if i + 1 < end && b[i + 1].is_ascii_punctuation() => {
                flush!();
                push(out, i..i + 1, style, true);
                text_start = i + 1;
                i += 2;
            }
            b'`' => {
                let n = b[i..end].iter().take_while(|c| **c == b'`').count();
                let pat = &src[i..i + n];
                if let Some(close) = src[i + n..end].find(pat).map(|p| p + i + n) {
                    flush!();
                    push(out, i..i + n, style, true);
                    push(out, i + n..close, style | CODE, false);
                    push(out, close..close + n, style, true);
                    i = close + n;
                    text_start = i;
                } else {
                    i += n;
                }
            }
            b'[' if b.get(i + 1) == Some(&b'[') => {
                if let Some(close) = find_close(src, i + 2, end, "]]") {
                    flush!();
                    push(out, i..i + 2, style, true);
                    push(out, i + 2..close, style | WIKI, false);
                    push(out, close..close + 2, style, true);
                    i = close + 2;
                    text_start = i;
                } else {
                    i += 2;
                }
            }
            b'[' => {
                let parsed = find_close(src, i + 1, end, "](").and_then(|mid| {
                    find_close(src, mid + 2, end, ")").map(|close| (mid, close))
                });
                if let Some((mid, close)) = parsed {
                    flush!();
                    push(out, i..i + 1, style, true);
                    walk(src, i + 1..mid, style | LINK, out);
                    push(out, mid..close + 1, style, true);
                    i = close + 1;
                    text_start = i;
                } else {
                    i += 1;
                }
            }
            b'*' | b'_' | b'~' | b'=' => {
                let c = b[i];
                let n = b[i..end].iter().take_while(|x| **x == c).count();
                let (want, flag) = match (c, n) {
                    (b'*' | b'_', 2..) => (2, BOLD),
                    (b'*' | b'_', 1) => (1, ITALIC),
                    (b'~', 2..) => (2, STRIKE),
                    (b'=', 2..) => (2, MARK),
                    _ => (0, 0),
                };
                let opens = want > 0
                    && b.get(i + want).is_some_and(|x| !x.is_ascii_whitespace())
                    && (c != b'_' || i == 0 || !b[i - 1].is_ascii_alphanumeric());
                let closer = opens
                    .then(|| closer_of(src, i + want, end, c, want))
                    .flatten();
                if let Some(close) = closer {
                    flush!();
                    push(out, i..i + want, style, true);
                    walk(src, i + want..close, style | flag, out);
                    push(out, close..close + want, style, true);
                    i = close + want;
                    text_start = i;
                } else {
                    i += n;
                }
            }
            _ => i += 1,
        }
    }
    push(out, text_start..end, style, false);
}

fn closer_of(src: &str, from: usize, end: usize, c: u8, want: usize) -> Option<usize> {
    let b = src.as_bytes();
    let mut i = from;
    while i + want <= end {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b'`' {
            // Delimiters inside a code span don't close anything.
            match src[i + 1..end].find('`') {
                Some(p) => {
                    i += p + 2;
                    continue;
                }
                None => return None,
            }
        }
        if b[i] == c {
            let n = b[i..end].iter().take_while(|x| **x == c).count();
            if n >= want && i > from && !b[i - 1].is_ascii_whitespace() {
                if want == 1 && n >= 2 {
                    i += n;
                    continue;
                }
                return Some(i);
            }
            i += n;
            continue;
        }
        i += 1;
    }
    None
}

// Tables ------------------------------------------------------------------

/// Cell text ranges of one table line, without the outer pipes or padding.
pub fn table_cells(line: &str) -> Vec<Range<usize>> {
    let b = line.as_bytes();
    let mut cells = Vec::new();
    let mut start = 0;
    let mut in_code = false;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'`' => in_code = !in_code,
            b'|' if !in_code => {
                cells.push(start..i);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    cells.push(start..b.len());
    if line[cells[0].clone()].trim().is_empty() {
        cells.remove(0);
    }
    if cells.last().is_some_and(|c| line[c.clone()].trim().is_empty()) {
        cells.pop();
    }
    cells
        .into_iter()
        .map(|c| {
            let s = &line[c.clone()];
            let lead = s.len() - s.trim_start().len();
            c.start + lead..c.start + lead + s.trim().len()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cover(src: &str) -> Vec<Seg> {
        let segs = parse_inline(src);
        let mut at = 0;
        for s in &segs {
            assert_eq!(s.range.start, at, "gap in {src:?}: {segs:?}");
            at = s.range.end;
        }
        assert_eq!(at, src.len(), "{src:?}: {segs:?}");
        segs
    }

    #[test]
    fn segments_cover_the_source_exactly() {
        for src in [
            "",
            "plain 中文 text",
            "**粗体 bold** and *it* and `code` ~~x~~ ==m==",
            "[link](https://a.b/c) [[Some Note]] ![i](p.png)",
            "unclosed **bold and `tick and [bracket and [[wiki",
            "\\*escaped\\* snake_case_word 2*3*4 a ** b ** c",
            "***both*** __under__ _i_ ``a`b`` ** ",
            "混排**粗体**混排`代码`混排",
        ] {
            cover(src);
        }
    }

    #[test]
    fn styles_and_markers() {
        let src = "a **b** `c`";
        let segs = cover(src);
        let bold = segs.iter().find(|s| s.style & BOLD != 0).unwrap();
        assert_eq!(&src[bold.range.clone()], "b");
        let code = segs.iter().find(|s| s.style & CODE != 0).unwrap();
        assert_eq!(&src[code.range.clone()], "c");
        assert_eq!(segs.iter().filter(|s| s.marker).count(), 4);
    }

    #[test]
    fn no_emphasis_inside_words_or_with_spaces() {
        assert!(cover("snake_case_word").iter().all(|s| !s.marker));
        assert!(cover("a * b * c").iter().all(|s| !s.marker));
    }

    #[test]
    fn classifies_blocks() {
        let c = |l, n| classify(l, n, Scan::Normal, 5).kind;
        assert_eq!(c("# 标题", None), Kind::Heading(1));
        assert_eq!(c("#nope", None), Kind::Para);
        assert_eq!(c("- item", None), Kind::List { indent: 0, marker: 2 });
        assert_eq!(c("12. item", None), Kind::List { indent: 0, marker: 4 });
        assert_eq!(c("> q", None), Kind::Quote);
        assert_eq!(c("---", None), Kind::Rule);
        assert_eq!(c("| a | b |", Some("| --- | --- |")), Kind::TableHead);
        assert_eq!(c("| a | b |", Some("text")), Kind::Para);
        assert_eq!(c("![a](p.png)", None), Kind::Image);
        assert_eq!(c("$$x$$", None), Kind::MathOne);
        assert_eq!(classify("---", None, Scan::Normal, 0).kind, Kind::FrontOpen);
    }

    #[test]
    fn table_cells_trim_and_keep_code_pipes() {
        let line = "| a | `b|c` | 中 |";
        let cells: Vec<&str> = table_cells(line).into_iter().map(|c| &line[c]).collect();
        assert_eq!(cells, ["a", "`b|c`", "中"]);
    }
}
