//! A memo's inline Markdown, read the way Foundation's
//! `AttributedString(markdown:, interpretedSyntax: .inlineOnlyPreservingWhitespace)`
//! reads it (cmark-gfm with autolinks and strikethrough): the Swift app's
//! `MemoRow.attributed` hands it each stretch of text between `#tags`.
//!
//! The tests' expectations were printed by that Swift API.

/// A stretch of text in one style. Syntax is gone from `text`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InlineRun {
    pub text: String,
    pub strong: bool,
    pub emphasis: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}

/// The runs of `source`, in order; neighbors in the same style are merged.
pub fn parse(source: &str) -> Vec<InlineRun> {
    let chars: Vec<char> = source.chars().collect();
    let nodes = emphasize(tokenize(&chars));
    let mut runs = Vec::new();
    flatten(&nodes, Flags::default(), &mut runs);
    runs
}

/// `source` with its syntax removed.
pub fn plain(source: &str) -> String {
    parse(source).into_iter().map(|run| run.text).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Flags {
    strong: bool,
    emphasis: bool,
    strike: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wrap {
    Strong,
    Emphasis,
    Strike,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Delim {
    ch: char,
    count: usize,
    original: usize,
    can_open: bool,
    can_close: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Text(String),
    Code(String),
    Link { text: String, url: String },
    Wrapped { kind: Wrap, children: Vec<Node> },
    Delim(Delim),
}

fn flatten(nodes: &[Node], flags: Flags, runs: &mut Vec<InlineRun>) {
    for node in nodes {
        match node {
            Node::Text(text) => push(runs, text, flags, false, None),
            Node::Code(text) => push(runs, text, flags, true, None),
            Node::Link { text, url } => push(runs, text, flags, false, Some(url)),
            Node::Wrapped { kind, children } => {
                let mut inner = flags;
                match kind {
                    Wrap::Strong => inner.strong = true,
                    Wrap::Emphasis => inner.emphasis = true,
                    Wrap::Strike => inner.strike = true,
                }
                flatten(children, inner, runs);
            }
            Node::Delim(delim) => {
                let text: String = std::iter::repeat_n(delim.ch, delim.count).collect();
                push(runs, &text, flags, false, None);
            }
        }
    }
}

fn push(runs: &mut Vec<InlineRun>, text: &str, flags: Flags, code: bool, link: Option<&String>) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut()
        && last.strong == flags.strong
        && last.emphasis == flags.emphasis
        && last.strike == flags.strike
        && last.code == code
        && last.link.as_ref() == link
    {
        last.text.push_str(text);
        return;
    }
    runs.push(InlineRun {
        text: text.to_string(),
        strong: flags.strong,
        emphasis: flags.emphasis,
        code,
        strike: flags.strike,
        link: link.cloned(),
    });
}

// Characters.

fn is_space(c: char) -> bool {
    c.is_whitespace()
}

/// CommonMark's punctuation: ASCII, and the Unicode P and S blocks people meet.
fn is_punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(c as u32,
            0xA1 | 0xA7 | 0xAB | 0xB6 | 0xB7 | 0xBB | 0xBF
            | 0x2010..=0x2027 | 0x2030..=0x205E | 0x20A0..=0x20C0 | 0x2190..=0x2BFF
            | 0x3001..=0x3003 | 0x3008..=0x3011 | 0x3014..=0x301F | 0x30FB
            | 0xFE10..=0xFE19 | 0xFE30..=0xFE52 | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20
            | 0xFF3B..=0xFF40 | 0xFF5B..=0xFF65)
}

// First pass: code, links, escapes, and delimiter runs left for the second.

fn tokenize(chars: &[char]) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    macro_rules! flush {
        () => {
            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }
        };
    }
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                match chars.get(i + 1) {
                    Some(next) if next.is_ascii_punctuation() => {
                        text.push(*next);
                        i += 2;
                    }
                    // A hard break: the backslash goes, the line break stays.
                    Some('\n') => {
                        text.push('\n');
                        i += 2;
                    }
                    _ => {
                        text.push('\\');
                        i += 1;
                    }
                }
                continue;
            }
            '`' => {
                let open = run_length(chars, i);
                if let Some((content, end)) = code_span(chars, i, open) {
                    flush!();
                    nodes.push(Node::Code(content));
                    i = end;
                } else {
                    text.extend(std::iter::repeat_n('`', open));
                    i += open;
                }
                continue;
            }
            '*' | '_' | '~' => {
                let count = run_length(chars, i);
                let delim = delimiter(chars, i, count);
                flush!();
                nodes.push(Node::Delim(delim));
                i += count;
                continue;
            }
            '&' => {
                if let Some((decoded, end)) = entity(chars, i) {
                    text.push(decoded);
                    i = end;
                } else {
                    text.push('&');
                    i += 1;
                }
                continue;
            }
            '<' => {
                if let Some((label, url, end)) = angle_autolink(chars, i) {
                    flush!();
                    nodes.push(Node::Link { text: label, url });
                    i = end;
                    continue;
                }
            }
            '!' if chars.get(i + 1) == Some(&'[') => {
                if let Some((label, _, end)) = bracket_link(chars, i + 1) {
                    text.push_str(&label);
                    i = end;
                    continue;
                }
            }
            '[' => {
                if let Some((label, url, end)) = bracket_link(chars, i) {
                    flush!();
                    nodes.push(Node::Link { text: label, url });
                    i = end;
                    continue;
                }
            }
            _ => {
                if starts_word(chars, i)
                    && let Some((label, url, end)) = bare_autolink(chars, i)
                {
                    flush!();
                    nodes.push(Node::Link { text: label, url });
                    i = end;
                    continue;
                }
            }
        }
        text.push(c);
        i += 1;
    }
    flush!();
    nodes
}

