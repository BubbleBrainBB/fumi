//! One source line as it appears on screen: the visible text, its styled runs,
//! and a map back to source bytes. Sizes follow Fumi's Markdown spec: body 16
//! on a fixed 28 line, title 30, headings 26/21/18 at 1.3 times their size.

use crate::markdown::*;
use gpui::*;
use std::ops::Range;

pub fn ink() -> Hsla {
    rgb(0x37352F).into()
}
pub fn ink2() -> Hsla {
    rgb(0x6B6A65).into()
}
pub fn marker_gray() -> Hsla {
    rgb(0xB4B2AB).into()
}
pub fn hairline() -> Hsla {
    rgb(0xE9E8E3).into()
}
pub fn tint() -> Hsla {
    rgb(0xF6F5F2).into()
}
pub fn accent() -> Hsla {
    rgb(0x2F6F5E).into()
}
pub fn selection() -> Hsla {
    hsla(0.46, 0.45, 0.45, 0.22)
}

pub fn body_font() -> Font {
    let mut f = font(".SystemUIFont");
    f.fallbacks = Some(FontFallbacks::from_fonts(vec!["PingFang SC".into()]));
    f
}

pub fn mono_font() -> Font {
    let mut f = font("Menlo");
    f.fallbacks = Some(FontFallbacks::from_fonts(vec!["PingFang SC".into()]));
    f
}

#[derive(Clone, Copy, PartialEq)]
pub enum Deco {
    None,
    Quote,
    Rule,
    Code { first: bool, last: bool },
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

pub struct DisplayLine {
    pub text: SharedString,
    pub runs: Vec<TextRun>,
    pub map: Vec<MapSeg>,
    pub src_len: usize,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub top: Pixels,
    pub bottom: Pixels,
    pub indent: Pixels,
    /// The page margin on each side; GPUI's `list` ignores horizontal padding.
    pub margin: Pixels,
    /// Leading syntax on the caret's line, in visible bytes. It hangs left of
    /// the text column, so showing it doesn't push the words sideways.
    pub hang: usize,
    /// A to-do whose box is drawn in the gutter.
    pub task: bool,
    pub gutter: Option<SharedString>,
    pub deco: Deco,
}

impl DisplayLine {
    pub fn height(&self, rows: usize) -> Pixels {
        self.top + self.line_height * rows.max(1) as f32 + self.bottom
    }

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
}

struct Builder<'a> {
    src: &'a str,
    text: String,
    runs: Vec<TextRun>,
    map: Vec<MapSeg>,
}

impl Builder<'_> {
    fn add(&mut self, range: Range<usize>, run: TextRun) {
        if range.is_empty() {
            return;
        }
        let disp = self.text.len();
        self.text.push_str(&self.src[range.clone()]);
        self.map.push(MapSeg {
            disp,
            src: range.start,
            disp_len: range.len(),
            src_len: range.len(),
        });
        self.runs.push(TextRun { len: range.len(), ..run });
    }
}

fn run(font: &Font, color: Hsla) -> TextRun {
    TextRun {
        len: 0,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn inline_run(base: &Font, color: Hsla, style: u8) -> TextRun {
    let mut r = run(base, color);
    if style & BOLD != 0 {
        r.font.weight = FontWeight::SEMIBOLD;
    }
    if style & ITALIC != 0 {
        r.font.style = FontStyle::Italic;
    }
    if style & CODE != 0 {
        r.font = mono_font();
        r.background_color = Some(tint());
        r.color = rgb(0xB5482F).into();
    }
    if style & STRIKE != 0 {
        r.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.),
            color: Some(ink2()),
        });
        r.color = ink2();
    }
    if style & (LINK | WIKI) != 0 {
        r.color = accent();
    }
    if style & LINK != 0 {
        r.underline = Some(UnderlineStyle {
            thickness: px(1.),
            color: Some(accent().opacity(0.4)),
            wavy: false,
        });
    }
    if style & MARK != 0 {
        r.background_color = Some(rgb(0xFFF2A8).into());
    }
    r
}

/// Inline styling for `range` of the source. `reveal` keeps markers, dimmed.
fn add_inline(b: &mut Builder, range: Range<usize>, base: &Font, color: Hsla, reveal: bool) {
    let offset = range.start;
    let src = b.src;
    for seg in parse_inline(&src[range]) {
        let r = seg.range.start + offset..seg.range.end + offset;
        if seg.marker {
            if reveal {
                b.add(r, run(base, marker_gray()));
            }
        } else {
            b.add(r, inline_run(base, color, seg.style));
        }
    }
}

