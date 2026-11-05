//! The window's place between launches: what `NSWindow`'s frame autosave does for
//! SwiftUI's window. The frame is in GPUI's terms, relative to its display.

use crate::settings;
use gpui::{Bounds, Pixels, Size, point, px, size};

/// A first launch's window.
pub const FIRST_SIZE: (f32, f32) = (1200., 800.);
/// How much of the title bar has to stay on screen for a place to be kept.
const MIN_VISIBLE: f32 = 100.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowFrame {
    pub display: u64,
    pub bounds: Bounds<Pixels>,
}

impl WindowFrame {
    fn encode(&self) -> String {
        let b = self.bounds;
        format!(
            "{} {} {} {} {}",
            self.display,
            f32::from(b.origin.x),
            f32::from(b.origin.y),
            f32::from(b.size.width),
            f32::from(b.size.height)
        )
    }

    fn decode(text: &str) -> Option<Self> {
        let mut parts = text.split(' ');
        let display = parts.next()?.parse().ok()?;
        let mut number = || parts.next()?.parse::<f32>().ok().filter(|n| n.is_finite());
        let (x, y, width, height) = (number()?, number()?, number()?, number()?);
        Some(WindowFrame { display, bounds: Bounds::new(point(px(x), px(y)), size(px(width), px(height))) })
    }

    pub fn load() -> Option<Self> {
        Self::decode(&settings::get_string(settings::keys::WINDOW_FRAME)?)
    }

    pub fn save(&self) {
        settings::set_string(settings::keys::WINDOW_FRAME, &self.encode());
    }

    /// Its size, no smaller than `min` and no larger than the screen.
    pub fn fitted_size(&self, min: Size<Pixels>, visible: Size<Pixels>) -> Size<Pixels> {
        size(
            self.bounds.size.width.max(min.width).min(visible.width),
            self.bounds.size.height.max(min.height).min(visible.height),
        )
    }

    /// Where it opens on a display whose usable area is `visible`: where it was,
    /// unless its title bar would be out of reach, then centered.
    pub fn place(&self, min: Size<Pixels>, visible: Bounds<Pixels>) -> Bounds<Pixels> {
        let fitted = self.fitted_size(min, visible.size);
        let origin = self.bounds.origin;
        let reach = px(MIN_VISIBLE);
        let on_screen = origin.x + fitted.width >= visible.origin.x + reach
            && origin.x <= visible.origin.x + visible.size.width - reach
            && origin.y >= visible.origin.y
            && origin.y <= visible.origin.y + visible.size.height - reach;
        if on_screen { Bounds::new(origin, fitted) } else { Bounds::centered_at(visible.center(), fitted) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: f32, y: f32, w: f32, h: f32) -> WindowFrame {
        WindowFrame { display: 7, bounds: Bounds::new(point(px(x), px(y)), size(px(w), px(h))) }
    }

    fn screen() -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(25.)), size(px(1512.), px(900.)))
    }

    const MIN: Size<Pixels> = Size { width: Pixels::ZERO, height: Pixels::ZERO };

    #[core::prelude::v1::test]
    fn survives_the_defaults() {
        let saved = frame(10., 40.5, 1300., 820.);
        assert_eq!(WindowFrame::decode(&saved.encode()), Some(saved));
        assert_eq!(WindowFrame::decode("7 1 2 3"), None);
        assert_eq!(WindowFrame::decode("nonsense"), None);
    }

    #[core::prelude::v1::test]
    fn stays_where_it_was() {
        let saved = frame(100., 60., 1300., 820.);
        assert_eq!(saved.place(MIN, screen()), saved.bounds);
    }

    #[core::prelude::v1::test]
    fn respects_the_minimum_and_the_screen() {
        let min = size(px(1016.), px(600.));
        assert_eq!(frame(0., 25., 800., 500.).place(min, screen()).size, min);
        assert_eq!(frame(0., 25., 3000., 2000.).place(min, screen()).size, screen().size);
    }

    #[core::prelude::v1::test]
    fn a_title_bar_out_of_reach_comes_back_to_the_center() {
        let lost = frame(4000., 60., 1200., 800.);
        assert_eq!(lost.place(MIN, screen()), Bounds::centered_at(screen().center(), lost.bounds.size));
        let above = frame(100., 0., 1200., 800.);
        assert_eq!(above.place(MIN, screen()).origin, Bounds::centered_at(screen().center(), above.bounds.size).origin);
    }
}