fn run_length(chars: &[char], at: usize) -> usize {
    chars[at..].iter().take_while(|c| **c == chars[at]).count()
}

fn code_span(chars: &[char], at: usize, open: usize) -> Option<(String, usize)> {
    let mut i = at + open;
    while i < chars.len() {
        if chars[i] == '`' {
            let close = run_length(chars, i);
            if close == open {
                let inner: String = chars[at + open..i]
                    .iter()
                    .map(|c| if *c == '\n' { ' ' } else { *c })
                    .collect();
                let strip = inner.len() >= 2
                    && inner.starts_with(' ')
                    && inner.ends_with(' ')
                    && inner.chars().any(|c| c != ' ');
                let inner = if strip { inner[1..inner.len() - 1].to_string() } else { inner };
                return Some((inner, i + close));
            }
            i += close;
        } else {
            i += 1;
        }
    }
    None
}

fn delimiter(chars: &[char], at: usize, count: usize) -> Delim {
    let ch = chars[at];
    let before = if at == 0 { '\n' } else { chars[at - 1] };
    let after = chars.get(at + count).copied().unwrap_or('\n');
    let (before_space, after_space) = (is_space(before), is_space(after));
    let (before_punct, after_punct) = (is_punctuation(before), is_punctuation(after));
    let left = !after_space && (!after_punct || before_space || before_punct);
    let right = !before_space && (!before_punct || after_space || after_punct);
    let (can_open, can_close) = match ch {
        '_' => (left && (!right || before_punct), right && (!left || after_punct)),
        _ => (left, right),
    };
    Delim { ch, count, original: count, can_open, can_close }
}

fn entity(chars: &[char], at: usize) -> Option<(char, usize)> {
    let end = at + 1 + chars[at + 1..].iter().take(12).position(|c| *c == ';')?;
    let name: String = chars[at + 1..end].iter().collect();
    let decoded = if let Some(number) = name.strip_prefix('#') {
        let value = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        char::from_u32(value).filter(|c| *c != '\0').unwrap_or('\u{FFFD}')
    } else {
        match name.as_str() {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            "nbsp" => '\u{A0}',
            "copy" => '©',
            "reg" => '®',
            "mdash" => '—',
            "ndash" => '–',
            "hellip" => '…',
            _ => return None,
        }
    };
    Some((decoded, end + 1))
}

