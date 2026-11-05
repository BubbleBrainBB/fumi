//! The editor view: source-exact state, incremental block structure, and the
//! virtualized list of items that GPUI's `list` lays out on demand.

use crate::blocks::{self, BlockCache};
use fumi_core::markdown::{find_title, scan_all};
use fumi_ui::native::SymbolCache;
use fumi_ui::theme::t;
use crate::display::{self, *};
use crate::spell;
use fumi_core::doc::Doc;
use crate::element::*;
use fumi_core::markdown::*;
use crate::media::{self, Lookup, MediaContext, MediaKind, MediaLookup};
use crate::metrics;
use crate::outline::{self, OutlineHeading, RailState};
use crate::paste;
use crate::popover::{self, CreateOffer, OfferCancel, OfferCreate};
use crate::slash::{self, SlashAccept, SlashClose, SlashDown, SlashMenu, SlashUp};
use crate::words;
use crate::video::{self, Control, VideoPlayer, VideoUi};
use fumi_core::attachment_import;
use fumi_core::wiki_link::WikiLink;
use gpui::prelude::*;
use gpui::*;
use std::cell::RefCell;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::cell::Cell;
use unicode_segmentation::UnicodeSegmentation;

actions!(
    editor,
    [
        Backspace, Delete, Enter, Left, Right, Up, Down, SelectLeft, SelectRight, SelectUp,
        SelectDown, WordLeft, WordRight, SelectWordLeft, SelectWordRight, LineStart, LineEnd,
        SelectLineStart, SelectLineEnd, DocStart, DocEnd, SelectDocStart, SelectDocEnd, PageUp,
        PageDown, SelectAll, Copy, Cut, Paste, Undo, Redo, Save, CharacterPalette, Quit, Bold, Italic, InsertLink, ToggleTaskLine,
        ParagraphStart, ParagraphEnd, ScrollToStart, ScrollToEnd, DeleteWordLeft, DeleteWordRight, DeleteToRowStart,
        DeleteToParagraphEnd,
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
        KeyBinding::new("ctrl-a", ParagraphStart, c),
        KeyBinding::new("ctrl-e", ParagraphEnd, c),
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
        // What NSTextView has on these keys.
        KeyBinding::new("home", ScrollToStart, c),
        KeyBinding::new("end", ScrollToEnd, c),
        KeyBinding::new("alt-backspace", DeleteWordLeft, c),
        KeyBinding::new("alt-delete", DeleteWordRight, c),
        KeyBinding::new("cmd-backspace", DeleteToRowStart, c),
        KeyBinding::new("ctrl-k", DeleteToParagraphEnd, c),
        KeyBinding::new("ctrl-h", Backspace, c),
        KeyBinding::new("ctrl-d", Delete, c),
        KeyBinding::new("ctrl-b", Left, c),
        KeyBinding::new("ctrl-f", Right, c),
        KeyBinding::new("ctrl-p", Up, c),
        KeyBinding::new("ctrl-n", Down, c),
        // Bound last, so they come before the plain keys while the menu's open. A
        // key the menu has no use for just goes on to the plain binding.
        KeyBinding::new("up", SlashUp, Some("Editor && slash_menu")),
        KeyBinding::new("ctrl-p", SlashUp, Some("Editor && slash_menu")),
        KeyBinding::new("down", SlashDown, Some("Editor && slash_menu")),
        KeyBinding::new("ctrl-n", SlashDown, Some("Editor && slash_menu")),
        KeyBinding::new("enter", SlashAccept, Some("Editor && slash_menu")),
        KeyBinding::new("tab", SlashAccept, Some("Editor && slash_menu")),
        KeyBinding::new("escape", SlashClose, Some("Editor && slash_menu")),
        // The offer to create a linked note holds the focus while it's up.
        KeyBinding::new("escape", OfferCancel, Some("CreateNoteOffer")),
        KeyBinding::new("enter", OfferCreate, Some("CreateNoteOffer")),
    ]);
}

/// The note's first and last items carry this much room themselves. As list
/// padding it would hold all text 30 pt below the page's top at every scroll
/// position, rather than just at the top of the note.
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
    /// The line drawn with its syntax showing, and the offset that shows it. They
    /// stay put while a selection is made or a press is held, as only a caret
    /// settling somewhere restyles anything.
    caret_line: usize,
    reveal_head: usize,
    /// A press is held: where the caret landed shows once it lets go.
    hold_reveal: bool,
    /// The line the head is on, as of this frame.
    head_line: usize,
    /// This move is the caret going somewhere by itself: its line keeps its height
    /// in the viewport through the restyle.
    keep_wanted: bool,
    /// A line and where its caret row must stand, once layout has run: next
    /// frame's, and this frame's.
    keep_next: Option<(usize, Pixels)>,
    keep_now: Option<(usize, Pixels)>,
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
    /// The line whose element should bring the head into view.
    autoscroll_line: Option<usize>,
    restore_hints: bool,
    pub show_status: bool,
    pub load_error: Option<String>,
    pub typo: Typography,
    pub links: Option<LinkContext>,
    pub symbols: Rc<RefCell<SymbolCache>>,
    hovered_link: Option<(usize, Range<usize>)>,
    pub pane_width: Option<Pixels>,
    /// The list's viewport as of this frame's start. Items are laid out while
    /// the list holds its own state, so they can't ask it.
    viewport_cache: Bounds<Pixels>,
    pub column_setting: String,
    /// `fumi.spellCheck` and `fumi.smartPunctuation`, as of the last settings change.
    spell_check: bool,
    smart_punctuation: bool,
    pending_link: Option<PendingLink>,
    was_focused: bool,
    /// Only the key window's focused editor draws a caret.
    caret_shows: bool,
    /// Set by a host that draws the caret on its own layer; see `CaretSlot`.
    pub caret_slot: Option<Rc<CaretSlot>>,
    /// Redraws the host's caret layer on a blink.
    pub on_blink: Option<Rc<dyn Fn(&mut App)>>,
    hovered_web: Option<String>,
    edit_epoch: u64,
    scrolled_at: Option<Instant>,
    last_scroll_top: (usize, f32),
    thumb_hover: bool,
    /// While the thumb's held: how far down it the pointer took it.
    thumb_drag: Option<Pixels>,
    /// False when the doc was replaced from outside and has no unsaved change.
    pub dirty: bool,
    /// The library this note is in: where relative paths and `![[embeds]]` are
    /// also looked for, and where pasted media goes. None: media can't be pasted.
    pub library: Option<PathBuf>,
    /// Something finished loading since the last frame.
    media_dirty: bool,
    /// Lines drawn while something on them was still loading.
    media_wait: Vec<usize>,
    videos: Rc<RefCell<VideoUi>>,
    video_drag: Option<VideoDrag>,
    over_video: bool,
    /// The moment the timer for the next animated-image frame is set for.
    frame_timer: Rc<std::cell::Cell<Option<Instant>>>,
    /// The slash menu, while it's open.
    pub(crate) slash: Option<SlashMenu>,
    /// An input method is mid-edit: the query waits until the edit's done.
    slash_hold: bool,
    /// Where its slash is drawn, as of this frame's layout.
    pub(crate) slash_box: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The headings, as of when typing last paused.
    pub outline: Rc<Vec<OutlineHeading>>,
    pub(crate) outline_epoch: u64,
    pub(crate) rail: RailState,
    pub pane_height: Option<Pixels>,
    pub(crate) offer: Option<CreateOffer>,
    pub(crate) offer_focus: FocusHandle,
    /// A draft's faint title and hint show while it's untouched.
    pub draft_placeholder: bool,
}

/// A press on one of a video's sliders, until it lets go.
struct VideoDrag {
    player: usize,
    control: Control,
    track: Bounds<Pixels>,
}

