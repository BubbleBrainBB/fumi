//! One source line as it appears on screen: the visible text, its styled runs,
//! and a map back to source bytes. A port of `WysiwygEngine.applyStyle`, line by
//! line: the same rules, the same sizes (body 16 with a fixed line, title 30,
//! headings 26/21/18), the same decorations.
//!
//! Each byte of the line carries style flags, and the inline rules stack on them
//! independently the way the Swift rules stack attributes.

use crate::media::{InlineMedia, Lookup, MediaInfo, MediaLookup};
use crate::textlayout::OBJECT;
use fumi_core::inline_link;
use fumi_core::markdown::*;
use fumi_core::wiki_link::{self, WikiLink};
use fumi_ui::theme::{CalloutKind, Tokens, t};
use gpui::*;
use regex::Regex;
use std::ops::Range;
use std::sync::OnceLock;

// Typography -----------------------------------------------------------------

pub const QUOTE_INDENT: f32 = 17.;
pub const PILL_PADDING: f32 = 4.;
pub const CALLOUT_TEXT_INDENT: f32 = 40.;
pub const CALLOUT_TAIL: f32 = 14.;
pub const CODE_TEXT_INDENT: f32 = 16.;
pub const LIST_SLOT: f32 = 24.;
pub const BLOCK_PADDING: f32 = 10.;
pub const BLOCK_SPACING: f32 = 14.;
pub const BLOCK_RADIUS: f32 = 8.;
const TITLE_SCALE: f32 = 1.875;
const HEADING_SCALES: [f32; 3] = [1.625, 1.3125, 1.125];

pub fn small_size(body: f32) -> f32 {
    12f32.max((body * 0.875).round())
}

/// Fonts and sizes from Settings › Writing. The port of `MarkdownStyle`.
#[derive(Clone, Debug, PartialEq)]
pub struct Typography {
    pub family: String,
    pub size: f32,
    /// A body line's fixed height: size × the line-height setting.
    pub line: f32,
    pub small: f32,
    pub composite: Option<Composite>,
}

/// A Latin-only body family set beside a matching CJK one.
#[derive(Clone, Debug, PartialEq)]
pub struct Composite {
    pub family: &'static str,
    pub scale: f32,
}

impl Typography {
    pub fn new(family: &str, size: f32, line_height: f32) -> Self {
        Typography {
            family: family.to_string(),
            size,
            line: (size * line_height).round(),
            small: small_size(size),
            composite: Composite::for_family(family),
        }
    }

    pub fn from_settings() -> Self {
        use fumi_ui::settings as s;
        // A family that's gone falls back to the system font.
        let family = s::editor_font();
        let family = if crate::fonts::is_available(&family) { family } else { "System".to_string() };
        Self::new(&family, s::font_size() as f32, s::line_height() as f32)
    }

    pub fn body(&self) -> Font {
        family_font(&self.family)
    }

    pub fn weighted(&self, weight: FontWeight) -> Font {
        let mut f = self.body();
        f.weight = weight;
        f
    }

    pub fn italic(&self, bold: bool) -> Font {
        let mut f = self.weighted(if bold { FontWeight::BOLD } else { FontWeight::NORMAL });
        f.style = FontStyle::Italic;
        f
    }

    pub fn heading_size(&self, level: u8, title: bool) -> (f32, f32, f32) {
        let scaled = |r: f32| (self.size * r).round();
        if title {
            return (scaled(TITLE_SCALE), 0., 10.);
        }
        match level {
            1 => (scaled(HEADING_SCALES[0]), 24., 8.),
            2 => (scaled(HEADING_SCALES[1]), 16., 8.),
            3 => (scaled(HEADING_SCALES[2]), 12., 4.),
            _ => (self.size, 8., 2.),
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::new("System", 16., 1.75)
    }
}

const CJK_FALLBACK: &str = "PingFang SC";
/// `NSFont.monospacedSystemFont`'s family. CoreText finds it by this name, the way
/// it finds New York by `.AppleSystemUIFontSerif`, and font-kit asks by family.
pub const SYSTEM_MONO: &str = ".AppleSystemUIFontMonospaced";

pub fn family_font(family: &str) -> Font {
    let name: String = match family {
        "System" | "" => ".SystemUIFont".into(),
        "SF Mono" => return mono_font(),
        // The system serif; not a family font lookups can find by name.
        "New York" => ".AppleSystemUIFontSerif".into(),
        other => other.into(),
    };
    let mut f = font(name);
    f.fallbacks = Some(FontFallbacks::from_fonts(vec![CJK_FALLBACK.into()]));
    f
}

pub fn mono_font() -> Font {
    let mut f = font(SYSTEM_MONO);
    f.fallbacks = Some(FontFallbacks::from_fonts(vec![CJK_FALLBACK.into()]));
    f
}

/// `NSFont.isFixedPitch`: the system's monospace, or a family that is one.
pub fn is_fixed_pitch(font: &Font) -> bool {
    match font.family.as_ref() {
        SYSTEM_MONO => true,
        ".SystemUIFont" | ".AppleSystemUIFontSerif" => false,
        family => crate::fonts::is_fixed_pitch(family),
    }
}

pub fn ui_font() -> Font {
    family_font("System")
}

const SERIF_HINTS: &[&str] = &[
    "new york", "times", "georgia", "charter", "palatino", "baskerville", "didot", "hoefler",
    "iowan", "garamond", "caslon", "minion", "athelas", "serif", "song", "ming", "宋", "slab",
    "literata", "lora", "merriweather", "playfair", "crimson", "spectral", "newsreader", "gelasio",
    "tinos", "bitter", "domine", "alegreya", "cormorant", "vollkorn", "arvo",
];

/// CJK partners for a serif family, most preferred first.
const SERIF_PARTNERS: &[&str] = &["Songti SC", "STSong", "Songti TC"];
/// …and for any other.
const SANS_PARTNERS: &[&str] = &["PingFang SC", "Hiragino Sans GB"];

impl Composite {
    pub fn for_family(family: &str) -> Option<Composite> {
        if matches!(family, "System" | "SF Mono" | "") {
            return None;
        }
        Self::pick(family, crate::fonts::covers_han(family), crate::fonts::is_installed)
    }

    /// System and SF Mono ride the native cascade and a family covering Han sets
    /// both scripts itself; any other pairs with the first partner installed.
    pub fn pick(family: &str, covers_han: bool, installed: impl Fn(&str) -> bool) -> Option<Composite> {
        if matches!(family, "System" | "SF Mono" | "") || covers_han {
            return None;
        }
        let name = family.to_lowercase();
        let partners = if SERIF_HINTS.iter().any(|h| name.contains(h)) { SERIF_PARTNERS } else { SANS_PARTNERS };
        let family = *partners.iter().find(|p| installed(p))?;
        Some(Composite { family, scale: 0.97 })
    }
}

// The model ------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BulletShape {
    Disc,
    Ring,
    Square,
}