/// `<https://a.com>` or `<a@b.com>`.
fn angle_autolink(chars: &[char], at: usize) -> Option<(String, String, usize)> {
    let close = at + 1 + chars[at + 1..].iter().position(|c| *c == '>')?;
    let inner: String = chars[at + 1..close].iter().collect();
    if inner.is_empty() || inner.chars().any(|c| is_space(c) || c == '<') {
        return None;
    }
    if let Some(colon) = inner.find(':') {
        let scheme = &inner[..colon];
        let valid = (2..=32).contains(&scheme.len())
            && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
        if valid {
            return Some((inner.clone(), inner, close + 1));
        }
    }
    let chars_inner: Vec<char> = inner.chars().collect();
    let (end, _) = email_at(&chars_inner, 0)?;
    (end == chars_inner.len()).then(|| (inner.clone(), format!("mailto:{inner}"), close + 1))
}

/// `[label](destination "title")`, from the `[` at `at`.
fn bracket_link(chars: &[char], at: usize) -> Option<(String, String, usize)> {
    let mut depth = 0usize;
    let mut i = at;
    let close = loop {
        match chars.get(i)? {
            '\\' => i += 1,
            '`' => {
                let open = run_length(chars, i);
                if let Some((_, end)) = code_span(chars, i, open) {
                    i = end;
                    continue;
                }
                i += open;
                continue;
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break i;
                }
            }
            _ => {}
        }
        i += 1;
    };
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let (url, end) = link_target(chars, close + 2)?;
    let label: String = {
        let inner = emphasize(tokenize(&chars[at + 1..close]));
        let mut runs = Vec::new();
        flatten(&inner, Flags::default(), &mut runs);
        runs.into_iter().map(|run| run.text).collect()
    };
    Some((label, url, end))
}