/// What a press on a picture means.
enum MediaHit {
    Image { caret: usize },
    Video { line: usize, path: PathBuf, area: Bounds<Pixels> },
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
    /// A link to a note that doesn't exist, which the user chose to create.
    CreateNote { name: String, new_tab: bool },
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
            reveal_head: 0,
            hold_reveal: false,
            head_line: 0,
            keep_wanted: false,
            keep_next: None,
            keep_now: None,
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
            autoscroll_line: None,
            restore_hints: false,
            show_status: false,
            load_error: None,
            typo: Typography::from_settings(),
            links: None,
            symbols: Rc::new(RefCell::new(SymbolCache::default())),
            hovered_link: None,
            pane_width: None,
            viewport_cache: Bounds::default(),
            column_setting: fumi_ui::settings::editor_width(),
            spell_check: fumi_ui::settings::spell_check(),
            smart_punctuation: fumi_ui::settings::smart_punctuation(),
            pending_link: None,
            was_focused: false,
            caret_shows: false,
            caret_slot: None,
            on_blink: None,
            hovered_web: None,
            edit_epoch: 0,
            scrolled_at: None,
            last_scroll_top: (0, 0.),
            thumb_hover: false,
            thumb_drag: None,
            dirty: false,
            library: None,
            media_dirty: false,
            media_wait: Vec::new(),
            videos: Rc::default(),
            video_drag: None,
            over_video: false,
            frame_timer: Rc::default(),
            slash: None,
            slash_hold: false,
            slash_box: Rc::default(),
            outline: Rc::default(),
            outline_epoch: 0,
            rail: RailState::default(),
            pane_height: None,
            offer: None,
            offer_focus: cx.focus_handle(),
            draft_placeholder: false,
        };
        let weak = cx.weak_entity();
        media::with_store(|s| s.add_listener(weak));
        this.load_bytes(bytes);
        if blink {
            cx.spawn(async move |this: WeakEntity<Editor>, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(530)).await;
                    let alive = this.update(cx, |e, cx| {
                        // Only a caret that shows blinks; an editor without focus
                        // stays still and costs nothing.
                        if e.selection_is_empty() && e.blink_epoch == 0 && e.caret_shows {
                            match (&e.caret_slot, &e.on_blink) {
                                (Some(slot), Some(redraw)) => {
                                    slot.on.set(!slot.on.get());
                                    let redraw = redraw.clone();
                                    redraw(cx);
                                }
                                _ => {
                                    e.caret_on = !e.caret_on;
                                    cx.notify();
                                }
                            }
                        } else if !e.caret_shows {
                            e.caret_on = true;
                            if let Some(slot) = &e.caret_slot {
                                slot.on.set(true);
                            }
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
                self.outline = Rc::new(outline::outline(text));
                self.load_error = None;
            }
            Err(e) => {
                // Opening it would mean changing it. Say so and leave it alone.
                self.doc = Doc::from_bytes(b"").unwrap();
                self.lines = scan_all("");
                self.path = None;
                self.outline = Rc::default();
                self.load_error = Some(format!("Not UTF-8, not opened: {e}"));
            }
        }
        self.anchor = 0;
        self.head = 0;
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.caret_line = 0;
        self.reveal_head = 0;
        self.slash = None;
        self.videos.borrow_mut().players.clear();
        self.media_wait.clear();
        self.video_drag = None;
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
            Kind::MathOne => Some(line..line + 1),
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
        // The last item has the page's foot in it, and a new one takes it over.
        if self.items.len() != new.len() && new.len() >= 2 {
            let at = new.len().min(self.items.len()) - 1;
            self.list.remeasure_items(at..(at + 2).min(new.len()));
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
    /// have the two lines whose syntax changed visibility measured again. Making a
    /// selection changes none of that; nor does a press, until it lets go.
    pub(crate) fn caret_moved(&mut self, scroll: bool) {
        metrics::note_change();
        self.caret_on = true;
        self.blink_epoch = 1;
        if let Some(slot) = &self.caret_slot {
            slot.on.set(true);
        }
        let keep_wanted = std::mem::take(&mut self.keep_wanted);
        if !self.slash_hold {
            self.refresh_slash();
        }
        let line = self.doc.line_of(self.head);
        if !self.selection_is_empty() || self.hold_reveal {
            if scroll {
                self.reveal(line);
            }
            return;
        }
        let prev = self.caret_line;
        let block = self.foldable_block_at(line);
        if line != prev || block != self.revealed {
            self.clear_link_hover();
            // Before anything moves: where the line it landed on stands now.
            if keep_wanted {
                self.keep_next = self.row_top_of(self.head).map(|y| (line, y));
            }
        }
        self.reveal_head = self.head;
        self.close_revealed_players(line);
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
    }

    /// The window height of the row holding `at`, if it's on the page now.
    fn row_top_of(&self, at: usize) -> Option<Pixels> {
        let line = self.doc.line_of(at);
        let painted = self.painted.borrow();
        let item = painted.iter().find(|i| line >= i.first_line && line < i.first_line + i.n_lines)?;
        let y = match &item.text {
            Some((d, s)) => {
                let start = self.doc.line_start(line);
                let row = s.row_of(d.src_to_disp(at.saturating_sub(start)));
                item.bounds.top() + d.top + s.laid.rows[row].y
            }
            None => item.bounds.top(),
        };
        let view = self.list.viewport_bounds();
        (y >= view.top() && y <= view.bottom()).then_some(y)
    }

    fn clear_link_hover(&mut self) {
        self.hovered_link = None;
        self.hovered_web = None;
    }

    /// Brings the caret's item into view. `ListState::scroll_to_reveal_item`
    /// works from measured heights and sends us to the wrong place when the
    /// items between here and there were never laid out, so this only trusts
    /// measured neighbours and otherwise jumps by item index.
    fn reveal(&mut self, line: usize) {
        // Inside a folded block there's no line element to do it.
        self.autoscroll_line = (self.items.get(self.item_of(line)).is_some_and(|i| i.kind == ItemKind::Line)).then_some(line);
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
                .map(|b| b.bottom() + px(30.) * k as f32)
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
        self.media_lines_moved(first, removed, added);

        let caret = range.start + text.len();
        self.anchor = caret;
        self.head = caret;
        self.goal_x = None;
        self.marked = None;
        self.hold_reveal = false;
        self.clear_link_hover();

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
                        e.refresh_outline(cx);
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
    /// scroll where they were as nearly as the change allows, as one undoable
    /// edit. Only what differs is touched. Refused (false) while an input method
    /// is composing, which a replacement would break.
    pub fn replace_from_outside(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        if self.marked.is_some() {
            return false;
        }
        let old = self.doc.rope.to_string();
        let sel_before = (self.anchor, self.head);
        let Some(op) = outside_op(&old, text, sel_before) else {
            self.dirty = false;
            return true;
        };
        self.close_slash();
        // The caret's line stays where it is in the window.
        let keep = self.row_top_of(self.head);
        self.apply(op.at, op.old.len(), &op.new, op.sel_after);
        if self.selection_is_empty() {
            self.keep_next = keep.map(|y| (self.doc.line_of(self.head), y));
        }
        record_outside(&mut self.undo, &mut self.redo, op);
        self.dirty = false;
        self.refresh_outline(cx);
        cx.notify();
        true
    }

    /// After Settings changed fonts, sizes or the theme: drops what baked the old
    /// look in and measures everything again.
    pub fn settings_changed(&mut self, typo: Typography, cx: &mut Context<Self>) {
        self.typo = typo;
        self.column_setting = fumi_ui::settings::editor_width();
        self.spell_check = fumi_ui::settings::spell_check();
        self.smart_punctuation = fumi_ui::settings::smart_punctuation();
        self.blocks.clear();
        let top = self.list.logical_scroll_top();
        self.list.reset_with_uniform_height(self.items.len(), px(ITEM_HINT));
        self.list.scroll_to(top);
        cx.notify();
    }

    /// The link index changed: shown link text may differ, and with it the
    /// height of any line, so everything is measured again.
    pub fn links_changed(&mut self, cx: &mut Context<Self>) {
        let top = self.list.logical_scroll_top();
        self.list.reset_with_uniform_height(self.items.len(), px(ITEM_HINT));
        self.list.scroll_to(top);
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

    /// Puts the note back where it was left, before anything is drawn.
    pub fn restore_view(&mut self, state: ViewState) {
        let n = self.doc.len();
        self.anchor = self.doc.floor_boundary(state.anchor.min(n));
        self.head = self.doc.floor_boundary(state.head.min(n));
        self.caret_moved(false);
        self.list.scroll_to(ListOffset {
            item_ix: state.scroll_item,
            offset_in_item: px(state.scroll_offset),
        });
    }

    /// The tab showing this note was left: what floats over the page goes with it.
    pub fn left_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_slash();
        self.clear_link_hover();
        self.rail.close();
        if self.offer.take().is_some() {
            window.focus(&self.focus, cx);
        }
        self.selecting = false;
        self.hold_reveal = false;
    }

    /// A draft takes focus with the caret at its end.
    pub fn caret_to_end(&mut self) {
        let end = self.doc.len();
        self.anchor = end;
        self.head = end;
        self.caret_moved(false);
    }

    pub fn text(&self) -> String {
        self.doc.rope.to_string()
    }

    /// Scrolls a line to the top, with 12 pt above it, wherever that takes the
    /// list: to the item now, to the exact height once its neighbours are measured.
    pub fn scroll_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let ix = self.item_of(line);
        self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
        let view = self.list.viewport_bounds();
        self.keep_next = Some((line, view.top() + px(12.)));
        cx.notify();
    }

    /// Opened at `[[Note#Heading]]`: to that heading, whatever its case.
    pub fn scroll_to_heading_named(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(line) = outline::find_heading(&self.doc.rope.to_string(), name) {
            self.scroll_to_line(line, cx);
        }
    }

    fn apply(&mut self, at: usize, old_len: usize, text: &str, sel: (usize, usize)) {
        let first = self.doc.line_of(at);
        let removed = self.doc.line_of(at + old_len) - first;
        let sensitive = self.edit_is_sensitive(&(at..at + old_len), text);
        self.doc.replace(at..at + old_len, text);
        let added = text.bytes().filter(|b| *b == b'\n').count();
        let changed = self.rescan(first, removed, added, sensitive);
        self.media_lines_moved(first, removed, added);
        self.anchor = sel.0;
        self.head = sel.1;
        self.marked = None;
        self.caret_line = self.caret_line.min(self.lines.len().saturating_sub(1));
        self.reveal_head = self.reveal_head.min(self.doc.len());
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

    /// Option-arrow: to the end of the next word or the start of the last one,
    /// past lines with none, as the system's tokenizer sees words.
    fn word_boundary(&self, at: usize, forward: bool) -> usize {
        let mut line = self.doc.line_of(at);
        let mut off = at - self.doc.line_start(line);
        loop {
            let found = {
                let tokens = words::words(&self.doc.line_text(line));
                if forward { words::end_after(&tokens, off) } else { words::start_before(&tokens, off) }
            };
            if let Some(found) = found {
                return self.doc.line_start(line) + found;
            }
            if forward {
                if line + 1 >= self.doc.line_count() {
                    return self.doc.len();
                }
                line += 1;
                off = 0;
            } else {
                if line == 0 {
                    return 0;
                }
                line -= 1;
                off = usize::MAX;
            }
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
        // Up into the properties panel opens it for editing, rather than leaving
        // the caret at its foot.
        if head == 0 && !extend && self.head != 0 && self.has_folded_panel() {
            self.open_frontmatter();
            return;
        }
        self.head = head;
        if !extend {
            self.anchor = head;
        }
        if !keep_goal {
            self.goal_x = None;
        }
        self.can_coalesce = false;
        self.marked = None;
        self.keep_wanted = true;
        self.caret_moved(true);
    }

    /// What of `line` shows as source: the caret's offset, if it's the caret's.
    fn reveal_offset(&self, line: usize) -> Option<usize> {
        (line == self.caret_line).then(|| self.reveal_head.saturating_sub(self.doc.line_start(line)))
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
        let (d, s) = self.shape_line(line, self.reveal_offset(line), window);
        let disp = d.src_to_disp(self.head - start);
        let row = s.row_of(disp);
        let x = self.goal_x.unwrap_or(d.indent + s.x_of(disp, row));
        self.goal_x = Some(x);

        // A revealed image has a row of its own and no text to land on: arrows
        // pass over it, to the next text or the next line.
        let rows = &s.laid.rows;
        let mut next_row = if down { row + 1 } else { row.wrapping_sub(1) };
        while rows.get(next_row).is_some_and(|r| r.fill) {
            next_row = if down { next_row + 1 } else { next_row.wrapping_sub(1) };
        }
        let target = if next_row < s.row_count() {
            start + d.disp_to_src(s.index_at(next_row, x - d.indent))
        } else if down && line + 1 >= self.doc.line_count() {
            self.doc.len()
        } else if !down && line == 0 {
            0
        } else {
            let next = if down { line + 1 } else { line - 1 };
            let item = self.items[self.item_of(next)];
            if item.kind == ItemKind::Front {
                0
            } else if item.kind != ItemKind::Line {
                // A folded block opens on the near line, source visible.
                self.doc.line_start(next)
            } else {
                let (nd, ns) = self.shape_line(next, self.reveal_offset(next), window);
                let r = if down { 0 } else { ns.row_count() - 1 };
                self.doc.line_start(next) + nd.disp_to_src(ns.index_at(r, x - nd.indent))
            }
        };
        self.set_head(target, extend, true);
    }

    /// Page Up and Down scroll and leave the caret, as NSTextView's do.
    fn page(&mut self, down: bool, _: &mut Window) {
        let h = self.list.viewport_bounds().size.height - px(10.);
        let delta = if down { h } else { -h };
        self.list.scroll_by(delta);
    }

    fn paragraph_edge(&self, end: bool) -> usize {
        let line = self.doc.line_of(self.head);
        if end { self.doc.line_end(line) } else { self.doc.line_start(line) }
    }

    /// ⌘← and ⌘→: the ends of the row the caret is on, on a wrapped line too.
    fn row_edge(&self, end: bool, window: &Window) -> usize {
        let line = self.doc.line_of(self.head);
        let start = self.doc.line_start(line);
        let (d, s) = self.shape_line(line, self.reveal_offset(line), window);
        let row = s.row_of(d.src_to_disp(self.head - start));
        let r = s.laid.rows[row];
        let disp = if !end {
            r.start
        } else if row + 1 == s.laid.rows.len() {
            r.end
        } else {
            // Its end is the next row's start, so stop before the last character.
            s.laid.last_index_before(r.end).max(r.start)
        };
        start + d.disp_to_src(disp)
    }

    fn delete_to(&mut self, to: usize, cx: &mut Context<Self>) {
        if !self.selection_is_empty() {
            self.replace_selection("", false, cx);
            return;
        }
        let range = self.head.min(to)..self.head.max(to);
        if !range.is_empty() {
            self.edit(range, "", false, false);
            self.schedule_after_edit(cx);
        }
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
            .on_action(cx.listener(|_, _: &CharacterPalette, window, _| window.show_character_palette()))
            .on_action(cx.listener(Self::slash_up))
            .on_action(cx.listener(Self::slash_down))
            .on_action(cx.listener(Self::slash_accept))
            .on_action(cx.listener(Self::slash_close));
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
        let el = act!(el, LineStart, |this, w| this.set_head(this.row_edge(false, w), false, false));
        let el = act!(el, LineEnd, |this, w| this.set_head(this.row_edge(true, w), false, false));
        let el = act!(el, SelectLineStart, |this, w| this.set_head(this.row_edge(false, w), true, false));
        let el = act!(el, SelectLineEnd, |this, w| this.set_head(this.row_edge(true, w), true, false));
        let el = act!(el, ParagraphStart, |this, _w| this.set_head(this.paragraph_edge(false), false, false));
        let el = act!(el, ParagraphEnd, |this, _w| this.set_head(this.paragraph_edge(true), false, false));
        let el = act!(el, ScrollToStart, |this, _w| this.list.scroll_to(ListOffset { item_ix: 0, offset_in_item: px(0.) }));
        let el = act!(el, ScrollToEnd, |this, _w| this.list.scroll_to(ListOffset { item_ix: this.items.len(), offset_in_item: px(0.) }));
        let el = el
            .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| {
                let to = this.word_boundary(this.head, false);
                this.delete_to(to, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &DeleteWordRight, _, cx| {
                let to = this.word_boundary(this.head, true);
                this.delete_to(to, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &DeleteToRowStart, window, cx| {
                let to = this.row_edge(false, window);
                this.delete_to(to, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &DeleteToParagraphEnd, _, cx| {
                let end = this.paragraph_edge(true);
                let to = if end == this.head { this.next_boundary(end) } else { end };
                this.delete_to(to, cx);
                cx.notify();
            }));
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
        if self.paste_media(cx) {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
            self.replace_selection(&text, false, cx);
        }
    }

    /// Files that are images or videos, or else image data, become embeds.
    fn paste_media(&mut self, cx: &mut Context<Self>) -> bool {
        if self.library.is_none() {
            return false;
        }
        self.paste_board(paste::read_general(), cx)
    }

    /// Imports what a pasteboard held and links it in. False if there's nothing
    /// for it to do, and the paste is an ordinary one.
    pub fn paste_board(&mut self, board: paste::PastedMedia, cx: &mut Context<Self>) -> bool {
        let Some(library) = self.library.clone() else { return false };
        let imported: Vec<PathBuf> = if board.has_files {
            board
                .files
                .iter()
                .filter(|f| attachment_import::is_media(f))
                .filter_map(|f| attachment_import::import_file(f, &library).ok())
                .collect()
        } else if let Some((data, ext)) = board.image {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0., |d| d.as_secs_f64());
            attachment_import::save_image(&data, ext, &library, now, paste::utc_offset(now as i64))
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        self.insert_imported(&imported, self.selection(), cx)
    }

    /// Links files now in the library, each on a line of its own.
    pub(crate) fn insert_imported(&mut self, imported: &[PathBuf], range: Range<usize>, cx: &mut Context<Self>) -> bool {
        let Some(library) = &self.library else { return false };
        if imported.is_empty() {
            return false;
        }
        let folder = if self.dir.as_os_str().is_empty() { library.clone() } else { self.dir.clone() };
        let links: Vec<String> = imported
            .iter()
            .map(|file| attachment_import::markdown_for_path(&attachment_import::relative_path(&folder, file)))
            .collect();
        let before = (range.start > 0)
            .then(|| self.doc.slice(self.prev_boundary(range.start)..range.start).bytes().last())
            .flatten();
        let after = (range.end < self.doc.len())
            .then(|| self.doc.slice(range.end..self.next_boundary(range.end)).bytes().next())
            .flatten();
        let text = paste::embed_text(&links, before, after);
        self.edit(range, &text, false, false);
        self.schedule_after_edit(cx);
        cx.notify();
        true
    }

    /// Image and video files dropped on the page go in at the character nearest
    /// the pointer; other drags have nothing to do here.
    pub fn drop_files(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else { return };
        let imported: Vec<PathBuf> = paths
            .paths()
            .iter()
            .filter(|f| attachment_import::is_media(f))
            .filter_map(|f| attachment_import::import_file(f, &library).ok())
            .collect();
        if imported.is_empty() {
            return;
        }
        let at = self.hit_test(window.mouse_position());
        window.focus(&self.focus, cx);
        self.insert_imported(&imported, at..at, cx);
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
            // A block's right half is its trailing edge, as a caret placed
            // beside an attachment has it.
            None if self.items[self.item_of(item.first_line)].kind != ItemKind::Front
                && p.x > item.bounds.center().x =>
            {
                self.doc.line_end(item.first_line + item.n_lines - 1)
            }
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
        self.close_slash();
        window.focus(&self.focus, cx);
        if self.press_scroller(e.position, cx) {
            return;
        }
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
        if let Some(hit) = self.media_at(e.position) {
            match hit {
                MediaHit::Video { line, path, area } => {
                    self.press_video(line, &path, area, e.position);
                    cx.notify();
                    return;
                }
                // Not selected: the caret goes to the end of its source, which
                // shows the source above it.
                MediaHit::Image { caret } if e.click_count == 1 && !e.modifiers.shift => {
                    self.keep_wanted = true;
                    self.set_head_from_mouse(caret, false);
                    cx.notify();
                    return;
                }
                MediaHit::Image { .. } => {}
            }
        }
        let at = self.hit_test(e.position);
        if at == 0 && e.click_count == 1 && !e.modifiers.shift && self.has_folded_panel() {
            self.open_frontmatter();
            cx.notify();
            return;
        }
        let line = self.doc.line_of(at);
        match e.click_count {
            2 => {
                let start = self.doc.line_start(line);
                let text = self.doc.line_text(line);
                let off = at - start;
                let word = words::selection_at(&text, off).map_or(at..at, |w| start + w.start..start + w.end);
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
                self.hold_reveal = true;
                self.set_head_from_mouse(at, e.modifiers.shift);
            }
        }
        cx.notify();
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.video_drag.is_some() {
            if e.pressed_button == Some(MouseButton::Left) {
                self.drag_video(e.position, false);
                cx.notify();
            }
            return;
        }
        self.track_video_pointer(e.position, cx);
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
                self.hold_reveal = true;
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
            self.track_scroller(e.position, cx);
        }
    }

    /// The pointer moving with the button down, wherever it is by now: this runs
    /// for as long as a press is held on the scroller or in the text.
    fn drag_move(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        if self.thumb_drag.is_some() {
            self.drag_scroller(p, cx);
            return;
        }
        if !self.selecting {
            return;
        }
        let moved = p - self.press_at;
        if !self.dragged && f32::from(moved.x).abs() < 4. && f32::from(moved.y).abs() < 4. {
            return;
        }
        self.dragged = true;
        let at = self.hit_test(p);
        if at != self.head {
            self.set_head_from_mouse(at, true);
            cx.notify();
        }
    }

    /// A press lets go: the caret, if that's what it left, shows where it landed.
    fn release(&mut self, cx: &mut Context<Self>) {
        self.selecting = false;
        if self.thumb_drag.take().is_some() {
            self.list.scrollbar_drag_ended();
            cx.notify();
        }
        if std::mem::take(&mut self.hold_reveal) {
            self.keep_wanted = true;
            self.caret_moved(false);
            cx.notify();
        }
    }

    fn mouse_up(&mut self, e: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.release(cx);
        if self.video_drag.is_some() {
            self.drag_video(e.position, true);
            self.video_drag = None;
            cx.notify();
        }
        if let Some(pending) = self.pending_link.take() {
            match pending.link {
                Link::Web(address) => {
                    cx.open_url(&address);
                    cx.emit(EditorEvent::OpenedWeb(address));
                }
                Link::Wiki(inner) => self.open_wiki(inner, pending.new_tab, pending.anchor, window, cx),
            }
        }
    }

    /// A clicked `[[link]]`: its note opens, or, if there's none, there's an offer
    /// to create it.
    fn open_wiki(&mut self, inner: String, new_tab: bool, anchor: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_link_hover();
        let missing = self.links.as_ref().and_then(|links| {
            let link = WikiLink::parse(&inner)?;
            links.index.resolve(&link.target, links.folder.as_deref()).is_none().then_some(link.target)
        });
        match missing {
            Some(target) => self.offer_create(CreateOffer { name: popover::name_of_target(&target), new_tab, anchor }, window, cx),
            None => cx.emit(EditorEvent::OpenWiki { inner, new_tab, anchor }),
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
            Kind::FenceOpen => fence_language(&text).to_string(),
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
        let media = text.contains("![").then(|| self.media_context());
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
                media: media.as_ref().map(|m| m as &dyn MediaLookup),
                reveal_media: media.is_some() && (self.selection_is_empty() || self.doc.line_of(self.anchor) == line),
                first: line == 0,
                last: line + 1 == self.lines.len(),
                comment_carry: if kind == Kind::FenceBody { self.comment_carry(line, &block_type) } else { (false, false) },
            },
        );
        d.margin = self.margin;
        d
    }

    /// For a line in a code block's body: whether it starts inside a `/* … */`,
    /// and whether one runs on past its end.
    fn comment_carry(&self, line: usize, language: &str) -> (bool, bool) {
        display::comment_carry_at(line, self.lines.len(), language, |l| self.lines[l].kind, |l| self.doc.line_text(l))
    }

    /// Marks the line's misspelled words, if spell checking is on: in prose only,
    /// not code, math, or what stands for something else.
    fn fill_spelling(&self, d: &mut DisplayLine, kind: Kind, caret: Option<usize>) {
        if !self.spell_check || matches!(kind, Kind::FenceBody | Kind::FenceOpen | Kind::FenceClose | Kind::MathBody | Kind::FrontBody) {
            return;
        }
        let checkable = |r: &Range<usize>| {
            d.spans.iter().filter(|s| s.range.start < r.end && r.start < s.range.end).all(|s| {
                s.math.is_none() && s.media.is_none() && !s.unbroken && !display::is_fixed_pitch(&s.font)
            })
        };
        d.misspelled = spell::without_typing(spell::misspelled(&d.text), caret).into_iter().filter(|r| checkable(r)).collect();
    }

    /// Typesets the line's inline math and hands the result to its spans.
    fn fill_math(&mut self, d: &mut DisplayLine, window: &Window) {
        let size = f32::from(d.font_size);
        let display_size = self.typo.size + 2.;
        for span in &mut d.spans {
            if let Some(math) = &mut span.math {
                let size = if math.display { display_size } else { size };
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
            let start = self.doc.line_start(line);
            let caret_at = self.reveal_offset(line);
            let mut built = self.display_of(line, caret_at);
            let own_bottom = built.bottom;
            // The room at the page's two ends.
            if ix == 0 {
                built.top += px(TOP_PAD);
            }
            if ix + 1 == self.items.len() {
                built.bottom += px(BOTTOM_PAD);
            }
            self.fill_math(&mut built, window);
            let typing_at = (line == self.head_line && self.selection_is_empty()).then(|| built.src_to_disp(self.head - start));
            self.fill_spelling(&mut built, self.lines[line].kind, typing_at);
            let display = Rc::new(built);
            let src_len = display.src_len;
            let head_here = line == self.head_line;
            let display_head = if head_here { display.src_to_disp(self.head - start) } else { 0 };
            let blinking_here = self.caret_on || self.caret_slot.is_some();
            let caret = (self.selection_is_empty() && blinking_here && self.caret_shows && head_here).then_some(display_head);
            let autoscroll = head_here && self.autoscroll_line == Some(line) && self.autoscroll_line.take().is_some();
            let keep_at = self
                .keep_now
                .filter(|(kept, _)| *kept == line)
                .map(|(_, y)| (y, self.viewport_cache));
            let slash = self.slash.as_ref().and_then(|menu| {
                (menu.anchor >= start && menu.anchor <= start + src_len)
                    .then(|| (display.src_to_disp(menu.anchor - start), self.slash_box.clone()))
            });
            let placeholder = (line == 0 && self.draft_placeholder && self.is_untouched_draft())
                .then(|| (self.typo.body(), px(self.typo.size), own_bottom));
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
            if display.media_pending && !self.media_wait.contains(&line) {
                self.media_wait.push(line);
            }
            if media::with_store(|s| s.has_queued()) {
                media::pump(cx);
            }
            let element = LineEl {
                line,
                display,
                caret,
                caret_head: head_here.then_some(display_head),
                caret_slot: self.caret_slot.clone(),
                autoscroll,
                selection,
                hovered,
                painted: self.painted.clone(),
                shaped: Rc::new(RefCell::new(None)),
                symbols: self.symbols.clone(),
                videos: self.videos.clone(),
                body_size: px(self.typo.size),
                keep_at,
                slash,
                placeholder,
            };
            return element.into_any_element();
        }

        let range = line..line + item.len as usize;
        let src = self.doc.slice(self.doc.line_start(range.start)..self.doc.line_end(range.end - 1));
        let body: AnyElement = match item.kind {
            ItemKind::Table => blocks::table(&src, self.text_width, &self.typo, &mut self.blocks, window, cx),
            ItemKind::Math => blocks::math(&src, self.text_width, &self.typo, &mut self.blocks, window),
            ItemKind::Front => blocks::frontmatter(&src, self.text_width, &self.typo, window, cx),
            ItemKind::Line => unreachable!(),
        };
        let sel = self.selection();
        let start = self.doc.line_start(range.start);
        BlockEl {
            child: div()
                .w_full()
                .px(self.margin)
                .when(ix == 0, |d| d.pt(px(TOP_PAD)))
                .when(ix + 1 == self.items.len(), |d| d.pb(px(BOTTOM_PAD)))
                .child(body)
                .into_any_element(),
            first_line: range.start,
            n_lines: range.len(),
            selected: !sel.is_empty() && sel.start <= start && sel.end >= start + src.len(),
            painted: self.painted.clone(),
        }
        .into_any_element()
    }

    fn is_untouched_draft(&self) -> bool {
        let draft = fumi_core::note_store::DRAFT_CONTENT;
        self.doc.len() == draft.len() && self.doc.slice(0..draft.len()) == draft
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
        if let Some(slot) = &self.caret_slot {
            slot.rect.set(None);
        }
        self.viewport_cache = self.list.viewport_bounds();
        metrics::frame_begin();
        if std::mem::take(&mut self.media_dirty) {
            self.settle_media(window);
        }
        {
            let ui = self.videos.borrow();
            ui.live.set(false);
            ui.next_frame.set(None);
        }
        self.painted.borrow_mut().clear();
        self.keep_now = self.keep_next.take();
        self.head_line = self.doc.line_of(self.head);
        self.slash_box.set(None);
        // Losing focus closes the menu.
        if self.slash.is_some() && !self.focus.is_focused(window) {
            self.close_slash();
        }
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
        let mut context = KeyContext::default();
        // With the offer to create a note up, the text behind it takes no keys.
        if self.offer.is_none() {
            context.add("Editor");
            if self.slash.is_some() && self.marked.is_none() {
                context.add("slash_menu");
            }
        }
        let root = div()
            .id("editor")
            .key_context(context)
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(t().page_content)
            .text_color(t().text)
            .font(self.typo.body())
            .cursor(if self.thumb_hover || self.thumb_drag.is_some() {
                CursorStyle::Arrow
            } else if self.hovered_link.is_some() {
                CursorStyle::PointingHand
            } else if self.over_video {
                CursorStyle::Arrow
            } else {
                CursorStyle::IBeam
            })
            .on_drop(cx.listener(Self::drop_files));
        let root = self.register_actions(root, cx);
        self.was_focused = self.focus.is_focused(window);
        self.caret_shows = self.was_focused && window.is_window_active();
        self.note_scroll(window, cx);

        let entity = cx.entity();
        let measured = cx.weak_entity();
        let focus = self.focus.clone();
        let thumb = self.scroll_thumb();
        let status = self.show_status.then(|| self.status_text());
        let videos = self.videos.clone();
        let frame_timer = self.frame_timer.clone();
        let dragging = self.selecting || self.thumb_drag.is_some();
        let panel = self.slash_panel(window, cx);
        let offer = self.offer_view(window, cx);
        let rail = self.rail_view(window, cx);
        let content = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(list(self.list.clone(), cx.processor(Self::render_item)).size_full())
            .children(thumb.map(|(top, height)| {
                div()
                    .absolute()
                    .right(px(1.5))
                    .top(top)
                    .w(px(4.))
                    .h(height)
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
                    .text_ellipsis_middle()
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
            .children(panel)
            .children(offer)
            .child(
                canvas(
                    // The column centers in the text pane's own width, which only
                    // layout knows. A change takes effect on the next frame.
                    move |bounds, _, cx| {
                        let _ = measured.update(cx, |e, cx| {
                            if e.pane_width != Some(bounds.size.width) || e.pane_height != Some(bounds.size.height) {
                                e.pane_width = Some(bounds.size.width);
                                e.pane_height = Some(bounds.size.height);
                                cx.notify();
                            }
                        });
                    },
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                        if media::with_store(|s| s.has_queued()) {
                            media::pump(cx);
                        }
                        media_clock(&videos, &frame_timer, &entity, cx);
                        // A press held on the text or the scroller goes on wherever
                        // the pointer goes.
                        if dragging {
                            let moved = entity.clone();
                            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble && e.pressed_button == Some(MouseButton::Left) {
                                    moved.update(cx, |this, cx| this.drag_move(e.position, cx));
                                }
                            });
                            let released = entity.clone();
                            window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                                    released.update(cx, |this, cx| this.release(cx));
                                }
                            });
                        }
                        metrics::frame_end();
                    },
                )
                .absolute()
                .size_full(),
            );
        root.on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !*hovered {
                    this.clear_link_hover();
                    this.thumb_hover = false;
                    cx.notify();
                }
            }))
            .child(content)
            .children(self.fade_band())
            .children(rail)
    }
}

/// Keeps animated things going, and only those that were just drawn: a video
/// playing on screen asks for the next frame, an animated image for a timer.
fn media_clock(
    videos: &Rc<RefCell<VideoUi>>,
    frame_timer: &Rc<std::cell::Cell<Option<Instant>>>,
    entity: &Entity<Editor>,
    cx: &mut App,
) {
    let ui = videos.borrow();
    if ui.live.get() && !ui.watching.replace(true) {
        // Looks often, and draws only when the video has a new frame: a clip at
        // 24 fps needn't redraw the page 120 times a second.
        let weak = entity.downgrade();
        cx.spawn(async move |cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(Duration::from_millis(8)).await;
                let ready = weak.update(cx, |e, cx| {
                    let ready = e.videos.borrow_mut().frame_due();
                    if ready {
                        e.videos.borrow().watching.set(false);
                        cx.notify();
                    }
                    ready
                });
                if ready.unwrap_or(true) {
                    break;
                }
            }
        })
        .detach();
    }
    let Some(due) = ui.next_frame.get() else { return };
    let now = Instant::now();
    if frame_timer.get().is_some_and(|set| set > now && set <= due) {
        return;
    }
    frame_timer.set(Some(due));
    let weak = entity.downgrade();
    let timer = frame_timer.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(due.saturating_duration_since(now)).await;
        if timer.get() == Some(due) {
            timer.set(None);
            let _ = weak.update(cx, |_, cx| cx.notify());
        }
    })
    .detach();
}

