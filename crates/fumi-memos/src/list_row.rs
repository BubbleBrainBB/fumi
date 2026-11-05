//! One day of memos, diary style, and one memo in it: its time, its text or its
//! editor, and the `…` menu. `MemoDayGroup`, `MemoRow`, `MemoBody` in Swift.

use crate::inline::{self, BlockDoc};
use crate::logic;
use crate::rich::RichText;
use crate::typo::{self, line_box};
use crate::view::{MemosView, TIP_DELAY, tip};
use fumi_core::memo_format::Memo;
use fumi_core::memo_markdown::Marker;
use fumi_ui::icons;
use fumi_ui::native::{self, MenuRow, weight};
use fumi_ui::theme::t;
use gpui::prelude::*;
use gpui::*;
use std::rc::Rc;

const SLOT: f32 = 22.;
const HEAVY: f64 = 0.56;

/// The memo font's first baseline and natural row height, at 15 pt.
fn text_metrics(font: &Font, window: &Window) -> (f32, f32) {
    let ts = window.text_system();
    let id = ts.resolve_font(font);
    let ascent = f32::from(ts.ascent(id, px(15.))).round();
    let descent = f32::from(ts.descent(id, px(15.))).abs().round();
    (ascent, ascent + descent)
}

/// The Edit, Copy, Show in File and Delete… rows, in order.
fn menu_rows() -> Vec<MenuRow> {
    vec![
        MenuRow::item("Edit"),
        MenuRow::item("Copy"),
        MenuRow::item("Show in File"),
        MenuRow::Separator,
        MenuRow::destructive("Delete…"),
    ]
}

impl MemosView {
    pub(crate) fn render_group(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        let groups = self.groups.clone();
        let group = &groups[index];
        let (year, month) = (self.this_year(), self.this_month());
        let date_column = div()
            .w(px(40.))
            .flex_none()
            .pt(px(2.))
            .flex()
            .flex_col()
            .items_end()
            .gap(px(4.))
            .child(
                div()
                    .font(self.ty.font(false, false))
                    .text_size(px(20.))
                    .line_height(line_box(20.))
                    .text_color(c.text)
                    .whitespace_nowrap()
                    .child(logic::day_number(&group.day)),
            )
            .child(
                div()
                    .flex_none()
                    .font(self.ty.font(false, true))
                    .text_size(px(12.))
                    .line_height(line_box(12.))
                    .text_color(c.faint_text)
                    .whitespace_nowrap()
                    .child(logic::day_subtitle(&group.day, year, month)),
            );
        let rows: Vec<AnyElement> = group.memos.iter().map(|memo| self.render_row(memo, window, cx)).collect();
        div()
            .pt(px(12.))
            .child(
                div()
                    .relative()
                    .pt(px(16.))
                    .pb(px(4.))
                    .flex()
                    .items_start()
                    .gap(px(20.))
                    .when(index > 0, |el| {
                        el.child(div().absolute().top_0().left_0().right_0().h(px(1.)).bg(c.text.opacity(0.07)))
                    })
                    .child(date_column)
                    .child(div().flex_1().min_w_0().flex().flex_col().gap(px(12.)).children(rows)),
            )
            .into_any_element()
    }

