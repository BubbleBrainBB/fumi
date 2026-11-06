//! One window's state: its library and its navigation. Every screen in the
//! window reads this entity and redraws when it notifies. The Swift app's
//! `FileNoteStore` and `AppModel` pair, per window.

use fumi_core::app_model::{AppModel, SortMode};
use fumi_core::note::Note;
use fumi_core::note_store::{NoteStore, Now};
use fumi_core::tabs::{self, Content, SavedTabs};
use fumi_core::wiki_link::WikiLinkIndex;
use fumi_ui::fsevents::Watcher;
use fumi_ui::settings;
use gpui::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub mod libraries;

pub fn now() -> Now {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    Now::new(secs, local_utc_offset(secs as i64))
}

/// Seconds east of UTC here, at `secs`.
pub fn local_utc_offset(secs: i64) -> i32 {
    let mut out: libc::tm = unsafe { std::mem::zeroed() };
    let time = secs as libc::time_t;
    unsafe { libc::localtime_r(&time, &mut out) };
    out.tm_gmtoff as i32
}

/// A fresh tab id. Only needs to be unique among saved tabs.
pub fn new_tab_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let pid = std::process::id();
    format!("{:08X}-{:04X}-4{:03X}-8{:03X}-{:012X}", (t >> 32) as u32, (t >> 16) as u16, pid & 0xFFF, n & 0xFFF, t & 0xFFFF_FFFF_FFFF)
}

/// What a new window opens on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowStart {
    /// The first window of a launch: the library last open, or the page saying
    /// it's missing.
    Restore,
    /// A library asked for, as by Open in New Window.
    Library(PathBuf),
    /// The welcome page: what a new window shows with no library requested.
    Empty,
}

/// What screens may want to react to beyond a redraw.
#[derive(Clone, Debug)]
pub enum WorkspaceEvent {
    /// The library's notes changed on disk or in the app.
    LibraryChanged,
    /// A workspace was opened or closed.
    WorkspaceChanged,
}

impl EventEmitter<WorkspaceEvent> for Workspace {}

pub struct Workspace {
    pub store: NoteStore,
    pub model: AppModel,
    pub links: Rc<WikiLinkIndex>,
    watcher: Option<Watcher>,
    /// Woken by the watcher after each batch of file events.
    wake: futures::channel::mpsc::UnboundedSender<()>,
    /// Bumps when the store's notes change, for views that cache from them.
    pub revision: u64,
    /// Read-only copy of `store.missing_workspace`, which is the one to write. Kept
    /// in step by `navigated`, `open_workspace` and `close_workspace`.
    pub missing_workspace: Option<PathBuf>,
}

