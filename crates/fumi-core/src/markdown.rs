//! Reading Markdown structure out of lines. Nothing here rewrites text: the
//! results are classifications and byte ranges over the source.

use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Blank,
    Para,
    Heading(u8),
    List { indent: u8, marker: u8 },
    /// A plain `>` line.
    Quote,
    /// First line of a `> [!type]` callout, its title.
    QuoteCalloutTitle,
    /// A later line of a `> [!type]` callout.
    QuoteCalloutBody,
    /// A line of a quote whose first line opens with an emoji: a gray callout.
    EmojiQuote,
    /// `::: type title`.
    CalloutOpen,
    CalloutBody,
    /// The closing `:::`.
    CalloutClose,
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
    Callout,
    /// Inside a run of `>` lines; says what the run's first line made of it.
    QuoteRun { callout: bool, emoji: bool },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineInfo {
    pub kind: Kind,
    pub after: Scan,
    /// For a fence, `$$` or `:::` line: it pairs into a closed block. An
    /// unclosed one is an ordinary line, so a half-typed fence can't turn the
    /// rest of the note into code. Decided by `pair_blocks` over the whole text.
    pub real: bool,
}

/// `^\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?$`
pub fn is_table_delim(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('-') {
        return false;
    }
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').all(|cell| {
        let c = cell.trim();
        let c = c.strip_prefix(':').unwrap_or(c);
        let c = c.strip_suffix(':').unwrap_or(c);
        !c.is_empty() && c.bytes().all(|b| b == b'-')
    })
}

/// A fence-looking line: an optionally indented run of three or more backticks
/// or tildes. Returns the character, the run's length, and whether text follows.
pub fn fence_of(line: &str) -> Option<(u8, u8, bool)> {
    let t = line.trim_start_matches([' ', '\t']);
    let ch = *t.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = t.bytes().take_while(|b| *b == ch).count();
    (len >= 3).then(|| (ch, len.min(255) as u8, !t[len..].trim().is_empty()))
}

/// A quote line's text after its `>` markers.
pub fn quote_content(line: &str) -> &str {
    line.trim_start_matches([' ', '\t', '>'])
}

/// The type of a quote line that opens a callout: `note` for `[!note]`,
/// `[!note]-` or `[!note] Title`; `None` for any other quote.
pub fn callout_type(content: &str) -> Option<&str> {
    let rest = content.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let name = &rest[..close];
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let after = &rest[close + 1..];
    let after = after.strip_prefix(['+', '-']).unwrap_or(after);
    (after.is_empty() || after.starts_with(char::is_whitespace)).then_some(name)
}

/// True for a pictographic first character (⚠️, ✅, 💡 …), not for digits or
/// `#`/`*`, which can be emoji but read as text.
pub fn starts_with_emoji(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return false };
    let c = first as u32;
    let wide = matches!(c, 0x1F300..=0x1FAFF | 0x1F000..=0x1F2FF);
    let needs_selector = matches!(c, 0x2190..=0x2BFF | 0x3030 | 0x303D | 0x3297 | 0x3299);
    wide || (needs_selector && (chars.next() == Some('\u{FE0F}') || is_emoji_presentation(c)))
}

fn is_emoji_presentation(c: u32) -> bool {
    matches!(
        c,
        0x231A | 0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3 | 0x25FD | 0x25FE | 0x2614 | 0x2615
            | 0x2648..=0x2653 | 0x267F | 0x2693 | 0x26A1 | 0x26AA | 0x26AB | 0x26BD | 0x26BE
            | 0x26C4 | 0x26C5 | 0x26CE | 0x26D4 | 0x26EA | 0x26F2 | 0x26F3 | 0x26F5 | 0x26FA
            | 0x26FD | 0x2705 | 0x270A | 0x270B | 0x2728 | 0x274C | 0x274E | 0x2753..=0x2755
            | 0x2757 | 0x2795..=0x2797 | 0x27B0 | 0x27BF | 0x2B1B | 0x2B1C | 0x2B50 | 0x2B55
    )
}