/// What's drawn in a list item's indent. The syntax it stands for is hidden.
#[derive(Clone, PartialEq, Debug)]
pub enum ListGlyph {
    Bullet(BulletShape),
    Ordered(SharedString),
    Task { checked: bool },
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BoxDeco {
    pub fill: Hsla,
    pub first: bool,
    pub last: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Link {
    Wiki(String),
    Web(String),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Underline {
    pub color: Hsla,
    pub dashed: bool,
    /// How far under the baseline it's drawn.
    pub below: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fill {
    pub color: Hsla,
    pub pad: f32,
    pub inset: f32,
    pub radius: f32,
}

#[derive(Clone)]
pub struct Span {
    /// Bytes of the visible text.
    pub range: Range<usize>,
    pub font: Font,
    pub size: Pixels,
    pub color: Hsla,
    pub underline: Option<Underline>,
    pub strike: Option<Hsla>,
    pub fill: Option<Fill>,
    pub kern_after: Pixels,
    pub math: Option<InlineMath>,
    /// An image or video standing in the line.
    pub media: Option<std::rc::Rc<InlineMedia>>,
    /// Link text that isn't in the source: one box, one row, cut short at the
    /// column, one point wider than its text. `WikiLinkAttachment`.
    pub unbroken: bool,
}

#[derive(Clone)]
pub struct InlineMath {
    pub latex: String,
    /// Written `$$…$$` in the middle of a line: display size, on the line.
    pub display: bool,
    /// Typeset and filled in by the editor. What can't be typeset never gets
    /// here: its source stays on the line as text.
    pub rendered: Option<std::rc::Rc<crate::blocks::Formula>>,
}

/// A stretch of visible text and the source bytes behind it. A hidden marker
/// has no entry at all.
#[derive(Clone, Copy, Debug)]
pub struct MapSeg {
    pub disp: usize,
    pub src: usize,
    pub disp_len: usize,
    pub src_len: usize,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LineMetric {
    /// Every row exactly this tall, whatever fonts it holds.
    Fixed(f32),
    /// The font's own height plus this much spacing.
    Natural { spacing: f32 },
}

pub struct DisplayLine {
    pub text: SharedString,
    pub spans: Vec<Span>,
    pub map: Vec<MapSeg>,
    pub src_len: usize,
    pub font_size: Pixels,
    pub base_font: Font,
    pub metric: LineMetric,
    pub top: Pixels,
    pub bottom: Pixels,
    pub indent: Pixels,
    /// Room kept free on the right, inside a box.
    pub tail: Pixels,
    /// The page margin on each side; GPUI's `list` ignores horizontal padding.
    pub margin: Pixels,
    /// Leading syntax on the caret's line, in visible bytes. It hangs left of
    /// the text column, so showing it doesn't push the words sideways.
    pub hang: usize,
    pub list: Option<ListGlyph>,
    pub quote_depth: u8,
    pub rule: bool,
    pub boxed: Option<BoxDeco>,
    /// Icon type and color, drawn at the box's top-left corner.
    pub callout_icon: Option<(String, Hsla)>,
    /// The language of a code block, and how far above this line's text top the
    /// label's top sits. Drawn at the box's top right.
    pub code_language: Option<(SharedString, f32)>,
    pub links: Vec<(Range<usize>, Link)>,
    pub centered: bool,
    pub wrap: bool,
    /// Something on this line may change height once it has loaded.
    pub media_pending: bool,
    /// Misspelled words, in visible bytes. Filled in by the editor.
    pub misspelled: Vec<Range<usize>>,
}

impl DisplayLine {
    pub fn disp_to_src(&self, disp: usize) -> usize {
        let mut last_end = 0;
        for seg in &self.map {
            if disp < seg.disp {
                return seg.src.min(last_end.max(seg.src));
            }
            if disp <= seg.disp + seg.disp_len {
                return if seg.disp_len == seg.src_len {
                    seg.src + (disp - seg.disp)
                } else if disp == seg.disp {
                    seg.src
                } else {
                    seg.src + seg.src_len
                };
            }
            last_end = seg.src + seg.src_len;
        }
        // Past all visible text: the end of the visible content, not any
        // hidden closing marker, unless nothing is visible.
        if self.map.is_empty() { self.src_len } else { last_end }
    }

    pub fn src_to_disp(&self, src: usize) -> usize {
        let mut disp_end = 0;
        for seg in &self.map {
            if src < seg.src {
                return seg.disp;
            }
            if src <= seg.src + seg.src_len {
                return if seg.disp_len == seg.src_len {
                    seg.disp + (src - seg.src)
                } else if src == seg.src {
                    seg.disp
                } else {
                    seg.disp + seg.disp_len
                };
            }
            disp_end = seg.disp + seg.disp_len;
        }
        disp_end
    }

    pub fn link_at(&self, disp: usize) -> Option<&(Range<usize>, Link)> {
        self.links.iter().find(|(r, _)| r.start <= disp && disp < r.end)
    }
}

// Byte styles ----------------------------------------------------------------

const BOLD: u32 = 1 << 0;
const ITALIC: u32 = 1 << 1;
const STRIKE: u32 = 1 << 2;
const CODE: u32 = 1 << 3;
const LINK: u32 = 1 << 4;
const MISSING: u32 = 1 << 5;
const MARK: u32 = 1 << 6;
const UNDER: u32 = 1 << 7;
const TAG: u32 = 1 << 8;
const FAINT: u32 = 1 << 9;
const MARKER: u32 = 1 << 10;
const HIDDEN: u32 = 1 << 11;
const TERTIARY: u32 = 1 << 12;
const SYN_KEYWORD: u32 = 1 << 13;
const SYN_STRING: u32 = 1 << 14;
const SYN_NUMBER: u32 = 1 << 15;
const KIND_FADED: u32 = 1 << 16;
/// Inside a typeset `$math$`, replaced by a box.
const MATH: u32 = 1 << 17;
const RAW_MONO: u32 = 1 << 18;
const PILL_END: u32 = 1 << 19;
const PILL_BEFORE: u32 = 1 << 20;
const NO_INLINE: u32 = 1 << 21;
/// Syntax on the caret's line: at the body's size whatever the line's.
const BODY_SIZE: u32 = 1 << 22;

struct Styled<'a> {
    src: &'a str,
    flags: Vec<u32>,
}

impl Styled<'_> {
    fn set(&mut self, range: Range<usize>, flag: u32) {
        let end = range.end.min(self.flags.len());
        if range.start < end {
            for f in &mut self.flags[range.start..end] {
                *f |= flag;
            }
        }
    }

    fn clear(&mut self, range: Range<usize>, flag: u32) {
        let end = range.end.min(self.flags.len());
        if range.start < end {
            for f in &mut self.flags[range.start..end] {
                *f &= !flag;
            }
        }
    }

    fn any(&self, range: Range<usize>, flag: u32) -> bool {
        self.flags[range].iter().any(|f| f & flag != 0)
    }

    /// Reveals syntax on the caret's span; hides it elsewhere.
    fn marker(&mut self, range: Range<usize>, active: bool) {
        if active {
            self.set(range, MARKER);
        } else {
            self.set(range, HIDDEN);
        }
    }
}

// Inline rules. Each runs independently over the line, as the regexes do.

macro_rules! regex {
    ($name:ident, $pattern:expr) => {
        fn $name() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($pattern).unwrap())
        }
    };
}

regex!(bold_italic_re, r"\*{3}(.+?)\*{3}");
regex!(bold_re, r"\*{2}(.+?)\*{2}");
regex!(strike_re, r"~~(.+?)~~");
regex!(under_re, r"<u>(.+?)</u>");
regex!(code_re, r"`([^`]+)`");
regex!(keyword_re, r"\b(func|let|var|if|else|for|while|return|guard|import|class|struct|enum|protocol|extension|def|function|const|true|false|nil|null|None|True|False|self|this|in|case|switch|break|continue|try|catch|throw|throws|async|await|public|private|static|new|from|as|interface|type|extends|implements|package|go|fn|pub|mut|impl|use|where|yield|lambda|with|and|or|not|elif|end|do|then|fi|echo|export|select|insert|update|delete|create|table|into|values)\b");
regex!(number_re, r"\b\d+(?:\.\d+)?\b");
regex!(string_re, r#""(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'"#);
/// `//[^\n]*|/\*[\s\S]*?\*/`, a line at a time: the bare `/*` is one that doesn't
/// close on its own line.
regex!(slash_comment_re, r"//[^\n]*|/\*.*?\*/|/\*");
regex!(block_comment_re, r"(?s)//[^\n]*|/\*.*?\*/");
regex!(hash_comment_re, r"(?:^|\s)#[^\n]*");

/// `(?<!\*)\*(?!\s)([^*]+?)(?<!\s)\*(?!\*)`
fn italic_spans(s: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'*' || (i > 0 && b[i - 1] == b'*') {
            i += 1;
            continue;
        }
        let Some(next) = s[i + 1..].chars().next() else { break };
        if next.is_whitespace() || next == '*' {
            i += 1;
            continue;
        }
        let Some(close) = s[i + 1..].find('*').map(|p| p + i + 1) else { break };
        let before = s[..close].chars().next_back().unwrap_or(' ');
        if before.is_whitespace() || b.get(close + 1) == Some(&b'*') {
            i = close + 1;
            continue;
        }
        out.push((i..close + 1, i + 1..close));
        i = close + 1;
    }
    out
}

/// `==(?![\s=])(.+?)(?<!\s)==`
fn highlight_spans(s: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(open) = s[from..].find("==").map(|p| p + from) {
        let inner = open + 2;
        let Some(first) = s[inner..].chars().next() else { break };
        if first.is_whitespace() || first == '=' {
            from = open + 1;
            continue;
        }
        let mut search = inner + first.len_utf8();
        let mut found = None;
        while let Some(close) = s[search..].find("==").map(|p| p + search) {
            if !s[..close].chars().next_back().unwrap_or(' ').is_whitespace() {
                found = Some(close);
                break;
            }
            search = close + 1;
        }
        let Some(close) = found else { break };
        out.push((open..close + 2, inner..close));
        from = close + 2;
    }
    out
}

/// `(?<=^|[\s(\[])#[\p{L}][\p{L}\p{N}_/-]*`
fn hashtag_spans(s: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    for (i, c) in s.char_indices() {
        if c != '#' {
            continue;
        }
        let before_ok = s[..i]
            .chars()
            .next_back()
            .is_none_or(|p| p.is_whitespace() || p == '(' || p == '[');
        if !before_ok {
            continue;
        }
        let mut chars = s[i + 1..].char_indices();
        let Some((_, first)) = chars.next() else { continue };
        if !first.is_alphabetic() {
            continue;
        }
        let mut end = i + 1 + first.len_utf8();
        for (k, ch) in chars {
            if ch.is_alphanumeric() || matches!(ch, '_' | '/' | '-') {
                end = i + 1 + k + ch.len_utf8();
            } else {
                break;
            }
        }
        out.push(i..end);
    }
    out
}

/// `(?<![\w/(<])https?://[^\s<>\[\]()（）"'“”，。、；：！？]+`, less trailing punctuation.
fn bare_address_spans(s: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(p) = s[from..].find("http") {
        let at = from + p;
        from = at + 4;
        let rest = &s[at..];
        let scheme = if rest.starts_with("https://") {
            8
        } else if rest.starts_with("http://") {
            7
        } else {
            continue;
        };
        let before_ok = s[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '_' | '/' | '(' | '<')));
        if !before_ok {
            continue;
        }
        let tail = &rest[scheme..];
        let stop = tail
            .find(|c: char| c.is_whitespace() || "<>[]()（）\"'“”，。、；：！？".contains(c))
            .unwrap_or(tail.len());
        if stop == 0 {
            continue;
        }
        let mut end = at + scheme + stop;
        while end > at && ".,;:!?)".contains(s[..end].chars().next_back().unwrap_or(' ')) {
            end -= 1;
        }
        out.push(at..end);
        from = end.max(at + 1);
    }
    out
}

/// `$…$`, no spaces just inside the delimiters and no `$` inside. `$$` is not one.
pub fn inline_math_spans(s: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'$' || (i > 0 && b[i - 1] == b'\\') {
            i += 1;
            continue;
        }
        let Some(close) = s[i + 1..].find('$').map(|p| p + i + 1) else { break };
        let inner = &s[i + 1..close];
        let last = inner.chars().next_back();
        if inner.is_empty()
            || inner.starts_with(char::is_whitespace)
            || last.is_some_and(|c| c.is_whitespace() || c == '\\')
        {
            i += 1;
            continue;
        }
        out.push((i..close + 1, i + 1..close));
        i = close + 1;
    }
    out
}

/// `$$…$$` within a line, not after a backslash, at least one character inside.
pub fn display_math_spans(s: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = s[i..].find("$$").map(|p| p + i) {
        if p > 0 && s.as_bytes()[p - 1] == b'\\' {
            i = p + 2;
            continue;
        }
        let inner_start = p + 2;
        let Some(first) = s[inner_start..].chars().next() else { break };
        let Some(close) = s[inner_start + first.len_utf8()..].find("$$").map(|c| c + inner_start + first.len_utf8()) else {
            break;
        };
        out.push((p..close + 2, inner_start..close));
        i = close + 2;
    }
    out
}

pub fn is_web_address(address: &str) -> bool {
    let l = address.to_lowercase();
    l.starts_with("http://") || l.starts_with("https://") || l.starts_with("mailto:")
}

/// What a `[[link]]` shows: the text, whether its note exists, and where that
/// text already sits in the link's inner source, or `None` when it isn't there.
fn shown_wiki_text(
    link: &WikiLink,
    inner: &str,
    links: Option<&LinkContext>,
) -> (String, Option<Range<usize>>, bool) {
    let start = inner
        .find('|')
        .map(|p| p + 1)
        .or_else(|| inner.rfind('/').map(|p| p + 1))
        .unwrap_or(0);
    let trimmed = inner[start..].trim_matches(' ');
    let lead = inner[start..].len() - inner[start..].trim_start_matches(' ').len();
    let visible = start + lead..start + lead + trimmed.len();
    let entry = links.and_then(|l| l.index.resolve(&link.target, l.folder.as_deref()));
    let Some(entry) = entry else {
        return (trimmed.to_string(), Some(visible), false);
    };
    let text = link.display_text(Some(&entry.title));
    let same = trimmed == text;
    (text, same.then_some(visible), true)
}

/// What resolves links to notes.
pub struct LinkContext {
    pub index: std::rc::Rc<wiki_link::WikiLinkIndex>,
    pub folder: Option<std::path::PathBuf>,
}

fn replace_atom(atoms: &mut Vec<(Range<usize>, String, Option<Link>)>, range: Range<usize>, text: String, link: Option<Link>) {
    atoms.push((range, text, link));
}

/// An image or video the inline pass found in the line and can show.
struct MediaAtom {
    range: Range<usize>,
    info: MediaInfo,
    caption: String,
    /// The caret touches it: the source stays, the picture comes after.
    revealed: bool,
}

/// `![[file]]`, as `WysiwygEngine.embedPattern`.
fn embed_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"!\[\[([^\]\n]+)\]\]").unwrap())
}

/// What the inline pass needs for a line's media, and what it reports back.
#[derive(Default)]
struct MediaPass<'a> {
    lookup: Option<&'a dyn MediaLookup>,
    /// A selection doesn't open what it passes over.
    reveal: bool,
    found: Vec<MediaAtom>,
    pending: bool,
}

/// One image or embed in a line's source.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaSpan {
    pub range: Range<usize>,
    pub dest: String,
    pub embed: bool,
}

/// Every image and embed in `src` outside inline code, in order.
pub fn media_spans(src: &str) -> Vec<MediaSpan> {
    if !src.contains("![") {
        return Vec::new();
    }
    let code: Vec<Range<usize>> = if src.contains('`') {
        code_re().captures_iter(src).map(|m| m.get(0).unwrap().range()).collect()
    } else {
        Vec::new()
    };
    let in_code = |r: &Range<usize>| code.iter().any(|c| c.start <= r.start && r.start < c.end);
    let mut out = Vec::new();
    for m in inline_link::image_pattern().captures_iter(src) {
        let range = m.get(0).unwrap().range();
        if !in_code(&range) {
            let dest = src[inline_link::destination_range(&m)].to_string();
            out.push(MediaSpan { range, dest, embed: false });
        }
    }
    for m in embed_re().captures_iter(src) {
        let range = m.get(0).unwrap().range();
        if !in_code(&range) {
            out.push(MediaSpan { range, dest: m[1].to_string(), embed: true });
        }
    }
    out.sort_by_key(|m| m.range.start);
    out
}