// Media -----------------------------------------------------------------------

impl Editor {
    fn media_context(&self) -> MediaContext {
        MediaContext {
            base: (!self.dir.as_os_str().is_empty()).then(|| self.dir.clone()),
            workspace: self.library.clone(),
        }
    }

    /// The store has something new; takes effect on the next frame, however many
    /// arrive before it.
    pub fn media_changed(&mut self, cx: &mut Context<Self>) {
        self.media_dirty = true;
        cx.notify();
    }

    /// Lines that were waiting and aren't any more change height now. Text above
    /// the fold doesn't move: if the change happens above the caret's line while
    /// that's in view, or to something partly scrolled past, the page goes along.
    fn settle_media(&mut self, window: &Window) {
        let waiting = std::mem::take(&mut self.media_wait);
        let mut done = Vec::new();
        for line in waiting {
            if line >= self.lines.len() {
                continue;
            }
            if self.display_of(line, None).media_pending {
                self.media_wait.push(line);
            } else {
                done.push(line);
            }
        }
        if done.is_empty() {
            return;
        }
        let painted = self.painted.clone();
        let painted = painted.borrow();
        let view = self.list.viewport_bounds();
        let caret_top = painted
            .iter()
            .find(|p| self.caret_line >= p.first_line && self.caret_line < p.first_line + p.n_lines)
            .map(|p| p.bounds.top())
            .filter(|top| *top >= view.top() && *top < view.bottom());
        let mut shift = px(0.);
        let mut heights = Vec::with_capacity(painted.len());
        for p in painted.iter() {
            let mut height = p.bounds.size.height;
            if done.contains(&p.first_line) {
                let caret = (p.first_line == self.caret_line).then(|| self.head - self.doc.line_start(p.first_line));
                let mut d = self.display_of(p.first_line, caret);
                self.fill_math(&mut d, window);
                let ix = self.item_of(p.first_line);
                let room = if ix == 0 { px(TOP_PAD) } else { px(0.) } + if ix + 1 == self.items.len() { px(BOTTOM_PAD) } else { px(0.) };
                let new_height = shape(&d, self.list_width, window).height(&d) + room;
                let above_caret = caret_top.is_some_and(|top| p.bounds.top() < top);
                if above_caret || p.bounds.top() < view.top() {
                    shift += new_height - height;
                }
                height = new_height;
            }
            heights.push((self.item_of(p.first_line), height));
        }
        for line in &done {
            self.remeasure_lines(*line..*line + 1);
        }
        if shift != px(0.) {
            let top = self.list.logical_scroll_top();
            let mut rest = top.offset_in_item + shift;
            let mut item_ix = top.item_ix;
            for (ix, height) in heights.iter().filter(|(ix, _)| *ix >= top.item_ix) {
                if rest < *height {
                    break;
                }
                rest -= *height;
                item_ix = ix + 1;
            }
            self.list.scroll_to(ListOffset { item_ix, offset_in_item: rest.max(px(0.)) });
        }
    }

