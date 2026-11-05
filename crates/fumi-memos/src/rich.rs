//! Lays out and draws memo text: runs in several fonts and sizes, wrapped by
//! `fumi_editor::textlayout`, on TextKit's line metrics (each row as tall as its
//! tallest font, spacing added below it). The saved memos draw through the
//! selectable [`RichText`] element, the composer through `field::MemoStyler`.

use crate::typo::MemoType;
use fumi_editor::textlayout::{BoxMode, Laid, RunInput, is_cjk};
use gpui::*;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq)]
pub struct RunStyle {
    pub font: Font,
    pub size: Pixels,
    pub color: Hsla,
    pub background: Option<Hsla>,
    pub strike: Option<Hsla>,
    pub underline: Option<Hsla>,
    pub wavy: Option<Hsla>,
}

impl RunStyle {
    pub fn plain(font: Font, size: f32, color: Hsla) -> Self {
        RunStyle { font, size: px(size), color, background: None, strike: None, underline: None, wavy: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: RunStyle,
}

/// Splits a sorted run of spans at `range` and changes what's inside it.
pub fn restyle(spans: Vec<Span>, range: Range<usize>, change: impl Fn(&mut RunStyle)) -> Vec<Span> {
    let mut out = Vec::with_capacity(spans.len() + 2);
    for span in spans {
        let (a, b) = (span.range.start.max(range.start), span.range.end.min(range.end));
        if a >= b {
            out.push(span);
            continue;
        }
        if span.range.start < a {
            out.push(Span { range: span.range.start..a, style: span.style.clone() });
        }
        let mut style = span.style.clone();
        change(&mut style);
        out.push(Span { range: a..b, style });
        if b < span.range.end {
            out.push(Span { range: b..span.range.end, style: span.style });
        }
    }
    out
}

/// How a block's lines stand apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    pub line: f32,
    /// TextKit keeps the spacing after the last row too; SwiftUI's `Text` doesn't.
    pub after_last: bool,
}

/// Whether the paired face takes `c`: the Han and Kana ranges, and the curly
/// punctuation that follows them.
pub fn composite_claims(c: char, prev_claimed: bool) -> bool {
    is_cjk(c) || (matches!(c as u32, 0x2010..=0x2027 | 0x2030..=0x205E) && prev_claimed)
}

struct RowMetric {
    baseline: Pixels,
    natural: Pixels,
}

struct LaidLine {
    start: usize,
    end: usize,
    laid: Laid,
    metrics: Vec<RowMetric>,
    decor: Vec<(Range<usize>, Decor)>,
}

#[derive(Clone, Copy)]
enum Decor {
    Background(Hsla),
    Strike(Hsla, Pixels),
    Underline(Hsla),
    Wavy(Hsla),
}

pub struct RichLayout {
    lines: Vec<LaidLine>,
    pub height: Pixels,
    text_len: usize,
}

fn natural_metrics(fonts: &[(FontId, Pixels)], window: &Window) -> RowMetric {
    let ts = window.text_system();
    let (mut ascent, mut descent) = (px(0.), px(0.));
    for (font, size) in fonts {
        ascent = ascent.max(px(f32::from(ts.ascent(*font, *size)).round()));
        descent = descent.max(px(f32::from(ts.descent(*font, *size)).abs().round()));
    }
    RowMetric { baseline: ascent, natural: ascent + descent }
}

impl RichLayout {
    pub fn new(
        text: &str,
        spans: &[Span],
        base: &RunStyle,
        ty: &MemoType,
        spacing: Spacing,
        width: Pixels,
        window: &Window,
    ) -> Self {
        let ts = window.text_system();
        let base_font = ts.resolve_font(&base.font);
        let mut lines = Vec::new();
        let mut y = px(0.);
        let mut start = 0;
        let mut pieces: Vec<(Range<usize>, Font, Pixels, Hsla)> = Vec::new();
        let hard_lines = text.split('\n').count();
        for (index, line) in text.split('\n').enumerate() {
            let end = start + line.len();
            pieces.clear();
            let mut decor = Vec::new();
            for span in spans {
                let (a, b) = (span.range.start.max(start), span.range.end.min(end));
                if a >= b {
                    continue;
                }
                let local = a - start..b - start;
                split_cjk(line, local.clone(), &span.style, ty, &mut pieces);
                let thin_x = {
                    let id = ts.resolve_font(&span.style.font);
                    ts.x_height(id, span.style.size)
                };
                if let Some(c) = span.style.background {
                    decor.push((local.clone(), Decor::Background(c)));
                }
                if let Some(c) = span.style.strike {
                    decor.push((local.clone(), Decor::Strike(c, thin_x / 2.)));
                }
                if let Some(c) = span.style.underline {
                    decor.push((local.clone(), Decor::Underline(c)));
                }
                if let Some(c) = span.style.wavy {
                    decor.push((local.clone(), Decor::Wavy(c)));
                }
            }
            let runs: Vec<RunInput> = pieces
                .iter()
                .map(|(range, font, size, color)| RunInput {
                    text: &line[range.clone()],
                    font,
                    size: *size,
                    color: *color,
                    kern_after: px(0.),
                    box_size: None,
                    box_mode: BoxMode::Centered,
                })
                .collect();
            let mut laid = Laid::shape(line, &runs, |_| px(0.), width, true, |_, _| px(0.), window);
            let mut metrics = Vec::with_capacity(laid.rows.len());
            let row_count = laid.rows.len();
            for r in 0..row_count {
                let (row_start, row_end) = (laid.rows[r].start, laid.rows[r].end);
                let mut fonts: Vec<(FontId, Pixels)> = Vec::new();
                for g in laid.glyphs_in(row_start..row_end) {
                    if !fonts.contains(&(g.font, g.size)) {
                        fonts.push((g.font, g.size));
                    }
                }
                if fonts.is_empty() {
                    fonts.push((base_font, base.size));
                }
                let metric = natural_metrics(&fonts, window);
                let last = index + 1 == hard_lines && r + 1 == row_count;
                let gap = if last && !spacing.after_last { 0. } else { spacing.line };
                laid.rows[r].y = y;
                laid.rows[r].height = metric.natural + px(gap);
                y += laid.rows[r].height;
                metrics.push(metric);
            }
            lines.push(LaidLine { start, end, laid, metrics, decor });
            start = end + 1;
        }
        RichLayout { lines, height: y, text_len: text.len() }
    }

