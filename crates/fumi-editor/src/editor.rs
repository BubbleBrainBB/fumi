//! The editor view: source-exact state, incremental block structure, and the
//! virtualized list of items that GPUI's `list` lays out on demand.

use crate::blocks::{self, BlockCache};
use fumi_core::markdown::{find_title, scan_all};
use fumi_ui::native::SymbolCache;
use fumi_ui::theme::t;
use crate::display::*;
use fumi_core::doc::Doc;
use crate::element::*;
use fumi_core::markdown::*;
use crate::metrics;
use gpui::prelude::*;
use gpui::*;
use std::cell::RefCell;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    editor,
    [
        Backspace, Delete, Enter, Left, Right, Up, Down, SelectLeft, SelectRight, SelectUp,
        SelectDown, WordLeft, WordRight, SelectWordLeft, SelectWordRight, LineStart, LineEnd,
        SelectLineStart, SelectLineEnd, DocStart, DocEnd, SelectDocStart, SelectDocEnd, PageUp,
        PageDown, SelectAll, Copy, Cut, Paste, Undo, Redo, Save, CharacterPalette, Quit, Bold, Italic, InsertLink, ToggleTaskLine,
    ]
);

pub fn bind_keys(cx: &mut App) {
    let c = Some("Editor");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, c),
        KeyBinding::new("shift-backspace", Backspace, c),
        KeyBinding::new("delete", Delete, c),
        KeyBinding::new("enter", Enter, c),
        KeyBinding::new("shift-enter", Enter, c),
        KeyBinding::new("left", Left, c),
        KeyBinding::new("right", Right, c),
        KeyBinding::new("up", Up, c),
        KeyBinding::new("down", Down, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new("alt-left", WordLeft, c),
        KeyBinding::new("alt-right", WordRight, c),
        KeyBinding::new("alt-shift-left", SelectWordLeft, c),
        KeyBinding::new("alt-shift-right", SelectWordRight, c),
        KeyBinding::new("cmd-left", LineStart, c),
        KeyBinding::new("cmd-right", LineEnd, c),
        KeyBinding::new("ctrl-a", LineStart, c),
        KeyBinding::new("ctrl-e", LineEnd, c),
        KeyBinding::new("cmd-shift-left", SelectLineStart, c),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, c),
        KeyBinding::new("cmd-up", DocStart, c),
        KeyBinding::new("cmd-down", DocEnd, c),
        KeyBinding::new("cmd-shift-up", SelectDocStart, c),
        KeyBinding::new("cmd-shift-down", SelectDocEnd, c),
        KeyBinding::new("pageup", PageUp, c),
        KeyBinding::new("pagedown", PageDown, c),
        KeyBinding::new("cmd-a", SelectAll, c),
        KeyBinding::new("cmd-c", Copy, c),
        KeyBinding::new("cmd-x", Cut, c),
        KeyBinding::new("cmd-v", Paste, c),
        KeyBinding::new("cmd-z", Undo, c),
        KeyBinding::new("cmd-shift-z", Redo, c),
        KeyBinding::new("cmd-s", Save, c),
        KeyBinding::new("cmd-b", Bold, c),
        KeyBinding::new("cmd-i", Italic, c),
        KeyBinding::new("cmd-k", InsertLink, c),
        KeyBinding::new("cmd-l", ToggleTaskLine, c),
        KeyBinding::new("ctrl-cmd-space", CharacterPalette, c),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
}

