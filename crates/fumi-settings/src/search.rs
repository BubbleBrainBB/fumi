//! Spotlight-style quick search floating over the window: a port of
//! `SearchPopover`. The fuzzy matching itself is `fumi_core::fuzzy_search`.

use crate::anim::{Transition, ease_out};
use crate::scroller::{ScrollTarget, Scroller};
use fumi_core::fuzzy_search::{self, DEFAULT_LIMIT, FuzzyRange, NoteMatchField, NoteSearchHit, SearchCorpus, Snippet};
use fumi_core::note::Note;
use fumi_ui::icons::{Glyph, glyph, symbol};
use fumi_ui::native::weight;
use fumi_ui::text_field::{FieldEvent, TextField};
use fumi_ui::theme::t;
use fumi_workspace::Workspace;
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const PANEL_WIDTH: f32 = 600.;
pub const PANEL_TOP: f32 = 92.;
pub const RADIUS: f32 = 12.;
const FADE_SECS: f32 = 0.16;
const APPEAR_SCALE: f32 = 0.96;
const DEBOUNCE: Duration = Duration::from_millis(60);
const SCROLL_SECS: f32 = 0.12;
const RECENT_COUNT: usize = 6;

// Row metrics: 7 + 16 + 2 + 15 + 7, spaced by 1, under a 22 tall label and 6 of padding.
pub const ROW_HEIGHT: f32 = 47.;
pub const ROW_SPACING: f32 = 1.;
pub const LABEL_HEIGHT: f32 = 22.;
pub const LIST_PADDING_Y: f32 = 6.;

/// A hit, owned, so it can cross from the search thread.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub note_id: String,
    pub filename: String,
    pub pinned: bool,
    pub field: NoteMatchField,
    pub title_ranges: Vec<FuzzyRange>,
    pub subtitle: String,
    pub subtitle_ranges: Vec<FuzzyRange>,
    pub snippet: Option<Snippet>,
}

impl Row {
    fn of(hit: &NoteSearchHit<'_>) -> Self {
        Row {
            note_id: hit.note.id.clone(),
            filename: hit.note.filename(),
            pinned: hit.note.pinned,
            field: hit.field,
            title_ranges: hit.title_ranges.clone(),
            subtitle: hit.subtitle.clone(),
            subtitle_ranges: hit.subtitle_ranges.clone(),
            snippet: hit.snippet.clone(),
        }
    }
}

// The panel's rules, kept free of the UI.

pub fn trimmed(query: &str) -> &str {
    // `.whitespacesAndNewlines`: Unicode's, plus the zero-width space.
    query.trim_matches(|c: char| c.is_whitespace() || c == '\u{200B}')
}

/// The row ↩ would open: the hovered one, else the keyboard's.
pub fn active_index(hover: Option<usize>, selection: usize, count: usize) -> usize {
    let index = hover.unwrap_or(selection);
    if index < count { index } else { 0 }
}

/// ↑↓ wrap around.
pub fn moved_selection(selection: usize, delta: i32, count: usize) -> usize {
    if count == 0 {
        return selection;
    }
    let next = selection as i64 + delta as i64;
    if next < 0 {
        count - 1
    } else if next >= count as i64 {
        0
    } else {
        next as usize
    }
}

pub fn section_title(query_empty: bool, hit_count: usize, settled: bool) -> String {
    if query_empty {
        return "Recently Updated".into();
    }
    if hit_count == 0 {
        return if settled { "No matching notes".into() } else { "Searching…".into() };
    }
    format!("{hit_count} result{}", if hit_count == 1 { "" } else { "s" })
}

/// Hugs short lists, caps at 480 for long ones.
pub fn panel_max_height(query_empty: bool, hit_count: usize) -> f32 {
    if query_empty || hit_count == 0 {
        return 330.;
    }
    (112. + hit_count as f32 * 44.).min(480.)
}

/// With nothing found the panel hugs its message instead of filling its cap.
pub fn shows_empty_state(query_empty: bool, hit_count: usize, settled: bool) -> bool {
    !query_empty && hit_count == 0 && settled
}