pub struct LineCtx {
    pub kind: Kind,
    pub reveal: bool,
    pub title: bool,
    pub marked: Option<Range<usize>>,
    pub prev_is_code: bool,
    pub next_is_code: bool,
}

pub fn build_line(src: &str, cx: &LineCtx) -> DisplayLine {
    let mut b = Builder {
        src,
        text: String::with_capacity(src.len()),
        runs: Vec::with_capacity(6),
        map: Vec::with_capacity(6),
    };
    let body = body_font();
    let mono = mono_font();
    let reveal = cx.reveal;
    let mut size = px(16.);
    let mut lh = px(28.);
    let (mut top, mut bottom) = (px(0.), px(0.));
    let mut indent = px(0.);
    let mut gutter = None;
    let mut deco = Deco::None;
    let mut hang = 0;
    let mut task_box = false;

    match cx.kind {
        Kind::Heading(level) => {
            let (sz, before, after) = if cx.title {
                (30., 0., 10.)
            } else {
                match level {
                    1 => (26., 24., 8.),
                    2 => (21., 16., 8.),
                    3 => (18., 12., 4.),
                    _ => (16., 8., 2.),
                }
            };
            size = px(sz);
            lh = px((sz * 1.3_f32).round().max(28.));
            top = px(before);
            bottom = px(after);
            let mut f = body.clone();
            f.weight = FontWeight::SEMIBOLD;
            let hashes = src.bytes().take_while(|c| *c == b' ' || *c == b'#').count();
            let hashes = hashes + usize::from(src.as_bytes().get(hashes) == Some(&b' '));
            if reveal {
                b.add(0..hashes, run(&f, marker_gray()));
                hang = hashes;
            }
            let color = if level >= 6 { ink2() } else { ink() };
            add_inline(&mut b, hashes..src.len(), &f, color, reveal);
        }
        Kind::List { indent: n, marker } => {
            let n = n as usize;
            let marker = marker as usize;
            let mut text_at = n + marker;
            indent = px(22. + n as f32 * 8.);
            let tail = &src[text_at.min(src.len())..];
            let task = if tail.starts_with("[ ] ") {
                Some("☐")
            } else if tail.starts_with("[x] ") || tail.starts_with("[X] ") {
                Some("☑")
            } else {
                None
            };
            if task.is_some() {
                text_at += 4;
            }
            if reveal {
                b.add(0..text_at.min(src.len()), run(&body, marker_gray()));
                hang = text_at.min(src.len());
            } else {
                task_box = task.is_some();
                let bullet = src.as_bytes().get(n).copied().unwrap_or(b'-');
                gutter = Some(match task {
                    Some(t) => SharedString::from(t),
                    None if matches!(bullet, b'-' | b'*' | b'+') => SharedString::from("•"),
                    None => SharedString::from(src[n..n + marker - 1].to_string()),
                });
            }
            add_inline(&mut b, text_at.min(src.len())..src.len(), &body, ink(), reveal);
        }
        Kind::Quote => {
            let lead = src.bytes().take_while(|c| *c == b' ').count();
            let mut at = lead + 1;
            if src.as_bytes().get(at) == Some(&b' ') {
                at += 1;
            }
            indent = px(16.);
            deco = Deco::Quote;
            if reveal {
                b.add(0..at, run(&body, marker_gray()));
                hang = at;
            }
            add_inline(&mut b, at..src.len(), &body, ink2(), reveal);
        }
        Kind::Rule => {
            if reveal {
                b.add(0..src.len(), run(&body, marker_gray()));
            } else {
                deco = Deco::Rule;
            }
        }
        Kind::FenceOpen | Kind::FenceBody | Kind::FenceClose => {
            size = px(14.);
            lh = px(22.);
            indent = px(12.);
            deco = Deco::Code {
                first: !cx.prev_is_code,
                last: !cx.next_is_code,
            };
            match cx.kind {
                Kind::FenceBody => b.add(0..src.len(), run(&mono, ink())),
                _ if reveal => {
                    let lang = src.trim_start_matches([' ', '`', '~']);
                    let at = src.len() - lang.len();
                    b.add(0..at, run(&mono, marker_gray()));
                    b.add(at..src.len(), run(&mono, marker_gray()));
                    hang = at;
                }
                Kind::FenceOpen => {
                    // The language tag alone, as a quiet label.
                    let lang = src.trim_start_matches([' ', '`', '~']);
                    let at = src.len() - lang.len();
                    b.add(at..src.len(), run(&mono, marker_gray()));
                }
                _ => {}
            }
        }
        Kind::FrontOpen | Kind::FrontBody | Kind::FrontClose => {
            size = px(13.);
            lh = px(20.);
            b.add(0..src.len(), run(&mono, ink2()));
        }
        Kind::MathOpen | Kind::MathBody | Kind::MathClose | Kind::MathOne => {
            size = px(14.);
            lh = px(22.);
            b.add(0..src.len(), run(&mono, ink2()));
        }
        Kind::TableHead | Kind::TableDelim | Kind::TableRow => {
            size = px(15.);
            lh = px(26.);
            b.add(0..src.len(), run(&body, ink2()));
        }
        Kind::Image => b.add(0..src.len(), run(&mono, ink2())),
        Kind::Blank | Kind::Para => add_inline(&mut b, 0..src.len(), &body, ink(), reveal),
    }

    let task = task_box;
    let mut runs = b.runs;
    let map = b.map;
    let text = b.text;
    if let Some(marked) = &cx.marked {
        runs = underline_marked(runs, marked, &map);
    }
    DisplayLine {
        text: text.into(),
        runs,
        map,
        src_len: src.len(),
        font_size: size,
        line_height: lh,
        top,
        bottom,
        indent,
        margin: px(0.),
        hang,
        task,
        gutter,
        deco,
    }
}

