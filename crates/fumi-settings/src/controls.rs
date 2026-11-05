//! What the sheet, the search panel and the welcome page are built from.

use fumi_ui::theme::t;
use gpui::prelude::*;
use gpui::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const CJK_FALLBACK: &str = "PingFang SC";

/// A family by the name Settings stores: `WysiwygEngine.resolveFont`'s pins, then
/// whatever is installed.
pub fn family_font(family: &str) -> Font {
    let name: String = match family {
        "System" | "" => ".SystemUIFont".into(),
        "SF Mono" => "Menlo".into(),
        other => other.into(),
    };
    let mut f = font(name);
    f.fallbacks = Some(FontFallbacks::from_fonts(vec![CJK_FALLBACK.into()]));
    f
}

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

/// `chevron.right` or, rotated 90 degrees, `chevron.down`, at 9 pt semibold.
pub fn chevron(degrees: f32, color: Hsla) -> impl IntoElement {
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

/// `f` over `1 - v`, for a color at `v` of the way from `a` to `b`.
pub fn mix(a: Hsla, b: Hsla, v: f32) -> Hsla {
    let (ca, cb) = (Rgba::from(a), Rgba::from(b));
    let l = |x: f32, y: f32| x + (y - x) * v;
    Hsla::from(Rgba { r: l(ca.r, cb.r), g: l(ca.g, cb.g), b: l(ca.b, cb.b), a: l(ca.a, cb.a) })
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