/// The scroll offset that centers row `index`.
pub fn centered_offset(index: usize, viewport: f32, max_offset: f32) -> f32 {
    let top = LIST_PADDING_Y + LABEL_HEIGHT + ROW_SPACING + index as f32 * (ROW_HEIGHT + ROW_SPACING);
    (top + ROW_HEIGHT / 2. - viewport / 2.).clamp(0., max_offset.max(0.))
}

/// Ranges that are safe to style: within the text, on character boundaries.
pub fn valid_ranges(text: &str, ranges: &[FuzzyRange]) -> Vec<FuzzyRange> {
    ranges
        .iter()
        .filter(|r| r.start < r.end && r.end <= text.len() && text.is_char_boundary(r.start) && text.is_char_boundary(r.end))
        .cloned()
        .collect()
}

fn search_rows(corpus: &SearchCorpus, query: &str) -> Vec<Row> {
    corpus.hits(query, DEFAULT_LIMIT).iter().map(Row::of).collect()
}

fn recent_rows(notes: &[Note]) -> Vec<Row> {
    fuzzy_search::recent(notes, RECENT_COUNT).into_iter().map(|n| Row::of(&NoteSearchHit::recent(n))).collect()
}

fn highlighted(text: &str, ranges: &[FuzzyRange], accent: Hsla) -> StyledText {
    let styles = valid_ranges(text, ranges).into_iter().map(|r| {
        (r, HighlightStyle { color: Some(accent), font_weight: Some(FontWeight::BOLD), ..Default::default() })
    });
    StyledText::new(SharedString::from(text.to_string())).with_highlights(styles)
}

struct ScrollTween {
    from: f32,
    to: f32,
    started: Instant,
}

pub struct SearchPanel {
    workspace: Entity<Workspace>,
    field: Entity<TextField>,
    shown: Transition,
    prev_focus: Option<FocusHandle>,
    query: String,
    selection: usize,
    hover: Option<usize>,
    recent: Vec<Row>,
    result_query: String,
    result: Vec<Row>,
    corpus: Option<Arc<SearchCorpus>>,
    corpus_notes: usize,
    corpus_task: Option<Task<()>>,
    search_task: Option<Task<()>>,
    scroll: ScrollHandle,
    tween: Option<ScrollTween>,
    scroller: Scroller,
    _subs: Vec<Subscription>,
}

