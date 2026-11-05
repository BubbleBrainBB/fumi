//! What the sidebar decides without drawing: the tree's rows, how they move, what
//! a drag is over, menus, geometry. Everything here is unit tested.

use fumi_core::app_model::SortMode;
use fumi_core::library_move;
use fumi_core::note::{FileKind, LibraryFile, Note};
use fumi_core::note_tree::{self, NoteFolder};
use fumi_ui::native::MenuRow;
use gpui::SharedString;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

// MARK: - Geometry

/// The window's toolbar band, which the sidebar's column runs under.
pub const BAND_HEIGHT: f32 = 52.;
/// The sidebar button, on the traffic lights' line, after them with the gap a
/// native toolbar item gets: its center 31 pt past the zoom button's, as measured
/// against an AppKit toolbar on macOS 27.
pub const TOGGLE_LEFT: f32 = 93.;
pub const TOGGLE_SIZE: f32 = 28.;
/// Leading and trailing inset of the sidebar's content.
pub const INSET: f32 = 8.;

/// `NoteTreeIndent`.
pub const INDENT_BASE: f32 = 14.;
pub const INDENT_ICON: f32 = 16.;
pub const INDENT_GAP: f32 = 8.;
pub const INDENT_PER_LEVEL: f32 = INDENT_ICON + INDENT_GAP;

pub const ROW_HEIGHT: f32 = 28.;
pub const ROW_SPACING: f32 = 1.;
pub const ROW_PITCH: f32 = ROW_HEIGHT + ROW_SPACING;
pub const LIST_BOTTOM: f32 = 16.;

/// A row's leading inset at `depth`.
pub fn row_leading(depth: usize) -> f32 {
    INDENT_BASE + depth as f32 * INDENT_PER_LEVEL
}

/// The scrolling height of `count` rows.
pub fn content_height(count: f32) -> f32 {
    if count <= 0. { LIST_BOTTOM } else { count * ROW_PITCH - ROW_SPACING + LIST_BOTTOM }
}

/// The rows showing in a viewport, with a few extra on either side.
pub fn visible_range(offset: f32, viewport: f32, count: usize) -> std::ops::Range<usize> {
    const OVERSCAN: usize = 4;
    let first = ((offset / ROW_PITCH).floor().max(0.) as usize).saturating_sub(OVERSCAN);
    let last = (((offset + viewport) / ROW_PITCH).ceil().max(0.) as usize + OVERSCAN).min(count);
    first.min(last)..last
}

/// The natural width of a note's row, for double-click-to-fit on the divider.
/// `depth` is how many folders down the note is; `name_width` its file name at
/// 13 pt semibold.
pub fn fit_row_width(depth: usize, name_width: f32, pinned: bool) -> f32 {
    let outer = INSET + 8.;
    let indent = row_leading(depth) + INDENT_ICON + INDENT_GAP;
    outer + indent + name_width.ceil() + 10. + if pinned { 16. } else { 0. }
}

/// How many folders down a note in the folder `relative_path` is.
pub fn folder_depth(relative_path: &str) -> usize {
    relative_path.split('/').filter(|part| !part.is_empty()).count()
}

/// The narrowest a drag holds the sidebar at, just under the 200 it settles to.
pub const DRAG_FLOOR: f32 = 192.;

/// Where the divider is while it's dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DividerDrag {
    Width(f32),
    /// Past half the narrowest width, the split view lets go and collapses.
    Collapsed,
}

pub fn divider_drag(proposed: f32) -> DividerDrag {
    if proposed < DRAG_FLOOR / 2. {
        DividerDrag::Collapsed
    } else {
        DividerDrag::Width(proposed.clamp(DRAG_FLOOR, fumi_core::sidebar_width::MAX as f32))
    }
}

/// The pointer's reach around the divider's line, on either side.
pub const DIVIDER_REACH: f32 = 5.;

/// The width saved for a library, if it counts: widths from when the icon rail
/// existed (64) count as unset.
pub fn saved_width(saved: Option<f64>) -> f32 {
    match saved {
        Some(width) if width >= fumi_core::sidebar_width::MINIMUM => width as f32,
        _ => fumi_core::sidebar_width::STANDARD_DEFAULT as f32,
    }
}

pub fn width_key(library: &str) -> String {
    format!("fumi.sidebarWidth.{library}")
}

// MARK: - Motion

/// SwiftUI's named curves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    EaseInOut,
    EaseOut,
}

impl Curve {
    /// How far along at `t` in 0...1.
    pub fn at(self, t: f32) -> f32 {
        match self {
            Curve::EaseInOut => bezier(0.42, 0., 0.58, 1., t),
            Curve::EaseOut => bezier(0., 0., 0.58, 1., t),
        }
    }
}

fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    if t == 0. || t == 1. {
        return t;
    }
    let sample = |a: f32, b: f32, u: f32| {
        let v = 1. - u;
        3. * v * v * u * a + 3. * v * u * u * b + u * u * u
    };
    // Bisection: exact enough at any frame rate, and never diverges.
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..16 {
        let mid = (lo + hi) / 2.;
        if sample(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    sample(y1, y2, (lo + hi) / 2.)
}

/// A value on its way somewhere. `elapsed` comes from the caller's clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub seconds: f32,
    pub curve: Curve,
}

impl Tween {
    pub fn value(&self, elapsed: Duration) -> f32 {
        if self.is_done(elapsed) {
            return self.to;
        }
        let e = self.curve.at(elapsed.as_secs_f32() / self.seconds);
        self.from + (self.to - self.from) * e
    }

    pub fn is_done(&self, elapsed: Duration) -> bool {
        self.seconds <= 0. || elapsed.as_secs_f32() >= self.seconds
    }
}

// MARK: - The tree's rows

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowKind {
    Folder,
    Note { pinned: bool },
    File(FileKind),
}