const TOP_PAD: f32 = 30.;
const BOTTOM_PAD: f32 = 30.;
const MIN_SIDE_INSET: f32 = 40.;
/// What an item not yet laid out is assumed to measure.
const ITEM_HINT: f32 = 48.;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ItemKind {
    Line,
    Table,
    Math,
    Image,
    Front,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Item {
    pub start: u32,
    pub len: u32,
    pub kind: ItemKind,
}

struct Op {
    at: usize,
    old: String,
    new: String,
    sel_before: (usize, usize),
    sel_after: (usize, usize),
    typing: bool,
}

pub struct Editor {
    pub focus: FocusHandle,
    pub doc: Doc,
    pub path: Option<PathBuf>,
    pub dir: PathBuf,
    pub lines: Vec<LineInfo>,
    pub items: Vec<Item>,
    title_line: Option<usize>,
    pub list: ListState,
    pub anchor: usize,
    pub head: usize,
    goal_x: Option<Pixels>,
    pub marked: Option<Range<usize>>,
    undo: Vec<Op>,
    redo: Vec<Op>,
    can_coalesce: bool,
    revealed: Option<Range<usize>>,
    caret_line: usize,
    pub painted: Rc<RefCell<Vec<Painted>>>,
    pub blocks: BlockCache,
    selecting: bool,
    press_at: Point<Pixels>,
    dragged: bool,
    caret_on: bool,
    blink_epoch: u64,
    pub text_width: Pixels,
    list_width: Pixels,
    margin: Pixels,
    /// `FUMI_BLUR`: leave the page translucent over the window's blur.
    see_through: bool,
    autoscroll_caret: bool,
    last_shown: usize,
    restore_hints: bool,
    pub show_status: bool,
    pub load_error: Option<String>,
    pub typo: Typography,
    pub links: Option<LinkContext>,
    pub symbols: Rc<RefCell<SymbolCache>>,
    hovered_link: Option<(usize, Range<usize>)>,
    pub pane_width: Option<Pixels>,
    pub column_setting: String,
    pending_link: Option<PendingLink>,
    was_focused: bool,
    hovered_web: Option<String>,
    edit_epoch: u64,
    scrolled_at: Option<Instant>,
    last_scroll_top: (usize, f32),
    thumb_hover: bool,
    thumb_drag: Option<f32>,
    /// False when the doc was replaced from outside and has no unsaved change.
    pub dirty: bool,
}

/// What the editor tells the screen around it.
#[derive(Clone, Debug)]
pub enum EditorEvent {
    /// 500 ms after the last change: time to save.
    SaveRequested,
    /// 300 ms after the last change: outline, counts and previews may refresh.
    ContentSettled,
    /// Saved at once, on ⌘S.
    SaveNow,
    /// A web address was clicked; already handed to the browser.
    OpenedWeb(String),
    /// A `[[link]]` was clicked. The inner text, `target#heading|alias`.
    OpenWiki { inner: String, new_tab: bool, anchor: Bounds<Pixels> },
    /// The caret or selection moved.
    SelectionChanged,
}

impl EventEmitter<EditorEvent> for Editor {}

/// A press that may become a link click: it stays one until it drags 3 pt.
struct PendingLink {
    link: Link,
    at: Point<Pixels>,
    new_tab: bool,
    anchor: Bounds<Pixels>,
}

/// Where a note was left, to come back to.
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewState {
    pub anchor: usize,
    pub head: usize,
    pub scroll_item: usize,
    pub scroll_offset: f32,
}

impl Editor {
    pub fn new(bytes: Vec<u8>, path: Option<PathBuf>, blink: bool, cx: &mut Context<Self>) -> Self {
        let dir = path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default();
        let mut this = Self {
            focus: cx.focus_handle(),
            doc: Doc::from_bytes(b"").unwrap(),
            path,
            dir,
            lines: Vec::new(),
            items: Vec::new(),
            title_line: None,
            list: ListState::new(0, ListAlignment::Top, px(300.)),
            anchor: 0,
            head: 0,
            goal_x: None,
            marked: None,
            undo: Vec::new(),
            redo: Vec::new(),
            can_coalesce: false,
            revealed: None,
            caret_line: 0,
            painted: Rc::new(RefCell::new(Vec::new())),
            blocks: BlockCache::default(),
            selecting: false,
            press_at: Point::default(),
            dragged: false,
            caret_on: true,
            blink_epoch: 0,
            text_width: px(720.),
            list_width: px(720.),
            margin: px(0.),
            see_through: std::env::var("FUMI_BLUR").is_ok(),
            autoscroll_caret: false,
            last_shown: 0,
            restore_hints: false,
            show_status: false,
            load_error: None,
            typo: Typography::from_settings(),
            links: None,
            symbols: Rc::new(RefCell::new(SymbolCache::default())),
            hovered_link: None,
            pane_width: None,
            column_setting: fumi_ui::settings::editor_width(),
            pending_link: None,
            was_focused: false,
            hovered_web: None,
            edit_epoch: 0,
            scrolled_at: None,
            last_scroll_top: (0, 0.),
            thumb_hover: false,
            thumb_drag: None,
            dirty: false,
        };
        this.load_bytes(bytes);
        if blink {
            cx.spawn(async move |this: WeakEntity<Editor>, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(530)).await;
                    let alive = this.update(cx, |e, cx| {
                        if e.selection_is_empty() && e.blink_epoch == 0 {
                            e.caret_on = !e.caret_on;
                            cx.notify();
                        }
                        e.blink_epoch = 0;
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        this
    }

    /// Replaces the document. Parsing is one pass over the text; layout only
    /// happens for what the list asks to show.
    pub fn load_bytes(&mut self, bytes: Vec<u8>) {
        let t0 = Instant::now();
        match std::str::from_utf8(&bytes) {
            Ok(text) => {
                self.doc = Doc::from_bytes(text.as_bytes()).unwrap();
                let t1 = Instant::now();
                self.lines = scan_all(text);
                let t2 = Instant::now();
                metrics::note_load(t1 - t0, t2 - t1);
                self.load_error = None;
            }
            Err(e) => {
                // Opening it would mean changing it. Say so and leave it alone.
                self.doc = Doc::from_bytes(b"").unwrap();
                self.lines = scan_all("");
                self.path = None;
                self.load_error = Some(format!("Not UTF-8, not opened: {e}"));
            }
        }
        self.anchor = 0;
        self.head = 0;
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.caret_line = 0;
        self.revealed = self.foldable_block_at(0);
        self.items = build_items(&self.lines, self.revealed.as_ref());
        self.title_line = find_title(&self.lines);
        self.list.reset_with_uniform_height(self.items.len(), px(ITEM_HINT));
    }

    pub fn selection_is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    // Structure ------------------------------------------------------------

    /// The lines of a table or math block touching `line`, whether or not it
    /// is currently folded.
    fn foldable_block_at(&self, line: usize) -> Option<Range<usize>> {
        let kind = self.lines.get(line)?.kind;
        let is_table = |k: Kind| matches!(k, Kind::TableHead | Kind::TableDelim | Kind::TableRow);
        let is_math = |k: Kind| matches!(k, Kind::MathOpen | Kind::MathBody | Kind::MathClose);
        match kind {
            Kind::Image | Kind::MathOne => Some(line..line + 1),
            // Offset 0 keeps the panel; it opens only from inside.
            Kind::FrontOpen | Kind::FrontBody | Kind::FrontClose if self.head == 0 => None,
            Kind::FrontOpen | Kind::FrontBody | Kind::FrontClose => {
                let mut b = line;
                while b < self.lines.len() && self.lines[b].kind != Kind::FrontClose {
                    b += 1;
                }
                (b < self.lines.len()).then_some(0..b + 1)
            }
            k if is_table(k) => {
                let mut a = line;
                while self.lines[a].kind != Kind::TableHead && a > 0 {
                    a -= 1;
                }
                let mut b = line + 1;
                while b < self.lines.len() && matches!(self.lines[b].kind, Kind::TableDelim | Kind::TableRow) {
                    b += 1;
                }
                Some(a..b)
            }
            k if is_math(k) => {
                let mut a = line;
                while self.lines[a].kind != Kind::MathOpen && a > 0 {
                    a -= 1;
                }
                let mut b = line;
                while b < self.lines.len() && self.lines[b].kind != Kind::MathClose {
                    b += 1;
                }
                (b < self.lines.len()).then_some(a..b + 1)
            }
            _ => None,
        }
    }

    /// Re-reads the lines an edit touched, and past them until the scan state
    /// agrees with what it was. Returns whether any classification changed.
    /// `sensitive` says the edit may have changed which fences and `$$` pair up,
    /// which only a pass over the whole text can tell.
    fn rescan(&mut self, first: usize, removed: usize, added: usize, sensitive: bool) -> bool {
        if sensitive {
            let new = scan_all(&self.doc.rope.to_string());
            let changed = new != self.lines;
            self.lines = new;
            return changed;
        }
        let placeholder = LineInfo { kind: Kind::Para, after: Scan::Normal, real: false };
        let mut changed = removed != added;
        self.lines.splice(
            first..first + removed + 1,
            std::iter::repeat_n(placeholder, added + 1),
        );
        let n = self.lines.len();
        let start = first.saturating_sub(1);
        let mut before = if start == 0 { Scan::Normal } else { self.lines[start - 1].after };
        let mut prev = if start == 0 { Kind::Blank } else { self.lines[start - 1].kind };
        let dirty_end = first + added;
        let mut i = start;
        let mut text = self.doc.line_text(i);
        while i < n {
            let next = (i + 1 < n).then(|| self.doc.line_text(i + 1));
            let old = self.lines[i];
            let mut info = classify(&text, next.as_deref(), before, i, old.real);
            info.kind = table_kind(&text, prev, info.kind);
            if i > dirty_end && old == info {
                break;
            }
            if old != info && (removed == added || i > dirty_end) {
                changed = true;
            }
            self.lines[i] = info;
            before = info.after;
            prev = info.kind;
            i += 1;
            text = next.unwrap_or_default();
        }
        changed
    }

    /// Whether replacing `range` with `text` could change block pairing.
    fn edit_is_sensitive(&self, range: &Range<usize>, text: &str) -> bool {
        let first = self.doc.line_of(range.start);
        let last = self.doc.line_of(range.end);
        if first == 0 || self.lines.first().is_some_and(|l| l.kind == Kind::FrontOpen) && first < 200 {
            return true;
        }
        (first..=last).any(|l| pairing_sensitive(&self.doc.line_text(l)))
            || text.split('\n').any(pairing_sensitive)
            || {
                // The line as it will read once joined with what's around the edit.
                let start = self.doc.line_start(first);
                let end = self.doc.line_end(last);
                let joined = format!(
                    "{}{}{}",
                    self.doc.slice(start..range.start),
                    text,
                    self.doc.slice(range.end..end)
                );
                joined.split('\n').any(pairing_sensitive)
            }
    }

    /// Recomputes items and tells the list only what differs.
    fn rebuild_items(&mut self) {
        let new = build_items(&self.lines, self.revealed.as_ref());
        let lines_of = |items: &[Item]| items.last().map_or(0, |i| i.start + i.len);
        if lines_of(&self.items) == lines_of(&new) {
            // Same text, so only folds changed. Touch just those, or the items
            // between an old fold and a new one lose their measurements and the
            // page jumps.
            for (old_range, count) in fold_changes(&self.items, &new).into_iter().rev() {
                self.list.splice(old_range, count);
            }
        } else {
            let old = &self.items;
            let shape = |a: &Item, b: &Item| a.len == b.len && a.kind == b.kind;
            let mut prefix = 0;
            while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
                prefix += 1;
            }
            let mut suffix = 0;
            while suffix < old.len() - prefix
                && suffix < new.len() - prefix
                && shape(&old[old.len() - 1 - suffix], &new[new.len() - 1 - suffix])
            {
                suffix += 1;
            }
            let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
            if prefix != old_end || prefix != new_end {
                self.list.splice(prefix..old_end, new_end - prefix);
            }
        }
        self.items = new;
        self.title_line = find_title(&self.lines);
    }

    pub fn item_of(&self, line: usize) -> usize {
        let line = line.min(self.lines.len().saturating_sub(1)) as u32;
        self.items
            .partition_point(|i| i.start <= line)
            .saturating_sub(1)
    }

    fn remeasure_lines(&self, lines: Range<usize>) {
        if self.items.is_empty() {
            return;
        }
        let a = self.item_of(lines.start);
        let b = self.item_of(lines.end.saturating_sub(1).max(lines.start)) + 1;
        self.list.remeasure_items(a..b.min(self.items.len()));
    }

    /// After the caret moved: fold what it left, unfold what it entered, and
    /// have the two lines whose syntax changed visibility measured again.
    fn caret_moved(&mut self, scroll: bool) {
        metrics::note_change();
        let line = self.doc.line_of(self.head);
        let prev = self.caret_line;
        let block = self.foldable_block_at(line);
        if block != self.revealed {
            self.revealed = block;
            self.rebuild_items();
        }
        // Before remeasuring, while the neighbours still have known bounds.
        if scroll {
            self.reveal(line);
        }
        if line != prev {
            self.remeasure_lines(prev..prev + 1);
            self.caret_line = line;
        }
        // Inline syntax shows only on the span the caret touches, so even a
        // move within the line can change its layout.
        self.remeasure_lines(line..line + 1);
        self.caret_on = true;
        self.blink_epoch = 1;
    }

    /// Brings the caret's item into view. `ListState::scroll_to_reveal_item`
    /// works from measured heights and sends us to the wrong place when the
    /// items between here and there were never laid out, so this only trusts
    /// measured neighbours and otherwise jumps by item index.
    fn reveal(&mut self, line: usize) {
        self.autoscroll_caret = true;
        // On screen already: the element itself asks for any small correction
        // during layout, in the same frame.
        let visible = self
            .painted
            .borrow()
            .iter()
            .any(|i| line >= i.first_line && line < i.first_line + i.n_lines);
        if visible {
            return;
        }
        let ix = self.item_of(line);
        let top = self.list.logical_scroll_top().item_ix;
        if ix < top {
            self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
            return;
        }
        let view = self.list.viewport_bounds();
        let inset = px(BOTTOM_PAD) ;
        let probe = (0..3).find_map(|k| {
            ix.checked_sub(k)
                .filter(|i| *i >= top)
                .and_then(|i| self.list.bounds_for_item(i))
                .map(|b| b.bottom() + px(TOP_PAD) + px(30.) * k as f32)
        });
        match probe {
            Some(bottom) if bottom - view.bottom() < view.size.height * 2. => {
                let over = bottom + inset - view.bottom();
                if over > px(0.) {
                    self.list.scroll_by(over);
                }
            }
            _ => self.list.scroll_to(ListOffset {
                item_ix: ix.saturating_sub(2),
                offset_in_item: px(0.),
            }),
        }
    }

    // Editing --------------------------------------------------------------

    /// The one path every change takes.
    pub fn edit(&mut self, range: Range<usize>, text: &str, typing: bool, composing: bool) {
        let t0 = Instant::now();
        let sel_before = (self.anchor, self.head);
        let first = self.doc.line_of(range.start);
        let removed = self.doc.line_of(range.end) - first;
        let old = self.doc.slice(range.clone());
        let sensitive = self.edit_is_sensitive(&range, text);
        self.doc.replace(range.clone(), text);
        let added = text.bytes().filter(|b| *b == b'\n').count();
        let changed = self.rescan(first, removed, added, sensitive);

        let caret = range.start + text.len();
        self.anchor = caret;
        self.head = caret;
        self.goal_x = None;
        self.marked = None;

        let sel_after = (caret, caret);
        let coalesce = self.can_coalesce
            && self.undo.last().is_some_and(|op| {
                (composing && op.at == range.start && op.new == old)
                    || (typing
                        && op.typing
                        && old.is_empty()
                        && !text.contains('\n')
                        && op.at + op.new.len() == range.start)
            });
        if coalesce {
            let op = self.undo.last_mut().unwrap();
            if composing {
                op.new = text.to_string();
            } else {
                op.new.push_str(text);
            }
            op.sel_after = sel_after;
        } else {
            self.undo.push(Op {
                at: range.start,
                old,
                new: text.to_string(),
                sel_before,
                sel_after,
                typing: typing && !text.contains('\n'),
            });
        }
        self.can_coalesce = typing || composing;
        self.redo.clear();

        if changed {
            self.revealed = self.foldable_block_at(self.doc.line_of(caret));
            self.rebuild_items();
        }
        self.caret_moved(true);
        self.remeasure_lines(first..first + added + 1);
        metrics::note_handler(t0.elapsed());
    }

    /// Starts the 300 ms and 500 ms timers again; only the last edit's fire.
    pub fn schedule_after_edit(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.edit_epoch += 1;
        let epoch = self.edit_epoch;
        cx.spawn(async move |this: WeakEntity<Editor>, cx| {
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let live = this
                .update(cx, |e, cx| {
                    let live = e.edit_epoch == epoch;
                    if live {
                        cx.emit(EditorEvent::ContentSettled);
                    }
                    live
                })
                .unwrap_or(false);
            if !live {
                return;
            }
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let _ = this.update(cx, |e, cx| {
                if e.edit_epoch == epoch && e.marked.is_none() {
                    cx.emit(EditorEvent::SaveRequested);
                }
            });
        })
        .detach();
    }

    /// ⌘B, ⌘I, ⌘K: always adds. With a selection the result, markers included,
    /// stays selected; with none the caret sits inside the markers.
    pub fn wrap_selection(&mut self, open: &str, close: &str, cx: &mut Context<Self>) {
        let sel = self.selection();
        let inner = self.doc.slice(sel.clone());
        let text = format!("{open}{inner}{close}");
        let had_selection = !sel.is_empty();
        self.edit(sel.clone(), &text, false, false);
        if had_selection {
            self.anchor = sel.start;
            self.head = sel.start + text.len();
        } else {
            self.anchor = sel.start + open.len();
            self.head = self.anchor;
        }
        self.caret_moved(true);
        self.schedule_after_edit(cx);
        cx.notify();
    }

    /// ⌘L: flips the first `[ ]`, `[x]` or `[X]` on the caret's line.
    fn toggle_task_on_caret_line(&mut self, cx: &mut Context<Self>) {
        let line = self.doc.line_of(self.head);
        let text = self.doc.line_text(line);
        let found = ["[ ]", "[x]", "[X]"]
            .iter()
            .filter_map(|b| text.find(b))
            .min();
        let Some(at) = found else { return };
        let start = self.doc.line_start(line) + at;
        let new = if &text[at..at + 3] == "[ ]" { "[x]" } else { "[ ]" };
        let sel = (self.anchor, self.head);
        self.edit(start..start + 3, new, false, false);
        self.anchor = sel.0;
        self.head = sel.1;
        self.caret_moved(false);
        self.schedule_after_edit(cx);
        cx.notify();
    }

    /// Replaces the text with what's on disk now, keeping the caret and the
    /// scroll where they were as nearly as the change allows, with undo reset
    /// at the seam. Only what differs is touched.
    pub fn replace_from_outside(&mut self, text: &str, cx: &mut Context<Self>) {
        let old = self.doc.rope.to_string();
        if old == text {
            self.dirty = false;
            return;
        }
        let prefix = old.bytes().zip(text.bytes()).take_while(|(a, b)| a == b).count();
        let mut prefix = prefix;
        while !old.is_char_boundary(prefix) || !text.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let max_suffix = old.len().min(text.len()) - prefix;
        let mut suffix = old.bytes().rev().zip(text.bytes().rev()).take(max_suffix).take_while(|(a, b)| a == b).count();
        while !old.is_char_boundary(old.len() - suffix) || !text.is_char_boundary(text.len() - suffix) {
            suffix -= 1;
        }
        let (old_end, new_end) = (old.len() - suffix, text.len() - suffix);
        let map = |at: usize| {
            if at <= prefix {
                at
            } else if at >= old_end {
                at - old_end + new_end
            } else {
                new_end
            }
        };
        let (anchor, head) = (map(self.anchor), map(self.head));
        self.apply(prefix, old_end - prefix, &text[prefix..new_end], (anchor, head));
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
        cx.notify();
    }

    pub fn view_state(&self) -> ViewState {
        let top = self.list.logical_scroll_top();
        ViewState {
            anchor: self.anchor,
            head: self.head,
            scroll_item: top.item_ix,
            scroll_offset: f32::from(top.offset_in_item),
        }
    }

    pub fn restore_view(&mut self, state: ViewState) {
        let n = self.doc.len();
        self.anchor = state.anchor.min(n);
        self.head = state.head.min(n);
        self.caret_moved(false);
        self.list.scroll_to(ListOffset {
            item_ix: state.scroll_item,
            offset_in_item: px(state.scroll_offset),
        });
    }

    pub fn text(&self) -> String {
        self.doc.rope.to_string()
    }

    /// Scrolls a heading's line to the top with a little margin.
    pub fn scroll_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let ix = self.item_of(line);
        self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(-12.) });
        cx.notify();
    }

    fn apply(&mut self, at: usize, old_len: usize, text: &str, sel: (usize, usize)) {
        let first = self.doc.line_of(at);
        let removed = self.doc.line_of(at + old_len) - first;
        let sensitive = self.edit_is_sensitive(&(at..at + old_len), text);
        self.doc.replace(at..at + old_len, text);
        let added = text.bytes().filter(|b| *b == b'\n').count();
        let changed = self.rescan(first, removed, added, sensitive);
        self.anchor = sel.0;
        self.head = sel.1;
        self.marked = None;
        if changed {
            self.revealed = self.foldable_block_at(self.doc.line_of(self.head));
            self.rebuild_items();
        }
        self.can_coalesce = false;
        self.caret_moved(true);
        self.remeasure_lines(first..first + added + 1);
    }

    fn undo_op(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(op) = self.undo.pop() {
            self.apply(op.at, op.new.len(), &op.old.clone(), op.sel_before);
            self.redo.push(op);
            self.schedule_after_edit(cx);
            cx.notify();
        }
    }

    fn redo_op(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(op) = self.redo.pop() {
            self.apply(op.at, op.old.len(), &op.new.clone(), op.sel_after);
            self.undo.push(op);
            self.schedule_after_edit(cx);
            cx.notify();
        }
    }

    pub fn replace_selection(&mut self, text: &str, typing: bool, cx: &mut Context<Self>) {
        let range = self.selection();
        self.edit(range, text, typing, false);
        self.schedule_after_edit(cx);
        cx.notify();
    }

    /// Plain, as NSTextView has it: no list or quote continuation.
    fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        self.replace_selection("\n", false, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection_is_empty() {
            let at = self.prev_boundary(self.head);
            self.edit(at..self.head, "", false, false);
            self.schedule_after_edit(cx);
        } else {
            self.replace_selection("", false, cx);
        }
        cx.notify();
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection_is_empty() {
            let at = self.next_boundary(self.head);
            self.edit(self.head..at, "", false, false);
            self.schedule_after_edit(cx);
        } else {
            self.replace_selection("", false, cx);
        }
        cx.notify();
    }

    // Movement -------------------------------------------------------------

    fn prev_boundary(&self, at: usize) -> usize {
        let line = self.doc.line_of(at);
        let start = self.doc.line_start(line);
        if at == start {
            return if line == 0 { 0 } else { self.doc.line_end(line - 1) };
        }
        let text = self.doc.line_text(line);
        let off = (at - start).min(text.len());
        start
            + text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|i| *i < off)
                .last()
                .unwrap_or(0)
    }

    fn next_boundary(&self, at: usize) -> usize {
        let line = self.doc.line_of(at);
        let start = self.doc.line_start(line);
        let text = self.doc.line_text(line);
        let off = at - start;
        if off >= text.len() {
            return if line + 1 < self.doc.line_count() {
                self.doc.line_start(line + 1)
            } else {
                self.doc.len()
            };
        }
        start
            + text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|i| *i > off)
                .unwrap_or(text.len())
    }

    fn word_boundary(&self, at: usize, forward: bool) -> usize {
        let line = self.doc.line_of(at);
        let start = self.doc.line_start(line);
        let text = self.doc.line_text(line);
        let off = at - start;
        let words: Vec<(usize, &str)> = text.split_word_bound_indices().collect();
        if forward {
            if off >= text.len() {
                return self.next_boundary(at);
            }
            words
                .iter()
                .find(|(i, w)| i + w.len() > off && w.chars().any(char::is_alphanumeric))
                .map_or(start + text.len(), |(i, w)| start + i + w.len())
        } else {
            if off == 0 {
                return self.prev_boundary(at);
            }
            words
                .iter()
                .rev()
                .find(|(i, w)| *i < off && w.chars().any(char::is_alphanumeric))
                .map_or(start, |(i, _)| start + i)
        }
    }

    /// For the benchmark driver, which sets `anchor` and `head` itself.
    pub fn set_caret_for_driver(&mut self) {
        self.goal_x = None;
        self.can_coalesce = false;
        self.caret_moved(true);
    }

    /// A click or drag: what's under the pointer is already on screen, and the
    /// page must not move under it.
    fn set_head_from_mouse(&mut self, head: usize, extend: bool) {
        self.head = head;
        if !extend {
            self.anchor = head;
        }
        self.goal_x = None;
        self.can_coalesce = false;
        self.marked = None;
        self.caret_moved(false);
    }

    fn set_head(&mut self, head: usize, extend: bool, keep_goal: bool) {
        self.head = head;
        if !extend {
            self.anchor = head;
        }
        if !keep_goal {
            self.goal_x = None;
        }
        self.can_coalesce = false;
        self.marked = None;
        self.caret_moved(true);
    }

    fn shape_line(&self, line: usize, caret: Option<usize>, window: &Window) -> (Rc<DisplayLine>, Rc<Shaped>) {
        let d = Rc::new(self.display_of(line, caret));
        let s = shape(&d, self.list_width, window);
        (d, s)
    }

    /// Up or down one visual row, keeping the caret's x.
    fn vertical(&mut self, down: bool, extend: bool, window: &mut Window) {
        if !extend && !self.selection_is_empty() {
            let edge = if down { self.selection().end } else { self.selection().start };
            self.anchor = edge;
            self.head = edge;
        }
        let line = self.doc.line_of(self.head);
        let start = self.doc.line_start(line);
        let (d, s) = self.shape_line(line, Some(self.head - start), window);
        let disp = d.src_to_disp(self.head - start);
        let row = s.row_of(disp);
        let x = self.goal_x.unwrap_or(d.indent + s.x_of(disp, row));
        self.goal_x = Some(x);

        let target = if down && row + 1 < s.row_count() {
            start + d.disp_to_src(s.index_at(row + 1, x - d.indent))
        } else if !down && row > 0 {
            start + d.disp_to_src(s.index_at(row - 1, x - d.indent))
        } else if down && line + 1 >= self.doc.line_count() {
            self.doc.len()
        } else if !down && line == 0 {
            0
        } else {
            let next = if down { line + 1 } else { line - 1 };
            let item = self.items[self.item_of(next)];
            if item.kind != ItemKind::Line {
                // A folded block opens on the near line, source visible.
                self.doc.line_start(next)
            } else {
                let (nd, ns) = self.shape_line(next, None, window);
                let r = if down { 0 } else { ns.row_count() - 1 };
                self.doc.line_start(next) + nd.disp_to_src(ns.index_at(r, x - nd.indent))
            }
        };
        self.set_head(target, extend, true);
    }

    fn page(&mut self, down: bool, window: &mut Window) {
        let h = window.viewport_size().height * 0.85;
        let delta = if down { h } else { -h };
        self.list.scroll_by(delta);
    }

    fn line_edge(&self, end: bool) -> usize {
        let line = self.doc.line_of(self.head);
        if end { self.doc.line_end(line) } else { self.doc.line_start(line) }
    }

    fn register_actions(&self, el: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        macro_rules! act {
            ($el:ident, $action:ty, |$this:ident, $window:ident| $body:expr) => {
                $el.on_action(cx.listener(|$this: &mut Self, _: &$action, $window: &mut Window, cx| {
                    $body;
                    cx.notify();
                }))
            };
        }
        let el = el
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::undo_op))
            .on_action(cx.listener(Self::redo_op))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(|this, _: &Bold, _, cx| this.wrap_selection("**", "**", cx)))
            .on_action(cx.listener(|this, _: &Italic, _, cx| this.wrap_selection("*", "*", cx)))
            .on_action(cx.listener(|this, _: &InsertLink, _, cx| this.wrap_selection("[", "](url)", cx)))
            .on_action(cx.listener(|this, _: &ToggleTaskLine, _, cx| this.toggle_task_on_caret_line(cx)))
            .on_action(cx.listener(|_, _: &CharacterPalette, window, _| window.show_character_palette()));
        let el = act!(el, Left, |this, _w| {
            let to = if this.selection_is_empty() { this.prev_boundary(this.head) } else { this.selection().start };
            this.set_head(to, false, false)
        });
        let el = act!(el, Right, |this, _w| {
            let to = if this.selection_is_empty() { this.next_boundary(this.head) } else { this.selection().end };
            this.set_head(to, false, false)
        });
        let el = act!(el, Up, |this, w| this.vertical(false, false, w));
        let el = act!(el, Down, |this, w| this.vertical(true, false, w));
        let el = act!(el, SelectLeft, |this, _w| this.set_head(this.prev_boundary(this.head), true, false));
        let el = act!(el, SelectRight, |this, _w| this.set_head(this.next_boundary(this.head), true, false));
        let el = act!(el, SelectUp, |this, w| this.vertical(false, true, w));
        let el = act!(el, SelectDown, |this, w| this.vertical(true, true, w));
        let el = act!(el, WordLeft, |this, _w| this.set_head(this.word_boundary(this.head, false), false, false));
        let el = act!(el, WordRight, |this, _w| this.set_head(this.word_boundary(this.head, true), false, false));
        let el = act!(el, SelectWordLeft, |this, _w| this.set_head(this.word_boundary(this.head, false), true, false));
        let el = act!(el, SelectWordRight, |this, _w| this.set_head(this.word_boundary(this.head, true), true, false));
        let el = act!(el, LineStart, |this, _w| this.set_head(this.line_edge(false), false, false));
        let el = act!(el, LineEnd, |this, _w| this.set_head(this.line_edge(true), false, false));
        let el = act!(el, SelectLineStart, |this, _w| this.set_head(this.line_edge(false), true, false));
        let el = act!(el, SelectLineEnd, |this, _w| this.set_head(this.line_edge(true), true, false));
        let el = act!(el, DocStart, |this, _w| this.set_head(0, false, false));
        let el = act!(el, DocEnd, |this, _w| this.set_head(this.doc.len(), false, false));
        let el = act!(el, SelectDocStart, |this, _w| this.set_head(0, true, false));
        let el = act!(el, SelectDocEnd, |this, _w| this.set_head(this.doc.len(), true, false));
        let el = act!(el, PageUp, |this, w| this.page(false, w));
        let el = act!(el, PageDown, |this, w| this.page(true, w));
        act!(el, SelectAll, |this, _w| {
            this.anchor = 0;
            this.set_head(this.doc.len(), true, false)
        })
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection_is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.doc.slice(self.selection())));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection_is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.doc.slice(self.selection())));
            self.replace_selection("", false, cx);
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
            self.replace_selection(&text, false, cx);
        }
    }

    fn save(&mut self, _: &Save, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(EditorEvent::SaveNow);
    }

    // Mouse ----------------------------------------------------------------

    pub fn hit_test(&self, p: Point<Pixels>) -> usize {
        let painted = self.painted.borrow();
        let hit = painted
            .iter()
            .find(|i| p.y >= i.bounds.top() && p.y < i.bounds.bottom())
            .or_else(|| {
                painted.iter().min_by_key(|i| {
                    let d = if p.y < i.bounds.top() { i.bounds.top() - p.y } else { p.y - i.bounds.bottom() };
                    (f32::from(d) * 100.) as i64
                })
            });
        let Some(item) = hit else { return self.head };
        let start = self.doc.line_start(item.first_line);
        match &item.text {
            None => start,
            Some((d, s)) => {
                let row = s.row_at(p.y - item.bounds.top() - d.top);
                let x = p.x - item.bounds.left() - d.margin - d.indent;
                start + d.disp_to_src(s.index_at(row, x))
            }
        }
    }

    /// The to-do whose box is under `p`, if any.
    fn task_at(&self, p: Point<Pixels>) -> Option<usize> {
        let painted = self.painted.borrow();
        let item = painted
            .iter()
            .find(|i| p.y >= i.bounds.top() && p.y < i.bounds.bottom())?;
        let (d, s) = item.text.as_ref()?;
        if !matches!(d.list, Some(ListGlyph::Task { .. })) {
            return None;
        }
        // The box's frame, a little forgiving, as the Swift hit test has it.
        let text_x = item.bounds.left() + d.margin + d.indent;
        let base = item.bounds.top() + d.top + s.baseline(0);
        let center_y = base - d.font_size * 0.35;
        let x0 = text_x - px(LIST_SLOT) + px(1.);
        (p.x >= x0 - px(4.) && p.x <= x0 + px(20.) && (p.y - center_y).abs() <= px(12.)).then_some(item.first_line)
    }

    /// Flips `[ ]` and `[x]` without moving the caret, as Fumi does.
    fn toggle_task(&mut self, line: usize) {
        let Kind::List { indent, marker } = self.lines[line].kind else { return };
        let at = self.doc.line_start(line) + indent as usize + marker as usize + 1;
        debug_assert!(at < self.doc.len());
        let flipped = if self.doc.slice(at..at + 1) == " " { "x" } else { " " };
        let sel = (self.anchor, self.head);
        self.edit(at..at + 1, flipped, false, false);
        self.anchor = sel.0;
        self.head = sel.1;
        self.caret_moved(false);
    }

    /// The link under `p`: its visible range, its line, its target and its box.
    fn link_at(&self, p: Point<Pixels>) -> Option<(usize, Range<usize>, Link, Bounds<Pixels>)> {
        let painted = self.painted.borrow();
        let item = painted.iter().find(|i| p.y >= i.bounds.top() && p.y < i.bounds.bottom())?;
        let (d, s) = item.text.as_ref()?;
        let text_x = item.bounds.left() + d.margin + d.indent;
        let top = item.bounds.top() + d.top;
        let row_ix = s.row_at(p.y - top);
        let r = s.laid.rows[row_ix];
        for (range, link) in &d.links {
            let (a, e) = (range.start.max(r.start), range.end.min(r.end));
            if a >= e {
                continue;
            }
            // The glyph box, a little forgiving.
            let x0 = text_x + s.laid.x_for_index(a) - r.x0 - px(1.);
            let x1 = text_x + s.laid.x_for_index(e) - r.x0 + px(1.);
            let base = top + r.y + s.baseline(row_ix);
            let (y0, y1) = (base - s.ascent - px(2.), base + s.descent + px(2.));
            if p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1 {
                let anchor = Bounds::from_corners(point(x0, y0), point(x1, y1));
                return Some((item.first_line, range.clone(), link.clone(), anchor));
            }
        }
        None
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if let Some(line) = self.task_at(e.position) {
            self.toggle_task(line);
            self.schedule_after_edit(cx);
            cx.notify();
            return;
        }
        self.pending_link = None;
        if e.click_count == 1
            && !e.modifiers.shift
            && let Some((line, _, link, anchor)) = self.link_at(e.position)
        {
            // Unmodified: opens, unless the caret is already on that line and
            // the editor has focus, when it edits. ⌘ always opens.
            let editing = self.caret_line == line && self.was_focused;
            if e.modifiers.platform || !editing {
                self.pending_link = Some(PendingLink {
                    link,
                    at: e.position,
                    new_tab: e.modifiers.platform,
                    anchor,
                });
                cx.notify();
                return;
            }
        }
        let at = self.hit_test(e.position);
        let line = self.doc.line_of(at);
        match e.click_count {
            2 => {
                let start = self.doc.line_start(line);
                let text = self.doc.line_text(line);
                let off = at - start;
                let word = text
                    .split_word_bound_indices()
                    .find(|(i, w)| off >= *i && off < i + w.len())
                    .map(|(i, w)| start + i..start + i + w.len())
                    .unwrap_or(at..at);
                self.anchor = word.start;
                self.set_head_from_mouse(word.end, true);
            }
            3 => {
                self.anchor = self.doc.line_start(line);
                self.set_head_from_mouse(self.doc.line_end(line), true);
            }
            _ => {
                self.selecting = true;
                self.press_at = e.position;
                self.dragged = false;
                self.set_head_from_mouse(at, e.modifiers.shift);
            }
        }
        cx.notify();
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(pending) = &self.pending_link {
            // Dragging off a link selects instead of opening.
            let moved = e.position - pending.at;
            if f32::from(moved.x).abs() > 3. || f32::from(moved.y).abs() > 3. {
                let at = pending.at;
                self.pending_link = None;
                let head = self.hit_test(at);
                self.selecting = true;
                self.press_at = at;
                self.dragged = true;
                self.set_head_from_mouse(head, false);
            } else {
                return;
            }
        }
        if e.pressed_button.is_none() {
            let hit = self.link_at(e.position);
            let hovered = hit.as_ref().map(|(l, r, _, _)| (*l, r.clone()));
            let web = hit.and_then(|(_, _, link, _)| match link {
                Link::Web(address) => Some(address),
                Link::Wiki(_) => None,
            });
            if hovered != self.hovered_link || web != self.hovered_web {
                self.hovered_link = hovered;
                self.hovered_web = web;
                cx.notify();
            }
        }
        if self.selecting && e.pressed_button == Some(MouseButton::Left) {
            // A click can open a block and move text under a still pointer; a
            // little hand tremor mustn't turn that into a selection.
            let moved = e.position - self.press_at;
            if !self.dragged && f32::from(moved.x).abs() < 4. && f32::from(moved.y).abs() < 4. {
                return;
            }
            self.dragged = true;
            let at = self.hit_test(e.position);
            if at != self.head {
                self.set_head_from_mouse(at, true);
                cx.notify();
            }
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        if let Some(pending) = self.pending_link.take() {
            match pending.link {
                Link::Web(address) => {
                    cx.open_url(&address);
                    cx.emit(EditorEvent::OpenedWeb(address));
                }
                Link::Wiki(inner) => cx.emit(EditorEvent::OpenWiki {
                    inner,
                    new_tab: pending.new_tab,
                    anchor: pending.anchor,
                }),
            }
        }
    }

    // Rendering ------------------------------------------------------------

    /// The language of the code block holding `line`, or the callout type of
    /// the `:::` or `> [!x]` block holding it.
    fn block_type_of(&self, line: usize) -> String {
        let kind = self.lines[line].kind;
        let open = match kind {
            Kind::FenceOpen | Kind::FenceBody | Kind::FenceClose => Kind::FenceOpen,
            Kind::CalloutOpen | Kind::CalloutBody | Kind::CalloutClose => Kind::CalloutOpen,
            Kind::QuoteCalloutTitle | Kind::QuoteCalloutBody => Kind::QuoteCalloutTitle,
            _ => return String::new(),
        };
        let at = (0..=line).rev().find(|l| self.lines[*l].kind == open).unwrap_or(line);
        let text = self.doc.line_text(at);
        match open {
            Kind::FenceOpen => text
                .trim()
                .trim_start_matches(['`', '~'])
                .split(' ')
                .next()
                .unwrap_or("")
                .to_string(),
            Kind::CalloutOpen => text
                .trim()
                .trim_start_matches(':')
                .trim()
                .split(' ')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("note")
                .to_string(),
            _ => callout_type(quote_content(&text)).unwrap_or("note").to_string(),
        }
    }

    /// Whether a `:::` body is inside a closed callout.
    fn callout_closed(&self, line: usize) -> bool {
        (0..=line)
            .rev()
            .find(|l| self.lines[*l].kind == Kind::CalloutOpen)
            .is_some_and(|l| self.lines[l].real)
    }

    pub fn display_of(&self, line: usize, caret: Option<usize>) -> DisplayLine {
        let text = self.doc.line_text(line);
        let info = self.lines[line];
        let kind = info.kind;
        let start = self.doc.line_start(line);
        let marked = self.marked.as_ref().and_then(|m| {
            let (a, b) = (m.start.max(start), m.end.min(start + text.len()));
            (a < b).then(|| a - start..b - start)
        });
        let block_type = self.block_type_of(line);
        let real = match kind {
            Kind::CalloutBody | Kind::CalloutClose => self.callout_closed(line),
            _ => info.real,
        };
        let kind_at = |l: Option<usize>| l.and_then(|l| self.lines.get(l)).map_or(Kind::Blank, |i| i.kind);
        let mut d = build_line(
            &text,
            &LineCtx {
                kind,
                caret,
                title: self.title_line == Some(line),
                marked,
                prev: kind_at(line.checked_sub(1)),
                next: kind_at(Some(line + 1)),
                block_type: &block_type,
                typo: &self.typo,
                links: self.links.as_ref(),
                real,
                raw_block: false,
                prev_has_caret: line > 0 && self.caret_line == line - 1,
            },
        );
        d.margin = self.margin;
        d
    }

    /// Typesets the line's inline math and hands the result to its spans.
    fn fill_math(&mut self, d: &mut DisplayLine, window: &Window) {
        let size = f32::from(d.font_size);
        for span in &mut d.spans {
            if let Some(math) = &mut span.math {
                math.rendered = blocks::inline_math(&math.latex, size, &mut self.blocks, window);
            }
        }
    }

    fn render_item(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.items.get(ix).copied() else {
            return div().into_any_element();
        };
        let line = item.start as usize;
        if item.kind == ItemKind::Line {
            let reveal = line == self.caret_line;
            let autoscroll = reveal && std::mem::take(&mut self.autoscroll_caret);
            let start = self.doc.line_start(line);
            let caret_at = reveal.then(|| self.head.saturating_sub(start));
            let mut built = self.display_of(line, caret_at);
            self.fill_math(&mut built, window);
            let display = Rc::new(built);
            let src_len = display.src_len;
            let display_head = if reveal { display.src_to_disp(self.head - start) } else { 0 };
            let caret = (self.selection_is_empty() && self.caret_on && reveal).then_some(display_head);
            let sel = self.selection();
            let selection = (!sel.is_empty()
                && sel.start <= start + src_len
                && sel.end >= start)
                .then(|| {
                    let a = sel.start.max(start) - start;
                    let b = sel.end.min(start + src_len) - start;
                    (display.src_to_disp(a)..display.src_to_disp(b), sel.end > start + src_len)
                });
            let hovered = self
                .hovered_link
                .as_ref()
                .filter(|(l, _)| *l == line)
                .map(|(_, r)| r.clone());
            let image_below = (reveal && self.lines[line].kind == Kind::Image).then(|| {
                let src = self.doc.line_text(line);
                let img = blocks::image(&src, &self.dir, self.text_width, &mut self.blocks);
                div().w_full().px(self.margin).child(img)
            });
            let element = LineEl {
                line,
                display,
                caret,
                caret_head: if reveal { Some(display_head) } else { None },
                autoscroll,
                selection,
                hovered,
                painted: self.painted.clone(),
                shaped: Rc::new(RefCell::new(None)),
                symbols: self.symbols.clone(),
            };
            // An image stays on the page under its source, as in Fumi, so
            // clicking one doesn't pull everything below it upward.
            return match image_below {
                Some(image) => div().w_full().child(element).child(image).into_any_element(),
                None => element.into_any_element(),
            };
        }

        let range = line..line + item.len as usize;
        let src = self.doc.slice(self.doc.line_start(range.start)..self.doc.line_end(range.end - 1));
        let body: AnyElement = match item.kind {
            ItemKind::Table => blocks::table(&src, self.text_width, &self.typo, &mut self.blocks, window, cx),
            ItemKind::Math => blocks::math(&src, self.text_width, &self.typo, &mut self.blocks, window),
            ItemKind::Front => blocks::frontmatter(&src, self.text_width, &self.typo, window, cx),
            ItemKind::Image => blocks::image(&src, &self.dir, self.text_width, &mut self.blocks),
            ItemKind::Line => unreachable!(),
        };
        let sel = self.selection();
        let start = self.doc.line_start(range.start);
        BlockEl {
            child: div().w_full().px(self.margin).child(body).into_any_element(),
            first_line: range.start,
            n_lines: range.len(),
            selected: !sel.is_empty() && sel.start <= start && sel.end >= start + src.len(),
            painted: self.painted.clone(),
        }
        .into_any_element()
    }

    fn status_text(&self) -> String {
        if let Some(error) = &self.load_error {
            return error.clone();
        }
        let line = self.doc.line_of(self.head);
        format!(
            "Line {} of {}  ·  {} bytes  ·  {} blocks",
            line + 1,
            self.doc.line_count(),
            self.doc.len(),
            self.items.len()
        )
    }
}