/// Which `$$`, `:::` and fence lines pair into closed blocks. Fences pair the
/// way CommonMark does: the closer has the same character, is at least as long,
/// and has no info string; an opener with no closer is left unpaired and the
/// search resumes on the next line. This is the whole-text half of the scan.
pub fn pair_blocks<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<bool> {
    let lines: Vec<&str> = lines.collect();
    let mut real = vec![false; lines.len()];
    let fences: Vec<(usize, (u8, u8, bool))> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| fence_of(l).map(|f| (i, f)))
        .collect();
    let mut in_code = vec![false; lines.len()];
    let mut i = 0;
    while i < fences.len() {
        let (open_at, open) = fences[i];
        match fences[i + 1..]
            .iter()
            .position(|(_, f)| f.0 == open.0 && f.1 >= open.1 && !f.2)
        {
            Some(k) => {
                let close_at = fences[i + 1 + k].0;
                real[open_at] = true;
                real[close_at] = true;
                in_code[open_at..=close_at].fill(true);
                i += k + 2;
            }
            None => i += 1,
        }
    }
    // Math and callouts, outside code.
    let mut open_math = None;
    let mut open_callout = None;
    for (i, line) in lines.iter().enumerate() {
        if in_code[i] {
            continue;
        }
        let t = line.trim();
        if t == "$$" {
            match open_math.take() {
                Some(a) => {
                    real[a] = true;
                    real[i] = true;
                }
                None => open_math = Some(i),
            }
        } else if open_math.is_none() && t.starts_with(":::") {
            match open_callout.take() {
                Some(a) => {
                    real[a] = true;
                    real[i] = true;
                }
                None => open_callout = Some(i),
            }
        }
    }
    real
}

/// Classifies one line given the scan state before it, the next line (the only
/// lookahead a table needs), and whether `pair_blocks` called this line real.
pub fn classify(line: &str, next: Option<&str>, before: Scan, _index: usize, real: bool) -> LineInfo {
    let t = line.trim();
    let info = |kind, after| LineInfo { kind, after, real };
    match before {
        Scan::Front => {
            if t == "---" || t == "..." {
                return info(Kind::FrontClose, Scan::Normal);
            }
            return info(Kind::FrontBody, Scan::Front);
        }
        Scan::Fence { .. } => {
            if real && fence_of(line).is_some() {
                return info(Kind::FenceClose, Scan::Normal);
            }
            return info(Kind::FenceBody, before);
        }
        Scan::Math => {
            if t == "$$" && real {
                return info(Kind::MathClose, Scan::Normal);
            }
            return info(Kind::MathBody, Scan::Math);
        }
        Scan::Callout => {
            if real && fence_of(line).is_some() {
                let (ch, len, _) = fence_of(line).unwrap();
                return info(Kind::FenceOpen, Scan::Fence { ch, len });
            }
            if t.starts_with(":::") {
                return info(Kind::CalloutClose, Scan::Normal);
            }
            return info(Kind::CalloutBody, Scan::Callout);
        }
        Scan::Table => {
            if !t.is_empty() && t.contains('|') && fence_of(line).is_none() {
                return info(Kind::TableRow, Scan::Table);
            }
        }
        Scan::QuoteRun { callout, emoji } => {
            if t.starts_with('>') {
                let kind = match (callout, emoji) {
                    (true, _) => Kind::QuoteCalloutBody,
                    (_, true) => Kind::EmojiQuote,
                    _ => Kind::Quote,
                };
                return info(kind, before);
            }
        }
        Scan::Normal => {}
    }

    if real {
        if let Some((ch, len, _)) = fence_of(line) {
            return info(Kind::FenceOpen, Scan::Fence { ch, len });
        }
        if t == "$$" {
            return info(Kind::MathOpen, Scan::Math);
        }
    }
    if t.starts_with(":::") {
        return info(Kind::CalloutOpen, Scan::Callout);
    }
    if t.is_empty() {
        return info(Kind::Blank, Scan::Normal);
    }
    if t.len() > 4 && t.starts_with("$$") && t.ends_with("$$") && !t[2..t.len() - 2].contains("$$") {
        return info(Kind::MathOne, Scan::Normal);
    }
    if t.contains('|') && fence_of(line).is_none() && next.is_some_and(is_table_delim) {
        // The delimiter line is told apart below by its own text.
        return info(Kind::TableHead, Scan::Table);
    }
    let bytes = line.as_bytes();
    let indent = bytes.iter().take_while(|b| matches!(**b, b' ' | b'\t')).count();
    let rest = &line[indent..];
    let spaces = bytes.iter().take_while(|b| **b == b' ').count();
    if spaces <= 3 {
        let body = &line[spaces..];
        let hashes = body.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&hashes)
            && body.as_bytes().get(hashes).is_none_or(|b| matches!(*b, b' ' | b'\t'))
        {
            return info(Kind::Heading(hashes as u8), Scan::Normal);
        }
    }
    if t.starts_with('>') {
        let first = quote_content(line);
        let callout = callout_type(first).is_some();
        let emoji = starts_with_emoji(first);
        let kind = match (callout, emoji) {
            (true, _) => Kind::QuoteCalloutTitle,
            (_, true) => Kind::EmojiQuote,
            _ => Kind::Quote,
        };
        return info(kind, Scan::QuoteRun { callout, emoji });
    }
    if is_rule(t) {
        return info(Kind::Rule, Scan::Normal);
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
    if is_lone_image(t) {
        return info(Kind::Image, Scan::Normal);
    }
    info(Kind::Para, Scan::Normal)
}