/// The line's inline pass, over `range`, with the caret at `caret` if it's here.
#[allow(clippy::too_many_arguments)]
fn inline_pass(
    st: &mut Styled,
    range: Range<usize>,
    caret: Option<usize>,
    math_ok: bool,
    links_ctx: Option<&LinkContext>,
    links: &mut Vec<(Range<usize>, Link)>,
    atoms: &mut Vec<(Range<usize>, String, Option<Link>)>,
    latex: &mut Vec<(Range<usize>, String, bool)>,
    media: &mut MediaPass,
) {
    if range.is_empty() {
        return;
    }
    let src = st.src;
    let s = &src[range.clone()];
    let at = range.start;
    let active = |r: &Range<usize>| caret.is_some_and(|c| c >= r.start + at && c <= r.end + at);

    // Inline code first: nothing inside it is Markdown. (The Swift regexes run
    // on regardless and hide stray asterisks in code; that isn't kept.)
    let mut code_spans: Vec<Range<usize>> = Vec::new();
    if s.contains('`') {
        for m in code_re().captures_iter(s) {
            let full = m.get(0).unwrap().range();
            let inner = m.get(1).unwrap().range();
            code_spans.push(full.clone());
            let a = active(&full);
            st.marker(at + full.start..at + inner.start, a);
            st.marker(at + inner.end..at + full.end, a);
            st.set(at + inner.start..at + inner.end, CODE);
            st.set(at + inner.end - last_char_len(&s[inner.clone()])..at + inner.end, PILL_END);
            // Room before the pill: on the character before the span's content,
            // which is the visible opening marker or the character before that.
            if full.start > 0 {
                let prev = at + full.start - prev_char_len(s, full.start);
                st.set(prev..at + full.start, PILL_BEFORE);
            }
        }
    }
    let in_code = |r: &Range<usize>| code_spans.iter().any(|c| c.start <= r.start && r.start < c.end);

    let mut images: Vec<Range<usize>> = Vec::new();
    if s.contains("![") {
        for m in inline_link::image_pattern().captures_iter(s) {
            let full = m.get(0).unwrap().range();
            if in_code(&full) {
                continue;
            }
            images.push(full.clone());
            let dest = &s[inline_link::destination_range(&m)];
            let revealed = media.reveal && active(&full);
            match media.lookup.map_or(Lookup::Missing, |l| l.lookup(dest, false)) {
                Lookup::Ready(info) => {
                    media.pending |= info.settling;
                    let caption = m.get(1).map_or("", |a| a.as_str()).to_string();
                    media.found.push(MediaAtom { range: at + full.start..at + full.end, info, caption, revealed });
                    if !revealed {
                        continue;
                    }
                }
                Lookup::Pending => media.pending = true,
                Lookup::Missing => {}
            }
            st.set(at + full.start..at + full.end, FAINT);
            if let Some(alt) = m.get(1)
                && !alt.is_empty()
            {
                st.clear(at + alt.start()..at + alt.end(), FAINT);
                st.set(at + alt.start()..at + alt.end(), TERTIARY);
            }
        }
    }
    if s.contains("![[") {
        for m in embed_re().captures_iter(s) {
            let full = m.get(0).unwrap().range();
            if in_code(&full) {
                continue;
            }
            let revealed = media.reveal && active(&full);
            match media.lookup.map_or(Lookup::Missing, |l| l.lookup(&m[1], true)) {
                Lookup::Ready(info) => {
                    media.pending |= info.settling;
                    images.push(full.clone());
                    media.found.push(MediaAtom {
                        range: at + full.start..at + full.end,
                        info,
                        caption: String::new(),
                        revealed,
                    });
                    if revealed {
                        st.set(at + full.start..at + full.end, FAINT);
                    }
                }
                Lookup::Pending => media.pending = true,
                Lookup::Missing => {}
            }
        }
    }
    media.found.sort_by_key(|a| a.range.start);
    let skip = |r: &Range<usize>| in_code(r) || images.iter().any(|c| c.start <= r.start && r.start < c.end);

    let mut linked: Vec<Range<usize>> = images.clone();
    if s.contains("](") {
        for m in inline_link::link_pattern().captures_iter(s) {
            let full = m.get(0).unwrap().range();
            if skip(&full) {
                continue;
            }
            linked.push(full.clone());
            let text = m.get(1).unwrap().range();
            let a = active(&full);
            st.marker(at + full.start..at + text.start, a);
            st.marker(at + text.end..at + full.end, a);
            st.set(at + text.start..at + text.end, LINK);
            let address = &s[inline_link::destination_range(&m)];
            if !text.is_empty() && is_web_address(address) {
                links.push((at + text.start..at + text.end, Link::Web(address.to_string())));
            }
        }
    }

    let wrapped = |st: &mut Styled, re: &Regex, n: usize, flag: u32| {
        for m in re.captures_iter(s) {
            let full = m.get(0).unwrap().range();
            if skip(&full) {
                continue;
            }
            let a = active(&full);
            st.marker(at + full.start..at + full.start + n, a);
            st.marker(at + full.end - n..at + full.end, a);
            let inner = m.get(1).unwrap().range();
            st.set(at + inner.start..at + inner.end, flag);
        }
    };
    if s.contains('*') {
        wrapped(st, bold_italic_re(), 3, BOLD | ITALIC);
        wrapped(st, bold_re(), 2, BOLD);
        for (full, inner) in italic_spans(s) {
            if skip(&full) {
                continue;
            }
            let a = active(&full);
            st.marker(at + full.start..at + inner.start, a);
            st.marker(at + inner.end..at + full.end, a);
            st.set(at + inner.start..at + inner.end, ITALIC);
        }
    }
    if s.contains("~~") {
        wrapped(st, strike_re(), 2, STRIKE);
    }
    if s.contains("==") {
        for (full, inner) in highlight_spans(s) {
            if skip(&full) {
                continue;
            }
            let a = active(&full);
            st.marker(at + full.start..at + inner.start, a);
            st.marker(at + inner.end..at + full.end, a);
            st.set(at + inner.start..at + inner.end, MARK);
        }
    }
    if s.contains("<u>") {
        for m in under_re().captures_iter(s) {
            let full = m.get(0).unwrap().range();
            if skip(&full) {
                continue;
            }
            let a = active(&full);
            let inner = m.get(1).unwrap().range();
            st.marker(at + full.start..at + inner.start, a);
            st.marker(at + inner.end..at + full.end, a);
            st.set(at + inner.start..at + inner.end, UNDER);
        }
    }
    if s.contains('#') {
        for tag in hashtag_spans(s) {
            if skip(&tag) {
                continue;
            }
            st.set(at + tag.start..at + tag.end, TAG);
            st.set(at + tag.start..at + tag.start + 1, FAINT);
            let end = at + tag.end;
            let last = last_char_len(&s[tag.clone()]);
            st.set(end - last..end, PILL_END);
            if tag.start > 0 && s.as_bytes()[tag.start - 1] == b' ' {
                st.set(at + tag.start - 1..at + tag.start, PILL_BEFORE);
            }
        }
    }

    if s.contains("[[") {
        for m in wiki_link::find_all(s) {
            let full = m.range.clone();
            if skip(&full) {
                continue;
            }
            let inner = m.inner.clone();
            let inner_text = &s[inner.clone()];
            let Some(link) = WikiLink::parse(inner_text) else { continue };
            // `[[#Heading]]` and `[[a#b]]` hold no tags.
            for flag in [TAG, FAINT, PILL_END] {
                st.clear(at + full.start..at + full.end, flag);
            }
            if active(&full) {
                st.set(at + full.start..at + full.start + 2, MARKER);
                st.set(at + full.end - 2..at + full.end, MARKER);
                continue;
            }
            let (text, in_text, exists) = shown_wiki_text(&link, inner_text, links_ctx);
            match in_text {
                Some(vis) if !vis.is_empty() => {
                    let visible = at + inner.start + vis.start..at + inner.start + vis.end;
                    st.set(at + full.start..visible.start, HIDDEN);
                    st.set(visible.end..at + full.end, HIDDEN);
                    st.set(visible.clone(), if exists { LINK } else { MISSING });
                    links.push((visible, Link::Wiki(inner_text.to_string())));
                }
                _ => replace_atom(
                    atoms,
                    at + full.start..at + full.end,
                    text,
                    Some(Link::Wiki(inner_text.to_string())),
                ),
            }
        }
    }

    if s.contains("http") {
        for addr in bare_address_spans(s) {
            let overlap = linked.iter().any(|l| l.start < addr.end && addr.start < l.end);
            if overlap || in_code(&addr) {
                continue;
            }
            st.set(at + addr.start..at + addr.end, LINK);
            links.push((at + addr.start..at + addr.end, Link::Web(s[addr].to_string())));
        }
    }

    if math_ok && s.contains('$') {
        // Display first, so `$$` isn't read as two empty inline spans.
        let display = display_math_spans(s);
        let mut masked = s.to_string();
        for (full, _) in &display {
            masked.replace_range(full.clone(), &" ".repeat(full.len()));
        }
        let inline = inline_math_spans(&masked);
        let spans = display.into_iter().map(|(full, inner)| (full, inner, true)).chain(inline.into_iter().map(|(full, inner)| (full, inner, false)));
        for (full, inner, is_display) in spans {
            if skip(&full) {
                continue;
            }
            let latex_text = s[inner].trim();
            // On the caret, or what can't be typeset: its source, as text.
            if active(&full) || !crate::blocks::can_typeset(latex_text) {
                st.set(at + full.start..at + full.end, RAW_MONO);
            } else {
                st.set(at + full.start..at + full.end, MATH);
                latex.push((at + full.start..at + full.end, latex_text.to_string(), is_display));
            }
        }
    }
}

fn last_char_len(s: &str) -> usize {
    s.chars().next_back().map_or(0, char::len_utf8)
}

fn prev_char_len(s: &str, at: usize) -> usize {
    s[..at].chars().next_back().map_or(0, char::len_utf8)
}

const HASH_COMMENT_LANGUAGES: [&str; 15] = [
    "python", "py", "sh", "bash", "zsh", "shell", "yaml", "yml", "ruby", "rb", "toml", "r", "perl", "makefile",
    "dockerfile",
];

fn has_hash_comments(language: &str) -> bool {
    HASH_COMMENT_LANGUAGES.contains(&language.to_lowercase().as_str())
}

/// For each line of a code block's body, whether it starts inside a `/* … */`.
/// One that never closes isn't a comment, as the Swift pattern has it.
pub fn comment_carries(body: &str, language: &str) -> Vec<bool> {
    let lines = body.bytes().filter(|b| *b == b'\n').count() + 1;
    let mut carries = vec![false; lines];
    if has_hash_comments(language) || !body.contains("/*") {
        return carries;
    }
    let mut line = 0;
    let mut at = 0;
    for m in block_comment_re().find_iter(body) {
        line += body[at..m.start()].bytes().filter(|b| *b == b'\n').count();
        let spans = m.as_str().bytes().filter(|b| *b == b'\n').count();
        for carry in &mut carries[line + 1..line + 1 + spans] {
            *carry = true;
        }
        line += spans;
        at = m.end();
    }
    carries
}

/// `LineCtx::comment_carry` for line `at` of a note, a code block's body line:
/// the body around it is read through `kind` and `text`, by line number.
pub fn comment_carry_at<T: AsRef<str>>(
    at: usize,
    count: usize,
    language: &str,
    kind: impl Fn(usize) -> Kind,
    text: impl Fn(usize) -> T,
) -> (bool, bool) {
    let open = (0..at).rev().find(|l| kind(*l) == Kind::FenceOpen).unwrap_or(0);
    let mut end = at;
    while end + 1 < count && kind(end + 1) == Kind::FenceBody {
        end += 1;
    }
    let body: Vec<T> = (open + 1..=end).map(&text).collect();
    if !body.iter().any(|l| l.as_ref().contains("/*")) {
        return (false, false);
    }
    let joined = body.iter().map(AsRef::as_ref).collect::<Vec<&str>>().join("\n");
    let carries = comment_carries(&joined, language);
    let i = at - open - 1;
    (carries.get(i).copied().unwrap_or(false), carries.get(i + 1).copied().unwrap_or(false))
}

/// What's comment on one line of a block that isn't a hash-comment language,
/// given whether it starts inside a `/* … */` and whether one runs on past it.
fn slash_comments(s: &str, carry_in: bool, carry_out: bool) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    if carry_in {
        match s.find("*/") {
            Some(close) => {
                out.push(0..close + 2);
                at = close + 2;
            }
            None => return vec![0..s.len()],
        }
    }
    while let Some(m) = slash_comment_re().find_at(s, at) {
        if m.as_str() == "/*" {
            if carry_out {
                out.push(m.start()..s.len());
                break;
            }
            // Never closes: not a comment, and what follows it may still be.
            at = m.start() + 1;
            continue;
        }
        out.push(m.range());
        at = m.end();
    }
    out
}

/// Four quiet colors only: keywords, strings, numbers, comments.
fn highlight_code(st: &mut Styled, language: &str, carry: (bool, bool)) {
    let s = st.src;
    for m in keyword_re().find_iter(s) {
        st.set(m.range(), SYN_KEYWORD);
    }
    for m in number_re().find_iter(s) {
        st.clear(m.range(), SYN_KEYWORD);
        st.set(m.range(), SYN_NUMBER);
    }
    for m in string_re().find_iter(s) {
        st.clear(m.range(), SYN_KEYWORD | SYN_NUMBER);
        st.set(m.range(), SYN_STRING);
    }
    if has_hash_comments(language) {
        if let Some(m) = hash_comment_re().find(s) {
            st.clear(m.range(), SYN_KEYWORD | SYN_NUMBER | SYN_STRING);
            st.set(m.range(), FAINT);
        }
        return;
    }
    for range in slash_comments(s, carry.0, carry.1) {
        st.clear(range.clone(), SYN_KEYWORD | SYN_NUMBER | SYN_STRING);
        st.set(range, FAINT);
    }
}

// Lines ----------------------------------------------------------------------

