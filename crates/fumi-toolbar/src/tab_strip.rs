//! The row of paper tabs. The active tab is the page's own color and its lower
//! corners flare out into the page; the others are gray titles on the ground, split
//! by hairlines. Port of `TabStrip`.

use crate::labels;
use crate::layout::{self, FLARE, LABEL_HEIGHT, TAB_HEIGHT, TAB_RADIUS};
use crate::menu_rows::MenuSpec;
use fumi_ui::tip::{tip, tip_with};
use crate::toolbar::{Toolbar, edited};
use fumi_core::tabs::{Content, NoteTab};
use fumi_ui::icons::{glyph, symbol};
use fumi_ui::theme::{t, text_size};
use fumi_workspace::Workspace;
use gpui::prelude::*;
use gpui::*;
use std::time::{Duration, Instant};

/// The tab being dragged.
#[derive(Clone)]
struct DraggedTab {
    id: String,
}

/// What follows the pointer during a drag.
struct TabGhost {
    font: Font,
    title: SharedString,
    italic: bool,
    width: f32,
}

impl Render for TabGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div()
            .w(px(self.width))
            .h(px(LABEL_HEIGHT))
            .flex()
            .items_center()
            .pl(px(14.))
            .pr(px(14.))
            .rounded(px(TAB_RADIUS))
            .bg(c.page)
            .border_1()
            .border_color(c.page_edge)
            .opacity(0.85)
            .font(self.font.clone())
            .text_size(px(text_size::BODY))
            .font_weight(FontWeight::MEDIUM)
            .text_color(c.text2)
            .when(self.italic, |el| el.italic())
            .child(div().min_w_0().truncate().child(self.title.clone()))
    }
}

/// The index tab's outline, `width` including its flares.
fn paper_tab(origin: Point<Pixels>, width: f32, height: f32, mut path: PathBuilder, closed: bool) -> Option<Path<Pixels>> {
    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
    let flare = point(px(FLARE), px(FLARE));
    let round = point(px(TAB_RADIUS), px(TAB_RADIUS));
    let (w, h, f, r) = (width, height, FLARE, TAB_RADIUS);
    path.move_to(at(0., h));
    path.arc_to(flare, px(0.), false, false, at(f, h - f));
    path.line_to(at(f, r));
    path.arc_to(round, px(0.), false, true, at(f + r, 0.));
    path.line_to(at(w - f - r, 0.));
    path.arc_to(round, px(0.), false, true, at(w - f, r));
    path.line_to(at(w - f, h - f));
    path.arc_to(flare, px(0.), false, false, at(w, h));
    if closed {
        path.close();
    }
    path.build().ok()
}

fn tab_help_text(workspace: &Entity<Workspace>, content: &Content, cx: &App) -> String {
    let w = workspace.read(cx);
    let note = match content {
        Content::Note(id) => w.note(id),
        _ => None,
    };
    let file = match content {
        Content::File(id) => w.store.files().iter().find(|file| file.id == *id),
        _ => None,
    };
    let label = note.map(|n| n.updated_at).or(file.map(|f| f.updated_at)).map(edited).unwrap_or_default();
    labels::tab_help(content, note, file, &label)
}

impl Toolbar {
    fn refresh_titles(&mut self, tabs: &[NoteTab], cx: &App) {
        let w = self.workspace.read(cx);
        if self.titles_revision != w.revision {
            self.tab_titles.clear();
            self.titles_revision = w.revision;
        }
        for tab in tabs {
            if !self.tab_titles.contains_key(&tab.content) {
                let note = match &tab.content {
                    Content::Note(id) => w.note(id),
                    _ => None,
                };
                let kind = match &tab.content {
                    Content::File(id) => w.store.files().iter().find(|file| file.id == *id).map(|file| file.kind),
                    _ => None,
                };
                self.tab_titles.insert(tab.content.clone(), (labels::tab_title(&tab.content, note), kind));
            }
        }
    }

    fn all_tabs_menu(&mut self, tabs: &[NoteTab], active: &str, position: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let mut menu = MenuSpec::default();
        for tab in tabs {
            let title = self.tab_titles.get(&tab.content).map(|(title, _)| title.clone()).unwrap_or_default();
            menu.choice(title, tab.id == active, tab.id.clone());
        }
        self.popup(menu, position, window, cx, |this, id, _, cx| this.nav(cx, |w| w.model.select_tab(&id)));
    }