impl Workspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::starting(WindowStart::Restore, cx)
    }

    pub fn starting(start: WindowStart, cx: &mut Context<Self>) -> Self {
        let mut model = AppModel::new(new_tab_id);
        model.sort_mode = SortMode::from_raw(&settings::get_string(settings::keys::SORT_MODE).unwrap_or_default());
        let (wake, mut wake_rx) = futures::channel::mpsc::unbounded();
        let mut this = Workspace {
            store: NoteStore::new(),
            model,
            links: Rc::new(WikiLinkIndex::new()),
            watcher: None,
            wake,
            revision: 0,
            missing_workspace: None,
        };
        match start {
            WindowStart::Restore => {
                // A dev override, so tests never touch the library in Settings.
                let last = std::env::var("FUMI_WORKSPACE").ok().unwrap_or_else(settings::workspace_path);
                if !last.is_empty() {
                    let path = PathBuf::from(last);
                    if path.exists() {
                        this.open_workspace(&path, cx);
                    } else {
                        this.store.missing_workspace = Some(path);
                        this.missing_workspace = this.store.missing_workspace.clone();
                    }
                }
            }
            WindowStart::Library(path) => this.open_workspace(&path, cx),
            WindowStart::Empty => {}
        }
        // Reloads are debounced by the store; this drains the watcher and runs
        // what's due.
        // Sleeps until the watcher reports or a debounced reload falls due: an
        // idle window doesn't wake for nothing.
        cx.spawn(async move |this: WeakEntity<Workspace>, cx| {
            use futures::{FutureExt, StreamExt};
            loop {
                let Ok(due) = this.update(cx, |w, _| w.store.reload_deadline()) else { break };
                let wait = match due {
                    Some(at) => Duration::from_secs_f64((at - now().secs).clamp(0., 1.)),
                    // A reload scheduled in the app (a new note, a move) is
                    // seen within a second.
                    None => Duration::from_secs(1),
                };
                let timer = cx.background_executor().timer(wait).fuse();
                futures::pin_mut!(timer);
                futures::select! {
                    woke = wake_rx.next() => if woke.is_none() { break },
                    _ = timer => {}
                }
                if this.update(cx, |w, cx| w.poll_files(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        this
    }

    fn poll_files(&mut self, cx: &mut Context<Self>) {
        let t = now().secs;
        let mut batches = Vec::new();
        if let Some(watcher) = &self.watcher {
            while let Ok(paths) = watcher.events.try_recv() {
                batches.push(paths);
            }
        }
        for paths in batches {
            self.store.handle_file_events(&paths, t);
        }
        if self.store.reload_is_due(t) {
            self.reload(cx);
        }
    }

    /// Scans off the main thread and applies the result.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(ticket) = self.store.begin_reload() else { return };
        cx.spawn(async move |this: WeakEntity<Workspace>, cx| {
            let scan_ticket = ticket.clone();
            let scan = cx.background_executor().spawn(async move { scan_ticket.scan(now().secs) }).await;
            let _ = this.update(cx, |w, cx| {
                let was_loaded = w.store.has_loaded_notes();
                let change = w.store.apply_reload(&ticket, scan);
                // The first scan counts even if it found nothing: tabs of notes
                // gone since last time close now.
                let first_scan = !was_loaded && w.store.has_loaded_notes();
                if change.applied && (change.notes_changed || change.files_changed || first_scan) {
                    w.library_changed(cx);
                } else {
                    // An empty library still has to say it finished loading.
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// After anything changes the notes: index, tabs, and everyone watching.
    pub fn library_changed(&mut self, cx: &mut Context<Self>) {
        self.revision += 1;
        self.rebuild_links();
        let (notes, files): (std::collections::HashSet<String>, std::collections::HashSet<String>) = (
            self.store.notes().iter().map(|n| n.id.clone()).collect(),
            self.store.files().iter().map(|f| f.id.clone()).collect(),
        );
        if self.store.has_loaded_notes() {
            self.model.prune_tabs(|c| match c {
                Content::Note(id) => notes.contains(id),
                Content::File(id) => files.contains(id),
                _ => true,
            });
        }
        if self.store.take_pins_changed() {
            self.save_pins();
        }
        self.save_tabs();
        cx.emit(WorkspaceEvent::LibraryChanged);
        cx.notify();
    }

    fn rebuild_links(&mut self) {
        let mut index = WikiLinkIndex::new();
        let notes: Vec<(&str, &Path, &str)> = self
            .store
            .notes()
            .iter()
            .map(|n| (n.id.as_str(), n.path.as_path(), n.title.as_str()))
            .collect();
        index.update(&notes, self.store.workspace());
        self.links = Rc::new(index);
    }

    pub fn open_workspace(&mut self, path: &Path, cx: &mut Context<Self>) {
        let key = path.to_string_lossy().into_owned();
        // Only the library opened on purpose becomes the remembered one, not a
        // dev override.
        if std::env::var("FUMI_WORKSPACE").is_err() {
            settings::set_string(settings::keys::WORKSPACE, &key);
            settings::note_recent_workspace(&key);
        }
        libraries::with(|list| list.add(&key));
        // Every window's library list reads it.
        cx.refresh_windows();
        self.store.open_workspace(path, load_pins(&key));
        self.missing_workspace = None;
        self.watcher = Watcher::with_wake(path, Some(self.wake.clone()));
        self.model.restore_tabs(&key, load_saved_tabs(&key));
        self.reload(cx);
        cx.emit(WorkspaceEvent::WorkspaceChanged);
        cx.notify();
    }

    pub fn close_workspace(&mut self, cx: &mut Context<Self>) {
        self.watcher = None;
        self.store.close_workspace();
        self.missing_workspace = self.store.missing_workspace.clone();
        cx.emit(WorkspaceEvent::WorkspaceChanged);
        cx.notify();
    }

    pub fn workspace_path(&self) -> Option<PathBuf> {
        self.store.workspace().map(Path::to_path_buf)
    }

    pub fn save_tabs(&self) {
        let Some(path) = self.store.workspace() else { return };
        let key = path.to_string_lossy();
        if let Some(data) = self.model.saved_tabs(&key) {
            save_tabs_data(&key, &data);
        }
    }

    fn save_pins(&self) {
        let Some(path) = self.store.workspace() else { return };
        settings::set_strings(&pins_key(&path.to_string_lossy()), &self.store.pinned_paths());
    }

    pub fn note(&self, id: &str) -> Option<&Note> {
        self.store.note(id)
    }

    /// Redraw and remember tabs after navigating.
    pub fn navigated(&mut self, cx: &mut Context<Self>) {
        self.missing_workspace = self.store.missing_workspace.clone();
        self.save_tabs();
        cx.notify();
    }
}

fn pins_key(_workspace: &str) -> String {
    // The Swift app keeps pinned files under one key, by path.
    "fumi.pinnedFiles".to_string()
}

fn load_pins(workspace: &str) -> Vec<String> {
    settings::get_strings(&pins_key(workspace)).unwrap_or_default()
}

fn load_saved_tabs(workspace: &str) -> Option<SavedTabs> {
    settings::get_dict_data(tabs::KEY, workspace).and_then(|d| SavedTabs::decode(&d))
}

fn save_tabs_data(workspace: &str, data: &[u8]) {
    settings::set_dict_data(tabs::KEY, workspace, data);
}
