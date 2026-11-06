//! The Memos page (`MemosView.swift`): write on a line at the top, the memos
//! below, newest first, grouped by day; in the right margin a month calendar and
//! the tags. One `list` scrolls it all, the page's top as its first item.

use crate::logic::{self, DayGroup, Filter};
use crate::rich::{Link, RichHost, TextSelection};
use crate::typo::{self, MemoType, line_box};
use crate::{field, inline::BlockDoc};
use fumi_core::memo_format::Memo;
use fumi_ui::icons;
use fumi_ui::native::weight;
use fumi_ui::scroller::{ScrollTarget, Scroller};
use fumi_ui::settings;
use fumi_ui::text_field::{FieldEvent, TextField};
use fumi_ui::theme::{AppTheme, t};
use fumi_workspace::{Workspace, now};
use gpui::prelude::*;
use gpui::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

actions!(memos, [CopySelection, SaveComposer, CancelEditing]);

pub fn bind_keys(cx: &mut App) {
    let c = Some("MemosPage");
    cx.bind_keys([
        KeyBinding::new("cmd-c", CopySelection, c),
        KeyBinding::new("cmd-enter", SaveComposer, c),
        KeyBinding::new("escape", CancelEditing, c),
    ]);
}

pub(crate) fn ease_out(t: f32) -> f32 {
    1. - (1. - t) * (1. - t)
}

/// A wash that eases to a new strength over 0.12 s.
#[derive(Clone, Copy, Default)]
pub(crate) struct Wash {
    from: f32,
    to: f32,
    generation: u64,
}

impl Wash {
    pub fn set(&mut self, to: f32) {
        if to != self.to {
            self.from = self.to;
            self.to = to;
            self.generation += 1;
        }
    }

    /// Fills its parent, which must be `relative`.
    pub fn layer(self, name: &'static str, color: Hsla, radius: f32) -> AnyElement {
        let layer = div().absolute().inset_0().rounded(px(radius));
        if self.from == self.to || self.generation == 0 {
            return layer.bg(color.opacity(self.to)).into_any_element();
        }
        let (from, to) = (self.from, self.to);
        layer
            .with_animation(
                (name, self.generation),
                Animation::new(Duration::from_millis(120)).with_easing(ease_out),
                move |el, delta| el.bg(color.opacity(from + (to - from) * delta)),
            )
            .into_any_element()
    }
}

/// A hover and press pair driving a wash.
#[derive(Clone, Copy, Default)]
pub(crate) struct Press {
    pub hovered: bool,
    pub pressed: bool,
    pub wash: Wash,
}

impl Press {
    pub fn update(&mut self, hover: f32, press: f32, enabled: bool) {
        self.wash.set(if !enabled {
            0.
        } else if self.pressed {
            press
        } else if self.hovered {
            hover
        } else {
            0.
        });
    }
}

pub(crate) use fumi_ui::tip::{TIP_DELAY, tip};

/// A memo being edited in place.
pub(crate) struct Edit {
    pub memo: Memo,
    pub field: Entity<TextField>,
    _events: Subscription,
}

/// What the docs were built for: a change restyles them.
#[derive(Clone, PartialEq)]
struct StyleKey {
    theme: AppTheme,
    dark: bool,
    accent: Hsla,
    strong: Hsla,
    family: String,
}

/// Fading the calendar's hover card.
#[derive(Clone, Default)]
pub(crate) struct CardFade {
    /// `Some` while the shown card eases in.
    pub fade_in: Option<u64>,
    /// A day whose card eases away.
    pub fading: Option<(String, u64)>,
    pub generation: u64,
}

