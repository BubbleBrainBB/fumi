//! The sidebar's state and its frame: width, divider, toolbar button, the sliding
//! levels, dialogs. The levels themselves are in `library_level` and
//! `libraries_level`.

use crate::chrome::{multiply_shade, text_width, tip};
use crate::logic::*;
use crate::{BackToLibraries, OpenFolder, ShowLibraries, ToggleSidebar};
use fumi_core::app_model::SortMode;
use fumi_core::library_list;
use fumi_core::sidebar_width;
use fumi_ui::icons::{self, Glyph};
use fumi_ui::native;
use fumi_ui::settings;
use fumi_ui::text_field::{FieldEvent, TextField};
use fumi_ui::theme::{self, t};
use fumi_workspace::{Workspace, libraries};
use gpui::prelude::*;
use gpui::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// A note, file or folder being dragged in the app. It leaves the window as a real
/// file drag; screens other than the tree can `on_drop` it too.
#[derive(Clone, Debug)]
pub struct TreeDrag {
    pub path: PathBuf,
    pub is_folder: bool,
}

/// What only the app can do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarEvent {
    /// A new window, with this library if one is named. The sidebar has already
    /// brought forward a window that shows it, if there is one.
    OpenWindow { library: Option<PathBuf> },
}

impl EventEmitter<SidebarEvent> for SidebarView {}

/// A tween running on the wall clock.
#[derive(Clone, Copy)]
pub(crate) struct Timed {
    tween: Tween,
    start: Instant,
}

impl Timed {
    pub(crate) fn new(from: f32, to: f32, seconds: f32, curve: Curve) -> Self {
        Timed { tween: Tween { from, to, seconds, curve }, start: Instant::now() }
    }

    pub(crate) fn value(&self) -> f32 {
        self.tween.value(self.start.elapsed())
    }

    fn done(&self) -> bool {
        self.tween.is_done(self.start.elapsed())
    }
}

/// A divider being dragged.
#[derive(Clone, Copy)]
struct DividerGrab {
    /// Pointer to divider, as the drag began.
    offset: f32,
    /// Where it was, to go back to after collapsing.
    start: f32,
    last: f32,
    collapsed: bool,
}

/// Which list a scroller belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ListKind {
    Tree,
    Libraries,
}

/// A list's overlay scroller: seen while it scrolls, then gone.
#[derive(Default)]
pub(crate) struct KnobState {
    pub(crate) offset: f32,
    pub(crate) moved_at: Option<Instant>,
    pub(crate) timer: bool,
}

pub(crate) struct KnobGrab {
    pub(crate) list: ListKind,
    pub(crate) grab: f32,
    pub(crate) content: f32,
}

/// The tree's rows between two arrangements.
pub(crate) struct RowsInMotion {
    pub(crate) rows: Vec<MovingRow>,
    pub(crate) timed: Timed,
    pub(crate) from_height: f32,
}

/// What a dialog commits to.
pub(crate) enum DialogKind {
    RenameNote { id: String },
    RenameLibrary { path: String },
    ChangeIcon { path: String },
    /// Only an OK.
    Notice,
}

pub(crate) struct Dialog {
    kind: DialogKind,
    title: SharedString,
    message: SharedString,
    confirm: SharedString,
    field: Option<Entity<TextField>>,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    _sub: Option<Subscription>,
}

/// The tree as of what changes it.
#[derive(PartialEq, Clone)]
struct TreeKey {
    revision: u64,
    sort: SortMode,
    loaded: bool,
    library: Option<PathBuf>,
}

/// The left column of one window.
pub struct SidebarView {
    pub(crate) workspace: Entity<Workspace>,

    // Width. `rest_width` is where a shown sidebar sits; `width` is the way there.
    rest_width: f32,
    width: Option<Timed>,
    shown: bool,
    divider: Option<DividerGrab>,
    left: Rc<Cell<f32>>,

    // Levels. 0 shows the libraries, 1 inside the library.
    at_libraries: bool,
    slide: Option<Timed>,
    swipe: Swipe,