    /// Keeps line numbers right through an edit, and players on their cards. One
    /// whose card is gone stops.
    fn media_lines_moved(&mut self, first: usize, removed: usize, added: usize) {
        let moved = |line: usize| line + added - removed;
        let last = first + removed;
        self.media_wait.retain_mut(|line| {
            if *line > last {
                *line = moved(*line);
            }
            *line < first || *line > first + added
        });
        let videos = self.videos.clone();
        let mut ui = videos.borrow_mut();
        if ui.players.is_empty() {
            return;
        }
        let players = std::mem::take(&mut ui.players);
        for mut player in players {
            if player.line > last {
                player.line = moved(player.line);
            } else if player.line >= first {
                match (first..=first + added).find(|line| self.line_shows_video(*line, &player.path)) {
                    Some(line) => player.line = line,
                    None => continue,
                }
            }
            ui.players.push(player);
        }
        ui.hovered = None;
        self.video_drag = None;
    }

    fn video_in(&self, dest: &str, embed: bool) -> Option<PathBuf> {
        match self.media_context().lookup(dest, embed) {
            Lookup::Ready(info) => match info.kind {
                MediaKind::Video { path } => Some(path),
                MediaKind::Image { .. } => None,
            },
            _ => None,
        }
    }

    fn line_shows_video(&self, line: usize, path: &std::path::Path) -> bool {
        let text = self.doc.line_text(line);
        crate::display::media_spans(&text)
            .iter()
            .any(|m| self.video_in(&m.dest, m.embed).is_some_and(|p| p == path))
    }

