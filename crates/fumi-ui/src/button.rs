//! The buttons Fumi's screens share. Each returns the chrome; the caller adds its
//! label, size of text and what a click does.

use crate::theme::{mix, t};
use gpui::prelude::*;
use gpui::*;

/// How much of the text colour a quiet button takes under the pointer.
pub const HOVER: f32 = 0.05;
/// And while it's held down.
pub const PRESS: f32 = 0.09;

/// A quiet button's fill: nothing at rest, `HOVER` as `hover` reaches 1, `PRESS`
/// while held.
pub fn quiet_fill(hover: f32, pressed: bool) -> Hsla {
    t().text.opacity(if pressed { PRESS } else { HOVER * hover.clamp(0., 1.) })
}

/// A filled control's colour (pop-ups, Settings buttons): the inline fill,
/// deepened by `hover` going to 1.
pub fn control_fill(hover: f32) -> Hsla {
    let c = t();
    mix(c.inline_bg, c.inline_bg.blend(c.text.opacity(HOVER)), hover.clamp(0., 1.))
}

/// The macOS bordered push button: 24 tall, a light fill, a hairline and a faint
/// drop shadow in light mode; deeper under the pointer and while pressed.
pub fn bordered(id: impl Into<ElementId>) -> Stateful<Div> {
    let c = t();
    let (fill, hover) = if c.dark { (hsla(0., 0., 1., 0.1), hsla(0., 0., 1., 0.16)) } else { (white(), c.quiet_surface) };
    let press = c.inline_bg;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(24.))
        .px(px(11.))
        .rounded(px(6.))
        .bg(fill)
        .border_1()
        .border_color(c.control_border)
        .when(!c.dark, |el| {
            el.shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.05),
                offset: point(px(0.), px(0.5)),
                blur_radius: px(0.5),
                spread_radius: px(0.),
                inset: false,
            }])
        })
        .text_color(c.text)
        .whitespace_nowrap()
        .hover(move |s| s.bg(hover))
        .active(move |s| s.bg(press))
}

/// The default button: the bordered button's shape in the accent colour.
pub fn prominent(id: impl Into<ElementId>) -> Stateful<Div> {
    let c = t();
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .h(px(24.))
        .px(px(11.))
        .rounded(px(6.))
        .bg(c.accent)
        .text_color(white())
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .active(|s| s.opacity(0.85))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn a_quiet_fill_is_clear_at_rest_and_deepest_when_pressed() {
        assert_eq!(quiet_fill(0., false).a, 0.);
        assert!(quiet_fill(1., false).a < quiet_fill(0., true).a);
        assert_eq!(quiet_fill(3., false).a, quiet_fill(1., false).a);
    }
}