/// The text column's cap for Settings › Writing › Width.
pub fn column_cap(setting: &str) -> f32 {
    match setting {
        "narrow" => 600.,
        "wide" => 860.,
        _ => 680.,
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        metrics::frame_begin();
        self.last_shown = self.painted.borrow().len();
        self.painted.borrow_mut().clear();
        let vw = self.pane_width.unwrap_or(window.viewport_size().width);
        let cap = px(column_cap(&self.column_setting));
        let side = ((vw - cap) / 2.).max(px(MIN_SIDE_INSET));
        let column = vw - side * 2.;
        if column != self.text_width || vw != self.list_width {
            self.text_width = column;
            self.list_width = vw;
            self.margin = side;
            // The list forgets every height hint when its width changes, on
            // this frame's layout. Put them back on the next one.
            self.restore_hints = true;
            window.on_next_frame(|window, _| window.refresh());
        } else if std::mem::take(&mut self.restore_hints) {
            let top = self.list.logical_scroll_top();
            self.list.reset_with_uniform_height(self.items.len(), px(ITEM_HINT));
            self.list.scroll_to(top);
        }
        let root = div()
            .id("editor")
            .key_context("Editor")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(t().page_content)
            .text_color(t().text)
            .font(self.typo.body())
            .cursor(if self.hovered_link.is_some() { CursorStyle::PointingHand } else { CursorStyle::IBeam });
        let root = self.register_actions(root, cx);
        self.was_focused = self.focus.is_focused(window);
        self.note_scroll(window, cx);

        let entity = cx.entity();
        let focus = self.focus.clone();
        let thumb = self.scroll_thumb();
        let status = self.show_status.then(|| self.status_text());
        root.on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .child(
                list(self.list.clone(), cx.processor(Self::render_item))
                    .size_full()
                    .pt(px(TOP_PAD))
                    .pb(px(BOTTOM_PAD)),
            )
            .children(self.fade_band())
            .children(thumb.map(|(top, height)| {
                div()
                    .absolute()
                    .right(px(1.5))
                    .top(top + px(2.))
                    .w(px(4.))
                    .h(height - px(4.))
                    .rounded(px(2.))
                    .bg(t().faint_icon.opacity(0.6))
            }))
            .children(self.hovered_web.clone().map(|address| {
                let c = t();
                div()
                    .absolute()
                    .left(px(12.))
                    .bottom(px(12.))
                    .h(px(20.))
                    .max_w(vw - px(24.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(c.inline_bg)
                    .text_size(px(11.))
                    .text_color(c.text2)
                    .font(ui_font())
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(address)
            }))
            .children(status.map(|text| {
                div()
                    .absolute()
                    .bottom(px(8.))
                    .right(px(14.))
                    .text_size(px(11.))
                    .text_color(t().marker)
                    .child(text)
            }))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                        metrics::frame_end();
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

impl Editor {
    /// The soft edge text scrolls under at the page's top. Blur isn't available
    /// here, so this is the Swift band's wash alone; on Glass it would need the
    /// page color, which Glass has none of, so there's no band.
    fn fade_band(&self) -> Option<Div> {
        let c = t();
        if c.glass {
            return None;
        }
        let top = linear_color_stop(c.page_content.opacity(0.8), 0.);
        let bottom = linear_color_stop(c.page_content.opacity(0.), 1.);
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(30.))
                .bg(linear_gradient(180., top, bottom)),
        )
    }

    /// Notes scrolling, for the scroller: it shows while moving, then hides.
    fn note_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let top = self.list.logical_scroll_top();
        let now = (top.item_ix, f32::from(top.offset_in_item));
        if now != self.last_scroll_top {
            self.last_scroll_top = now;
            self.scrolled_at = Some(Instant::now());
            let _ = window;
            cx.spawn(async move |this: WeakEntity<Editor>, cx| {
                cx.background_executor().timer(Duration::from_millis(1100)).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
    }

    fn scroll_thumb(&self) -> Option<(Pixels, Pixels)> {
        let recently = self.scrolled_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1000));
        if !recently && !self.thumb_hover && self.thumb_drag.is_none() {
            return None;
        }
        // By item index, because the list only knows the heights it has laid
        // out and a pixel-based thumb would wander as it learns more.
        let view = self.list.viewport_bounds().size.height;
        let count = self.items.len();
        if view <= px(0.) || count < 2 {
            return None;
        }
        let shown = self.painted.borrow().len().max(self.last_shown).max(1);
        if shown >= count {
            return None;
        }
        let height = (view * (shown as f32 / count as f32)).max(px(28.));
        let at = self.list.logical_scroll_top().item_ix as f32 / (count - shown) as f32;
        Some(((view - height) * at.clamp(0., 1.), height))
    }
}

// The input method path -------------------------------------------------------

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let (a, b) = (self.doc.byte_of_utf16(range.start), self.doc.byte_of_utf16(range.end));
        *adjusted = Some(self.doc.utf16_of(a)..self.doc.utf16_of(b));
        Some(self.doc.slice(a..b))
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let sel = self.selection();
        Some(UTF16Selection {
            range: self.doc.utf16_of(sel.start)..self.doc.utf16_of(sel.end),
            reversed: self.head < self.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|m| self.doc.utf16_of(m.start)..self.doc.utf16_of(m.end))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let composing = self.marked.is_some();
        let range = range
            .map(|r| self.doc.byte_of_utf16(r.start)..self.doc.byte_of_utf16(r.end))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selection());
        self.edit(range, text, !composing, composing);
        // A committed composition is a finished unit for undo.
        if composing {
            self.can_coalesce = false;
        }
        self.schedule_after_edit(cx);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|r| self.doc.byte_of_utf16(r.start)..self.doc.byte_of_utf16(r.end))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selection());
        let at = range.start;
        self.edit(range, text, false, true);
        self.can_coalesce = true;
        self.marked = (!text.is_empty()).then(|| at..at + text.len());
        if let Some(sel) = selected {
            let base = self.doc.utf16_of(at);
            self.anchor = self.doc.byte_of_utf16(base + sel.start);
            self.head = self.doc.byte_of_utf16(base + sel.end);
        }
        self.schedule_after_edit(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // The candidate window follows this rectangle.
        let at = self.doc.byte_of_utf16(range.start);
        let line = self.doc.line_of(at);
        let start = self.doc.line_start(line);
        let painted = self.painted.borrow();
        let item = painted
            .iter()
            .find(|i| line >= i.first_line && line < i.first_line + i.n_lines)?;
        let Some((d, s)) = &item.text else {
            return Some(item.bounds);
        };
        let disp = d.src_to_disp(at - start);
        let row = s.row_of(disp);
        let x = s.x_of(disp, row);
        let r = s.laid.rows[row];
        let origin = item.bounds.origin + point(d.margin + d.indent + x, d.top + r.y);
        Some(Bounds::new(origin, size(px(1.), r.height)))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.doc.utf16_of(self.hit_test(p)))
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.doc.len_utf16())
    }
}