    /// A card whose source the caret has opened closes its player.
    fn close_revealed_players(&mut self, line: usize) {
        if !self.selection_is_empty() {
            return;
        }
        let videos = self.videos.clone();
        let mut ui = videos.borrow_mut();
        if ui.players.iter().all(|p| p.line != line) {
            return;
        }
        let text = self.doc.line_text(line);
        let caret = self.head - self.doc.line_start(line);
        for span in crate::display::media_spans(&text) {
            if caret >= span.range.start
                && caret <= span.range.end
                && let Some(path) = self.video_in(&span.dest, span.embed)
            {
                ui.players.retain(|p| p.line != line || p.path != path);
                ui.hovered = None;
                self.video_drag = None;
            }
        }
    }

    fn media_at(&self, p: Point<Pixels>) -> Option<MediaHit> {
        let painted = self.painted.borrow();
        let item = painted.iter().find(|i| p.y >= i.bounds.top() && p.y < i.bounds.bottom())?;
        let (d, s) = item.text.as_ref()?;
        let origin = point(item.bounds.left() + d.margin + d.indent, item.bounds.top() + d.top);
        for (r, span) in d.spans.iter().enumerate() {
            let Some(media) = &span.media else { continue };
            let (at, geom) = media_box(s, r, span, media, origin);
            match &media.info.kind {
                MediaKind::Video { path } => {
                    // Not the caption under it.
                    let area = Bounds::new(at, size(px(geom.width), px(geom.height)));
                    if area.contains(&p) {
                        return Some(MediaHit::Video { line: item.first_line, path: path.clone(), area });
                    }
                }
                MediaKind::Image { .. } => {
                    let area = Bounds::new(at, size(px(geom.box_width), px(geom.box_height)));
                    if area.contains(&p) {
                        return Some(MediaHit::Image { caret: self.doc.line_start(item.first_line) + media.src.end });
                    }
                }
            }
        }
        None
    }