pub struct MemosView {
    pub(crate) workspace: Entity<Workspace>,
    pub(crate) focus: FocusHandle,
    pub(crate) composer: Entity<TextField>,
    pub(crate) list: ListState,
    seen_revision: Option<u64>,
    pub(crate) memos: Rc<Vec<Memo>>,
    pub(crate) counts: Rc<HashMap<String, usize>>,
    pub(crate) tags: Rc<Vec<(String, usize)>>,
    pub(crate) groups: Rc<Vec<DayGroup>>,
    pub(crate) filter: Filter,
    pub(crate) month: (i32, u32),
    pub(crate) today: String,
    page_width: f32,
    pub(crate) hovered_row: Option<String>,
    pub(crate) edits: Vec<Edit>,
    pub(crate) host: RichHost,
    pub(crate) docs: HashMap<String, (String, Rc<Vec<BlockDoc>>)>,
    style_key: Option<StyleKey>,
    pub(crate) ty: MemoType,
    pub(crate) card: logic::CardHover,
    pub(crate) card_timer: u64,
    pub(crate) card_fade: CardFade,
    pub(crate) tag_button_hover: bool,
    pub(crate) save_button: Press,
    pub(crate) filter_buttons: [Press; 2],
    pub(crate) arrow_hover: [bool; 2],
    pub(crate) hovered_tag: Option<String>,
    pub(crate) menu_anchor: Rc<Cell<Bounds<Pixels>>>,
    scroller: Scroller,
    _subscriptions: Vec<Subscription>,
}

impl MemosView {
    pub fn new(workspace: Entity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let composer = field::memo_field("Write it down…", 26., None, window, cx);
        let today = logic_today();
        let month = logic::civil(&today).map_or((2026, 1), |date| (date.year, date.month));
        let weak = cx.entity().downgrade();
        let host = RichHost {
            selection: Rc::new(RefCell::new(TextSelection::default())),
            on_change: Rc::new({
                let (weak, focus) = (weak.clone(), focus.clone());
                move |window, cx| {
                    window.focus(&focus, cx);
                    let _ = weak.update(cx, |_, cx| cx.notify());
                }
            }),
            on_link: Rc::new({
                let weak = weak.clone();
                move |link, _, cx| match link {
                    Link::Tag(tag) => {
                        let tag = tag.clone();
                        let _ = weak.update(cx, move |this, cx| {
                            this.filter.tag = Some(tag);
                            this.apply_filter(cx);
                        });
                    }
                    Link::Url(url) => cx.open_url(url),
                }
            }),
        };
        let list = ListState::new(2, ListAlignment::Top, px(400.)).with_uniform_item_height(px(120.));
        list.set_scroll_handler({
            let weak = weak.clone();
            move |_, _, cx| {
                let _ = weak.update(cx, |this, cx| this.note_scroll(cx));
            }
        });
        let subscriptions = vec![
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.subscribe(&composer, |this, _, event: &FieldEvent, cx| match event {
                FieldEvent::Changed => cx.notify(),
                FieldEvent::Submit => this.submit_composer(cx),
                _ => {}
            }),
        ];
        // The composer takes focus when the page appears.
        window.focus(&composer.read(cx).focus.clone(), cx);
        let mut this = MemosView {
            workspace,
            focus,
            composer,
            list: list.clone(),
            seen_revision: None,
            memos: Rc::new(Vec::new()),
            counts: Rc::new(HashMap::new()),
            tags: Rc::new(Vec::new()),
            groups: Rc::new(Vec::new()),
            filter: Filter::default(),
            month,
            today,
            page_width: 1000.,
            hovered_row: None,
            edits: Vec::new(),
            host,
            docs: HashMap::new(),
            style_key: None,
            ty: MemoType::from_settings(),
            card: logic::CardHover::default(),
            card_timer: 0,
            card_fade: CardFade::default(),
            tag_button_hover: false,
            save_button: Press::default(),
            filter_buttons: [Press::default(); 2],
            arrow_hover: [false; 2],
            hovered_tag: None,
            menu_anchor: Rc::new(Cell::new(Bounds::default())),
            scroller: Scroller::new(ScrollTarget::List(list.clone())),
            _subscriptions: subscriptions,
        };
        this.sync(cx);
        this
    }

