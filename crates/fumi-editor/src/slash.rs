//! The slash menu: its state, the keys and the pointer, what picking an item does
//! to the text, and the panel. In Swift a borderless NSPanel; here a panel drawn
//! over the page, which likewise never takes focus: the editor keeps the caret
//! and the keys while it shows. The items and text rules are `fumi_core`'s.

use crate::display::{family_font, mono_font, ui_font};
use crate::editor::Editor;
use crate::textlayout::OBJECT;
use fumi_core::slash_command::{self, Action as SlashAction, Group, MenuResult};
use fumi_ui::icons;
use fumi_ui::native;
use fumi_ui::theme::t;
use gpui::prelude::*;
use gpui::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

pub const ROW_HEIGHT: f32 = 30.;
pub const LABEL_HEIGHT: f32 = 24.;
pub const FOOTER_HEIGHT: f32 = 29.;
pub const PADDING: f32 = 5.;
pub const WIDTH: f32 = 248.;
/// Nine rows, then the list scrolls.
pub const MAX_LIST_HEIGHT: f32 = 9. * ROW_HEIGHT + LABEL_HEIGHT;
const GAP: f32 = 6.;
const PAGE_INSET: f32 = 8.;

actions!(editor, [SlashUp, SlashDown, SlashAccept, SlashClose]);

/// What an item's tile shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    Letters(&'static str),
    SerifLetters(&'static str),
    Symbol(&'static str),
}

pub fn tile(id: &str) -> Tile {
    match id {
        "text" => Tile::SerifLetters("Aa"),
        "h1" => Tile::Letters("H1"),
        "h2" => Tile::Letters("H2"),
        "h3" => Tile::Letters("H3"),
        "bullets" => Tile::Symbol("list.bullet"),
        "numbers" => Tile::Letters("1."),
        "todo" => Tile::Symbol("checkmark.square"),
        "quote" => Tile::Symbol("text.quote"),
        "callout" => Tile::Symbol("info.square"),
        "code" => Tile::Symbol("chevron.left.forwardslash.chevron.right"),
        "math" => Tile::SerifLetters("∑"),
        "table" => Tile::Symbol("tablecells"),
        "divider" => Tile::Symbol("minus"),
        "image" => Tile::Symbol("photo"),
        "link" => Tile::Symbol("link"),
        "bold" => Tile::Symbol("bold"),
        "italic" => Tile::Symbol("italic"),
        "strike" => Tile::Symbol("strikethrough"),
        "highlight" => Tile::Symbol("highlighter"),
        _ => Tile::Letters("`"),
    }
}

/// The open menu.
pub struct SlashMenu {
    /// The slash, in the note's bytes.
    pub anchor: usize,
    pub query: String,
    pub results: Vec<MenuResult<'static>>,
    pub selected: usize,
    /// Query length when there were last any results, so a run of misses closes
    /// the menu.
    last_matched: usize,
    pub scroll: ScrollHandle,
    last_offset: Pixels,
    scrolled_at: Option<Instant>,
}

impl SlashMenu {
    pub fn open(anchor: usize) -> Self {
        SlashMenu {
            anchor,
            query: String::new(),
            results: slash_command::results(""),
            selected: 0,
            last_matched: 0,
            scroll: ScrollHandle::new(),
            last_offset: px(0.),
            scrolled_at: None,
        }
    }

    /// Group labels only while nothing is typed; a filtered list is one run.
    pub fn shows_groups(&self) -> bool {
        self.query.is_empty()
    }

    /// Filters for the text now after the slash. False: it has run out of reasons
    /// to stay.
    pub fn update(&mut self, query: &str) -> bool {
        let results = slash_command::results(query);
        let length: usize = query.chars().map(char::len_utf16).sum();
        if !results.is_empty() {
            self.last_matched = length;
        } else if length.saturating_sub(self.last_matched) >= 3 {
            return false;
        }
        let keep = self.results.get(self.selected).map(|r| r.command.id);
        let changed = self.query != query;
        self.query = query.to_string();
        self.results = results;
        let selected = if query.is_empty() {
            self.results.iter().position(|r| Some(r.command.id) == keep).unwrap_or(0)
        } else {
            0
        };
        self.select(selected);
        if changed && !query.is_empty() {
            self.scroll.set_offset(point(px(0.), px(0.)));
        }
        true
    }

    pub fn move_selection(&mut self, step: isize) {
        if self.results.is_empty() {
            return;
        }
        let last = self.results.len() as isize - 1;
        self.select((self.selected as isize + step).clamp(0, last) as usize);
    }

    pub fn select(&mut self, index: usize) {
        self.selected = index;
        if let Some(child) = self.child_index(index) {
            self.scroll.scroll_to_item(child);
        }
    }

    /// Where a row is among the list's children, which group labels are too.
    pub fn child_index(&self, index: usize) -> Option<usize> {
        if index >= self.results.len() {
            return None;
        }
        if !self.shows_groups() {
            return Some(index);
        }
        Some(index + self.groups_through(index))
    }

    fn groups_through(&self, index: usize) -> usize {
        let mut groups = 0;
        let mut last: Option<Group> = None;
        for r in &self.results[..=index] {
            if last != Some(r.command.group) {
                groups += 1;
                last = Some(r.command.group);
            }
        }
        groups
    }

    pub fn natural_list_height(&self) -> f32 {
        if self.results.is_empty() {
            return ROW_HEIGHT;
        }
        let groups = if self.shows_groups() { self.groups_through(self.results.len() - 1) } else { 0 };
        self.results.len() as f32 * ROW_HEIGHT + groups as f32 * LABEL_HEIGHT
    }

    pub fn list_height(&self) -> f32 {
        self.natural_list_height().min(MAX_LIST_HEIGHT)
    }

    pub fn height(&self) -> f32 {
        self.list_height() + PADDING * 2. + if self.results.is_empty() { 0. } else { FOOTER_HEIGHT }
    }
}

/// What `raw`, the text between slash and caret, filters by. None: it can't be a
/// query any more. Pinyin being composed may show separators between syllables.
pub fn query_of(raw: &str, composing: bool) -> Option<String> {
    let query: String = if composing {
        raw.chars().filter(|c| !c.is_whitespace() && *c != '\'').collect()
    } else {
        raw.to_string()
    };
    (query.graphemes(true).count() <= 32 && !query.chars().any(char::is_whitespace)).then_some(query)
}

/// A typed text that may open the menu: a slash, or what a Chinese input method
/// types for that key, alone.
pub fn is_lone_trigger(text: &str) -> bool {
    let mut chars = text.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if slash_command::is_trigger(c))
}

