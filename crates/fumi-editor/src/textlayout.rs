//! Lays out one line of styled runs. GPUI's own wrapped text takes one font size
//! per line and can't add space between characters; a note needs both (inline
//! code a step smaller, a gap where Han meets Latin, room for pills). So each run
//! is shaped on its own, by GPUI's cached shaper, and this places the glyphs.

use gpui::*;
use std::ops::Range;

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub x: Pixels,
    pub id: GlyphId,
    pub font: FontId,
    pub size: Pixels,
    pub color: Hsla,
    /// Byte offset in the visible text.
    pub index: u32,
    pub emoji: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    pub start: usize,
    pub end: usize,
    /// Where the row's first glyph sits in the unwrapped line.
    pub x0: Pixels,
    pub y: Pixels,
    pub height: Pixels,
}

pub struct RunInput<'a> {
    pub text: &'a str,
    pub font: &'a Font,
    pub size: Pixels,
    pub color: Hsla,
    pub kern_after: Pixels,
    /// An inline box (typeset math) stands for this run: no glyphs.
    pub box_size: Option<Size<Pixels>>,
}

pub struct Laid {
    pub glyphs: Vec<Glyph>,
    /// Each run's start in the unwrapped line, then the line's full width.
    pub run_x: Vec<Pixels>,
    /// Byte offset each run starts at, then the text's length.
    pub run_at: Vec<usize>,
    pub rows: Vec<Row>,
    pub height: Pixels,
}

pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2FDF | 0x3000..=0x30FF | 0x3100..=0x312F | 0x3130..=0x318F | 0x31A0..=0x31BF
        | 0x31F0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x2FA1F)
}

/// Chinese line-breaking rules: closing punctuation never starts a line.
fn no_break_before(c: char) -> bool {
    matches!(c,
        '，' | '。' | '、' | '；' | '：' | '！' | '？' | '）' | '》' | '」' | '』' | '】' | '〕' | '〉'
        | '”' | '’' | '…' | '—' | '·' | '～' | ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}'
        | '%' | '％' | 'ー' | 'っ' | 'ゃ' | 'ゅ' | 'ょ' | 'ッ' | 'ャ' | 'ュ' | 'ョ')
}

/// …and opening punctuation never ends one.
fn no_break_after(c: char) -> bool {
    matches!(c, '（' | '《' | '「' | '『' | '【' | '〔' | '〈' | '“' | '‘' | '(' | '[' | '{')
}

pub fn can_break_between(prev: char, next: char) -> bool {
    if next.is_whitespace() || no_break_before(next) || no_break_after(prev) {
        return false;
    }
    if prev.is_whitespace() || prev == '-' || prev == '—' {
        return true;
    }
    is_cjk(prev) || is_cjk(next)
}

/// Finds where the rows of `text` end, given the x at which each glyph starts.
/// `boundaries` holds (byte index, x) for each glyph in order; `total` closes the
/// last one. Returns the rows' start bytes after the first.
pub fn wrap(text: &str, boundaries: &[(usize, f32)], total: f32, first_x: f32, width: f32) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut row_x = first_x;
    let mut last_break: Option<(usize, f32)> = None;
    let mut row_start = boundaries.first().map_or(0, |b| b.0);
    for (k, (index, x)) in boundaries.iter().copied().enumerate() {
        let end = boundaries.get(k + 1).map_or(total, |b| b.1);
        let c = text[index..].chars().next().unwrap_or(' ');
        if index > row_start {
            let prev = text[..index].chars().next_back().unwrap_or(' ');
            if can_break_between(prev, c) {
                last_break = Some((index, x));
            }
        }
        // Spaces hang past the edge rather than wrap on their own.
        if end - row_x > width && !c.is_whitespace() {
            let (at, at_x) = match last_break {
                Some(b) if b.0 > row_start => b,
                _ if index > row_start => (index, x),
                _ => continue,
            };
            starts.push(at);
            row_start = at;
            row_x = at_x;
            last_break = None;
        }
    }
    starts
}

