//! The strip's geometry and motion, in plain numbers.

use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const BAND_HEIGHT: f32 = 52.;
pub const TAB_HEIGHT: f32 = 40.;
pub const LABEL_HEIGHT: f32 = 28.;
pub const PINNED_WIDTH: f32 = 46.;
pub const MAX_TAB_WIDTH: f32 = 208.;
pub const MIN_TAB_WIDTH: f32 = 96.;
pub const LEADING_INSET: f32 = 14.;
pub const TRAILING_INSET: f32 = 10.;
pub const BUTTON_WIDTH: f32 = 28.;
pub const FLARE: f32 = 10.;
pub const TAB_RADIUS: f32 = 10.;
/// The page's inset from the band's sides, as the shell lays it out.
pub const PAGE_INSET: f32 = 8.;
/// Where the traffic lights and the sidebar toggle end.
pub const TRAFFIC_LIGHTS: f32 = 132.;
/// One trailing toolbar item, the width the Swift strip leaves per icon.
pub const ICON_CELL: f32 = 44.;
/// From the page's right edge to the last icon's cell.
pub const ICON_TRAILING: f32 = 8.;

/// The strip's width: the detail column's (`pageFrame` is measured outside the
/// page's inset), less the trailing icons, the Preview icon when an extension is
/// on, and the traffic lights where the column slides under them.
pub fn strip_width(column_left: f32, column_width: f32, any_extension: bool) -> f32 {
    let width = column_width - 164. - if any_extension { 44. } else { 0. } - (TRAFFIC_LIGHTS - column_left).max(0.);
    width.max(240.)
}

/// Where the title or strip starts in the window.
pub fn leading_x(page_left: f32) -> f32 {
    page_left.max(TRAFFIC_LIGHTS)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripLayout {
    pub tab_width: f32,
    pub overflows: bool,
}

/// Tabs share the room between 96 and 208; below 96 each, the strip scrolls.
pub fn strip_layout(strip_width: f32, pinned: usize, unpinned: usize) -> StripLayout {
    let room = strip_width - LEADING_INSET - TRAILING_INSET - BUTTON_WIDTH - pinned as f32 * PINNED_WIDTH;
    if unpinned == 0 {
        return StripLayout { tab_width: MAX_TAB_WIDTH, overflows: false };
    }
    let share = (room / unpinned as f32).floor();
    StripLayout { tab_width: share.clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH), overflows: share < MIN_TAB_WIDTH }
}

/// One tab's slot in the scrolled content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub x: f32,
    pub width: f32,
}

/// Slots of tabs laid side by side, pinned ones narrow.
pub fn tab_slots(pinned: &[bool], tab_width: f32) -> Vec<Slot> {
    let mut x = LEADING_INSET;
    pinned
        .iter()
        .map(|&is_pinned| {
            let width = if is_pinned { PINNED_WIDTH } else { tab_width };
            let slot = Slot { x, width };
            x += width;
            slot
        })
        .collect()
}

/// The scrolled content's width.
pub fn content_width(slots: &[Slot]) -> f32 {
    slots.last().map_or(LEADING_INSET, |slot| slot.x + slot.width) + TRAILING_INSET
}

/// Which tab is at `x` in the scrolled content.
pub fn tab_at(slots: &[Slot], x: f32) -> Option<usize> {
    slots.iter().position(|slot| x >= slot.x && x < slot.x + slot.width)
}

/// Separators sit between tabs, hidden next to a raised tab.
pub fn shows_separator(index: usize, count: usize, active: usize, hovered: Option<usize>) -> bool {
    let next = index + 1;
    next < count && index != active && next != active && hovered != Some(index) && hovered != Some(next)
}

/// The least scroll that shows `slot`, as `proxy.scrollTo` without an anchor does.
pub fn scroll_to_show(slot: Slot, offset: f32, viewport: f32, content: f32) -> f32 {
    let (left, right) = (slot.x - LEADING_INSET, slot.x + slot.width + TRAILING_INSET);
    let target = if left < offset {
        left
    } else if right > offset + viewport {
        right - viewport
    } else {
        offset
    };
    target.clamp(0., (content - viewport).max(0.))
}