/// Where the panel goes: under the slash (or above without room), its tiles lined
/// up with the slash, inside the page's inset.
pub fn place(slash: Bounds<Pixels>, page: Bounds<Pixels>, height: f32) -> Point<Pixels> {
    let inset = px(PAGE_INSET);
    let (width, height) = (px(WIDTH), px(height));
    let x = (slash.left() - px(PADDING + 4.))
        .min(page.right() - inset - width)
        .max(page.left() + inset);
    let below = slash.bottom() + px(GAP);
    let above = slash.top() - px(GAP) - height;
    let y = if below + height > page.bottom() - inset && above >= page.top() + inset { above } else { below };
    point(x, y)
}

/// Matched characters of a title are semibold: `(text, bold)` runs.
pub fn title_runs(title: &str, matched: &[usize]) -> Vec<(String, bool)> {
    let mut runs: Vec<(String, bool)> = Vec::new();
    for (i, g) in title.graphemes(true).enumerate() {
        let bold = matched.contains(&i);
        match runs.last_mut() {
            Some((text, b)) if *b == bold => text.push_str(g),
            _ => runs.push((g.to_string(), bold)),
        }
    }
    runs
}

impl Editor {
    pub(crate) fn close_slash(&mut self) {
        self.slash = None;
    }

    /// After an edit that typed a lone slash: opens the menu, if it may open here.
    pub(crate) fn open_slash_if_typed(&mut self, cx: &mut Context<Self>) {
        let caret = self.head;
        if self.slash.is_some() || self.marked.is_some() || !self.selection_is_empty() || caret == 0 {
            return;
        }
        if !fumi_ui::settings::slash_commands() {
            return;
        }
        let at = self.doc.rope.byte_to_char(caret);
        let slash_char = self.doc.rope.char(at - 1);
        if !slash_command::is_trigger(slash_char) {
            return;
        }
        let slash = caret - slash_char.len_utf8();
        // The cheap test first: only then is the note worth reading.
        let opens_line = at < 2 || matches!(self.doc.rope.char(at - 2), ' ' | '\t' | '\u{3000}' | '\n');
        if opens_line && slash_command::opens_menu(slash, &self.doc.rope.to_string()) {
            self.slash = Some(SlashMenu::open(slash));
            cx.notify();
        }
    }