    fn line_of(&self, at: usize) -> &LaidLine {
        let at = at.min(self.text_len);
        let k = self.lines.partition_point(|line| line.start <= at);
        &self.lines[k.saturating_sub(1)]
    }

    /// The byte offset nearest to a point.
    pub fn index_at(&self, p: Point<Pixels>) -> usize {
        for line in &self.lines {
            for (r, row) in line.laid.rows.iter().enumerate() {
                if p.y < row.y + row.height {
                    return line.start + line.laid.index_at(r, p.x);
                }
            }
        }
        self.text_len
    }

    /// The caret's top-left at byte `at`, and its height.
    pub fn caret(&self, at: usize) -> (Point<Pixels>, Pixels) {
        let line = self.line_of(at);
        let local = at.min(line.end) - line.start;
        let r = line.laid.row_of(local);
        let row = line.laid.rows[r];
        (point(line.laid.x_in_row(local, r), row.y), line.metrics[r].natural)
    }

    fn rects_in_line(line: &LaidLine, a: usize, b: usize, mut each: impl FnMut(usize, Pixels, Pixels)) {
        for (r, row) in line.laid.rows.iter().enumerate() {
            let (from, to) = (a.max(row.start), b.min(row.end));
            if from > to || (from == to && !(row.start == row.end)) {
                continue;
            }
            let x0 = line.laid.x_for_index(from) - row.x0;
            let x1 = line.laid.x_for_index(to) - row.x0;
            each(r, x0, x1);
        }
    }

    /// Row by row, the boxes over a byte range.
    pub fn selection(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        let mut rects = Vec::new();
        for line in &self.lines {
            if range.end < line.start || range.start > line.end {
                continue;
            }
            let a = range.start.max(line.start) - line.start;
            let b = range.end.min(line.end) - line.start;
            let past_end = range.end > line.end;
            Self::rects_in_line(line, a, b, |r, x0, x1| {
                let row = line.laid.rows[r];
                // A selected line break shows as a sliver.
                let x1 = if past_end && r + 1 == line.laid.rows.len() { x1 + px(4.) } else { x1 };
                if x1 > x0 {
                    rects.push(Bounds::from_corners(point(x0, row.y), point(x1, row.y + row.height)));
                }
            });
        }
        rects
    }