    fn render_row(&mut self, memo: &Memo, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        let id = memo.id();
        let editing = self.edits.iter().position(|edit| edit.memo.id() == id);
        let hovered = self.hovered_row.as_deref() == Some(id.as_str());
        let show_menu = hovered && editing.is_none();
        let menu_button = div()
            .id(SharedString::from(format!("memo-menu-{id}")))
            .relative()
            .w(px(24.))
            .h(px(18.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .when(show_menu, |el| {
                let anchor = self.menu_anchor.clone();
                let memo = memo.clone();
                el.child(canvas(move |bounds, _, _| anchor.set(bounds), |_, _, _, _| {}).absolute().size_full())
                    .child(icons::symbol("ellipsis", 12., weight::SEMIBOLD, c.icon))
                    .on_click(cx.listener(move |this, _, window, cx| this.open_menu(memo.clone(), window, cx)))
            });
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font(typo::tabular(self.ty.font(false, true)))
                    .text_size(px(12.))
                    .line_height(line_box(12.))
                    .text_color(c.faint_text)
                    .child(memo.time.clone()),
            )
            .child(menu_button);
        let body = match editing {
            Some(at) => self.render_editor(at, cx),
            None => self.render_body(memo, window, cx),
        };
        div()
            .id(SharedString::from(format!("memo-{id}")))
            .w_full()
            .pb(px(14.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .on_hover(cx.listener({
                let id = id.clone();
                move |this, inside: &bool, _, cx| {
                    if *inside {
                        this.hovered_row = Some(id.clone());
                    } else if this.hovered_row.as_deref() == Some(id.as_str()) {
                        this.hovered_row = None;
                    }
                    cx.notify();
                }
            }))
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn docs_for(&mut self, memo: &Memo) -> Rc<Vec<BlockDoc>> {
        let id = memo.id();
        if let Some((text, docs)) = self.docs.get(&id)
            && *text == memo.text
        {
            return docs.clone();
        }
        let docs = Rc::new(inline::memo_blocks(&id, &memo.text, &self.ty, &t()));
        self.docs.insert(id, (memo.text.clone(), docs.clone()));
        docs
    }

    fn render_body(&mut self, memo: &Memo, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        let docs = self.docs_for(memo);
        let (baseline, natural) = text_metrics(&self.ty.font(false, false), window);
        let host = self.host.clone();
        let text = |doc: &Rc<_>| RichText::new(Rc::clone(doc), host.clone());
        let mut column = div().w_full().flex().flex_col().gap(px(8.));
        for (index, block) in docs.iter().enumerate() {
            let element = match block {
                BlockDoc::Paragraph(doc) => div().w_full().child(text(doc)).into_any_element(),
                BlockDoc::Item { level, marker, line, doc } => {
                    let slot = self.render_marker(memo, marker, *level, *line, index, baseline, natural, cx);
                    div()
                        .w_full()
                        .pl(px(*level as f32 * SLOT))
                        .flex()
                        .items_start()
                        .child(slot)
                        .child(div().flex_1().min_w_0().child(text(doc)))
                        .into_any_element()
                }
                BlockDoc::Quote(doc) => div()
                    .relative()
                    .w_full()
                    .pl(px(14.))
                    .child(div().absolute().top_0().bottom_0().left_0().w(px(2.)).rounded(px(1.)).bg(c.faint_icon))
                    .child(text(doc))
                    .into_any_element(),
                BlockDoc::Code(doc) => div()
                    .w_full()
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(8.))
                    .bg(c.code_block)
                    .child(text(doc))
                    .into_any_element(),
            };
            column = column.child(element);
        }
        column.into_any_element()
    }

    /// Each marker sits on the text's baseline, centered on its x-height like the
    /// editor's drawn ones.
    #[allow(clippy::too_many_arguments)]
    fn render_marker(
        &mut self,
        memo: &Memo,
        marker: &Marker,
        level: usize,
        line: usize,
        index: usize,
        baseline: f32,
        natural: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = t();
        let slot = div().relative().flex_none().min_w(px(SLOT)).h(px(natural));
        match marker {
            Marker::Bullet => {
                let bullet = div().absolute().left(px(4.)).top(px(baseline - 7.5)).w(px(5.)).h(px(5.));
                let bullet = match level % 3 {
                    0 => bullet.rounded_full().bg(c.text),
                    1 => stroke(bullet.rounded_full(), 1.2, c.text),
                    _ => bullet.child(div().m(px(0.5)).w(px(4.)).h(px(4.)).bg(c.text)),
                };
                slot.child(bullet).into_any_element()
            }
            Marker::Ordered(label) => slot
                .child(
                    div()
                        .pr(px(6.))
                        .font(typo::tabular(self.ty.font(false, false)))
                        .text_size(px(15.))
                        .line_height(px(natural))
                        .text_color(c.text)
                        .whitespace_nowrap()
                        .child(label.clone()),
                )
                .into_any_element(),
            Marker::Task { checked } => {
                let memo = memo.clone();
                let id = SharedString::from(format!("memo-task-{}-{index}", memo.id()));
                let tooltip = if *checked { "Mark as not done" } else { "Mark as done" };
                let box_ = div()
                    .id(id)
                    .absolute()
                    .left_0()
                    .top(px(baseline + 2. - 15.))
                    .w(px(15.))
                    .h(px(15.))
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_task(&memo, line, cx)))
                    .tooltip(tip(tooltip))
                    .tooltip_show_delay(TIP_DELAY);
                let box_ = if *checked {
                    box_.bg(c.accent).child(icons::symbol("checkmark", 8.5, HEAVY, white()))
                } else {
                    stroke_stateful(box_, 1.5, c.faint_icon)
                };
                slot.child(box_).into_any_element()
            }
        }
    }

    /// The text swapped for an editor, in place.
    fn render_editor(&mut self, at: usize, cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        let edit = &self.edits[at];
        let id = edit.memo.id();
        let field = edit.field.clone();
        let (save_id, cancel_id) = (id.clone(), id.clone());
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().w_full().px(px(8.)).py(px(6.)).rounded(px(8.)).bg(c.sidebar).child(field))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(settings_button(format!("memo-cancel-{id}"), "Cancel").on_click(cx.listener(
                        move |this, _, _, cx| this.cancel_edit(&cancel_id, cx),
                    )))
                    .child(settings_button(format!("memo-save-{id}"), "Save").on_click(cx.listener(
                        move |this, _, _, cx| this.save_edit(&save_id, cx),
                    ))),
            )
            .into_any_element()
    }

    /// Shows the row's menu from a spawned task: tracking runs a nested run loop,
    /// which must not run while an event handler holds the app.
    fn open_menu(&mut self, memo: Memo, window: &mut Window, cx: &mut Context<Self>) {
        let ns_window = native::ns_window_of(window);
        let anchor = self.menu_anchor.get();
        let at = point(anchor.left(), anchor.bottom());
        cx.spawn_in(window, async move |this, cx| {
            let chosen = native::popup_menu_on(ns_window, &menu_rows(), at);
            let _ = this.update_in(cx, move |this, window, cx| match chosen {
                Some(0) => this.start_edit(memo, window, cx),
                Some(1) => cx.write_to_clipboard(ClipboardItem::new_string(memo.text.clone())),
                Some(2) => this.show_in_file(&memo, cx),
                Some(3) => this.confirm_delete(memo, window, cx),
                _ => {}
            });
        })
        .detach();
    }

    fn show_in_file(&mut self, memo: &Memo, cx: &mut Context<Self>) {
        let day = memo.day.clone();
        self.workspace.update(cx, |w, cx| {
            let Some(id) = w.store.memo_note(&day).map(|note| note.id.clone()) else { return };
            w.model.open_note(Some(&id), false);
            w.navigated(cx);
        });
    }

    fn confirm_delete(&mut self, memo: Memo, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(PromptLevel::Warning, "Delete this memo?", None, &["Delete", "Cancel"], cx);
        cx.spawn(async move |this: WeakEntity<MemosView>, cx| {
            if answer.await == Ok(0) {
                let _ = this.update(cx, |this, cx| this.delete_memo(&memo, cx));
            }
        })
        .detach();
    }
}