/// One line of the tree. Keeps what it draws, so scrolling asks the store nothing.
#[derive(Debug, PartialEq)]
pub struct TreeRow {
    pub kind: RowKind,
    /// The folder's id, or the note's or file's.
    pub id: SharedString,
    pub name: SharedString,
    pub depth: usize,
    /// The folder this row is in; a drop on a note or file lands there.
    pub parent: String,
    /// Where it is on disk.
    pub path: PathBuf,
}

impl TreeRow {
    pub fn is_folder(&self) -> bool {
        matches!(self.kind, RowKind::Folder)
    }

    /// What a drop on this row moves into.
    pub fn drop_folder(&self) -> String {
        if self.is_folder() { self.id.to_string() } else { self.parent.clone() }
    }
}

#[derive(Debug)]
struct Branch {
    row: Option<Rc<TreeRow>>,
    folders: Vec<Branch>,
    items: Vec<Rc<TreeRow>>,
}

/// The tree as of one revision of the library: everything but the Memos folder,
/// each folder's subfolders first, then its notes in the chosen sort, then its
/// files by name.
#[derive(Debug)]
pub struct TreeSnapshot {
    root: Branch,
    /// Notes in the tree: none shows "No notes yet".
    pub note_count: usize,
    /// Every folder, in tree order, for Move To.
    pub folder_ids: Vec<String>,
}

impl TreeSnapshot {
    pub fn build(notes: &[Note], files: &[LibraryFile], mode: SortMode, library: &Path) -> Self {
        let sorted = note_tree::sorted(notes.iter().filter(|note| note_tree::is_in_tree(&note.relative_path)), mode);
        let files = files.iter().filter(|file| note_tree::is_in_tree(&file.relative_path));
        let tree = note_tree::build_from(sorted, files);
        TreeSnapshot {
            note_count: tree.total_count(),
            folder_ids: note_tree::folder_ids(&tree),
            root: branch(&tree, None, 0, library),
        }
    }

    /// The rows showing with these folders open.
    pub fn flatten(&self, expanded: &HashSet<String>) -> Vec<Rc<TreeRow>> {
        let mut rows = Vec::new();
        flatten_into(&self.root, expanded, &mut rows);
        rows
    }
}

fn branch(folder: &NoteFolder<'_>, row: Option<Rc<TreeRow>>, depth: usize, library: &Path) -> Branch {
    let folders = folder
        .folders
        .iter()
        .map(|child| {
            let row = TreeRow {
                kind: RowKind::Folder,
                id: child.id.clone().into(),
                name: child.name.clone().into(),
                depth,
                parent: parent_folder(&child.id),
                path: library_move::folder_path(&child.id, library),
            };
            branch(child, Some(Rc::new(row)), depth + 1, library)
        })
        .collect();
    let notes = folder.notes.iter().map(|note| {
        Rc::new(TreeRow {
            kind: RowKind::Note { pinned: note.pinned },
            id: note.id.clone().into(),
            name: note.filename().into(),
            depth,
            parent: note.relative_path.clone(),
            path: note.path.clone(),
        })
    });
    let files = folder.files.iter().map(|file| {
        Rc::new(TreeRow {
            kind: RowKind::File(file.kind),
            id: file.id.clone().into(),
            name: file.name().into(),
            depth,
            parent: file.relative_path.clone(),
            path: file.path.clone(),
        })
    });
    Branch { row, folders, items: notes.chain(files).collect() }
}

fn flatten_into(branch: &Branch, expanded: &HashSet<String>, rows: &mut Vec<Rc<TreeRow>>) {
    for child in &branch.folders {
        let Some(row) = &child.row else { continue };
        rows.push(row.clone());
        if expanded.contains(row.id.as_ref()) {
            flatten_into(child, expanded, rows);
        }
    }
    rows.extend(branch.items.iter().cloned());
}

/// The folder a folder id is in: "/A/B" is in "/A", "/A" in "/".
pub fn parent_folder(id: &str) -> String {
    match id.rfind('/') {
        None | Some(0) => "/".to_string(),
        Some(index) => id[..index].to_string(),
    }
}

/// One row on its way between two places, fading in or out if it's coming or going.
#[derive(Clone, Debug)]
pub struct MovingRow {
    pub row: Rc<TreeRow>,
    pub from_y: f32,
    pub to_y: f32,
    pub from_alpha: f32,
    pub to_alpha: f32,
}

impl MovingRow {
    pub fn at(&self, e: f32) -> (f32, f32) {
        (self.from_y + (self.to_y - self.from_y) * e, self.from_alpha + (self.to_alpha - self.from_alpha) * e)
    }
}

/// How the rows change: a row that stays slides to its new place, a new one fades
/// in where it will be, a gone one fades out where it was. `current` is what's on
/// screen now, as (row, y, alpha), so an interrupted move carries on from there.
pub fn motion(current: &[(Rc<TreeRow>, f32, f32)], rows: &[Rc<TreeRow>]) -> Vec<MovingRow> {
    let before: HashMap<&str, (f32, f32)> = current.iter().map(|(row, y, alpha)| (row.id.as_ref(), (*y, *alpha))).collect();
    let after: HashSet<&str> = rows.iter().map(|row| row.id.as_ref()).collect();
    let mut moving: Vec<MovingRow> = current
        .iter()
        .filter(|(row, _, _)| !after.contains(row.id.as_ref()))
        .map(|(row, y, alpha)| MovingRow { row: row.clone(), from_y: *y, to_y: *y, from_alpha: *alpha, to_alpha: 0. })
        .collect();
    moving.extend(rows.iter().enumerate().map(|(index, row)| {
        let to_y = index as f32 * ROW_PITCH;
        let (from_y, from_alpha) = before.get(row.id.as_ref()).copied().unwrap_or((to_y, 0.));
        MovingRow { row: row.clone(), from_y, to_y, from_alpha, to_alpha: 1. }
    }));
    moving
}