impl Laid {
    pub fn shape(
        text: &str,
        runs: &[RunInput],
        first_row_x: impl Fn(&[Pixels]) -> Pixels,
        width: Pixels,
        wrap_text: bool,
        line_height: impl Fn(usize, Pixels) -> Pixels,
        window: &Window,
    ) -> Laid {
        let mut glyphs = Vec::with_capacity(text.len());
        let mut run_x = Vec::with_capacity(runs.len() + 1);
        let mut run_at = Vec::with_capacity(runs.len() + 1);
        let mut x = px(0.);
        let mut at = 0;
        let ts = window.text_system();
        for run in runs {
            run_x.push(x);
            run_at.push(at);
            if let Some(size) = run.box_size {
                x += size.width + run.kern_after;
                at += run.text.len();
                continue;
            }
            if !run.text.is_empty() {
                let layout = ts.layout_line(
                    run.text,
                    run.size,
                    &[TextRun {
                        len: run.text.len(),
                        font: run.font.clone(),
                        color: run.color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                for shaped in &layout.runs {
                    for g in &shaped.glyphs {
                        glyphs.push(Glyph {
                            x: x + g.position.x,
                            id: g.id,
                            font: shaped.font_id,
                            size: run.size,
                            color: run.color,
                            index: (at + g.index) as u32,
                            emoji: g.is_emoji,
                        });
                    }
                }
                x += layout.width + run.kern_after;
            }
            at += run.text.len();
        }
        run_x.push(x);
        run_at.push(at);

        let x0 = first_row_x(&run_x);
        let mut boundaries: Vec<(usize, f32)> = Vec::with_capacity(glyphs.len() + runs.len());
        let mut g = 0;
        for (r, run) in runs.iter().enumerate() {
            if run.box_size.is_some() {
                boundaries.push((run_at[r], f32::from(run_x[r])));
            } else {
                while g < glyphs.len() && (glyphs[g].index as usize) < run_at[r + 1] {
                    boundaries.push((glyphs[g].index as usize, f32::from(glyphs[g].x)));
                    g += 1;
                }
            }
        }
        boundaries.retain(|b| f32::from(x0) <= b.1);
        let starts = if wrap_text {
            wrap(text, &boundaries, f32::from(x), f32::from(x0), f32::from(width))
        } else {
            Vec::new()
        };

        let mut laid = Laid { glyphs, run_x, run_at, rows: Vec::with_capacity(starts.len() + 1), height: px(0.) };
        let mut start = 0;
        let mut row_x = x0;
        let mut y = px(0.);
        for (i, end) in starts.iter().copied().chain([text.len()]).enumerate() {
            let tallest = runs
                .iter()
                .enumerate()
                .filter(|(r, run)| {
                    run.box_size.is_some() && laid.run_at[*r] >= start && laid.run_at[*r] < end.max(start + 1)
                })
                .filter_map(|(_, run)| run.box_size.map(|s| s.height))
                .fold(px(0.), Pixels::max);
            let h = line_height(i, tallest);
            laid.rows.push(Row { start, end, x0: row_x, y, height: h });
            y += h;
            if end < text.len() {
                row_x = laid.x_for_index(end);
            }
            start = end;
        }
        laid.height = y;
        laid
    }

    pub fn width(&self) -> Pixels {
        self.run_x.last().copied().unwrap_or_default()
    }

    /// Position of byte `index` in the unwrapped line.
    pub fn x_for_index(&self, index: usize) -> Pixels {
        let after = self.glyphs.partition_point(|g| (g.index as usize) < index);
        let glyph_x = self.glyphs.get(after).map(|g| (g.index as usize, g.x));
        let run = self.run_at.partition_point(|r| *r <= index).saturating_sub(1);
        let run = run.min(self.run_x.len() - 1);
        let (run_at, run_x) = (self.run_at[run], self.run_x[run]);
        match glyph_x {
            Some((at, x)) if at == index => x,
            _ if run_at == index => run_x,
            // Inside a cluster or a box, or at the very end.
            Some((_, x)) => x.max(run_x),
            None => self.width(),
        }
    }

    pub fn row_of(&self, index: usize) -> usize {
        self.rows
            .iter()
            .rposition(|r| r.start <= index)
            .unwrap_or(0)
    }

    /// The caret's row for `index`, which may be the end of an earlier row: a
    /// wrapped row's end is the next row's start, so it shows on the next row.
    pub fn x_in_row(&self, index: usize, row: usize) -> Pixels {
        self.x_for_index(index) - self.rows[row].x0
    }

    /// The boundary nearest to `x` (relative to the row's text edge) in `row`.
    pub fn index_at(&self, row: usize, x: Pixels) -> usize {
        let row = row.min(self.rows.len() - 1);
        let r = self.rows[row];
        let target = r.x0 + x;
        let first = self.glyphs.partition_point(|g| (g.index as usize) < r.start);
        let mut best = r.start;
        let mut best_d = (self.x_for_index(r.start) - target).abs();
        for g in &self.glyphs[first..] {
            let i = g.index as usize;
            if i >= r.end {
                break;
            }
            let d = (g.x - target).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        // Past the last glyph: the row's end, unless that's the next row's start.
        let end_x = if r.end < self.run_x_total_len() {
            self.x_for_index(r.end)
        } else {
            self.width()
        };
        if (end_x - target).abs() < best_d {
            let last = row + 1 == self.rows.len();
            return if last { r.end } else { self.last_index_before(r.end).max(r.start) };
        }
        best
    }

    fn run_x_total_len(&self) -> usize {
        self.run_at.last().copied().unwrap_or(0)
    }

    fn last_index_before(&self, end: usize) -> usize {
        let k = self.glyphs.partition_point(|g| (g.index as usize) < end);
        if k == 0 { 0 } else { self.glyphs[k - 1].index as usize }
    }

    pub fn glyphs_in(&self, range: Range<usize>) -> &[Glyph] {
        let a = self.glyphs.partition_point(|g| (g.index as usize) < range.start);
        let b = self.glyphs.partition_point(|g| (g.index as usize) < range.end);
        &self.glyphs[a..b]
    }

    /// Paints every glyph, with baselines given per row.
    pub fn paint_glyphs(&self, origin: Point<Pixels>, baseline: impl Fn(usize) -> Pixels, window: &mut Window) {
        for (i, row) in self.rows.iter().enumerate() {
            let y = origin.y + row.y + baseline(i);
            for g in self.glyphs_in(row.start..row.end) {
                let at = point(origin.x + g.x - row.x0, y);
                if g.emoji {
                    let _ = window.paint_emoji(at, g.font, g.id, g.size);
                } else {
                    let _ = window.paint_glyph(at, g.font, g.id, g.size, g.color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(text: &str, w: f32) -> Vec<(usize, f32)> {
        text.char_indices().enumerate().map(|(k, (i, _))| (i, k as f32 * w)).collect()
    }

    #[core::prelude::v1::test]
    fn latin_wraps_at_spaces_and_han_anywhere() {
        let t = "hello world again";
        let starts = wrap(t, &bounds(t, 10.), 170., 0., 80.);
        assert_eq!(starts, [6, 12]);
        let t = "一二三四五六七八";
        let starts = wrap(t, &bounds(t, 10.), 80., 0., 30.);
        assert_eq!(starts, [9, 18]);
    }

    #[core::prelude::v1::test]
    fn closing_punctuation_stays_on_its_line() {
        let t = "一二三，四五";
        let starts = wrap(t, &bounds(t, 10.), 60., 0., 30.);
        // "，" can't start a row, so 三 goes down with it; 五 then starts the third.
        assert_eq!(starts, [6, 15]);
    }

    #[core::prelude::v1::test]
    fn a_word_longer_than_the_row_breaks_inside() {
        let t = "abcdefghij";
        let starts = wrap(t, &bounds(t, 10.), 100., 0., 40.);
        assert_eq!(starts, [4, 8]);
    }

    #[core::prelude::v1::test]
    fn trailing_spaces_do_not_wrap() {
        let t = "abcd      ";
        assert!(wrap(t, &bounds(t, 10.), 100., 0., 40.).is_empty());
    }
}