    /// Re-reads the memos when the library changed; all derived data keys on this.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let revision = self.workspace.read(cx).revision;
        if self.seen_revision != Some(revision) {
            self.seen_revision = Some(revision);
            let memos = self.workspace.read(cx).store.memos();
            self.counts = Rc::new(logic::counts_by_day(&memos));
            self.tags = Rc::new(logic::top_tags(&memos));
            self.memos = Rc::new(memos);
            let live: std::collections::HashSet<String> = self.memos.iter().map(Memo::id).collect();
            self.docs.retain(|id, _| live.contains(id));
            self.host.selection.borrow_mut().clear();
            self.apply_groups();
        }
        self.today = logic_today();
        let c = t();
        let key = StyleKey {
            theme: c.theme,
            dark: c.dark,
            accent: c.accent,
            strong: c.strong,
            family: settings::editor_font(),
        };
        if self.style_key.as_ref() != Some(&key) {
            self.ty = MemoType::new(&key.family);
            let font_changed = self.style_key.as_ref().is_some_and(|old| old.family != key.family);
            self.style_key = Some(key);
            self.docs.clear();
            self.host.selection.borrow_mut().clear();
            if font_changed {
                self.list.remeasure();
            }
        }
    }

    /// Takes the list to the memos now shown, touching only the days that changed
    /// and staying where the page is scrolled.
    fn apply_groups(&mut self) {
        let new: Vec<DayGroup> = logic::grouped_by_day(self.memos.iter().filter(|m| self.filter.admits(m)).cloned());
        let top = self.list.logical_scroll_top();
        if self.list.item_count() != self.groups.len() + 2 || self.groups.is_empty() {
            // A first guess at every height, so the scroller is about right at once.
            self.list.reset_with_uniform_height(new.len() + 2, px(120.));
        } else if let Some((range, count)) = logic::splice_between(&self.groups, &new) {
            // Items: the page's top, the days, the bottom padding.
            let changed = range.start + 1..range.end + 1;
            let same_count = changed.len() == count;
            let in_changed = changed.contains(&top.item_ix);
            self.list.splice(changed, count);
            if same_count && in_changed {
                self.list.scroll_to(top);
            }
        }
        self.groups = Rc::new(new);
    }

    pub(crate) fn apply_filter(&mut self, cx: &mut Context<Self>) {
        self.apply_groups();
        cx.notify();
    }

    /// The scroller notices the move when it next draws.
    fn note_scroll(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub(crate) fn submit_composer(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        if logic::is_blank(&text) {
            return;
        }
        self.workspace.update(cx, |w, cx| {
            if let Err(error) = w.store.add_memo(&text, now()) {
                eprintln!("couldn't save the memo: {error}");
            }
            w.library_changed(cx);
        });
        self.composer.update(cx, |field, cx| field.set_text("", cx));
        cx.notify();
    }

    pub(crate) fn add_tag_marker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = logic::with_tag_marker(self.composer.read(cx).text());
        let focus = self.composer.update(cx, |field, cx| {
            field.set_text(draft, cx);
            cx.emit(FieldEvent::Changed);
            field.focus.clone()
        });
        window.focus(&focus, cx);
    }

    pub(crate) fn start_edit(&mut self, memo: Memo, window: &mut Window, cx: &mut Context<Self>) {
        if self.edits.iter().any(|edit| edit.memo.id() == memo.id()) {
            return;
        }
        let field = field::memo_field("", 30., None, window, cx);
        let focus = field.update(cx, |f, cx| {
            f.set_text(memo.text.clone(), cx);
            f.focus.clone()
        });
        let id = memo.id();
        let events = cx.subscribe(&field, move |this, _, event: &FieldEvent, cx| match event {
            FieldEvent::Changed => cx.notify(),
            FieldEvent::Submit => this.save_edit(&id, cx),
            FieldEvent::Cancel => this.cancel_edit(&id, cx),
            _ => {}
        });
        self.edits.push(Edit { memo, field, _events: events });
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(crate) fn save_edit(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(at) = self.edits.iter().position(|edit| edit.memo.id() == id) else { return };
        let edit = self.edits.remove(at);
        let text = edit.field.read(cx).text().to_string();
        self.workspace.update(cx, |w, cx| {
            if let Err(error) = w.store.update_memo(&edit.memo, &text, now()) {
                eprintln!("couldn't save the memo: {error}");
            }
            w.library_changed(cx);
        });
        cx.notify();
    }

    pub(crate) fn cancel_edit(&mut self, id: &str, cx: &mut Context<Self>) {
        self.edits.retain(|edit| edit.memo.id() != id);
        cx.notify();
    }

    pub(crate) fn toggle_task(&mut self, memo: &Memo, line: usize, cx: &mut Context<Self>) {
        let text = fumi_core::memo_markdown::toggling_task(line, &memo.text);
        self.workspace.update(cx, |w, cx| {
            if let Err(error) = w.store.update_memo(memo, &text, now()) {
                eprintln!("couldn't save the memo: {error}");
            }
            w.library_changed(cx);
        });
    }

    pub(crate) fn delete_memo(&mut self, memo: &Memo, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if let Err(error) = w.store.delete_memo(memo, now()) {
                eprintln!("couldn't delete the memo: {error}");
            }
            w.library_changed(cx);
        });
    }

    pub(crate) fn this_year(&self) -> i32 {
        logic::civil(&self.today).map_or(0, |date| date.year)
    }

    pub(crate) fn this_month(&self) -> u32 {
        logic::civil(&self.today).map_or(0, |date| date.month)
    }

    fn render_item(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let body = if ix == 0 {
            self.render_top(cx).into_any_element()
        } else if ix > self.groups.len() {
            div().h(px(48.)).into_any_element()
        } else {
            self.render_group(ix - 1, window, cx)
        };
        div()
            .w_full()
            .child(div().ml(px(logic::column_x(self.page_width))).w(px(logic::COLUMN_WIDTH)).child(body))
            .into_any_element()
    }

    /// Above the memos: the writing line, the calendar when there's no margin,
    /// the filters and the first-run line.
    fn render_top(&mut self, cx: &mut Context<Self>) -> Div {
        let c = t();
        let mut top = div().pt(px(34.)).flex().flex_col().child(self.render_composer(cx));
        if !logic::shows_margin(self.page_width) {
            top = top.child(
                div()
                    .pt(px(22.))
                    .flex()
                    .justify_end()
                    .child(div().w(px(logic::MARGIN_WIDTH)).child(self.render_calendar(cx))),
            );
        }
        if self.filter.is_active() {
            top = top.child(self.render_filter_line(cx));
        }
        if self.memos.is_empty() {
            top = top.child(
                div()
                    .pt(px(28.))
                    .font(typo::ui(FontWeight::NORMAL))
                    .text_size(px(12.))
                    .line_height(line_box(12.))
                    .text_color(c.faint_text)
                    .child("Memos are saved in the Memos folder, one file per day."),
            );
        }
        top
    }

    fn render_composer(&mut self, cx: &mut Context<Self>) -> Div {
        let c = t();
        let draft_is_blank = logic::is_blank(self.composer.read(cx).text());
        self.save_button.update(0.10, 0.16, !draft_is_blank);
        let tag_color = if self.tag_button_hover { c.text2 } else { c.faint_text };
        let tag_button = div()
            .id("memo-add-tag")
            .font(typo::ui(FontWeight::NORMAL))
            .text_size(px(12.))
            .line_height(line_box(12.))
            .text_color(tag_color)
            .cursor_pointer()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.tag_button_hover = *hovered;
                cx.notify();
            }))
            .on_click(cx.listener(|this, _, window, cx| this.add_tag_marker(window, cx)))
            .tooltip(tip("Add a tag"))
            .tooltip_show_delay(TIP_DELAY)
            .child("# Tag");
        let save_button = div()
            .id("memo-save")
            .relative()
            .h(px(24.))
            .px(px(10.))
            // The word lines up with the hairline's end; the wash reaches past it.
            .mr(px(-10.))
            .flex()
            .items_center()
            .font(typo::ui(FontWeight::NORMAL))
            .text_size(px(12.))
            .line_height(line_box(12.))
            .text_color(if draft_is_blank { c.faint_text } else { c.accent })
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.save_button.hovered = *hovered;
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.save_button.pressed = true;
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.save_button.pressed = false;
                cx.notify();
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.save_button.pressed = false;
                cx.notify();
            }))
            .on_click(cx.listener(|this, _, _, cx| this.submit_composer(cx)))
            .tooltip(tip("Save (⌘↩)"))
            .tooltip_show_delay(TIP_DELAY)
            .child(self.save_button.wash.layer("memo-save-wash", c.accent, 6.))
            .child(div().relative().child("Save"));
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .pb(px(12.))
                    .child(self.composer.clone())
                    .child(div().absolute().bottom_0().left_0().right_0().h(px(1.)).bg(c.text.opacity(0.09))),
            )
            .child(
                div()
                    .pt(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(tag_button)
                    .child(div().flex_1())
                    .child(save_button),
            )
    }

    /// What the memos are narrowed to, each with an × that clears it.
    fn render_filter_line(&mut self, cx: &mut Context<Self>) -> Div {
        let mut line = div().pt(px(18.)).flex().gap(px(16.));
        if let Some(day) = self.filter.day.clone() {
            let label = logic::long_day_label(&day, self.this_year());
            line = line.child(self.filter_label(0, label, cx));
        }
        if let Some(tag) = self.filter.tag.clone() {
            line = line.child(self.filter_label(1, format!("#{tag}"), cx));
        }
        line
    }

    fn filter_label(&mut self, slot: usize, label: String, cx: &mut Context<Self>) -> Stateful<Div> {
        let c = t();
        self.filter_buttons[slot].update(0.05, 0.08, true);
        let hovered = self.filter_buttons[slot].hovered;
        let names = ["memo-filter-day-wash", "memo-filter-tag-wash"];
        div()
            .id(("memo-filter", slot))
            .relative()
            .h(px(24.))
            .px(px(8.))
            // The label stays in line with the memos; the wash reaches past it.
            .mx(px(-8.))
            .flex()
            .items_center()
            .cursor_pointer()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.filter_buttons[slot].hovered = *hovered;
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                this.filter_buttons[slot].pressed = true;
                cx.notify();
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                this.filter_buttons[slot].pressed = false;
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.filter_buttons[slot] = Press::default();
                if slot == 0 {
                    this.filter.day = None;
                } else {
                    this.filter.tag = None;
                }
                this.apply_filter(cx);
            }))
            .tooltip(tip("Show all memos"))
            .tooltip_show_delay(TIP_DELAY)
            .child(self.filter_buttons[slot].wash.layer(names[slot], c.text, 6.))
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .font(typo::ui(FontWeight::NORMAL))
                            .text_size(px(12.))
                            .line_height(line_box(12.))
                            .whitespace_nowrap()
                            .text_color(if hovered { c.text } else { c.text2 })
                            .child(label),
                    )
                    .child(icons::symbol("xmark", 8.5, weight::SEMIBOLD, if hovered { c.text2 } else { c.faint_text })),
            )
    }

    /// The margin: calendar, then tags. Stays put while the memos scroll by.
    fn render_margin(&mut self, cx: &mut Context<Self>) -> Div {
        let c = t();
        let mut margin = div()
            .absolute()
            .top(px(36.))
            .left(px(logic::margin_x(self.page_width)))
            .w(px(logic::MARGIN_WIDTH))
            .flex()
            .flex_col()
            .child(self.render_calendar(cx));
        let tags = self.tags.clone();
        if !tags.is_empty() {
            margin = margin
                .child(div().mt(px(18.)).mb(px(14.)).h(px(1.)).bg(c.text.opacity(0.09)))
                .child(
                    div()
                        .pb(px(8.))
                        .font(typo::serif(FontWeight::NORMAL, true))
                        .text_size(px(13.))
                        .line_height(line_box(13.))
                        .text_color(c.faint_text)
                        .child("Tags"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(7.))
                        .children(tags.iter().enumerate().map(|(i, (name, count))| self.tag_line(i, name, *count, cx))),
                );
        }
        margin
    }

    fn tag_line(&self, index: usize, name: &str, count: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let c = t();
        let selected = self.filter.tag.as_deref() == Some(name);
        let hovered = self.hovered_tag.as_deref() == Some(name);
        let name_owned = name.to_string();
        let mut label = div()
            .min_w_0()
            .truncate()
            .font(typo::ui(if selected { FontWeight::SEMIBOLD } else { FontWeight::NORMAL }))
            .text_size(px(12.))
            .line_height(line_box(12.))
            .text_color(c.accent.opacity(if hovered && !selected { 0.75 } else { 1. }))
            .child(format!("#{name}"));
        if selected {
            label = label.underline().text_decoration_color(c.accent);
        }
        let tooltip = if selected { "Show all memos".to_string() } else { format!("Show memos tagged #{name}") };
        div()
            .id(("memo-tag", index))
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .on_hover(cx.listener({
                let name = name_owned.clone();
                move |this, inside: &bool, _, cx| {
                    if *inside {
                        this.hovered_tag = Some(name.clone());
                    } else if this.hovered_tag.as_deref() == Some(name.as_str()) {
                        this.hovered_tag = None;
                    }
                    cx.notify();
                }
            }))
            .on_click(cx.listener({
                let name = name_owned;
                move |this, _, _, cx| {
                    this.filter.toggle_tag(&name);
                    this.apply_filter(cx);
                }
            }))
            .tooltip(tip(tooltip))
            .tooltip_show_delay(TIP_DELAY)
            .child(label)
            .child(div().flex_1())
            .child(
                div()
                    .flex_none()
                    .font(typo::tabular(typo::ui(FontWeight::NORMAL)))
                    .text_size(px(12.))
                    .line_height(line_box(12.))
                    .text_color(c.faint_icon)
                    .child(count.to_string()),
            )
    }

    /// The page's top edge, `PageEdgeBlur`'s wash without the blur GPUI can't do:
    /// one straight wash from 80% to clear, within 7% of its two-part gradient,
    /// since GPUI caps a corner at half the element's height and the page's
    /// rounded corners need a band taller than twice their radius. Nothing in
    /// Glass, where the page itself is clear.
    fn render_top_fade(&self) -> Option<Div> {
        let c = t();
        if c.glass {
            return None;
        }
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(12. + 10.5))
                .rounded_t(px(fumi_ui::theme::PAGE_INNER_RADIUS))
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(c.page_content.opacity(0.8), 0.),
                    linear_color_stop(c.page_content.opacity(0.), 1.),
                )),
        )
    }
}

