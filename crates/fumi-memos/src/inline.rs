//! A saved memo as drawn: its blocks, each with inline Markdown resolved and its
//! `#tags` as runs of their own. Port of `MemoRow.attributed` and `MemoBody`.

use crate::rich::{Link, RichDoc, RunStyle, Span, Spacing};
use crate::typo::{self, MemoType};
use fumi_core::memo_format;
use fumi_core::memo_inline;
use fumi_core::memo_markdown::{self, Block, Marker};
use fumi_ui::theme::Tokens;
use gpui::*;
use std::ops::Range;
use std::rc::Rc;

/// A run of drawn text. Tags split the source first, so Markdown never spans a tag.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Segment {
    pub text: String,
    pub strong: bool,
    pub emphasis: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<Link>,
}

pub fn segments(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut cursor = 0;
    let inline = |piece: &str, out: &mut Vec<Segment>| {
        for run in memo_inline::parse(piece) {
            out.push(Segment {
                text: run.text,
                strong: run.strong,
                emphasis: run.emphasis,
                code: run.code,
                strike: run.strike,
                link: run.link.map(Link::Url),
            });
        }
    };
    for (name, range) in memo_format::tag_ranges(text) {
        if range.start > cursor {
            inline(&text[cursor..range.start], &mut out);
        }
        out.push(Segment { text: text[range.clone()].to_string(), link: Some(Link::Tag(name)), ..Segment::default() });
        cursor = range.end;
    }
    if cursor < text.len() {
        inline(&text[cursor..], &mut out);
    }
    out
}

/// Line breaks and tabs as the text layout wants them.
pub fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "    ")
}

const TEXT_SIZE: f32 = 15.;
const LINE_SPACING: f32 = 6.;

#[derive(Clone)]
pub enum BlockDoc {
    Paragraph(Rc<RichDoc>),
    Item { level: usize, marker: Marker, line: usize, doc: Rc<RichDoc> },
    Quote(Rc<RichDoc>),
    Code(Rc<RichDoc>),
}

fn inline_doc(
    key: String,
    text: &str,
    color: Hsla,
    struck: Option<Hsla>,
    ty: &MemoType,
    c: &Tokens,
) -> Rc<RichDoc> {
    let base = RunStyle::plain(ty.font(false, false), TEXT_SIZE, color);
    let mut display = String::new();
    let mut spans = Vec::new();
    let mut links: Vec<(Range<usize>, Link)> = Vec::new();
    for segment in segments(&normalized(text)) {
        let start = display.len();
        display.push_str(&segment.text);
        let range = start..display.len();
        let mut style = base.clone();
        if segment.code {
            style.font = typo::mono();
            if segment.strong {
                style.font.weight = FontWeight::BOLD;
            }
            if segment.emphasis {
                style.font.style = FontStyle::Italic;
            }
        } else {
            style.font = ty.font(segment.strong, segment.emphasis);
        }
        if segment.strong {
            style.color = c.strong;
        }
        if let Some(link) = &segment.link {
            style.color = c.accent;
            links.push((range.clone(), link.clone()));
        }
        style.strike = if segment.strike { Some(color) } else { struck };
        spans.push(Span { range, style });
    }
    let spacing = Spacing { line: LINE_SPACING, after_last: false };
    Rc::new(RichDoc::new(key, display, spans, base, ty.clone(), spacing, links))
}

fn code_doc(key: String, text: &str, ty: &MemoType, c: &Tokens) -> Rc<RichDoc> {
    let base = RunStyle::plain(typo::mono(), 12., c.text);
    let display = normalized(text);
    let spans = vec![Span { range: 0..display.len(), style: base.clone() }];
    let spacing = Spacing { line: 3., after_last: false };
    Rc::new(RichDoc::new(key, display, spans, base, ty.clone(), spacing, Vec::new()))
}

/// A memo's blocks, ready to draw. `id` keeps selections apart.
pub fn memo_blocks(id: &str, text: &str, ty: &MemoType, c: &Tokens) -> Vec<BlockDoc> {
    memo_markdown::blocks(text)
        .into_iter()
        .enumerate()
        .map(|(index, block)| {
            let key = format!("{id}/{index}");
            match block {
                Block::Paragraph(text) => BlockDoc::Paragraph(inline_doc(key, &text, c.text, None, ty, c)),
                Block::Item { level, marker, text, line } => {
                    let done = matches!(marker, Marker::Task { checked: true });
                    let doc = if done {
                        inline_doc(key, &text, c.text3, Some(c.faint_text), ty, c)
                    } else {
                        inline_doc(key, &text, c.text, None, ty, c)
                    };
                    BlockDoc::Item { level, marker, line, doc }
                }
                Block::Quote(text) => BlockDoc::Quote(inline_doc(key, &text, c.text2, None, ty, c)),
                Block::Code { text, .. } => BlockDoc::Code(code_doc(key, &text, ty, c)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str) -> Option<Link> {
        Some(Link::Tag(name.to_string()))
    }

    #[core::prelude::v1::test]
    fn tags_are_runs_of_their_own() {
        let s = segments("read #book today");
        assert_eq!(s.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(), ["read ", "#book", " today"]);
        assert_eq!(s[1].link, tag("book"));
        assert_eq!(s[0].link, None);
    }

    #[core::prelude::v1::test]
    fn markdown_never_spans_a_tag() {
        // The Swift app parses each stretch between tags on its own.
        let s = segments("**a #b c**");
        assert_eq!(s.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(), ["**a ", "#b", " c**"]);
        assert!(!s[0].strong);
    }

    #[core::prelude::v1::test]
    fn inline_styles_and_links_come_through() {
        let s = segments("**b** [l](https://a.com)");
        assert!(s[0].strong);
        assert_eq!(s[2].link, Some(Link::Url("https://a.com".into())));
    }

    #[core::prelude::v1::test]
    fn line_breaks_normalize() {
        assert_eq!(normalized("a\r\nb\tc"), "a\nb    c");
    }

    #[core::prelude::v1::test]
    fn blocks_follow_the_markdown() {
        let c = Tokens::resolve(fumi_ui::theme::AppTheme::White, false, "#1D6FC2");
        let ty = MemoType::new("System");
        let blocks = memo_blocks("d#0", "hi\n\n- [x] done\n\n> q\n\n```\ncode\n```", &ty, &c);
        assert!(matches!(blocks[0], BlockDoc::Paragraph(_)));
        let BlockDoc::Item { marker, doc, .. } = &blocks[1] else { panic!() };
        assert_eq!(*marker, Marker::Task { checked: true });
        assert_eq!(doc.spans[0].style.color, c.text3);
        assert_eq!(doc.spans[0].style.strike, Some(c.faint_text));
        let BlockDoc::Quote(quote) = &blocks[2] else { panic!() };
        assert_eq!(quote.spans[0].style.color, c.text2);
        let BlockDoc::Code(code) = &blocks[3] else { panic!() };
        assert_eq!(code.text, "code");
        assert_eq!(code.base.size, px(12.));
    }
}
