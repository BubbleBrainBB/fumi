//! The editor's typeface, written in itself, as a pop-up that opens a searchable
//! list: `FontPicker` in `SettingsView.swift`.

use crate::anim::Fades;
use crate::controls::{Probes, chevron_down, hairline, mix};
use fumi_ui::fonts::family_font;
use crate::mac;
use crate::scroller::{ScrollTarget, Scroller};
use fumi_ui::text_field::{FieldEvent, TextField};
use fumi_ui::theme::t;
use gpui::prelude::*;
use gpui::*;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub const PINNED: [&str; 3] = ["System", "New York", "SF Mono"];
pub const POPOVER_WIDTH: f32 = 250.;
pub const POPOVER_HEIGHT: f32 = 340.;
const ARROW_HEIGHT: f32 = 7.;
const ARROW_WIDTH: f32 = 14.;
const EDGE_MARGIN: f32 = 8.;

/// One row of the list.
#[derive(Clone, Debug, PartialEq)]
pub enum FontRow {
    Font(String),
    Hairline,
}

/// The families worth listing: no hidden `.` ones, none of the pinned, sorted.
pub fn installed_families(all: Vec<String>) -> Vec<String> {
    let mut list: Vec<String> = all.into_iter().filter(|f| !f.starts_with('.') && !PINNED.contains(&f.as_str())).collect();
    list.sort();
    list
}

/// The pinned first, a hairline, then the rest, filtered case-insensitively.
pub fn font_rows(installed: &[String], search: &str) -> Vec<FontRow> {
    let query = search.trim_matches(|c: char| c.is_whitespace() && c != '\n').to_lowercase();
    let matches = |name: &str| query.is_empty() || name.to_lowercase().contains(&query);
    let pinned: Vec<FontRow> = PINNED.iter().filter(|f| matches(f)).map(|f| FontRow::Font(f.to_string())).collect();
    let rest: Vec<FontRow> = installed.iter().filter(|f| matches(f)).map(|f| FontRow::Font(f.clone())).collect();
    let divide = !pinned.is_empty() && !rest.is_empty();
    let mut rows = pinned;
    if divide {
        rows.push(FontRow::Hairline);
    }
    rows.extend(rest);
    rows
}

/// Where the popover goes under its button: centered on it, kept in the window.
/// Returns its left and top, and whether the arrow still points at the button.
pub fn popover_origin(button: Bounds<Pixels>, window: Size<Pixels>) -> (f32, f32, bool) {
    let center = f32::from(button.center().x);
    let (win_w, win_h) = (f32::from(window.width), f32::from(window.height));
    let x = (center - POPOVER_WIDTH / 2.).clamp(EDGE_MARGIN, (win_w - POPOVER_WIDTH - EDGE_MARGIN).max(EDGE_MARGIN));
    let top = f32::from(button.bottom()) + ARROW_HEIGHT + 1.;
    let highest = (win_h - POPOVER_HEIGHT - EDGE_MARGIN).max(EDGE_MARGIN);
    (x, top.min(highest), top <= highest)
}

pub type OnSelect = Rc<dyn Fn(String, &mut Window, &mut App)>;

pub struct FontPicker {
    selection: String,
    on_select: OnSelect,
    is_open: bool,
    closed_at: Option<Instant>,
    field: Entity<TextField>,
    installed: Option<Vec<String>>,
    rows: Vec<FontRow>,
    list: ListState,
    hovered: Option<usize>,
    button_hover: Fades<&'static str>,
    probes: Probes,
    prev_focus: Option<FocusHandle>,
    scroller: Scroller,
    _subs: Vec<Subscription>,
}