/// `---`, `***` or `___`, spaces allowed.
fn is_rule(t: &str) -> bool {
    let compact: String = t.chars().filter(|c| *c != ' ').collect();
    compact == "---" || compact == "***" || compact == "___"
}

/// Length of a list marker with the spaces after it, and a `[ ]` box if any is
/// not included. `^([-*+]|\d+[.)])[ \t]+`
pub fn list_marker(rest: &str) -> Option<usize> {
    let b = rest.as_bytes();
    let token = if b.first().is_some_and(|c| matches!(c, b'-' | b'*' | b'+')) {
        1
    } else {
        let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 || !matches!(b.get(digits), Some(b'.') | Some(b')')) {
            return None;
        }
        digits + 1
    };
    let gap = b[token..].iter().take_while(|c| matches!(**c, b' ' | b'\t')).count();
    (gap > 0).then_some(token + gap)
}

/// A line holding nothing but one image: `![alt](path)` or `![[embed]]`.
pub fn is_lone_image(t: &str) -> bool {
    if let Some(inner) = t.strip_prefix("![[").and_then(|r| r.strip_suffix("]]")) {
        return !inner.is_empty() && !inner.contains("]]") && !inner.contains('\n');
    }
    t.starts_with("![") && t.ends_with(')') && image_target(t).is_some()
}

/// The path of a line that is only an image, `![alt](path)`.
pub fn image_target(line: &str) -> Option<&str> {
    let t = line.trim();
    if let Some(inner) = t.strip_prefix("![[").and_then(|r| r.strip_suffix("]]")) {
        return Some(inner);
    }
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

// Whole-text passes ---------------------------------------------------------

pub fn lf_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l))
}

/// Classifies every line. Linear in the text.
pub fn scan_all(text: &str) -> Vec<LineInfo> {
    let real = pair_blocks(lf_lines(text));
    let front_close = front_close_line(text);
    let mut out = Vec::with_capacity(real.len());
    let mut before = Scan::Normal;
    let mut prev = Kind::Blank;
    let mut rest = lf_lines(text).peekable();
    let mut i = 0;
    while let Some(line) = rest.next() {
        let next = rest.peek().copied();
        let mut info = match front_close {
            Some(close) if i <= close => LineInfo {
                kind: if i == 0 {
                    Kind::FrontOpen
                } else if i == close {
                    Kind::FrontClose
                } else {
                    Kind::FrontBody
                },
                after: Scan::Normal,
                real: true,
            },
            _ => classify(line, next, before, i, real[i]),
        };
        info.kind = table_kind(line, prev, info.kind);
        out.push(info);
        before = info.after;
        prev = info.kind;
        i += 1;
    }
    out
}

/// Whether editing a line with this text can change block pairing elsewhere in
/// the note, which only a whole-text pass can tell.
pub fn pairing_sensitive(line: &str) -> bool {
    let t = line.trim();
    fence_of(line).is_some() || t.starts_with("$$") || t.ends_with("$$") || t.starts_with(":::") || t == "---" || t == "..."
}

/// The line that closes frontmatter opening on the first line.
pub fn front_close_line(text: &str) -> Option<usize> {
    let front = crate::frontmatter::Frontmatter::parse(text)?;
    Some(text[..front.range.end].bytes().filter(|b| *b == b'\n').count())
}

