//! The slim overlay scroller every scrolling area uses: no track, a 4 pt rounded
//! thumb in a 7 pt slot that shows while moving, then hides. `QuietScroller.swift`
//! over a GPUI scroll area. Views that route their own mouse events (the editor)
//! use the geometry and `thumb_bar` directly.

use crate::anim::Transition;
use crate::theme::t;
use gpui::prelude::*;
use gpui::*;
use std::time::{Duration, Instant};

pub const THUMB_WIDTH: f32 = 4.;
pub const SLOT_WIDTH: f32 = 7.;
pub const MIN_THUMB: f32 = 24.;
pub const END_INSET: f32 = 2.;
/// How long the thumb stays after the last movement.
pub const LINGER: Duration = Duration::from_millis(900);

/// Where the thumb goes in a viewport: top and height. True length, at least 24
/// tall, inset 2 at each end.
pub fn thumb_geometry(viewport: f32, max_offset: f32, offset: f32) -> Option<(f32, f32)> {
    if max_offset <= 0. || viewport <= 0. {
        return None;
    }
    let slot = (viewport - END_INSET * 2.).max(0.);
    let content = viewport + max_offset;
    let height = (slot * viewport / content).max(MIN_THUMB).min(slot);
    let travel = slot - height;
    let fraction = (offset / max_offset).clamp(0., 1.);
    Some((END_INSET + travel * fraction, height))
}

/// The scroll offset for a thumb dragged by `dy` from `start_offset`.
pub fn drag_offset(viewport: f32, max_offset: f32, start_offset: f32, dy: f32) -> f32 {
    let Some((_, height)) = thumb_geometry(viewport, max_offset, 0.) else { return 0. };
    let travel = viewport - END_INSET * 2. - height;
    if travel <= 0. {
        return start_offset;
    }
    (start_offset + dy / travel * max_offset).clamp(0., max_offset)
}

/// The scroll offset that puts the thumb's top at `top`, for a view that tracks
/// where in the thumb the pointer took hold.
pub fn offset_for_top(viewport: f32, max_offset: f32, top: f32) -> f32 {
    let Some((start, _)) = thumb_geometry(viewport, max_offset, 0.) else { return 0. };
    drag_offset(viewport, max_offset, 0., top - start)
}

/// The thumb's colour at `alpha` of fully shown.
pub fn thumb_color(alpha: f32) -> Hsla {
    t().faint_icon.opacity(0.6 * alpha)
}

/// The thumb itself, for a view that places it: 4 pt wide, rounded, `alpha` shown.
pub fn thumb_bar(alpha: f32) -> Div {
    div().w(px(THUMB_WIDTH)).rounded(px(THUMB_WIDTH / 2.)).bg(thumb_color(alpha))
}

/// What the thumb scrolls.
#[derive(Clone)]
pub enum ScrollTarget {
    Handle(ScrollHandle),
    List(ListState),
}

impl ScrollTarget {
    pub fn offset(&self) -> f32 {
        match self {
            Self::Handle(h) => -f32::from(h.offset().y),
            Self::List(l) => -f32::from(l.scroll_px_offset_for_scrollbar().y),
        }
    }

    pub fn max_offset(&self) -> f32 {
        match self {
            Self::Handle(h) => f32::from(h.max_offset().y),
            Self::List(l) => f32::from(l.max_offset_for_scrollbar().y),
        }
    }

    pub fn viewport(&self) -> f32 {
        match self {
            Self::Handle(h) => f32::from(h.bounds().size.height),
            Self::List(l) => f32::from(l.viewport_bounds().size.height),
        }
    }

    pub fn set_offset(&self, offset: f32) {
        match self {
            Self::Handle(h) => h.set_offset(point(px(0.), px(-offset))),
            Self::List(l) => l.set_offset_from_scrollbar(point(px(0.), px(-offset))),
        }
    }

    /// A list keeps its items measured as they are while the thumb is held.
    fn drag_started(&self) {
        if let Self::List(l) = self {
            l.scrollbar_drag_started();
        }
    }

    fn drag_ended(&self) {
        if let Self::List(l) = self {
            l.scrollbar_drag_ended();
        }
    }
}

pub struct Scroller {
    pub target: ScrollTarget,
    last_offset: f32,
    last_move: Option<Instant>,
    shown: Transition,
    hovered: bool,
    /// Pointer y and scroll offset when the drag began.
    drag: Option<(f32, f32)>,
    hide: Option<Task<()>>,
}

