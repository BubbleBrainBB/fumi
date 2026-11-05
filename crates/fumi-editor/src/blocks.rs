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
use ratex_gpui::render::{Rendered, render_latex};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

#[derive(Default)]
pub struct BlockCache {
    tables: HashMap<u64, Rc<TableModel>>,
    math: HashMap<u64, Option<Rc<Rendered>>>,
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
            box_mode: BoxMode::Centered,
        })
        .collect();
    Laid::shape(text, &runs, |_| px(0.), width, true, |_, _| px(19.), window)
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
                    Cell { text, spans }
                })
                .collect::<Vec<_>>(),
        );
    }
    let mut natural = vec![8f32; cols];
    let mut minimum = vec![0f32; cols];
    for row in &rows {
        for (c, cell) in row.iter().enumerate() {
            let laid = shape_cell(cell, px(1e6), window);
            natural[c] = natural[c].max(f32::from(laid.width()).ceil());
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
                .py(px(PAD_V))
                .text_size(px(typo.small))
                .line_height(px(19.));
            if col > 0 {
                cell = cell.border_l_1().border_color(c.table_border);
            }
            let align = match model.aligns[col] {
                Align::Left if model.numeric[col] => Align::Right,
                a => a,
            };
            cell = match align {
                Align::Right => cell.flex().justify_end(),
                Align::Center => cell.flex().justify_center(),
                Align::Left => cell,
            };
            if let Some(content) = row.get(col) {
                let runs: Vec<TextRun> = content
                    .spans
                    .iter()
                    .map(|s| TextRun {
                        len: s.range.len(),
                        font: s.font.clone(),
                        color: s.color,
                        background_color: s.fill.map(|f| f.color),
                        underline: s.underline.map(|u| UnderlineStyle {
                            thickness: px(1.),
                            color: Some(u.color),
                            wavy: false,
                        }),
                        strikethrough: s.strike.map(|color| StrikethroughStyle {
                            thickness: px(1.),
                            color: Some(color),
                        }),
                    })
                    .collect();
                cell = cell.child(StyledText::new(content.text.clone()).with_runs(runs));
            }
            line = line.child(cell);
        }
        grid = grid.child(line);
    }
    div().w_full().pt(px(8.)).pb(px(10.)).child(grid).into_any_element()
}

// Math -----------------------------------------------------------------------

pub fn latex_of(src: &str) -> &str {
    let t = src.trim();
    let t = t.strip_prefix("$$").unwrap_or(t);
    let t = t.strip_suffix("$$").unwrap_or(t);
    t.trim()
}

pub fn inline_math(latex: &str, size: f32, cache: &mut BlockCache, window: &Window) -> Option<Rc<Rendered>> {
    let dpr = window.scale_factor();
    cache
        .math
        .entry(key(&format!("I{size}|{latex}")))
        .or_insert_with(|| render_latex(latex, size, dpr, t().text).map(Rc::new))
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
        .entry(key(&format!("D{size}|{src}")))
        .or_insert_with(|| render_latex(latex_of(src), size, dpr, c.text).map(Rc::new))
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

pub fn frontmatter(
    src: &str,
    available: Pixels,
    _typo: &Typography,
    _window: &mut Window,
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
            Value::List(items) => div().flex_1().min_w_0().flex().gap(px(4.)).overflow_hidden().children(
                items.iter().enumerate().map(|(j, item)| {
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
                }),
            ),
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
    fn numbers_and_cells() {
        assert!(is_numeric("1,234.5"));
        assert!(is_numeric("$12%"));
        assert!(!is_numeric("12a"));
        assert_eq!(split_cells("| a | b |"), ["a", "b"]);
    }
}
