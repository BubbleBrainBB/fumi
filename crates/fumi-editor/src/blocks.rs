//! Blocks drawn in place of their source: a table grid, typeset math, the
//! frontmatter panel. Each renders from the lines it covers and never writes
//! back. Ports of `TableAttachment`, `MathAttachment` and `FrontmatterAttachment`.

use crate::display::*;
use crate::textlayout::*;
use fumi_core::frontmatter::{Frontmatter, Value};
use fumi_core::markdown::*;
use fumi_ui::theme::{t, text_size};
use gpui::prelude::*;
use gpui::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
pub struct BlockCache {
    tables: HashMap<u64, Rc<TableModel>>,
    math: HashMap<u64, Option<Rc<Formula>>>,
}

impl BlockCache {
    /// Colors are baked into cached blocks; a theme change drops them.
    pub fn clear(&mut self) {
        self.tables.clear();
        self.math.clear();
    }
}

fn key(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

// Tables ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

pub struct TableModel {
    /// Text width of each column.
    widths: Vec<f32>,
    rows: Vec<Vec<Cell>>,
    aligns: Vec<Align>,
    numeric: Vec<bool>,
}

struct Cell {
    text: SharedString,
    spans: Vec<Span>,
    /// The cell's own font, whatever its spans do, for the height of its lines.
    base: Rc<Font>,
    size: Pixels,
}

const BORDER: f32 = 1.;
const PAD_H: f32 = 12.;
const PAD_V: f32 = 8.;
const RADIUS: f32 = 8.;

fn split_cells(line: &str) -> Vec<&str> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(str::trim).collect()
}

fn is_numeric(s: &str) -> bool {
    let t: String = s.trim().chars().filter(|c| *c != ',').collect();
    let t = t.trim_matches(['%', '$', '¥', '€', '+', '-']);
    !t.is_empty() && t.parse::<f64>().is_ok()
}

fn shape_cell(cell: &Cell, width: Pixels, window: &Window) -> Laid {
    let text = cell.text.as_ref();
    let runs: Vec<RunInput> = cell
        .spans
        .iter()
        .map(|s| RunInput {
            text: &text[s.range.clone()],
            font: &s.font,
            size: s.size,
            color: s.color,
            kern_after: s.kern_after,
            box_size: None,
            box_mode: BoxMode::Baseline,
            unbroken: None,
        })
        .collect();
    let (_, height) = cell_line(cell.base.as_ref(), cell.size, window);
    Laid::shape(text, &runs, |_| px(0.), width, true, |_, _| height, window)
}

/// A cell's baseline and line height: the font's own, as TextKit's `usedRect`
/// measures it, whole points.
fn cell_line(font: &Font, font_size: Pixels, window: &Window) -> (Pixels, Pixels) {
    let ts = window.text_system();
    let id = ts.resolve_font(font);
    let (ascent, descent) = natural_line(f32::from(ts.ascent(id, font_size)), f32::from(ts.descent(id, font_size)));
    (px(ascent), px(ascent + descent))
}

/// Ascent and descent rounded up to whole points.
pub fn natural_line(ascent: f32, descent: f32) -> (f32, f32) {
    (ascent.ceil(), descent.abs().ceil())
}

/// Text widths for the columns: as wide as their content when it all fits;
/// otherwise every column keeps room for its longest word (when the words fit
/// at all) and the rest of the width is shared by how much more each wants.
pub fn column_widths(natural: &[f32], minimum: &[f32], available: f32) -> Vec<f32> {
    if natural.iter().sum::<f32>() <= available {
        return natural.to_vec();
    }
    const FLOOR: f32 = 30.;
    let mins: Vec<f32> = natural.iter().zip(minimum).map(|(n, m)| n.min(m.max(FLOOR))).collect();
    let min_total: f32 = mins.iter().sum();
    if min_total >= available {
        let scale = available / min_total.max(1.);
        return mins.iter().map(|m| FLOOR.max((m * scale).floor())).collect();
    }
    let extra = available - min_total;
    let wants: Vec<f32> = natural.iter().zip(&mins).map(|(n, m)| n - m).collect();
    let want_total = wants.iter().sum::<f32>().max(1.);
    mins.iter().zip(&wants).map(|(m, w)| (m + w * extra / want_total).floor()).collect()
}

