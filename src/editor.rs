//! The editor view: source-exact state, incremental block structure, and the
//! virtualized list of items that GPUI's `list` lays out on demand.

use crate::blocks::{self, BlockCache};
use crate::display::*;
use crate::doc::Doc;
use crate::element::*;
use crate::markdown::*;
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
        PageDown, SelectAll, Copy, Cut, Paste, Undo, Redo, Save, CharacterPalette, Quit,
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
        KeyBinding::new("ctrl-cmd-space", CharacterPalette, c),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
}

const TOP_PAD: f32 = 52.;
const BOTTOM_PAD: f32 = 120.;
/// What an item not yet laid out is assumed to measure.
const ITEM_HINT: f32 = 48.;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ItemKind {
    Line,
    Table,
    Math,
    Image,
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
            caret_on: true,
            blink_epoch: 0,
            text_width: px(720.),
            list_width: px(720.),
            margin: px(0.),
            see_through: std::env::var("FUMI_BLUR").is_ok(),
            autoscroll_caret: false,
            last_shown: 0,
            restore_hints: false,
            show_status: true,
            load_error: None,
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
    fn rescan(&mut self, first: usize, removed: usize, added: usize) -> bool {
        let placeholder = LineInfo { kind: Kind::Para, after: Scan::Normal };
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
            let mut info = classify(&text, next.as_deref(), before, i);
            info.kind = table_kind(&text, prev, info.kind);
            let old = self.lines[i];
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

    /// Recomputes items and tells the list only what differs.
    fn rebuild_items(&mut self) {
        let new = build_items(&self.lines, self.revealed.as_ref());
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
            self.remeasure_lines(line..line + 1);
            self.caret_line = line;
        }
        self.caret_on = true;
        self.blink_epoch = 1;
    }

    /// Brings the caret's item into view. `ListState::scroll_to_reveal_item`
    /// works from measured heights and sends us to the wrong place when the
    /// items between here and there were never laid out, so this only trusts
    /// measured neighbours and otherwise jumps by item index.
    fn reveal(&mut self, line: usize) {
        self.autoscroll_caret = true;
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
        self.doc.replace(range.clone(), text);
        let added = text.bytes().filter(|b| *b == b'\n').count();
        let changed = self.rescan(first, removed, added);

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

    fn apply(&mut self, at: usize, old_len: usize, text: &str, sel: (usize, usize)) {
        let first = self.doc.line_of(at);
        let removed = self.doc.line_of(at + old_len) - first;
        self.doc.replace(at..at + old_len, text);
        let added = text.bytes().filter(|b| *b == b'\n').count();
        let changed = self.rescan(first, removed, added);
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
            cx.notify();
        }
    }

    fn redo_op(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(op) = self.redo.pop() {
            self.apply(op.at, op.old.len(), &op.new.clone(), op.sel_after);
            self.undo.push(op);
            cx.notify();
        }
    }

    pub fn replace_selection(&mut self, text: &str, typing: bool, cx: &mut Context<Self>) {
        let range = self.selection();
        self.edit(range, text, typing, false);
        cx.notify();
    }

    fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        let line = self.doc.line_of(self.head);
        let start = self.doc.line_start(line);
        let text = self.doc.line_text(line);
        if let Kind::List { indent, marker } = self.lines[line].kind
            && self.selection_is_empty()
        {
            let (n, m) = (indent as usize, marker as usize);
            if text.len() <= n + m {
                // Enter on an empty item ends the list.
                self.edit(start..start + text.len(), "", false, false);
                cx.notify();
                return;
            }
            let lead = &text[..n + m];
            let next = if lead.as_bytes()[n].is_ascii_digit() {
                let digits: String = lead[n..].chars().take_while(char::is_ascii_digit).collect();
                let value: u64 = digits.parse().unwrap_or(0) + 1;
                format!("{}{value}{}", &lead[..n], &lead[n + digits.len()..])
            } else {
                lead.to_string()
            };
            self.replace_selection(&format!("\n{next}"), false, cx);
            return;
        }
        self.replace_selection("\n", false, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection_is_empty() {
            let at = self.prev_boundary(self.head);
            self.edit(at..self.head, "", false, false);
        } else {
            self.replace_selection("", false, cx);
        }
        cx.notify();
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection_is_empty() {
            let at = self.next_boundary(self.head);
            self.edit(self.head..at, "", false, false);
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

    fn shape_line(&self, line: usize, reveal: bool, window: &Window) -> (Rc<DisplayLine>, Rc<Shaped>) {
        let d = Rc::new(self.display_of(line, reveal));
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
        let (d, s) = self.shape_line(line, true, window);
        let disp = d.src_to_disp(self.head - start);
        let row = s.row_of(disp);
        let x = self.goal_x.unwrap_or(d.indent + s.x_of(disp, row));
        self.goal_x = Some(x);

        let target = if down && row + 1 < s.rows.len() {
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
                let (nd, ns) = self.shape_line(next, false, window);
                let r = if down { 0 } else { ns.rows.len() - 1 };
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

    fn save(&mut self, _: &Save, _: &mut Window, _: &mut Context<Self>) {
        if let Some(path) = &self.path {
            // Written beside the file, then moved over it, so a crash mid-save
            // can't leave a half-written note.
            let tmp = path.with_extension("fumi-tmp");
            if std::fs::write(&tmp, self.doc.to_bytes()).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
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
                let y = p.y - item.bounds.top() - d.top;
                let row = ((f32::from(y) / f32::from(d.line_height)).floor().max(0.) as usize)
                    .min(s.rows.len() - 1);
                let x = p.x - item.bounds.left() - d.margin - d.indent;
                start + d.disp_to_src(s.index_at(row, x))
            }
        }
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
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
                self.set_head(word.end, true, false);
            }
            3 => {
                self.anchor = self.doc.line_start(line);
                self.set_head(self.doc.line_end(line), true, false);
            }
            _ => {
                self.selecting = true;
                self.set_head(at, e.modifiers.shift, false);
            }
        }
        cx.notify();
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting && e.pressed_button == Some(MouseButton::Left) {
            let at = self.hit_test(e.position);
            if at != self.head {
                self.set_head(at, true, false);
                cx.notify();
            }
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    // Rendering ------------------------------------------------------------

    pub fn display_of(&self, line: usize, reveal: bool) -> DisplayLine {
        let text = self.doc.line_text(line);
        let kind = self.lines[line].kind;
        let is_code = |l: usize| {
            self.lines.get(l).is_some_and(|i| {
                matches!(i.kind, Kind::FenceOpen | Kind::FenceBody | Kind::FenceClose)
            })
        };
        let start = self.doc.line_start(line);
        let marked = self.marked.as_ref().and_then(|m| {
            let (a, b) = (m.start.max(start), m.end.min(start + text.len()));
            (a < b).then(|| a - start..b - start)
        });
        let mut d = build_line(
            &text,
            &LineCtx {
                kind,
                reveal,
                title: self.title_line == Some(line),
                marked,
                prev_is_code: line > 0 && is_code(line - 1),
                next_is_code: is_code(line + 1),
            },
        );
        d.margin = self.margin;
        d
    }

    fn render_item(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.items.get(ix).copied() else {
            return div().into_any_element();
        };
        let line = item.start as usize;
        if item.kind == ItemKind::Line {
            let reveal = line == self.caret_line;
            let autoscroll = reveal && std::mem::take(&mut self.autoscroll_caret);
            let display = Rc::new(self.display_of(line, reveal));
            let start = self.doc.line_start(line);
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
            return LineEl {
                line,
                display,
                caret,
                caret_head: if reveal { Some(display_head) } else { None },
                autoscroll,
                selection,
                painted: self.painted.clone(),
                shaped: Rc::new(RefCell::new(None)),
            }
            .into_any_element();
        }

        let range = line..line + item.len as usize;
        let src = self.doc.slice(self.doc.line_start(range.start)..self.doc.line_end(range.end - 1));
        let body: AnyElement = match item.kind {
            ItemKind::Table => blocks::table(&src, self.text_width, &mut self.blocks, window, cx),
            ItemKind::Math => blocks::math(&src, &mut self.blocks, window),
            ItemKind::Image => blocks::image(&src, &self.dir, self.text_width, &mut self.blocks),
            ItemKind::Line => unreachable!(),
        };
        let painted = self.painted.clone();
        let sel = self.selection();
        let start = self.doc.line_start(range.start);
        let selected = !sel.is_empty() && sel.start <= start && sel.end >= start + src.len();
        div()
            .relative()
            .w_full()
            .px(self.margin)
            .child(body)
            .when(selected, |d| {
                d.child(div().absolute().inset_0().bg(selection()))
            })
            .child(
                canvas(
                    move |bounds, _, _| {
                        painted.borrow_mut().push(Painted {
                            first_line: range.start,
                            n_lines: range.len(),
                            bounds,
                            text: None,
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
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
        let vw = window.viewport_size().width;
        let column = px(720.).min(vw - px(96.));
        let side = (vw - column) / 2.;
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
            .bg(if self.see_through { hsla(0., 0., 1., 0.55) } else { hsla(0., 0., 1., 1.) })
            .text_color(ink())
            .font(body_font())
            .cursor(CursorStyle::IBeam);
        let root = self.register_actions(root, cx);

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
            .children(thumb.map(|(top, height)| {
                div()
                    .absolute()
                    .right(px(3.))
                    .top(top)
                    .w(px(5.))
                    .h(height)
                    .rounded(px(3.))
                    .bg(rgb(0xDDDCD7))
            }))
            .children(status.map(|text| {
                div()
                    .absolute()
                    .bottom(px(8.))
                    .right(px(14.))
                    .text_size(px(11.))
                    .text_color(marker_gray())
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
    fn scroll_thumb(&self) -> Option<(Pixels, Pixels)> {
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
        let height = (view * (shown as f32 / count as f32)).max(px(24.));
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
        let origin = item.bounds.origin + point(d.margin + d.indent + x, d.top + d.line_height * row as f32);
        Some(Bounds::new(origin, size(px(1.), d.line_height)))
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

pub fn scan_all(text: &str) -> Vec<LineInfo> {
    let mut out = Vec::with_capacity(text.len() / 40 + 1);
    let mut before = Scan::Normal;
    let mut prev = Kind::Blank;
    let mut rest = text.split('\n').peekable();
    let mut i = 0;
    while let Some(raw) = rest.next() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let next = rest.peek().map(|n| n.strip_suffix('\r').unwrap_or(n));
        let mut info = classify(line, next, before, i);
        info.kind = table_kind(line, prev, info.kind);
        out.push(info);
        before = info.after;
        prev = info.kind;
        i += 1;
    }
    out
}

pub fn find_title(lines: &[LineInfo]) -> Option<usize> {
    for (i, l) in lines.iter().enumerate() {
        match l.kind {
            Kind::Blank | Kind::FrontOpen | Kind::FrontBody | Kind::FrontClose => continue,
            Kind::Heading(1) => return Some(i),
            _ => return None,
        }
    }
    None
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
        assert_eq!(kinds, [ItemKind::Table, ItemKind::Math, ItemKind::Image]);
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
    fn unclosed_math_is_not_folded() {
        let lines = scan_all("$$\nx\n");
        assert!(build_items(&lines, None).iter().all(|i| i.kind == ItemKind::Line));
    }

    #[core::prelude::v1::test]
    fn crlf_tables_classify() {
        let lines = scan_all("| a |\r\n| - |\r\n| 1 |\r\n");
        assert_eq!(lines[0].kind, Kind::TableHead);
        assert_eq!(lines[1].kind, Kind::TableDelim);
        assert_eq!(lines[2].kind, Kind::TableRow);
    }
}
