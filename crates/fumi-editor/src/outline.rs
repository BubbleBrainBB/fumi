//! The outline rail at the page's right edge: one tick per heading that swell
//! under the pointer like a dock, and a preview card gliding beside them.
//! `OutlineRail.swift`, with SwiftUI's springs and eases stepped by hand.

use crate::display::ui_font;
use crate::editor::Editor;
use fumi_core::outline_preview;
use fumi_ui::theme::t;
use gpui::prelude::*;
use gpui::*;
use std::time::{Duration, Instant};

pub const PITCH: f32 = 12.;
pub const MAX_RAIL_HEIGHT: f32 = 520.;
pub const RAIL_WIDTH: f32 = 30.;
pub const EDGE_INSET: f32 = 14.;
pub const CARD_WIDTH: f32 = 280.;
pub const CARD_GAP: f32 = 8.;
pub const CARD_MARGIN: f32 = 12.;
const HOVER_DELAY: Duration = Duration::from_millis(120);

/// A heading in the note's outline.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineHeading {
    pub level: u8,
    pub text: String,
    /// Plain-text excerpt of the section's body, for the card.
    pub summary: String,
    pub line: usize,
}

fn trim_spaces(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() && c != '\n')
}

/// The level of a line reading `#{1,6}\s+\S` once trimmed, and its text.
fn heading_of(line: &str) -> Option<(u8, &str)> {
    let trimmed = trim_spaces(line);
    let level = trimmed.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &trimmed[level..];
    let text = rest.trim_start();
    (rest.starts_with(char::is_whitespace) && !text.is_empty()).then_some((level as u8, trim_spaces(text)))
}

/// Each heading line: its index, byte range, level and text. Lines between fence
/// toggles don't count.
fn heading_lines(text: &str) -> Vec<(usize, std::ops::Range<usize>, u8, &str)> {
    let mut out = Vec::new();
    let mut in_code = false;
    let mut at = 0;
    for (index, raw) in text.split('\n').enumerate() {
        let start = at;
        at += raw.len() + 1;
        let trimmed = trim_spaces(raw);
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        if let Some((level, heading)) = heading_of(raw) {
            out.push((index, start..(start + raw.len() + 1).min(text.len()), level, heading));
        }
    }
    out
}

/// The note's outline, in document order. One pass over the text, run when typing
/// pauses.
pub fn outline(text: &str) -> Vec<OutlineHeading> {
    let lines = heading_lines(text);
    lines
        .iter()
        .enumerate()
        .map(|(i, (line, range, level, heading))| {
            let end = lines.get(i + 1).map_or(text.len(), |next| next.1.start);
            OutlineHeading {
                level: *level,
                text: heading.to_string(),
                summary: outline_preview::text(&text[range.end.min(end)..end], outline_preview::DEFAULT_LIMIT),
                line: *line,
            }
        })
        .collect()
}

/// The line of the first heading reading `name`, whatever its case.
pub fn find_heading(text: &str, name: &str) -> Option<usize> {
    let name = name.to_lowercase();
    heading_lines(text)
        .into_iter()
        .find(|(_, _, _, heading)| heading.trim_matches(['#', ' ']).to_lowercase() == name)
        .map(|h| h.0)
}

/// How the ticks sit in a pane of some height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailMetrics {
    pub count: usize,
    pub height: f32,
    pub top: f32,
}

impl RailMetrics {
    /// Ticks 12 apart, compressed beyond 520 or the pane less 40, centered.
    pub fn new(count: usize, pane: f32) -> Self {
        let height = (count.saturating_sub(1) as f32 * PITCH).min(MAX_RAIL_HEIGHT).min((pane - 40.).max(0.));
        RailMetrics { count, height, top: (pane - height) / 2. }
    }

    pub fn tick_y(&self, index: usize) -> f32 {
        if self.count > 1 { index as f32 / (self.count - 1) as f32 * self.height } else { 0. }
    }

    /// Pointer position in tick units, `y` being from the hit area's top, which
    /// starts half a pitch above the first tick.
    pub fn index_at(&self, y: f32) -> f32 {
        ((y - PITCH / 2.) / self.height.max(1.)).clamp(0., 1.) * self.count.saturating_sub(1) as f32
    }

    pub fn hit_top(&self) -> f32 {
        self.top - PITCH / 2.
    }

    pub fn hit_height(&self) -> f32 {
        self.height + PITCH
    }