// MARK: - Dragging over the tree

/// What the pointer is over during a drag of tree rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Zone {
    /// The library's heading: the top level, at once.
    Header,
    /// A row: its folder, at once. `springs` when it's a closed folder.
    Row { folder: String, springs: bool },
    /// The list behind the rows: the top level, after a beat.
    Behind,
}

impl Zone {
    pub fn folder(&self) -> &str {
        match self {
            Zone::Header | Zone::Behind => "/",
            Zone::Row { folder, .. } => folder,
        }
    }
}

/// The zone at a point inside the list, `y` measured down its scrolled content and
/// `x` across its full width. The hairline between rows is the list behind them.
pub fn zone_at(rows: &[Rc<TreeRow>], expanded: &HashSet<String>, x: f32, y: f32, width: f32) -> Zone {
    if y < 0. || x < INSET || x >= width - INSET {
        return Zone::Behind;
    }
    let index = (y / ROW_PITCH).floor() as usize;
    if y - index as f32 * ROW_PITCH >= ROW_HEIGHT {
        return Zone::Behind;
    }
    match rows.get(index) {
        None => Zone::Behind,
        Some(row) => Zone::Row {
            folder: row.drop_folder(),
            springs: row.is_folder() && !expanded.contains(row.id.as_ref()),
        },
    }
}

/// How long the pointer rests on a closed folder before it opens.
pub const SPRING_DELAY: Duration = Duration::from_millis(650);
/// Rows sit nearly edge to edge, so leaving one usually means entering the next a
/// moment later. Changes off a row or onto the list behind wait this long, so the
/// highlight goes straight to the next row.
pub const SETTLE_DELAY: Duration = Duration::from_millis(70);

/// Port of `NoteTreeState`'s drop highlight and spring loading. Timers are the
/// caller's: a call that returns a ticket wants a callback after the delay.
#[derive(Default, Debug)]
pub struct DropState {
    pub target: Option<String>,
    pending: Option<(u64, Option<String>)>,
    spring: Option<(u64, String)>,
    tickets: u64,
    zone: Option<Zone>,
}

impl DropState {
    fn ticket(&mut self) -> u64 {
        self.tickets += 1;
        self.tickets
    }

    /// A row takes the highlight at once; the list behind (`settles`) and a target
    /// turning off wait for [`settled`](Self::settled), in case something claims it.
    pub fn set_targeted(&mut self, targeted: bool, folder: &str, settles: bool) -> Option<u64> {
        if targeted && !settles {
            self.pending = None;
            if self.target.as_deref() != Some(folder) {
                self.target = Some(folder.to_string());
            }
            return None;
        }
        if !targeted && self.target.as_deref() != Some(folder) {
            return None;
        }
        let ticket = self.ticket();
        self.pending = Some((ticket, targeted.then(|| folder.to_string())));
        Some(ticket)
    }

    /// The settle delay ran out. Whether the target changed.
    pub fn settled(&mut self, ticket: u64) -> bool {
        match self.pending.take() {
            Some((pending, target)) if pending == ticket => {
                self.target = target;
                true
            }
            other => {
                self.pending = other;
                false
            }
        }
    }

    pub fn spring_load(&mut self, folder: &str) -> u64 {
        let ticket = self.ticket();
        self.spring = Some((ticket, folder.to_string()));
        ticket
    }

    pub fn cancel_spring(&mut self, folder: Option<&str>) {
        if let Some((_, current)) = &self.spring {
            if folder.is_none_or(|folder| folder == current) {
                self.spring = None;
            }
        }
    }

    /// The spring delay ran out: the folder to open, if the drag is still on it.
    pub fn sprung(&mut self, ticket: u64) -> Option<String> {
        match self.spring.take() {
            Some((pending, folder)) if pending == ticket => (self.target.as_deref() == Some(&folder)).then_some(folder),
            other => {
                self.spring = other;
                None
            }
        }
    }

    /// The pointer is now over `zone`. Tickets to schedule, for settling and springing.
    pub fn enter_zone(&mut self, zone: Option<Zone>) -> (Option<u64>, Option<u64>) {
        if self.zone == zone {
            return (None, None);
        }
        let mut settle = None;
        if let Some(old) = self.zone.take() {
            let folder = old.folder().to_string();
            settle = self.set_targeted(false, &folder, false);
            self.cancel_spring(Some(&folder));
        }
        let mut spring = None;
        if let Some(new) = &zone {
            let (folder, settles) = (new.folder().to_string(), *new == Zone::Behind);
            if let Some(ticket) = self.set_targeted(true, &folder, settles) {
                settle = Some(ticket);
            } else if self.pending.is_none() {
                settle = None;
            }
            if matches!(new, Zone::Row { springs: true, .. }) {
                spring = Some(self.spring_load(&folder));
            }
        }
        self.zone = zone;
        (settle, spring)
    }

    pub fn zone(&self) -> Option<&Zone> {
        self.zone.as_ref()
    }

    /// The drag ended, one way or another.
    pub fn reset(&mut self) {
        self.target = None;
        self.pending = None;
        self.spring = None;
        self.zone = None;
    }
}

/// `tree.folderName` of the Move To menu's first row and the alerts.
pub fn destination_label(id: &str, library_name: &str) -> String {
    if id == "/" {
        library_name.to_string()
    } else {
        id.split('/').filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" › ")
    }
}