fn logic_today() -> String {
    let at = now();
    fumi_core::memo_format::day_of(at.secs, at.utc_offset)
}

impl Focusable for MemosView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for MemosView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync(cx);
        let c = t();
        let weak = cx.entity().downgrade();
        let page_width = self.page_width;
        let measure = canvas(
            move |bounds, _, cx| {
                let width = f32::from(bounds.size.width);
                if width != page_width {
                    let weak = weak.clone();
                    cx.defer(move |cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.page_width = width;
                            cx.notify();
                        });
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let margin = logic::shows_margin(self.page_width).then(|| self.render_margin(cx));
        let thumb = self.scroller.thumb(|this: &mut Self| &mut this.scroller, Instant::now(), window, cx);
        div()
            .id("memos-page")
            .key_context("MemosPage")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .rounded(px(fumi_ui::theme::PAGE_INNER_RADIUS))
            .bg(c.page_content)
            .on_action(cx.listener(|this, _: &CopySelection, _, cx| {
                if let Some(text) = this.host.selection.borrow().text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }))
            .on_action(cx.listener(|this, _: &SaveComposer, _, cx| this.submit_composer(cx)))
            .on_action(cx.listener(|this, _: &CancelEditing, _, cx| {
                if let Some(edit) = this.edits.pop() {
                    drop(edit);
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                if this.host.selection.borrow_mut().clear() {
                    cx.notify();
                }
            }))
            .child(measure)
            .child(
                list(self.list.clone(), cx.processor(|this, ix: usize, window, cx| this.render_item(ix, window, cx)))
                    .size_full(),
            )
            .children(self.render_top_fade())
            .children(margin)
            .children(thumb)
    }
}