impl SearchPanel {
    pub fn new(workspace: Entity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| {
            let mut f = TextField::new("Search notes", window, cx);
            f.font_size = px(15.);
            f.line_height = px(18.);
            f
        });
        let subs = vec![
            cx.observe_in(&workspace, window, |this, _, window, cx| this.workspace_changed(window, cx)),
            cx.subscribe_in(&field, window, |this, _, event: &FieldEvent, window, cx| match event {
                FieldEvent::Changed => this.query_changed(cx),
                FieldEvent::Submit => {
                    let new_tab = window.modifiers().platform;
                    this.open_selected(new_tab, cx);
                }
                FieldEvent::Cancel => this.dismiss(cx),
                FieldEvent::MoveUp => this.move_selection(-1, cx),
                FieldEvent::MoveDown => this.move_selection(1, cx),
                _ => {}
            }),
        ];
        let scroll = ScrollHandle::new();
        SearchPanel {
            workspace,
            field,
            shown: Transition::new(false, FADE_SECS),
            prev_focus: None,
            query: String::new(),
            selection: 0,
            hover: None,
            recent: Vec::new(),
            result_query: String::new(),
            result: Vec::new(),
            corpus: None,
            corpus_notes: 0,
            corpus_task: None,
            search_task: None,
            scroller: Scroller::new(ScrollTarget::Handle(scroll.clone())),
            scroll,
            tween: None,
            _subs: subs,
        }
    }

    fn workspace_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let w = self.workspace.read(cx);
        let wanted = w.model.search_presented && w.store.is_open();
        let count = w.store.notes().len();
        if self.corpus.is_some() && count != self.corpus_notes {
            self.corpus = None;
        }
        if wanted == self.shown.target() {
            return;
        }
        self.shown.set(wanted, Instant::now());
        if wanted {
            self.prepare(window, cx);
        } else {
            self.search_task = None;
            self.corpus_task = None;
            if let Some(handle) = self.prev_focus.take() {
                window.focus(&handle, cx);
            }
        }
        cx.notify();
    }

    /// A fresh panel each time, as Swift's view state is.
    fn prepare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prev_focus = window.focused(cx);
        self.field.update(cx, |f, cx| f.set_text("", cx));
        self.query.clear();
        self.selection = 0;
        self.hover = None;
        self.result_query.clear();
        self.result.clear();
        self.corpus = None;
        self.tween = None;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.recent = recent_rows(self.workspace.read(cx).store.notes());
        let focus = self.field.read(cx).focus.clone();
        window.focus(&focus, cx);
    }

    fn query_empty(&self) -> bool {
        self.query.is_empty()
    }

    fn settled(&self) -> bool {
        self.result_query == self.query
    }

    fn hits(&self) -> &[Row] {
        if self.query_empty() { &self.recent } else { &self.result }
    }

    fn query_changed(&mut self, cx: &mut Context<Self>) {
        self.selection = 0;
        let query = trimmed(self.field.read(cx).text()).to_string();
        if query != self.query {
            self.query = query;
            self.schedule_search(cx);
        }
        cx.notify();
    }

    /// A newer query cancels this one. With the corpus ready it waits a moment,
    /// so fast typing doesn't queue a search per keystroke.
    fn schedule_search(&mut self, cx: &mut Context<Self>) {
        self.search_task = None;
        if self.query.is_empty() {
            return;
        }
        let Some(corpus) = self.corpus.clone() else {
            self.build_corpus(cx);
            return;
        };
        let query = self.query.clone();
        self.search_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let q = query.clone();
            let rows = cx.background_spawn(async move { search_rows(&corpus, &q) }).await;
            let _ = this.update(cx, |this, cx| {
                this.result_query = query;
                this.result = rows;
                cx.notify();
            });
        }));
    }

    /// Prepares every note's body once, off the main thread. Whatever is typed
    /// meanwhile just waits for it.
    fn build_corpus(&mut self, cx: &mut Context<Self>) {
        if self.corpus_task.is_some() {
            return;
        }
        let notes: Vec<Note> = self.workspace.read(cx).store.notes().to_vec();
        let count = notes.len();
        self.corpus_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let corpus = cx.background_spawn(async move { Arc::new(SearchCorpus::new(notes)) }).await;
            let _ = this.update(cx, |this, cx| {
                this.corpus_task = None;
                this.corpus = Some(corpus.clone());
                this.corpus_notes = count;
                // The first search skips the pause.
                if !this.query.is_empty() {
                    let query = this.query.clone();
                    let corpus = corpus.clone();
                    this.search_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
                        let q = query.clone();
                        let rows = cx.background_spawn(async move { search_rows(&corpus, &q) }).await;
                        let _ = this.update(cx, |this, cx| {
                            this.result_query = query;
                            this.result = rows;
                            cx.notify();
                        });
                    }));
                }
            });
        }));
    }

    fn move_selection(&mut self, delta: i32, cx: &mut Context<Self>) {
        let count = self.hits().len();
        if count == 0 {
            return;
        }
        self.selection = moved_selection(self.selection, delta, count);
        let target = ScrollTarget::Handle(self.scroll.clone());
        self.tween = Some(ScrollTween {
            from: target.offset(),
            to: centered_offset(self.selection, target.viewport(), target.max_offset()),
            started: Instant::now(),
        });
        cx.notify();
    }

    fn open_selected(&mut self, new_tab: bool, cx: &mut Context<Self>) {
        // ⏎ typed faster than the background search: finish it here. A search over
        // the prepared corpus takes about 10 ms.
        if !self.query_empty() && !self.settled() {
            if let Some(corpus) = self.corpus.clone() {
                self.search_task = None;
                self.result = search_rows(&corpus, &self.query);
                self.result_query = self.query.clone();
            }
        }
        let hits = self.hits();
        let index = active_index(self.hover, self.selection, hits.len());
        if index < hits.len() {
            self.open_row(index, new_tab, cx);
        }
    }

    fn open_row(&mut self, index: usize, new_tab: bool, cx: &mut Context<Self>) {
        let Some(id) = self.hits().get(index).map(|r| r.note_id.clone()) else { return };
        self.workspace.update(cx, |w, cx| {
            w.model.open_note(Some(&id), new_tab);
            w.navigated(cx);
        });
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.model.search_presented = false;
            cx.notify();
        });
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let empty = self.field.read(cx).text().is_empty();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .py(px(11.))
            .child(symbol("magnifyingglass", 15., weight::MEDIUM, c.icon))
            .child(div().flex_1().min_w_0().text_color(c.text).child(self.field.clone()))
            .when(empty, |d| d.child(shortcut_hint("esc")))
            .when(!empty, |d| {
                d.child(
                    div()
                        .id("search-clear")
                        .flex_none()
                        .cursor_pointer()
                        .tooltip(crate::controls::tip("Clear (esc)"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.field.update(cx, |f, cx| f.set_text("", cx));
                            this.query_changed(cx);
                        }))
                        .child(symbol("xmark.circle.fill", 13., weight::REGULAR, c.icon)),
                )
            })
    }

    fn render_row(&self, index: usize, row: &Row, selected: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let c = t();
        let icon_color = if selected { c.accent } else { c.faint_icon };
        let hovering = self.hover == Some(index);
        let icon = if row.pinned { symbol("pin.fill", 12., weight::REGULAR, icon_color) } else { glyph(Glyph::Note, 15., icon_color) };
        let title = highlighted(&row.filename, &row.title_ranges, c.accent);
        div()
            .id(("search-row", index))
            .flex()
            .flex_none()
            .items_start()
            .h(px(ROW_HEIGHT))
            .gap(px(10.))
            .px(px(12.))
            .py(px(7.))
            .rounded(px(7.))
            .cursor_pointer()
            .when(selected, |d| d.bg(c.accent_selected))
            .when(!selected && hovering, |d| d.bg(c.card_hover))
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                if *hovering {
                    this.hover = Some(index);
                } else if this.hover == Some(index) {
                    this.hover = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                this.open_row(index, e.modifiers().platform, cx);
            }))
            .child(div().w(px(16.)).flex_none().flex().justify_center().pt(px(2.)).child(icon))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(16.))
                            .text_color(c.text)
                            .when(selected, |d| d.font_weight(FontWeight::SEMIBOLD))
                            .truncate()
                            .child(title),
                    )
                    .child(second_line(row)),
            )
            .when(selected, |d| {
                d.child(div().flex_none().pt(px(3.)).child(symbol("return", 10., weight::MEDIUM, c.text3)))
            })
    }

    fn render_results(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let title = section_title(self.query_empty(), self.hits().len(), self.settled());
        let count = self.hits().len();
        let active = active_index(self.hover, self.selection, count);
        let rows: Vec<_> =
            self.hits().iter().enumerate().map(|(i, row)| self.render_row(i, row, i == active, cx).into_any_element()).collect();
        let thumb = self.scroller.thumb(|this: &mut Self| &mut this.scroller, Instant::now(), window, cx);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .id("search-results")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(ROW_SPACING))
                            .px(px(8.))
                            .py(px(LIST_PADDING_Y))
                            .child(
                                div()
                                    .flex_none()
                                    .h(px(LABEL_HEIGHT))
                                    .px(px(10.))
                                    .pt(px(4.))
                                    .pb(px(3.))
                                    .text_size(px(12.))
                                    .line_height(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(c.text3)
                                    .child(title),
                            )
                            .children(rows),
                    ),
            )
            .children(thumb)
    }

    fn render_footer(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_none()
            .justify_center()
            .gap(px(16.))
            .px(px(14.))
            .py(px(7.))
            .child(labeled_key("↑↓", "Navigate"))
            .child(labeled_key("↩", "Open"))
            .child(labeled_key("esc", "Dismiss"))
    }
}