// Whole-document passes -------------------------------------------------------

/// Where two item lists over the same lines differ: old index ranges and how
/// many items replace each, in order.
pub fn fold_changes(old: &[Item], new: &[Item]) -> Vec<(Range<usize>, usize)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            i += 1;
            j += 1;
            continue;
        }
        let (i0, j0) = (i, j);
        loop {
            let (a, b) = (old[i].start, new[j].start);
            if a <= b {
                i += 1;
            }
            if b <= a {
                j += 1;
            }
            if i >= old.len() || j >= new.len() || old[i].start == new[j].start {
                break;
            }
        }
        out.push((i0..i, j - j0));
    }
    if i < old.len() || j < new.len() {
        out.push((i..old.len(), new.len() - j));
    }
    out
}

pub fn build_items(lines: &[LineInfo], revealed: Option<&Range<usize>>) -> Vec<Item> {
    let mut items = Vec::with_capacity(lines.len());
    let mut i = 0;
    let line = |at: usize| Item { start: at as u32, len: 1, kind: ItemKind::Line };
    while i < lines.len() {
        if revealed.is_some_and(|r| r.contains(&i)) {
            items.push(line(i));
            i += 1;
            continue;
        }
        let folded = match lines[i].kind {
            Kind::FrontOpen => {
                let close = lines[i..].iter().position(|l| l.kind == Kind::FrontClose);
                close.map(|c| (c + 1, ItemKind::Front))
            }
            Kind::Image => Some((1, ItemKind::Image)),
            Kind::MathOne => Some((1, ItemKind::Math)),
            Kind::TableHead => {
                let mut n = 1;
                while i + n < lines.len()
                    && matches!(lines[i + n].kind, Kind::TableDelim | Kind::TableRow)
                {
                    n += 1;
                }
                Some((n, ItemKind::Table))
            }
            Kind::MathOpen => {
                let close = lines[i..].iter().position(|l| l.kind == Kind::MathClose);
                close.map(|c| (c + 1, ItemKind::Math))
            }
            _ => None,
        };
        match folded {
            Some((n, kind)) => {
                items.push(Item { start: i as u32, len: n as u32, kind });
                i += n;
            }
            None => {
                items.push(line(i));
                i += 1;
            }
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "---\ntitle: x\n---\n\n# T\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\ntext\n\n$$\nx\n$$\n\n![p](a.png)\n\n```\n| not | table |\n$$\n```\nend";

    #[core::prelude::v1::test]
    fn folds_blocks_and_respects_fences() {
        let lines = scan_all(DOC);
        assert_eq!(lines.len(), DOC.split('\n').count());
        let items = build_items(&lines, None);
        let kinds: Vec<ItemKind> = items.iter().map(|i| i.kind).filter(|k| *k != ItemKind::Line).collect();
        assert_eq!(kinds, [ItemKind::Front, ItemKind::Table, ItemKind::Math, ItemKind::Image]);
        let covered: u32 = items.iter().map(|i| i.len).sum();
        assert_eq!(covered as usize, lines.len());
    }

    #[core::prelude::v1::test]
    fn revealed_block_becomes_lines() {
        let lines = scan_all(DOC);
        let folded = build_items(&lines, None).len();
        let open = build_items(&lines, Some(&(6..9))).len();
        assert_eq!(open, folded + 2);
    }

    #[core::prelude::v1::test]
    fn moving_between_folds_touches_only_those_folds() {
        let lines = scan_all(DOC);
        let in_table = build_items(&lines, Some(&(6..9)));
        let in_math = build_items(&lines, Some(&(12..15)));
        let changes = fold_changes(&in_table, &in_math);
        assert_eq!(changes.len(), 2, "{changes:?}");
        // Applying them in reverse gives the new list's length and leaves the
        // items between the two folds alone.
        let mut len = in_table.len();
        for (range, count) in changes.iter().rev() {
            len = len - range.len() + count;
        }
        assert_eq!(len, in_math.len());
        assert_eq!(changes[0], (item_ix(&in_table, 6)..item_ix(&in_table, 6) + 3, 1));
        assert_eq!(changes[1].1, 3);
        assert_eq!(changes[1].0.len(), 1);
    }

    fn item_ix(items: &[Item], line: u32) -> usize {
        items.iter().position(|i| i.start == line).unwrap()
    }

    #[core::prelude::v1::test]
    fn unclosed_math_is_not_folded() {
        let lines = scan_all("$$\nx\n");
        assert!(build_items(&lines, None).iter().all(|i| i.kind == ItemKind::Line));
    }

}