impl Scroller {
    pub fn new(target: ScrollTarget) -> Self {
        Scroller { target, last_offset: 0., last_move: None, shown: Transition::new(false, 0.2), hovered: false, drag: None, hide: None }
    }

    /// Call from the owner's render. Returns the thumb, and asks for the frames
    /// and the timer that hide it.
    pub fn thumb<V: 'static>(
        &mut self,
        get: fn(&mut V) -> &mut Scroller,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Option<AnyElement> {
        let offset = self.target.offset();
        if (offset - self.last_offset).abs() > 0.25 {
            self.last_offset = offset;
            self.last_move = Some(now);
            self.hide = Some(cx.spawn(async move |this: WeakEntity<V>, cx| {
                cx.background_executor().timer(LINGER + Duration::from_millis(10)).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            }));
        }
        let moving = self.last_move.is_some_and(|m| now.saturating_duration_since(m) < LINGER);
        self.shown.set(moving || self.hovered || self.drag.is_some(), now);
        if self.shown.is_animating(now) {
            window.request_animation_frame();
        }
        let alpha = self.shown.value(now);
        let viewport = self.target.viewport();
        let max = self.target.max_offset();
        let (top, height) = thumb_geometry(viewport, max, offset)?;
        if alpha <= 0. {
            return None;
        }
        let entity = cx.entity();
        let entity2 = entity.clone();
        Some(
            div()
                .absolute()
                .top_0()
                .right_0()
                .w(px(SLOT_WIDTH))
                .h(px(viewport))
                .child(
                    div()
                        .id("quiet-thumb")
                        .absolute()
                        .top(px(top))
                        .right(px((SLOT_WIDTH - THUMB_WIDTH) / 2.))
                        .w(px(THUMB_WIDTH))
                        .h(px(height))
                        .rounded(px(THUMB_WIDTH / 2.))
                        .bg(thumb_color(alpha))
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            get(this).hovered = *hovered;
                            cx.notify();
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                let s = get(this);
                                s.drag = Some((f32::from(e.position.y), s.target.offset()));
                                s.target.drag_started();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            let entity = entity.clone();
                            window.on_mouse_event(move |e: &MouseMoveEvent, _, _, cx| {
                                entity.update(cx, |this, cx| {
                                    let s = get(this);
                                    if let Some((y, start)) = s.drag {
                                        let to = drag_offset(s.target.viewport(), s.target.max_offset(), start, f32::from(e.position.y) - y);
                                        s.target.set_offset(to);
                                        cx.notify();
                                    }
                                });
                            });
                            let entity = entity2.clone();
                            window.on_mouse_event(move |_: &MouseUpEvent, _, _, cx| {
                                entity.update(cx, |this, cx| {
                                    let s = get(this);
                                    if s.drag.take().is_some() {
                                        s.target.drag_ended();
                                        cx.notify();
                                    }
                                });
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn no_thumb_when_it_fits() {
        assert_eq!(thumb_geometry(400., 0., 0.), None);
    }

    #[core::prelude::v1::test]
    fn thumb_has_its_true_length_inset_at_both_ends() {
        let (top, h) = thumb_geometry(400., 400., 0.).unwrap();
        assert_eq!(top, 2.);
        assert!((h - 198.).abs() < 0.01);
        let (top, h) = thumb_geometry(400., 400., 400.).unwrap();
        assert!((top + h - 398.).abs() < 0.01);
    }

    #[core::prelude::v1::test]
    fn thumb_stays_at_least_24_tall() {
        let (_, h) = thumb_geometry(100., 100000., 50.).unwrap();
        assert_eq!(h, 24.);
    }

    #[core::prelude::v1::test]
    fn dragging_covers_the_whole_range() {
        let (_, h) = thumb_geometry(400., 400., 0.).unwrap();
        let travel = 396. - h;
        assert!((drag_offset(400., 400., 0., travel) - 400.).abs() < 0.01);
        assert_eq!(drag_offset(400., 400., 0., -50.), 0.);
    }

    #[core::prelude::v1::test]
    fn a_thumb_top_maps_back_to_its_offset() {
        for offset in [0., 100., 250., 400.] {
            let (top, _) = thumb_geometry(400., 400., offset).unwrap();
            assert!((offset_for_top(400., 400., top) - offset).abs() < 0.01, "{offset}");
        }
        assert_eq!(offset_for_top(400., 400., -30.), 0.);
        assert_eq!(offset_for_top(400., 400., 900.), 400.);
    }
}
