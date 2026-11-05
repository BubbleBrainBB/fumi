//! The custom element for a text line: shapes with GPUI's text system, paints
//! glyphs, selection, caret and block decoration, and records where it landed
//! so hit testing and the input method can find it.

use crate::display::*;
use gpui::*;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    pub x0: Pixels,
}

pub struct Shaped {
    pub line: WrappedLine,
    pub rows: Vec<Row>,
    pub baseline_shift: Pixels,
}

impl Shaped {
    pub fn row_of(&self, disp: usize) -> usize {
        self.rows
            .iter()
            .rposition(|r| r.start <= disp)
            .unwrap_or(0)
    }

    pub fn x_of(&self, disp: usize, row: usize) -> Pixels {
        self.line.unwrapped_layout.x_for_index(disp) - self.rows[row].x0
    }

    pub fn index_at(&self, row: usize, x: Pixels) -> usize {
        let r = self.rows[row.min(self.rows.len() - 1)];
        self.line
            .unwrapped_layout
            .closest_index_for_x(r.x0 + x)
            .clamp(r.start, r.end)
    }
}

pub fn shape(d: &DisplayLine, width: Pixels, window: &Window) -> Rc<Shaped> {
    let wrap = (width - d.margin * 2. - d.indent).max(px(40.));
    let line = window
        .text_system()
        .shape_text(d.text.clone(), d.font_size, &d.runs, Some(wrap), None)
        .ok()
        .and_then(|mut lines| lines.pop())
        .unwrap_or_default();
    let layout = &line.unwrapped_layout;
    let len = d.text.len();
    let mut rows = Vec::with_capacity(line.wrap_boundaries.len() + 1);
    let mut start = 0;
    for b in &line.wrap_boundaries {
        let at = layout.runs[b.run_ix].glyphs[b.glyph_ix].index;
        rows.push(Row { start, end: at, x0: layout.x_for_index(start) });
        start = at;
    }
    rows.push(Row { start, end: len, x0: layout.x_for_index(start) });

    // Core Text reports the ascent of whatever fonts a line happens to use, so
    // a Chinese line and a Latin one would sit at different heights inside the
    // same fixed line. Pin every line's baseline to the same place instead.
    let (asc, desc) = (layout.ascent, layout.descent);
    let natural = (d.line_height - asc - desc) / 2. + asc;
    let wanted = d.line_height / 2. + d.font_size * 0.35;
    let baseline_shift = if d.text.is_empty() { px(0.) } else { wanted - natural };
    Rc::new(Shaped { line, rows, baseline_shift })
}

pub struct Painted {
    pub first_line: usize,
    pub n_lines: usize,
    pub bounds: Bounds<Pixels>,
    pub text: Option<(Rc<DisplayLine>, Rc<Shaped>)>,
}

pub struct LineEl {
    pub line: usize,
    pub display: Rc<DisplayLine>,
    pub caret: Option<usize>,
    /// Where the caret is on this line, drawn or not (it blinks).
    pub caret_head: Option<usize>,
    pub autoscroll: bool,
    pub selection: Option<(Range<usize>, bool)>,
    pub painted: Rc<RefCell<Vec<Painted>>>,
    pub shaped: Rc<RefCell<Option<(Pixels, Rc<Shaped>)>>>,
}

impl IntoElement for LineEl {
    type Element = Self;
    fn into_element(self) -> Self {
        self
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
            let h = display.height(shaped.rows.len());
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
        let stale = self
            .shaped
            .borrow()
            .as_ref()
            .is_none_or(|(w, _)| *w != bounds.size.width);
        if stale {
            let s = shape(&self.display, bounds.size.width, window);
            *self.shaped.borrow_mut() = Some((bounds.size.width, s));
        }
        if self.autoscroll
            && let Some(head) = self.caret_head
        {
            let d = &self.display;
            let shaped = self.shaped.borrow().as_ref().unwrap().1.clone();
            let row = shaped.row_of(head);
            let y = bounds.origin.y + d.top + d.line_height * row as f32;
            // Keep a line of air around the caret where there's room.
            let air = d.line_height;
            window.request_autoscroll(Bounds::new(
                point(bounds.origin.x, y - air),
                size(bounds.size.width, d.line_height + air * 2.),
            ));
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
        let d = &self.display;
        let shaped = self.shaped.borrow().as_ref().unwrap().1.clone();
        let lh = d.line_height;
        let page = Bounds::new(
            bounds.origin + point(d.margin, px(0.)),
            size(bounds.size.width - d.margin * 2., bounds.size.height),
        );
        let origin = page.origin + point(d.indent, d.top);
        let text_origin = origin + point(px(0.), shaped.baseline_shift);

        match d.deco {
            Deco::Quote => {
                window.paint_quad(fill(
                    Bounds::new(page.origin + point(px(2.), px(3.)), size(px(2.), page.size.height - px(6.))),
                    hairline(),
                ));
            }
            Deco::Rule => {
                let y = page.origin.y + page.size.height / 2.;
                window.paint_quad(fill(
                    Bounds::new(point(page.origin.x, y), size(page.size.width, px(1.))),
                    hairline(),
                ));
            }
            Deco::Code { first, last } => {
                let radius = px(6.);
                let mut q = fill(page, tint());
                q.corner_radii = Corners {
                    top_left: if first { radius } else { px(0.) },
                    top_right: if first { radius } else { px(0.) },
                    bottom_left: if last { radius } else { px(0.) },
                    bottom_right: if last { radius } else { px(0.) },
                };
                window.paint_quad(q);
            }
            Deco::None => {}
        }

        if let Some((sel, past_end)) = &self.selection {
            for (i, r) in shaped.rows.iter().enumerate() {
                let a = sel.start.max(r.start);
                let b = sel.end.min(r.end);
                let tail = *past_end && i + 1 == shaped.rows.len();
                if a < b || (tail && a == b) || (a == b && d.text.is_empty()) {
                    let x0 = shaped.x_of(a, i);
                    let mut x1 = shaped.x_of(b, i);
                    if tail {
                        x1 += px(6.);
                    }
                    window.paint_quad(fill(
                        Bounds::new(
                            origin + point(x0, lh * i as f32),
                            size((x1 - x0).max(px(1.)), lh),
                        ),
                        selection(),
                    ));
                }
            }
        }

        let _ = shaped
            .line
            .paint_background(text_origin, lh, TextAlign::Left, None, window, cx);
        let _ = shaped.line.paint(text_origin, lh, TextAlign::Left, None, window, cx);

        if let Some(mark) = &d.gutter {
            let run = TextRun {
                len: mark.len(),
                font: body_font(),
                color: ink2(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let g = window.text_system().shape_line(mark.clone(), d.font_size, &[run], None);
            let x = page.origin.x + d.indent - g.width - px(6.);
            let _ = g.paint(point(x, text_origin.y), lh, TextAlign::Left, None, window, cx);
        }

        if let Some(caret) = self.caret {
            let row = shaped.row_of(caret);
            let x = shaped.x_of(caret, row);
            let h = (d.font_size * 1.3).min(lh);
            let y = origin.y + lh * row as f32 + (lh - h) / 2.;
            window.paint_quad(fill(
                Bounds::new(point(origin.x + x - px(0.5), y), size(px(1.5), h)),
                ink(),
            ));
        }

        self.painted.borrow_mut().push(Painted {
            first_line: self.line,
            n_lines: 1,
            bounds,
            text: Some((self.display.clone(), shaped)),
        });
    }
}