    /// A press on a poster opens the player; on an open player, it's for the
    /// controls, or else plays or pauses.
    fn press_video(&mut self, line: usize, path: &std::path::Path, area: Bounds<Pixels>, p: Point<Pixels>) {
        let videos = self.videos.clone();
        let mut ui = videos.borrow_mut();
        ui.moved_at = Instant::now();
        let Some(index) = ui.player_on(line, path) else {
            let turned = media::with_store(|s| s.video_info(path)).is_some_and(|i| i.turned);
            if let Some(player) = VideoPlayer::start(line, path, turned) {
                ui.players.push(player);
                ui.hovered = Some(ui.players.len() - 1);
            }
            return;
        };
        ui.hovered = Some(index);
        let visible = ui.controls_visible(index);
        let player = &mut ui.players[index];
        let layout = video::controls_layout(area, player.duration() >= 3600.);
        match visible.then(|| layout.control_at(p)).flatten() {
            Some(Control::PlayPause) => player.toggle(),
            Some(Control::Sound) => player.toggle_muted(),
            Some(Control::FullScreen) => player.enter_full_screen(),
            Some(control @ Control::Scrubber) => {
                player.resume = Some(player.is_playing());
                player.pause();
                self.video_drag = Some(VideoDrag { player: index, control, track: layout.track });
                drop(ui);
                self.drag_video(p, false);
            }
            Some(control @ Control::Volume) => {
                self.video_drag = Some(VideoDrag { player: index, control, track: layout.volume.unwrap_or(layout.track) });
                drop(ui);
                self.drag_video(p, false);
            }
            // Clicks on the bar between its controls do nothing.
            None if visible && layout.bar.contains(&p) => {}
            None => player.toggle(),
        }
    }