pub struct LineCtx<'a> {
    pub kind: Kind,
    /// The caret's byte in this line, if it's here.
    pub caret: Option<usize>,
    pub title: bool,
    pub marked: Option<Range<usize>>,
    pub prev: Kind,
    pub next: Kind,
    /// For a code line: the block's language. For a `> [!x]` body: the type.
    pub block_type: &'a str,
    pub typo: &'a Typography,
    pub links: Option<&'a LinkContext>,
    pub real: bool,
    /// For a table or `$$` line opened for editing.
    pub raw_block: bool,
    pub prev_has_caret: bool,
    /// Finds what `![…]` points at. Without it images stay source.
    pub media: Option<&'a dyn MediaLookup>,
    /// Whether the caret on this line opens the media it touches.
    pub reveal_media: bool,
    /// Nothing before or after this line: a box here has no neighbour to space.
    pub first: bool,
    pub last: bool,
    /// For a code line: whether it starts inside a `/* … */`, and whether one
    /// runs on past its end.
    pub comment_carry: (bool, bool),
}

/// `paragraphSpacing` a line of this kind has on its own, for block spacing.
fn own_after(kind: Kind, typo: &Typography, title: bool) -> f32 {
    match kind {
        Kind::Heading(level) => typo.heading_size(level, title).2,
        Kind::List { .. } | Kind::Rule => 4.,
        _ => 0.,
    }
}

fn own_before(kind: Kind, typo: &Typography) -> f32 {
    match kind {
        Kind::Heading(level) => typo.heading_size(level, false).1,
        Kind::Rule => 4.,
        _ => 0.,
    }
}

/// `^\s*((?:>\s?)+)`: the prefix's length and the quote's depth.
pub fn quote_prefix(line: &str) -> (usize, u8) {
    let b = line.as_bytes();
    let mut i = b.iter().take_while(|c| c.is_ascii_whitespace()).count();
    let mut depth = 0;
    while b.get(i) == Some(&b'>') {
        depth += 1;
        i += 1;
        if b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
    }
    (i, depth)
}

/// `^\s*(?:>\s*)+\[!([^\]\s]+)\][+-]?[ \t]*`: the header's end and the type's range.
pub fn callout_header(line: &str) -> Option<(usize, Range<usize>)> {
    let content = quote_content(line);
    let at = line.len() - content.len();
    let name = callout_type(content)?;
    let word = at + 2..at + 2 + name.len();
    let mut end = word.end + 1;
    if matches!(line.as_bytes().get(end), Some(b'+' | b'-')) {
        end += 1;
    }
    end += line[end..].bytes().take_while(|b| matches!(*b, b' ' | b'\t')).count();
    Some((end, word))
}

