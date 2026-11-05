//! The Xiaohongshu Cards extension's input: a note broken into the blocks the
//! card layout paginates, plus the post's title and caption, and what can't go
//! on cards. The title is the post's, not a card's: the cards start with the
//! note's first words. Port of `Services/XhsDeck.swift`.
//!
//! The JSON has the same keys as Swift's `Codable`, for the card layout.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::frontmatter::{Frontmatter, Value};
use crate::fuzzy_search::foundation::{
    drop_first_grapheme, grapheme_count, has_prefix, has_suffix, is_word_char, lowercased, strip_regex_final_terminator,
    trim_whitespaces,
};
use crate::memo_markdown::{Delimited, never};
use crate::tags;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockKind {
    Heading,
    Paragraph,
    Figure,
    Quote,
    List,
    Code,
    Table,
    PageBreak,
}

impl BlockKind {
    /// The name Swift's `rawValue` gives, which block ids are hashed from.
    fn raw_value(self) -> &'static str {
        match self {
            BlockKind::Heading => "heading",
            BlockKind::Paragraph => "paragraph",
            BlockKind::Figure => "figure",
            BlockKind::Quote => "quote",
            BlockKind::List => "list",
            BlockKind::Code => "code",
            BlockKind::Table => "table",
            BlockKind::PageBreak => "pageBreak",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// Stable across edits elsewhere in the note: Leave Out of Cards remembers
    /// blocks by it.
    pub id: String,
    pub kind: BlockKind,
    /// Headings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<usize>,
    /// Headings, quotes and tables, as inline HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// Paragraphs, one inline-HTML string per sentence: a paragraph may break
    /// between two cards only there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentences: Option<Vec<String>>,
    /// Lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordered: Option<bool>,
    /// Figures: the image, its caption (the alt text), and the short sentence
    /// before it that introduces it, which stays on its card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
    /// Code blocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Block {
    fn new(id: &str, kind: BlockKind) -> Self {
        Block {
            id: id.to_string(),
            kind,
            level: None,
            html: None,
            sentences: None,
            items: None,
            ordered: None,
            src: None,
            caption: None,
            lead: None,
            text: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XhsDeck {
    pub blocks: Vec<Block>,
    /// The post's title: `xhs_title` in the front matter, else the note's.
    pub title: String,
    /// The post's text: the whole note as plain text, and its tags.
    pub caption: String,
    /// What can't go on cards, in English.
    pub warnings: Vec<String>,
    /// Blocks hidden with Leave Out of Cards.
    pub left_out_count: usize,
}

pub const CARD_LIMIT: usize = 18;

impl XhsDeck {
    /// Builds a deck. Image targets are checked on disk, against the note's
    /// folder.
    pub fn make(markdown: &str, note_path: &str, file_name: &str, left_out: &HashSet<String>) -> XhsDeck {
        let frontmatter = Frontmatter::parse(markdown);
        let body = frontmatter.as_ref().map_or(markdown, |f| &markdown[f.range.end..]);
        let field = |key: &str| -> Option<String> {
            let found = frontmatter.as_ref()?.fields.iter().find(|f| f.key == key)?;
            let Value::Text(value) = &found.value else {
                return None;
            };
            let trimmed = trim_whitespaces(value);
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };

        let mut raw = raw_blocks(body);
        let mut warnings = Vec::new();

        // The title: the front matter's, else a leading # heading (left off the
        // cards: it is the post's title), else the file name.
        let mut title = field("title");
        if raw.first().is_some_and(|first| first.kind == BlockKind::Heading && first.level == Some(1)) {
            let first = raw.remove(0);
            if title.is_none() {
                title = Some(plain(&first.markdown));
            }
        }
        let note_title = title.unwrap_or_else(|| deleting_path_extension(file_name).to_string());

        // Images resolve against the note's folder; videos can't go on cards.
        let folder = deleting_last_path_component(note_path);
        let mut videos = 0;
        let mut missing = Vec::new();
        raw.retain_mut(|block| {
            if block.kind != BlockKind::Figure {
                return true;
            }
            let Some(target) = block.src.clone() else {
                return true;
            };
            if is_video(&target) {
                videos += 1;
                return false;
            }
            match resolve_image(&target, folder) {
                Some(resolved) => {
                    block.src = Some(resolved);
                    true
                }
                None => {
                    missing.push(target);
                    false
                }
            }
        });
        if videos == 1 {
            warnings.push("1 video can't go on a card; add it in Xiaohongshu.".to_string());
        } else if videos > 1 {
            warnings.push(format!("{videos} videos can't go on cards; add them in Xiaohongshu."));
        }
        warnings.extend(missing.iter().map(|target| format!("Image not found: {target}")));
        if raw.iter().any(|block| block.markdown.contains("$$")) {
            warnings.push("Math stays as plain text.".to_string());
        }

        // Stable ids, and what Leave Out of Cards hid.
        let mut seen: HashMap<String, usize> = HashMap::new();
        for block in &mut raw {
            let base = fingerprint(&format!("{}\u{1}{}", block.kind.raw_value(), block.markdown));
            let count = seen.entry(base.clone()).or_insert(0);
            block.id = if *count == 0 { base } else { format!("{base}-{count}") };
            *count += 1;
        }
        let left_out_count = raw.iter().filter(|block| left_out.contains(&block.id)).count();
        raw.retain(|block| !left_out.contains(&block.id));

        let mut topics = tags::frontmatter_tags(markdown);
        if topics.is_empty() {
            topics = tags::extract(body);
        }
        XhsDeck {
            blocks: with_leads(&raw),
            title: field("xhs_title").unwrap_or(note_title),
            caption: caption(&raw, &topics),
            warnings,
            left_out_count,
        }
    }
}

/// A block as written, before its sentences and leads are worked out.
struct RawBlock {
    id: String,
    kind: BlockKind,
    level: Option<usize>,
    /// The block's Markdown, for its text and its id.
    markdown: String,
    items: Option<Vec<String>>,
    ordered: Option<bool>,
    src: Option<String>,
    caption: Option<String>,
}

impl RawBlock {
    fn new(kind: BlockKind, markdown: String) -> Self {
        RawBlock { id: String::new(), kind, level: None, markdown, items: None, ordered: None, src: None, caption: None }
    }
}

// ICU's `.`: everything but a line terminator. `regex`'s stops only at `\n`.
macro_rules! icu_dot {
    () => {
        r"[^\n\x0B\x0C\r\x{85}\x{2028}\x{2029}]"
    };
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid deck pattern")
}

// Patterns ending in `$` run on `strip_regex_final_terminator(line)`.
static HEADING_LINE: LazyLock<Regex> = LazyLock::new(|| regex(concat!(r"^(#{1,6})[ \t]+(", icu_dot!(), r"*?)[ \t#]*$")));
static LIST_LINE: LazyLock<Regex> =
    LazyLock::new(|| regex(concat!(r"^[ \t]*([-*+]|(\d+)[.)])[ \t]+(?:\[[ xX]\][ \t]+)?(", icu_dot!(), r"*)$")));
static IMAGE_LINE: LazyLock<Regex> =
    LazyLock::new(|| regex(r#"^!\[([^\]\n]*)\]\(<?([^)>\n]*?)>?(?:[ \t]+"[^"\n]*")?\)$"#));
static TABLE_SEPARATOR: LazyLock<Regex> =
    LazyLock::new(|| regex(r"^[ \t]*\|?[ \t]*:?-{2,}:?[ \t]*(\|[ \t]*:?-{2,}:?[ \t]*)*\|?[ \t]*$"));

/// `^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$`: `regex` has no backreferences.
fn is_rule_line(line: &str) -> bool {
    let line = strip_regex_final_terminator(line);
    let spaces = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[spaces..];
    if spaces > 3 {
        return false;
    }
    let Some(mark @ ('-' | '*' | '_')) = rest.chars().next() else {
        return false;
    };
    rest.chars().all(|c| c == mark || c == ' ' || c == '\t') && rest.chars().filter(|c| *c == mark).count() >= 3
}

fn raw_blocks(body: &str) -> Vec<RawBlock> {
    let normalized = body.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut blocks: Vec<RawBlock> = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut index = 0;

    fn flush(blocks: &mut Vec<RawBlock>, paragraph: &mut Vec<&str>) {
        if !paragraph.is_empty() {
            blocks.push(RawBlock::new(BlockKind::Paragraph, join_lines(paragraph)));
            paragraph.clear();
        }
    }

    while index < lines.len() {
        let line = lines[index];
        let trimmed = trim_whitespaces(line);

        if trimmed.is_empty() || (has_prefix(trimmed, "<!--") && has_suffix(trimmed, "-->")) {
            flush(&mut blocks, &mut paragraph);
            index += 1;
            continue;
        }
        // Fenced code.
        if has_prefix(trimmed, "```") || has_prefix(trimmed, "~~~") {
            flush(&mut blocks, &mut paragraph);
            let fence = &trimmed[..3];
            let mut code: Vec<&str> = Vec::new();
            index += 1;
            while index < lines.len() && !has_prefix(trim_whitespaces(lines[index]), fence) {
                code.push(lines[index]);
                index += 1;
            }
            index += 1;
            blocks.push(RawBlock::new(BlockKind::Code, code.join("\n")));
            continue;
        }
        if let Some(parts) = HEADING_LINE.captures(strip_regex_final_terminator(trimmed)) {
            flush(&mut blocks, &mut paragraph);
            let mut block = RawBlock::new(BlockKind::Heading, parts[2].to_string());
            block.level = Some(parts[1].len());
            blocks.push(block);
            index += 1;
            continue;
        }
        if is_rule_line(line) {
            flush(&mut blocks, &mut paragraph);
            blocks.push(RawBlock::new(BlockKind::PageBreak, "---".to_string()));
            index += 1;
            continue;
        }
        if let Some(parts) = IMAGE_LINE.captures(strip_regex_final_terminator(trimmed)) {
            flush(&mut blocks, &mut paragraph);
            let mut block = RawBlock::new(BlockKind::Figure, trimmed.to_string());
            block.src = Some(trim_whitespaces(&parts[2]).to_string());
            block.caption = Some(trim_whitespaces(&parts[1]).to_string());
            blocks.push(block);
            index += 1;
            continue;
        }
        // Obsidian embeds (![[file.png]]) count as images too.
        if has_prefix(trimmed, "![[") && has_suffix(trimmed, "]]") {
            flush(&mut blocks, &mut paragraph);
            let inner = trimmed.get(3..trimmed.len().saturating_sub(2)).unwrap_or("");
            let target = inner.split('|').next().unwrap_or("");
            let mut block = RawBlock::new(BlockKind::Figure, trimmed.to_string());
            block.src = Some(target.to_string());
            block.caption = Some(String::new());
            blocks.push(block);
            index += 1;
            continue;
        }
        if has_prefix(trimmed, ">") {
            flush(&mut blocks, &mut paragraph);
            let mut quote: Vec<String> = Vec::new();
            while index < lines.len() {
                let next = trim_whitespaces(lines[index]);
                if !has_prefix(next, ">") {
                    break;
                }
                let mut text = drop_first_grapheme(next);
                if has_prefix(text, " ") {
                    text = &text[1..];
                }
                // A callout's [!type] line is its label, not its words.
                if quote.is_empty() && has_prefix(text, "[!") {
                    let label = text.split_once(']').map_or("", |(_, label)| label);
                    let label = label.trim_matches(['-', '+', ' ']);
                    if !label.is_empty() {
                        quote.push(format!("**{label}**"));
                    }
                } else {
                    quote.push(text.to_string());
                }
                index += 1;
            }
            let lines: Vec<&str> = quote.iter().map(String::as_str).collect();
            blocks.push(RawBlock::new(BlockKind::Quote, join_lines(&lines)));
            continue;
        }
        if let Some(first) = LIST_LINE.captures(strip_regex_final_terminator(line)) {
            flush(&mut blocks, &mut paragraph);
            let ordered = first.get(2).is_some_and(|digits| !digits.is_empty());
            let mut items: Vec<String> = Vec::new();
            while index < lines.len() {
                let next = lines[index];
                if let Some(parts) = LIST_LINE.captures(strip_regex_final_terminator(next)) {
                    items.push(parts[3].to_string());
                } else if !trim_whitespaces(next).is_empty()
                    && (has_prefix(next, " ") || has_prefix(next, "\t"))
                    && let Some(last) = items.last_mut()
                {
                    *last = join_lines(&[last.as_str(), trim_whitespaces(next)]);
                } else {
                    break;
                }
                index += 1;
            }
            let mut block = RawBlock::new(BlockKind::List, items.join("\n"));
            block.items = Some(items);
            block.ordered = Some(ordered);
            blocks.push(block);
            continue;
        }
        if trimmed.contains('|')
            && index + 1 < lines.len()
            && TABLE_SEPARATOR.is_match(strip_regex_final_terminator(lines[index + 1]))
        {
            flush(&mut blocks, &mut paragraph);
            let mut rows: Vec<&str> = vec![line];
            index += 2;
            while index < lines.len() && lines[index].contains('|') && !trim_whitespaces(lines[index]).is_empty() {
                rows.push(lines[index]);
                index += 1;
            }
            blocks.push(RawBlock::new(BlockKind::Table, rows.join("\n")));
            continue;
        }
        paragraph.push(line);
        index += 1;
    }
    flush(&mut blocks, &mut paragraph);
    blocks
}

/// Lines of one paragraph as one: no space where Chinese meets Chinese.
fn join_lines(lines: &[&str]) -> String {
    let mut joined = String::new();
    for line in lines.iter().map(|line| trim_whitespaces(line)).filter(|line| !line.is_empty()) {
        if let (Some(last), Some(first)) = (joined.graphemes(true).next_back(), line.graphemes(true).next())
            && !is_cjk(last)
            && !is_cjk(first)
        {
            joined.push(' ');
        }
        joined.push_str(line);
    }
    joined
}

/// The blocks for the layout: paragraphs in sentences, and each image with the
/// short sentence that introduces it.
fn with_leads(raw: &[RawBlock]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    for block in raw {
        let mut out = Block::new(&block.id, block.kind);
        match block.kind {
            BlockKind::Paragraph => {
                out.sentences = Some(sentences(&block.markdown).iter().map(|s| inline(s)).collect());
            }
            BlockKind::Figure => {
                // A sentence ending in a colon, or a paragraph of one short
                // sentence, right before the image goes with it.
                if let Some(last) = blocks.last_mut()
                    && last.kind == BlockKind::Paragraph
                    && let Some(sentences) = &mut last.sentences
                    && let Some(fin) = sentences.last()
                {
                    let text = plain_html(fin);
                    let ends_in_colon = has_suffix(&text, "：") || has_suffix(&text, ":");
                    if ends_in_colon || (sentences.len() == 1 && grapheme_count(&text) <= 40) {
                        out.lead = sentences.pop();
                        if sentences.is_empty() {
                            blocks.pop();
                        }
                    }
                }
                out.src = block.src.clone();
                out.caption = block.caption.as_deref().filter(|c| !c.is_empty()).map(escape);
            }
            BlockKind::Heading => {
                out.level = block.level;
                out.html = Some(inline(&block.markdown));
            }
            BlockKind::Quote => out.html = Some(inline(&block.markdown)),
            BlockKind::List => {
                out.items = Some(block.items.iter().flatten().map(|item| inline(item)).collect());
                out.ordered = block.ordered;
            }
            BlockKind::Code => out.text = Some(block.markdown.clone()),
            BlockKind::Table => out.html = Some(table(&block.markdown)),
            BlockKind::PageBreak => {}
        }
        blocks.push(out);
    }
    blocks
}

// Sentences.

fn is_sentence_end(c: &str) -> bool {
    matches!(c, "。" | "！" | "？" | "!" | "?" | "；" | "…")
}

fn is_closer(c: &str) -> bool {
    matches!(c, "”" | "’" | "」" | "』" | "）" | ")" | "】" | "\"" | "'" | "*" | "_" | "~" | "=" | "。" | "…")
}

/// `markdown` in sentences, cut only where no bold, code or link is open, so
/// each piece stands as Markdown of its own. A very long sentence is cut again
/// at its commas.
pub fn sentences(markdown: &str) -> Vec<String> {
    let characters: Vec<&str> = markdown.graphemes(true).collect();
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    let (mut bold, mut italic, mut code, mut brackets) = (false, false, false, 0usize);
    let mut index = 0;

    // Appends the character at `index` (two for `**`), tracking what it opens or closes.
    let take = |index: &mut usize, current: &mut String, bold: &mut bool, italic: &mut bool, code: &mut bool, brackets: &mut usize| {
        let character = characters[*index];
        current.push_str(character);
        if *code {
            if character == "`" {
                *code = false;
            }
            return;
        }
        match character {
            "`" => *code = true,
            "*" => {
                if characters.get(*index + 1) == Some(&"*") {
                    *index += 1;
                    current.push('*');
                    *bold = !*bold;
                } else {
                    *italic = !*italic;
                }
            }
            "[" => *brackets += 1,
            "]" => *brackets = brackets.saturating_sub(1),
            _ => {}
        }
    };

    while index < characters.len() {
        let character = characters[index];
        take(&mut index, &mut current, &mut bold, &mut italic, &mut code, &mut brackets);
        if !code && is_sentence_end(character) {
            // Take the closing marks and quotes that belong to it.
            while characters.get(index + 1).is_some_and(|next| is_closer(next)) {
                index += 1;
                take(&mut index, &mut current, &mut bold, &mut italic, &mut code, &mut brackets);
            }
            if !bold && !italic && !code && brackets == 0 {
                pieces.push(std::mem::take(&mut current));
            }
        } else if !bold && !italic && !code && brackets == 0 && character == "，" && grapheme_count(&plain(&current)) > 90 {
            pieces.push(std::mem::take(&mut current));
        }
        index += 1;
    }
    if !trim_whitespaces(&current).is_empty() {
        pieces.push(current);
    }
    pieces.iter().map(|piece| trim_whitespaces(piece)).filter(|piece| !piece.is_empty()).map(str::to_string).collect()
}

// Inline Markdown.

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

static UNESCAPE: LazyLock<Regex> = LazyLock::new(|| regex(r"\\([\\`*_{}\[\]()#+\-.!~=|<>])"));
static BREAK_TAG: LazyLock<Regex> = LazyLock::new(|| regex(r"&lt;br\s*/?&gt;"));
static IMAGE_TEXT: LazyLock<Regex> = LazyLock::new(|| regex(r"!\[([^\]]*)\]\([^)]*\)"));
static LINK_TEXT: LazyLock<Regex> = LazyLock::new(|| regex(r"\[([^\]]+)\]\((?:[^)]*)\)"));
static WIKI_TEXT: LazyLock<Regex> = LazyLock::new(|| regex(r"\[\[(?:[^\]|]*\|)?([^\]]+)\]\]"));
static OTHER_HTML: LazyLock<Regex> = LazyLock::new(|| regex(r"&lt;/?[A-Za-z][^&]*?&gt;"));
static ANY_TAG: LazyLock<Regex> = LazyLock::new(|| regex(r"<[^>]+>"));

fn word_or_star(c: char) -> bool {
    c == '*' || is_word_char(c)
}

fn star(c: char) -> bool {
    c == '*'
}

fn pair(open: &'static str, close: &'static str) -> Delimited {
    Delimited {
        open,
        close,
        refuse_before_open: never,
        refuse_first: char::is_whitespace,
        refuse_last: char::is_whitespace,
        refuse_after_close: never,
    }
}

/// One `(...)` capture wrapped in `before` and `after`, in place of each match.
fn wrap(text: &str, rule: &Delimited, before: &str, after: &str) -> String {
    let matches = rule.matches(text);
    if matches.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut position = 0;
    for (start, end) in matches {
        out.push_str(&text[position..start]);
        out.push_str(before);
        out.push_str(&text[start + rule.open.len()..end - rule.close.len()]);
        out.push_str(after);
        position = end;
    }
    out.push_str(&text[position..]);
    out
}

fn is_backtick_at(s: &str, at: usize) -> bool {
    has_prefix(&s[at..], "`")
        && (at == 0 || unicode_segmentation::GraphemeCursor::new(at, s.len(), true).is_boundary(s, 0).unwrap_or(true))
}

fn find_backtick(s: &str) -> Option<usize> {
    s.match_indices('`').map(|(at, _)| at).find(|&at| is_backtick_at(s, at))
}

/// Inline Markdown as HTML: bold, italic, strike, highlight, underline, code and
/// links (as text: a card can't be clicked). Other HTML is dropped.
pub fn inline(markdown: &str) -> String {
    // Code spans first, so nothing inside them is read as Markdown.
    let mut codes: Vec<&str> = Vec::new();
    let mut text = String::new();
    let mut rest = markdown;
    while let Some(open) = find_backtick(rest) {
        text.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = find_backtick(after) else {
            text.push_str(&rest[open..]);
            rest = "";
            break;
        };
        codes.push(&after[..close]);
        text.push_str(&format!("\0{}\0", codes.len() - 1));
        rest = &after[close + 1..];
    }
    text.push_str(rest);

    let unescaped = UNESCAPE.replace_all(&text, "$1");
    let mut html = escape(&unescaped);
    html = wrap(&html, &pair("**", "**"), "<b>", "</b>");
    html = wrap(&html, &pair("__", "__"), "<b>", "</b>");
    html = wrap(
        &html,
        &Delimited {
            open: "*",
            close: "*",
            refuse_before_open: word_or_star,
            refuse_first: char::is_whitespace,
            refuse_last: char::is_whitespace,
            refuse_after_close: star,
        },
        "<i>",
        "</i>",
    );
    html = wrap(&html, &pair("~~", "~~"), "<s>", "</s>");
    html = wrap(&html, &pair("==", "=="), "<mark>", "</mark>");
    html = wrap(
        &html,
        &Delimited {
            open: "&lt;u&gt;",
            close: "&lt;/u&gt;",
            refuse_before_open: never,
            refuse_first: never,
            refuse_last: never,
            refuse_after_close: never,
        },
        "<u>",
        "</u>",
    );
    html = BREAK_TAG.replace_all(&html, " ").into_owned();
    html = IMAGE_TEXT.replace_all(&html, "${1}").into_owned();
    html = LINK_TEXT.replace_all(&html, "<span class=\"link\">${1}</span>").into_owned();
    html = WIKI_TEXT.replace_all(&html, "${1}").into_owned();
    html = OTHER_HTML.replace_all(&html, "").into_owned();
    for (index, code) in codes.iter().enumerate() {
        html = html.replace(&format!("\0{index}\0"), &format!("<code>{}</code>", escape(code)));
    }
    trim_whitespaces(&html).to_string()
}

/// The words alone.
pub fn plain(markdown: &str) -> String {
    plain_html(&inline(markdown))
}

pub fn plain_html(html: &str) -> String {
    let text = ANY_TAG.replace_all(html, "");
    let text = text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&");
    trim_whitespaces(&text).to_string()
}

fn table(markdown: &str) -> String {
    fn cells(row: &str) -> Vec<String> {
        let mut line = trim_whitespaces(row);
        if has_prefix(line, "|") {
            line = &line[1..];
        }
        if has_suffix(line, "|") {
            line = &line[..line.len() - 1];
        }
        line.split('|').map(|cell| inline(trim_whitespaces(cell))).collect()
    }
    let mut rows = markdown.split('\n');
    let header: String = rows.next().map_or(Vec::new(), cells).iter().map(|c| format!("<th>{c}</th>")).collect();
    let body: String = rows
        .map(|row| format!("<tr>{}</tr>", cells(row).iter().map(|c| format!("<td>{c}</td>")).collect::<String>()))
        .collect();
    format!("<table><thead><tr>{header}</tr></thead><tbody>{body}</tbody></table>")
}

// Caption.

/// The whole note as plain text (headings, paragraphs, quotes and lists, in
/// order), then the note's tags as topics. Images, code and tables are on the
/// cards only.
fn caption(raw: &[RawBlock], tags: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for block in raw {
        match block.kind {
            BlockKind::Heading | BlockKind::Paragraph | BlockKind::Quote => parts.push(plain(&block.markdown)),
            BlockKind::List => {
                let ordered = block.ordered == Some(true);
                let items: Vec<String> = block
                    .items
                    .iter()
                    .flatten()
                    .enumerate()
                    .map(|(index, item)| {
                        let marker = if ordered { format!("{}. ", index + 1) } else { "• ".to_string() };
                        marker + &plain(item)
                    })
                    .collect();
                parts.push(items.join("\n"));
            }
            BlockKind::Figure | BlockKind::Code | BlockKind::Table | BlockKind::PageBreak => {}
        }
    }
    parts.retain(|part| !part.is_empty());
    let caption = parts.join("\n\n");
    let topics: Vec<String> = tags
        .iter()
        .map(|tag| format!("#{}", tag.split('/').rfind(|part| !part.is_empty()).unwrap_or(tag)))
        .collect();
    if topics.is_empty() { caption } else { format!("{caption}\n\n{}", topics.join(" ")) }
}

// Images.

fn is_video(target: &str) -> bool {
    let path = target.split('?').next().unwrap_or("");
    let extension = lowercased(path_extension(path));
    matches!(extension.as_str(), "mp4" | "mov" | "m4v" | "webm" | "avi" | "mkv")
}

/// A web image as it is; a file next to the note through the card view's own
/// scheme; `None` when the file isn't there.
fn resolve_image(target: &str, folder: &str) -> Option<String> {
    if target.starts_with("http://") || target.starts_with("https://") || target.starts_with("data:image/") {
        return Some(target.to_string());
    }
    let mut path = removing_percent_encoding(target).unwrap_or_else(|| target.to_string());
    if path.starts_with("file://")
        && let Some(url_path) = file_url_path(target)
    {
        path = url_path;
    }
    let absolute = if path.starts_with('/') { path } else { appending_path_component(folder, &path) };
    let standardized = standardizing_path(&absolute);
    let extension = lowercased(path_extension(&standardized));
    let is_image =
        matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "tif" | "tiff" | "bmp" | "svg");
    (Path::new(&standardized).exists() && is_image).then(|| asset_url(&standardized))
}

/// `fumi-xhs://local/<path>`: how the card view loads a file on this Mac.
pub fn asset_url(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        // `CharacterSet.urlPathAllowed`, as dumped from Foundation. A colon is
        // encoded only in a relative path's first segment, where a URL would
        // read it as a scheme.
        let colon_allowed = byte == b':' && (path.starts_with('/') || encoded.contains('/'));
        if byte.is_ascii_alphanumeric() || b"!$&'()*+,-./;=@_~".contains(&byte) || colon_allowed {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    if encoded.starts_with('/') { format!("fumi-xhs://local{encoded}") } else { format!("fumi-xhs://local/{encoded}") }
}

/// `removingPercentEncoding`: `None` for a stray `%` or bytes that aren't UTF-8.
fn removing_percent_encoding(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let high = char::from(*bytes.get(i + 1)?).to_digit(16)?;
            let low = char::from(*bytes.get(i + 2)?).to_digit(16)?;
            out.push((high * 16 + low) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `URL(string:)?.path` of a `file://` URL: past the host, before any query or
/// fragment, percent-decoded where that's valid.
fn file_url_path(target: &str) -> Option<String> {
    let rest = target.strip_prefix("file://")?;
    let path = rest.find('/').map_or("", |slash| &rest[slash..]);
    let path = path.split(['?', '#']).next().unwrap_or("");
    Some(removing_percent_encoding(path).unwrap_or_else(|| path.to_string()))
}

fn last_component(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

/// The dot before a valid extension in a last path component: not a leading
/// one, not with only dots before it, and no space or empty extension after it.
fn extension_dot(component: &str) -> Option<usize> {
    let dot = component.rfind('.')?;
    let (stem, extension) = (&component[..dot], &component[dot + 1..]);
    (!extension.is_empty() && !extension.contains(' ') && stem.chars().any(|c| c != '.')).then_some(dot)
}

/// `NSString.pathExtension`.
fn path_extension(path: &str) -> &str {
    let component = last_component(path);
    extension_dot(component).map_or("", |dot| &component[dot + 1..])
}

/// `NSString.deletingPathExtension`.
fn deleting_path_extension(path: &str) -> &str {
    let trimmed = if path.len() > 1 { path.trim_end_matches('/') } else { path };
    let component = last_component(trimmed);
    match extension_dot(component) {
        Some(dot) => &trimmed[..trimmed.len() - component.len() + dot],
        None => trimmed,
    }
}

/// `NSString.deletingLastPathComponent`.
fn deleting_last_path_component(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return if path.is_empty() { "" } else { "/" };
    }
    match trimmed.rfind('/') {
        None => "",
        Some(0) => "/",
        Some(slash) => &trimmed[..slash],
    }
}

/// `NSString.appendingPathComponent`.
fn appending_path_component(folder: &str, component: &str) -> String {
    if folder.is_empty() {
        component.to_string()
    } else {
        format!("{}/{component}", folder.trim_end_matches('/'))
    }
}

/// `NSString.standardizingPath` for what reaches it here: `.`, `..` and empty
/// components worked out in an absolute path, and a leading `/private` dropped
/// when the rest still exists. A relative path keeps its components.
fn standardizing_path(path: &str) -> String {
    if !path.starts_with('/') {
        return path.to_string();
    }
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
    let standardized = format!("/{}", parts.join("/"));
    match standardized.strip_prefix("/private/") {
        Some(rest) if Path::new("/").join(rest).exists() => format!("/{rest}"),
        _ => standardized,
    }
}

// Characters.

pub fn is_cjk(character: &str) -> bool {
    character.chars().any(|c| {
        matches!(c as u32, 0x2E80..=0x2FFF | 0x3040..=0x9FFF | 0xF900..=0xFAFF | 0x3000..=0x303F | 0xFF00..=0xFFEF)
    })
}

/// FNV-1a, as hex: short and the same on every launch, and in the Swift app.
fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:x}")
}

#[cfg(test)]
mod tests {
    // Ported from Tests/XhsDeckTests.swift.
    use super::*;

    fn deck(markdown: &str) -> XhsDeck {
        deck_in(markdown, "Note.md")
    }

    fn deck_in(markdown: &str, file: &str) -> XhsDeck {
        XhsDeck::make(markdown, &format!("/tmp/notes/{file}"), file, &HashSet::new())
    }

    fn kinds(deck: &XhsDeck) -> Vec<BlockKind> {
        deck.blocks.iter().map(|b| b.kind).collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    use BlockKind::*;

    #[test]
    fn sentences_break_only_where_nothing_is_open() {
        assert_eq!(sentences("一句。两句！三句？"), ["一句。", "两句！", "三句？"], "three sentences");
        assert_eq!(sentences("**加粗。还在加粗**，结束。"), ["**加粗。还在加粗**，结束。"], "no break inside bold");
        assert_eq!(sentences("他说：“好。”然后走了。"), ["他说：“好。”", "然后走了。"], "closing quote stays");
        assert_eq!(sentences("用 `a.b()。` 调用。好。"), ["用 `a.b()。` 调用。", "好。"], "no break inside code");
        assert_eq!(sentences("*斜体。*后面。"), ["*斜体。*", "后面。"], "italic closes with its sentence");
    }

    #[test]
    fn inline_markdown() {
        assert_eq!(inline("等到了**Kimi K3.1 Pro**。"), "等到了<b>Kimi K3.1 Pro</b>。", "bold");
        assert_eq!(inline("在<u>幻觉测试</u>上"), "在<u>幻觉测试</u>上", "underline kept");
        assert_eq!(inline("a <script>x</script> b"), "a x b", "other HTML dropped");
        assert_eq!(
            inline("看 [这里](https://x.y) 和 `**不加粗**`"),
            "看 <span class=\"link\">这里</span> 和 <code>**不加粗**</code>",
            "links and code"
        );
    }

    #[test]
    fn titles() {
        // The title comes from the leading heading, which leaves the cards: they
        // start with the note's first words, short or not.
        let titled = deck("# 标题\n\n第一段很短。\n\n第二段。");
        assert_eq!(titled.title, "标题", "title from H1");
        assert_eq!(kinds(&titled), [Paragraph, Paragraph], "H1 leaves the blocks; the first paragraph stays");

        // The title falls back to the file name; front matter wins over both.
        assert_eq!(deck_in("正文。", "刚刚.md").title, "刚刚", "title from file name");
        let fm = deck("---\ntitle: 前言\nxhs_title: 短标题\ntags: [AI/新模型]\n---\n\n# 不是这个\n\n正文。");
        assert_eq!(fm.title, "短标题", "xhs_title for the post");
        assert_eq!(deck("---\ntitle: 前言\n---\n\n正文。").title, "前言", "front matter title");
        assert!(fm.caption.ends_with("#新模型"), "tags end the caption as topics");
    }

    #[test]
    fn figures_and_leads() {
        // An image takes the colon sentence before it, and its alt as caption.
        let figure = deck("开头很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的一段。\n\n基于 K3 做的后训练。下面是官方放出的 Benchmark：\n\n![官方 Benchmark](https://e.com/a.png)\n## 代码\n必须要提一下。");
        assert_eq!(kinds(&figure), [Paragraph, Paragraph, Figure, Heading, Paragraph], "blocks");
        assert_eq!(figure.blocks[1].sentences, Some(strings(&["基于 K3 做的后训练。"])), "colon sentence left the paragraph");
        assert_eq!(figure.blocks[2].lead.as_deref(), Some("下面是官方放出的 Benchmark："), "lead");
        assert_eq!(figure.blocks[2].caption.as_deref(), Some("官方 Benchmark"), "caption");
        assert_eq!(figure.blocks[2].src.as_deref(), Some("https://e.com/a.png"), "web image as is");

        // A one-sentence bold paragraph before an image is its lead too.
        let bold = deck("前面一段很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的话。\n\n**除了性能之外，还有一件事儿是成本。**\n\n![](https://e.com/b.png)");
        assert_eq!(kinds(&bold), [Paragraph, Figure], "bold lead joined the figure");
        assert_eq!(bold.blocks[1].lead.as_deref(), Some("<b>除了性能之外，还有一件事儿是成本。</b>"), "bold lead");

        // Videos and missing files leave the cards, with a warning.
        let media = deck("长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n![](attachments/a.mp4)\n\n![](attachments/b.mp4)\n\n![](missing.png)");
        assert_eq!(kinds(&media), [Paragraph], "media left out");
        assert_eq!(media.warnings, ["2 videos can't go on cards; add them in Xiaohongshu.", "Image not found: missing.png"], "warnings");
    }

    #[test]
    fn block_kinds_and_joining() {
        // Rules are page breaks; lists, quotes, code and tables are blocks.
        let mixed = deck("长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n---\n\n- 一\n- 二\n\n> 金句。\n\n```js\nx()\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |");
        assert_eq!(kinds(&mixed), [Paragraph, PageBreak, List, Quote, Code, Table], "kinds");
        assert_eq!(mixed.blocks[2].items, Some(strings(&["一", "二"])), "list items");

        // Lines of a Chinese paragraph join without a space.
        assert_eq!(
            deck("长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n第一行\n第二行").blocks.last().unwrap().sentences,
            Some(strings(&["第一行第二行"])),
            "CJK lines join"
        );
    }

    #[test]
    fn leave_out_of_cards() {
        // Leave Out of Cards hides a block by its id, which survives other edits.
        let full = deck("长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n别放这段。");
        let left_out: HashSet<String> = [full.blocks[1].id.clone()].into();
        let hidden = XhsDeck::make(
            "改了开头长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长长。\n\n别放这段。",
            "/tmp/notes/Note.md",
            "Note.md",
            &left_out,
        );
        assert_eq!(hidden.blocks.len(), 1, "left out");
        assert_eq!(hidden.left_out_count, 1, "left out count");
    }

    #[test]
    fn captions() {
        let kimi = deck("# 刚刚，月之暗面发布Kimi K3.1 Pro。\n\n等啊等，终于等到了**Kimi K3.1 Pro**。\n\n基于**K3**做的。");
        assert_eq!(kimi.caption, "等啊等，终于等到了Kimi K3.1 Pro。\n\n基于K3做的。", "caption");

        // The caption is the whole note: headings, paragraphs, lists, quotes; no images or code.
        let whole = deck("# 标题\n\n很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的第一段。\n\n![](https://e.com/a.png)\n\n## 代码\n\n- 一\n- 二\n\n> 金句。\n\n```\nx()\n```\n\n最后一段。");
        assert_eq!(whole.caption, "很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的第一段。\n\n代码\n\n• 一\n• 二\n\n金句。\n\n最后一段。", "whole note caption");

        // The caption is never cut short, however long the opening is.
        let long = format!("{}。", "很长的开头".repeat(300));
        assert_eq!(deck(&format!("# 标题\n\n{long}")).caption, long, "long caption kept whole");
    }

    // The Swift test's last check reads a real note from ~/Documents when it is
    // there. PORTING.md rules out real notes, so its opening is written out here.
    #[test]
    fn a_note_starts_with_its_first_words() {
        let kimi = deck("# 刚刚，月之暗面发布Kimi K3.1 Pro。\n\n等啊等，终于等到了**Kimi K3.1 Pro**。\n\n基于**K3**做的。");
        assert_eq!(
            kimi.blocks.first().and_then(|b| b.sentences.clone()),
            Some(strings(&["等啊等，终于等到了<b>Kimi K3.1 Pro</b>。"])),
            "real note starts with its first words"
        );
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    fn deck(markdown: &str) -> XhsDeck {
        XhsDeck::make(markdown, "/nonexistent/fumi/Note.md", "Note.md", &HashSet::new())
    }

    #[test]
    fn crlf_notes_read_as_lf_notes() {
        let lf = deck("---\ntitle: 前言\n---\n\n第一行\n第二行。\n\n- 一\n- 二\n\n> 金句\n");
        let crlf = deck("---\r\ntitle: 前言\r\n---\r\n\r\n第一行\r\n第二行。\r\n\r\n- 一\r\n- 二\r\n\r\n> 金句\r\n");
        assert_eq!(lf, crlf, "same blocks and same ids");
        assert_eq!(lf.title, "前言");
    }

    #[test]
    fn ids_match_the_swift_app() {
        // Left-out blocks are remembered by id, so they must not change. Values
        // from running the Swift code.
        let d = deck("同一段。\n\n别放这段。");
        assert_eq!(d.blocks[0].id, "7825a3594ae6933", "no leading zero");
        assert_eq!(d.blocks[1].id, "a6ee2ddacb897e2c");
    }

    #[test]
    fn repeated_blocks_get_numbered_ids() {
        let d = deck("同一段。\n\n同一段。\n\n同一段。");
        let base = &d.blocks[0].id;
        assert_eq!(d.blocks[1].id, format!("{base}-1"));
        assert_eq!(d.blocks[2].id, format!("{base}-2"));
    }

    #[test]
    fn json_keys_are_swifts() {
        let d = deck("![](a.mp4)\n\n---\n\n正文。");
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["leftOutCount"], 0);
        assert_eq!(json["blocks"][0]["kind"], "pageBreak");
        assert!(json["blocks"][0].get("level").is_none(), "nil fields are left out");
        assert_eq!(json["blocks"][1]["sentences"][0], "正文。");
        let back: XhsDeck = serde_json::from_value(json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn images_resolve_on_disk_and_percent_encode() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().canonicalize().unwrap();
        std::fs::create_dir(folder.join("图 片")).unwrap();
        std::fs::write(folder.join("图 片").join("封面 1.PNG"), b"x").unwrap();
        std::fs::write(folder.join("doc.txt"), b"x").unwrap();
        let note = folder.join("Note.md");
        let make = |markdown: &str| XhsDeck::make(markdown, note.to_str().unwrap(), "Note.md", &HashSet::new());

        let folder_str = folder.to_str().unwrap().strip_prefix("/private").unwrap_or(folder.to_str().unwrap());
        let expected = asset_url(&format!("{folder_str}/图 片/封面 1.PNG"));
        assert!(expected.starts_with("fumi-xhs://local/"));
        assert!(expected.contains("%E5%9B%BE%20%E7%89%87/%E5%B0%81%E9%9D%A2%201.PNG"), "{expected}");

        for target in ["![](图%20片/封面%201.PNG)", "![](<图 片/封面 1.PNG>)", "![[图 片/封面 1.PNG|300]]", "![](./图%20片/../图%20片/封面%201.PNG)"] {
            let d = make(target);
            assert_eq!(d.blocks.first().and_then(|b| b.src.as_deref()), Some(expected.as_str()), "{target}");
        }
        let url = format!("![](file://{}/%E5%9B%BE%20%E7%89%87/%E5%B0%81%E9%9D%A2%201.PNG?x=1)", folder.to_str().unwrap());
        assert_eq!(make(&url).blocks[0].src.as_deref(), Some(expected.as_str()), "file URL");

        assert_eq!(make("![](doc.txt)").warnings, ["Image not found: doc.txt"], "not an image");
        assert_eq!(make("![](100%.png)").warnings, ["Image not found: 100%.png"], "a stray % stays as written");
    }

    #[test]
    fn asset_url_allowed_set() {
        // Values from running the Swift code.
        assert_eq!(asset_url("/a:b;c=d@e!$&'()*+,-._~"), "fumi-xhs://local/a:b;c=d@e!$&'()*+,-._~");
        assert_eq!(asset_url("rel/中"), "fumi-xhs://local/rel/%E4%B8%AD");
        assert_eq!(asset_url("a:b/c:d"), "fumi-xhs://local/a%3Ab/c:d");
        assert_eq!(asset_url(":"), "fumi-xhs://local/%3A");
        assert_eq!(asset_url("./a:b"), "fumi-xhs://local/./a:b");
    }

    #[test]
    fn path_helpers_match_nsstring() {
        // (path, extension, deleting extension), from running NSString.
        for (path, extension, deleted) in [
            ("a.md", "md", "a"),
            (".md", "", ".md"),
            ("a.", "", "a."),
            ("a.b.c", "c", "a.b"),
            ("a b.m d", "", "a b.m d"),
            ("a.md/", "md", "a"),
            ("/", "", "/"),
            ("", "", ""),
            ("x/.md", "", "x/.md"),
            ("x/y/", "", "x/y"),
            ("a..b", "b", "a."),
            (".a.b", "b", ".a"),
            ("a.b/c", "", "a.b/c"),
            ("a.中文", "中文", "a"),
            ("a.b-c", "b-c", "a"),
            ("a.b\tc", "b\tc", "a"),
            ("a.b.", "", "a.b."),
            ("..", "", ".."),
            ("...a", "", "...a"),
            ("a.b ", "", "a.b "),
            ("a. b", "", "a. b"),
            ("a.😀", "😀", "a"),
        ] {
            assert_eq!(path_extension(path), extension, "extension of {path:?}");
            assert_eq!(deleting_path_extension(path), deleted, "deleting from {path:?}");
        }
        for (path, parent) in [("a.md", ""), ("/", "/"), ("", ""), ("x/y", "x"), ("x/y/", "x"), ("/x", "/")] {
            assert_eq!(deleting_last_path_component(path), parent, "{path:?}");
        }
        assert_eq!(appending_path_component("", "z"), "z");
        assert_eq!(appending_path_component("/", "z"), "/z");
        assert_eq!(appending_path_component("a", "z"), "a/z");
        for (path, standardized) in [
            ("/a/../b", "/b"),
            ("/a/./b//c/", "/a/b/c"),
            ("/..", "/"),
            ("/a/b/..", "/a"),
            ("a/../b", "a/../b"),
            ("/private/nonexistent-fumi", "/private/nonexistent-fumi"),
            ("/private/etc/hosts", "/etc/hosts"),
            ("/private", "/private"),
        ] {
            assert_eq!(standardizing_path(path), standardized, "{path:?}");
        }
        assert_eq!(removing_percent_encoding("%E4%B8%AD").as_deref(), Some("中"));
        assert_eq!(removing_percent_encoding("%zz"), None);
        assert_eq!(removing_percent_encoding("%FF"), None);
        assert_eq!(removing_percent_encoding("100%"), None);
    }

    #[test]
    fn video_targets() {
        assert!(is_video("a.MP4"));
        assert!(is_video("https://e.com/v.mov?x=1.png"));
        assert!(!is_video("a.png?x=1.mp4"));
        assert_eq!(deck("![](v.webm)").warnings, ["1 video can't go on a card; add it in Xiaohongshu."]);
    }

    #[test]
    fn line_kinds_as_the_patterns_have_them() {
        let kinds = |m: &str| deck(m).blocks.iter().map(|b| b.kind).collect::<Vec<_>>();
        use BlockKind::*;
        assert_eq!(kinds("- - -"), [PageBreak]);
        assert_eq!(kinds("   ***"), [PageBreak]);
        assert_eq!(kinds("____ "), [PageBreak]);
        assert_eq!(kinds("    ---"), [Paragraph], "too indented for a rule");
    }

    #[test]
    fn math_warns() {
        assert_eq!(deck("$$x$$").warnings, ["Math stays as plain text."]);
    }

    #[test]
    fn cjk_and_latin_lines_join_as_written() {
        assert_eq!(join_lines(&["hello", "world"]), "hello world");
        assert_eq!(join_lines(&["你好", "world"]), "你好world");
        assert_eq!(join_lines(&["hello", "世界"]), "hello世界");
        assert_eq!(join_lines(&["  a  ", "", "\u{3000}b"]), "a b");
    }

    #[test]
    fn long_sentences_break_at_commas_and_emoji_count_once() {
        let clause = "字".repeat(95);
        assert_eq!(sentences(&format!("{clause}，后面")), [format!("{clause}，"), "后面".to_string()]);
        let short = "👨\u{200D}👩\u{200D}👧".repeat(60);
        assert_eq!(sentences(&format!("{short}，后面")), [format!("{short}，后面")], "60 characters, however many bytes");
        assert_eq!(sentences("，。"), ["，。"]);
        assert_eq!(sentences("  "), Vec::<String>::new());
    }

    #[test]
    fn inline_edges() {
        assert_eq!(inline("a & b < c \"d\""), "a &amp; b &lt; c &quot;d&quot;");
        // As Swift: unescaping runs before the rules, so this does turn italic.
        assert_eq!(inline(r"\*not\* `<b>`"), "<i>not</i> <code>&lt;b&gt;</code>");
        assert_eq!(inline("`open"), "`open");
        assert_eq!(inline("x<br/>y<br >z"), "x y z");
        assert_eq!(inline("![图](a.png) [[目标|别名]] [[仅目标]]"), "图 别名 仅目标");
        assert_eq!(inline("~~删~~ ==高亮== __粗__ *斜*"), "<s>删</s> <mark>高亮</mark> <b>粗</b> <i>斜</i>");
        assert_eq!(inline("2*3*4"), "2*3*4", "a star after a word opens nothing");
        assert_eq!(inline("** a** *b *"), "<i>* a*</i> *b *", "from running the Swift code");
        assert_eq!(plain("**粗** &amp; <i>"), "粗 &amp;");
        assert_eq!(plain_html("<b>a</b> &lt;b&gt; &amp;amp;"), "a <b> &amp;");
    }

    #[test]
    fn tables_render() {
        assert_eq!(table("| a | **b** |\n| 1 | 2 |"), "<table><thead><tr><th>a</th><th><b>b</b></th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>");
    }

    #[test]
    fn callouts_and_quotes() {
        let d = deck("> [!tip]- 小心\n> 正文\n> 第二行");
        assert_eq!(d.blocks[0].html.as_deref(), Some("<b>小心</b>正文第二行"));
        let d = deck("> [!note]\n> 正文");
        assert_eq!(d.blocks[0].html.as_deref(), Some("正文"));
    }

    #[test]
    fn list_continuations_and_order() {
        let d = deck("1. 一\n   接着\n2. [x] 二\n\n后面");
        assert_eq!(d.blocks[0].items, Some(vec!["一接着".to_string(), "二".to_string()]));
        assert_eq!(d.blocks[0].ordered, Some(true));
        assert_eq!(d.caption, "1. 一接着\n2. 二\n\n后面");
    }

    #[test]
    fn topics_are_last_path_parts_from_the_body_when_no_front_matter_tags() {
        assert_eq!(deck("正文 #AI/新模型 #a2").caption, "正文 #AI/新模型 #a2\n\n#新模型 #a2");
    }

    #[test]
    fn deck_bytes_on_character_boundaries_with_emoji() {
        let d = deck("😀**粗😀**。下一句👨\u{200D}👩\u{200D}👧！");
        assert_eq!(d.blocks[0].sentences, Some(vec!["😀<b>粗😀</b>。".to_string(), "下一句👨\u{200D}👩\u{200D}👧！".to_string()]));
    }
}