/// The note's title: the first `#` heading before any other content.
pub fn find_title(lines: &[LineInfo]) -> Option<usize> {
    for (i, l) in lines.iter().enumerate() {
        match l.kind {
            Kind::Blank | Kind::FrontOpen | Kind::FrontBody | Kind::FrontClose => continue,
            Kind::Heading(1) => return Some(i),
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<Kind> {
        scan_all(text).into_iter().map(|l| l.kind).collect()
    }

    #[test]
    fn classifies_blocks() {
        let c = |l, n| classify(l, n, Scan::Normal, 5, false).kind;
        assert_eq!(c("# 标题", None), Kind::Heading(1));
        assert_eq!(c("#nope", None), Kind::Para);
        assert_eq!(c("- item", None), Kind::List { indent: 0, marker: 2 });
        assert_eq!(c("12. item", None), Kind::List { indent: 0, marker: 4 });
        assert_eq!(c("> q", None), Kind::Quote);
        assert_eq!(c("---", None), Kind::Rule);
        assert_eq!(c("* * *", None), Kind::Rule);
        assert_eq!(c("----", None), Kind::Para);
        assert_eq!(c("| a | b |", Some("| --- | --- |")), Kind::TableHead);
        assert_eq!(c("| a | b |", Some("text")), Kind::Para);
        assert_eq!(c("![a](p.png)", None), Kind::Image);
        assert_eq!(c("![[p.png|300]]", None), Kind::Image);
        assert_eq!(c("$$x$$", None), Kind::MathOne);
        assert_eq!(c("> [!note] T", None), Kind::QuoteCalloutTitle);
        assert_eq!(c("> ⚠️ careful", None), Kind::EmojiQuote);
        assert_eq!(c("> 1 not emoji", None), Kind::Quote);
    }

    #[test]
    fn an_unclosed_fence_is_an_ordinary_line() {
        assert_eq!(kinds("```\ncode"), [Kind::Para, Kind::Para]);
        assert_eq!(
            kinds("```\ncode\n```\nafter"),
            [Kind::FenceOpen, Kind::FenceBody, Kind::FenceClose, Kind::Para]
        );
    }

    #[test]
    fn a_closer_matches_character_and_length_and_has_no_info() {
        assert_eq!(
            kinds("````\n```\n~~~~\n````"),
            [Kind::FenceOpen, Kind::FenceBody, Kind::FenceBody, Kind::FenceClose]
        );
        // An info string can't close; with no other closer the opener is plain.
        assert_eq!(kinds("```\n```swift"), [Kind::Para, Kind::Para]);
    }

    #[test]
    fn unclosed_math_and_frontmatter_are_ordinary() {
        assert_eq!(kinds("$$\nx"), [Kind::Para, Kind::Para]);
        assert_eq!(kinds("$$\nx\n$$"), [Kind::MathOpen, Kind::MathBody, Kind::MathClose]);
        assert_eq!(kinds("---\ntitle: x")[0], Kind::Rule);
        assert_eq!(
            kinds("---\ntitle: x\n---\nbody"),
            [Kind::FrontOpen, Kind::FrontBody, Kind::FrontClose, Kind::Para]
        );
    }

    #[test]
    fn nothing_inside_code_is_a_block() {
        assert_eq!(
            kinds("```\n| a |\n| - |\n$$\n:::\n```"),
            [Kind::FenceOpen, Kind::FenceBody, Kind::FenceBody, Kind::FenceBody, Kind::FenceBody, Kind::FenceClose]
        );
    }

    #[test]
    fn callouts_and_quote_groups() {
        assert_eq!(
            kinds("::: warning 标题\nbody\n:::\nafter"),
            [Kind::CalloutOpen, Kind::CalloutBody, Kind::CalloutClose, Kind::Para]
        );
        assert_eq!(
            kinds("> [!tip] T\n> body\n\n> plain\n> more\n\n> 💡 x\n> y"),
            [
                Kind::QuoteCalloutTitle,
                Kind::QuoteCalloutBody,
                Kind::Blank,
                Kind::Quote,
                Kind::Quote,
                Kind::Blank,
                Kind::EmojiQuote,
                Kind::EmojiQuote
            ]
        );
    }

    #[test]
    fn crlf_tables_classify() {
        let k = kinds("| a |\r\n| - |\r\n| 1 |\r\n");
        assert_eq!(k[0], Kind::TableHead);
        assert_eq!(k[1], Kind::TableDelim);
        assert_eq!(k[2], Kind::TableRow);
    }

    #[test]
    fn table_cells_trim_and_keep_code_pipes() {
        let line = "| a | `b|c` | 中 |";
        let cells: Vec<&str> = table_cells(line).into_iter().map(|c| &line[c]).collect();
        assert_eq!(cells, ["a", "`b|c`", "中"]);
    }
}