fn set_stroke(style: &mut StyleRefinement, width: f32, color: Hsla) {
    let w: AbsoluteLength = px(width).into();
    style.border_widths.top = Some(w);
    style.border_widths.bottom = Some(w);
    style.border_widths.left = Some(w);
    style.border_widths.right = Some(w);
    style.border_color = Some(color);
}

/// A border of any width: GPUI's helpers come in whole points.
pub(crate) fn stroke(mut el: Div, width: f32, color: Hsla) -> Div {
    set_stroke(el.style(), width, color);
    el
}

pub(crate) fn stroke_stateful(mut el: Stateful<Div>, width: f32, color: Hsla) -> Stateful<Div> {
    set_stroke(el.style(), width, color);
    el
}

/// Settings' plain button: Cancel and Save under a memo being edited.
fn settings_button(id: String, title: &'static str) -> Stateful<Div> {
    let c = t();
    let (fill, hover, edge) = if c.dark {
        (hsla(0., 0., 1., 0.12), hsla(0., 0., 1., 0.16), hsla(0., 0., 1., 0.08))
    } else {
        (white(), c.quiet_surface, hsla(0., 0., 0., 0.12))
    };
    stroke_stateful(
        div()
            .id(SharedString::from(id))
            .h(px(26.))
            .px(px(12.))
            .flex()
            .items_center()
            .rounded(px(7.))
            .bg(fill)
            .hover(move |s| s.bg(hover))
            .cursor_pointer()
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.05),
                offset: point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
                inset: false,
            }])
            .font(typo::ui(FontWeight::NORMAL))
            .text_size(px(12.))
            .line_height(line_box(12.))
            .text_color(c.text)
            .child(title),
        0.5,
        edge,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every non-separator row is an index the menu handler acts on.
    #[core::prelude::v1::test]
    fn the_menu_reads_in_swifts_order() {
        let titles: Vec<String> = menu_rows()
            .into_iter()
            .map(|row| match row {
                MenuRow::Item { title, .. } => title,
                MenuRow::Separator => "-".to_string(),
                _ => panic!("unexpected row"),
            })
            .collect();
        assert_eq!(titles, ["Edit", "Copy", "Show in File", "-", "Delete…"]);
    }
}