/// Move To: the library's top level and every folder of the tree, but not the
/// item's own folder, nor, for a folder, itself or a folder inside it.
pub fn move_destinations(folder_ids: &[String], parent: &str, moving_folder: Option<&str>) -> Vec<String> {
    std::iter::once("/")
        .chain(folder_ids.iter().map(String::as_str))
        .filter(|id| *id != parent && !moving_folder.is_some_and(|folder| library_move::is_same_or_inside(id, folder)))
        .map(str::to_string)
        .collect()
}

// MARK: - Menus

/// A native menu's rows, and what each item means, in the order `popup_menu`
/// numbers them. A disabled item takes a number and means nothing.
pub struct Menu<A> {
    pub rows: Vec<MenuRow>,
    pub actions: Vec<Option<A>>,
}

impl<A> Default for Menu<A> {
    fn default() -> Self {
        Menu { rows: Vec::new(), actions: Vec::new() }
    }
}

impl<A> Menu<A> {
    pub fn item(mut self, title: impl Into<String>, action: A) -> Self {
        self.rows.push(MenuRow::item(title));
        self.actions.push(Some(action));
        self
    }

    pub fn item_if(mut self, enabled: bool, title: impl Into<String>, action: A) -> Self {
        if enabled {
            return self.item(title, action);
        }
        self.rows.push(MenuRow::disabled(title));
        self.actions.push(None);
        self
    }

    pub fn destructive(mut self, title: impl Into<String>, action: A) -> Self {
        self.rows.push(MenuRow::destructive(title));
        self.actions.push(Some(action));
        self
    }

    pub fn separator(mut self) -> Self {
        self.rows.push(MenuRow::Separator);
        self
    }

    pub fn choice(mut self, title: impl Into<String>, checked: bool, action: A) -> Self {
        self.rows.push(MenuRow::Choice { title: title.into(), checked, image: None });
        self.actions.push(Some(action));
        self
    }

    pub fn submenu(mut self, title: impl Into<String>, menu: Menu<A>) -> Self {
        self.rows.push(MenuRow::Submenu { title: title.into(), rows: menu.rows });
        self.actions.extend(menu.actions);
        self
    }

    /// What the item numbered `index` means.
    pub fn take(mut self, index: usize) -> Option<A> {
        if index < self.actions.len() { self.actions.swap_remove(index) } else { None }
    }
}

/// What a tree row's menu can do.
#[derive(Clone, Debug, PartialEq)]
pub enum RowAction {
    TogglePin,
    Rename,
    MoveTo(String),
    Reveal,
    Delete,
    CopyLink,
    OpenInDefaultApp,
    Trash,
}

fn move_to(menu: Menu<RowAction>, destinations: Vec<String>, library_name: &str) -> Menu<RowAction> {
    if destinations.is_empty() {
        return menu.item_if(false, "Move To", RowAction::Reveal);
    }
    let mut sub = Menu::default();
    for id in destinations {
        sub = sub.item(destination_label(&id, library_name), RowAction::MoveTo(id));
    }
    menu.submenu("Move To", sub)
}

/// The context menu of a row in the tree.
pub fn row_menu(row: &TreeRow, folder_ids: &[String], library_name: &str) -> Menu<RowAction> {
    match row.kind {
        RowKind::Note { pinned } => {
            let menu = Menu::default()
                .item(if pinned { "Unpin" } else { "Pin" }, RowAction::TogglePin)
                .item("Rename…", RowAction::Rename);
            move_to(menu, move_destinations(folder_ids, &row.parent, None), library_name)
                .item("Reveal in Finder", RowAction::Reveal)
                .separator()
                .destructive("Delete", RowAction::Delete)
        }
        RowKind::File(_) => {
            let menu = Menu::default()
                .item("Copy Markdown Link", RowAction::CopyLink)
                .item("Open in Default App", RowAction::OpenInDefaultApp);
            move_to(menu, move_destinations(folder_ids, &row.parent, None), library_name)
                .item("Reveal in Finder", RowAction::Reveal)
                .separator()
                .destructive("Move to Trash", RowAction::Trash)
        }
        RowKind::Folder => move_to(Menu::default(), move_destinations(folder_ids, &row.parent, Some(&row.id)), library_name)
            .item("Reveal in Finder", RowAction::Reveal),
    }
}

/// What the library heading's … menu can do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeaderAction {
    OpenFolder,
    NewWindow,
    Reveal,
    Sort(SortMode),
}

pub fn header_menu(has_library: bool, sort: SortMode) -> Menu<HeaderAction> {
    let mut menu = Menu::default().item("Open Folder…", HeaderAction::OpenFolder).item("Open in New Window", HeaderAction::NewWindow);
    if has_library {
        menu = menu.item("Reveal in Finder", HeaderAction::Reveal);
    }
    let mut picker = Menu::default();
    for mode in [SortMode::UpdatedDesc, SortMode::CreatedDesc, SortMode::TitleAsc] {
        picker = picker.choice(mode.label(), mode == sort, HeaderAction::Sort(mode));
    }
    menu.separator().submenu("Sort By", picker)
}

/// What a row on the Libraries level's menu can do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LibraryAction {
    OpenInNewWindow,
    Reveal,
    Rename,
    ChangeIcon,
    Remove,
}

pub fn library_menu(current: bool, missing: bool) -> Menu<LibraryAction> {
    Menu::default()
        .item_if(!current && !missing, "Open in New Window", LibraryAction::OpenInNewWindow)
        .item_if(!missing, "Reveal in Finder", LibraryAction::Reveal)
        .item("Rename…", LibraryAction::Rename)
        .item("Change Icon…", LibraryAction::ChangeIcon)
        .separator()
        .item_if(!current, "Remove from List", LibraryAction::Remove)
}

// MARK: - The Libraries level

pub const LIBRARY_ROW_HEIGHT: f32 = 44.;
pub const LIBRARY_ROW_SPACING: f32 = 2.;
pub const LIBRARY_ROW_PITCH: f32 = LIBRARY_ROW_HEIGHT + LIBRARY_ROW_SPACING;

