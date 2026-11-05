//! Folded blocks: a table grid, a typeset formula, an image. Each is rendered
//! from the source lines it covers and never writes back.

use crate::display::*;
use crate::markdown::*;
use gpui::prelude::*;
use gpui::*;
use ratex_gpui::render::{Rendered, render_latex};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::rc::Rc;

#[derive(Default)]
pub struct BlockCache {
    tables: HashMap<u64, Rc<TableModel>>,
    math: HashMap<u64, Option<Rc<Rendered>>>,
    images: HashMap<String, Option<(f32, f32)>>,
}

pub struct TableModel {
    widths: Vec<f32>,
    rows: Vec<Vec<(SharedString, Vec<TextRun>)>>,
    right: Vec<bool>,
}

fn key(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

const TABLE_FONT: f32 = 15.;
const CELL_PAD: f32 = 10.;

fn table_model(src: &str, window: &Window) -> TableModel {
    let mut rows = Vec::new();
    let mut right = Vec::new();
    for (i, line) in src.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if i == 1 {
            right = table_cells(line)
                .into_iter()
                .map(|c| line[c].trim_end().ends_with(':'))
                .collect();
            continue;
        }
        rows.push(
            table_cells(line)
                .into_iter()
                .map(|c| cell_text(&line[c], i == 0))
                .collect::<Vec<_>>(),
        );
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0f32; cols];
    let ts = window.text_system();
    for row in &rows {
        for (col, (text, runs)) in row.iter().enumerate() {
            let w = ts.shape_line(text.clone(), px(TABLE_FONT), runs, None).width;
            widths[col] = widths[col].max(f32::from(w));
        }
    }
    for w in &mut widths {
        *w = (*w + CELL_PAD * 2. + 1.).max(44.);
    }
    TableModel { widths, rows, right }
}

pub fn table(
    src: &str,
    available: Pixels,
    cache: &mut BlockCache,
    window: &mut Window,
    _cx: &mut App,
) -> AnyElement {
    let model = cache
        .tables
        .entry(key(src))
        .or_insert_with(|| Rc::new(table_model(src, window)))
        .clone();
    let total: f32 = model.widths.iter().sum();
    let avail = f32::from(available);
    let scale = if total > avail { avail / total } else { 1. };

    let mut grid = div()
        .flex()
        .flex_col()
        .w(px(total * scale))
        .border_1()
        .border_color(hairline())
        .rounded(px(6.))
        .overflow_hidden();
    for (r, row) in model.rows.iter().enumerate() {
        let mut line = div().flex().w_full();
        if r == 0 {
            line = line.bg(tint());
        } else {
            line = line.border_t_1().border_color(hairline());
        }
        for (c, width) in model.widths.iter().enumerate() {
            let mut cell = div()
                .w(px(width * scale))
                .flex_none()
                .px(px(CELL_PAD))
                .py(px(5.))
                .text_size(px(TABLE_FONT))
                .line_height(px(22.));
            if model.right.get(c).copied().unwrap_or(false) {
                cell = cell.flex().justify_end();
            }
            if let Some((text, runs)) = row.get(c) {
                cell = cell.child(StyledText::new(text.clone()).with_runs(runs.clone()));
            }
            line = line.child(cell);
        }
        grid = grid.child(line);
    }
    div().w_full().py(px(8.)).child(grid).into_any_element()
}

pub fn latex_of(src: &str) -> &str {
    let t = src.trim();
    let t = t.strip_prefix("$$").unwrap_or(t);
    let t = t.strip_suffix("$$").unwrap_or(t);
    t.trim()
}

pub fn math(src: &str, cache: &mut BlockCache, window: &mut Window) -> AnyElement {
    let dpr = window.scale_factor();
    let rendered = cache
        .math
        .entry(key(src))
        .or_insert_with(|| render_latex(latex_of(src), 20., dpr, ink()).map(Rc::new))
        .clone();
    match rendered {
        Some(r) => div()
            .w_full()
            .py(px(6.))
            .flex()
            .justify_center()
            .child(
                img(r.image.clone())
                    .w(px(r.width))
                    .h(px(r.height))
                    .flex_none(),
            )
            .into_any_element(),
        None => div()
            .w_full()
            .py(px(6.))
            .text_size(px(14.))
            .font(mono_font())
            .text_color(ink2())
            .child(src.to_string())
            .into_any_element(),
    }
}

pub fn image(src: &str, dir: &Path, available: Pixels, cache: &mut BlockCache) -> AnyElement {
    let target = image_target(src).unwrap_or("");
    let path = if target.contains("://") { None } else { Some(dir.join(target)) };
    let dims = cache
        .images
        .entry(target.to_string())
        .or_insert_with(|| {
            path.as_ref()
                .and_then(|p| image::image_dimensions(p).ok())
                .map(|(w, h)| (w as f32, h as f32))
        })
        .to_owned();
    match (path, dims) {
        (Some(path), Some((w, h))) => {
            // Images fill the column at most; the height is known before
            // decoding, so the list's measurement never changes under it.
            let maxw = f32::from(available);
            let scale = (maxw / w).min(1.);
            div()
                .w_full()
                .py(px(8.))
                .child(
                    img(path)
                        .w(px(w * scale))
                        .h(px(h * scale))
                        .rounded(px(6.))
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element()
        }
        _ => div()
            .w_full()
            .py(px(8.))
            .text_size(px(13.))
            .text_color(marker_gray())
            .child(format!("Image not found: {target}"))
            .into_any_element(),
    }
}