    /// Where the card's top goes for a tick, kept on the page.
    pub fn card_top(&self, index: usize, card: f32, pane: f32) -> f32 {
        (self.top + self.tick_y(index) - card / 2.).max(CARD_MARGIN).min(pane - card - CARD_MARGIN)
    }
}

/// How near the pointer is to a tick, 0 to 1.
pub fn proximity(index: usize, position: Option<f32>) -> f32 {
    let Some(position) = position else { return 0. };
    let t = (1. - (index as f32 - position).abs() / 3.5).max(0.);
    t * t
}

pub fn tick_width(level: u8) -> f32 {
    match level {
        1 => 7.,
        2 => 5.,
        _ => 3.5,
    }
}

/// A tick's drawn width and opacity at a proximity.
pub fn tick_look(level: u8, p: f32) -> (f32, f32) {
    (tick_width(level) * (1. + 3. * p), 0.22 + 0.78 * p)
}

/// One step of the ticks' spring: mass 0.5, stiffness 700, damping 45.
pub fn spring_step(value: f32, velocity: f32, target: f32, dt: f32) -> (f32, f32) {
    const SUBSTEP: f32 = 1. / 480.;
    let (mut x, mut v) = (value, velocity);
    let mut left = dt.min(0.05);
    while left > 0. {
        let h = left.min(SUBSTEP);
        let accel = (-700. * (x - target) - 45. * v) / 0.5;
        v += accel * h;
        x += v * h;
        left -= h;
    }
    (x, v)
}

/// The curve (0.22, 1, 0.36, 1) at `p`, a fraction of the time.
pub fn ease(p: f32) -> f32 {
    let p = p.clamp(0., 1.) as f64;
    let (x1, y1, x2, y2) = (0.22f64, 1f64, 0.36f64, 1f64);
    let bezier = |a: f64, b: f64, t: f64| 3. * (1. - t) * (1. - t) * t * a + 3. * (1. - t) * t * t * b + t * t * t;
    let (mut low, mut high) = (0f64, 1f64);
    for _ in 0..24 {
        let mid = (low + high) / 2.;
        if bezier(x1, x2, mid) < p {
            low = mid;
        } else {
            high = mid;
        }
    }
    bezier(y1, y2, (low + high) / 2.) as f32
}

/// A value easing to where it was last sent.
#[derive(Clone, Copy, Debug)]
pub struct Eased {
    from: f32,
    to: f32,
    start: Instant,
    duration: f32,
}

impl Eased {
    pub fn at(value: f32) -> Self {
        Eased { from: value, to: value, start: Instant::now(), duration: 0. }
    }

    pub fn value(&self, now: Instant) -> f32 {
        if self.duration <= 0. {
            return self.to;
        }
        let p = now.saturating_duration_since(self.start).as_secs_f32() / self.duration;
        self.from + (self.to - self.from) * ease(p)
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn running(&self, now: Instant) -> bool {
        self.duration > 0. && now.saturating_duration_since(self.start).as_secs_f32() < self.duration
    }

    pub fn go(&mut self, to: f32, duration: f32, now: Instant) {
        if self.to == to {
            return;
        }
        *self = Eased { from: self.value(now), to, start: now, duration };
    }

    pub fn snap(&mut self, to: f32) {
        *self = Eased::at(to);
    }
}

/// What the pointer has done to the rail. Timers are told apart by token: only
/// the latest of each kind counts.
pub struct RailState {
    /// Pointer position in tick units; None while the rail is idle.
    pub position: Option<f32>,
    /// The nearest heading, which the card shows.
    pub selected: Option<usize>,
    pub card_visible: bool,
    pub over_card: bool,
    open_token: u64,
    open_pending: bool,
    close_token: u64,
    swell: Vec<(f32, f32)>,
    last_step: Option<Instant>,
    was_moving: bool,
    pub fade: Eased,
    pub card_top: Eased,
    pub card_height: Eased,
    /// The card's content, as last laid out.
    pub measured: f32,
    /// When the card's heading last changed, for the dip in its opacity.
    dip_from: Option<Instant>,
    pub reduce_motion: bool,
}

impl Default for RailState {
    fn default() -> Self {
        RailState {
            position: None,
            selected: None,
            card_visible: false,
            over_card: false,
            open_token: 0,
            open_pending: false,
            close_token: 0,
            swell: Vec::new(),
            last_step: None,
            was_moving: false,
            fade: Eased::at(0.),
            card_top: Eased::at(0.),
            card_height: Eased::at(0.),
            measured: 0.,
            dip_from: None,
            reduce_motion: false,
        }
    }
}

impl RailState {
    /// The pointer is over the rail at `position`. A token: start the open timer.
    pub fn select(&mut self, position: f32, count: usize, now: Instant) -> Option<u64> {
        self.close_token += 1;
        self.position = Some(position);
        let nearest = (position.round() as usize).min(count.saturating_sub(1));
        if self.selected != Some(nearest) {
            self.selected = Some(nearest);
            if self.card_visible && !self.reduce_motion {
                self.dip_from = Some(now);
            }
        }
        if !self.card_visible && !self.open_pending {
            self.open_pending = true;
            self.open_token += 1;
            return Some(self.open_token);
        }
        None
    }