/// The Libraries level's scrolling height: its rows, then Open Folder….
pub fn libraries_content_height(count: usize) -> f32 {
    4. + count as f32 * LIBRARY_ROW_PITCH + 6. + 30. + 16.
}

/// " · 516 notes", for the open library once it's loaded.
pub fn note_count_label(count: usize) -> String {
    format!(" · {count} {}", if count == 1 { "note" } else { "notes" })
}

pub fn library_tooltip(path: &str, missing: bool) -> String {
    if missing { path.to_string() } else { format!("{path}\n⌘-click to open in a new window") }
}

/// `Unicode.Scalar.Properties.isEmojiPresentation`, which sets a chosen icon's size.
pub fn is_emoji_presentation(c: char) -> bool {
    matches!(c as u32,
        0x231A..=0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3 | 0x25FD..=0x25FE | 0x2614..=0x2615
        | 0x2648..=0x2653 | 0x267F | 0x2693 | 0x26A1 | 0x26AA..=0x26AB | 0x26BD..=0x26BE
        | 0x26C4..=0x26C5 | 0x26CE | 0x26D4 | 0x26EA | 0x26F2..=0x26F3 | 0x26F5 | 0x26FA | 0x26FD
        | 0x2705 | 0x270A..=0x270B | 0x2728 | 0x274C | 0x274E | 0x2753..=0x2755 | 0x2757
        | 0x2795..=0x2797 | 0x27B0 | 0x27BF | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55
        | 0x1F004 | 0x1F0CF | 0x1F18E | 0x1F191..=0x1F19A | 0x1F1E6..=0x1F202 | 0x1F21A | 0x1F22F
        | 0x1F232..=0x1F236 | 0x1F238..=0x1F23A | 0x1F250..=0x1F251 | 0x1F300..=0x1F320
        | 0x1F32D..=0x1F335 | 0x1F337..=0x1F37C | 0x1F37E..=0x1F393 | 0x1F3A0..=0x1F3CA
        | 0x1F3CF..=0x1F3D3 | 0x1F3E0..=0x1F3F0 | 0x1F3F4 | 0x1F3F8..=0x1F43E | 0x1F440
        | 0x1F442..=0x1F4FC | 0x1F4FF..=0x1F53D | 0x1F54B..=0x1F54E | 0x1F550..=0x1F567 | 0x1F57A
        | 0x1F595..=0x1F596 | 0x1F5A4 | 0x1F5FB..=0x1F64F | 0x1F680..=0x1F6C5 | 0x1F6CC
        | 0x1F6D0..=0x1F6D2 | 0x1F6D5..=0x1F6D7 | 0x1F6DC..=0x1F6DF | 0x1F6EB..=0x1F6EC
        | 0x1F6F4..=0x1F6FC | 0x1F7E0..=0x1F7EB | 0x1F7F0 | 0x1F90C..=0x1F93A | 0x1F93C..=0x1F945
        | 0x1F947..=0x1F9FF | 0x1FA70..=0x1FAFF)
}

// MARK: - Scroller

/// The thumb of a sidebar scroller: 45% of its true length, 28 to 88 pt, travelling
/// the full slot. Returns (y, height) in the viewport; `None` with nothing to scroll.
pub fn scroller_knob(viewport: f32, content: f32, offset: f32) -> Option<(f32, f32)> {
    if content <= viewport || viewport <= 0. {
        return None;
    }
    let raw = (viewport / content * viewport - 4.).max(0.);
    let height = (raw * 0.45).max(28.).min(88.);
    let progress = (offset / (content - viewport)).clamp(0., 1.);
    Some((2. + (viewport - 4. - height).max(0.) * progress, height))
}

/// The scroll offset for a thumb dragged so its top is at `knob_y`.
pub fn scroller_offset(viewport: f32, content: f32, knob_y: f32) -> f32 {
    let Some((_, height)) = scroller_knob(viewport, content, 0.) else { return 0. };
    let travel = (viewport - 4. - height).max(1.);
    ((knob_y - 2.) / travel).clamp(0., 1.) * (content - viewport)
}

// MARK: - Swipe

/// A two-finger swipe to the right over the sidebar: back to the libraries, once.
#[derive(Default, Debug)]
pub struct Swipe {
    distance: f64,
    fired: bool,
}