    pub fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        let _ = &cx;
        for line in &self.lines {
            for (range, decor) in &line.decor {
                if let Decor::Background(color) = decor {
                    Self::rects_in_line(line, range.start, range.end, |r, x0, x1| {
                        let row = line.laid.rows[r];
                        let bounds = Bounds::from_corners(
                            origin + point(x0, row.y),
                            origin + point(x1, row.y + line.metrics[r].natural),
                        );
                        window.paint_quad(fill(bounds, *color));
                    });
                }
            }
            line.laid.paint_glyphs(origin, |r| line.metrics[r].baseline, window);
            for (range, decor) in &line.decor {
                Self::rects_in_line(line, range.start, range.end, |r, x0, x1| {
                    let row = line.laid.rows[r];
                    let base = origin.y + row.y + line.metrics[r].baseline;
                    let width = x1 - x0;
                    match decor {
                        Decor::Strike(color, lift) => {
                            let bounds = Bounds::new(point(origin.x + x0, base - *lift), size(width, px(1.)));
                            window.paint_quad(fill(bounds, *color));
                        }
                        Decor::Underline(color) => {
                            let bounds = Bounds::new(point(origin.x + x0, base + px(1.5)), size(width, px(1.)));
                            window.paint_quad(fill(bounds, *color));
                        }
                        Decor::Wavy(color) => {
                            window.paint_underline(
                                point(origin.x + x0, base + px(1.)),
                                width,
                                &UnderlineStyle { thickness: px(1.), color: Some(*color), wavy: true },
                            );
                        }
                        Decor::Background(_) => {}
                    }
                });
            }
        }
    }
}

/// Splits a run where the script changes, setting Chinese in the paired face at
/// its slightly smaller size.
fn split_cjk(
    line: &str,
    range: Range<usize>,
    style: &RunStyle,
    ty: &MemoType,
    pieces: &mut Vec<(Range<usize>, Font, Pixels, Hsla)>,
) {
    let Some(composite) = &ty.composite else {
        pieces.push((range, style.font.clone(), style.size, style.color));
        return;
    };
    let paired = |r: Range<usize>| {
        let mut f = font(composite.family);
        f.weight = style.font.weight;
        f.style = style.font.style;
        let size = px((f32::from(style.size) * composite.scale * 2.).round() / 2.);
        (r, f, size, style.color)
    };
    let mut prev = line[..range.start].chars().next_back().is_some_and(|c| composite_claims(c, false));
    let mut start = range.start;
    let mut current = None::<bool>;
    for (offset, c) in line[range.clone()].char_indices() {
        let at = range.start + offset;
        let claimed = composite_claims(c, prev);
        if let Some(was) = current
            && was != claimed
        {
            pieces.push(if was { paired(start..at) } else { (start..at, style.font.clone(), style.size, style.color) });
            start = at;
        }
        current = Some(claimed);
        prev = claimed;
    }
    if let Some(was) = current {
        let rest = start..range.end;
        pieces.push(if was { paired(rest) } else { (rest, style.font.clone(), style.size, style.color) });
    }
}

// The element.

/// Where a click on a run leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Tag(String),
    Url(String),
}

/// One block of memo text, ready to lay out at any width.
pub struct RichDoc {
    /// Selections belong to one doc at a time, by this key.
    pub key: SharedString,
    pub text: String,
    pub spans: Vec<Span>,
    pub base: RunStyle,
    pub ty: MemoType,
    pub spacing: Spacing,
    pub links: Vec<(Range<usize>, Link)>,
    cache: RefCell<Option<(Pixels, Rc<RichLayout>)>>,
}

impl RichDoc {
    pub fn new(
        key: impl Into<SharedString>,
        text: String,
        spans: Vec<Span>,
        base: RunStyle,
        ty: MemoType,
        spacing: Spacing,
        links: Vec<(Range<usize>, Link)>,
    ) -> Self {
        RichDoc { key: key.into(), text, spans, base, ty, spacing, links, cache: RefCell::new(None) }
    }