    /// The open timer ran out.
    pub fn open_fired(&mut self, token: u64) -> bool {
        if token != self.open_token || !self.open_pending {
            return false;
        }
        self.open_pending = false;
        self.card_visible = true;
        true
    }

    /// The pointer left the rail or the card: a token for the close timer.
    pub fn schedule_close(&mut self) -> u64 {
        self.open_pending = false;
        self.open_token += 1;
        self.close_token += 1;
        self.close_token
    }

    /// The close timer ran out.
    pub fn close_fired(&mut self, token: u64) -> bool {
        if token != self.close_token || self.over_card {
            return false;
        }
        self.close();
        true
    }

    pub fn card_hovered(&mut self, inside: bool) -> Option<u64> {
        self.over_card = inside;
        if inside {
            self.close_token += 1;
            None
        } else {
            Some(self.schedule_close())
        }
    }

    pub fn close(&mut self) {
        self.open_pending = false;
        self.open_token += 1;
        self.close_token += 1;
        self.card_visible = false;
        self.over_card = false;
        self.position = None;
    }

    /// Moves every tick toward its proximity. True while any is still moving.
    fn step_ticks(&mut self, count: usize, now: Instant) -> bool {
        self.swell.resize(count, (0., 0.));
        // After a rest, one frame's worth: the time since says nothing about it.
        let dt = match self.last_step {
            Some(at) if self.was_moving => now.saturating_duration_since(at).as_secs_f32(),
            _ => 1. / 120.,
        };
        self.last_step = Some(now);
        let mut moving = false;
        for (i, (value, velocity)) in self.swell.iter_mut().enumerate() {
            let target = proximity(i, self.position);
            if self.reduce_motion {
                *value = target;
                *velocity = 0.;
                continue;
            }
            let (x, v) = spring_step(*value, *velocity, target, dt);
            if (x - target).abs() < 0.001 && v.abs() < 0.01 {
                *value = target;
                *velocity = 0.;
            } else {
                *value = x;
                *velocity = v;
                moving = true;
            }
        }
        self.was_moving = moving;
        moving
    }

