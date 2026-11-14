//! The element for one source line: measures it, draws the block decoration
//! behind it, then selection, glyphs and what stands in for hidden syntax.

use crate::display::*;
use crate::media::{self, InlineMedia, MediaKind};
use crate::textlayout::*;
use crate::video::{self, VideoUi};
use fumi_ui::native::{self, SymbolCache};
use fumi_ui::theme::t;
use gpui::*;
use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

/// Shaped text with the line's vertical metrics.
pub struct Shaped {
    pub laid: Laid,
    pub ascent: Pixels,
    pub descent: Pixels,
    pub natural: bool,
    /// The room a box gets: the column, less the line's indents.
    pub column: Pixels,
}

impl Shaped {
    /// From the row's top down to its baseline. With a fixed line height the
    /// extra room goes above the text, as TextKit puts it.
    pub fn baseline(&self, row: usize) -> Pixels {
        let r = &self.laid.rows[row];
        if self.natural {
            self.ascent.max(r.box_tall - self.descent).max(r.math_above)
        } else {
            r.height - self.descent.max(r.math_depth)
        }
    }

    pub fn row_of(&self, disp: usize) -> usize {
        self.laid.row_of(disp)
    }

    pub fn caret_row(&self, disp: usize, upstream: bool) -> usize {
        self.laid.caret_row(disp, upstream)
    }

    /// Distance from the text column; negative inside hung syntax.
    pub fn x_of(&self, disp: usize, row: usize) -> Pixels {
        self.laid.x_in_row(disp, row)
    }

    pub fn index_at(&self, row: usize, x: Pixels) -> usize {
        self.laid.index_at(row, x)
    }

    pub fn extent_at(&self, row: usize, x: Pixels) -> usize {
        self.laid.extent_at(row, x)
    }

    pub fn row_count(&self) -> usize {
        self.laid.rows.len()
    }

    pub fn height(&self, d: &DisplayLine) -> Pixels {
        d.top + self.laid.height + d.bottom
    }

    /// The row at `y`, measured from the line's text top.
    pub fn row_at(&self, y: Pixels) -> usize {
        let rows = &self.laid.rows;
        rows.iter().rposition(|r| r.y <= y).unwrap_or(0)
    }
}

pub fn shape(d: &DisplayLine, width: Pixels, window: &Window) -> Rc<Shaped> {
    let ts = window.text_system();
    let font_id = ts.resolve_font(&d.base_font);
    let ascent = ts.ascent(font_id, d.font_size);
    let descent = ts.descent(font_id, d.font_size).abs();
    let text = d.text.as_ref();
    let wrap_width = (width - d.margin * 2. - d.indent - d.tail).max(px(40.));
    let runs: Vec<RunInput> = d
        .spans
        .iter()
        .map(|s| {
            // A picture stands on the baseline, with the line's descent below it.
            let media_box = s.media.as_ref().map(|m| {
                let g = m.geom(f32::from(wrap_width));
                size(px(g.box_width), px(g.box_height) + descent)
            });
            RunInput {
                text: &text[s.range.clone()],
                font: &s.font,
                size: s.size,
                color: s.color,
                kern_after: s.kern_after,
                box_size: media_box.or(s.math.as_ref().map(|m| match &m.rendered {
                    Some(r) => size(px(r.width), px(r.height)),
                    None => size(px(0.), px(0.)),
                })),
                box_mode: match &s.media {
                    Some(m) if m.fills_line => BoxMode::BaselineFill,
                    Some(_) => BoxMode::Baseline,
                    None => BoxMode::Formula {
                        depth: px(s.math.as_ref().and_then(|m| m.rendered.as_ref()).map_or(0., |r| r.depth)),
                    },
                },
                unbroken: s.unbroken.then_some(wrap_width),
            }
        })
        .collect();
    let hang = d.hang;
    let (natural, fixed, spacing) = match d.metric {
        LineMetric::Fixed(h) => (false, px(h), px(0.)),
        LineMetric::Natural { spacing } => (true, ascent + descent + px(spacing), px(spacing)),
    };
    let laid = Laid::shape(
        text,
        &runs,
        |run_x| if hang > 0 { hang_x(d, run_x) } else { px(0.) },
        wrap_width,
        d.wrap,
        |_, boxes| boxes.height(fixed, spacing, descent),
        window,
    );
    Rc::new(Shaped { laid, ascent, descent, natural, column: wrap_width })
}