/// Splits runs to underline the composing text an input method has marked.
fn underline_marked(runs: Vec<TextRun>, marked: &Range<usize>, map: &[MapSeg]) -> Vec<TextRun> {
    let to_disp = |src: usize| {
        map.iter()
            .find(|s| src >= s.src && src <= s.src + s.src_len)
            .map_or(0, |s| s.disp + (src - s.src).min(s.disp_len))
    };
    let (start, end) = (to_disp(marked.start), to_disp(marked.end));
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut at = 0;
    for r in runs {
        let (a, b) = (at, at + r.len);
        at = b;
        let mut piece = |from: usize, to: usize, under: bool| {
            if to > from {
                let mut p = r.clone();
                p.len = to - from;
                if under {
                    p.underline = Some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(ink()),
                        wavy: false,
                    });
                }
                out.push(p);
            }
        };
        piece(a, b.min(start).max(a), false);
        piece(a.max(start).min(b), b.min(end).max(a), true);
        piece(a.max(end).min(b), b, false);
    }
    out
}

/// A table cell or other folded text: markers gone, no map.
pub fn cell_text(src: &str, header: bool) -> (SharedString, Vec<TextRun>) {
    let mut f = body_font();
    if header {
        f.weight = FontWeight::SEMIBOLD;
    }
    let mut b = Builder {
        src,
        text: String::with_capacity(src.len()),
        runs: Vec::with_capacity(2),
        map: Vec::new(),
    };
    add_inline(&mut b, 0..src.len(), &f, ink(), false);
    (b.text.into(), b.runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(src: &str, kind: Kind, reveal: bool) -> DisplayLine {
        build_line(
            src,
            &LineCtx {
                kind,
                reveal,
                title: false,
                marked: None,
                prev_is_code: false,
                next_is_code: false,
            },
        )
    }

    #[core::prelude::v1::test]
    fn hides_markers_off_the_caret_line() {
        let d = line("a **粗体** `x`", Kind::Para, false);
        assert_eq!(d.text.as_ref(), "a 粗体 x");
        let d = line("## 标题 **b**", Kind::Heading(2), false);
        assert_eq!(d.text.as_ref(), "标题 b");
        let d = line("- item", Kind::List { indent: 0, marker: 2 }, false);
        assert_eq!(d.text.as_ref(), "item");
        assert_eq!(d.gutter.as_deref(), Some("•"));
    }

    #[core::prelude::v1::test]
    fn caret_line_shows_the_source_unchanged() {
        for (src, kind) in [
            ("a **粗体** `x` [l](u) [[w]]", Kind::Para),
            ("## 标题 **b**", Kind::Heading(2)),
            ("- [ ] item *i*", Kind::List { indent: 0, marker: 2 }),
            ("> quote", Kind::Quote),
            ("---", Kind::Rule),
            ("```swift", Kind::FenceOpen),
            ("| a | b |", Kind::TableRow),
        ] {
            let d = line(src, kind, true);
            assert_eq!(d.text.as_ref(), src);
            assert_eq!(d.runs.iter().map(|r| r.len).sum::<usize>(), src.len());
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
        // A caret on a hidden marker lands on the visible text after it.
        let star = src.find('*').unwrap();
        assert_eq!(&d.text[d.src_to_disp(star)..d.src_to_disp(star) + 6], "粗体");
    }
}