/// `1 → a`, `26 → z`, `27 → aa`.
pub fn letter_label(number: u64) -> String {
    let mut n = number.max(1);
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(b'a' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// What a `::: type title` opening line hides: through the type when a title
/// follows, or just the `:::` so the type itself reads as the title.
fn callout_open_marker(line: &str) -> Range<usize> {
    let lead = line.len() - line.trim_start().len();
    let after = lead + 3;
    let rest = &line[after..];
    let gap = rest.len() - rest.trim_start().len();
    let word_at = after + gap;
    let word_len = line[word_at..].find(char::is_whitespace).unwrap_or(line.len() - word_at);
    let after_word = word_at + word_len;
    let title_at = after_word + (line[after_word..].len() - line[after_word..].trim_start().len());
    if word_len > 0 && title_at < line.len() {
        0..title_at
    } else {
        0..word_at
    }
}

struct Base {
    font: Font,
    size: f32,
    color: Hsla,
}

pub fn build_line(src: &str, cx: &LineCtx) -> DisplayLine {
    let c = t();
    let typo = cx.typo;
    let caret_here = cx.caret.is_some();
    let mut st = Styled { src, flags: vec![0; src.len()] };
    let mut base = Base { font: typo.body(), size: typo.size, color: c.text };
    let mut metric = LineMetric::Fixed(typo.line);
    let (mut top, mut bottom) = (0f32, 0f32);
    let (mut indent, mut tail) = (0f32, 0f32);
    let mut hang = 0;
    let mut list = None;
    let mut quote_depth = 0;
    let mut rule = false;
    let mut boxed = None;
    let mut callout_icon = None;
    let mut code_language = None;
    let mut links = Vec::new();
    let mut atoms: Vec<(Range<usize>, String, Option<Link>)> = Vec::new();
    let mut latex: Vec<(Range<usize>, String, bool)> = Vec::new();
    let mut inline_from = 0;
    let mut inline = true;
    let mut math_ok = true;
    let mut wrap = true;
    let mut title_override: Option<(Range<usize>, String)> = None;
    let mut media = MediaPass { lookup: cx.media, reveal: cx.reveal_media, ..Default::default() };

    let callout_box = |raw: &str, first: bool, last: bool| BoxDeco {
        fill: CalloutKind::of(raw).background(c.dark, c.glass),
        first,
        last,
    };

    match cx.kind {
        Kind::Heading(level) => {
            let (size, before, after) = typo.heading_size(level, cx.title);
            let spaces = src.bytes().take_while(|b| *b == b' ').count();
            let hashes = src[spaces..].bytes().take_while(|b| *b == b'#').count();
            let gap = src[spaces + hashes..].bytes().take_while(|b| matches!(*b, b' ' | b'\t')).count();
            let marker = spaces + hashes + gap;
            base.size = size;
            base.font = typo.weighted(FontWeight::SEMIBOLD);
            if level >= 6 {
                base.color = c.text3;
            }
            metric = LineMetric::Fixed((size * 1.3).round().max(typo.line));
            top = before;
            bottom = after;
            // Syntax shows inline on the caret's line only, and pushes the title.
            st.marker(0..marker, caret_here);
            inline_from = marker;
        }
        Kind::List { indent: leading, marker } => {
            let leading = leading as usize;
            let marker = marker as usize;
            let columns: usize = src[..leading].chars().map(|ch| if ch == '\t' { 4 } else { 1 }).sum();
            let level = (columns / 2).min(6);
            indent = LIST_SLOT * (level + 1) as f32;
            let token = src[leading..leading + marker].trim_end();
            let tail_text = &src[leading + marker..];
            let boxed_task = ["[ ]", "[x]", "[X]"].iter().find(|b| tail_text.starts_with(**b));
            let mut prefix = leading + marker;
            let checked = boxed_task.is_some_and(|b| *b != "[ ]");
            if let Some(b) = boxed_task {
                prefix += b.len();
                prefix += src[prefix..].bytes().take_while(|x| matches!(*x, b' ' | b'\t')).count();
            }
            bottom = 4.;
            if caret_here {
                // The raw marker replaces the drawn one, hung in the slot.
                st.set(leading..prefix, MARKER);
                st.set(0..leading, HIDDEN);
                hang = prefix - leading;
            } else {
                st.set(0..prefix, HIDDEN);
                list = Some(if boxed_task.is_some() {
                    ListGlyph::Task { checked }
                } else if token.as_bytes()[0].is_ascii_digit() {
                    let digits = &token[..token.len() - 1];
                    let label = if level % 2 == 1 {
                        format!("{}.", letter_label(digits.parse().unwrap_or(1)))
                    } else {
                        token.to_string()
                    };
                    ListGlyph::Ordered(label.into())
                } else {
                    ListGlyph::Bullet(match level % 3 {
                        0 => BulletShape::Disc,
                        1 => BulletShape::Ring,
                        _ => BulletShape::Square,
                    })
                });
            }
            if checked && prefix < src.len() {
                st.set(prefix..src.len(), STRIKE);
            }
            inline_from = prefix;
        }
        Kind::Quote => {
            let (prefix, depth) = quote_prefix(src);
            indent = QUOTE_INDENT * depth as f32;
            quote_depth = depth;
            if caret_here {
                st.set(0..prefix, MARKER | BODY_SIZE);
                hang = prefix;
            } else {
                st.set(0..prefix, HIDDEN);
            }
            let content = &src[prefix..];
            if content.starts_with("— ") || content.starts_with("——") || content.starts_with("-- ") {
                base.size = typo.small;
                base.color = c.text3;
            }
            inline_from = prefix;
        }
        Kind::QuoteCalloutTitle => {
            let kind = CalloutKind::of(cx.block_type);
            base.font = ui_semibold();
            base.size = typo.small;
            base.color = kind.color(c.dark);
            indent = CALLOUT_TEXT_INDENT;
            tail = CALLOUT_TAIL;
            boxed = Some(callout_box(cx.block_type, true, cx.next != Kind::QuoteCalloutBody));
            callout_icon = Some((cx.block_type.to_string(), kind.color(c.dark)));
            if let Some((end, word)) = callout_header(src) {
                if caret_here {
                    st.set(0..end, KIND_FADED);
                } else if src[end..].trim().is_empty() {
                    st.set(0..src.len(), HIDDEN);
                    title_override = Some((word, CalloutKind::title(cx.block_type)));
                } else {
                    st.set(0..end, HIDDEN);
                }
                inline_from = end;
            }
        }
        Kind::QuoteCalloutBody | Kind::EmojiQuote => {
            let (prefix, _) = quote_prefix(src);
            let emoji = cx.kind == Kind::EmojiQuote;
            indent = if emoji { 14. } else { CALLOUT_TEXT_INDENT };
            tail = CALLOUT_TAIL;
            let raw = if emoji { CalloutKind::EMOJI_QUOTE } else { cx.block_type };
            let next_same = if emoji { cx.next == Kind::EmojiQuote } else { cx.next == Kind::QuoteCalloutBody };
            let first = emoji && cx.prev != Kind::EmojiQuote;
            boxed = Some(callout_box(raw, first, !next_same));
            if caret_here {
                st.set(0..prefix, MARKER);
                hang = prefix;
            } else {
                st.set(0..prefix, HIDDEN);
            }
            inline_from = prefix;
        }
        Kind::CalloutOpen => {
            let kind = CalloutKind::of(cx.block_type);
            base.font = ui_semibold();
            base.size = typo.small;
            base.color = kind.color(c.dark);
            indent = CALLOUT_TEXT_INDENT;
            tail = CALLOUT_TAIL;
            // The icon whether the block ever closes or not; the box only if it does.
            callout_icon = Some((cx.block_type.to_string(), kind.color(c.dark)));
            if cx.real {
                boxed = Some(callout_box(cx.block_type, true, false));
            }
            let marker = callout_open_marker(src);
            if caret_here {
                st.set(marker.clone(), KIND_FADED);
            } else {
                st.set(marker.clone(), HIDDEN);
            }
            inline_from = marker.end;
        }
        Kind::CalloutBody => {
            indent = CALLOUT_TEXT_INDENT;
            tail = CALLOUT_TAIL;
            if cx.real {
                boxed = Some(callout_box(cx.block_type, false, false));
            }
        }
        Kind::CalloutClose => {
            indent = CALLOUT_TEXT_INDENT;
            tail = CALLOUT_TAIL;
            boxed = Some(callout_box(cx.block_type, false, true));
            st.marker(0..src.len(), caret_here);
            inline = false;
        }
        Kind::Rule => {
            metric = LineMetric::Fixed(typo.line);
            top = 4.;
            bottom = 4.;
            if caret_here {
                st.set(0..src.len(), MARKER);
            } else {
                st.set(0..src.len(), HIDDEN);
                rule = true;
            }
            inline = false;
        }
        Kind::FenceOpen | Kind::FenceBody | Kind::FenceClose => {
            base.font = mono_font();
            base.size = typo.small;
            metric = LineMetric::Natural { spacing: 6. };
            indent = CODE_TEXT_INDENT;
            tail = CODE_TEXT_INDENT;
            inline = false;
            math_ok = false;
            wrap = true;
            boxed = Some(BoxDeco {
                fill: c.code_block,
                first: cx.kind == Kind::FenceOpen,
                last: cx.kind == Kind::FenceClose,
            });
            // The label reaches below the collapsed opening fence, and each line
            // paints over the one before, so the line after it draws the label.
            if cx.kind != Kind::FenceOpen && cx.prev == Kind::FenceOpen && !cx.prev_has_caret && !cx.block_type.is_empty() {
                code_language = Some((cx.block_type.to_string().into(), BLOCK_PADDING - 7. + 6. + 0.));
            }
            match cx.kind {
                Kind::FenceBody => highlight_code(&mut st, cx.block_type, cx.comment_carry),
                _ => {
                    st.marker(0..src.len(), caret_here);
                    if !caret_here {
                        // Collapsed syntax leaves only the line's spacing.
                        metric = LineMetric::Fixed(6.);
                    }
                    if !cx.block_type.is_empty() && cx.kind == Kind::FenceOpen && caret_here {
                        code_language = Some((cx.block_type.to_string().into(), 3.));
                    }
                }
            }
        }
        Kind::FrontOpen | Kind::FrontBody | Kind::FrontClose => {
            base.font = mono_font();
            base.size = typo.small;
            base.color = c.text3;
            metric = LineMetric::Natural { spacing: 4. };
            if cx.kind == Kind::FrontClose {
                bottom = 16.;
            }
            inline = false;
            math_ok = false;
        }
        Kind::MathOpen | Kind::MathBody | Kind::MathClose | Kind::MathOne => {
            base.font = mono_font();
            base.size = typo.small;
            inline = false;
            math_ok = false;
        }
        Kind::TableHead | Kind::TableDelim | Kind::TableRow => {
            // Being edited: the raw source, monospaced so the pipes line up.
            base.font = mono_font();
            base.size = typo.small;
            inline = false;
            math_ok = false;
        }
        Kind::Blank => {}
        Kind::Para | Kind::Image => {
            // Markdown ignores up to three spaces before a paragraph; so does the
            // page, on the caret's line too, so the text never shifts.
            let spaces = src.bytes().take_while(|b| *b == b' ').count();
            if (1..=3).contains(&spaces) {
                st.set(0..spaces, HIDDEN);
                inline_from = spaces;
            }
        }
    }

    // Block spacing: 14 between a box and what's around it, by raising the
    // neighbours' own paragraph spacing, so it's added here once, on the box's
    // side. The box's padding reaches into it. At the note's ends there's no
    // neighbour to raise.
    if boxed.is_some_and(|b| b.first) && !cx.first {
        top += (BLOCK_SPACING - own_after(cx.prev, typo, false)).max(0.);
    }
    if boxed.is_some_and(|b| b.last) && !cx.last {
        bottom += (BLOCK_SPACING - own_before(cx.next, typo)).max(0.);
    }

    if inline {
        inline_pass(
            &mut st,
            inline_from..src.len(),
            cx.caret,
            math_ok,
            cx.links,
            &mut links,
            &mut atoms,
            &mut latex,
            &mut media,
        );
    }

    // A paragraph takes its style from its first character, so a picture opening
    // the line sets the spacing for all of it; one further along leaves the
    // line's style alone. One whose source shows sets it however far along it is.
    if matches!(cx.kind, Kind::Para | Kind::Image)
        && let Some(first) = media.found.first()
    {
        let opens = src[..first.range.start].trim().is_empty();
        let shown = media.found.iter().any(|a| a.revealed);
        if shown || opens {
            metric = LineMetric::Natural { spacing: if shown { 10. } else { 4. } };
            top = top.max(8.);
            bottom = bottom.max(8.);
        }
    }

    let mut b = Assembler::new(src, &c, typo, &base, cx.kind);
    b.run(&st, &atoms, &latex, title_override.as_ref(), &media.found);
    let (text, mut spans, map, display_links) = (b.text, b.spans, b.map, b.links);

    // Links, in source bytes so far, move to visible bytes.
    let mut vis_links: Vec<(Range<usize>, Link)> = display_links;
    for (range, link) in links {
        let a = src_to_disp_in(&map, range.start);
        let e = src_to_disp_in(&map, range.end);
        if a < e {
            vis_links.push((a..e, link));
        }
    }
    vis_links.sort_by_key(|l| l.0.start);

    if let Some(marked) = &cx.marked {
        let a = src_to_disp_in(&map, marked.start);
        let e = src_to_disp_in(&map, marked.end);
        spans = underline_marked(spans, a..e, c.text);
    }

    DisplayLine {
        text: text.into(),
        spans,
        map,
        src_len: src.len(),
        font_size: px(base.size),
        base_font: base.font,
        metric,
        top: px(top),
        bottom: px(bottom),
        indent: px(indent),
        tail: px(tail),
        margin: px(0.),
        hang,
        list,
        quote_depth,
        rule,
        boxed,
        callout_icon,
        code_language,
        links: vis_links,
        centered: false,
        wrap,
        media_pending: media.pending,
        misspelled: Vec::new(),
    }
}

pub fn ui_semibold() -> Font {
    let mut f = ui_font();
    f.weight = FontWeight::SEMIBOLD;
    f
}

fn src_to_disp_in(map: &[MapSeg], src: usize) -> usize {
    let mut end = 0;
    for seg in map {
        if src < seg.src {
            return seg.disp;
        }
        if src <= seg.src + seg.src_len {
            return if seg.disp_len == seg.src_len {
                seg.disp + (src - seg.src)
            } else if src == seg.src {
                seg.disp
            } else {
                seg.disp + seg.disp_len
            };
        }
        end = seg.disp + seg.disp_len;
    }
    end
}

/// Splits spans to underline the composing text an input method has marked.
fn underline_marked(spans: Vec<Span>, marked: Range<usize>, color: Hsla) -> Vec<Span> {
    if marked.is_empty() {
        return spans;
    }
    let mut out = Vec::with_capacity(spans.len() + 2);
    for span in spans {
        let cuts = [
            span.range.start,
            marked.start.clamp(span.range.start, span.range.end),
            marked.end.clamp(span.range.start, span.range.end),
            span.range.end,
        ];
        for w in cuts.windows(2) {
            if w[0] < w[1] {
                let mut piece = span.clone();
                piece.range = w[0]..w[1];
                if w[1] < span.range.end {
                    piece.kern_after = px(0.);
                }
                if w[0] >= marked.start && w[1] <= marked.end {
                    piece.underline = Some(Underline { color, dashed: false, below: 2. });
                }
                out.push(piece);
            }
        }
    }
    out
}

// Bytes to spans -------------------------------------------------------------

struct Assembler<'a> {
    src: &'a str,
    c: &'a Tokens,
    typo: &'a Typography,
    base: &'a Base,
    kind: Kind,
    text: String,
    spans: Vec<Span>,
    map: Vec<MapSeg>,
    links: Vec<(Range<usize>, Link)>,
    /// The last character that could take a face, for punctuation to go by.
    prev_composite: Option<char>,
    /// The last character the Han/Latin gap goes by, and the span it ended up in.
    /// Monospaced runs are skipped over: it carries across them.
    gap_prev: Option<(Script, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Script {
    Han,
    Latin,
    Other,
}

fn script_of(c: char) -> Script {
    match c as u32 {
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF => Script::Han,
        0x30..=0x39 | 0x41..=0x5A | 0x61..=0x7A => Script::Latin,
        _ => Script::Other,
    }
}

/// What `CompositeFont.category` sorts a character into.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FaceClaim {
    /// Han, kana, hangul, fullwidth forms, CJK punctuation: the paired face.
    Cjk,
    /// Quotes, dashes, currency: go with a neighbour.
    Punctuation,
    Other,
}

pub fn face_claim(c: char) -> FaceClaim {
    match c as u32 {
        0x2E80..=0x2EFF | 0x2F00..=0x2FDF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x3100..=0x312F
        | 0x31A0..=0x31BF | 0x3130..=0x318F | 0x31F0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF
        | 0xA960..=0xA97F | 0xAC00..=0xD7AF | 0xD7B0..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F
        | 0xFF01..=0xFFEE | 0x20000..=0x2FA1F | 0x30000..=0x3134F => FaceClaim::Cjk,
        0x2010..=0x2027 | 0x2030..=0x205E | 0x2070..=0x209F | 0x20A0..=0x20CF => FaceClaim::Punctuation,
        _ => FaceClaim::Other,
    }
}

/// Whether `c` is set in the paired face. A quote belongs to what it quotes, so
/// an opening one follows the character after it, whatever precedes; closing
/// quotes, dashes and the rest follow the character before them.
pub fn takes_cjk_face(c: char, previous: Option<char>, next: impl FnOnce() -> Option<char>) -> bool {
    match face_claim(c) {
        FaceClaim::Cjk => true,
        FaceClaim::Punctuation => {
            let neighbour = if matches!(c, '\u{2018}' | '\u{201C}') { next() } else { previous };
            neighbour.is_some_and(|n| face_claim(n) == FaceClaim::Cjk)
        }
        FaceClaim::Other => false,
    }
}

impl Assembler<'_> {
    fn new<'a>(src: &'a str, c: &'a Tokens, typo: &'a Typography, base: &'a Base, kind: Kind) -> Assembler<'a> {
        Assembler {
            src,
            c,
            typo,
            base,
            kind,
            text: String::with_capacity(src.len()),
            spans: Vec::with_capacity(8),
            map: Vec::with_capacity(8),
            links: Vec::new(),
            prev_composite: None,
            gap_prev: None,
        }
    }

    fn span_for(&self, flags: u32) -> Span {
        let (c, typo, base) = (self.c, self.typo, self.base);
        let mut font = base.font.clone();
        let mut size = base.size;
        let mut color = base.color;
        let mut underline = None;
        let mut strike = None;
        let mut fill = None;
        if flags & BOLD != 0 {
            font.weight = FontWeight::BOLD;
            if !matches!(
                self.kind,
                Kind::CalloutOpen
                    | Kind::CalloutBody
                    | Kind::QuoteCalloutTitle
                    | Kind::QuoteCalloutBody
                    | Kind::EmojiQuote
            ) {
                color = c.strong;
            }
        }
        if flags & ITALIC != 0 {
            font.style = FontStyle::Italic;
        }
        if flags & STRIKE != 0 {
            color = c.faint_text;
            strike = Some(c.faint_text);
        }
        if flags & (LINK | UNDER) != 0 {
            underline = Some(Underline {
                color: if flags & LINK != 0 { c.link_underline } else { c.text },
                dashed: false,
                below: 2.,
            });
            if flags & LINK != 0 {
                color = c.text;
            }
        }
        if flags & MISSING != 0 {
            color = c.text3;
            underline = Some(Underline { color: c.link_underline, dashed: true, below: 2. });
        }
        if flags & MARK != 0 {
            fill = Some(Fill { color: c.highlight, pad: 0., inset: 1., radius: 3. });
        }
        if flags & TAG != 0 {
            font = typo.body();
            size = typo.small;
            color = c.text2;
            fill = Some(Fill { color: c.inline_bg, pad: PILL_PADDING, inset: 2., radius: 4. });
        }
        if flags & CODE != 0 {
            font = mono_font();
            size = typo.small;
            color = c.inline_code;
            fill = Some(Fill { color: c.inline_bg, pad: PILL_PADDING, inset: 2., radius: 4. });
        }
        if flags & RAW_MONO != 0 {
            font = mono_font();
            size = typo.small;
        }
        if flags & SYN_KEYWORD != 0 {
            color = c.syntax_keyword;
        }
        if flags & SYN_STRING != 0 {
            color = c.syntax_string;
        }
        if flags & SYN_NUMBER != 0 {
            color = c.syntax_number;
        }
        if flags & TERTIARY != 0 {
            color = c.text3;
        }
        if flags & FAINT != 0 {
            color = c.faint_text;
        }
        if flags & MARKER != 0 {
            color = c.marker;
        }
        if flags & BODY_SIZE != 0 {
            size = typo.size;
        }
        if flags & KIND_FADED != 0 {
            color = color.opacity(0.4);
        }
        Span {
            range: 0..0,
            font,
            size: px(size),
            color,
            underline,
            strike,
            fill,
            kern_after: px(0.),
            math: None,
            media: None,
            unbroken: false,
        }
    }

    /// The first character at or after `at` that isn't collapsed syntax.
    fn next_visible(&self, st: &Styled, at: usize) -> Option<char> {
        let mut i = at;
        while i < st.flags.len() && st.flags[i] & HIDDEN != 0 {
            i += 1;
        }
        self.src.get(i..)?.chars().next()
    }

    fn push_text(&mut self, st: &Styled, range: Range<usize>, flags: u32) {
        if range.is_empty() {
            return;
        }
        let src = self.src;
        let span = self.span_for(flags);
        let fixed_pitch = is_fixed_pitch(&span.font);
        if fixed_pitch {
            // Neither a gap nor a face here, and what's before it still counts.
            self.push_piece(range, &span, false, flags);
            return;
        }
        let boundary_gap = (self.typo.size * 0.22).round();
        // Cut at script boundaries: the composite face, and the gap where Han
        // meets Latin, both act per stretch.
        let text = &src[range.clone()];
        let mut start = 0;
        let composite = self.typo.composite.as_ref();
        let mut piece_cjk = None::<bool>;
        for (i, ch) in text.char_indices() {
            let script = script_of(ch);
            let cjk = composite.is_some()
                && takes_cjk_face(ch, self.prev_composite, || {
                    let after = i + ch.len_utf8();
                    text[after..].chars().next().or_else(|| self.next_visible(st, range.end))
                });
            if composite.is_some() {
                self.prev_composite = Some(ch);
            }
            let gap = self.gap_prev.is_some_and(|(s, _)| {
                (s == Script::Han && script == Script::Latin) || (s == Script::Latin && script == Script::Han)
            });
            let switch = piece_cjk.is_some_and(|p| p != cjk);
            if gap || switch {
                if i > start {
                    self.push_piece(range.start + start..range.start + i, &span, piece_cjk == Some(true), flags);
                    start = i;
                }
                if gap {
                    // On that character, which may be back before a monospaced run.
                    let at = self.gap_prev.expect("a gap has a character before it").1;
                    self.spans[at].kern_after += px(boundary_gap);
                }
            }
            piece_cjk = Some(cjk);
            // Where it lands is known once its piece goes in.
            self.gap_prev = Some((script, self.spans.len()));
        }
        if start < text.len() {
            self.push_piece(range.start + start..range.end, &span, piece_cjk == Some(true), flags);
        }
    }

    /// An object or atom: not a letter of either script.
    fn break_gap(&mut self) {
        self.gap_prev = Some((Script::Other, self.spans.len()));
    }

    fn push_piece(&mut self, range: Range<usize>, span: &Span, cjk: bool, flags: u32) {
        let text = &self.src[range.clone()];
        let disp = self.text.len();
        self.text.push_str(text);
        let mut piece = span.clone();
        piece.range = disp..disp + text.len();
        if cjk && let Some(comp) = &self.typo.composite {
            // Its weight, and never a slant: the paired faces have no italic and a
            // synthesised one on Han is the failure this exists to avoid.
            let weight = piece.font.weight;
            piece.font = font(comp.family);
            piece.font.weight = weight;
            piece.font.style = FontStyle::Normal;
            piece.size = piece.size * comp.scale;
        }
        self.map.push(MapSeg { disp, src: range.start, disp_len: text.len(), src_len: text.len() });
        // Room for the pills: kerning on the last character of a span and on
        // the character before one.
        if flags & PILL_END != 0 {
            piece.kern_after += px(PILL_PADDING);
        }
        if flags & PILL_BEFORE != 0 {
            piece.kern_after += px(PILL_PADDING);
        }
        self.spans.push(piece);
    }

    fn push_atom(&mut self, range: Range<usize>, text: &str, flags: u32, link: Option<Link>, math: Option<(String, bool)>) {
        self.break_gap();
        let disp = self.text.len();
        self.text.push_str(text);
        let mut span = self.span_for(flags & !HIDDEN);
        span.range = disp..disp + text.len();
        span.math = math.map(|(latex, display)| InlineMath { latex, display, rendered: None });
        if let Some(link) = link {
            span.underline = Some(Underline { color: self.c.link_underline, dashed: false, below: 3. });
            span.unbroken = true;
            self.links.push((disp..disp + text.len(), link));
        }
        self.map.push(MapSeg { disp, src: range.start, disp_len: text.len(), src_len: range.len() });
        self.spans.push(span);
    }

    /// An object standing for an image: for all of its source, or, behind the
    /// source when that shows, for none of it.
    fn push_media(&mut self, atom: &MediaAtom) {
        self.break_gap();
        let disp = self.text.len();
        self.text.push(OBJECT);
        let mut span = self.span_for(0);
        span.range = disp..disp + OBJECT.len_utf8();
        span.media = Some(std::rc::Rc::new(InlineMedia {
            info: atom.info.clone(),
            caption: atom.caption.clone(),
            fills_line: atom.revealed,
            src: atom.range.clone(),
        }));
        let (src, src_len) = if atom.revealed { (atom.range.end, 0) } else { (atom.range.start, atom.range.len()) };
        self.map.push(MapSeg { disp, src, disp_len: OBJECT.len_utf8(), src_len });
        self.spans.push(span);
    }

    fn run(
        &mut self,
        st: &Styled,
        atoms: &[(Range<usize>, String, Option<Link>)],
        latex: &[(Range<usize>, String, bool)],
        title: Option<&(Range<usize>, String)>,
        media: &[MediaAtom],
    ) {
        let mut i = 0;
        let n = st.flags.len();
        let boundary = |a: &MediaAtom, at: usize| if a.revealed { a.range.end == at } else { a.range.start == at };
        while i < n {
            if let Some(atom) = media.iter().find(|a| a.revealed && a.range.end == i) {
                self.push_media(atom);
            }
            if let Some(atom) = media.iter().find(|a| !a.revealed && a.range.start == i) {
                self.push_media(atom);
                i = atom.range.end;
                continue;
            }
            if let Some((range, text, link)) = atoms.iter().find(|a| a.0.start == i) {
                self.push_atom(range.clone(), text, 0, link.clone(), None);
                i = range.end;
                continue;
            }
            if let Some((range, inner, display)) = latex.iter().find(|a| a.0.start == i) {
                // One object-replacement character stands for the box.
                self.push_atom(range.clone(), "\u{FFFC}", 0, None, Some((inner.clone(), *display)));
                i = range.end;
                continue;
            }
            if let Some((range, text)) = title
                && range.start == i
            {
                self.push_atom(range.clone(), text, 0, None, None);
                i = range.end;
                continue;
            }
            let f = st.flags[i];
            let mut j = i + 1;
            while j < n
                && st.flags[j] == f
                && !atoms.iter().any(|a| a.0.start == j)
                && !latex.iter().any(|a| a.0.start == j)
                && !title.is_some_and(|t| t.0.start == j)
                && !media.iter().any(|a| boundary(a, j))
            {
                j += 1;
            }
            if f & HIDDEN == 0 {
                self.push_text(st, i..j, f);
            }
            i = j;
        }
        if let Some(atom) = media.iter().find(|a| a.revealed && a.range.end == n) {
            self.push_media(atom);
        }
    }
}

// Cells ----------------------------------------------------------------------

const CELL_CODE: u8 = 1 << 0;
const CELL_LINK: u8 = 1 << 1;
const CELL_BOLD: u8 = 1 << 2;
const CELL_ITALIC: u8 = 1 << 3;
const CELL_STRIKE: u8 = 1 << 4;

regex!(cell_code_re, r"`([^`\n]+)`");
regex!(cell_link_re, r"\[([^\]\n]+)\]\(([^)\s]+)\)");
regex!(cell_bold_re, r"\*\*(\S(?:.*?\S)?)\*\*|__(\S(?:.*?\S)?)__");
regex!(cell_strike_re, r"~~(\S(?:.*?\S)?)~~");

/// `(?<![*\w])\*(?=[^\s*])(.+?)(?<=[^\s*])\*(?![*\w])`, and the same for `_`.
fn cell_italics(s: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut i = 0;
    'scan: while i < s.len() {
        let Some(d) = s[i..].chars().next() else { break };
        let open = i + d.len_utf8();
        if !matches!(d, '*' | '_') {
            i = open;
            continue;
        }
        let before = s[..i].chars().next_back();
        let first = s[open..].chars().next();
        let opens = before.is_none_or(|c| c != d && !word(c)) && first.is_some_and(|c| c != d && !c.is_whitespace());
        if opens {
            let from = open + first.map_or(0, char::len_utf8);
            for (k, c) in s[from..].char_indices() {
                if c != d {
                    continue;
                }
                let close = from + k;
                let last = s[..close].chars().next_back().unwrap_or(' ');
                let after = s[close + 1..].chars().next();
                if last != d && !last.is_whitespace() && after.is_none_or(|c| c != d && !word(c)) {
                    out.push((i..close + 1, open..close));
                    i = close + 1;
                    continue 'scan;
                }
            }
        }
        i = open;
    }
    out
}