impl Swipe {
    /// One scroll event. Whether the swipe just went far enough.
    pub fn feed(&mut self, gesture: &fumi_ui::native::ScrollGesture) -> bool {
        if gesture.began {
            self.distance = 0.;
            self.fired = false;
        }
        if !(gesture.began || gesture.changed) || self.fired || gesture.delta_x.abs() <= gesture.delta_y.abs() * 1.5 {
            return false;
        }
        // Fingers to the right, whichever way scrolling is set.
        self.distance += if gesture.inverted { gesture.delta_x } else { -gesture.delta_x };
        if self.distance > 60. {
            self.fired = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fumi_ui::native::ScrollGesture;

    fn note(path: &str, relative: &str, updated: f64, pinned: bool) -> Note {
        Note {
            id: path.to_string(),
            path: PathBuf::from(path),
            title: String::new(),
            content: String::new(),
            tags: Vec::new(),
            created_at: 100. - updated,
            updated_at: updated,
            pinned,
            relative_path: relative.to_string(),
        }
    }

    fn file(path: &str, relative: &str) -> LibraryFile {
        LibraryFile { id: path.to_string(), path: PathBuf::from(path), kind: FileKind::Image, updated_at: 0., relative_path: relative.to_string() }
    }

    fn sample() -> TreeSnapshot {
        let notes = vec![
            note("/L/b.md", "/", 2., false),
            note("/L/a.md", "/", 1., false),
            note("/L/p.md", "/", 0., true),
            note("/L/Travel/Japan/kyoto.md", "/Travel/Japan", 3., false),
            note("/L/Travel/plan.md", "/Travel", 3., false),
            note("/L/Memos/2026-01-01.md", "/Memos", 9., false),
            note("/L/Memos/old/x.md", "/Memos/old", 9., false),
            note("/L/xfolder10/n.md", "/xfolder10", 3., false),
            note("/L/xfolder2/n.md", "/xfolder2", 3., false),
        ];
        let files = vec![file("/L/z.png", "/"), file("/L/c.png", "/"), file("/L/Memos/pic.png", "/Memos")];
        TreeSnapshot::build(&notes, &files, SortMode::UpdatedDesc, Path::new("/L"))
    }

    fn names(rows: &[Rc<TreeRow>]) -> Vec<&str> {
        rows.iter().map(|row| row.name.as_ref()).collect()
    }

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn folders_then_pinned_then_sorted_notes_then_files() {
        let tree = sample();
        let rows = tree.flatten(&HashSet::new());
        assert_eq!(names(&rows), ["Travel", "xfolder2", "xfolder10", "p", "b", "a", "c.png", "z.png"]);
    }

    #[test]
    fn memos_stay_out() {
        let tree = sample();
        assert!(!tree.folder_ids.iter().any(|id| id.contains("Memos")));
        assert_eq!(tree.note_count, 7);
    }

    #[test]
    fn open_folders_show_their_children_one_level_down() {
        let tree = sample();
        let rows = tree.flatten(&set(&["/Travel", "/Travel/Japan"]));
        assert_eq!(&names(&rows)[..4], ["Travel", "Japan", "kyoto", "plan"]);
        let depths: Vec<usize> = rows[..4].iter().map(|row| row.depth).collect();
        assert_eq!(depths, [0, 1, 2, 1]);
        assert_eq!(rows[1].parent, "/Travel");
        assert_eq!(rows[1].path, PathBuf::from("/L/Travel/Japan"));
        // A closed parent hides an open child.
        assert_eq!(names(&tree.flatten(&set(&["/Travel/Japan"])))[..2], ["Travel", "xfolder2"]);
    }

    #[test]
    fn sort_modes() {
        let notes = vec![note("/L/note10.md", "/", 1., false), note("/L/note2.md", "/", 3., false), note("/L/pinned.md", "/", 0., true)];
        let order = |mode| {
            note_tree::sorted(&notes, mode).iter().map(|note| note.filename()).collect::<Vec<_>>()
        };
        assert_eq!(order(SortMode::UpdatedDesc), ["pinned", "note2", "note10"]);
        assert_eq!(order(SortMode::CreatedDesc), ["pinned", "note10", "note2"]);
        assert_eq!(order(SortMode::TitleAsc), ["pinned", "note2", "note10"]);
    }

    #[test]
    fn geometry() {
        assert_eq!(row_leading(0), 14.);
        assert_eq!(row_leading(2), 62.);
        assert_eq!(content_height(0.), 16.);
        assert_eq!(content_height(2.), 28. + 1. + 28. + 16.);
        assert_eq!(visible_range(0., 100., 1000), 0..8);
        assert_eq!(visible_range(2900., 100., 1000), 96..108);
        assert_eq!(visible_range(0., 100., 3), 0..3);
        assert_eq!(visible_range(500., 100., 3), 3..3);
    }

    #[test]
    fn fit_width_formula() {
        // 16 outer + 14 base + 24 x depth + 16 icon + 8 gap + name + 10, + 16 if pinned.
        assert_eq!(fit_row_width(0, 100., false), 16. + 14. + 16. + 8. + 100. + 10.);
        assert_eq!(fit_row_width(2, 100.2, true), 16. + 14. + 48. + 16. + 8. + 101. + 10. + 16.);
        assert_eq!(folder_depth("/"), 0);
        assert_eq!(folder_depth("/A/B"), 2);
    }

    #[test]
    fn divider_collapses_past_half_the_floor() {
        assert_eq!(divider_drag(500.), DividerDrag::Width(420.));
        assert_eq!(divider_drag(250.), DividerDrag::Width(250.));
        assert_eq!(divider_drag(150.), DividerDrag::Width(192.));
        assert_eq!(divider_drag(95.), DividerDrag::Collapsed);
    }

    #[test]
    fn saved_widths_under_the_minimum_are_unset() {
        assert_eq!(saved_width(None), 240.);
        assert_eq!(saved_width(Some(64.)), 240.);
        assert_eq!(saved_width(Some(200.)), 200.);
        assert_eq!(saved_width(Some(333.)), 333.);
        assert_eq!(width_key("/a b"), "fumi.sidebarWidth./a b");
    }

    #[test]
    fn curves_and_tweens() {
        assert_eq!(Curve::EaseInOut.at(0.), 0.);
        assert_eq!(Curve::EaseInOut.at(1.), 1.);
        assert!((Curve::EaseInOut.at(0.5) - 0.5).abs() < 0.001);
        assert!(Curve::EaseOut.at(0.5) > 0.6);
        let tween = Tween { from: 240., to: 0., seconds: 0.2, curve: Curve::EaseInOut };
        assert_eq!(tween.value(Duration::ZERO), 240.);
        assert!((tween.value(Duration::from_millis(100)) - 120.).abs() < 0.5);
        assert_eq!(tween.value(Duration::from_millis(200)), 0.);
        assert!(tween.is_done(Duration::from_millis(200)));
    }

    #[test]
    fn motion_slides_stayers_and_fades_the_rest() {
        let tree = sample();
        let closed = tree.flatten(&HashSet::new());
        let open = tree.flatten(&set(&["/Travel"]));
        let current: Vec<_> = closed.iter().enumerate().map(|(i, row)| (row.clone(), i as f32 * ROW_PITCH, 1.)).collect();
        let moving = motion(&current, &open);
        let find = |name: &str| moving.iter().find(|m| m.row.name.as_ref() == name).unwrap();
        // Coming: fades in where it will be.
        let plan = find("plan");
        assert_eq!((plan.from_y, plan.to_y, plan.from_alpha, plan.to_alpha), (2. * ROW_PITCH, 2. * ROW_PITCH, 0., 1.));
        // Staying: slides down to make room.
        let folder2 = find("xfolder2");
        assert_eq!((folder2.from_y, folder2.to_y, folder2.from_alpha), (ROW_PITCH, 3. * ROW_PITCH, 1.));
        assert_eq!(folder2.at(0.5).0, 2. * ROW_PITCH);
        // And back: going fades out where it was.
        let current: Vec<_> = open.iter().enumerate().map(|(i, row)| (row.clone(), i as f32 * ROW_PITCH, 1.)).collect();
        let moving = motion(&current, &closed);
        let plan = moving.iter().find(|m| m.row.name.as_ref() == "plan").unwrap();
        assert_eq!((plan.from_y, plan.to_y, plan.to_alpha), (2. * ROW_PITCH, 2. * ROW_PITCH, 0.));
    }

    #[test]
    fn zones_over_the_list() {
        let tree = sample();
        let rows = tree.flatten(&set(&["/Travel"]));
        let open = set(&["/Travel"]);
        let row_y = |i: usize| i as f32 * ROW_PITCH + 3.;
        // An open folder.
        assert_eq!(zone_at(&rows, &open, 50., row_y(0), 240.), Zone::Row { folder: "/Travel".into(), springs: false });
        // A closed one springs.
        let closed = rows.iter().position(|r| r.name.as_ref() == "xfolder2").unwrap();
        assert_eq!(zone_at(&rows, &open, 50., row_y(closed), 240.), Zone::Row { folder: "/xfolder2".into(), springs: true });
        // A note lands in its folder.
        let plan = rows.iter().position(|r| r.name.as_ref() == "plan").unwrap();
        assert_eq!(zone_at(&rows, &open, 50., row_y(plan), 240.), Zone::Row { folder: "/Travel".into(), springs: false });
        // The hairline, the margins and the empty space are the list behind.
        assert_eq!(zone_at(&rows, &open, 50., ROW_HEIGHT + 0.5, 240.), Zone::Behind);
        assert_eq!(zone_at(&rows, &open, 4., row_y(0), 240.), Zone::Behind);
        assert_eq!(zone_at(&rows, &open, 235., row_y(0), 240.), Zone::Behind);
        assert_eq!(zone_at(&rows, &open, 50., 100000., 240.), Zone::Behind);
    }

    #[test]
    fn highlight_goes_straight_between_rows() {
        let mut drop = DropState::default();
        let row = |f: &str| Some(Zone::Row { folder: f.into(), springs: false });
        drop.enter_zone(row("/A"));
        assert_eq!(drop.target.as_deref(), Some("/A"));
        // Onto the next row: at once, nothing pending.
        let (settle, _) = drop.enter_zone(row("/B"));
        assert_eq!(drop.target.as_deref(), Some("/B"));
        assert_eq!(settle, None);
        // Off a row onto the list behind: waits, then takes it.
        let (settle, _) = drop.enter_zone(Some(Zone::Behind));
        assert_eq!(drop.target.as_deref(), Some("/B"));
        let ticket = settle.unwrap();
        assert!(drop.settled(ticket));
        assert_eq!(drop.target.as_deref(), Some("/"));
        // Crossing the hairline: the wait is cancelled by the next row.
        let (settle, _) = drop.enter_zone(row("/C"));
        assert!(settle.is_none());
        let (settle, _) = drop.enter_zone(Some(Zone::Behind));
        let stale = settle.unwrap();
        drop.enter_zone(row("/D"));
        assert!(!drop.settled(stale));
        assert_eq!(drop.target.as_deref(), Some("/D"));
        // Leaving altogether: turns off after the wait.
        let (settle, _) = drop.enter_zone(None);
        assert_eq!(drop.target.as_deref(), Some("/D"));
        assert!(drop.settled(settle.unwrap()));
        assert_eq!(drop.target, None);
    }

    #[test]
    fn spring_loading_needs_the_drag_to_stay() {
        let mut drop = DropState::default();
        let closed = |f: &str| Some(Zone::Row { folder: f.into(), springs: true });
        let (_, spring) = drop.enter_zone(closed("/A"));
        assert_eq!(drop.sprung(spring.unwrap()).as_deref(), Some("/A"));
        let (_, spring) = drop.enter_zone(closed("/B"));
        let stale = spring.unwrap();
        drop.enter_zone(closed("/C"));
        assert_eq!(drop.sprung(stale), None);
        // Left the folder before the time ran out.
        let (_, spring) = drop.enter_zone(closed("/D"));
        drop.enter_zone(Some(Zone::Header));
        assert_eq!(drop.sprung(spring.unwrap()), None);
    }

    #[test]
    fn move_to_lists_everywhere_else() {
        let ids: Vec<String> = ["/A", "/A/B", "/A/B/C", "/D"].iter().map(|s| s.to_string()).collect();
        assert_eq!(move_destinations(&ids, "/A", None), ["/", "/A/B", "/A/B/C", "/D"]);
        assert_eq!(move_destinations(&ids, "/", Some("/A")), ["/D"]);
        assert_eq!(move_destinations(&ids, "/A", Some("/A/B")), ["/", "/D"]);
        assert_eq!(destination_label("/", "Garden"), "Garden");
        assert_eq!(destination_label("/Travel/Japan", "Garden"), "Travel › Japan");
        assert_eq!(parent_folder("/A"), "/");
        assert_eq!(parent_folder("/A/B"), "/A");
    }

    #[test]
    fn row_menus_number_their_items_as_popup_menu_does() {
        let row = TreeRow {
            kind: RowKind::Note { pinned: true },
            id: "/L/n.md".into(),
            name: "n".into(),
            depth: 0,
            parent: "/".into(),
            path: PathBuf::from("/L/n.md"),
        };
        let ids = vec!["/A".to_string()];
        let menu = row_menu(&row, &ids, "L");
        assert_eq!(menu.actions.len(), 5);
        assert!(matches!(&menu.rows[0], MenuRow::Item { title, .. } if title == "Unpin"));
        assert!(matches!(&menu.rows[2], MenuRow::Submenu { title, rows } if title == "Move To" && rows.len() == 1));
        assert!(matches!(&menu.rows[4], MenuRow::Separator));
        assert_eq!(row_menu(&row, &ids, "L").take(2), Some(RowAction::MoveTo("/A".into())));
        assert_eq!(row_menu(&row, &ids, "L").take(4), Some(RowAction::Delete));
    }

    #[test]
    fn move_to_is_disabled_with_nowhere_to_go() {
        let row = TreeRow {
            kind: RowKind::Folder,
            id: "/A".into(),
            name: "A".into(),
            depth: 0,
            parent: "/".into(),
            path: PathBuf::from("/L/A"),
        };
        let menu = row_menu(&row, &["/A".to_string()], "L");
        assert!(matches!(&menu.rows[0], MenuRow::Item { title, enabled: false, .. } if title == "Move To"));
        assert_eq!(menu.take(0), None);
    }

    #[test]
    fn library_menu_states() {
        let enabled = |menu: Menu<LibraryAction>| {
            menu.rows
                .iter()
                .filter_map(|row| if let MenuRow::Item { enabled, .. } = row { Some(*enabled) } else { None })
                .collect::<Vec<_>>()
        };
        assert_eq!(enabled(library_menu(true, false)), [false, true, true, true, false]);
        assert_eq!(enabled(library_menu(false, true)), [false, false, true, true, true]);
        assert_eq!(enabled(library_menu(false, false)), [true, true, true, true, true]);
    }

    #[test]
    fn header_menu_shows_the_sort() {
        let menu = header_menu(true, SortMode::CreatedDesc);
        let MenuRow::Submenu { rows, .. } = &menu.rows[4] else { panic!("Sort By") };
        assert!(matches!(&rows[1], MenuRow::Choice { title, checked: true, .. } if title == "Date Created"));
        assert_eq!(header_menu(true, SortMode::UpdatedDesc).take(5), Some(HeaderAction::Sort(SortMode::TitleAsc)));
        assert_eq!(header_menu(false, SortMode::UpdatedDesc).take(2), Some(HeaderAction::Sort(SortMode::UpdatedDesc)));
    }

    #[test]
    fn labels() {
        assert_eq!(note_count_label(1), " · 1 note");
        assert_eq!(note_count_label(516), " · 516 notes");
        assert_eq!(library_tooltip("/a", true), "/a");
        assert_eq!(library_tooltip("/a", false), "/a\n⌘-click to open in a new window");
        assert!(is_emoji_presentation('📚'));
        assert!(!is_emoji_presentation('A'));
        assert!(!is_emoji_presentation('文'));
    }

    #[test]
    fn scroller_is_short_and_travels_the_full_slot() {
        assert_eq!(scroller_knob(400., 300., 0.), None);
        // 400 of 1000: true length 156, 45% of it.
        let (y, h) = scroller_knob(400., 1000., 0.).unwrap();
        assert_eq!(y, 2.);
        assert!((h - 70.2).abs() < 0.01);
        let (y, _) = scroller_knob(400., 1000., 600.).unwrap();
        assert!((y + h - 398.).abs() < 0.01);
        // At least 28, at most 88.
        assert_eq!(scroller_knob(400., 100000., 0.).unwrap().1, 28.);
        assert_eq!(scroller_knob(800., 1000., 0.).unwrap().1, 88.);
        // The thumb's way back to an offset.
        assert_eq!(scroller_offset(400., 1000., 2.), 0.);
        assert!((scroller_offset(400., 1000., 398. - h) - 600.).abs() < 0.01);
    }

    fn gesture(began: bool, dx: f64, dy: f64, inverted: bool) -> ScrollGesture {
        ScrollGesture { began, changed: !began, inverted, delta_x: dx, delta_y: dy }
    }

    #[test]
    fn swipe_right_past_60_once() {
        let mut swipe = Swipe::default();
        assert!(!swipe.feed(&gesture(true, 30., 0., true)));
        assert!(!swipe.feed(&gesture(false, 30., 0., true)));
        assert!(swipe.feed(&gesture(false, 1., 0., true)));
        assert!(!swipe.feed(&gesture(false, 100., 0., true)), "once per swipe");
        assert!(!swipe.feed(&gesture(true, 0., 0., true)));
        // Scrolling set the other way.
        assert!(swipe.feed(&gesture(false, -61., 0., false)));
    }

    #[test]
    fn swipe_needs_horizontal_dominance_and_a_phase() {
        let mut swipe = Swipe::default();
        assert!(!swipe.feed(&gesture(true, 100., 67., true)), "1.49x isn't enough");
        assert!(swipe.feed(&gesture(false, 100., 66., true)));
        let mut swipe = Swipe::default();
        // Momentum carries no phase.
        let momentum = ScrollGesture { began: false, changed: false, inverted: true, delta_x: 500., delta_y: 0. };
        assert!(!swipe.feed(&momentum));
        // Leftwards doesn't count.
        assert!(!swipe.feed(&gesture(true, -500., 0., true)));
    }
}