    /// Filters the open menu for the text between its slash and the caret, or
    /// closes it once the caret has left that text, the slash is gone or a space
    /// was typed.
    pub(crate) fn refresh_slash(&mut self) {
        let Some(slash) = self.slash.as_ref().map(|m| m.anchor) else { return };
        let caret = self.head;
        let intact = self.selection_is_empty()
            && slash < self.doc.len()
            && caret > slash
            // Way past anything 32 characters could be.
            && caret - slash <= 1024
            && self.doc.floor_boundary(slash) == slash
            && slash_command::is_trigger(self.doc.rope.char(self.doc.rope.byte_to_char(slash)));
        if !intact {
            self.close_slash();
            return;
        }
        let width = self.doc.rope.char(self.doc.rope.byte_to_char(slash)).len_utf8();
        let raw = self.doc.slice(slash + width..caret);
        let open = query_of(&raw, self.marked.is_some())
            .is_some_and(|query| self.slash.as_mut().is_some_and(|m| m.update(&query)));
        if !open {
            self.close_slash();
        }
    }

    pub(crate) fn slash_up(&mut self, _: &SlashUp, _: &mut Window, cx: &mut Context<Self>) {
        self.slash_key(-1, cx);
    }

    pub(crate) fn slash_down(&mut self, _: &SlashDown, _: &mut Window, cx: &mut Context<Self>) {
        self.slash_key(1, cx);
    }

    fn slash_key(&mut self, step: isize, cx: &mut Context<Self>) {
        match &mut self.slash {
            Some(menu) if self.marked.is_none() => {
                menu.move_selection(step);
                cx.notify();
            }
            _ => cx.propagate(),
        }
    }

    /// ↵, keypad Enter and ⇥ insert the selected item; with no results they close
    /// the menu and the key goes on to do what it does.
    pub(crate) fn slash_accept(&mut self, _: &SlashAccept, window: &mut Window, cx: &mut Context<Self>) {
        if self.marked.is_some() {
            cx.propagate();
            return;
        }
        let Some(menu) = &self.slash else {
            cx.propagate();
            return;
        };
        if menu.results.is_empty() {
            self.close_slash();
            cx.notify();
            cx.propagate();
            return;
        }
        let selected = menu.selected;
        self.apply_slash(selected, window, cx);
    }

    pub(crate) fn slash_close(&mut self, _: &SlashClose, _: &mut Window, cx: &mut Context<Self>) {
        if self.slash.is_none() || self.marked.is_some() {
            cx.propagate();
            return;
        }
        self.close_slash();
        cx.notify();
    }

    /// Replaces the slash and its query with what the item inserts. Line items
    /// rewrite the whole line; a line holding something drawn in place of its
    /// source keeps it, and only the slash and query are replaced.
    fn apply_slash(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.slash.take() else { return };
        let Some(result) = menu.results.get(index) else { return };
        let command = result.command;
        let slash = menu.anchor;
        let caret = self.head;
        if slash >= self.doc.len() || caret <= slash {
            return;
        }
        let line = self.doc.line_of(slash);
        let start = self.doc.line_start(line);
        let mut span = start..self.doc.line_end(line);
        if self.display_of(line, Some(caret - start)).text.contains(OBJECT) {
            span = slash..caret;
        }
        let query = slash - span.start..caret - span.start;
        let text = self.doc.slice(span.clone());
        let (new_text, selection) = command.edit(&text, query);
        self.edit(span.clone(), &new_text, false, false);
        // The caret first, so a code block or table opens as source around it,
        // then whatever the item selects within that.
        let (a, b) = (span.start + selection.start, span.start + selection.end);
        self.anchor = a;
        self.head = a;
        self.caret_moved(true);
        if b != a {
            self.head = b;
            self.caret_moved(true);
        }
        self.schedule_after_edit(cx);
        cx.notify();
        if command.action == SlashAction::ChooseImage {
            self.choose_media(window, cx);
        }
    }