    // Tree.
    tree_key: Option<TreeKey>,
    pub(crate) snapshot: Option<Rc<TreeSnapshot>>,
    pub(crate) expanded: HashSet<String>,
    pub(crate) rows: Vec<Rc<TreeRow>>,
    rows_stale: bool,
    next_motion: Option<f32>,
    pub(crate) in_motion: Option<RowsInMotion>,
    selected: Option<String>,
    library_key: Option<PathBuf>,
    was_loaded: bool,
    pub(crate) tree_scroll: ScrollHandle,

    // Drag and drop over the tree.
    pub(crate) drop_state: DropState,
    shown_target: Option<String>,
    fading_target: Option<String>,
    target_change: Option<Timed>,

    // Libraries level.
    pub(crate) libraries_scroll: ScrollHandle,
    pub(crate) library_positions: Option<(Vec<String>, Timed)>,

    // Scrollers.
    pub(crate) tree_knob: KnobState,
    pub(crate) libraries_knob: KnobState,
    pub(crate) knob_grab: Option<KnobGrab>,

    pub(crate) hovered: HashSet<SharedString>,
    pub(crate) more_button: Rc<Cell<Bounds<Pixels>>>,
    scrim: f32,
    scrim_motion: Option<Timed>,
    pub(crate) dialog: Option<Dialog>,
    _subs: Vec<Subscription>,
}

thread_local! {
    /// Every window's library, so a library opens where it's already open.
    static WINDOWS: RefCell<Vec<(AnyWindowHandle, WeakEntity<Workspace>)>> = const { RefCell::new(Vec::new()) };
}

/// Brings forward the window showing `library`, other than `except`'s. False when
/// there is none. `LibraryWindows.bringForward`.
fn bring_forward(library: &str, except: Option<EntityId>, cx: &mut App) -> bool {
    let windows: Vec<_> = WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        windows.retain(|(_, workspace)| workspace.upgrade().is_some());
        windows.clone()
    });
    for (handle, workspace) in windows {
        let Some(workspace) = workspace.upgrade() else { continue };
        if Some(workspace.entity_id()) == except {
            continue;
        }
        let shows = workspace
            .read(cx)
            .workspace_path()
            .is_some_and(|path| library_list::standardized_path(&path.to_string_lossy()) == library);
        if shows && handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            return true;
        }
    }
    false
}

/// Adds the sidebar's commands to root's element, so they work wherever focus is.
pub fn wire_actions<E: InteractiveElement>(sidebar: &Entity<SidebarView>, element: E) -> E {
    let (back, toggle, libraries, open) = (sidebar.clone(), sidebar.clone(), sidebar.clone(), sidebar.clone());
    element
        .on_action(move |_: &BackToLibraries, _, cx| back.update(cx, |this, cx| this.back_to_libraries(cx)))
        .on_action(move |_: &ToggleSidebar, _, cx| toggle.update(cx, |this, cx| this.toggle_sidebar(cx)))
        .on_action(move |_: &ShowLibraries, _, cx| libraries.update(cx, |this, cx| this.show_libraries(cx)))
        .on_action(move |_: &OpenFolder, window, cx| open.update(cx, |this, cx| this.open_folder(window, cx)))
}

impl SidebarView {
    pub fn new(workspace: Entity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        WINDOWS.with(|windows| windows.borrow_mut().push((window.window_handle(), workspace.downgrade())));
        let (shown, at_libraries) = {
            let model = &workspace.read(cx).model;
            (model.sidebar_visible, model.shows_libraries)
        };
        let subs = vec![cx.observe(&workspace, |_, _, cx| cx.notify())];
        SidebarView {
            workspace,
            rest_width: sidebar_width::STANDARD_DEFAULT as f32,
            width: None,
            shown,
            divider: None,
            left: Rc::new(Cell::new(0.)),
            at_libraries,
            slide: None,
            swipe: Swipe::default(),
            tree_key: None,
            snapshot: None,
            expanded: HashSet::new(),
            rows: Vec::new(),
            rows_stale: false,
            next_motion: None,
            in_motion: None,
            selected: None,
            library_key: None,
            was_loaded: false,
            tree_scroll: ScrollHandle::new(),
            drop_state: DropState::default(),
            shown_target: None,
            fading_target: None,
            target_change: None,
            libraries_scroll: ScrollHandle::new(),
            library_positions: None,
            tree_knob: KnobState::default(),
            libraries_knob: KnobState::default(),
            knob_grab: None,
            hovered: HashSet::new(),
            more_button: Rc::new(Cell::new(Bounds::default())),
            scrim: 0.,
            scrim_motion: None,
            dialog: None,
            _subs: subs,
        }
    }