fn longest_word(text: &str, laid: &Laid) -> f32 {
    let mut best = 0f32;
    let mut start = None;
    let close = |start: usize, end: usize, best: &mut f32| {
        let w = laid.x_for_index(end) - laid.x_for_index(start);
        *best = best.max(f32::from(w).ceil());
    };
    for (i, ch) in text.char_indices() {
        let breaks = ch.is_whitespace() || is_cjk(ch);
        match (breaks, start) {
            (true, Some(s)) => {
                close(s, i, &mut best);
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(s) = start {
        close(s, text.len(), &mut best);
    }
    best
}

fn table_model(src: &str, available: f32, typo: &Typography, window: &Window) -> TableModel {
    let lines: Vec<&str> = lf_lines(src).collect();
    let headers = split_cells(lines[0]);
    let cols = headers.len().max(1);
    let mut aligns: Vec<Align> = split_cells(lines.get(1).copied().unwrap_or(""))
        .into_iter()
        .map(|c| match (c.starts_with(':'), c.ends_with(':')) {
            (true, true) => Align::Center,
            (false, true) => Align::Right,
            _ => Align::Left,
        })
        .collect();
    aligns.resize(cols, Align::Left);
    let body: Vec<Vec<&str>> = lines.iter().skip(2).map(|l| split_cells(l)).collect();
    let numeric: Vec<bool> = (0..cols)
        .map(|c| !body.is_empty() && body.iter().all(|r| is_numeric(r.get(c).copied().unwrap_or(""))))
        .collect();
    let mut rows = Vec::with_capacity(body.len() + 1);
    for (r, cells) in std::iter::once(&headers).chain(body.iter()).enumerate() {
        rows.push(
            (0..cols)
                .map(|c| {
                    let (text, spans) = cell_spans(cells.get(c).copied().unwrap_or(""), r == 0, numeric[c], typo);
                    let mut base = ui_font();
                    if r == 0 {
                        base.weight = FontWeight::SEMIBOLD;
                    }
                    Cell { text, spans, base: Rc::new(base), size: px(typo.small) }
                })
                .collect::<Vec<_>>(),
        );
    }
    let mut natural = vec![8f32; cols];
    let mut minimum = vec![0f32; cols];
    for row in &rows {
        for (c, cell) in row.iter().enumerate() {
            let laid = shape_cell(cell, px(1e6), window);
            natural[c] = natural[c].max(f32::from(laid.width()).ceil() + 1.);
            minimum[c] = minimum[c].max(longest_word(cell.text.as_ref(), &laid));
        }
    }
    let chrome = BORDER * (cols + 1) as f32 + PAD_H * 2. * cols as f32;
    let avail_text = (cols as f32 * 30.).max(available - chrome);
    let widths = column_widths(&natural, &minimum, avail_text);
    TableModel { widths, rows, aligns, numeric }
}

pub fn table(
    src: &str,
    available: Pixels,
    typo: &Typography,
    cache: &mut BlockCache,
    window: &mut Window,
    _cx: &mut App,
) -> AnyElement {
    let avail = f32::from(available);
    let model = cache
        .tables
        .entry(key(&format!("{avail}|{src}")))
        .or_insert_with(|| Rc::new(table_model(src, avail, typo, window)))
        .clone();
    let c = t();
    let cols = model.widths.len();
    let total = model.widths.iter().sum::<f32>()
        + BORDER * (cols + 1) as f32
        + PAD_H * 2. * cols as f32;

    let mut grid = div()
        .flex()
        .flex_col()
        .w(px(total.ceil()))
        .border_1()
        .border_color(c.table_border)
        .rounded(px(RADIUS))
        .overflow_hidden();
    for (r, row) in model.rows.iter().enumerate() {
        let mut line = div().flex().w_full();
        if r == 0 {
            line = line.bg(c.code_block);
        } else {
            line = line.border_t_1().border_color(c.table_border);
        }
        for (col, width) in model.widths.iter().enumerate() {
            let mut cell = div()
                .w(px(width + PAD_H * 2.))
                .flex_none()
                .px(px(PAD_H))
                .py(px(PAD_V));
            if col > 0 {
                cell = cell.border_l_1().border_color(c.table_border);
            }
            let align = match model.aligns[col] {
                Align::Left if model.numeric[col] => Align::Right,
                a => a,
            };
            if row.get(col).is_some() {
                cell = cell.child(CellEl { model: model.clone(), row: r, col, align });
            }
            line = line.child(cell);
        }
        grid = grid.child(line);
    }
    div().w_full().pt(px(8.)).pb(px(10.)).child(grid).into_any_element()
}

/// One cell's text, in its column: wrapped at its width, aligned in it, inline
/// code on a rounded fill. `CellLayout`.
struct CellEl {
    model: Rc<TableModel>,
    row: usize,
    col: usize,
    align: Align,
}

impl IntoElement for CellEl {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for CellEl {
    type RequestLayoutState = Rc<RefCell<Option<(Pixels, Rc<Laid>)>>>;
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
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let slot: Self::RequestLayoutState = Rc::default();
        let (model, row, col) = (self.model.clone(), self.row, self.col);
        let filled = slot.clone();
        let id = window.request_measured_layout(style, move |known, avail, window, _| {
            let width = known.width.unwrap_or(match avail.width {
                AvailableSpace::Definite(w) => w,
                _ => px(model.widths[col]),
            });
            let laid = Rc::new(shape_cell(&model.rows[row][col], width.max(px(1.)), window));
            let height = laid.height;
            *filled.borrow_mut() = Some((width, laid));
            size(width, height)
        });
        (id, slot)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        slot: &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        let cell = &self.model.rows[self.row][self.col];
        let cached = slot.borrow().clone();
        let laid = match cached {
            Some((w, laid)) if w == bounds.size.width => laid,
            _ => Rc::new(shape_cell(cell, bounds.size.width.max(px(1.)), window)),
        };
        let (baseline, _) = cell_line(cell.base.as_ref(), cell.size, window);
        let ts = window.text_system().clone();
        for row in &laid.rows {
            let row_width = laid.x_for_index(row.end) - row.x0;
            let free = (bounds.size.width - row_width).max(px(0.));
            let shift = match self.align {
                Align::Left => px(0.),
                Align::Center => free / 2.,
                Align::Right => free,
            };
            let left = bounds.left() + shift - row.x0;
            let top = bounds.top() + row.y;
            let base = top + baseline;
            for span in &cell.spans {
                let (a, e) = (span.range.start.max(row.start), span.range.end.min(row.end));
                if a >= e {
                    continue;
                }
                let (x0, x1) = (left + laid.x_for_index(a), left + laid.x_for_index(e));
                if let Some(pill) = span.fill {
                    window.paint_quad(PaintQuad {
                        bounds: Bounds::from_corners(
                            point(x0 - px(pill.pad), top - px(pill.inset)),
                            point(x1 + px(pill.pad), top + row.height + px(pill.inset)),
                        ),
                        corner_radii: Corners::all(px(pill.radius)),
                        background: pill.color.into(),
                        border_widths: Edges::default(),
                        border_color: transparent_black(),
                        border_style: BorderStyle::default(),
                    });
                }
            }
            for g in laid.glyphs_in(row.start..row.end) {
                let at = point(left + g.x, base);
                if g.emoji {
                    let _ = window.paint_emoji(at, g.font, g.id, g.size);
                } else {
                    let _ = window.paint_glyph(at, g.font, g.id, g.size, g.color);
                }
            }
            for span in &cell.spans {
                let (a, e) = (span.range.start.max(row.start), span.range.end.min(row.end));
                if a >= e || (span.underline.is_none() && span.strike.is_none()) {
                    continue;
                }
                let (x0, x1) = (left + laid.x_for_index(a), left + laid.x_for_index(e));
                if let Some(u) = span.underline {
                    let y = (base + px(u.below)).round();
                    window.paint_quad(fill(Bounds::new(point(x0, y), size(x1 - x0, px(1.))), u.color));
                }
                if let Some(color) = span.strike {
                    let x_height = ts.x_height(ts.resolve_font(&span.font), span.size);
                    let y = (base - x_height / 2.).round();
                    window.paint_quad(fill(Bounds::new(point(x0, y), size(x1 - x0, px(1.))), color));
                }
            }
        }
    }
}

// Math -----------------------------------------------------------------------

/// Around a formula, as `MathRenderer.render` leaves it.
pub const FORMULA_PAD: f32 = 2.;

/// A typeset formula: RaTeX's pixels and how they sit on the text's baseline.
pub struct Formula {
    pub image: Arc<RenderImage>,
    pub width: f32,
    pub height: f32,
    /// From the baseline down to the image's foot, padding included.
    pub depth: f32,
}

fn display_list(latex: &str) -> Option<ratex_types::DisplayList> {
    let nodes = ratex_parser::parse(latex).ok()?;
    let lbox = ratex_layout::layout(&nodes, &ratex_layout::LayoutOptions::default());
    Some(ratex_layout::to_display_list(&lbox))
}

/// Whether RaTeX can set this. Asked for each formula on each line drawn, so
/// remembered: the answer depends on nothing but the text.
pub fn can_typeset(latex: &str) -> bool {
    thread_local! {
        static KNOWN: RefCell<HashMap<u64, bool>> = RefCell::default();
    }
    let k = key(latex);
    if let Some(known) = KNOWN.with(|m| m.borrow().get(&k).copied()) {
        return known;
    }
    let fits = |n: f64| n.is_finite() && n >= 0.;
    let ok = display_list(latex).is_some_and(|l| fits(l.width) && fits(l.height + l.depth));
    KNOWN.with(|m| m.borrow_mut().insert(k, ok));
    ok
}

/// Typesets a formula at `size` with only `FORMULA_PAD` around it, its glyphs in
/// `color` on nothing. `None` if RaTeX can't.
pub fn typeset(latex: &str, font_size: f32, dpr: f32, color: Hsla) -> Option<Formula> {
    let list = display_list(latex)?;
    let options = ratex_render::RenderOptions {
        font_size,
        padding: FORMULA_PAD,
        device_pixel_ratio: dpr,
        ..Default::default()
    };
    let png = ratex_render::render_to_png(&list, &options).ok()?;
    let rgba = image::load_from_memory(&png).ok()?.into_rgba8();
    let (w, h) = rgba.dimensions();
    let mut bytes = rgba.into_raw();
    // RaTeX rasters black on white: a pixel's darkness becomes the glyph's alpha,
    // over the text color, straight BGRA as a RenderImage holds it.
    let c: Rgba = color.into();
    let (r, g, b) = ((c.r * 255.).round() as u8, (c.g * 255.).round() as u8, (c.b * 255.).round() as u8);
    for px in bytes.chunks_exact_mut(4) {
        let a = 255 - px[0];
        px[0] = b;
        px[1] = g;
        px[2] = r;
        px[3] = a;
    }
    let buffer = image::RgbaImage::from_raw(w, h, bytes)?;
    let height = h as f32 / dpr;
    Some(Formula {
        image: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
        width: w as f32 / dpr,
        height,
        depth: formula_depth(height, list.height as f32, font_size),
    })
}

/// What's below the baseline: RaTeX puts it `padding + ascent × size` from the top.
fn formula_depth(image_height: f32, ascent_em: f32, font_size: f32) -> f32 {
    image_height - FORMULA_PAD - ascent_em * font_size
}

pub fn latex_of(src: &str) -> &str {
    let t = src.trim();
    let t = t.strip_prefix("$$").unwrap_or(t);
    let t = t.strip_suffix("$$").unwrap_or(t);
    t.trim()
}

pub fn inline_math(latex: &str, size: f32, cache: &mut BlockCache, window: &Window) -> Option<Rc<Formula>> {
    let dpr = window.scale_factor();
    cache
        .math
        .entry(key(&format!("I{size}|{dpr}|{latex}")))
        .or_insert_with(|| typeset(latex, size, dpr, t().text).map(Rc::new))
        .clone()
}

pub fn math(
    src: &str,
    available: Pixels,
    typo: &Typography,
    cache: &mut BlockCache,
    window: &mut Window,
) -> AnyElement {
    let dpr = window.scale_factor();
    let c = t();
    let size = typo.size + 2.;
    let rendered = cache
        .math
        .entry(key(&format!("D{size}|{dpr}|{src}")))
        .or_insert_with(|| typeset(latex_of(src), size, dpr, c.text).map(Rc::new))
        .clone();
    match rendered {
        Some(r) => {
            // A formula wider than the column shrinks to fit.
            let scale = (f32::from(available) / r.width).min(1.);
            div()
                .w_full()
                .py(px(10.))
                .flex()
                .justify_center()
                .child(img(r.image.clone()).w(px(r.width * scale)).h(px(r.height * scale)).flex_none())
                .into_any_element()
        }
        None => div()
            .w_full()
            .py(px(10.))
            .text_size(px(typo.small))
            .font(mono_font())
            .text_color(c.text3)
            .child(src.to_string())
            .into_any_element(),
    }
}

// Frontmatter ----------------------------------------------------------------

const ROW_HEIGHT: f32 = 26.;
const KEY_COLUMN: f32 = 96.;

fn pill_colors(index: usize, dark: bool) -> (Hsla, Hsla) {
    use fumi_ui::theme::hex;
    const LIGHT: [(u32, u32); 5] = [
        (0xFBF0D9, 0x7A5410),
        (0xE8F1FB, 0x1F4E80),
        (0xFDECEE, 0x8A2F36),
        (0xE7F5EC, 0x2F6B4A),
        (0xF3EEF8, 0x6B4A8A),
    ];
    const DARK: [(u32, u32); 5] = [
        (0x3A3220, 0xE8C77A),
        (0x1E2A38, 0x9CC4EE),
        (0x3A2224, 0xEBA0A6),
        (0x1E2E25, 0x8FCBA6),
        (0x2E2638, 0xC6A9E0),
    ];
    let (bg, fg) = if dark { DARK[index % 5] } else { LIGHT[index % 5] };
    (hex(bg), hex(fg))
}

fn pill_text_width(text: &str, window: &Window) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: ui_font(),
        color: t().text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window.text_system().shape_line(SharedString::from(text.to_string()), px(text_size::BODY), &[run], None);
    f32::from(shaped.width).ceil()
}

/// How many pills fit on the row from `start`: one that would run past the edge
/// stops the row, with room for its 6 pt each side and 4 between.
pub fn pills_that_fit(text_widths: &[f32], start: f32, available: f32) -> usize {
    let mut x = start;
    for (i, width) in text_widths.iter().enumerate() {
        let right = x + width + 12.;
        if right > available {
            return i;
        }
        x = right + 4.;
    }
    text_widths.len()
}

pub fn frontmatter(
    src: &str,
    available: Pixels,
    _typo: &Typography,
    window: &mut Window,
    _cx: &mut App,
) -> AnyElement {
    let c = t();
    let fields = Frontmatter::parse(src).map(|f| f.fields).unwrap_or_default();
    let mut panel = div()
        .flex()
        .flex_col()
        .w(available)
        .text_size(px(text_size::BODY))
        .font(ui_font());
    let row = || div().flex().items_center().h(px(ROW_HEIGHT)).w_full();
    if fields.is_empty() {
        panel = panel.child(row().text_color(c.text3).child("Properties"));
    }
    for field in fields {
        let key = div()
            .w(px(KEY_COLUMN - 8.))
            .mr(px(8.))
            .flex_none()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_color(c.text3)
            .child(field.key.clone());
        let value = match &field.value {
            Value::Text(text) => div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(c.text)
                .child(text.clone()),
            Value::List(items) => div().flex_1().min_w_0().flex().gap(px(4.)).children({
                let widths: Vec<f32> = items.iter().map(|item| pill_text_width(item, window)).collect();
                let shown = pills_that_fit(&widths, KEY_COLUMN, f32::from(available));
                items.iter().take(shown).enumerate().map(|(j, item)| {
                    let (bg, fg) = pill_colors(j, c.dark);
                    div()
                        .flex_none()
                        .h(px(20.))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .rounded(px(4.))
                        .bg(bg)
                        .text_color(fg)
                        .child(item.clone())
                })
            }),
        };
        panel = panel.child(row().child(key).child(value));
    }
    panel = panel.child(div().mt(px(10.)).h(px(1.)).w_full().bg(c.rule));
    div().w_full().pb(px(16.)).child(panel).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn columns_fit_or_share() {
        assert_eq!(column_widths(&[10., 20.], &[5., 5.], 100.), [10., 20.]);
        let w = column_widths(&[100., 100.], &[40., 40.], 120.);
        assert_eq!(w.iter().sum::<f32>(), 120.);
    }

    #[core::prelude::v1::test]
    fn a_pill_that_would_overflow_stops_the_row() {
        assert_eq!(pills_that_fit(&[20., 20.], 96., 200.), 2);
        // 96 + 32 = 128, +4; then 132 + 32 = 164, +4; the third ends at 200.
        assert_eq!(pills_that_fit(&[20., 20., 20.], 96., 200.), 3);
        assert_eq!(pills_that_fit(&[20., 20., 21.], 96., 200.), 2);
        assert_eq!(pills_that_fit(&[500.], 96., 200.), 0);
        assert_eq!(pills_that_fit(&[], 96., 200.), 0);
    }

    #[core::prelude::v1::test]
    fn what_cant_be_typeset_is_known_before_it_is_drawn() {
        assert!(can_typeset(r"\frac{1}{2}"));
        assert!(can_typeset("x^2"));
        assert!(!can_typeset(r"\nosuchcommand"));
        assert!(!can_typeset(r"\frac{1"));
        assert!(typeset(r"\nosuchcommand", 16., 2., black()).is_none());
    }

    #[core::prelude::v1::test]
    fn a_formula_keeps_2pt_around_it_and_knows_its_baseline() {
        let x = typeset("x", 16., 2., black()).unwrap();
        let list = display_list("x").unwrap();
        // Exactly the 2 pt padding around the ink's box, whole device pixels up.
        assert!((x.width - (list.width as f32 * 16. + FORMULA_PAD * 2.)).abs() <= 0.5, "{}", x.width);
        assert!((x.height - ((list.height + list.depth) as f32 * 16. + FORMULA_PAD * 2.)).abs() <= 0.5, "{}", x.height);
        // Nothing below the baseline but the padding.
        assert!((x.depth - (list.depth as f32 * 16. + FORMULA_PAD)).abs() <= 0.5, "{}", x.depth);
        // A subscript reaches further down.
        let sub = typeset("x_{i}", 16., 2., black()).unwrap();
        assert!(sub.depth > x.depth);
        // A fraction stands on the math axis, well below the baseline.
        let frac = typeset(r"\frac{a}{b}", 16., 2., black()).unwrap();
        assert!(frac.depth > sub.depth);
        assert_eq!(formula_depth(20., 0.5, 16.), 10.);
    }

    #[core::prelude::v1::test]
    fn cell_lines_are_whole_points() {
        assert_eq!(natural_line(13.3, -3.4), (14., 4.));
        assert_eq!(natural_line(13., 3.), (13., 3.));
    }

    #[core::prelude::v1::test]
    fn numbers_and_cells() {
        assert!(is_numeric("1,234.5"));
        assert!(is_numeric("$12%"));
        assert!(!is_numeric("12a"));
        assert_eq!(split_cells("| a | b |"), ["a", "b"]);
    }
}