/// What matched: the body excerpt, the title, the tag or the folder path.
fn second_line(row: &Row) -> impl IntoElement + use<> {
    let c = t();
    let line = div().flex().items_center().gap(px(5.)).text_size(px(12.)).line_height(px(15.)).h(px(15.)).min_w_0();
    let piece = |color: Hsla| div().text_color(color).truncate().min_w_0();
    match row.field {
        NoteMatchField::Content => line
            .when(!row.subtitle.is_empty(), |d| {
                d.child(piece(c.text3).flex_shrink_0().max_w(px(180.)).child(row.subtitle.clone()))
                    .child(div().flex_none().text_color(c.faint_text).child("·"))
            })
            .when_some(row.snippet.clone(), |d, snippet| {
                d.child(piece(c.text2).flex_1().child(highlighted(&snippet.text, &snippet.ranges, c.accent)))
            }),
        NoteMatchField::Tag => line
            .child(div().flex_none().child(glyph(Glyph::Tag, 11., c.faint_icon)))
            .child(piece(c.text2).flex_1().child(highlighted(&row.subtitle, &row.subtitle_ranges, c.accent))),
        NoteMatchField::Title => {
            line.child(piece(c.text2).flex_1().child(highlighted(&row.subtitle, &row.subtitle_ranges, c.accent)))
        }
        NoteMatchField::FileName | NoteMatchField::Folder | NoteMatchField::Recent => {
            line.child(piece(c.text3).flex_1().child(highlighted(&row.subtitle, &row.subtitle_ranges, c.accent)))
        }
    }
}

