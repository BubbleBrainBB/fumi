//! The hover tips: SwiftUI's `.help`, drawn in GPUI as the system draws them.

use fumi_ui::theme::t;
use gpui::*;

struct Tip {
    lines: Vec<SharedString>,
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div()
            .pl(px(8.))
            .pt(px(10.))
            .child(
                div()
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
                    .text_size(px(11.))
                    .text_color(c.text2)
                    .children(self.lines.iter().map(|line| div().child(line.clone()))),
            )
    }
}

/// A tip builder for `.tooltip(...)`.
pub fn tip(text: impl Into<String>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: String = text.into();
    move |_, cx| {
        let lines = text.split('\n').map(|line| SharedString::from(line.to_string())).collect();
        cx.new(|_| Tip { lines }).into()
    }
}

/// A tip whose text is worked out only when it shows.
pub fn tip_with(text: impl Fn(&mut App) -> String + 'static) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_, cx| {
        let lines = text(cx).split('\n').map(|line| SharedString::from(line.to_string())).collect();
        cx.new(|_| Tip { lines }).into()
    }
}