/// Where a box goes, within a line drawn from `text_origin`: its corner, and how
/// it divides up.
pub fn media_box(
    shaped: &Shaped,
    span_index: usize,
    span: &Span,
    media: &InlineMedia,
    text_origin: Point<Pixels>,
) -> (Point<Pixels>, media::MediaGeom) {
    let geom = media.geom(f32::from(shaped.column));
    let laid = &shaped.laid;
    let row_ix = laid.row_holding(span.range.start);
    let row = laid.rows[row_ix];
    let base = text_origin.y + row.y + shaped.baseline(row_ix);
    let x = text_origin.x + laid.run_x[span_index] - row.x0;
    (point(x, base - px(geom.box_height)), geom)
}

/// A picture on the page: rounded and captioned if it's still, playing on if it
/// isn't. Nothing is drawn but its place until its pixels arrive.
fn paint_image_box(media: &InlineMedia, geom: media::MediaGeom, origin: Point<Pixels>, ui: &RefCell<VideoUi>, window: &mut Window, cx: &mut App) {
    let MediaKind::Image { animated } = media.info.kind else { return };
    let c = t();
    let b = Bounds::new(origin, size(px(geom.width), px(geom.height)));
    let radius = if animated { Corners::default() } else { Corners::all(px(media::IMAGE_RADIUS)) };
    match media::with_store(|s| s.frame(&media.info.key)) {
        Some((image, frame, left)) => {
            let _ = window.paint_image(b, b, radius, image, frame, false);
            if animated {
                let ui = ui.borrow();
                let due = std::time::Instant::now() + std::time::Duration::from_secs_f32(left.clamp(0.02, 60.));
                if ui.next_frame.get().is_none_or(|at| due < at) {
                    ui.next_frame.set(Some(due));
                }
            }
        }
        None => window.paint_quad(PaintQuad {
            bounds: b,
            corner_radii: radius,
            background: c.quiet_surface.into(),
            border_widths: Edges::default(),
            border_color: transparent_black(),
            border_style: BorderStyle::default(),
        }),
    }
    if media.has_caption() {
        video::paint_caption(
            media.caption.trim(),
            point(origin.x, origin.y + px(geom.height + media::CAPTION_GAP)),
            px(geom.width),
            TruncateFrom::End,
            window,
            cx,
        );
    }
}

/// Where the text starts after hung syntax.
fn hang_x(d: &DisplayLine, run_x: &[Pixels]) -> Pixels {
    d.spans
        .iter()
        .position(|s| s.range.start >= d.hang)
        .and_then(|i| run_x.get(i).copied())
        .unwrap_or_else(|| run_x.last().copied().unwrap_or_default())
}

/// How much of the page stays clear around the caret when it scrolls into view.
const CARET_ROOM: f32 = 30.;

pub struct Painted {
    pub first_line: usize,
    pub n_lines: usize,
    pub bounds: Bounds<Pixels>,
    pub text: Option<(Rc<DisplayLine>, Rc<Shaped>)>,
}

/// A draft's two faint lines.
pub const DRAFT_TITLE: &str = "Title";
pub const DRAFT_HINT: &str = "Start typing — the note is saved once you write something.";

/// The font the caret is measured by: the one of the text before it, or the
/// line's own at its start. Under 80% of the body size it's the body size, so it
/// never shrinks beside something small.
pub fn caret_font(d: &DisplayLine, caret: usize, body: Pixels) -> (Font, Pixels) {
    let before = d
        .spans
        .iter()
        .rfind(|s| s.range.start < caret && s.range.end >= caret && s.math.is_none() && s.media.is_none());
    let (font, size) = before.map_or((d.base_font.clone(), d.font_size), |s| (s.font.clone(), s.size));
    (font, if size < body * 0.8 { body } else { size })
}

