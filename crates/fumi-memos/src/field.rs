//! The memo text field: a `TextField` styled like the editor on every change, the
//! syntax left in place but quieter. Port of `MemoTextEditor` and
//! `MemoTypography.highlight`.

use crate::rich::{RichLayout, RunStyle, Span, Spacing, restyle};
use crate::typo::{self, MemoType};
use cocoa::base::{id, nil};
use fumi_core::memo_markdown::{self, SpanKind};
use fumi_editor::display::small_size;
use fumi_ui::settings;
use fumi_ui::text_field::{FieldLayout, FieldStyler, TextField};
use fumi_ui::theme::{Tokens, t};
use gpui::*;
use objc::{class, msg_send, sel, sel_impl};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

pub const TEXT_SIZE: f32 = 15.;
pub const LINE_SPACING: f32 = 6.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ink {
    #[default]
    Text,
    Marker,
    Strong,
    InlineCode,
    Accent,
    Secondary,
    Tertiary,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attr {
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
    pub ink: Ink,
    pub background: bool,
    pub strike: bool,
}

/// `MemoTypography.highlight`: spans apply in order, each over what came before.
pub fn highlight(text: &str) -> Vec<(Range<usize>, Attr)> {
    let mut attrs = vec![Attr::default(); text.len()];
    for span in memo_markdown::spans(text) {
        if span.range.end > text.len() {
            continue;
        }
        for attr in &mut attrs[span.range.clone()] {
            match span.kind {
                SpanKind::Marker => attr.ink = Ink::Marker,
                SpanKind::Strong => {
                    attr.bold = true;
                    attr.mono = false;
                    attr.italic = false;
                    attr.ink = Ink::Strong;
                }
                // Spans come strong first, so italic keeps an enclosing bold.
                SpanKind::Emphasis => attr.italic = true,
                SpanKind::Strike => attr.strike = true,
                SpanKind::Code => {
                    attr.mono = true;
                    attr.bold = false;
                    attr.italic = false;
                    attr.ink = Ink::InlineCode;
                    attr.background = true;
                }
                SpanKind::Link | SpanKind::Tag => attr.ink = Ink::Accent,
                SpanKind::Quote => attr.ink = Ink::Secondary,
                SpanKind::CodeBlock => {
                    attr.mono = true;
                    attr.bold = false;
                    attr.italic = false;
                }
                SpanKind::Done => {
                    attr.strike = true;
                    attr.ink = Ink::Tertiary;
                }
            }
        }
    }
    let mut runs: Vec<(Range<usize>, Attr)> = Vec::new();
    for (i, attr) in attrs.into_iter().enumerate() {
        match runs.last_mut() {
            Some((range, last)) if *last == attr => range.end = i + 1,
            _ => runs.push((i..i + 1, attr)),
        }
    }
    runs
}

fn style_of(attr: Attr, ty: &MemoType, c: &Tokens) -> RunStyle {
    let color = match attr.ink {
        Ink::Text => c.text,
        Ink::Marker => c.marker,
        Ink::Strong => c.strong,
        Ink::InlineCode => c.inline_code,
        Ink::Accent => c.accent,
        Ink::Secondary => c.text2,
        Ink::Tertiary => c.text3,
    };
    let (font, size) = if attr.mono {
        let mut f = typo::mono();
        if attr.italic {
            f.style = FontStyle::Italic;
        }
        (f, small_size(TEXT_SIZE))
    } else {
        (ty.font(attr.bold, attr.italic), TEXT_SIZE)
    };
    let mut style = RunStyle::plain(font, size, color);
    style.background = attr.background.then_some(c.inline_bg);
    style.strike = attr.strike.then_some(c.faint_text);
    style
}

/// The misspelled words of `text`, by byte, as `NSSpellChecker` finds them. The
/// word still being typed at the end isn't one yet.
pub fn misspelled(text: &str) -> Vec<Range<usize>> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRange {
        location: u64,
        length: u64,
    }
    let mut ranges = Vec::new();
    let units: Vec<u16> = text.encode_utf16().collect();
    let to_byte = |unit: usize| String::from_utf16_lossy(&units[..unit.min(units.len())]).len();
    #[allow(deprecated)]
    unsafe {
        let checker: id = msg_send![class!(NSSpellChecker), sharedSpellChecker];
        if checker == nil {
            return ranges;
        }
        let string = fumi_ui::native::ns(text);
        let mut from = 0u64;
        while (from as usize) < units.len() {
            let range: NSRange = msg_send![checker, checkSpellingOfString: string startingAt: from as i64];
            if range.length == 0 || range.location >= units.len() as u64 || range.location < from {
                break;
            }
            let end = range.location + range.length;
            if (end as usize) < units.len() {
                ranges.push(to_byte(range.location as usize)..to_byte(end as usize));
            }
            from = end;
        }
        let _: () = msg_send![string, release];
    }
    ranges
}