/// A cell's text as it's shown, and each visible byte's styling. Each rule runs
/// over what the last left, taking its markers out; one that reaches into inline
/// code is left as it is. `TableCellText.attributed`, which isn't the body's pass.
pub fn cell_markup(src: &str) -> (String, Vec<u8>) {
    let mut text = src.to_string();
    let mut flags = vec![0u8; text.len()];
    let rule = |text: &mut String, flags: &mut Vec<u8>, matches: Vec<(Range<usize>, Range<usize>)>, flag: u8| {
        for (full, inner) in matches.into_iter().rev() {
            if flag != CELL_CODE && flags[full.clone()].iter().any(|f| f & CELL_CODE != 0) {
                continue;
            }
            for f in &mut flags[inner.clone()] {
                *f |= flag;
            }
            flags.drain(inner.end..full.end);
            flags.drain(full.start..inner.start);
            text.replace_range(inner.end..full.end, "");
            text.replace_range(full.start..inner.start, "");
        }
    };
    let by_regex = |re: &Regex, s: &str| -> Vec<(Range<usize>, Range<usize>)> {
        re.captures_iter(s)
            .filter_map(|m| {
                let inner = m.iter().skip(1).flatten().next()?.range();
                Some((m.get(0)?.range(), inner))
            })
            .collect()
    };
    let m = by_regex(cell_code_re(), &text);
    rule(&mut text, &mut flags, m, CELL_CODE);
    let m = by_regex(cell_link_re(), &text);
    rule(&mut text, &mut flags, m, CELL_LINK);
    let m = by_regex(cell_bold_re(), &text);
    rule(&mut text, &mut flags, m, CELL_BOLD);
    let m = cell_italics(&text);
    rule(&mut text, &mut flags, m, CELL_ITALIC);
    let m = by_regex(cell_strike_re(), &text);
    rule(&mut text, &mut flags, m, CELL_STRIKE);
    (text, flags)
}