    /// The column's width right now: 0 collapsed, in between while it animates.
    pub fn current_width(&self) -> f32 {
        match &self.width {
            Some(timed) => timed.value(),
            None if self.shown => self.rest_width,
            None => 0.,
        }
    }

    // MARK: Commands

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.model.toggle_sidebar();
            w.navigated(cx);
        });
    }

    /// ⌘↑, or the row, or the swipe. Only while it would show something.
    pub fn back_to_libraries(&mut self, cx: &mut Context<Self>) {
        let active = {
            let model = &self.workspace.read(cx).model;
            !model.shows_libraries && model.sidebar_visible
        };
        if !active {
            cx.propagate();
            return;
        }
        self.go_to_libraries(cx);
    }

    pub(crate) fn go_to_libraries(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.model.shows_libraries = true;
            w.navigated(cx);
        });
    }

    /// The sidebar's first level, showing the sidebar first if it's hidden.
    pub fn show_libraries(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if !w.model.sidebar_visible {
                w.model.toggle_sidebar();
            }
            w.model.shows_libraries = true;
            w.navigated(cx);
        });
    }

    /// The Open Notes Folder panel, then in. A panel spins its own run loop, so it
    /// runs outside any GPUI borrow.
    pub fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) = native::choose_notes_folder(None) else { return };
            let _ = this.update_in(cx, |this, _, cx| this.enter_library(&path, cx));
        })
        .detach();
    }

    /// Makes `path` this window's library and shows its files. One already open in
    /// another window brings that window forward; one that can't be found shows the
    /// missing-folder page. `AppModel.enterLibrary`.
    pub fn enter_library(&mut self, path: &Path, cx: &mut Context<Self>) {
        let wanted = library_list::standardized_path(&path.to_string_lossy());
        let current = self
            .workspace
            .read(cx)
            .workspace_path()
            .map(|current| library_list::standardized_path(&current.to_string_lossy()));
        if current.as_deref() == Some(wanted.as_str()) {
            self.workspace.update(cx, |w, cx| {
                w.model.shows_libraries = false;
                w.navigated(cx);
            });
            return;
        }
        if library_list::missing_reason_on_disk(&wanted).is_some() {
            let path = path.to_path_buf();
            self.workspace.update(cx, |w, cx| {
                w.close_workspace(cx);
                w.store.missing_workspace = Some(path);
                w.model.shows_libraries = false;
                w.navigated(cx);
            });
            return;
        }
        if bring_forward(&wanted, Some(self.workspace.entity_id()), cx) {
            return;
        }
        let path = path.to_path_buf();
        self.workspace.update(cx, |w, cx| {
            w.open_workspace(&path, cx);
            w.model.shows_libraries = false;
            w.navigated(cx);
        });
        cx.refresh_windows();
    }

    /// `path` in a window of its own: the one already showing it, or a new one.
    pub(crate) fn open_in_window(&mut self, path: &str, cx: &mut Context<Self>) {
        let library = library_list::standardized_path(path);
        if !bring_forward(&library, None, cx) {
            cx.emit(SidebarEvent::OpenWindow { library: Some(PathBuf::from(path)) });
        }
    }

    // MARK: Staying in step with the workspace

    fn sync(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        let w = workspace.read(cx);

        // Toggled from anywhere: the button, the menu, a shortcut.
        if w.model.sidebar_visible != self.shown {
            let from = self.current_width();
            self.shown = w.model.sidebar_visible;
            if self.divider.is_none() {
                let to = if self.shown { self.rest_width } else { 0. };
                self.width = Some(Timed::new(from, to, 0.2, Curve::EaseInOut));
            }
        }

        // Going in or back.
        if w.model.shows_libraries != self.at_libraries {
            let from = self.slide.as_ref().map_or(if self.at_libraries { 0. } else { 1. }, Timed::value);
            self.at_libraries = w.model.shows_libraries;
            self.slide = Some(Timed::new(from, if self.at_libraries { 0. } else { 1. }, 0.25, Curve::EaseInOut));
        }

        // Behind Settings and search, the button dims with the window.
        let scrim = w.model.scrim_strength(t().glass);
        if self.scrim_motion.is_none_or(|m| m.tween.to != scrim) && self.scrim != scrim {
            let from = self.scrim_motion.as_ref().map_or(self.scrim, Timed::value);
            self.scrim_motion = Some(Timed::new(from, scrim, 0.16, Curve::EaseOut));
        }

        // Another library: its own folders, closed, and its own width.
        let library = w.workspace_path();
        let library_changed = library != self.library_key;
        if library_changed {
            let first = self.library_key.is_none() && self.tree_key.is_none();
            self.library_key = library.clone();
            self.expanded.clear();
            self.rows_stale = true;
            self.next_motion = None;
            if let Some(path) = &library {
                let key = width_key(&path.to_string_lossy());
                let saved = settings::has(&key).then(|| settings::get_f64(&key, 0.));
                let width = saved_width(saved);
                let from = self.current_width();
                self.rest_width = width;
                if self.shown && !first && (from - width).abs() > 0.5 {
                    self.width = Some(Timed::new(from, width, 0.2, Curve::EaseInOut));
                }
            }
        }

        // The tree, only when something it's made of changed.
        let key = TreeKey {
            revision: w.revision,
            sort: w.model.sort_mode,
            loaded: w.store.has_loaded_notes(),
            library: library.clone(),
        };
        if self.tree_key.as_ref() != Some(&key) {
            self.snapshot = library.as_ref().map(|path| Rc::new(TreeSnapshot::build(w.store.notes(), w.store.files(), key.sort, path)));
            self.tree_key = Some(key);
            self.rows_stale = true;
        }

        // The selected note's folders open whenever the selection changes.
        let selected = w.model.selected_note();
        let loaded = w.store.has_loaded_notes();
        if selected != self.selected.as_deref() || library_changed || (loaded && !self.was_loaded) {
            self.selected = selected.map(str::to_string);
            if let Some(note) = selected.and_then(|id| w.store.note(id)) {
                let before = self.expanded.len();
                self.expanded.extend(fumi_core::note_tree::ancestor_ids(&note.relative_path));
                self.rows_stale |= self.expanded.len() != before;
            }
        }
        self.was_loaded = loaded;

        if self.rows_stale {
            self.rows_stale = false;
            self.apply_rows();
        }
    }

    /// Flattens the tree for the folders open now, and takes up the motion asked for.
    fn apply_rows(&mut self) {
        let rows = self.snapshot.as_ref().map(|tree| tree.flatten(&self.expanded)).unwrap_or_default();
        let seconds = self.next_motion.take();
        let Some(seconds) = seconds.filter(|_| !self.rows.is_empty() || !rows.is_empty()) else {
            self.rows = rows;
            self.in_motion = None;
            return;
        };
        let (current, from_height): (Vec<(Rc<TreeRow>, f32, f32)>, f32) = match &self.in_motion {
            Some(moving) => {
                let e = moving.timed.value();
                let on_screen =
                    moving.rows.iter().map(|m| (m.row.clone(), m.at(e).0, m.at(e).1)).filter(|(_, _, alpha)| *alpha > 0.).collect();
                let height = moving.from_height + (content_height(self.rows.len() as f32) - moving.from_height) * e;
                (on_screen, height)
            }
            None => (
                self.rows.iter().enumerate().map(|(i, row)| (row.clone(), i as f32 * ROW_PITCH, 1.)).collect(),
                content_height(self.rows.len() as f32),
            ),
        };
        self.in_motion = Some(RowsInMotion {
            rows: motion(&current, &rows),
            timed: Timed::new(0., 1., seconds, Curve::EaseInOut),
            from_height,
        });
        self.rows = rows;
    }

    /// The tree after a folder opened or closed, moving rows over `seconds`.
    pub(crate) fn rows_changed(&mut self, seconds: f32, cx: &mut Context<Self>) {
        self.next_motion = Some(seconds);
        self.rows_stale = true;
        cx.notify();
    }

    pub(crate) fn expand_with_motion(&mut self, folders: impl IntoIterator<Item = String>, seconds: f32, cx: &mut Context<Self>) {
        let before = self.expanded.len();
        self.expanded.extend(folders);
        if self.expanded.len() != before {
            self.rows_changed(seconds, cx);
        }
    }

    pub(crate) fn toggle_folder(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(id) {
            self.expanded.insert(id.to_string());
        }
        self.rows_changed(0.15, cx);
    }

    // MARK: Animations

    /// Finishes what's done, and asks for another frame if anything isn't.
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut running = false;
        if self.width.is_some_and(|timed| timed.done()) {
            self.width = None;
            // Root reads the width as it renders, just before this; one more frame lets it
            // see the settled value instead of the last step of the tween.
            window.request_animation_frame();
        }
        running |= self.width.is_some();
        if self.slide.is_some_and(|timed| timed.done()) {
            self.slide = None;
        }
        running |= self.slide.is_some();
        if self.in_motion.as_ref().is_some_and(|moving| moving.timed.done()) {
            self.in_motion = None;
        }
        running |= self.in_motion.is_some();
        if self.target_change.is_some_and(|timed| timed.done()) {
            self.target_change = None;
            self.fading_target = None;
        }
        running |= self.target_change.is_some();
        if self.library_positions.as_ref().is_some_and(|(_, timed)| timed.done()) {
            self.library_positions = None;
        }
        running |= self.library_positions.is_some();
        if let Some(motion) = self.scrim_motion {
            self.scrim = motion.value();
            if motion.done() {
                self.scrim_motion = None;
            } else {
                running = true;
            }
        }

        // A drag let go anywhere: the highlight goes with it.
        if !cx.has_active_drag() && (self.drop_state.zone().is_some() || self.drop_state.target.is_some()) {
            self.drop_state.reset();
        }
        if self.drop_state.target != self.shown_target {
            self.fading_target = self.shown_target.take();
            self.shown_target = self.drop_state.target.clone();
            self.target_change = Some(Timed::new(0., 1., 0.12, Curve::EaseOut));
            running = true;
        }

        if running {
            window.request_animation_frame();
        }
    }

    /// How far the drop highlight on `folder` is showing, 0 to 1.
    pub(crate) fn highlight(&self, folder: &str) -> f32 {
        let e = self.target_change.as_ref().map_or(1., Timed::value);
        if self.shown_target.as_deref() == Some(folder) {
            e
        } else if self.fading_target.as_deref() == Some(folder) {
            1. - e
        } else {
            0.
        }
    }

    // MARK: Width

    fn save_width(&self, width: f32, cx: &App) {
        if let Some(path) = self.workspace.read(cx).workspace_path() {
            settings::set_f64(&width_key(&path.to_string_lossy()), width as f64);
        }
    }

    fn animate_width_to(&mut self, width: f32, save: bool, cx: &mut Context<Self>) {
        let from = self.current_width();
        self.rest_width = width;
        if (from - width).abs() > 0.5 {
            self.width = Some(Timed::new(from, width, 0.2, Curve::EaseInOut));
        }
        if save {
            self.save_width(width, cx);
        }
        cx.notify();
    }

    /// The natural width of the widest note row.
    fn longest_row_width(&self, window: &Window, cx: &App) -> f32 {
        self.workspace.read(cx).store.notes().iter().fold(sidebar_width::MINIMUM as f32, |widest, note| {
            let name = text_width(&note.filename(), 13., FontWeight::SEMIBOLD, window);
            widest.max(fit_row_width(folder_depth(&note.relative_path), name, note.pinned))
        })
    }

    fn divider_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let width = self.current_width();
        if event.click_count == 2 {
            let target = sidebar_width::double_click_target(width as f64, self.longest_row_width(window, cx) as f64);
            self.divider = None;
            self.animate_width_to(target as f32, true, cx);
            return;
        }
        self.width = None;
        self.rest_width = width;
        let x = f32::from(event.position.x) - self.left.get();
        self.divider = Some(DividerGrab { offset: x - width, start: width, last: width, collapsed: false });
        cx.notify();
    }

    fn pointer_moved(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(mut grab) = self.divider {
            let x = f32::from(position.x) - self.left.get();
            match divider_drag(x - grab.offset) {
                DividerDrag::Collapsed => {
                    if !grab.collapsed {
                        grab.collapsed = true;
                        self.rest_width = grab.start;
                        self.set_shown(false, cx);
                    }
                }
                DividerDrag::Width(width) => {
                    if grab.collapsed {
                        grab.collapsed = false;
                        self.set_shown(true, cx);
                    }
                    if sidebar_width::crossed_snap_point(grab.last as f64, width as f64).is_some() {
                        native::haptic_alignment();
                    }
                    grab.last = width;
                    self.rest_width = width;
                }
            }
            self.divider = Some(grab);
            cx.notify();
        }
        if let Some(knob) = &self.knob_grab {
            let (handle, y) = match knob.list {
                ListKind::Tree => (self.tree_scroll.clone(), f32::from(position.y) - knob.grab),
                ListKind::Libraries => (self.libraries_scroll.clone(), f32::from(position.y) - knob.grab),
            };
            let bounds = handle.bounds();
            let offset = scroller_offset(f32::from(bounds.size.height), knob.content, y - f32::from(bounds.origin.y));
            handle.set_offset(point(handle.offset().x, px(-offset)));
            cx.notify();
        }
    }

    fn pointer_up(&mut self, cx: &mut Context<Self>) {
        self.knob_grab = None;
        if let Some(grab) = self.divider.take() {
            if !grab.collapsed {
                self.animate_width_to(sidebar_width::settle(self.rest_width as f64) as f32, true, cx);
            }
            cx.notify();
        }
    }

    /// Dragging past the minimum collapses the column without going through the
    /// button; this keeps the model, and so the button, in step.
    fn set_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.shown = shown;
        self.width = None;
        self.workspace.update(cx, |w, cx| {
            w.model.sidebar_visible = shown;
            w.navigated(cx);
        });
    }

    // MARK: Dialogs

    pub(crate) fn open_dialog(
        &mut self,
        kind: DialogKind,
        title: &str,
        message: &str,
        confirm: &str,
        field: Option<(&str, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let restore = window.focused(cx);
        let focus = cx.focus_handle();
        let (field, sub) = match field {
            Some((placeholder, text)) => {
                let field = cx.new(|cx| {
                    let mut field = TextField::new(placeholder.to_string(), window, cx);
                    field.set_text(text, cx);
                    field.select_all(cx);
                    field
                });
                field.read(cx).focus.clone().focus(window, cx);
                let sub = cx.subscribe_in(&field, window, |this, _, event: &FieldEvent, window, cx| match event {
                    FieldEvent::Submit => this.commit_dialog(window, cx),
                    FieldEvent::Cancel => this.close_dialog(window, cx),
                    _ => {}
                });
                (Some(field), Some(sub))
            }
            None => {
                focus.focus(window, cx);
                (None, None)
            }
        };
        self.dialog = Some(Dialog {
            kind,
            title: title.to_string().into(),
            message: message.to_string().into(),
            confirm: confirm.to_string().into(),
            field,
            focus,
            restore,
            _sub: sub,
        });
        cx.notify();
    }

    fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = self.dialog.take() {
            if let Some(restore) = dialog.restore {
                restore.focus(window, cx);
            }
        }
        cx.notify();
    }

    fn commit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialog else { return };
        let text = dialog.field.as_ref().map(|field| field.read(cx).text().to_string()).unwrap_or_default();
        match &dialog.kind {
            DialogKind::RenameNote { id } => {
                let id = id.clone();
                self.workspace.update(cx, |w, cx| {
                    if let Some(new_id) = w.store.rename_note(&id, &text, fumi_workspace::now()) {
                        w.model.remap_tabs(&id, &new_id);
                        w.library_changed(cx);
                        w.navigated(cx);
                    }
                });
            }
            DialogKind::RenameLibrary { path } => {
                libraries::with(|list| list.rename(path, &text));
                cx.refresh_windows();
            }
            DialogKind::ChangeIcon { path } => {
                libraries::with(|list| list.set_icon(path, &text));
                cx.refresh_windows();
            }
            DialogKind::Notice => {}
        }
        self.close_dialog(window, cx);
    }

    fn render_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.dialog.as_ref()?;
        let c = t();
        let size = window.viewport_size();
        let cancellable = dialog.field.is_some();
        let button = |id: &'static str, label: SharedString, primary: bool| {
            div()
                .id(id)
                .flex_1()
                .h(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .text_size(px(13.))
                .map(|el| {
                    if primary {
                        el.bg(c.accent).text_color(white()).font_weight(FontWeight::MEDIUM)
                    } else {
                        el.bg(c.quiet_surface).border_1().border_color(c.control_border).text_color(c.text)
                    }
                })
                .child(label)
        };
        let buttons = div()
            .flex()
            .gap(px(8.))
            .when(cancellable, |row| {
                row.child(
                    button("dialog-cancel", "Cancel".into(), false)
                        .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
                )
            })
            .child(
                button("dialog-confirm", dialog.confirm.clone(), true)
                    .on_click(cx.listener(|this, _, window, cx| this.commit_dialog(window, cx))),
            );
        let panel = div()
            .id("dialog")
            .w(px(280.))
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .rounded(px(12.))
            .bg(c.page)
            .border_1()
            .border_color(c.page_edge)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.25),
                offset: point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
                inset: false,
            }])
            .track_focus(&dialog.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // A dialog with a field takes these from the field.
                if this.dialog.as_ref().is_some_and(|dialog| dialog.field.is_none()) {
                    match event.keystroke.key.as_str() {
                        "enter" | "escape" => this.commit_dialog(window, cx),
                        _ => {}
                    }
                }
            }))
            .child(
                div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(c.text).text_center().child(dialog.title.clone()),
            )
            .when(!dialog.message.is_empty(), |panel| {
                panel.child(div().text_size(px(12.)).text_color(c.text2).text_center().child(dialog.message.clone()))
            })
            .when_some(dialog.field.clone(), |panel, field| {
                panel.child(
                    div()
                        .px(px(6.))
                        .py(px(3.))
                        .rounded(px(5.))
                        .bg(c.page)
                        .border_1()
                        .border_color(c.control_border)
                        .text_size(px(13.))
                        .text_color(c.text)
                        .child(field),
                )
            })
            .child(buttons);
        Some(
            deferred(
                anchored().position(point(px(0.), px(0.))).child(
                    div()
                        .id("dialog-backdrop")
                        .w(size.width)
                        .h(size.height)
                        .occlude()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(panel),
                ),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }

    // MARK: Frame

    /// Where the pointer goes while the divider or a scroller is held, inside or
    /// outside the window.
    fn pointer_capture(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        if self.divider.is_none() && self.knob_grab.is_none() {
            return None;
        }
        let this = cx.entity();
        let resizing = self.divider.is_some();
        Some(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    if resizing {
                        window.set_window_cursor_style(CursorStyle::ResizeColumn);
                    }
                    let moved = this.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            moved.update(cx, |this, cx| this.pointer_moved(event.position, cx));
                        }
                    });
                    let released = this.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            released.update(cx, |this, cx| this.pointer_up(cx));
                        }
                    });
                },
            )
            .absolute()
            .size_0(),
        )
    }

    /// 5 pt either side of the column's edge. Drawn last, so it wins over the rows
    /// and the page. Nothing to grab while collapsed.
    fn divider(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let left = self.left.clone();
        deferred(
            div()
                .id("sidebar-divider")
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(width - DIVIDER_REACH))
                .w(px(DIVIDER_REACH * 2.))
                .occlude()
                .cursor(CursorStyle::ResizeColumn)
                .child(canvas(move |bounds, _, _| left.set(f32::from(bounds.origin.x) + DIVIDER_REACH - width), |_, _, _, _| {}).absolute().size_full())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        window.prevent_default();
                        this.divider_down(event, window, cx);
                    }),
                ),
        )
    }

    /// The hide button: one item, on the traffic lights' line, whether the column
    /// is there or not.
    fn toggle_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let shade = theme::scrim_shade(self.scrim, c.dark);
        let help = if self.shown { "Hide Sidebar (⌥⌘S)" } else { "Show Sidebar (⌥⌘S)" };
        deferred(
            div()
                .id("sidebar-toggle")
                .absolute()
                .left(px(TOGGLE_LEFT))
                .top(px((BAND_HEIGHT - TOGGLE_SIZE) / 2.))
                .w(px(TOGGLE_SIZE))
                .h(px(TOGGLE_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .occlude()
                .hover(move |s| s.bg(c.card_hover))
                .child(icons::glyph(Glyph::Sidebar, 18., multiply_shade(c.icon, shade)))
                .tooltip(tip(help))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
        )
    }

    /// Going back by swiping over the list, with a two-finger swipe to the right.
    pub(crate) fn swipe_catcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                window.on_mouse_event(move |event: &ScrollEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture || !bounds.contains(&event.position) {
                        return;
                    }
                    let Some(gesture) = native::current_scroll_gesture() else { return };
                    this.update(cx, |this, cx| {
                        if this.swipe.feed(&gesture) {
                            this.back_to_libraries(cx);
                        }
                    });
                });
            },
        )
        .absolute()
        .size_full()
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync(cx);
        self.tick(window, cx);

        let width = self.current_width();
        // The column narrows over its content, which keeps its own width.
        let level_width = if self.divider.is_some() { width } else { self.rest_width.max(width) };
        let slide = self.slide.as_ref().map_or(if self.at_libraries { 0. } else { 1. }, Timed::value);
        let moving = self.slide.is_some();

        let levels = div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(BAND_HEIGHT))
            .bottom_0()
            .overflow_hidden()
            .when(self.at_libraries || moving, |levels| {
                let level = self.render_libraries_level(window, cx);
                levels.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(-level_width * slide))
                        .w(px(level_width))
                        .child(level)
                        // The hidden level takes no clicks.
                        .when(!self.at_libraries, |level| level.child(div().id("libraries-cover").absolute().size_full().occlude())),
                )
            })
            .when(!self.at_libraries || moving, |levels| {
                let level = self.render_library_level(window, cx);
                levels.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(level_width * (1. - slide)))
                        .w(px(level_width))
                        .child(level)
                        .when(self.at_libraries, |level| level.child(div().id("library-cover").absolute().size_full().occlude())),
                )
            });

        let dialog = self.render_dialog(window, cx);
        let capture = self.pointer_capture(cx);
        // Drawn last, it would stand in the way of search and Settings over it.
        let scrimmed = self.scrim > 0. || self.workspace.read(cx).model.scrim_strength(t().glass) > 0.;
        div()
            .id("sidebar")
            .relative()
            .flex_none()
            .h_full()
            .w(px(width))
            // Clicking here leaves the keyboard where it is, in the editor, say.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .when(width > 0., |column| {
                column
                    .child(div().absolute().left_0().right_0().top_0().h(px(BAND_HEIGHT)).window_control_area(WindowControlArea::Drag))
                    .child(levels)
            })
            .when(self.shown && width > 0. && !scrimmed, |column| column.child(self.divider(width, cx)))
            .child(self.toggle_button(cx))
            .children(capture)
            .children(dialog)
    }
}

/// How long the overlay scroller stays once a list stops moving, and its fade.
pub(crate) const KNOB_HOLD: Duration = Duration::from_millis(800);
pub(crate) const KNOB_FADE: Duration = Duration::from_millis(200);