impl FontPicker {
    pub fn new(selection: String, on_select: OnSelect, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| {
            let mut f = TextField::new("Search fonts", window, cx);
            f.font_size = px(12.);
            f.line_height = px(15.);
            f
        });
        let list = ListState::new(0, ListAlignment::Top, px(300.));
        let subs = vec![cx.subscribe_in(&field, window, |this, _, event: &FieldEvent, window, cx| match event {
            FieldEvent::Changed => this.search_changed(cx),
            FieldEvent::Cancel => this.close(window, cx),
            _ => {}
        })];
        FontPicker {
            selection,
            on_select,
            is_open: false,
            closed_at: None,
            field,
            installed: None,
            rows: Vec::new(),
            scroller: Scroller::new(ScrollTarget::List(list.clone())),
            list,
            hovered: None,
            button_hover: Fades::new(0.12),
            probes: Probes::default(),
            prev_focus: None,
            _subs: subs,
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn set_selection(&mut self, selection: String, cx: &mut Context<Self>) {
        self.selection = selection;
        cx.notify();
    }

    fn search_changed(&mut self, cx: &mut Context<Self>) {
        let installed = self.installed.as_deref().unwrap_or(&[]);
        self.rows = font_rows(installed, self.field.read(cx).text());
        self.list.reset(self.rows.len());
        self.hovered = None;
        cx.notify();
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.installed.is_none() {
            self.installed = Some(installed_families(mac::font_families()));
        }
        self.is_open = true;
        self.prev_focus = window.focused(cx);
        self.search_changed(cx);
        let focus = self.field.read(cx).focus.clone();
        window.focus(&focus, cx);
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_open {
            return;
        }
        self.is_open = false;
        self.closed_at = Some(Instant::now());
        if let Some(focus) = self.prev_focus.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn render_row(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        let Some(row) = self.rows.get(ix).cloned() else { return div().into_any_element() };
        match row {
            FontRow::Hairline => div().px(px(9.)).py(px(5.)).child(hairline(window)).into_any_element(),
            FontRow::Font(name) => {
                let selected = name == self.selection;
                let hovered = self.hovered == Some(ix);
                let choose = name.clone();
                div()
                    .id(("font-row", ix))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(9.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(c.accent.opacity(0.08)))
                    .when(!selected && hovered, |d| d.bg(c.card_hover))
                    .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                        if *hovering {
                            this.hovered = Some(ix);
                        } else if this.hovered == Some(ix) {
                            this.hovered = None;
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selection = choose.clone();
                        // After this update: the sheet comes back to set our selection.
                        let (on_select, name) = (this.on_select.clone(), choose.clone());
                        window.defer(cx, move |window, cx| on_select(name, window, cx));
                        this.close(window, cx);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font(family_font(&name))
                            .text_size(px(13.))
                            .line_height(px(16.))
                            .text_color(c.text)
                            .truncate()
                            .child(name),
                    )
                    .when(selected, |d| {
                        d.child(div().flex_none().text_size(px(12.)).text_color(c.accent).child("✓"))
                    })
                    .into_any_element()
            }
        }
    }

    fn render_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let button = self.probes.get("typeface")?;
        let c = t();
        let (x, y, arrow) = popover_origin(button, window.viewport_size());
        let thumb = self.scroller.thumb(|this: &mut Self| &mut this.scroller, Instant::now(), window, cx);
        let arrow_x = f32::from(button.center().x) - x - ARROW_WIDTH / 2.;
        let panel = div()
            .id("font-popover")
            .occlude()
            .w(px(POPOVER_WIDTH))
            .h(px(POPOVER_HEIGHT))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .rounded(px(10.))
            .bg(c.page)
            .border_1()
            .border_color(c.control_border)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.2),
                offset: point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
                inset: false,
            }])
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close(window, cx)))
            .on_click(|_, _, _| {})
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(28.))
                    .px(px(9.))
                    .rounded(px(7.))
                    .bg(c.search_field)
                    .text_color(c.text)
                    .child(self.field.clone()),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(list(self.list.clone(), cx.processor(Self::render_row)).size_full())
                    .children(thumb),
            );
        let tail = arrow.then(|| {
            let (fill, edge) = (c.page, c.control_border);
            canvas(
                |_, _, _| {},
                move |b: Bounds<Pixels>, _, window, _| {
                    let (ox, oy) = (f32::from(b.origin.x), f32::from(b.origin.y));
                    let p = |x: f32, y: f32| point(px(ox + x), px(oy + y));
                    let mut shape = PathBuilder::fill();
                    shape.move_to(p(0., ARROW_HEIGHT + 1.));
                    shape.line_to(p(ARROW_WIDTH / 2., 0.));
                    shape.line_to(p(ARROW_WIDTH, ARROW_HEIGHT + 1.));
                    shape.close();
                    if let Ok(path) = shape.build() {
                        window.paint_path(path, fill);
                    }
                    let mut line = PathBuilder::stroke(px(1.));
                    line.move_to(p(0., ARROW_HEIGHT + 1.));
                    line.line_to(p(ARROW_WIDTH / 2., 0.));
                    line.line_to(p(ARROW_WIDTH, ARROW_HEIGHT + 1.));
                    if let Ok(path) = line.build() {
                        window.paint_path(path, edge);
                    }
                },
            )
            .absolute()
            .top(px(-ARROW_HEIGHT))
            .left(px(arrow_x))
            .w(px(ARROW_WIDTH))
            .h(px(ARROW_HEIGHT + 1.))
        });
        Some(
            deferred(
                anchored()
                    .position(point(px(x), px(y)))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(div().relative().child(panel).children(tail)),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

impl Render for FontPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let now = Instant::now();
        let hover = if self.is_open { 1. } else { self.button_hover.value(&"button", now) };
        if self.button_hover.is_animating(now) {
            window.request_animation_frame();
        }
        let fill = mix(c.inline_bg, c.inline_bg.blend(c.text.opacity(0.05)), hover);
        let popover = if self.is_open { self.render_popover(window, cx) } else { None };
        div()
            .id("typeface-button")
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .h(px(24.))
            .min_w(px(120.))
            .px(px(10.))
            .rounded(px(6.))
            .bg(fill)
            .cursor_pointer()
            .on_hover(cx.listener(|this, hovering: &bool, _, cx| {
                this.button_hover.set("button", *hovering, Instant::now());
                cx.notify();
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                // The click that closed it (outside the popover) lands here too.
                let just_closed = this.closed_at.is_some_and(|t| t.elapsed() < Duration::from_millis(250));
                if this.is_open {
                    this.close(window, cx);
                } else if !just_closed {
                    this.open(window, cx);
                }
                cx.notify();
            }))
            .child(self.probes.probe("typeface"))
            .child(
                div()
                    .font(family_font(&self.selection))
                    .text_size(px(13.))
                    .line_height(px(16.))
                    .text_color(c.text)
                    .whitespace_nowrap()
                    .child(self.selection.clone()),
            )
            .child(div().flex_1())
            .child(chevron_down(c.text3))
            .children(popover)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(rows: &[FontRow]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                FontRow::Font(n) => n.clone(),
                FontRow::Hairline => "---".into(),
            })
            .collect()
    }

    #[core::prelude::v1::test]
    fn hidden_and_pinned_families_leave_the_installed_list() {
        let all = vec!["Zapfino".to_string(), ".Hidden".into(), "SF Mono".into(), "Arial".into()];
        assert_eq!(installed_families(all), vec!["Arial", "Zapfino"]);
    }

    #[core::prelude::v1::test]
    fn pinned_come_first_over_a_hairline() {
        let installed = vec!["Arial".to_string()];
        assert_eq!(names(&font_rows(&installed, "")), ["System", "New York", "SF Mono", "---", "Arial"]);
    }

    #[core::prelude::v1::test]
    fn filter_ignores_case_and_edges() {
        let installed = vec!["Arial".to_string(), "Menlo".into()];
        assert_eq!(names(&font_rows(&installed, " ARI ")), ["Arial"]);
        assert_eq!(names(&font_rows(&installed, "york")), ["New York"]);
        assert!(font_rows(&installed, "zzz").is_empty());
    }

    #[core::prelude::v1::test]
    fn popover_sits_under_the_button_inside_the_window() {
        let window = size(px(1000.), px(800.));
        let button = Bounds::new(point(px(600.), px(100.)), size(px(120.), px(24.)));
        let (x, y, arrow) = popover_origin(button, window);
        assert_eq!(x, 660. - 125.);
        assert_eq!(y, 124. + 8.);
        assert!(arrow);
        // Near the right and bottom edges it stays in, and loses the arrow when it must rise.
        let low = Bounds::new(point(px(900.), px(700.)), size(px(120.), px(24.)));
        let (x, y, arrow) = popover_origin(low, window);
        assert_eq!(x, 1000. - 250. - 8.);
        assert_eq!(y, 800. - 340. - 8.);
        assert!(!arrow);
    }
}