/// A table cell's inline Markdown, markers gone. A numeric column's body cells
/// have tabular digits, `monospacedDigitSystemFont`.
pub fn cell_spans(src: &str, header: bool, numeric: bool, typo: &Typography) -> (SharedString, Vec<Span>) {
    let c = t();
    let mut base_font = ui_font();
    if header {
        base_font.weight = FontWeight::SEMIBOLD;
    } else if numeric {
        base_font.features = FontFeatures(std::sync::Arc::new(vec![("tnum".to_string(), 1)]));
    }
    let base_color = if header { c.text2 } else { c.text };
    let (text, flags) = cell_markup(src);
    let mut spans: Vec<Span> = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let f = flags[at];
        let end = flags[at..].iter().position(|x| *x != f).map_or(text.len(), |n| at + n);
        let mut font = base_font.clone();
        let mut size = typo.small;
        let mut color = base_color;
        if f & CELL_CODE != 0 {
            font = mono_font();
            size = small_size(typo.small);
            color = c.inline_code;
        }
        if f & CELL_BOLD != 0 {
            font.weight = FontWeight::BOLD;
            color = c.strong;
        }
        if f & CELL_ITALIC != 0 {
            font.style = FontStyle::Italic;
        }
        spans.push(Span {
            range: at..end,
            font,
            size: px(size),
            color,
            underline: (f & CELL_LINK != 0).then_some(Underline { color: c.link_underline, dashed: false, below: 2. }),
            strike: (f & CELL_STRIKE != 0).then_some(color),
            fill: (f & CELL_CODE != 0).then_some(Fill { color: c.inline_bg, pad: PILL_PADDING, inset: -1., radius: 4. }),
            kern_after: px(0.),
            math: None,
            media: None,
            unbroken: false,
        });
        at = end;
    }
    (text.into(), spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_at(src: &str, kind: Kind, caret: Option<usize>) -> DisplayLine {
        let typo = Typography::default();
        build_line(
            src,
            &LineCtx {
                kind,
                caret,
                title: false,
                marked: None,
                prev: Kind::Blank,
                next: Kind::Blank,
                media: None,
                reveal_media: true,
                block_type: "note",
                typo: &typo,
                links: None,
                real: true,
                raw_block: false,
                prev_has_caret: false,
                first: false,
                last: false,
                comment_carry: (false, false),
            },
        )
    }

    fn line(src: &str, kind: Kind, reveal: bool) -> DisplayLine {
        line_at(src, kind, reveal.then_some(0))
    }

    const LIST: Kind = Kind::List { indent: 0, marker: 2 };

    #[core::prelude::v1::test]
    fn hides_markers_off_the_caret_line() {
        let d = line("a **粗体** `x`", Kind::Para, false);
        assert_eq!(d.text.as_ref(), "a 粗体 x");
        let d = line("## 标题 **b**", Kind::Heading(2), false);
        assert_eq!(d.text.as_ref(), "标题 b");
        let d = line("- item", LIST, false);
        assert_eq!(d.text.as_ref(), "item");
        assert_eq!(d.list, Some(ListGlyph::Bullet(BulletShape::Disc)));
    }

    #[core::prelude::v1::test]
    fn markers_reveal_only_on_the_carets_span() {
        let src = "**a** **b**";
        // On the first span: its stars show, the second's stay hidden.
        assert_eq!(line_at(src, Kind::Para, Some(2)).text.as_ref(), "**a** b");
        assert_eq!(line_at(src, Kind::Para, Some(8)).text.as_ref(), "a **b**");
        // A caret touching a span's end counts.
        assert_eq!(line_at(src, Kind::Para, Some(5)).text.as_ref(), "**a** b");
    }

    #[core::prelude::v1::test]
    fn block_syntax_shows_on_the_caret_line() {
        for (src, kind) in [
            ("## 标题", Kind::Heading(2)),
            ("- item", LIST),
            ("- [ ] item", LIST),
            ("> quote", Kind::Quote),
            ("---", Kind::Rule),
            ("```swift", Kind::FenceOpen),
        ] {
            let d = line(src, kind, true);
            assert_eq!(d.text.as_ref(), src);
            let total: usize = d.spans.iter().map(|s| s.range.len()).sum();
            assert_eq!(total, src.len());
        }
    }

    #[core::prelude::v1::test]
    fn map_round_trips_visible_offsets() {
        let src = "前 **粗体** 后 `码` [链](u) 尾";
        let d = line(src, Kind::Para, false);
        for (i, _) in d.text.char_indices() {
            let s = d.disp_to_src(i);
            assert!(src.is_char_boundary(s));
            assert_eq!(d.src_to_disp(s), i, "at {i}");
        }
    }

    #[core::prelude::v1::test]
    fn spans_cover_the_visible_text() {
        for (src, kind) in [
            ("plain 中文 text", Kind::Para),
            ("**粗体 bold** and *it* and `code` ~~x~~ ==m== <u>u</u> #tag", Kind::Para),
            ("[link](https://a.b/c) [[Some Note]] ![i](p.png) https://x.y/z.", Kind::Para),
            ("unclosed **bold and `tick and [bracket and [[wiki", Kind::Para),
            ("\\*escaped\\* snake_case_word 2*3*4 a ** b ** c", Kind::Para),
            ("***both*** __under__ _i_ ``a`b`` ** ", Kind::Para),
            ("混排**粗体**混排`代码`混排abc中", Kind::Para),
            ("$x^2$ and $y$", Kind::Para),
        ] {
            for caret in [None, Some(0), Some(src.len())] {
                let d = line_at(src, kind, caret);
                let mut at = 0;
                for s in &d.spans {
                    assert_eq!(s.range.start, at, "{src:?} {caret:?}");
                    at = s.range.end;
                }
                assert_eq!(at, d.text.len(), "{src:?} {caret:?}");
            }
        }
    }

    #[core::prelude::v1::test]
    fn inline_rules_follow_the_swift_patterns() {
        // No underscore emphasis in the editor, only asterisks.
        assert_eq!(line("__a__ _b_", Kind::Para, false).text.as_ref(), "__a__ _b_");
        // `*` needs non-space inside and no neighbouring `*`.
        assert_eq!(line("a * b * c", Kind::Para, false).text.as_ref(), "a * b * c");
        assert_eq!(line("*a*", Kind::Para, false).text.as_ref(), "a");
        // `==` needs non-space just inside.
        assert_eq!(line("== a ==", Kind::Para, false).text.as_ref(), "== a ==");
        assert_eq!(line("==a==", Kind::Para, false).text.as_ref(), "a");
        // Nothing inside code is Markdown.
        assert_eq!(line("`*a*`", Kind::Para, false).text.as_ref(), "*a*");
    }

    #[core::prelude::v1::test]
    fn hashtags_need_a_letter_after_a_boundary() {
        let tags = |s: &str| hashtag_spans(s).into_iter().map(|r| s[r].to_string()).collect::<Vec<_>>();
        assert_eq!(tags("#决策 and (#a/b) x#c #1"), ["#决策", "#a/b"]);
    }

    #[core::prelude::v1::test]
    fn bare_addresses_lose_trailing_punctuation() {
        let f = |s: &str| bare_address_spans(s).into_iter().map(|r| s[r].to_string()).collect::<Vec<_>>();
        assert_eq!(f("see https://a.b/c. and (https://x.y)"), ["https://a.b/c"]);
        assert_eq!(f("http://a.b，下"), ["http://a.b"]);
    }

    #[core::prelude::v1::test]
    fn han_and_latin_get_a_gap_without_spaces_in_the_text() {
        let d = line("用Fumi写", Kind::Para, false);
        assert_eq!(d.text.as_ref(), "用Fumi写");
        let gaps: f32 = d.spans.iter().map(|s| f32::from(s.kern_after)).sum();
        assert_eq!(gaps, (16f32 * 0.22).round() * 2.);
    }

    fn block_line(src: &str, kind: Kind, prev: Kind, next: Kind, ends: (bool, bool), real: bool) -> DisplayLine {
        let typo = Typography::default();
        build_line(
            src,
            &LineCtx {
                kind,
                caret: None,
                title: false,
                marked: None,
                prev,
                next,
                media: None,
                reveal_media: true,
                block_type: "note",
                typo: &typo,
                links: None,
                real,
                raw_block: false,
                prev_has_caret: false,
                first: ends.0,
                last: ends.1,
                comment_carry: (false, false),
            },
        )
    }

    #[core::prelude::v1::test]
    fn a_box_is_spaced_once_and_not_at_the_notes_ends() {
        let open = |first| block_line("```", Kind::FenceOpen, Kind::Para, Kind::FenceBody, (first, false), true);
        assert_eq!(open(false).top - open(true).top, px(BLOCK_SPACING));
        let close = |last| block_line("```", Kind::FenceClose, Kind::FenceBody, Kind::Para, (false, last), true);
        assert_eq!(close(false).bottom - close(true).bottom, px(BLOCK_SPACING));
        // What follows the box adds nothing of its own: the 14 is the box's.
        let after_box = block_line("text", Kind::Para, Kind::FenceClose, Kind::Para, (false, false), true);
        let after_text = block_line("text", Kind::Para, Kind::Para, Kind::Para, (false, false), true);
        assert_eq!(after_box.top, after_text.top);
        let after_callout = block_line("text", Kind::Para, Kind::CalloutClose, Kind::Para, (false, false), true);
        assert_eq!(after_callout.top, after_text.top);
    }

    #[core::prelude::v1::test]
    fn an_unclosed_callout_keeps_its_icon_without_a_box() {
        let d = block_line("::: tip", Kind::CalloutOpen, Kind::Blank, Kind::CalloutBody, (true, false), false);
        assert!(d.callout_icon.is_some());
        assert!(d.boxed.is_none());
    }

    #[core::prelude::v1::test]
    fn a_shown_quote_marker_is_body_size_on_an_attribution() {
        let typo = Typography::default();
        let d = line("> — Someone", Kind::Quote, true);
        let marker = d.spans.iter().find(|s| s.range.start == 0).unwrap();
        assert_eq!(marker.size, px(typo.size));
        assert!(d.spans.iter().any(|s| s.size == px(typo.small)), "the attribution stays small");
    }

    #[core::prelude::v1::test]
    fn code_is_the_system_monospace() {
        assert_eq!(mono_font().family.as_ref(), SYSTEM_MONO);
        assert!(is_fixed_pitch(&mono_font()));
        assert!(!is_fixed_pitch(&ui_font()));
    }

    #[core::prelude::v1::test]
    fn the_han_latin_gap_carries_across_code() {
        let gap = px((16f32 * 0.22).round());
        let total = |d: &DisplayLine| d.spans.iter().map(|s| s.kern_after).fold(px(0.), |a, b| a + b);
        let han_kern = |d: &DisplayLine| d.spans.iter().find(|s| d.text[s.range.clone()].contains('中')).unwrap().kern_after;
        // Han, code, Latin: the code is skipped, so Han meets Latin, and the gap
        // goes on the Han character, before the code. Han, code, Han: none.
        let (latin, han) = (line("中`x`a", Kind::Para, false), line("中`x`文", Kind::Para, false));
        assert_eq!(total(&latin) - total(&han), gap);
        assert_eq!(han_kern(&latin) - han_kern(&han), gap);
        // Latin, code, Han: the same the other way.
        let (han_after, latin_after) = (line("a`x`文", Kind::Para, false), line("a`x`b", Kind::Para, false));
        assert_eq!(total(&han_after) - total(&latin_after), gap);
    }

    #[core::prelude::v1::test]
    fn block_comments_span_lines_only_when_they_close() {
        assert_eq!(comment_carries("a /* b\nc\nd */ e\nf", "swift"), [false, true, true, false]);
        // Never closed: not a comment at all.
        assert_eq!(comment_carries("a /* b\nc", "swift"), [false, false]);
        // Hash languages have no block comments.
        assert_eq!(comment_carries("a /* b\nc */", "python"), [false, false]);
        assert_eq!(slash_comments("x /* y */ z // w", false, false), [2..9, 12..16]);
        assert_eq!(slash_comments("a */ b", true, false), [0..4]);
        assert_eq!(slash_comments("a /* b", false, true), [2..6]);
        assert_eq!(slash_comments("a /* b", false, false), Vec::<Range<usize>>::new());
        assert_eq!(slash_comments("whole", true, true), [0..5]);
    }

    #[core::prelude::v1::test]
    fn cells_have_their_own_markup() {
        let (text, flags) = cell_markup("**b** __u__ _i_ *j* ~~s~~ `c` [l](u)");
        assert_eq!(text, "b u i j s c l");
        let flag_at = |ch: char| flags[text.find(ch).unwrap()];
        assert_eq!(flag_at('b'), CELL_BOLD);
        assert_eq!(flag_at('u'), CELL_BOLD);
        assert_eq!(flag_at('i'), CELL_ITALIC);
        assert_eq!(flag_at('j'), CELL_ITALIC);
        assert_eq!(flag_at('s'), CELL_STRIKE);
        assert_eq!(flag_at('c'), CELL_CODE);
        assert_eq!(flag_at('l'), CELL_LINK);
        // What the body styles, a cell leaves as it is.
        let (text, flags) = cell_markup("#tag ==m== <u>x</u> [[w]] snake_case_name");
        assert_eq!(text, "#tag ==m== <u>x</u> [[w]] snake_case_name");
        assert!(flags.iter().all(|f| *f == 0));
        // Nothing inside code is markup.
        let (text, _) = cell_markup("`**x**`");
        assert_eq!(text, "**x**");
    }

    #[core::prelude::v1::test]
    fn a_numeric_column_has_tabular_digits() {
        let typo = Typography::default();
        let (_, spans) = cell_spans("1,234", false, true, &typo);
        assert!(spans[0].font.features.tag_value_list().iter().any(|(tag, on)| tag == "tnum" && *on == 1));
        let (_, spans) = cell_spans("1,234", true, true, &typo);
        assert!(spans[0].font.features.tag_value_list().is_empty(), "not the header");
    }

    #[core::prelude::v1::test]
    fn list_labels_and_levels() {
        assert_eq!(letter_label(1), "a");
        assert_eq!(letter_label(26), "z");
        assert_eq!(letter_label(27), "aa");
        let d = line_at("  3. item", Kind::List { indent: 2, marker: 3 }, None);
        assert_eq!(d.list, Some(ListGlyph::Ordered("c.".into())));
        assert_eq!(d.indent, px(48.));
        let d = line_at("    - item", Kind::List { indent: 4, marker: 2 }, None);
        assert_eq!(d.list, Some(ListGlyph::Bullet(BulletShape::Square)));
    }

    #[core::prelude::v1::test]
    fn checked_tasks_strike_their_text() {
        let d = line("- [x] done", LIST, false);
        assert_eq!(d.list, Some(ListGlyph::Task { checked: true }));
        assert_eq!(d.text.as_ref(), "done");
        assert!(d.spans[0].strike.is_some());
    }

    #[core::prelude::v1::test]
    fn callouts_hide_their_headers() {
        let d = line("> [!note] 标题", Kind::QuoteCalloutTitle, false);
        assert_eq!(d.text.as_ref(), "标题");
        let d = line("> [!note]", Kind::QuoteCalloutTitle, false);
        assert_eq!(d.text.as_ref(), "Note");
        let d = line("::: warning 标题", Kind::CalloutOpen, false);
        assert_eq!(d.text.as_ref(), "标题");
        let d = line("::: warning", Kind::CalloutOpen, false);
        assert_eq!(d.text.as_ref(), "warning");
    }

    #[core::prelude::v1::test]
    fn display_math_can_sit_in_a_line() {
        assert_eq!(display_math_spans("a $$x$$ b"), [(2..7, 4..5)]);
        assert_eq!(display_math_spans("\\\\$$x$$"), []);
        assert_eq!(display_math_spans("$$$$"), []);
        let d = line("a $$x^2$$ b", Kind::Para, false);
        assert_eq!(d.text.as_ref(), "a \u{FFFC} b");
        let math: Vec<_> = d.spans.iter().filter_map(|s| s.math.as_ref()).collect();
        assert_eq!(math.len(), 1);
        assert!(math[0].display);
        assert_eq!(math[0].latex, "x^2");
        // Both kinds on one line.
        let d = line("$a$ and $$b$$", Kind::Para, false);
        let flags: Vec<bool> = d.spans.iter().filter_map(|s| s.math.as_ref()).map(|m| m.display).collect();
        assert_eq!(flags, [false, true]);
        // On the caret: its source, small and monospaced.
        let d = line_at("a $$x$$ b", Kind::Para, Some(4));
        assert_eq!(d.text.as_ref(), "a $$x$$ b");
        let typo = Typography::default();
        assert!(d.spans.iter().any(|s| s.size == px(typo.small)));
    }

    #[core::prelude::v1::test]
    fn the_paired_face_goes_by_the_families_installed() {
        let has = |names: &'static [&'static str]| move |f: &str| names.contains(&f);
        let pick = |family: &str, han: bool, names: &'static [&'static str]| Composite::pick(family, han, has(names)).map(|c| c.family);
        assert_eq!(pick("Georgia", false, &["Songti SC", "PingFang SC"]), Some("Songti SC"));
        assert_eq!(pick("Georgia", false, &["STSong", "PingFang SC"]), Some("STSong"));
        assert_eq!(pick("Georgia", false, &["Songti TC"]), Some("Songti TC"));
        assert_eq!(pick("Georgia", false, &["PingFang SC"]), None);
        assert_eq!(pick("Helvetica", false, &["Hiragino Sans GB"]), Some("Hiragino Sans GB"));
        assert_eq!(pick("Helvetica", false, &["PingFang SC", "Hiragino Sans GB"]), Some("PingFang SC"));
        assert_eq!(pick("Noto Sans CJK", true, &["PingFang SC"]), None);
        assert_eq!(pick("System", false, &["PingFang SC"]), None);
        assert_eq!(pick("SF Mono", false, &["PingFang SC"]), None);
    }

    #[core::prelude::v1::test]
    fn quotes_take_the_face_of_what_they_quote() {
        let next = |c: char| move || Some(c);
        // Opening quotes look ahead, whatever precedes them.
        assert!(takes_cjk_face('“', Some('a'), next('中')));
        assert!(!takes_cjk_face('“', Some('中'), next('a')));
        // Closing quotes and dashes look back.
        assert!(takes_cjk_face('”', Some('中'), next('a')));
        assert!(!takes_cjk_face('—', Some('a'), next('中')));
        // A punctuation mark before it doesn't count as CJK.
        assert!(!takes_cjk_face('—', Some('”'), next('中')));
        assert!(takes_cjk_face('。', None, || None));
        assert!(!takes_cjk_face('a', Some('中'), next('中')));
        assert_eq!(face_claim('\u{FF00}'), FaceClaim::Other);
        assert_eq!(face_claim('\u{FF01}'), FaceClaim::Cjk);
        assert_eq!(face_claim('€'), FaceClaim::Punctuation);
    }

    #[core::prelude::v1::test]
    fn an_opening_quote_skips_collapsed_syntax() {
        // Without a composite face nothing here changes, so go by the pure rule
        // over the same source the assembler would see.
        let typo = Typography {
            composite: Some(Composite { family: "Songti SC", scale: 0.97 }),
            ..Typography::new("Georgia", 16., 1.7)
        };
        let d = build_line(
            "“**中**”",
            &LineCtx {
                kind: Kind::Para,
                caret: None,
                title: false,
                marked: None,
                prev: Kind::Blank,
                next: Kind::Blank,
                block_type: "",
                typo: &typo,
                links: None,
                real: false,
                raw_block: false,
                prev_has_caret: false,
                first: false,
                last: false,
                comment_carry: (false, false),
                media: None,
                reveal_media: true,
            },
        );
        assert_eq!(d.text.as_ref(), "“中”");
        assert!(d.spans.iter().all(|s| s.font.family.as_ref() == "Songti SC"), "the quotes pair too");
        assert!(d.spans.iter().all(|s| s.font.style == FontStyle::Normal));
    }

    #[core::prelude::v1::test]
    fn the_caret_goes_by_the_font_before_it_but_never_small() {
        use crate::element::caret_font;
        let body = px(16.);
        let mut d = line("ab `c` d", Kind::Para, false);
        // At the start: the line's own.
        assert_eq!(caret_font(&d, 0, body).1, d.font_size);
        // Right after the code: its smaller size, which isn't under 80%.
        let code_end = d.text.find('c').unwrap() + 1;
        assert_eq!(caret_font(&d, code_end, body).1, px(Typography::default().small));
        // Under 80%: the body size.
        for span in &mut d.spans {
            span.size = px(0.1);
        }
        assert_eq!(caret_font(&d, 1, body).1, body);
    }

    #[core::prelude::v1::test]
    fn inline_math_is_one_atom() {
        let d = line("a $x^2$ b", Kind::Para, false);
        assert_eq!(d.text.as_ref(), "a \u{FFFC} b");
        assert_eq!(d.spans.iter().filter(|s| s.math.is_some()).count(), 1);
        assert_eq!(d.src_to_disp(9), d.text.len());
        // On the caret: its source, monospaced.
        let d = line_at("a $x^2$ b", Kind::Para, Some(3));
        assert_eq!(d.text.as_ref(), "a $x^2$ b");
        // Not a price.
        assert_eq!(line("$5 and $6", Kind::Para, false).text.as_ref(), "$5 and $6");
    }

    struct Fake;

    impl MediaLookup for Fake {
        fn lookup(&self, dest: &str, embed: bool) -> Lookup {
            match dest {
                "pending.png" => Lookup::Pending,
                "missing.png" | "missing" => Lookup::Missing,
                _ => Lookup::Ready(MediaInfo {
                    key: dest.into(),
                    kind: if dest.ends_with(".gif") {
                        crate::media::MediaKind::Image { animated: true }
                    } else {
                        crate::media::MediaKind::Image { animated: false }
                    },
                    natural: Some((100., 50.)),
                    settling: embed && dest == "settling.png",
                }),
            }
        }
    }

    fn media_line(src: &str, caret: Option<usize>) -> DisplayLine {
        let typo = Typography::default();
        build_line(
            src,
            &LineCtx {
                kind: if is_lone_image(src.trim()) { Kind::Image } else { Kind::Para },
                caret,
                title: false,
                marked: None,
                prev: Kind::Blank,
                next: Kind::Blank,
                media: Some(&Fake),
                reveal_media: true,
                block_type: "",
                typo: &typo,
                links: None,
                real: true,
                raw_block: false,
                prev_has_caret: false,
                first: false,
                last: false,
                comment_carry: (false, false),
            },
        )
    }

    fn medias(d: &DisplayLine) -> Vec<&InlineMedia> {
        d.spans.iter().filter_map(|s| s.media.as_deref()).collect()
    }

    #[core::prelude::v1::test]
    fn a_lone_image_collapses_to_one_box_with_its_spacing() {
        let src = "![a cat](a.png)";
        let d = media_line(src, None);
        assert_eq!(d.text.as_ref(), "\u{FFFC}");
        let m = medias(&d)[0];
        assert_eq!((m.caption.as_str(), m.fills_line, m.src.clone()), ("a cat", false, 0..src.len()));
        assert_eq!(d.metric, LineMetric::Natural { spacing: 4. });
        assert_eq!((d.top, d.bottom), (px(8.), px(8.)));
    }

    #[core::prelude::v1::test]
    fn a_revealed_image_follows_its_source_and_stands_for_no_text() {
        let src = "![a cat](a.png)";
        for caret in [0, 3, src.len()] {
            let d = media_line(src, Some(caret));
            assert_eq!(d.text.as_ref(), format!("{src}\u{FFFC}"));
            assert!(medias(&d)[0].fills_line);
            assert_eq!(d.metric, LineMetric::Natural { spacing: 10. });
            // The source's end is the box's start: the caret stays on the text.
            assert_eq!(d.src_to_disp(src.len()), src.len());
            assert_eq!(d.disp_to_src(src.len() + OBJECT.len_utf8()), src.len());
        }
    }

    #[core::prelude::v1::test]
    fn the_caret_past_a_box_maps_past_its_source() {
        let src = "x ![](a.png) y";
        let d = media_line(src, None);
        assert_eq!(d.text.as_ref(), "x \u{FFFC} y");
        let after = 2 + "![](a.png)".len();
        assert_eq!(d.src_to_disp(after), 2 + OBJECT.len_utf8());
        assert_eq!(d.disp_to_src(2 + OBJECT.len_utf8()), after);
        assert_eq!(d.src_to_disp(2), 2);
    }

    #[core::prelude::v1::test]
    fn an_image_opening_a_line_sets_its_spacing_and_one_further_along_does_not() {
        assert_eq!(media_line("![](a.png) words", None).metric, LineMetric::Natural { spacing: 4. });
        let d = media_line("words ![](a.png) words", None);
        assert_eq!(d.metric, LineMetric::Fixed(Typography::default().line));
        assert_eq!(d.top, px(0.));
        // Unless its source shows.
        let d = media_line("words ![](a.png) words", Some(8));
        assert_eq!(d.metric, LineMetric::Natural { spacing: 10. });
        assert_eq!(d.text.as_ref(), "words ![](a.png)\u{FFFC} words");
    }

    #[core::prelude::v1::test]
    fn what_can_not_be_shown_stays_source() {
        for src in ["![alt](missing.png)", "![alt](pending.png)"] {
            let d = media_line(src, None);
            assert_eq!(d.text.as_ref(), src);
            assert_eq!(d.metric, LineMetric::Fixed(Typography::default().line), "{src}");
            assert_eq!(d.top, px(0.));
        }
        assert!(media_line("![alt](pending.png)", None).media_pending);
        assert!(!media_line("![alt](missing.png)", None).media_pending);
        // Alt quieter than the rest, as in Fumi.
        let d = media_line("![alt](missing.png)", None);
        let c = t();
        let color_of = |at: usize| d.spans.iter().find(|s| s.range.contains(&at)).unwrap().color;
        assert_eq!(color_of(2), c.text3);
        assert_eq!(color_of(8), c.faint_text);
        // An embed nothing answers to is plain text, and no link.
        let d = media_line("![[missing]]", None);
        assert_eq!(d.text.as_ref(), "![[missing]]");
        assert!(d.links.is_empty());
    }

    #[core::prelude::v1::test]
    fn embeds_have_no_caption_and_inline_code_holds_no_images() {
        let d = media_line("![[a.png|300]]", None);
        assert_eq!(medias(&d)[0].caption, "");
        assert_eq!(d.text.as_ref(), "\u{FFFC}");
        let d = media_line("`![](a.png)` x", None);
        assert!(medias(&d).is_empty());
    }

    #[core::prelude::v1::test]
    fn a_selection_passing_over_leaves_images_closed() {
        let typo = Typography::default();
        let src = "![](a.png)";
        let d = build_line(
            src,
            &LineCtx {
                kind: Kind::Image,
                caret: Some(3),
                title: false,
                marked: None,
                prev: Kind::Blank,
                next: Kind::Blank,
                media: Some(&Fake),
                reveal_media: false,
                block_type: "",
                typo: &typo,
                links: None,
                real: true,
                raw_block: false,
                prev_has_caret: false,
                first: false,
                last: false,
                comment_carry: (false, false),
            },
        );
        assert_eq!(d.text.as_ref(), "\u{FFFC}");
    }

    #[core::prelude::v1::test]
    fn media_spans_are_found_in_order_outside_code() {
        let spans = media_spans("a ![x](b.mp4) `![](c.png)` ![[d.mov|2]]");
        let dests: Vec<_> = spans.iter().map(|m| (m.dest.as_str(), m.embed)).collect();
        assert_eq!(dests, [("b.mp4", false), ("d.mov|2", true)]);
    }
}