/// One trailing icon's left edge in the window: `index` 0 is the last icon.
pub fn icon_cell_x(band_right: f32, index_from_right: usize) -> f32 {
    band_right - PAGE_INSET - ICON_TRAILING - ICON_CELL * (index_from_right as f32 + 1.)
}

/// A popover under its button: the card's left edge and the arrow's center within
/// the card, kept inside the window.
pub fn popover_x(button_center: f32, width: f32, window_width: f32, margin: f32) -> (f32, f32) {
    let left = (button_center - width / 2.).min(window_width - width - margin).max(margin);
    (left, (button_center - left).clamp(16., width - 16.))
}

/// SwiftUI's `.easeOut`: the cubic bezier (0, 0), (0.58, 1).
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    if t == 0. || t == 1. {
        return t;
    }
    let (x1, y1, x2, y2) = (0.0_f32, 0.0_f32, 0.58_f32, 1.0_f32);
    let curve = |u: f32, a: f32, b: f32| 3. * (1. - u) * (1. - u) * u * a + 3. * (1. - u) * u * u * b + u * u * u;
    let (mut lo, mut hi) = (0., 1.);
    let mut u = t;
    for _ in 0..24 {
        let x = curve(u, x1, x2);
        if (x - t).abs() < 1e-4 {
            break;
        }
        if x < t {
            lo = u;
        } else {
            hi = u;
        }
        u = (lo + hi) / 2.;
    }
    curve(u, y1, y2)
}

/// A number moving to where it's going over a set time, eased out.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Tween {
    pub fn settled(value: f32) -> Self {
        Tween { from: value, to: value, start: Instant::now(), duration: Duration::ZERO }
    }

    pub fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let t = now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * ease_out(t)
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn is_moving(&self, now: Instant) -> bool {
        !self.duration.is_zero() && now.saturating_duration_since(self.start) < self.duration
    }

    /// Heads for `to` from wherever it is now.
    pub fn go(&mut self, to: f32, duration: Duration, now: Instant) {
        if to != self.to {
            *self = Tween { from: self.value(now), to, start: now, duration };
        }
    }

    /// `to` at once, as a window resize moves things.
    pub fn snap(&mut self, to: f32) {
        *self = Tween::settled(to);
    }

    /// `to` keeping any motion under way: it just ends there instead.
    pub fn retarget(&mut self, to: f32, now: Instant) {
        if self.is_moving(now) {
            self.to = to;
        } else {
            self.snap(to);
        }
    }
}

struct TabMotionEntry {
    x: Tween,
    width: Tween,
    opacity: Tween,
}

/// Where each tab is drawn right now. Slots change for two reasons: the tabs
/// changed, which animates, and the window resized, which doesn't, as Swift's
/// `.animation(value: tabs)` has it.
#[derive(Default)]
pub struct TabMotion {
    entries: HashMap<String, TabMotionEntry>,
    signature: Vec<(String, bool)>,
}

/// A tab as drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub x: f32,
    pub width: f32,
    pub opacity: f32,
}

impl TabMotion {
    /// Takes this frame's tabs and slots. `duration` is how long a change takes:
    /// 0.18 s, or 0.16 s under a drag. Returns where to draw each, and whether a
    /// frame should follow.
    pub fn update(
        &mut self,
        tabs: &[(String, bool)],
        slots: &[Slot],
        duration: Duration,
        now: Instant,
    ) -> (Vec<Drawn>, bool) {
        let changed = self.signature != tabs;
        let first = self.signature.is_empty();
        let mut moving = false;
        self.entries.retain(|id, _| tabs.iter().any(|(tab, _)| tab == id));
        let drawn = tabs
            .iter()
            .zip(slots)
            .map(|((id, _), slot)| {
                let entry = self.entries.entry(id.clone()).or_insert_with(|| TabMotionEntry {
                    x: Tween::settled(slot.x),
                    width: Tween::settled(slot.width),
                    opacity: if first { Tween::settled(1.) } else { fading_in(duration, now) },
                });
                if changed {
                    entry.x.go(slot.x, duration, now);
                    entry.width.go(slot.width, duration, now);
                } else {
                    entry.x.retarget(slot.x, now);
                    entry.width.retarget(slot.width, now);
                }
                moving |= entry.x.is_moving(now) || entry.width.is_moving(now) || entry.opacity.is_moving(now);
                Drawn { x: entry.x.value(now), width: entry.width.value(now), opacity: entry.opacity.value(now) }
            })
            .collect();
        if changed {
            self.signature = tabs.to_vec();
        }
        (drawn, moving)
    }