    fn drag_video(&mut self, p: Point<Pixels>, finished: bool) {
        let Some(drag) = &self.video_drag else { return };
        let videos = self.videos.clone();
        let mut ui = videos.borrow_mut();
        ui.moved_at = Instant::now();
        let Some(player) = ui.players.get_mut(drag.player) else { return };
        let fraction = video::fraction_along(drag.track, p.x);
        match drag.control {
            Control::Scrubber => {
                player.seek(player.duration() * fraction as f64, finished);
                if finished {
                    if player.resume.take() == Some(true) {
                        player.play();
                    }
                }
            }
            Control::Volume => player.set_volume(fraction),
            _ => {}
        }
    }

    /// A video's controls come up while the pointer moves over it.
    fn track_video_pointer(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let hit = self.media_at(p);
        let over = matches!(hit, Some(MediaHit::Video { .. }));
        let videos = self.videos.clone();
        let mut ui = videos.borrow_mut();
        let hovered = match &hit {
            Some(MediaHit::Video { line, path, .. }) => ui.player_on(*line, path),
            _ => None,
        };
        let changed = hovered != ui.hovered || over != self.over_video;
        if hovered.is_some() {
            ui.moved_at = Instant::now();
        }
        ui.hovered = hovered;
        self.over_video = over;
        if changed || hovered.is_some() {
            cx.notify();
        }
    }

    /// For scripts checking a fixture: where things stand.
    pub fn media_state(&self, _: &App) -> String {
        let ui = self.videos.borrow();
        let top = self.list.logical_scroll_top();
        let players: Vec<String> = ui
            .players
            .iter()
            .map(|p| format!("line={} playing={} t={:.2}/{:.2} muted={} vol={:.2}", p.line, p.is_playing(), p.time(), p.duration(), p.is_muted(), p.volume()))
            .collect();
        let painted: Vec<String> = self
            .painted
            .borrow()
            .iter()
            .map(|i| format!("{}@{:.0}+{:.0}", i.first_line, f32::from(i.bounds.top()), f32::from(i.bounds.size.height)))
            .collect();
        format!(
            "frames={} doc_bytes={} head={} anchor={} caret_line={} top=({},{:.1}) items={} panel={} players={players:?} painted={painted:?}",
            metrics::frames(),
            self.doc.len(),
            self.head,
            self.anchor,
            self.caret_line,
            top.item_ix,
            f32::from(top.offset_in_item),
            self.items.len(),
            self.has_folded_panel(),
        )
    }

    fn has_folded_panel(&self) -> bool {
        self.items.first().is_some_and(|i| i.kind == ItemKind::Front)
    }

    /// Shows the frontmatter as its source, the caret at the end of the last
    /// property, ready to edit. The panel is the top of the note, so the page
    /// stays where it is.
    fn open_frontmatter(&mut self) {
        let Some(first) = self.items.first().filter(|i| i.kind == ItemKind::Front) else { return };
        let close = (first.start + first.len - 1) as usize;
        let at = self.doc.line_end(close.saturating_sub(1));
        self.set_head_from_mouse(at, false);
    }
}

impl Editor {
    /// The soft edge text scrolls under at the page's top: the Swift band's wash,
    /// 80% of the page at the top, 30% at 40% down, clear from 75%. GPUI's
    /// gradients take two stops, so it's two bands. Blur isn't available here,
    /// and on Glass, which has no page color to wash with, there's no band.
    fn fade_band(&self) -> Option<Div> {
        let c = t();
        if c.glass {
            return None;
        }
        let wash = |from: f32, to: f32| {
            linear_gradient(180., linear_color_stop(c.page_content.opacity(from), 0.), linear_color_stop(c.page_content.opacity(to), 1.))
        };
        let (first, second) = fade_band_stops(FADE_BAND);
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(FADE_BAND))
                .flex()
                .flex_col()
                .child(div().w_full().h(px(first)).flex_none().bg(wash(0.8, 0.3)))
                .child(div().w_full().h(px(second)).flex_none().bg(wash(0.3, 0.))),
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

    /// The thumb's top and height in the pane, while it shows.
    fn scroll_thumb(&self) -> Option<(Pixels, Pixels)> {
        let recently = self.scrolled_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1000));
        if !recently && !self.thumb_hover && self.thumb_drag.is_none() {
            return None;
        }
        self.thumb_metrics()
    }

    fn thumb_metrics(&self) -> Option<(Pixels, Pixels)> {
        let view = f32::from(self.list.viewport_bounds().size.height);
        let max = f32::from(self.list.max_offset_for_scrollbar().y);
        let offset = -f32::from(self.list.scroll_px_offset_for_scrollbar().y);
        let (top, height) = thumb_rect(view, view + max, offset)?;
        Some((px(top), px(height)))
    }

    fn over_scroller(&self, p: Point<Pixels>) -> bool {
        let view = self.list.viewport_bounds();
        p.x >= view.right() - px(SCROLLER_SLOT) && p.x <= view.right() && p.y >= view.top() && p.y <= view.bottom()
    }

    /// A press in the scroller's slot, while the thumb can be seen: on the thumb
    /// it takes hold, elsewhere it pages toward the pointer.
    fn press_scroller(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        if !self.over_scroller(p) {
            return false;
        }
        let Some((top, height)) = self.scroll_thumb() else { return false };
        let view = self.list.viewport_bounds();
        let y = p.y - view.top();
        if y >= top && y <= top + height {
            self.thumb_drag = Some(y - top);
            self.list.scrollbar_drag_started();
        } else {
            let page = view.size.height - px(10.);
            self.list.scroll_by(if y < top { -page } else { page });
        }
        cx.notify();
        true
    }

    fn drag_scroller(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some(grab), Some((_, height))) = (self.thumb_drag, self.thumb_metrics()) else { return };
        let view = self.list.viewport_bounds();
        let max = f32::from(self.list.max_offset_for_scrollbar().y);
        let offset = thumb_offset(
            f32::from(p.y - view.top() - grab),
            f32::from(view.size.height),
            f32::from(height),
            max,
        );
        self.list.set_offset_from_scrollbar(point(px(0.), px(-offset)));
        self.scrolled_at = Some(Instant::now());
        cx.notify();
    }

    /// The pointer in the slot brings the thumb up.
    fn track_scroller(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let over = self.over_scroller(p) && self.thumb_metrics().is_some();
        if over != self.thumb_hover {
            self.thumb_hover = over;
            cx.notify();
        }
    }
}

/// The scroller's slot, at the pane's right edge.
const SCROLLER_SLOT: f32 = 7.;

/// The thumb's top and height for a pane `view` tall showing `content` from
/// `offset`: its true length, inset 2 at each end, at least 24 tall. None if it
/// all fits.
pub fn thumb_rect(view: f32, content: f32, offset: f32) -> Option<(f32, f32)> {
    if view <= 0. || content <= view {
        return None;
    }
    let raw_height = view * view / content;
    let raw_top = (view - raw_height) * (offset / (content - view)).clamp(0., 1.);
    let inset_height = raw_height - 4.;
    let height = inset_height.max(24.);
    let middle = raw_top + 2. + inset_height / 2.;
    Some((middle - height / 2., height))
}