struct Laid(Rc<RichLayout>);

impl FieldLayout for Laid {
    fn height(&self) -> Pixels {
        // `ceil(used) + 2`, as the Swift view measures itself.
        px(f32::from(self.0.height).ceil() + 2.)
    }

    fn index_at(&self, p: Point<Pixels>) -> usize {
        self.0.index_at(p)
    }

    fn caret(&self, at: usize) -> (Point<Pixels>, Pixels) {
        self.0.caret(at)
    }

    fn selection(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        self.0.selection(range)
    }

    fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        self.0.paint(origin, window, cx)
    }
}

type StyleKey = (String, Option<Range<usize>>, Pixels, u32, u32, String);

/// Styles a memo field. `tokens` pins its colors; otherwise it follows the theme.
pub struct MemoStyler {
    tokens: Option<fn() -> Tokens>,
    spell_check: bool,
    last: RefCell<Option<(StyleKey, Rc<dyn FieldLayout>)>>,
    last_hint: RefCell<Option<(StyleKey, Rc<dyn FieldLayout>)>>,
}

impl MemoStyler {
    pub fn new(tokens: Option<fn() -> Tokens>) -> Self {
        MemoStyler {
            tokens,
            spell_check: settings::spell_check(),
            last: RefCell::new(None),
            last_hint: RefCell::new(None),
        }
    }

    fn tokens(&self) -> Tokens {
        self.tokens.map_or_else(t, |tokens| tokens())
    }

    fn key(&self, text: &str, marked: Option<Range<usize>>, width: Pixels, c: &Tokens) -> StyleKey {
        let color = |h: Hsla| (h.l.to_bits() ^ h.h.to_bits().rotate_left(8)) ^ h.s.to_bits().rotate_left(16);
        (text.to_string(), marked, width, color(c.text), color(c.accent), settings::editor_font())
    }
}

impl FieldStyler for MemoStyler {
    fn layout(&self, text: &str, marked: Option<Range<usize>>, width: Pixels, window: &mut Window) -> Rc<dyn FieldLayout> {
        let c = self.tokens();
        let key = self.key(text, marked.clone(), width, &c);
        if let Some((last, layout)) = &*self.last.borrow()
            && *last == key
        {
            return layout.clone();
        }
        let ty = MemoType::from_settings();
        let mut spans: Vec<Span> = highlight(text)
            .into_iter()
            .map(|(range, attr)| Span { range, style: style_of(attr, &ty, &c) })
            .collect();
        if self.spell_check {
            for range in misspelled(text) {
                spans = restyle(spans, range, |s| s.wavy = Some(hsla(0.01, 0.9, 0.55, 1.)));
            }
        }
        if let Some(marked) = marked.clone() {
            spans = restyle(spans, marked, |s| s.underline = Some(s.color));
        }
        let base = RunStyle::plain(ty.font(false, false), TEXT_SIZE, c.text);
        let spacing = Spacing { line: LINE_SPACING, after_last: true };
        let laid = RichLayout::new(text, &spans, &base, &ty, spacing, width, window);
        let layout: Rc<dyn FieldLayout> = Rc::new(Laid(Rc::new(laid)));
        *self.last.borrow_mut() = Some((key, layout.clone()));
        layout
    }