    /// Picks images or movies to link at the caret, copied into the library as a
    /// drop would be.
    fn choose_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else { return };
        let range = self.selection();
        let (done, chosen) = futures::channel::oneshot::channel();
        native::choose_media_files(native::ns_window_of(window), move |paths| {
            let _ = done.send(paths);
        });
        cx.spawn(async move |this: WeakEntity<Editor>, cx| {
            let Ok(paths) = chosen.await else { return };
            let imported: Vec<_> = paths
                .iter()
                .filter_map(|f| fumi_core::attachment_import::import_file(f, &library).ok())
                .collect();
            let _ = this.update(cx, |e, cx| {
                let end = e.doc.len();
                e.insert_imported(&imported, range.start.min(end)..range.end.min(end), cx);
            });
        })
        .detach();
    }

    /// The panel, placed where the slash is drawn this frame. None while closed
    /// or while the app isn't active: it comes back with the app, as a panel that
    /// hides on deactivation does.
    pub(crate) fn slash_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !window.is_window_active() {
            return None;
        }
        let c = t();
        let menu = self.slash.as_mut()?;
        let height = menu.height();
        let body: AnyElement = if menu.results.is_empty() {
            div()
                .h(px(ROW_HEIGHT))
                .px(px(8.))
                .flex()
                .items_center()
                .text_size(px(13.))
                .text_color(c.text3)
                .child("No results")
                .into_any_element()
        } else {
            let list_height = menu.list_height();
            let natural = menu.natural_list_height();
            let capped = list_height >= MAX_LIST_HEIGHT;
            // The overlay scroller shows while the list moves.
            let offset = menu.scroll.offset().y;
            if offset != menu.last_offset {
                menu.last_offset = offset;
                menu.scrolled_at = Some(Instant::now());
                cx.spawn(async move |this: WeakEntity<Editor>, cx| {
                    cx.background_executor().timer(Duration::from_millis(1100)).await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
            let thumb = (capped && menu.scrolled_at.is_some_and(|at| at.elapsed() < Duration::from_secs(1))).then(|| {
                let h = (list_height * list_height / natural).max(24.);
                let travel = list_height - h;
                let top = (f32::from(-offset) / (natural - list_height)).clamp(0., 1.) * travel;
                div()
                    .absolute()
                    .right(px(0.))
                    .top(px(top))
                    .w(px(4.))
                    .h(px(h))
                    .rounded(px(2.))
                    .bg(c.faint_icon.opacity(0.6))
            });
            let shows_groups = menu.shows_groups();
            let selected = menu.selected;
            let mut rows: Vec<AnyElement> = Vec::with_capacity(menu.results.len() + 4);
            let mut last_group = None;
            for (ix, result) in menu.results.iter().enumerate() {
                let group = result.command.group;
                if shows_groups && last_group != Some(group) {
                    rows.push(
                        div()
                            .flex_none()
                            .h(px(LABEL_HEIGHT))
                            .px(px(8.))
                            .flex()
                            .items_end()
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c.text3)
                            .child(group.title())
                            .into_any_element(),
                    );
                }
                last_group = Some(group);
                rows.push(row(ix, result, ix == selected, cx));
            }
            div()
                .child(
                    div()
                        .relative()
                        .h(px(list_height))
                        .child(
                            div()
                                .id("slash-list")
                                .size_full()
                                .flex()
                                .flex_col()
                                .overflow_y_scroll()
                                .track_scroll(&menu.scroll)
                                .children(rows),
                        )
                        .children(thumb),
                )
                .child(footer())
                .into_any_element()
        };
        let line = if c.dark { hsla(0., 0., 1., 0.12) } else { hsla(0., 0., 0., 0.08) };
        let panel = div()
            .id("slash-menu")
            .absolute()
            .left_0()
            .top_0()
            .w(px(WIDTH))
            .h(px(height))
            // The 1 pt line sits inside the padding, not on top of it.
            .p(px(PADDING - 1.))
            .rounded(px(10.))
            .bg(c.page)
            .border_1()
            .border_color(line)
            .shadow(vec![
                BoxShadow { color: hsla(0., 0., 0., 0.16), offset: point(px(0.), px(8.)), blur_radius: px(24.), spread_radius: px(0.), inset: false },
                BoxShadow { color: hsla(0., 0., 0., 0.10), offset: point(px(0.), px(0.)), blur_radius: px(1.), spread_radius: px(0.5), inset: false },
            ])
            .font(ui_font())
            .cursor_default()
            // Keeps the press from the page: the menu closes on clicks in the text,
            // and the editor stays focused.
            .occlude()
            .child(body);
        Some(
            deferred(Following {
                child: panel.into_any_element(),
                slash: self.slash_box.clone(),
                list: self.list.clone(),
                height,
                placed: false,
            })
            .with_priority(2)
            .into_any_element(),
        )
    }
}