    pub fn layout(&self, width: Pixels, window: &Window) -> Rc<RichLayout> {
        if let Some((cached_width, layout)) = &*self.cache.borrow()
            && *cached_width == width
        {
            return layout.clone();
        }
        let layout =
            Rc::new(RichLayout::new(&self.text, &self.spans, &self.base, &self.ty, self.spacing, width, window));
        *self.cache.borrow_mut() = Some((width, layout.clone()));
        layout
    }

    pub fn link_at(&self, at: usize) -> Option<&Link> {
        self.links.iter().find(|(range, _)| range.start <= at && at < range.end).map(|(_, link)| link)
    }
}

/// The one selection on the page: SwiftUI's selectable text selects within a
/// `Text`, so within a block here.
#[derive(Default)]
pub struct TextSelection {
    pub doc: Option<Rc<RichDoc>>,
    pub anchor: usize,
    pub head: usize,
    dragging: bool,
    moved: bool,
    pending_link: Option<Link>,
}

impl TextSelection {
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub fn text(&self) -> Option<String> {
        let doc = self.doc.as_ref()?;
        let range = self.range();
        (!range.is_empty()).then(|| doc.text[range].to_string())
    }

    pub fn clear(&mut self) -> bool {
        let had = self.doc.is_some();
        *self = TextSelection::default();
        had
    }
}

/// The word around byte `at`.
pub fn word_range(text: &str, at: usize) -> Range<usize> {
    for (start, word) in text.split_word_bound_indices() {
        if at >= start && at < start + word.len() {
            return start..start + word.len();
        }
    }
    text.len()..text.len()
}

/// The hard line around byte `at`.
pub fn line_range(text: &str, at: usize) -> Range<usize> {
    let at = at.min(text.len());
    let start = text[..at].rfind('\n').map_or(0, |p| p + 1);
    let end = text[at..].find('\n').map_or(text.len(), |p| at + p);
    start..end
}

/// What a [`RichText`] tells its page.
#[derive(Clone)]
pub struct RichHost {
    pub selection: Rc<RefCell<TextSelection>>,
    pub on_change: Rc<dyn Fn(&mut Window, &mut App)>,
    pub on_link: Rc<dyn Fn(&Link, &mut Window, &mut App)>,
}

pub struct RichText {
    doc: Rc<RichDoc>,
    host: RichHost,
}

impl RichText {
    pub fn new(doc: Rc<RichDoc>, host: RichHost) -> Self {
        RichText { doc, host }
    }
}

impl IntoElement for RichText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

pub struct RichPrepaint {
    layout: Rc<RichLayout>,
    hitbox: Hitbox,
}