    fn placeholder(&self, text: &str, width: Pixels, window: &mut Window) -> Rc<dyn FieldLayout> {
        let c = self.tokens();
        let key = self.key(text, None, width, &c);
        if let Some((last, layout)) = &*self.last_hint.borrow()
            && *last == key
        {
            return layout.clone();
        }
        let ty = MemoType::from_settings();
        let base = RunStyle::plain(ty.font(false, false), TEXT_SIZE, c.faint_text);
        let spans = [Span { range: 0..text.len(), style: base.clone() }];
        let spacing = Spacing { line: LINE_SPACING, after_last: true };
        let laid = RichLayout::new(text, &spans, &base, &ty, spacing, width, window);
        let layout: Rc<dyn FieldLayout> = Rc::new(Laid(Rc::new(laid)));
        *self.last_hint.borrow_mut() = Some((key, layout.clone()));
        layout
    }
}

/// A memo field: plain text, undo on, wrapping, Return a new line and ⌘↩ submit.
pub fn memo_field(
    placeholder: &'static str,
    min_height: f32,
    tokens: Option<fn() -> Tokens>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<TextField> {
    cx.new(|cx| {
        let mut field = TextField::new(placeholder, window, cx);
        field.multiline = true;
        field.enter_adds_line = true;
        field.font_size = px(TEXT_SIZE);
        field.line_height = px(TEXT_SIZE * 1.2 + LINE_SPACING);
        field.min_height = px(min_height);
        field.smart_punctuation = settings::smart_punctuation();
        field.caret_color = tokens.map(|tokens| tokens().accent);
        field.selection_color = tokens.map(|tokens| tokens().selection);
        field.styler = Some(Rc::new(MemoStyler::new(tokens)));
        field
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr_at(text: &str, at: usize) -> Attr {
        highlight(text).into_iter().find(|(range, _)| range.contains(&at)).unwrap().1
    }

    #[core::prelude::v1::test]
    fn runs_cover_the_text() {
        assert!(highlight("").is_empty());
        let runs = highlight("a **b** c");
        assert_eq!(runs.first().unwrap().0.start, 0);
        assert_eq!(runs.last().unwrap().0.end, 9);
        for pair in runs.windows(2) {
            assert_eq!(pair[0].0.end, pair[1].0.start);
        }
    }

    #[core::prelude::v1::test]
    fn syntax_stays_but_is_quieter() {
        let text = "a **b** c";
        assert_eq!(attr_at(text, 0), Attr::default());
        assert_eq!(attr_at(text, 2).ink, Ink::Marker);
        let bold = attr_at(text, 4);
        assert!(bold.bold);
        assert_eq!(bold.ink, Ink::Strong);
    }

    #[core::prelude::v1::test]
    fn italic_keeps_an_enclosing_bold() {
        let text = "**a *b* c**";
        let x = attr_at(text, 5);
        assert!(x.bold && x.italic, "{x:?}");
    }

    #[core::prelude::v1::test]
    fn code_tags_quotes_and_done_items() {
        let code = attr_at("a `b` c", 3);
        assert!(code.mono && code.background);
        assert_eq!(code.ink, Ink::InlineCode);
        assert_eq!(attr_at("see #tag", 5).ink, Ink::Accent);
        assert_eq!(attr_at("> quote", 3).ink, Ink::Secondary);
        let done = attr_at("- [x] done", 7);
        assert!(done.strike);
        assert_eq!(done.ink, Ink::Tertiary);
        assert!(attr_at("```\ncode\n```", 5).mono);
    }

    #[core::prelude::v1::test]
    fn code_is_a_step_smaller() {
        let c = Tokens::resolve(fumi_ui::theme::AppTheme::White, false, "#1D6FC2");
        let style = style_of(Attr { mono: true, ..Attr::default() }, &MemoType::new("System"), &c);
        assert_eq!(style.size, px(13.));
    }
}
