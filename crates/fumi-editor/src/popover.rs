//! A link to a note that doesn't exist: "No note named “X” yet." with Cancel and
//! Create Note, under the link. An NSPopover in Swift; drawn here over the page.

use crate::display::ui_font;
use crate::editor::{Editor, EditorEvent};
use fumi_ui::theme::t;
use gpui::prelude::*;
use gpui::*;

const WIDTH: f32 = 280.;
const ARROW: f32 = 7.;

actions!(editor, [OfferCancel, OfferCreate]);

/// The note a clicked link names, and where to open it.
pub struct CreateOffer {
    pub name: String,
    pub new_tab: bool,
    pub anchor: Bounds<Pixels>,
}

/// The name a link's target gives a new note: its last path component.
pub fn name_of_target(target: &str) -> String {
    target.rsplit('/').next().unwrap_or(target).to_string()
}

/// Centered under the link, kept in the window.
pub fn offer_origin(anchor: Bounds<Pixels>, window_width: Pixels) -> Point<Pixels> {
    let x = (anchor.center().x - px(WIDTH / 2.)).min(window_width - px(WIDTH + 8.)).max(px(8.));
    point(x, anchor.bottom() + px(ARROW))
}

impl Editor {
    pub(crate) fn offer_create(&mut self, offer: CreateOffer, window: &mut Window, cx: &mut Context<Self>) {
        self.offer = Some(offer);
        window.focus(&self.offer_focus, cx);
        cx.notify();
    }

    fn close_offer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.offer.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    fn accept_offer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(offer) = self.offer.take() {
            window.focus(&self.focus, cx);
            cx.emit(EditorEvent::CreateNote { name: offer.name, new_tab: offer.new_tab });
            cx.notify();
        }
    }

    pub(crate) fn offer_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let offer = self.offer.as_ref()?;
        let c = t();
        let origin = offer_origin(offer.anchor, window.viewport_size().width);
        let arrow_x = (offer.anchor.center().x - origin.x).clamp(px(16.), px(WIDTH - 16.));
        let page = c.page;
        let arrow = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let mut path = PathBuilder::fill();
                let (x, y) = (bounds.origin.x, bounds.origin.y);
                path.move_to(point(x, y + px(ARROW)));
                path.line_to(point(x + px(ARROW), y));
                path.line_to(point(x + px(ARROW * 2.), y + px(ARROW)));
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, page);
                }
            },
        )
        .absolute()
        .left(arrow_x - px(ARROW))
        .top(px(-ARROW))
        .w(px(ARROW * 2.))
        .h(px(ARROW));
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .h(px(24.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .text_size(px(13.))
                .cursor_default()
                .child(label)
        };
        let on_accent = if c.accent.l > 0.65 { hsla(0., 0., 0., 0.9) } else { hsla(0., 0., 1., 1.) };
        let body = div()
            .id("create-note-offer")
            .key_context("CreateNoteOffer")
            .track_focus(&self.offer_focus)
            .relative()
            .w(px(WIDTH))
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .bg(c.page)
            .rounded(px(10.))
            .font(ui_font())
            .shadow(vec![
                BoxShadow { color: hsla(0., 0., 0., 0.16), offset: point(px(0.), px(8.)), blur_radius: px(24.), spread_radius: px(0.), inset: false },
                BoxShadow { color: hsla(0., 0., 0., 0.10), offset: point(px(0.), px(0.)), blur_radius: px(1.), spread_radius: px(0.5), inset: false },
            ])
            .occlude()
            .cursor_default()
            .on_action(cx.listener(|this, _: &OfferCancel, window, cx| this.close_offer(window, cx)))
            .on_action(cx.listener(|this, _: &OfferCreate, window, cx| this.accept_offer(window, cx)))
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close_offer(window, cx)))
            .child(arrow)
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(c.text2)
                    .child(format!("No note named “{}” yet.", offer.name)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button("offer-cancel", "Cancel")
                            .border_1()
                            .border_color(c.control_border)
                            .text_color(c.text)
                            .on_click(cx.listener(|this, _, window, cx| this.close_offer(window, cx))),
                    )
                    .child(
                        button("offer-create", "Create Note")
                            .bg(c.accent)
                            .text_color(on_accent)
                            .on_click(cx.listener(|this, _, window, cx| this.accept_offer(window, cx))),
                    ),
            );
        Some(deferred(anchored().position(origin).snap_to_window().child(body)).with_priority(2).into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn the_name_is_the_last_path_component() {
        assert_eq!(name_of_target("a/b/Note"), "Note");
        assert_eq!(name_of_target("Note"), "Note");
    }

    #[core::prelude::v1::test]
    fn it_sits_under_the_link_inside_the_window() {
        let link = |x: f32| Bounds::new(point(px(x), px(100.)), size(px(40.), px(20.)));
        let at = offer_origin(link(400.), px(1000.));
        assert_eq!((at.x, at.y), (px(420. - 140.), px(127.)));
        assert_eq!(offer_origin(link(0.), px(1000.)).x, px(8.));
        assert_eq!(offer_origin(link(960.), px(1000.)).x, px(1000. - 288.));
    }
}