impl Element for RichText {
    type RequestLayoutState = ();
    type PrepaintState = RichPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let doc = self.doc.clone();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let id = window.request_measured_layout(style, move |known, available, window, _| {
            let width = known.width.unwrap_or(match available.width {
                AvailableSpace::Definite(width) => width,
                _ => px(300.),
            });
            size(width, doc.layout(width, window).height)
        });
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> RichPrepaint {
        RichPrepaint {
            layout: self.doc.layout(bounds.size.width, window),
            hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut RichPrepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let origin = bounds.origin;
        let layout = prepaint.layout.clone();
        let doc = self.doc.clone();
        let hitbox = prepaint.hitbox.clone();
        {
            let selection = self.host.selection.borrow();
            if selection.doc.as_ref().is_some_and(|d| d.key == doc.key) {
                let tint = fumi_ui::theme::t().selection;
                for rect in layout.selection(selection.range()) {
                    window.paint_quad(fill(Bounds::new(origin + rect.origin, rect.size), tint));
                }
            }
        }
        layout.paint(origin, window, cx);

        // An I-beam over text, a hand over a link.
        if hitbox.is_hovered(window) {
            let at = layout.index_at(window.mouse_position() - origin);
            let cursor = if doc.link_at(at).is_some() { CursorStyle::PointingHand } else { CursorStyle::IBeam };
            window.set_cursor_style(cursor, &hitbox);
        }

        window.on_mouse_event({
            let (doc, layout, host, hitbox) = (doc.clone(), layout.clone(), self.host.clone(), hitbox.clone());
            move |e: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || e.button != MouseButton::Left || !hitbox.is_hovered(window) {
                    return;
                }
                let at = layout.index_at(e.position - origin);
                {
                    let mut s = host.selection.borrow_mut();
                    let range = match e.click_count {
                        2 => word_range(&doc.text, at),
                        n if n >= 3 => line_range(&doc.text, at),
                        _ => at..at,
                    };
                    s.pending_link = if e.click_count == 1 { doc.link_at(at).cloned() } else { None };
                    s.doc = Some(doc.clone());
                    s.anchor = range.start;
                    s.head = range.end;
                    s.dragging = true;
                    s.moved = false;
                }
                (host.on_change)(window, cx);
            }
        });
        window.on_mouse_event({
            let (doc, layout, host) = (doc.clone(), layout.clone(), self.host.clone());
            move |e: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || e.pressed_button != Some(MouseButton::Left) {
                    return;
                }
                let changed = {
                    let mut s = host.selection.borrow_mut();
                    if !s.dragging || !s.doc.as_ref().is_some_and(|d| d.key == doc.key) {
                        return;
                    }
                    let at = layout.index_at(e.position - origin);
                    let changed = at != s.head;
                    if changed {
                        s.head = at;
                        s.moved = true;
                        s.pending_link = None;
                    }
                    changed
                };
                if changed {
                    (host.on_change)(window, cx);
                }
            }
        });
        window.on_mouse_event({
            let (doc, host) = (doc, self.host.clone());
            move |e: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || e.button != MouseButton::Left {
                    return;
                }
                let link = {
                    let mut s = host.selection.borrow_mut();
                    if !s.dragging || !s.doc.as_ref().is_some_and(|d| d.key == doc.key) {
                        return;
                    }
                    s.dragging = false;
                    s.pending_link.take()
                };
                if let Some(link) = link {
                    (host.on_link)(&link, window, cx);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(color: f32) -> RunStyle {
        RunStyle::plain(font("Menlo"), 15., hsla(0., 0., color, 1.))
    }

    #[core::prelude::v1::test]
    fn restyling_splits_spans_at_the_range() {
        let spans = vec![
            Span { range: 0..5, style: style(0.) },
            Span { range: 5..10, style: style(0.5) },
        ];
        let out = restyle(spans, 3..7, |s| s.strike = Some(black()));
        let ranges: Vec<_> = out.iter().map(|s| s.range.clone()).collect();
        assert_eq!(ranges, [0..3, 3..5, 5..7, 7..10]);
        assert!(out[0].style.strike.is_none());
        assert!(out[1].style.strike.is_some() && out[2].style.strike.is_some());
        assert!(out[3].style.strike.is_none());
    }

    #[core::prelude::v1::test]
    fn the_paired_face_takes_han_and_the_punctuation_after_it() {
        assert!(composite_claims('中', false));
        assert!(composite_claims('，', false));
        assert!(!composite_claims('a', true));
        assert!(!composite_claims('“', false));
        assert!(composite_claims('“', true));
    }

    #[core::prelude::v1::test]
    fn a_run_splits_by_script_only_when_paired() {
        let mut pieces = Vec::new();
        let ty = MemoType::new("New York");
        let line = "ab中文cd";
        split_cjk(line, 0..line.len(), &style(0.), &ty, &mut pieces);
        let ranges: Vec<_> = pieces.iter().map(|p| p.0.clone()).collect();
        assert_eq!(ranges, [0..2, 2..8, 8..10]);
        assert_eq!(pieces[1].1.family.as_ref(), "Songti SC");
        assert_eq!(pieces[1].2, px(14.5));
        assert_eq!(pieces[0].2, px(15.));

        let mut pieces = Vec::new();
        split_cjk(line, 0..line.len(), &style(0.), &MemoType::new("System"), &mut pieces);
        assert_eq!(pieces.len(), 1);
    }

    #[core::prelude::v1::test]
    fn double_and_triple_clicks_take_a_word_and_a_line() {
        let text = "hello world\nnext";
        assert_eq!(word_range(text, 2), 0..5);
        assert_eq!(word_range(text, 7), 6..11);
        assert_eq!(line_range(text, 7), 0..11);
        assert_eq!(line_range(text, 13), 12..16);
        assert_eq!(word_range(text, 99), 16..16);
    }
}
