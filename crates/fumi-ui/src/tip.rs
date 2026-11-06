//! The hover tip every control uses, in the place of SwiftUI's `.help`.

use crate::fonts::family_font;
use crate::theme::t;
use gpui::*;

/// How long the pointer rests before a tip shows.
pub const TIP_DELAY: std::time::Duration = std::time::Duration::from_millis(900);

/// A small floating label, one line per `\n`, set off from the pointer.
pub struct Tip {
    lines: Vec<SharedString>,
}

impl Tip {
    pub fn new(text: &str) -> Self {
        Self { lines: split_lines(text) }
    }
}

fn split_lines(text: &str) -> Vec<SharedString> {
    text.split('\n').map(|line| SharedString::from(line.to_string())).collect()
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div().pl(px(8.)).pt(px(10.)).child(
            div()
                .flex()
                .flex_col()
                .max_w(px(360.))
                .px(px(6.))
                .py(px(3.))
                .bg(c.page)
                .border_1()
                .border_color(c.control_border)
                .rounded(px(5.))
                .shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.14),
                    offset: point(px(0.), px(2.)),
                    blur_radius: px(6.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .font(family_font("System"))
                .text_size(px(11.))
                .text_color(c.text2)
                .children(self.lines.iter().map(|line| div().child(line.clone()))),
        )
    }
}

/// A tip builder for `.tooltip(...)`.
pub fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |_, cx| cx.new(|_| Tip::new(&text)).into()
}

/// A tip whose text is worked out only when it shows.
pub fn tip_with(text: impl Fn(&mut App) -> String + 'static) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_, cx| {
        let text = text(cx);
        cx.new(|_| Tip::new(&text)).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn a_tip_keeps_one_line_per_newline() {
        assert_eq!(split_lines("Close tab\n⌘W"), vec![SharedString::from("Close tab"), SharedString::from("⌘W")]);
        assert_eq!(split_lines("One"), vec![SharedString::from("One")]);
    }
}