fn row(ix: usize, result: &MenuResult<'static>, selected: bool, cx: &mut Context<Editor>) -> AnyElement {
    let c = t();
    let command = result.command;
    let fill = if selected {
        c.accent.opacity(if c.dark { 0.24 } else { 0.12 })
    } else {
        transparent_black()
    };
    let runs: Vec<TextRun> = title_runs(command.title, &result.title_offsets)
        .iter()
        .map(|(text, bold)| {
            let mut font = ui_font();
            if *bold {
                font.weight = FontWeight::SEMIBOLD;
            }
            TextRun { len: text.len(), font, color: c.text, background_color: None, underline: None, strikethrough: None }
        })
        .collect();
    div()
        .id(("slash-row", ix))
        .flex_none()
        .h(px(ROW_HEIGHT))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(9.))
        .rounded(px(6.))
        .bg(fill)
        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered && let Some(menu) = &mut this.slash {
                menu.select(ix);
                cx.notify();
            }
        }))
        .on_click(cx.listener(move |this, _, window, cx| this.apply_slash(ix, window, cx)))
        .child(tile_view(tile(command.id), selected))
        .child(div().flex_1().min_w_0().text_size(px(13.)).whitespace_nowrap().child(StyledText::new(command.title).with_runs(runs)))
        .children((!command.hint.is_empty()).then(|| {
            div().flex_none().ml(px(6.)).text_size(px(11.)).font(mono_font()).text_color(c.text3).child(command.hint)
        }))
        .into_any_element()
}