    /// The strip went away: next time it starts in place.
    pub fn reset(&mut self) {
        self.entries.clear();
        self.signature.clear();
    }
}

fn fading_in(duration: Duration, now: Instant) -> Tween {
    Tween { from: 0., to: 1., start: now, duration }
}

/// Whether the pointer entered a new tab during a drag, so the dragged one moves
/// there: `dropEntered`, once per tab. Returns the target to move to.
pub fn drag_target<'a>(under: Option<&'a str>, dragged: &str, last: &mut Option<String>) -> Option<&'a str> {
    match under {
        Some(id) if id == dragged => {
            *last = None;
            None
        }
        Some(id) if last.as_deref() != Some(id) => {
            *last = Some(id.to_string());
            Some(id)
        }
        Some(_) => None,
        None => {
            *last = None;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_width_follows_the_column() {
        assert_eq!(strip_width(240., 960., false), 796.);
        assert_eq!(strip_width(240., 960., true), 752.);
        // The sidebar hidden: the column slides under the traffic lights.
        assert_eq!(strip_width(0., 1200., false), 904.);
        assert_eq!(strip_width(0., 300., false), 240.);
        assert_eq!(leading_x(8.), 132.);
        assert_eq!(leading_x(248.), 248.);
    }

    #[test]
    fn tabs_share_the_room() {
        // room = 500 - 14 - 10 - 28 = 448
        assert_eq!(strip_layout(500., 0, 2), StripLayout { tab_width: 208., overflows: false });
        assert_eq!(strip_layout(500., 0, 3), StripLayout { tab_width: 149., overflows: false });
        assert_eq!(strip_layout(500., 0, 4), StripLayout { tab_width: 112., overflows: false });
        assert_eq!(strip_layout(500., 0, 5), StripLayout { tab_width: 96., overflows: true });
        assert_eq!(strip_layout(500., 2, 3), StripLayout { tab_width: 118., overflows: false });
        assert_eq!(strip_layout(500., 3, 0), StripLayout { tab_width: 208., overflows: false });
    }

    #[test]
    fn slots_run_side_by_side() {
        let slots = tab_slots(&[true, false, false], 100.);
        assert_eq!(slots[0], Slot { x: 14., width: 46. });
        assert_eq!(slots[1], Slot { x: 60., width: 100. });
        assert_eq!(slots[2], Slot { x: 160., width: 100. });
        assert_eq!(content_width(&slots), 270.);
        assert_eq!(tab_at(&slots, 60.), Some(1));
        assert_eq!(tab_at(&slots, 13.), None);
        assert_eq!(tab_at(&slots, 260.), None);
    }

    #[test]
    fn separators_hide_next_to_raised_tabs() {
        // Tabs 0 1 2 3, 1 active.
        assert!(!shows_separator(0, 4, 1, None), "beside the active");
        assert!(!shows_separator(1, 4, 1, None), "the active's own");
        assert!(shows_separator(2, 4, 1, None));
        assert!(!shows_separator(3, 4, 1, None), "the last has none");
        assert!(!shows_separator(2, 4, 1, Some(3)), "beside the hovered");
        assert!(!shows_separator(2, 4, 1, Some(2)), "the hovered's own");
    }

    #[test]
    fn scrolling_shows_a_tab() {
        let slot = Slot { x: 400., width: 100. };
        assert_eq!(scroll_to_show(slot, 0., 300., 800.), 210.);
        assert_eq!(scroll_to_show(slot, 250., 300., 800.), 250.);
        assert_eq!(scroll_to_show(slot, 450., 300., 800.), 386.);
        assert_eq!(scroll_to_show(Slot { x: 700., width: 100. }, 0., 300., 800.), 500.);
    }

    #[test]
    fn icons_and_popovers() {
        assert_eq!(icon_cell_x(1000., 0), 940.);
        assert_eq!(icon_cell_x(1000., 2), 852.);
        assert_eq!(popover_x(500., 240., 1000., 8.), (380., 120.));
        // Near the window's edge the card stays in and the arrow still points.
        assert_eq!(popover_x(960., 240., 1000., 8.), (752., 208.));
    }

    #[test]
    fn easing_is_a_curve_from_0_to_1() {
        assert_eq!(ease_out(0.), 0.);
        assert_eq!(ease_out(1.), 1.);
        assert!(ease_out(0.5) > 0.6, "fast start");
        let mut last = 0.;
        for i in 1..=20 {
            let v = ease_out(i as f32 / 20.);
            assert!(v >= last);
            last = v;
        }
    }

    #[test]
    fn tweens_run_and_retarget() {
        let t0 = Instant::now();
        let mut tween = Tween::settled(0.);
        tween.go(100., Duration::from_millis(100), t0);
        assert!(tween.is_moving(t0));
        assert_eq!(tween.value(t0), 0.);
        let later = t0 + Duration::from_millis(200);
        assert_eq!(tween.value(later), 100.);
        assert!(!tween.is_moving(later));
        tween.retarget(50., later);
        assert_eq!(tween.value(later), 50.);
    }

    fn tabs(ids: &[&str]) -> Vec<(String, bool)> {
        ids.iter().map(|id| (id.to_string(), false)).collect()
    }

    #[test]
    fn motion_animates_tab_changes_but_not_resizes() {
        let d = Duration::from_millis(180);
        let t0 = Instant::now();
        let mut motion = TabMotion::default();
        let slots = tab_slots(&[false, false], 100.);
        let (drawn, moving) = motion.update(&tabs(&["a", "b"]), &slots, d, t0);
        assert!(!moving, "the first frame is in place");
        assert_eq!(drawn[1], Drawn { x: 114., width: 100., opacity: 1. });

        // A resize: no motion.
        let wide = tab_slots(&[false, false], 150.);
        let (drawn, moving) = motion.update(&tabs(&["a", "b"]), &wide, d, t0);
        assert!(!moving);
        assert_eq!(drawn[1].x, 164.);

        // Swapped: both slide; a new one fades in.
        let (drawn, moving) = motion.update(&tabs(&["b", "a"]), &wide, d, t0);
        assert!(moving);
        assert_eq!(drawn[0].x, 164., "b starts where it was");
        assert_eq!(drawn[1].x, 14.);
        let done = t0 + Duration::from_secs(1);
        let (drawn, moving) = motion.update(&tabs(&["b", "a"]), &wide, d, done);
        assert!(!moving);
        assert_eq!(drawn[0].x, 14.);

        let three = tab_slots(&[false, false, false], 150.);
        let (drawn, _) = motion.update(&tabs(&["b", "a", "c"]), &three, d, done);
        assert_eq!(drawn[2].opacity, 0.);
    }

    #[test]
    fn drags_move_once_per_tab_entered() {
        let mut last = None;
        assert_eq!(drag_target(Some("a"), "a", &mut last), None, "over itself");
        assert_eq!(drag_target(Some("b"), "a", &mut last), Some("b"));
        assert_eq!(drag_target(Some("b"), "a", &mut last), None, "already moved there");
        assert_eq!(drag_target(Some("a"), "a", &mut last), None);
        assert_eq!(drag_target(Some("b"), "a", &mut last), Some("b"), "entered again");
        assert_eq!(drag_target(None, "a", &mut last), None);
    }
}