/// The caret's top and height in a row: as tall as the font, standing on the
/// baseline, inside the row, whole points. A font as tall as the row fills it.
pub fn caret_box(baseline: f32, ascent: f32, descent: f32, row_top: f32, row_height: f32) -> (f32, f32) {
    let height = (ascent + descent).ceil();
    if height >= row_height {
        return (row_top, row_height);
    }
    let top = (baseline - ascent).max(row_top).min(row_top + row_height - height);
    (top.floor(), height)
}

/// The rectangle `request_autoscroll` is given so the list moves its content by
/// `shift` (down if positive), with no padding of its own: the viewport's top or
/// bottom edge is pushed past by exactly that much.
pub fn shifting_rect(view: Bounds<Pixels>, shift: Pixels) -> Bounds<Pixels> {
    let y = if shift > px(0.) { view.top() - shift } else { view.bottom() - shift - px(1.) };
    Bounds::new(point(view.left(), y), size(px(1.), px(1.)))
}

/// Where the caret is drawn, when something outside the editor draws it: the
/// window's caret layer. Blinking then redraws only that layer, not the note.
#[derive(Default)]
pub struct CaretSlot {
    pub rect: std::cell::Cell<Option<Bounds<Pixels>>>,
    pub on: std::cell::Cell<bool>,
    pub color: std::cell::Cell<Hsla>,
}

pub struct LineEl {
    pub line: usize,
    pub display: Rc<DisplayLine>,
    pub caret: Option<usize>,
    /// Where the caret is on this line, drawn or not (it blinks).
    pub caret_head: Option<usize>,
    /// The caret stands at the end of a wrapped row, not at the start of the next.
    pub caret_upstream: bool,
    pub caret_slot: Option<Rc<CaretSlot>>,
    pub autoscroll: bool,
    /// Visible bytes selected, and whether the selection runs on past the line.
    pub selection: Option<(Range<usize>, bool)>,
    /// A search hit flashed on this line: display offsets, and the colour at
    /// its strength right now.
    pub flash: Option<(Range<usize>, Hsla)>,
    pub hovered: Option<Range<usize>>,
    pub painted: Rc<RefCell<Vec<Painted>>>,
    pub shaped: Rc<RefCell<Option<(Pixels, Rc<Shaped>)>>>,
    pub symbols: Rc<RefCell<SymbolCache>>,
    pub videos: Rc<RefCell<VideoUi>>,
    /// The body size, which the caret never measures under.
    pub body_size: Pixels,
    /// Where the row holding `caret_head` (or the first) must stand after this
    /// layout, and the list's viewport to move it in.
    pub keep_at: Option<(Pixels, Bounds<Pixels>)>,
    /// The open slash menu's slash, if it's on this line: visible offset, and
    /// where to report its row's box.
    pub slash: Option<(usize, Rc<Cell<Option<Bounds<Pixels>>>>)>,
    /// An untouched draft: the body font and size, for the hint under the title,
    /// and the spacing the title line has after it.
    pub placeholder: Option<(Font, Pixels, Pixels)>,
}

impl IntoElement for LineEl {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn rounded(
    bounds: Bounds<Pixels>,
    color: Hsla,
    radius: f32,
    top: bool,
    bottom: bool,
    window: &mut Window,
) {
    let r = px(radius);
    window.paint_quad(PaintQuad {
        bounds,
        corner_radii: Corners {
            top_left: if top { r } else { px(0.) },
            top_right: if top { r } else { px(0.) },
            bottom_left: if bottom { r } else { px(0.) },
            bottom_right: if bottom { r } else { px(0.) },
        },
        background: color.into(),
        border_widths: Edges::default(),
        border_color: transparent_black(),
        border_style: BorderStyle::default(),
    });
}

/// Callout type → symbol, as `CalloutKind.symbolName`.
pub fn callout_symbol(raw: &str) -> &'static str {
    use fumi_ui::theme::CalloutKind as K;
    match raw.to_lowercase().as_str() {
        "success" | "check" | "done" => "checkmark.circle",
        "question" | "help" | "faq" => "questionmark.circle",
        "todo" => "checklist",
        "bug" => "ladybug",
        _ => match K::of(raw) {
            K::Note => "info.circle",
            K::Tip => "lightbulb",
            K::Important => "exclamationmark.bubble",
            K::Warning => "exclamationmark.triangle",
            K::Danger => "xmark.octagon",
            K::Plain => "quote.opening",
        },
    }
}

