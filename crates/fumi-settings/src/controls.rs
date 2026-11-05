//! What the sheet, the search panel and the welcome page are built from.

use fumi_ui::fonts::family_font;
pub use fumi_ui::theme::mix;
use fumi_ui::theme::t;
use gpui::prelude::*;
use gpui::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// `.monospacedDigit()`.
pub fn tabular_digits() -> FontFeatures {
    FontFeatures(std::sync::Arc::new(vec![("tnum".to_string(), 1)]))
}

/// One device pixel of `text` at 9%, between groups.
pub fn hairline(window: &Window) -> Div {
    div().w_full().h(px(1. / window.scale_factor())).flex_none().bg(t().text.opacity(0.09))
}

/// The 13 pt line every control uses.
pub fn body_text(text: impl Into<SharedString>) -> Div {
    let c = t();
    div().text_size(px(13.)).line_height(px(16.)).text_color(c.text).whitespace_nowrap().child(text.into())
}

/// The system tooltip's job: a small floating label.
pub struct Tip(pub SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div()
            .px(px(6.))
            .py(px(3.))
            .rounded(px(4.))
            .bg(c.page)
            .border_1()
            .border_color(c.control_border)
            .text_size(px(11.))
            .text_color(c.text2)
            .whitespace_nowrap()
            .child(self.0.clone())
    }
}

pub fn tip(text: &'static str) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_, cx| cx.new(|_| Tip(text.into())).into()
}

/// Remembers where elements landed, for what opens beside them.
#[derive(Clone, Default)]
pub struct Probes(Rc<RefCell<HashMap<&'static str, Bounds<Pixels>>>>);

impl Probes {
    pub fn get(&self, key: &'static str) -> Option<Bounds<Pixels>> {
        self.0.borrow().get(key).copied()
    }

    /// A child that fills its positioned parent and records its bounds.
    pub fn probe(&self, key: &'static str) -> impl IntoElement + use<> {
        let map = self.0.clone();
        canvas(
            move |bounds, _, _| {
                map.borrow_mut().insert(key, bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }
}

/// `chevron.down` at 9 pt semibold: a pop-up's mark.
pub fn chevron_down(color: Hsla) -> impl IntoElement {
    div()
        .w(px(10.))
        .h(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .child(fumi_ui::icons::symbol("chevron.down", 9., fumi_ui::native::weight::SEMIBOLD, color))
}

/// A disclosure mark turning from right (`p` 0) to down (`p` 1). The system's glyph
/// at rest; images don't rotate, so a stroke of its shape while it turns.
pub fn disclosure(p: f32, color: Hsla) -> AnyElement {
    let rest = |name: &str| {
        div()
            .w(px(10.))
            .h(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .child(fumi_ui::icons::symbol(name, 9., fumi_ui::native::weight::SEMIBOLD, color))
            .into_any_element()
    };
    if p <= 0. {
        return rest("chevron.right");
    }
    if p >= 1. {
        return rest("chevron.down");
    }
    let degrees = 90. * p;
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let center = bounds.center();
            let (cx_, cy) = (f32::from(center.x), f32::from(center.y));
            let (sin, cos) = degrees.to_radians().sin_cos();
            let at = |x: f32, y: f32| point(px(cx_ + x * cos - y * sin), px(cy + x * sin + y * cos));
            let mut path = PathBuilder::stroke(px(1.5));
            path.move_to(at(-1.8, -3.8));
            path.line_to(at(1.8, 0.));
            path.line_to(at(-1.8, 3.8));
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w(px(10.))
    .h(px(10.))
    .flex_none()
    .into_any_element()
}

/// The sidebar's closed folder, `FolderGlyph` stroked at 1.15 in 16 x 16.
pub fn folder_glyph(color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let k = 16. / 24.;
            let (ox, oy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
            let p = |x: f32, y: f32| point(px(ox + x * k), px(oy + y * k));
            let mut path = PathBuilder::stroke(px(1.15));
            path.move_to(p(3., 7.));
            path.curve_to(p(5., 5.), p(3., 5.));
            path.line_to(p(9., 5.));
            path.line_to(p(11., 7.));
            path.line_to(p(19., 7.));
            path.curve_to(p(21., 9.), p(21., 7.));
            path.line_to(p(21., 17.));
            path.curve_to(p(19., 19.), p(21., 19.));
            path.line_to(p(5., 19.));
            path.curve_to(p(3., 17.), p(3., 19.));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w(px(16.))
    .h(px(16.))
    .flex_none()
}

/// Cuts the middle out of `text` so it measures at most `max_width`, as
/// `.truncationMode(.middle)`.
pub fn truncate_middle(text: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(text) <= max_width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let cut = |keep: usize| -> String {
        let head = keep.div_ceil(2);
        let tail = keep / 2;
        let mut out: String = chars[..head].iter().collect();
        out.push('…');
        out.extend(&chars[chars.len() - tail..]);
        out
    };
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if measure(&cut(mid)) <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    cut(lo)
}

/// How wide `text` draws at `size`.
pub fn text_width(text: &str, size: f32, window: &Window) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: family_font("System"),
        color: t().text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().shape_line(text.to_string().into(), px(size), &[run], None);
    f32::from(line.width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn short_text_is_left_alone() {
        assert_eq!(truncate_middle("abc", 10., |s| s.chars().count() as f32), "abc");
    }

    #[core::prelude::v1::test]
    fn long_text_loses_its_middle() {
        let out = truncate_middle("~/Documents/Long/Path/Notes", 10., |s| s.chars().count() as f32);
        assert_eq!(out.chars().count(), 10);
        assert!(out.starts_with("~/Do"));
        assert!(out.contains('…'));
        assert!(out.ends_with("otes"));
    }

    #[core::prelude::v1::test]
    fn narrower_than_the_ellipsis_keeps_only_it() {
        assert_eq!(truncate_middle("abcdef", 0.5, |s| s.chars().count() as f32), "…");
    }

    #[core::prelude::v1::test]
    fn mix_goes_end_to_end() {
        let a = Hsla::from(rgb(0x000000));
        let b = Hsla::from(rgb(0xFFFFFF));
        assert!(Rgba::from(mix(a, b, 0.)).r < 0.01);
        assert!(Rgba::from(mix(a, b, 1.)).r > 0.99);
    }
}