fn shortcut_hint(text: &'static str) -> impl IntoElement {
    let c = t();
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(px(18.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c.text3)
        .child(text)
}

fn labeled_key(key: &'static str, label: &'static str) -> impl IntoElement {
    let c = t();
    div()
        .flex()
        .items_center()
        .gap(px(5.))
        .child(shortcut_hint(key))
        .child(div().text_size(px(12.)).text_color(c.text3).child(label))
}

fn empty_state(query: &str) -> impl IntoElement + use<> {
    let c = t();
    div()
        .flex()
        .flex_col()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .w_full()
        .py(px(34.))
        .child(
            div()
                .w(px(44.))
                .h(px(44.))
                .rounded_full()
                .bg(c.quiet_surface)
                .flex()
                .items_center()
                .justify_center()
                .child(symbol("magnifyingglass", 20., weight::MEDIUM, c.faint_icon)),
        )
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.text2)
                .child(format!("No results for “{query}”")),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(c.text3)
                .child("Try different keywords, or search tags with a # prefix."),
        )
}

impl Render for SearchPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        if self.shown.is_gone(now) {
            return div().absolute().size_0().into_any_element();
        }
        if self.shown.is_animating(now) {
            window.request_animation_frame();
        }
        if let Some(tween) = &self.tween {
            let p = now.saturating_duration_since(tween.started).as_secs_f32() / SCROLL_SECS;
            let at = tween.from + (tween.to - tween.from) * ease_out(p);
            self.scroll.set_offset(point(px(0.), px(-at)));
            if p >= 1. {
                self.tween = None;
            } else {
                window.request_animation_frame();
            }
        }
        let c = t();
        let p = self.shown.value(now);
        let box_width = PANEL_WIDTH * (APPEAR_SCALE + (1. - APPEAR_SCALE) * p);
        let empty = shows_empty_state(self.query_empty(), self.hits().len(), self.settled());
        let max_height = panel_max_height(self.query_empty(), self.hits().len());
        let room = f32::from(window.viewport_size().height) - PANEL_TOP - 8.;
        let height = max_height.min(room.max(120.));

        let header = self.render_header(cx).into_any_element();
        let middle = if empty {
            empty_state(&self.query).into_any_element()
        } else {
            self.render_results(window, cx).into_any_element()
        };
        let footer = self.render_footer().into_any_element();
        let rule = || div().w_full().h(px(1.)).flex_none().bg(c.border);

        let inner = div()
            .w(px(PANEL_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .when(empty, |d| d.max_h(px(height)))
            .when(!empty, |d| d.h(px(height)))
            .child(header)
            .child(rule())
            .child(middle)
            .child(rule())
            .child(footer);

        // The box grows from 96% around content that stays still.
        let panel = div()
            .id("search-panel")
            .occlude()
            .w(px(box_width))
            .flex()
            .justify_center()
            .overflow_hidden()
            .rounded(px(RADIUS))
            .bg(c.page)
            .border_1()
            .border_color(c.control_border)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.22),
                offset: point(px(0.), px(12.)),
                blur_radius: px(32.),
                spread_radius: px(0.),
                inset: false,
            }])
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    let focus = this.field.read(cx).focus.clone();
                    window.focus(&focus, cx);
                    window.prevent_default();
                }),
            )
            .on_click(|_, _, _| {})
            .child(inner);

        div()
            .id("search-scrim")
            .absolute()
            .inset_0()
            .occlude()
            .opacity(p)
            .bg(c.scrim)
            .flex()
            .flex_col()
            .items_center()
            .pt(px(PANEL_TOP))
            .font(crate::controls::family_font("System"))
            .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx)))
            .child(panel)
            .into_any_element()
    }
}

