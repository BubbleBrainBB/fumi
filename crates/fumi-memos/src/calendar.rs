//! A month, Monday first. A day's number sits on an accent wash that deepens with
//! how many memos it has; today has an accent ring. Hover a day for its memos,
//! click it to see only that day. `MemoMonthCalendar` and `MemoDayCard` in Swift.

use crate::list_row::stroke;
use crate::logic::{self, CARD_MEMOS, CARD_WIDTH, CELL, HoverEffect, ROW_HEIGHT};
use crate::typo::{self, line_box};
use crate::view::{MemosView, ease_out};
use fumi_ui::icons;
use fumi_ui::native::weight;
use fumi_ui::theme::t;
use gpui::prelude::*;
use gpui::*;
use std::time::Duration;

const WEEKDAYS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

impl MemosView {
    pub(crate) fn render_calendar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        let (year, month) = self.month;
        let weeks = logic::month_cells(year, month);
        let counts = self.counts.clone();
        let days = || weeks.iter().flatten().flatten();
        let month_count: usize = days().map(|day| counts.get(day).copied().unwrap_or(0)).sum();
        let active_days = days().filter(|day| counts.get(*day).copied().unwrap_or(0) > 0).count();

        let header = div()
            .pb(px(10.))
            .flex()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .text_size(px(15.))
                    .line_height(line_box(15.))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .font(typo::serif(FontWeight::SEMIBOLD, false))
                            .text_color(c.text)
                            .child(logic::month_name(month)),
                    )
                    .child(
                        div()
                            .font(typo::serif(FontWeight::NORMAL, false))
                            .text_color(c.faint_text)
                            .child(format!(" {year}")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(2.))
                    .child(self.month_arrow(0, "chevron.left", "Previous Month", -1, cx))
                    .child(self.month_arrow(1, "chevron.right", "Next Month", 1, cx)),
            );

        let weekday_row = div().pb(px(2.)).flex().children(WEEKDAYS.iter().map(|letter| {
            div()
                .w(px(CELL))
                .flex()
                .justify_center()
                .font(typo::ui(FontWeight::NORMAL))
                .text_size(px(12.))
                .line_height(line_box(12.))
                .text_color(c.faint_text)
                .child(*letter)
        }));

        let mut grid = div().flex().flex_col();
        for week in &weeks {
            let mut row = div().flex();
            for (column, cell) in week.iter().enumerate() {
                row = row.child(match cell {
                    Some(day) => self.day_cell(day, column, cx),
                    None => div().w(px(CELL)).h(px(ROW_HEIGHT)).into_any_element(),
                });
            }
            grid = grid.child(row);
        }

        div()
            .w(px(CELL * 7.))
            .flex()
            .flex_col()
            .child(header)
            .child(weekday_row)
            .child(grid)
            .child(
                div()
                    .pt(px(10.))
                    .font(typo::ui(FontWeight::NORMAL))
                    .text_size(px(12.))
                    .line_height(line_box(12.))
                    .text_color(c.faint_text)
                    .child(logic::month_footer(month_count, active_days)),
            )
            .into_any_element()
    }

    fn month_arrow(
        &mut self,
        slot: usize,
        symbol: &'static str,
        label: &'static str,
        by: i32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let c = t();
        let hovered = self.arrow_hover[slot];
        div()
            .id(label)
            .aria_label(label)
            .w(px(20.))
            .h(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .when(hovered, |el| el.bg(c.card_hover))
            .cursor_pointer()
            .on_hover(cx.listener(move |this, inside: &bool, _, cx| {
                this.arrow_hover[slot] = *inside;
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.month = logic::shift_month(this.month.0, this.month.1, by);
                cx.notify();
            }))
            .child(icons::symbol(symbol, 10., weight::SEMIBOLD, if hovered { c.text2 } else { c.faint_text }))
    }

    fn day_cell(&mut self, day: &str, column: usize, cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        let count = self.counts.get(day).copied().unwrap_or(0);
        let level = fumi_core::memo_format::heatmap::level(count as i64);
        let today = self.today == day;
        let selected = self.filter.day.as_deref() == Some(day);
        let number_color = if selected {
            white()
        } else if today {
            c.accent
        } else if count > 0 {
            c.text
        } else {
            c.faint_icon
        };
        let weight = if selected || today {
            FontWeight::SEMIBOLD
        } else if count > 0 {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        };
        let number = logic::civil(day).map_or(0, |date| date.day);
        let fill = if selected { c.accent } else { c.accent.opacity(logic::wash(level, c.dark)) };
        let mut square = div()
            .w(px(24.))
            .h(px(24.))
            .rounded(px(7.))
            .bg(fill)
            .flex()
            .items_center()
            .justify_center()
            .font(typo::tabular(typo::ui(weight)))
            .text_size(px(12.))
            .line_height(line_box(12.))
            .text_color(number_color)
            .child(number.to_string());
        if today && !selected {
            square = stroke(square, 1.5, c.accent);
        }
        let label = format!(
            "{} {number}, {}",
            logic::civil(day).map_or("", |date| logic::month_name(date.month)),
            logic::plural(count, "memo"),
        );
        let hover_day = day.to_string();
        let click_day = day.to_string();
        div()
            .id(SharedString::from(format!("memo-day-{day}")))
            .aria_label(label)
            .relative()
            .w(px(CELL))
            .h(px(ROW_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .on_hover(cx.listener(move |this, inside: &bool, _, cx| this.day_hover(&hover_day, count, *inside, cx)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.filter.click_day(&click_day, count);
                this.card.clicked();
                this.card_fade = Default::default();
                this.card_timer += 1;
                this.apply_filter(cx);
            }))
            .child(square)
            .children(self.card_for(day, column))
            .into_any_element()
    }

    fn day_hover(&mut self, day: &str, count: usize, inside: bool, cx: &mut Context<Self>) {
        match self.card.hover(day, count, inside) {
            HoverEffect::Nothing => {}
            HoverEffect::HideNow => {
                self.card_timer += 1;
                self.card_fade = Default::default();
                cx.notify();
            }
            HoverEffect::Follow => {
                self.card_timer += 1;
                // Day to day without a fade.
                self.card_fade.fade_in = None;
                self.card_fade.fading = None;
                cx.notify();
            }
            HoverEffect::ShowLater(day) => {
                self.card_timer += 1;
                let timer = self.card_timer;
                self.after(350, timer, cx, move |this, cx| {
                    if this.card.show_due(&day) {
                        this.card_fade.generation += 1;
                        this.card_fade.fade_in = Some(this.card_fade.generation);
                        this.card_fade.fading = None;
                        cx.notify();
                    }
                });
            }
            HoverEffect::HideLater => {
                self.card_timer += 1;
                let timer = self.card_timer;
                self.after(80, timer, cx, |this, cx| {
                    if let Some(day) = this.card.hide_due() {
                        this.card_fade.generation += 1;
                        let generation = this.card_fade.generation;
                        this.card_fade.fade_in = None;
                        this.card_fade.fading = Some((day, generation));
                        cx.notify();
                        cx.spawn(async move |this: WeakEntity<MemosView>, cx| {
                            cx.background_executor().timer(Duration::from_millis(120)).await;
                            let _ = this.update(cx, |this, cx| {
                                if this.card_fade.fading.as_ref().is_some_and(|(_, g)| *g == generation) {
                                    this.card_fade.fading = None;
                                    cx.notify();
                                }
                            });
                        })
                        .detach();
                    }
                });
            }
        }
    }

    /// Runs `then` after `ms`, unless the hover moved on: a timer's number is only
    /// current until the next hover.
    fn after(
        &mut self,
        ms: u64,
        timer: u64,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        cx.spawn(async move |this: WeakEntity<MemosView>, cx| {
            cx.background_executor().timer(Duration::from_millis(ms)).await;
            let _ = this.update(cx, |this, cx| {
                if this.card_timer == timer {
                    then(this, cx);
                }
            });
        })
        .detach();
    }

    /// The card over this day, if it's the one showing or fading away.
    fn card_for(&mut self, day: &str, column: usize) -> Option<AnyElement> {
        let fading = self.card_fade.fading.as_ref().filter(|(d, _)| d == day).map(|(_, g)| *g);
        let shown = self.card.card.as_deref() == Some(day);
        if !shown && fading.is_none() {
            return None;
        }
        let (x, y) = logic::card_offset(column);
        let body = div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(CARD_WIDTH));
        let content = self.card_content(day);
        let duration = Duration::from_millis(120);
        let element = if let Some(generation) = fading {
            body.child(content)
                .with_animation(("memo-card-out", generation), Animation::new(duration).with_easing(ease_out), |el, d| {
                    el.opacity(1. - d)
                })
                .into_any_element()
        } else if let Some(generation) = self.card_fade.fade_in {
            body.child(content)
                .with_animation(("memo-card-in", generation), Animation::new(duration).with_easing(ease_out), |el, d| {
                    el.opacity(d)
                })
                .into_any_element()
        } else {
            body.child(content).into_any_element()
        };
        Some(deferred(element).with_priority(1).into_any_element())
    }

    /// A day's memos at a glance: the date, how many, each memo's time and first line.
    fn card_content(&self, day: &str) -> Div {
        let c = t();
        let memos: Vec<_> = self.memos.iter().filter(|memo| memo.day == day).collect();
        let mut rows = div().flex().flex_col().gap(px(5.));
        for memo in memos.iter().take(CARD_MEMOS) {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .w(px(36.))
                            .flex_none()
                            .font(typo::tabular(self.ty.font(false, true)))
                            .text_size(px(12.))
                            .line_height(line_box(12.))
                            .text_color(c.faint_text)
                            .child(memo.time.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font(self.ty.font(false, false))
                            .text_size(px(12.))
                            .line_height(line_box(12.))
                            .text_color(c.text)
                            .child(logic::first_line(&memo.text).to_string()),
                    ),
            );
        }
        if memos.len() > CARD_MEMOS {
            rows = rows.child(
                div()
                    .pl(px(46.))
                    .font(typo::ui(FontWeight::NORMAL))
                    .text_size(px(12.))
                    .line_height(line_box(12.))
                    .text_color(c.faint_text)
                    .child(format!("{} more", memos.len() - CARD_MEMOS)),
            );
        }
        stroke(
            div()
                .w_full()
                .px(px(14.))
                .py(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .rounded(px(11.))
                .bg(c.page)
                .shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.14),
                    offset: point(px(0.), px(10.)),
                    blur_radius: px(40.),
                    spread_radius: px(0.),
                inset: false,
                }])
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font(typo::serif(FontWeight::SEMIBOLD, false))
                                .text_size(px(13.))
                                .line_height(line_box(13.))
                                .text_color(c.text)
                                .child(logic::long_day_label(day, self.this_year())),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font(typo::ui(FontWeight::NORMAL))
                                .text_size(px(12.))
                                .line_height(line_box(12.))
                                .text_color(c.faint_text)
                                .child(logic::plural(memos.len(), "memo")),
                        ),
                )
                .child(rows),
            1.,
            c.text.opacity(0.08),
        )
    }
}