/// Where the scroll offset goes for a thumb dragged to `top`.
pub fn thumb_offset(top: f32, view: f32, height: f32, max: f32) -> f32 {
    let travel = (view - height).max(1.);
    (top / travel).clamp(0., 1.) * max
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
        // A lone slash, typed or committed in place of what was being composed, not
        // one replacing a selection.
        let lone_slash = slash::is_lone_trigger(text) && (range.is_empty() || self.marked.as_ref() == Some(&range));
        let was_open = self.slash.is_some();
        // Smart quotes and dashes, as typed, not in what an input method commits.
        let (range, text) = match self.smart_punctuation && !composing {
            true => {
                let line_start = self.doc.line_start(self.doc.line_of(range.start));
                let before = self.doc.rope.byte_slice(line_start..range.start).to_string();
                match fumi_ui::text_field::smart_substitution(&before, text) {
                    Some((back, new)) => (range.start - back..range.end, std::borrow::Cow::Owned(new)),
                    None => (range, std::borrow::Cow::Borrowed(text)),
                }
            }
            false => (range, std::borrow::Cow::Borrowed(text)),
        };
        self.edit(range, &text, !composing, composing);
        // A committed composition is a finished unit for undo.
        if composing {
            self.can_coalesce = false;
        }
        if lone_slash && !was_open {
            self.open_slash_if_typed(cx);
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
        self.slash_hold = true;
        self.edit(range, text, false, true);
        self.slash_hold = false;
        self.can_coalesce = true;
        self.marked = (!text.is_empty()).then(|| at..at + text.len());
        if let Some(sel) = selected {
            let base = self.doc.utf16_of(at);
            self.anchor = self.doc.byte_of_utf16(base + sel.start);
            self.head = self.doc.byte_of_utf16(base + sel.end);
        }
        self.refresh_slash();
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

const FADE_BAND: f32 = 30.;

/// The heights of the band's two washes: to 40% down, then on to 75%.
fn fade_band_stops(height: f32) -> (f32, f32) {
    (height * 0.4, height * (0.75 - 0.4))
}

/// A fence's info string's first word: its language.
fn fence_language(line: &str) -> &str {
    line.trim().trim_start_matches(['`', '~']).split_whitespace().next().unwrap_or("")
}

/// The one step that takes `old` to `new`: only what differs, with the selection
/// carried across it. `None` when they're the same.
fn outside_op(old: &str, new: &str, sel_before: (usize, usize)) -> Option<Op> {
    if old == new {
        return None;
    }
    let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let max_suffix = old.len().min(new.len()) - prefix;
    let mut suffix = old.bytes().rev().zip(new.bytes().rev()).take(max_suffix).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    let map = |at: usize| {
        if at <= prefix {
            at
        } else if at >= old_end {
            at - old_end + new_end
        } else {
            new_end
        }
    };
    Some(Op {
        at: prefix,
        old: old[prefix..old_end].to_string(),
        new: new[prefix..new_end].to_string(),
        sel_before,
        sel_after: (map(sel_before.0), map(sel_before.1)),
        typing: false,
    })
}

/// A change from outside goes on the undo stack like any edit: what's under it
/// stays, and, as with `NSUndoManager`, what was undone can't be redone over it,
/// since a redo's offsets don't hold in the new text.
fn record_outside(undo: &mut Vec<Op>, redo: &mut Vec<Op>, op: Op) {
    undo.push(op);
    redo.clear();
}

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
        assert_eq!(kinds, [ItemKind::Front, ItemKind::Table, ItemKind::Math]);
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
    fn the_thumb_keeps_its_true_length_inside_its_inset() {
        // A third of the note showing, at the top: a third of the pane, less 4.
        let (top, height) = thumb_rect(600., 1800., 0.).unwrap();
        assert_eq!((top, height), (2., 196.));
        // At the end it stops 2 short of the bottom.
        let (top, height) = thumb_rect(600., 1800., 1200.).unwrap();
        assert_eq!(top + height, 598.);
        // A long note: 24 at the least.
        let (_, height) = thumb_rect(600., 600_000., 0.).unwrap();
        assert_eq!(height, 24.);
        // Everything fits: no thumb.
        assert_eq!(thumb_rect(600., 600., 0.), None);
        assert_eq!(thumb_rect(0., 600., 0.), None);
    }

    #[core::prelude::v1::test]
    fn dragging_the_thumb_reaches_both_ends() {
        assert_eq!(thumb_offset(0., 600., 200., 1200.), 0.);
        assert_eq!(thumb_offset(400., 600., 200., 1200.), 1200.);
        assert_eq!(thumb_offset(200., 600., 200., 1200.), 600.);
        assert_eq!(thumb_offset(-50., 600., 200., 1200.), 0.);
        assert_eq!(thumb_offset(900., 600., 200., 1200.), 1200.);
    }

    #[core::prelude::v1::test]
    fn multi_line_display_math_folds_into_one_item() {
        let lines = scan_all("a\n\n$$ x\ny\nz $$\n\nb");
        let items = build_items(&lines, None);
        let math: Vec<&Item> = items.iter().filter(|i| i.kind == ItemKind::Math).collect();
        assert_eq!(math.len(), 1);
        assert_eq!((math[0].start, math[0].len), (2, 3));
    }

    fn undo_step(text: &mut String, undo: &mut Vec<Op>, redo: &mut Vec<Op>) -> (usize, usize) {
        let op = undo.pop().unwrap();
        text.replace_range(op.at..op.at + op.new.len(), &op.old);
        let sel = op.sel_before;
        redo.push(op);
        sel
    }

    #[core::prelude::v1::test]
    fn an_outside_change_is_one_undoable_step_over_the_history_before_it() {
        let mut text = "one\ntwo\n".to_string();
        let (mut undo, mut redo) = (Vec::new(), Vec::new());
        // Typed here first.
        let typed = outside_op(&text, "one\ntwo!\n", (7, 7)).unwrap();
        text = "one\ntwo!\n".to_string();
        undo.push(typed);
        // Something undone, waiting to be redone, when the change comes in.
        redo.push(outside_op("x", "y", (0, 0)).unwrap());

        let outside = "zero\none\ntwo!\nthree\n";
        let op = outside_op(&text, outside, (8, 8)).unwrap();
        // The caret stays on its text.
        assert_eq!(&outside[op.sel_after.0..op.sel_after.0 + 1], "\n");
        text.replace_range(op.at..op.at + op.old.len(), &op.new);
        assert_eq!(text, outside);
        record_outside(&mut undo, &mut redo, op);
        assert_eq!(undo.len(), 2, "earlier history is kept");
        assert!(redo.is_empty(), "stale offsets must not be redone");

        // Takes the change back whole, selection included, then the typing.
        assert_eq!(undo_step(&mut text, &mut undo, &mut redo), (8, 8));
        assert_eq!(text, "one\ntwo!\n");
        undo_step(&mut text, &mut undo, &mut redo);
        assert_eq!(text, "one\ntwo\n");
    }

    #[core::prelude::v1::test]
    fn the_top_band_washes_in_two_stretches() {
        assert_eq!(fade_band_stops(30.), (12., 10.5));
    }

    #[core::prelude::v1::test]
    fn the_language_is_the_info_strings_first_word() {
        assert_eq!(fence_language("``` swift"), "swift");
        assert_eq!(fence_language("~~~rust title"), "rust");
        assert_eq!(fence_language("```\tpy"), "py");
        assert_eq!(fence_language("```"), "");
    }

    #[core::prelude::v1::test]
    fn a_merge_after_a_save_keeps_multibyte_edges_whole() {
        // A save's merge reaches the editor by the same step.
        let op = outside_op("写了字", "写了些字", (9, 9)).unwrap();
        assert_eq!((op.at, op.old.as_str(), op.new.as_str()), (6, "", "些"));
        assert_eq!(op.sel_after, (12, 12));
        assert!(!op.typing, "typing after it starts a step of its own");
        assert!(outside_op("same", "same", (0, 0)).is_none());
    }

    #[core::prelude::v1::test]
    fn unclosed_math_is_not_folded() {
        let lines = scan_all("$$\nx\n");
        assert!(build_items(&lines, None).iter().all(|i| i.kind == ItemKind::Line));
    }

}