/// ⇧⌘F and the sidebar's Search row. The panel shows only with a library open.
pub fn present(workspace: &Entity<Workspace>, cx: &mut App) {
    workspace.update(cx, |w, cx| {
        w.model.search_presented = true;
        cx.notify();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn hover_wins_over_keyboard() {
        assert_eq!(active_index(Some(3), 1, 5), 3);
        assert_eq!(active_index(None, 1, 5), 1);
    }

    #[core::prelude::v1::test]
    fn stale_indexes_fall_back_to_first() {
        assert_eq!(active_index(Some(9), 1, 5), 0);
        assert_eq!(active_index(None, 7, 5), 0);
        assert_eq!(active_index(None, 0, 0), 0);
    }

    #[core::prelude::v1::test]
    fn selection_wraps() {
        assert_eq!(moved_selection(0, -1, 3), 2);
        assert_eq!(moved_selection(2, 1, 3), 0);
        assert_eq!(moved_selection(1, 1, 3), 2);
        assert_eq!(moved_selection(0, 1, 0), 0);
    }

    #[core::prelude::v1::test]
    fn section_titles() {
        assert_eq!(section_title(true, 6, false), "Recently Updated");
        assert_eq!(section_title(false, 0, false), "Searching…");
        assert_eq!(section_title(false, 1, true), "1 result");
        assert_eq!(section_title(false, 12, true), "12 results");
        // Old hits on screen while a new search runs still count.
        assert_eq!(section_title(false, 3, false), "3 results");
    }

    #[core::prelude::v1::test]
    fn panel_height_rules() {
        assert_eq!(panel_max_height(true, 6), 330.);
        assert_eq!(panel_max_height(false, 0), 330.);
        assert_eq!(panel_max_height(false, 1), 156.);
        assert_eq!(panel_max_height(false, 8), 464.);
        assert_eq!(panel_max_height(false, 9), 480.);
        assert_eq!(panel_max_height(false, 100), 480.);
    }

    #[core::prelude::v1::test]
    fn empty_state_only_when_settled_on_nothing() {
        assert!(shows_empty_state(false, 0, true));
        assert!(!shows_empty_state(false, 0, false));
        assert!(!shows_empty_state(true, 0, true));
        assert!(!shows_empty_state(false, 2, true));
    }

    #[core::prelude::v1::test]
    fn centering_clamps_to_the_scroll_range() {
        assert_eq!(centered_offset(0, 300., 1000.), 0.);
        let mid = centered_offset(10, 300., 1000.);
        assert_eq!(mid, 6. + 22. + 1. + 480. + 23.5 - 150.);
        assert_eq!(centered_offset(99, 300., 1000.), 1000.);
        assert_eq!(centered_offset(5, 300., 0.), 0.);
    }

    #[core::prelude::v1::test]
    fn bad_ranges_are_dropped() {
        let text = "日本語abc";
        let ranges = vec![0..3, 1..4, 9..12, 9..40, 5..5];
        assert_eq!(valid_ranges(text, &ranges), vec![0..3, 9..12]);
    }

    #[core::prelude::v1::test]
    fn query_is_trimmed_of_whitespace_and_newlines() {
        assert_eq!(trimmed("  a b\n"), "a b");
        assert_eq!(trimmed(" \n "), "");
    }
}