fn tile_view(content: Tile, selected: bool) -> AnyElement {
    let c = t();
    let inner: AnyElement = match content {
        Tile::Letters(letters) => div()
            .text_size(px(9.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c.text2)
            .child(letters)
            .into_any_element(),
        Tile::SerifLetters(letters) => div()
            .text_size(px(11.))
            .font(family_font("New York"))
            .font_weight(FontWeight::MEDIUM)
            .text_color(c.text2)
            .child(letters)
            .into_any_element(),
        Tile::Symbol(name) => icons::symbol(name, 10.5, native::weight::MEDIUM, c.text2),
    };
    div()
        .flex_none()
        .size(px(22.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .bg(c.page.opacity(if c.glass { 0.6 } else { 1. }))
        .border_1()
        .border_color(if selected { c.accent.opacity(0.35) } else { c.control_border })
        .child(inner)
        .into_any_element()
}

fn footer() -> AnyElement {
    let c = t();
    div()
        .relative()
        .h(px(FOOTER_HEIGHT - 1.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(12.))
        .text_size(px(11.))
        .text_color(c.text3)
        .child(div().absolute().top_0().left_0().right_0().h(px(1.)).bg(c.border))
        .child("↑↓ Move")
        .child("↵ Insert")
        .child("esc Close")
        .into_any_element()
}

/// Lays its child out where the slash turned out to be this frame. Deferred, so
/// the list has placed its lines first, and follows scrolling with no lag. With
/// the slash out of view, nothing shows.
struct Following {
    child: AnyElement,
    slash: Rc<Cell<Option<Bounds<Pixels>>>>,
    list: ListState,
    height: f32,
    placed: bool,
}

impl IntoElement for Following {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Following {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(slash) = self.slash.get() else { return };
        let at = place(slash, self.list.viewport_bounds(), self.height);
        window.with_element_offset(at - bounds.origin, |window| self.child.prepaint(window, cx));
        self.placed = true;
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.placed {
            self.child.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn every_item_has_its_tile() {
        for command in &slash_command::ALL {
            let expected = match command.id {
                "text" => Tile::SerifLetters("Aa"),
                "math" => Tile::SerifLetters("∑"),
                "h1" => Tile::Letters("H1"),
                "h2" => Tile::Letters("H2"),
                "h3" => Tile::Letters("H3"),
                "numbers" => Tile::Letters("1."),
                "inlineCode" => Tile::Letters("`"),
                _ => {
                    assert!(matches!(tile(command.id), Tile::Symbol(_)), "{}", command.id);
                    continue;
                }
            };
            assert_eq!(tile(command.id), expected);
        }
    }

    #[core::prelude::v1::test]
    fn it_measures_as_designed() {
        let mut menu = SlashMenu::open(0);
        assert_eq!(MAX_LIST_HEIGHT, 294.);
        // All twenty items and four labels: capped, with a footer.
        assert_eq!(menu.natural_list_height(), 20. * 30. + 4. * 24.);
        assert_eq!(menu.list_height(), 294.);
        assert_eq!(menu.height(), 294. + 10. + 29.);
        assert!(menu.update("bold"));
        assert_eq!(menu.list_height(), 30.);
        // No match: one row, no footer.
        assert!(menu.update("zzz") || menu.results.is_empty());
    }

    #[core::prelude::v1::test]
    fn three_characters_past_the_last_match_close_it() {
        let mut menu = SlashMenu::open(0);
        assert!(menu.update("bo"));
        assert!(menu.update("boq"));
        assert!(menu.results.is_empty());
        assert_eq!(menu.height(), ROW_HEIGHT + PADDING * 2.);
        assert!(menu.update("boqq"));
        assert!(!menu.update("boqqq"));
    }

    #[core::prelude::v1::test]
    fn typing_goes_to_the_first_result_and_clearing_keeps_the_item() {
        let mut menu = SlashMenu::open(0);
        menu.move_selection(3);
        assert_eq!(menu.selected, 3);
        let item = menu.results[3].command.id;
        assert!(menu.update(""));
        assert_eq!(menu.results[menu.selected].command.id, item);
        assert!(menu.update("h"));
        assert_eq!(menu.selected, 0);
        menu.move_selection(1);
        let item = menu.results[1].command.id;
        assert!(menu.update(""));
        assert_eq!(menu.results[menu.selected].command.id, item);
    }

    #[core::prelude::v1::test]
    fn the_selection_stops_at_the_ends() {
        let mut menu = SlashMenu::open(0);
        menu.move_selection(-1);
        assert_eq!(menu.selected, 0);
        menu.move_selection(100);
        assert_eq!(menu.selected, 19);
        menu.move_selection(1);
        assert_eq!(menu.selected, 19);
    }

    #[core::prelude::v1::test]
    fn rows_are_counted_past_their_labels() {
        let menu = SlashMenu::open(0);
        assert_eq!(menu.child_index(0), Some(1));
        assert_eq!(menu.child_index(7), Some(8));
        // Callout opens "Blocks".
        assert_eq!(menu.child_index(8), Some(10));
        assert_eq!(menu.child_index(99), None);
    }

    #[core::prelude::v1::test]
    fn the_query_is_what_can_still_be_one() {
        assert_eq!(query_of("he", false).as_deref(), Some("he"));
        assert_eq!(query_of("he llo", false), None);
        assert_eq!(query_of("a\nb", false), None);
        assert_eq!(query_of("ni' hao", true).as_deref(), Some("nihao"));
        assert_eq!(query_of(&"x".repeat(32), false).map(|q| q.len()), Some(32));
        assert_eq!(query_of(&"x".repeat(33), false), None);
    }

    #[core::prelude::v1::test]
    fn only_a_lone_slash_opens_it() {
        assert!(is_lone_trigger("/"));
        assert!(is_lone_trigger("、"));
        assert!(!is_lone_trigger("//"));
        assert!(!is_lone_trigger("a"));
        assert!(!is_lone_trigger(""));
    }

    fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    #[core::prelude::v1::test]
    fn it_opens_under_the_slash_with_its_tile_aligned() {
        let page = b(0., 0., 1000., 800.);
        let at = place(b(100., 100., 1., 27.), page, 333.);
        assert_eq!((at.x, at.y), (px(91.), px(133.)));
    }

    #[core::prelude::v1::test]
    fn it_flips_above_without_room_and_stays_on_the_page() {
        let page = b(0., 0., 1000., 800.);
        let at = place(b(100., 700., 1., 27.), page, 333.);
        assert_eq!(at.y, px(700. - 6. - 333.));
        // No room either way: stays below.
        let at = place(b(100., 100., 1., 27.), b(0., 0., 1000., 200.), 333.);
        assert_eq!(at.y, px(133.));
        // Kept inside the inset on both sides.
        assert_eq!(place(b(990., 100., 1., 27.), page, 100.).x, px(1000. - 8. - 248.));
        assert_eq!(place(b(2., 100., 1., 27.), page, 100.).x, px(8.));
    }

    #[core::prelude::v1::test]
    fn matched_letters_make_runs() {
        assert_eq!(title_runs("Bold", &[0, 1]), [("Bo".to_string(), true), ("ld".to_string(), false)]);
        assert_eq!(title_runs("Bold", &[]), [("Bold".to_string(), false)]);
        assert_eq!(title_runs("Bold", &[1]), [("B".to_string(), false), ("o".to_string(), true), ("ld".to_string(), false)]);
    }
}