    /// New content arrives at 45% and settles over 0.09 s.
    fn content_opacity(&mut self, now: Instant) -> f32 {
        let Some(at) = self.dip_from else { return 1. };
        let p = now.saturating_duration_since(at).as_secs_f32() / 0.09;
        if p >= 1. {
            self.dip_from = None;
            return 1.;
        }
        0.45 + 0.55 * p
    }
}

impl Editor {
    /// The outline for what's in the note now. Off the main thread, as the text
    /// may be long; a newer pass replaces this one.
    pub(crate) fn refresh_outline(&mut self, cx: &mut Context<Self>) {
        self.outline_epoch += 1;
        let epoch = self.outline_epoch;
        let rope = self.doc.rope.clone();
        let task = cx.background_spawn(async move { outline(&rope.to_string()) });
        cx.spawn(async move |this: WeakEntity<Editor>, cx| {
            let headings = task.await;
            let _ = this.update(cx, |e, cx| {
                if e.outline_epoch == epoch && *e.outline != headings {
                    e.outline = std::rc::Rc::new(headings);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn rail_navigate(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(heading) = self.outline.get(index) {
            let line = heading.line;
            self.scroll_to_line(line, cx);
        }
    }

    fn rail_pointer(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let view = self.list.viewport_bounds();
        let count = self.outline.len();
        if count == 0 {
            return;
        }
        let metrics = RailMetrics::new(count, f32::from(view.size.height));
        let position = metrics.index_at(f32::from(y - view.top()) - metrics.hit_top());
        if self.rail.position.is_none() {
            self.rail.reduce_motion = fumi_ui::native::reduce_motion();
        }
        if let Some(token) = self.rail.select(position, count, Instant::now()) {
            cx.spawn(async move |this: WeakEntity<Editor>, cx| {
                cx.background_executor().timer(HOVER_DELAY).await;
                let _ = this.update(cx, |e, cx| {
                    if e.rail.open_fired(token) {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn rail_close_soon(&mut self, token: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Editor>, cx| {
            cx.background_executor().timer(HOVER_DELAY).await;
            let _ = this.update(cx, |e, cx| {
                if e.rail.close_fired(token) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The column at the page's right edge, while the note has headings. It takes
    /// its width from the text's pane, so it never covers the centered text.
    pub(crate) fn rail_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let count = self.outline.len();
        if count == 0 {
            self.rail.close();
            self.rail.selected = None;
            return None;
        }
        let c = t();
        let now = Instant::now();
        let pane = f32::from(self.pane_height.unwrap_or(px(0.)));
        let metrics = RailMetrics::new(count, pane);
        let mut animating = self.rail.step_ticks(count, now);

        let ticks: Vec<(f32, f32, f32)> = self
            .outline
            .iter()
            .enumerate()
            .map(|(i, heading)| {
                let (width, opacity) = tick_look(heading.level, self.rail.swell[i].0);
                (PITCH / 2. + metrics.tick_y(i) - 1., width, opacity)
            })
            .collect();
        let ink = c.text;
        let tick_canvas = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                for (y, width, opacity) in &ticks {
                    let at = Bounds::new(
                        point(bounds.right() - px(*width), bounds.top() + px(*y)),
                        size(px(*width), px(2.)),
                    );
                    window.paint_quad(PaintQuad {
                        bounds: at,
                        corner_radii: Corners::all(px(1.)),
                        background: ink.opacity(*opacity).into(),
                        border_widths: Edges::default(),
                        border_color: transparent_black(),
                        border_style: BorderStyle::default(),
                    });
                }
            },
        )
        .absolute()
        .size_full();

        let hit = div()
            .id("outline-rail")
            .absolute()
            .left_0()
            .top(px(metrics.hit_top()))
            .w(px(RAIL_WIDTH))
            .h(px(metrics.hit_height()))
            .occlude()
            .cursor_default()
            .child(tick_canvas)
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| this.rail_pointer(e.position.y, cx)))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !*hovered {
                    let token = this.rail.schedule_close();
                    this.rail_close_soon(token, cx);
                }
            }))
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                let view = this.list.viewport_bounds();
                let metrics = RailMetrics::new(this.outline.len(), f32::from(view.size.height));
                let index = metrics.index_at(f32::from(e.position().y - view.top()) - metrics.hit_top());
                this.rail_navigate(index.round() as usize, cx);
            }));

        let card = self.rail.selected.map(|i| i.min(count - 1)).map(|i| {
            let reduce = self.rail.reduce_motion;
            // The fade and the slide, on opening and closing.
            let fade_to = if self.rail.card_visible { 1. } else { 0. };
            if reduce {
                self.rail.fade.snap(fade_to);
            } else {
                self.rail.fade.go(fade_to, 0.16, now);
            }
            let fade = self.rail.fade.value(now);
            // Follows the card's content, measured last frame.
            let measured = self.rail.measured;
            if self.rail.card_visible && !reduce {
                self.rail.card_height.go(measured, 0.16, now);
            } else {
                self.rail.card_height.snap(measured);
            }
            let top = metrics.card_top(i, measured, pane);
            if self.rail.card_visible && !reduce {
                self.rail.card_top.go(top, 0.18, now);
            } else {
                self.rail.card_top.snap(top);
            }
            let opacity = self.rail.content_opacity(now);
            animating |= self.rail.fade.running(now)
                || self.rail.card_height.running(now)
                || self.rail.card_top.running(now)
                || opacity < 1.;
            let heading = &self.outline[i];
            let measure = cx.weak_entity();
            // Past the card's 1 pt line, which is drawn over it rather than beside it.
            let content = div()
                .absolute()
                .top(px(-1.))
                .left(px(-1.))
                .w(px(CARD_WIDTH))
                .px(px(14.))
                .py(px(12.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .opacity(opacity)
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.text)
                        .truncate()
                        .child(heading.text.clone()),
                )
                .children((!heading.summary.is_empty()).then(|| {
                    div()
                        .text_size(px(12.))
                        .line_height(px(17.))
                        .text_color(c.text3)
                        .line_clamp(3)
                        .child(heading.summary.clone())
                }))
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            let height = f32::from(bounds.size.height);
                            let _ = measure.update(cx, |e, cx| {
                                if e.rail.measured != height {
                                    e.rail.measured = height;
                                    cx.notify();
                                }
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                );
            let slide = if self.rail.reduce_motion { 0. } else { (1. - fade) * 4. };
            let mut card = div()
                .id("outline-card")
                .absolute()
                .left(px(-(CARD_WIDTH + CARD_GAP) + slide))
                .top(px(self.rail.card_top.value(now)))
                .w(px(CARD_WIDTH))
                .h(px(self.rail.card_height.value(now)))
                .rounded(px(10.))
                .bg(c.page)
                .border_1()
                .border_color(c.border.opacity(0.65))
                .opacity(fade)
                .shadow(vec![
                    BoxShadow { color: hsla(0., 0., 0., 0.12), offset: point(px(0.), px(6.)), blur_radius: px(24.), spread_radius: px(0.), inset: false },
                    BoxShadow { color: hsla(0., 0., 0., 0.04), offset: point(px(0.), px(2.)), blur_radius: px(5.), spread_radius: px(0.), inset: false },
                ])
                .font(ui_font())
                .overflow_hidden()
                .child(content);
            if self.rail.card_visible {
                card = card
                    .cursor_default()
                    .occlude()
                    .on_hover(cx.listener(|this, inside: &bool, _, cx| {
                        if let Some(token) = this.rail.card_hovered(*inside) {
                            this.rail_close_soon(token, cx);
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| this.rail_navigate(i, cx)));
            }
            card
        });

        if animating {
            window.request_animation_frame();
        }
        Some(
            div()
                .id("outline-column")
                // Over the page, as Swift's ZStack has it: the text column
                // doesn't move when a note gains its first heading.
                .absolute()
                .top_0()
                .right_0()
                .w(px(RAIL_WIDTH + EDGE_INSET))
                .h_full()
                // Only the tick strip and the card take the pointer; the rest of
                // the column is the page's, for scrolling and the scroller.
                .child(hit)
                .children(card)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn headings_are_found_as_the_swift_finds_them() {
        let text = "# One\n\nbody **a**\n\n## Two\n```\n# not\n```\n  ### Three  \n####### seven\n#none\n#\n";
        let o = outline(text);
        let shown: Vec<(u8, &str, usize)> = o.iter().map(|h| (h.level, h.text.as_str(), h.line)).collect();
        assert_eq!(shown, [(1, "One", 0), (2, "Two", 4), (3, "Three", 8)]);
        assert_eq!(o[0].summary, "body a");
        assert_eq!(o[1].summary, "");
        assert_eq!(o[2].summary, "####### seven #none #");
    }

    #[core::prelude::v1::test]
    fn a_heading_is_found_by_name_whatever_its_case() {
        let text = "# One\n## Two Words\n```\n## Hidden\n```\n";
        assert_eq!(find_heading(text, "two words"), Some(1));
        assert_eq!(find_heading(text, "ONE"), Some(0));
        assert_eq!(find_heading(text, "hidden"), None);
    }

    #[core::prelude::v1::test]
    fn ticks_sit_12_apart_and_compress_beyond_the_rail() {
        let m = RailMetrics::new(5, 800.);
        assert_eq!(m.height, 48.);
        assert_eq!(m.top, 376.);
        assert_eq!(m.tick_y(2), 24.);
        let m = RailMetrics::new(100, 800.);
        assert_eq!(m.height, 520.);
        assert!((m.tick_y(1) - 520. / 99.).abs() < 0.001);
        let m = RailMetrics::new(100, 300.);
        assert_eq!(m.height, 260.);
        assert_eq!(RailMetrics::new(1, 800.).tick_y(0), 0.);
    }

    #[core::prelude::v1::test]
    fn the_pointer_maps_to_ticks() {
        let m = RailMetrics::new(5, 800.);
        assert_eq!(m.index_at(0.), 0.);
        assert_eq!(m.index_at(6.), 0.);
        assert_eq!(m.index_at(6. + 24.), 2.);
        assert_eq!(m.index_at(1000.), 4.);
        assert_eq!(RailMetrics::new(1, 800.).index_at(10.), 0.);
    }

    #[core::prelude::v1::test]
    fn ticks_swell_near_the_pointer() {
        assert_eq!(proximity(3, None), 0.);
        assert_eq!(proximity(3, Some(3.)), 1.);
        assert_eq!(proximity(0, Some(3.5)), 0.);
        assert!((proximity(2, Some(3.)) - (2.5f32 / 3.5).powi(2)).abs() < 0.001);
        assert_eq!(tick_look(1, 0.), (7., 0.22));
        assert_eq!(tick_look(2, 1.), (20., 1.));
        assert_eq!(tick_look(5, 1.), (14., 1.));
    }

    #[core::prelude::v1::test]
    fn the_spring_settles_without_overshoot() {
        let (mut x, mut v) = (0f32, 0f32);
        let mut max = 0f32;
        for _ in 0..120 {
            (x, v) = spring_step(x, v, 1., 1. / 60.);
            max = max.max(x);
        }
        assert!((x - 1.).abs() < 0.001, "{x}");
        assert!(max <= 1.001, "overdamped: {max}");
    }

    #[core::prelude::v1::test]
    fn the_curve_runs_from_zero_to_one_and_starts_fast() {
        assert!(ease(0.).abs() < 0.001);
        assert!((ease(1.) - 1.).abs() < 0.001);
        assert!(ease(0.25) > 0.6);
        let mut last = 0.;
        for i in 0..=20 {
            let v = ease(i as f32 / 20.);
            assert!(v >= last - 0.0001);
            last = v;
        }
    }

    #[core::prelude::v1::test]
    fn the_card_stays_on_the_page() {
        let m = RailMetrics::new(3, 800.);
        assert_eq!(m.card_top(1, 100., 800.), m.top + 12. - 50.);
        let m = RailMetrics::new(100, 300.);
        assert_eq!(m.card_top(0, 200., 300.), 12f32.max(20. - 100.).min(300. - 200. - 12.));
        assert_eq!(m.card_top(99, 100., 300.), 188.);
    }

    #[core::prelude::v1::test]
    fn the_card_opens_after_the_delay_and_closes_after_leaving() {
        let now = Instant::now();
        let mut r = RailState::default();
        let token = r.select(1.2, 5, now).expect("starts the timer");
        assert_eq!(r.selected, Some(1));
        assert_eq!(r.select(1.3, 5, now), None, "already waiting");
        assert!(r.open_fired(token));
        assert!(r.card_visible);
        // Leaving closes it, after the delay.
        let close = r.schedule_close();
        assert!(r.close_fired(close));
        assert!(!r.card_visible);
        assert_eq!(r.position, None);
    }

    #[core::prelude::v1::test]
    fn coming_back_or_onto_the_card_keeps_it_open() {
        let now = Instant::now();
        let mut r = RailState::default();
        let t = r.select(0., 3, now).unwrap();
        r.open_fired(t);
        let close = r.schedule_close();
        // Back on the rail before the delay: the timer's stale.
        assert_eq!(r.select(0., 3, now), None);
        assert!(!r.close_fired(close));
        assert!(r.card_visible);
        // Onto the card.
        let close = r.schedule_close();
        assert_eq!(r.card_hovered(true), None);
        assert!(!r.close_fired(close));
        assert!(r.card_visible);
        // Off the card.
        let close = r.card_hovered(false).unwrap();
        assert!(r.close_fired(close));
        assert!(!r.card_visible);
    }

    #[core::prelude::v1::test]
    fn leaving_before_the_delay_opens_nothing() {
        let now = Instant::now();
        let mut r = RailState::default();
        let t = r.select(0., 3, now).unwrap();
        r.schedule_close();
        assert!(!r.open_fired(t));
        assert!(!r.card_visible);
    }

    #[core::prelude::v1::test]
    fn moving_between_headings_dips_the_content() {
        let now = Instant::now();
        let mut r = RailState::default();
        let t = r.select(0., 3, now).unwrap();
        r.open_fired(t);
        assert_eq!(r.content_opacity(now), 1.);
        r.select(2., 3, now);
        assert_eq!(r.selected, Some(2));
        assert_eq!(r.content_opacity(now), 0.45);
        assert_eq!(r.content_opacity(now + Duration::from_millis(100)), 1.);
    }

    #[core::prelude::v1::test]
    fn reduce_motion_has_no_spring() {
        let now = Instant::now();
        let mut r = RailState { reduce_motion: true, ..Default::default() };
        r.position = Some(1.);
        assert!(!r.step_ticks(3, now));
        assert_eq!(r.swell[1].0, 1.);
    }
}