fn link_target(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut i = start;
    let skip = |i: &mut usize| {
        while chars.get(*i).is_some_and(|c| is_space(*c)) {
            *i += 1;
        }
    };
    skip(&mut i);
    let mut url = String::new();
    if chars.get(i) == Some(&'<') {
        i += 1;
        loop {
            match chars.get(i)? {
                '>' => break,
                '\n' | '<' => return None,
                '\\' if chars.get(i + 1).is_some_and(char::is_ascii_punctuation) => {
                    url.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                c => url.push(*c),
            }
            i += 1;
        }
        i += 1;
    } else {
        let mut depth = 0usize;
        while let Some(c) = chars.get(i) {
            match c {
                c if is_space(*c) => break,
                '\\' if chars.get(i + 1).is_some_and(char::is_ascii_punctuation) => {
                    url.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                '(' => depth += 1,
                ')' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
            url.push(*c);
            i += 1;
        }
    }
    skip(&mut i);
    if let Some(open @ ('"' | '\'' | '(')) = chars.get(i).copied() {
        let close = if open == '(' { ')' } else { open };
        i += 1;
        while *chars.get(i)? != close {
            if chars[i] == '\\' {
                i += 1;
            }
            i += 1;
        }
        i += 1;
        skip(&mut i);
    }
    (chars.get(i) == Some(&')')).then(|| (url.replace(' ', "%20"), i + 1))
}

// GFM's bare autolinks.

fn starts_word(chars: &[char], at: usize) -> bool {
    at == 0 || {
        let before = chars[at - 1];
        is_space(before) || matches!(before, '*' | '_' | '~' | '(')
    }
}

fn has_prefix(chars: &[char], at: usize, prefix: &str) -> bool {
    let mut i = at;
    for p in prefix.chars() {
        if !chars.get(i).is_some_and(|c| c.eq_ignore_ascii_case(&p)) {
            return false;
        }
        i += 1;
    }
    true
}

fn bare_autolink(chars: &[char], at: usize) -> Option<(String, String, usize)> {
    let c = chars[at];
    if c.is_ascii_alphanumeric() {
        for scheme in ["https://", "http://", "ftp://"] {
            if has_prefix(chars, at, scheme) {
                let start = at + scheme.len();
                let end = trimmed_url_end(chars, at, start)?;
                let label: String = chars[at..end].iter().collect();
                return Some((label.clone(), label, end));
            }
        }
        if has_prefix(chars, at, "www.") {
            let end = trimmed_url_end(chars, at, at + 4)?;
            let label: String = chars[at..end].iter().collect();
            if !chars[at + 4..end].iter().any(|c| c.is_ascii_alphanumeric()) {
                return None;
            }
            return Some((label.clone(), format!("http://{label}"), end));
        }
    }
    if (c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
        && let Some((end, _)) = email_at(chars, at)
    {
        let label: String = chars[at..end].iter().collect();
        return Some((label.clone(), format!("mailto:{label}"), end));
    }
    None
}

/// Where a URL whose host starts at `host` ends, trailing punctuation left out.
fn trimmed_url_end(chars: &[char], at: usize, host: usize) -> Option<usize> {
    let mut end = host;
    while chars.get(end).is_some_and(|c| !is_space(*c) && *c != '<') {
        end += 1;
    }
    let has_host = chars.get(host).is_some_and(|c| c.is_alphanumeric());
    if !has_host {
        return None;
    }
    loop {
        match chars[end - 1] {
            '?' | '!' | '.' | ',' | ':' | '*' | '_' | '~' | '\'' | '"' => end -= 1,
            ')' => {
                let slice = &chars[at..end];
                let opens = slice.iter().filter(|c| **c == '(').count();
                let closes = slice.iter().filter(|c| **c == ')').count();
                if closes > opens {
                    end -= 1;
                } else {
                    break;
                }
            }
            ';' => {
                // An entity at the end isn't part of the link.
                let mut i = end - 1;
                while i > host && chars[i - 1].is_ascii_alphanumeric() {
                    i -= 1;
                }
                if i > host && chars[i - 1] == '&' && i < end - 1 {
                    end = i - 1;
                } else {
                    break;
                }
            }
            _ => break,
        }
        if end <= host {
            return None;
        }
    }
    Some(end)
}

/// `name@host.tld` starting at `at`: where it ends, and where the `@` is.
fn email_at(chars: &[char], at: usize) -> Option<(usize, usize)> {
    let mut i = at;
    while chars.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+')) {
        i += 1;
    }
    if i == at || chars.get(i) != Some(&'@') {
        return None;
    }
    let sign = i;
    i += 1;
    let host = i;
    let mut dots = 0;
    while let Some(c) = chars.get(i) {
        if *c == '.' && chars.get(i + 1).is_some_and(|n| n.is_ascii_alphanumeric()) {
            dots += 1;
        } else if !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) {
            break;
        }
        i += 1;
    }
    if i == host || dots == 0 || matches!(chars[i - 1], '-' | '_') {
        return None;
    }
    Some((i, sign))
}

// Second pass: pairs up the delimiter runs, CommonMark's way.

fn emphasize(mut nodes: Vec<Node>) -> Vec<Node> {
    let mut i = 0;
    while i < nodes.len() {
        let Node::Delim(closer) = &nodes[i] else {
            i += 1;
            continue;
        };
        if !closer.can_close {
            i += 1;
            continue;
        }
        let closer = closer.clone();
        let opener_at = (0..i).rev().find(|j| match &nodes[*j] {
            Node::Delim(open) => pairs(open, &closer),
            _ => false,
        });
        let Some(j) = opener_at else {
            i += 1;
            continue;
        };
        let Node::Delim(opener) = nodes[j].clone() else { unreachable!() };
        let (kind, used) = match closer.ch {
            '~' => (Wrap::Strike, closer.count),
            _ if opener.count >= 2 && closer.count >= 2 => (Wrap::Strong, 2),
            _ => (Wrap::Emphasis, 1),
        };
        let children: Vec<Node> = nodes.drain(j + 1..i).map(unpaired_to_text).collect();
        let wrapped = Node::Wrapped { kind, children };
        // Now: opener at j, closer at j + 1.
        nodes.insert(j + 1, wrapped);
        let closer_at = j + 2;
        let mut closer_left = closer;
        closer_left.count -= used;
        let mut opener_left = opener;
        opener_left.count -= used;
        if closer_left.count == 0 {
            nodes.remove(closer_at);
        } else {
            nodes[closer_at] = Node::Delim(closer_left);
        }
        if opener_left.count == 0 {
            nodes.remove(j);
            i = j + 1;
        } else {
            nodes[j] = Node::Delim(opener_left);
            i = j + 2;
        }
    }
    nodes.into_iter().map(unpaired_to_text).collect()
}

fn pairs(open: &Delim, close: &Delim) -> bool {
    if open.ch != close.ch || !open.can_open || open.count == 0 {
        return false;
    }
    if open.ch == '~' {
        return open.count == close.count && open.count <= 2;
    }
    // The rule of three.
    let odd = (open.can_close || close.can_open)
        && (open.original + close.original).is_multiple_of(3)
        && !(open.original.is_multiple_of(3) && close.original.is_multiple_of(3));
    !odd
}

fn unpaired_to_text(node: Node) -> Node {
    match node {
        Node::Delim(delim) => Node::Text(std::iter::repeat_n(delim.ch, delim.count).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints runs the way the Swift probe did: `"text":flags`.
    fn show(source: &str) -> String {
        parse(source)
            .iter()
            .map(|run| {
                let mut flags = String::new();
                if run.strong {
                    flags.push('B');
                }
                if run.emphasis {
                    flags.push('I');
                }
                if run.code {
                    flags.push('C');
                }
                if run.strike {
                    flags.push('S');
                }
                if let Some(link) = &run.link {
                    flags.push_str(&format!("L({link})"));
                }
                format!("{:?}:{flags}", run.text)
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn matches_foundation() {
        let cases = [
            ("plain text", r#""plain text":"#),
            (
                "**bold** and *it* and _it2_ and __bold2__",
                r#""bold":B " and ": "it":I " and ": "it2":I " and ": "bold2":B"#,
            ),
            ("***both***", r#""both":BI"#),
            ("`code **x**` tail", r#""code **x**":C " tail":"#),
            ("[link](https://a.com) x", r#""link":L(https://a.com) " x":"#),
            (
                "see https://a.com/b?c=1 now",
                r#""see ": "https://a.com/b?c=1":L(https://a.com/b?c=1) " now":"#,
            ),
            ("www.a.com x", r#""www.a.com":L(http://www.a.com) " x":"#),
            ("<https://a.com>", r#""https://a.com":L(https://a.com)"#),
            ("~~strike~~ ~one~", r#""strike":S " ": "one":S"#),
            ("\\*esc\\* \\# a", r##""*esc* # a":"##),
            ("a*b*c a_b_c", r#""a": "b":I "c a_b_c":"#),
            ("**unclosed", r#""**unclosed":"#),
            ("*a **b** c*", r#""a ":I "b":BI " c":I"#),
            ("[no url]", r#""[no url]":"#),
            ("![img](x.png)", r#""img":"#),
            ("line one\nline two", r#""line one\nline two":"#),
            ("  leading  spaces  ", r#""  leading  spaces  ":"#),
            ("a  \nb", r#""a  \nb":"#),
            ("==hi==", r#""==hi==":"#),
            ("&amp; &lt;", r#""& <":"#),
            ("<b>html</b>", r#""<b>html</b>":"#),
            ("中文**粗体**中文", r#""中文": "粗体":B "中文":"#),
            ("foo@bar.com", r#""foo@bar.com":L(mailto:foo@bar.com)"#),
            ("[a](<b c>)", r#""a":L(b%20c)"#),
            ("**a*b*c**", r#""a":B "b":BI "c":B"#),
            ("`a``b`", r#""a``b":C"#),
            ("``a`b``", r#""a`b":C"#),
            ("http://x.y", r#""http://x.y":L(http://x.y)"#),
            ("1. not list", r#""1. not list":"#),
            ("# not heading", r##""# not heading":"##),
            ("[link](fumi-tag://a)", r#""link":L(fumi-tag://a)"#),
            ("a\\\nb", r#""a\nb":"#),
            ("tab\there", r#""tab\there":"#),
            ("**bold **", r#""**bold **":"#),
            ("* a *", r#""* a *":"#),
            ("_a_b", r#""_a_b":"#),
            ("[t **b**](u)", r#""t b":L(u)"#),
        ];
        for (source, expected) in cases {
            assert_eq!(show(source), expected, "{source:?}");
        }
    }

    #[test]
    fn trailing_punctuation_stays_out_of_a_bare_link() {
        assert_eq!(show("https://a.com."), r#""https://a.com":L(https://a.com) ".":"#);
        assert_eq!(show("(https://a.com)"), r#""(": "https://a.com":L(https://a.com) ")":"#);
    }

    #[test]
    fn plain_drops_the_syntax() {
        assert_eq!(plain("**a** `b` [c](d)"), "a b c");
    }
}