    pub(crate) fn render_strip(&mut self, width: f32, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let now = Instant::now();
        let c = t();
        let (tabs, active_id) = {
            let w = self.workspace.read(cx);
            (w.model.tabs().to_vec(), w.model.active_id().to_string())
        };
        self.refresh_titles(&tabs, cx);

        let pinned_flags: Vec<bool> = tabs.iter().map(|tab| tab.is_pinned).collect();
        let pinned = pinned_flags.iter().filter(|p| **p).count();
        let layout = layout::strip_layout(width, pinned, tabs.len() - pinned);
        let slots = layout::tab_slots(&pinned_flags, layout.tab_width);
        let content = layout::content_width(&slots);
        let viewport = width - if layout.overflows { layout::BUTTON_WIDTH } else { 0. };
        let active = tabs.iter().position(|tab| tab.id == active_id).unwrap_or(0);

        let signature: Vec<(String, bool)> = tabs.iter().map(|tab| (tab.id.clone(), tab.is_pinned)).collect();
        let duration = Duration::from_millis(if cx.has_active_drag() { 160 } else { 180 });
        let (drawn, mut moving) = self.motion.update(&signature, &slots, duration, now);

        // The active tab scrolls into view when it changes.
        if !layout.overflows {
            self.scroll.set_offset(point(px(0.), px(0.)));
            self.scroll_tween = None;
        } else if self.last_active.as_deref() != Some(active_id.as_str()) {
            let offset = -f32::from(self.scroll.offset().x);
            let target = layout::scroll_to_show(slots[active], offset, viewport, content);
            let mut tween = layout::Tween::settled(offset);
            tween.go(target, Duration::from_millis(200), now);
            self.scroll_tween = Some(tween);
        }
        self.last_active = Some(active_id.clone());
        if let Some(tween) = self.scroll_tween {
            self.scroll.set_offset(point(px(-tween.value(now)), px(0.)));
            if tween.is_moving(now) {
                moving = true;
            } else {
                self.scroll_tween = None;
            }
        }
        if moving {
            window.request_animation_frame();
        }

        let hovered = self.hovered_tab.as_ref().and_then(|id| tabs.iter().position(|tab| tab.id == *id));
        let count = tabs.len();
        let mut children: Vec<AnyElement> = Vec::with_capacity(count);
        for (index, tab) in tabs.iter().enumerate() {
            children.push(self.paper_tab_element(tab, drawn[index], index == active, hovered == Some(index), layout::shows_separator(index, count, active, hovered), cx));
        }

        let inner = div().relative().flex_none().w(px(content)).h(px(layout::BAND_HEIGHT)).children(children);
        let scroller = div().id("tab-scroller").size_full();
        let scroller: AnyElement = if layout.overflows {
            scroller.flex().overflow_x_scroll().track_scroll(&self.scroll).child(inner).into_any_element()
        } else {
            scroller.child(inner).into_any_element()
        };

        let fade = layout.overflows.then(|| {
            // The tabs fade out at the trailing edge. GPUI has no masks: this paints
            // the ground over them instead, as see-through as the ground itself.
            let ground = c.ground_fill;
            div()
                .absolute()
                .right_0()
                .top_0()
                .h_full()
                .w(relative(0.06))
                .bg(linear_gradient(90., linear_color_stop(ground.opacity(0.), 0.), linear_color_stop(ground, 1.)))
        });

        let drag_slots = slots.clone();
        let drag_ids: Vec<String> = tabs.iter().map(|tab| tab.id.clone()).collect();
        let strip_active = tabs[active].clone();
        let strip_right_click = {
            let tab = strip_active.clone();
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_tab_menu(tab.clone(), event.position, window, cx);
            })
        };
        let strip_control_click = {
            let tab = strip_active;
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if event.modifiers.control {
                    cx.stop_propagation();
                    window.prevent_default();
                    this.open_tab_menu(tab.clone(), event.position, window, cx);
                }
            })
        };
        let area = div()
            .id("tab-area")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .on_mouse_down(MouseButton::Right, strip_right_click)
            .on_mouse_down(MouseButton::Left, strip_control_click)
            .on_drag_move(cx.listener(move |this, event: &DragMoveEvent<DraggedTab>, _, cx| {
                let position = event.event.position;
                let dragged = event.drag(cx).id.clone();
                let under = if event.bounds.contains(&position) {
                    let x = f32::from(position.x - event.bounds.left() - this.scroll.offset().x);
                    layout::tab_at(&drag_slots, x).map(|index| drag_ids[index].as_str())
                } else {
                    None
                };
                if let Some(target) = layout::drag_target(under, &dragged, &mut this.drag_last)
                    && let Some(to) = drag_ids.iter().position(|id| id == target)
                {
                    this.nav(cx, |w| w.model.move_tab(&dragged, to));
                }
            }))
            .on_drop(cx.listener(|this, _: &DraggedTab, _, _| this.drag_last = None))
            .child(scroller)
            .children(fade);

        let all_tabs = layout.overflows.then(|| {
            let (menu_tabs, menu_active) = (tabs.clone(), active_id.clone());
            div()
                .id("all-tabs")
                .flex_none()
                .mb(px((layout::BAND_HEIGHT - layout::BUTTON_WIDTH) / 2.))
                .w(px(layout::BUTTON_WIDTH))
                .h(px(layout::BUTTON_WIDTH))
                .flex()
                .items_center()
                .justify_center()
                .tooltip(tip("All Tabs"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let band = this.band.get();
                        let lead = layout::leading_x(f32::from(band.left()) + layout::PAGE_INSET);
                        let position = point(px(lead + width - layout::BUTTON_WIDTH), band.top() + px(layout::BAND_HEIGHT - 12.));
                        this.all_tabs_menu(&menu_tabs, &menu_active, position, window, cx);
                    }),
                )
                .child(symbol("chevron.down", 11., 0.3, c.text2))
        });

        div()
            .w(px(width))
            .h(px(layout::BAND_HEIGHT))
            .flex()
            .items_end()
            .child(area)
            .children(all_tabs)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn paper_tab_element(
        &mut self,
        tab: &NoteTab,
        drawn: layout::Drawn,
        is_active: bool,
        is_hovered: bool,
        separator: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = t();
        let (title, kind) = self.tab_titles.get(&tab.content).cloned().unwrap_or_default();
        let is_draft = tab.content == Content::Draft;
        let shows_close = !tab.is_pinned && (is_active || is_hovered);
        let width = drawn.width;

        let label: AnyElement = if tab.is_pinned {
            let icon_color = if is_active { c.text2 } else { c.icon };
            div()
                .w_full()
                .h(px(LABEL_HEIGHT))
                .flex()
                .items_center()
                .gap(px(3.))
                .pl(px(9.))
                .child(glyph(labels::tab_glyph(&tab.content, kind), 15., icon_color))
                .child(symbol("pin.fill", 8., 0.3, c.icon))
                .into_any_element()
        } else {
            let ink = if is_active {
                if c.glass { c.text } else { c.text2 }
            } else {
                c.text3
            };
            div()
                .w_full()
                .h(px(LABEL_HEIGHT))
                .flex()
                .items_center()
                .pl(px(14.))
                .pr(px(28.))
                .child(
                    div()
                        .min_w_0()
                        .font(self.title_font.clone())
                        .text_size(px(text_size::BODY))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(ink)
                        .when(is_draft, |el| el.italic())
                        .truncate()
                        .child(title.clone()),
                )
                .into_any_element()
        };

        let background: Option<AnyElement> = if is_active {
            let fill = c.page_sheet;
            let edge = c.page_edge;
            let glass = c.glass;
            let shape_width = width + FLARE * 2.;
            let lift = glass.then(|| {
                // A translucent tab on translucent glass needs a little lift. GPUI
                // shadows only follow rounded rectangles: this one is the tab's body.
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(width))
                    .h(px(TAB_HEIGHT))
                    .rounded_t(px(TAB_RADIUS))
                    .shadow(vec![BoxShadow {
                        color: hsla(0., 0., 0., 0.07),
                        offset: point(px(0.), px(-1.)),
                        blur_radius: px(5.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
            });
            let shape = canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    if let Some(path) = paper_tab(bounds.origin, shape_width, TAB_HEIGHT, PathBuilder::fill(), true) {
                        window.paint_path(path, fill);
                    }
                    if glass {
                        // The page's own hairline, carried up around the tab.
                        if let Some(path) = paper_tab(bounds.origin, shape_width, TAB_HEIGHT, PathBuilder::stroke(px(1.)), false) {
                            window.paint_path(path, edge);
                        }
                    } else {
                        // Over the page's top hairline, so the tab runs into the page.
                        let neck = Bounds::new(
                            point(bounds.origin.x + px(FLARE), bounds.origin.y + px(TAB_HEIGHT)),
                            size(px(width), px(1.)),
                        );
                        window.paint_quad(gpui::fill(neck, fill));
                    }
                },
            )
            .absolute()
            .left(px(-FLARE))
            .top_0()
            .w(px(shape_width))
            .h(px(TAB_HEIGHT));
            Some(div().absolute().left_0().top_0().children(lift).child(shape).into_any_element())
        } else if is_hovered {
            Some(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(width))
                    .h(px(TAB_HEIGHT))
                    .rounded_t(px(TAB_RADIUS))
                    .bg(c.text.opacity(0.04))
                    .into_any_element(),
            )
        } else {
            None
        };

        let separator = separator.then(|| {
            div()
                .absolute()
                .right(px(-0.5))
                .top(px((LABEL_HEIGHT - 14.) / 2.))
                .w(px(1.))
                .h(px(14.))
                .bg(c.text.opacity(0.16))
        });

        let close = shows_close.then(|| {
            let hovering = self.hovered_close.as_deref() == Some(tab.id.as_str());
            let ink = if hovering { c.text } else { c.icon };
            let (hover_id, close_id) = (tab.id.clone(), tab.id.clone());
            div()
                .id(SharedString::from(format!("close-{}", tab.id)))
                .absolute()
                .right(px(7.))
                .top(px((LABEL_HEIGHT - 16.) / 2.))
                .w(px(16.))
                .h(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .when(hovering, |el| el.bg(c.text.opacity(0.10)))
                .tooltip(tip("Close Tab (⌘W)"))
                .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                    if *hovering {
                        this.hovered_close = Some(hover_id.clone());
                    } else if this.hovered_close.as_deref() == Some(hover_id.as_str()) {
                        this.hovered_close = None;
                    }
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.hovered_close = None;
                    this.nav(cx, |w| w.model.close_tab(&close_id));
                }))
                .child(symbol("xmark", 8.5, 0.4, ink))
        });

        let select_id = tab.id.clone();
        let hover_id = tab.id.clone();
        let menu_tab = tab.clone();
        let control_tab = tab.clone();
        let help_workspace = self.workspace.clone();
        let help_content = tab.content.clone();
        let ghost_title: SharedString = title.into();
        let ghost_width = width;
        let ghost_font = self.title_font.clone();
        div()
            .id(SharedString::from(tab.id.clone()))
            .absolute()
            .left(px(drawn.x))
            .top(px(layout::BAND_HEIGHT - TAB_HEIGHT))
            .w(px(width))
            .h(px(TAB_HEIGHT))
            .opacity(drawn.opacity)
            .children(background)
            .child(label)
            .children(separator)
            .children(close)
            .tooltip(tip_with(move |cx| tab_help_text(&help_workspace, &help_content, cx)))
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                if *hovering {
                    this.hovered_tab = Some(hover_id.clone());
                } else if this.hovered_tab.as_deref() == Some(hover_id.as_str()) {
                    this.hovered_tab = None;
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if event.modifiers.control {
                        this.open_tab_menu(control_tab.clone(), event.position, window, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_tab_menu(menu_tab.clone(), event.position, window, cx);
                }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if !event.modifiers().control {
                    this.nav(cx, |w| w.model.select_tab(&select_id));
                }
            }))
            .on_drag(DraggedTab { id: tab.id.clone() }, move |_, _, _, cx| {
                let title = ghost_title.clone();
                let font = ghost_font.clone();
                cx.new(|_| TabGhost { font, title, italic: is_draft, width: ghost_width })
            })
            .into_any_element()
    }
}