impl Element for LineEl {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let display = self.display.clone();
        let slot = self.shaped.clone();
        let id = window.request_measured_layout(style, move |known, avail, window, _| {
            let width = known.width.unwrap_or(match avail.width {
                AvailableSpace::Definite(w) => w,
                _ => px(720.),
            });
            let shaped = shape(&display, width, window);
            let h = shaped.height(&display);
            *slot.borrow_mut() = Some((width, shaped));
            size(width, h)
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
    ) {
        if !self.autoscroll && self.keep_at.is_none() && self.slash.is_none() {
            return;
        }
        let d = &self.display;
        let cached = self.shaped.borrow().clone();
        let shaped = match cached {
            Some((w, s)) if w == bounds.size.width => s,
            _ => shape(d, bounds.size.width, window),
        };
        if let Some((disp, cell)) = &self.slash {
            let row = shaped.row_of(*disp);
            let r = shaped.laid.rows[row];
            let x = bounds.left() + d.margin + d.indent + shaped.x_of(*disp, row);
            cell.set(Some(Bounds::new(point(x, bounds.top() + d.top + r.y), size(px(1.), r.height))));
        }
        let row = shaped.caret_row(self.caret_head.unwrap_or(0), self.caret_upstream);
        let top = bounds.top() + d.top + shaped.laid.rows[row].y;
        let height = shaped.laid.rows[row].height;
        // Heights above may have changed: put this row back where it stood.
        if let Some((want, view)) = self.keep_at {
            let shift = want - top;
            if f32::from(shift).abs() > 0.5 && view.size.height > px(0.) {
                window.request_autoscroll(shifting_rect(view, shift));
                return;
            }
        }
        if self.autoscroll {
            // With the same room around it as the page has at its ends.
            let room = px(CARET_ROOM);
            window.request_autoscroll(Bounds::new(point(bounds.left(), top - room), size(px(1.), height + room * 2.)));
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let d = self.display.clone();
        let c = t();
        let cached = self.shaped.borrow().clone();
        let shaped = match cached {
            Some((w, s)) if w == bounds.size.width => s,
            _ => shape(&d, bounds.size.width, window),
        };
        let laid = &shaped.laid;
        let column_x = bounds.left() + d.margin;
        let column_w = bounds.size.width - d.margin * 2.;
        let text_x = column_x + d.indent;
        let top_y = bounds.top() + d.top;
        let origin = point(text_x, top_y);
        let ts = window.text_system().clone();

        // Block boxes reach into the spacing around their text.
        if let Some(b) = d.boxed {
            let pad = px(BLOCK_PADDING);
            let y0 = if b.first { top_y - pad } else { bounds.top() };
            let y1 = if b.last { top_y + laid.height + pad } else { bounds.bottom() };
            rounded(
                Bounds::from_corners(point(column_x, y0), point(column_x + column_w, y1)),
                b.fill,
                BLOCK_RADIUS,
                b.first,
                b.last,
                window,
            );
        }

        // A link under the pointer.
        if let Some(range) = &self.hovered {
            for (i, row) in laid.rows.iter().enumerate() {
                let (a, e) = (range.start.max(row.start), range.end.min(row.end));
                if a >= e {
                    continue;
                }
                let base = top_y + row.y + shaped.baseline(i);
                let x0 = laid.x_for_index(a) - row.x0;
                let x1 = laid.x_for_index(e) - row.x0;
                rounded(
                    Bounds::from_corners(
                        point(text_x + x0 - px(3.), base - shaped.ascent - px(2.)),
                        point(text_x + x1 + px(3.), base + shaped.descent + px(2.)),
                    ),
                    c.text.opacity(0.07),
                    4.,
                    true,
                    true,
                    window,
                );
            }
        }

        // Pills and highlights.
        for (r, span) in d.spans.iter().enumerate() {
            let Some(fill) = span.fill else { continue };
            let font_id = ts.resolve_font(&span.font);
            let asc = ts.ascent(font_id, span.size);
            let desc = ts.descent(font_id, span.size).abs();
            for (i, row) in laid.rows.iter().enumerate() {
                let (a, e) = (span.range.start.max(row.start), span.range.end.min(row.end));
                if a >= e {
                    continue;
                }
                // The trailing kerning is the pill's own padding.
                let end_x = if e == span.range.end { laid.run_x[r + 1] } else { laid.x_for_index(e) };
                let x0 = laid.x_for_index(a) - row.x0 - px(fill.pad);
                let x1 = end_x - row.x0 - if fill.pad > 0. { px(0.) } else { span.kern_after };
                let base = top_y + row.y + shaped.baseline(i);
                rounded(
                    Bounds::from_corners(
                        point(text_x + x0, base - asc - px(fill.inset)),
                        point(text_x + x1, base + desc + px(fill.inset)),
                    ),
                    fill.color,
                    fill.radius,
                    true,
                    true,
                    window,
                );
            }
        }

        // Selection, over the boxes and under the text.
        if let Some((range, past_end)) = &self.selection {
            let right = column_x + column_w - d.tail;
            let last_row = laid.rows.len() - 1;
            for (i, row) in laid.rows.iter().enumerate() {
                let row_end = row.end.max(row.start);
                if range.end < row.start || range.start > row_end {
                    continue;
                }
                if range.start == range.end && !*past_end {
                    continue;
                }
                // Starting where a row wraps, it starts on the row below.
                if range.start == row_end && laid.rows.get(i + 1).is_some_and(|next| !next.fill) {
                    continue;
                }
                let (a, e) = (range.start.max(row.start), range.end.min(row_end));
                let x0 = text_x + laid.x_for_index(a) - row.x0;
                // Selection that continues on reaches the column's edge, as NSTextView draws it.
                let continues = (i < last_row && range.end > row_end) || (i == last_row && *past_end);
                let x1 = if continues { right } else { text_x + laid.x_for_index(e) - row.x0 };
                if x1 > x0 {
                    window.paint_quad(fill(
                        Bounds::from_corners(
                            point(x0, top_y + row.y),
                            point(x1, top_y + row.y + row.height),
                        ),
                        c.selection,
                    ));
                }
            }
        }

        // A search hit, flashed: same place as the selection, rounded, tinted
        // with the accent and fading out.
        if let Some((range, color)) = &self.flash {
            for row in laid.rows.iter() {
                let row_end = row.end.max(row.start);
                if range.end <= row.start || range.start >= row_end {
                    continue;
                }
                let (a, e) = (range.start.max(row.start), range.end.min(row_end));
                let x0 = text_x + laid.x_for_index(a) - row.x0 - px(2.);
                let x1 = text_x + laid.x_for_index(e) - row.x0 + px(2.);
                if x1 > x0 {
                    rounded(
                        Bounds::from_corners(
                            point(x0, top_y + row.y + px(2.)),
                            point(x1, top_y + row.y + row.height - px(2.)),
                        ),
                        *color,
                        4.,
                        true,
                        true,
                        window,
                    );
                }
            }
        }

        // Math boxes.
        for (r, span) in d.spans.iter().enumerate() {
            let Some(rendered) = span.math.as_ref().and_then(|m| m.rendered.clone()) else { continue };
            let row_ix = laid.row_holding(span.range.start);
            let row = laid.rows[row_ix];
            let base = top_y + row.y + shaped.baseline(row_ix);
            let x = text_x + laid.run_x[r] - row.x0;
            let h = px(rendered.height);
            // Its baseline on the text's.
            let b = Bounds::new(point(x, base + px(rendered.depth) - h), size(px(rendered.width), h));
            let _ = window.paint_image(
                b,
                b,
                Corners::default(),
                rendered.image.clone(),
                0,
                false,
            );
        }

        // Images and videos.
        for (r, span) in d.spans.iter().enumerate() {
            let Some(media) = &span.media else { continue };
            let (at, geom) = media_box(&shaped, r, span, media, origin);
            match media.info.kind {
                MediaKind::Image { .. } => paint_image_box(media, geom, at, &self.videos, window, cx),
                MediaKind::Video { .. } => video::paint_video(self.line, media, geom, at, &self.videos, &self.symbols, window, cx),
            }
            // Selected along with the text around it.
            if let Some((range, _)) = &self.selection
                && range.start <= span.range.start
                && range.end >= span.range.end
            {
                window.paint_quad(fill(Bounds::new(at, size(px(geom.width), px(geom.height))), c.selection));
            }
        }

        // Text.
        let shaped_ref = shaped.clone();
        laid.paint_glyphs(origin, move |row| shaped_ref.baseline(row), window);

        // Underlines and strikes.
        for (r, span) in d.spans.iter().enumerate() {
            if span.underline.is_none() && span.strike.is_none() {
                continue;
            }
            let font_id = ts.resolve_font(&span.font);
            let x_height = ts.x_height(font_id, span.size);
            for (i, row) in laid.rows.iter().enumerate() {
                let (a, e) = (span.range.start.max(row.start), span.range.end.min(row.end));
                if a >= e {
                    continue;
                }
                let end_x = if e == span.range.end { laid.run_x[r + 1] - span.kern_after } else { laid.x_for_index(e) };
                let x0 = text_x + laid.x_for_index(a) - row.x0;
                // A unit is a point wider than its line.
                let x1 = text_x + end_x - row.x0 - if span.unbroken { px(1.) } else { px(0.) };
                let base = top_y + row.y + shaped.baseline(i);
                if let Some(u) = span.underline {
                    let y = (base + px(u.below)).round();
                    // A link under the pointer darkens its line.
                    let over = self.hovered.as_ref().is_some_and(|h| h.start < span.range.end && span.range.start < h.end);
                    let color = if over { c.text.opacity(0.7) } else { u.color };
                    if u.dashed {
                        let mut x = x0;
                        while x < x1 {
                            let w = px(3.).min(x1 - x);
                            window.paint_quad(fill(Bounds::new(point(x, y), size(w, px(1.))), color));
                            x += px(5.);
                        }
                    } else {
                        window.paint_quad(fill(Bounds::new(point(x0, y), size(x1 - x0, px(1.))), color));
                    }
                }
                if let Some(color) = span.strike {
                    let y = (base - x_height / 2.).round();
                    window.paint_quad(fill(Bounds::new(point(x0, y), size(x1 - x0, px(1.))), color));
                }
            }
        }

        // Misspelled words: red dots under them, as NSTextView marks them.
        for range in &d.misspelled {
            for (i, row) in laid.rows.iter().enumerate() {
                let (a, e) = (range.start.max(row.start), range.end.min(row.end));
                if a >= e {
                    continue;
                }
                let base = top_y + row.y + shaped.baseline(i);
                let mut x = text_x + laid.x_for_index(a) - row.x0;
                let end = text_x + laid.x_for_index(e) - row.x0;
                let dot = px(crate::spell::DOT);
                while x + dot <= end {
                    rounded(Bounds::new(point(x, (base + px(2.)).round()), size(dot, dot)), crate::spell::color(c.dark), crate::spell::DOT / 2., true, true, window);
                    x += px(crate::spell::DOT_STEP);
                }
            }
        }

        // Quote bars, one per level.
        if d.quote_depth > 0 {
            for level in 0..d.quote_depth {
                let x = column_x + px(level as f32 * QUOTE_INDENT);
                let color = if level == 0 { c.text } else { c.nested_quote_bar };
                window.paint_quad(fill(
                    Bounds::from_corners(
                        point(x, top_y + px(2.)),
                        point(x + px(3.), top_y + laid.height),
                    ),
                    color,
                ));
            }
        }

        // What stands in for a list item's syntax.
        if let Some(glyph) = &d.list {
            let base = top_y + shaped.baseline(0);
            let font_id = ts.resolve_font(&d.base_font);
            let cap = ts.cap_height(font_id, d.font_size);
            let center_y = base - cap / 2.;
            let slot = text_x - px(LIST_SLOT);
            let body = f32::from(d.font_size);
            match glyph {
                ListGlyph::Bullet(shape) => {
                    let cx_ = slot + px(7.);
                    let at = |w: f32| Bounds::new(point(cx_ - px(w / 2.), center_y - px(w / 2.)), size(px(w), px(w)));
                    match shape {
                        BulletShape::Disc => {
                            let w = 5f32.max((body * 0.33).round());
                            rounded(at(w), c.text, w / 2., true, true, window);
                        }
                        BulletShape::Ring => {
                            let w = 5f32.max((body * 0.33).round());
                            window.paint_quad(PaintQuad {
                                bounds: at(w),
                                corner_radii: Corners::all(px(w / 2.)),
                                background: transparent_black().into(),
                                border_widths: Edges::all(px(1.2)),
                                border_color: c.text,
                                border_style: BorderStyle::default(),
                            });
                        }
                        BulletShape::Square => {
                            let w = 4f32.max((body * 0.28).round());
                            window.paint_quad(fill(at(w), c.text));
                        }
                    }
                }
                ListGlyph::Ordered(label) => {
                    let mut f = ui_font();
                    f.features = font_features_tnum();
                    let run = TextRun {
                        len: label.len(),
                        font: f,
                        color: c.text,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(label.clone(), d.font_size, &[run], None);
                    let x = text_x - px(6.) - line.width;
                    let row_h = laid.rows[0].height;
                    let y = base - (row_h - line.ascent - line.descent) / 2. - line.ascent;
                    let _ = line.paint(point(x, y), row_h, TextAlign::Left, None, window, cx);
                }
                ListGlyph::Task { checked } => {
                    let box_ = Bounds::new(point(slot + px(1.), center_y - px(8.)), size(px(16.), px(16.)));
                    if *checked {
                        rounded(box_, c.accent, 4., true, true, window);
                        paint_check(box_, window);
                    } else {
                        window.paint_quad(PaintQuad {
                            bounds: box_,
                            corner_radii: Corners::all(px(4.)),
                            background: transparent_black().into(),
                            border_widths: Edges::all(px(1.5)),
                            border_color: c.checkbox_border,
                            border_style: BorderStyle::default(),
                        });
                    }
                }
            }
        }

        // A horizontal rule across the column.
        if d.rule {
            let y = (top_y + laid.height / 2.).round();
            window.paint_quad(fill(Bounds::new(point(column_x, y), size(column_w, px(1.))), c.rule));
        }

        // Callout icon.
        if let Some((raw, color)) = &d.callout_icon {
            let scale = window.scale_factor();
            let image = self.symbols.borrow_mut().get(callout_symbol(raw), 13., native::weight::SEMIBOLD, *color, scale);
            if let Some((image, (w, h))) = image {
                let base = top_y + shaped.baseline(0);
                let font_id = ts.resolve_font(&d.base_font);
                let cy = base - ts.cap_height(font_id, d.font_size) / 2.;
                let x = column_x + px(14. + (16. - w) / 2.);
                let b = Bounds::new(point(x, cy - px(h / 2.)), size(px(w), px(h)));
                let _ = window.paint_image(
                    b,
                    b,
                    Corners::default(),
                    image,
                    0,
                    false,
                );
            }
        }

        // The code block's language, top right.
        if let Some((language, above)) = &d.code_language {
            let run = TextRun {
                len: language.len(),
                font: ui_font(),
                color: c.text3,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(language.clone(), px(12.), &[run], None);
            let x = column_x + column_w - px(12.) - line.width;
            let _ = line.paint(point(x, top_y - px(*above)), px(15.), TextAlign::Left, None, window, cx);
        }

        // A draft's faint title, right past the `# `, and the hint under it. As
        // the Swift draws them: over the line's whole rectangle, spacing included.
        if let Some((body_font, body_size, after)) = &self.placeholder {
            let run = |len: usize, font: Font| TextRun {
                len,
                font,
                color: c.faint_icon,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line_height = laid.rows[0].height + *after;
            let title = window.text_system().shape_line(
                DRAFT_TITLE.into(),
                d.font_size,
                &[run(DRAFT_TITLE.len(), d.base_font.clone())],
                None,
            );
            let title_box = title.ascent + title.descent;
            let x = text_x + shaped.x_of(d.text.len(), 0);
            let y = top_y + ((line_height - title_box) / 2.).max(px(0.));
            let _ = title.paint(point(x, y), title_box, TextAlign::Left, None, window, cx);
            let hint = window.text_system().shape_line(
                DRAFT_HINT.into(),
                *body_size,
                &[run(DRAFT_HINT.len(), body_font.clone())],
                None,
            );
            let hint_box = hint.ascent + hint.descent;
            let _ = hint.paint(point(text_x, top_y + line_height + px(14.)), hint_box, TextAlign::Left, None, window, cx);
        }

        if let Some(caret) = self.caret {
            let row = shaped.caret_row(caret, self.caret_upstream);
            let x = shaped.x_of(caret, row);
            let r = laid.rows[row];
            let base = top_y + r.y + shaped.baseline(row);
            let (font, font_size) = caret_font(&d, caret, self.body_size);
            let font_id = ts.resolve_font(&font);
            let asc = ts.ascent(font_id, font_size);
            let desc = ts.descent(font_id, font_size).abs();
            let (y, h) = caret_box(f32::from(base), f32::from(asc), f32::from(desc), f32::from(top_y + r.y), f32::from(r.height));
            let caret_rect = Bounds::new(point(text_x + x - px(0.5), px(y)), size(px(1.5), px(h)));
            match &self.caret_slot {
                Some(slot) => {
                    slot.rect.set(Some(caret_rect));
                    slot.color.set(c.accent);
                }
                None => window.paint_quad(fill(caret_rect, c.accent)),
            }
        }

        self.painted.borrow_mut().push(Painted {
            first_line: self.line,
            n_lines: 1,
            bounds,
            text: Some((self.display.clone(), shaped)),
        });
    }
}

fn font_features_tnum() -> FontFeatures {
    FontFeatures(std::sync::Arc::new(vec![("tnum".to_string(), 1)]))
}

/// The white tick in a checked box.
fn paint_check(b: Bounds<Pixels>, window: &mut Window) {
    let (l, r) = (b.left(), b.right());
    let (t_, bt) = (b.top(), b.bottom());
    let mid = (t_ + bt) / 2.;
    let mut path = PathBuilder::stroke(px(2.));
    path.move_to(point(l + px(4.), mid - px(0.5)));
    path.line_to(point(l + px(7.), mid + px(2.5)));
    path.line_to(point(r - px(3.5), t_ + px(5.)));
    if let Ok(path) = path.build() {
        window.paint_path(path, white());
    }
}

/// A folded block. It records the bounds it really got, at paint time, so clicks
/// can find it. A child overlay asked to fill the block came back several times
/// too tall, and a list may prepaint an item more than once in a frame.
pub struct BlockEl {
    pub child: AnyElement,
    pub first_line: usize,
    pub n_lines: usize,
    pub selected: bool,
    pub painted: Rc<RefCell<Vec<Painted>>>,
}

impl IntoElement for BlockEl {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for BlockEl {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
        if self.selected {
            window.paint_quad(fill(bounds, t().selection));
        }
        self.painted.borrow_mut().push(Painted {
            first_line: self.first_line,
            n_lines: self.n_lines,
            bounds,
            text: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn the_caret_stands_on_the_baseline_inside_its_row() {
        // A 27 pt body row: font height, on the baseline.
        assert_eq!(caret_box(23., 15.2, 3.9, 0., 27.), (7., 20.));
        // Pushed back inside when the font would stick out above or below.
        assert_eq!(caret_box(10., 15., 4., 0., 27.), (0., 19.));
        assert_eq!(caret_box(26., 15., 4., 0., 27.), (8., 19.));
        // As tall as the row: the row.
        assert_eq!(caret_box(20., 20., 8., 3., 27.), (3., 27.));
    }

    #[core::prelude::v1::test]
    fn a_shift_pushes_past_the_viewport_edge_by_itself() {
        let view = Bounds::new(point(px(0.), px(100.)), size(px(10.), px(500.)));
        let down = shifting_rect(view, px(8.));
        assert_eq!(view.top() - down.top(), px(8.));
        let up = shifting_rect(view, px(-8.));
        assert_eq!(up.bottom() - view.bottom(), px(8.));
    }
}
