//! The notes and files of an open library, and everything that writes them.
//! Port of `Services/FileNoteStore.swift`.
//!
//! Plain data and plain methods: no observers, no watcher, no timers. Whoever
//! holds the store reacts to what the methods return. Dates are epoch seconds
//! and "now" is an argument ([`Now`]), so all of it runs in a test.
//!
//! **Watching.** A file-system watcher can be added on top without changing this
//! module. It passes event paths to [`NoteStore::handle_file_events`], which drops
//! the echoes of our own saves and arms the debounce, and polls
//! [`NoteStore::reload_is_due`] to know when to rescan. A rescan is either
//! [`NoteStore::reload`], or, off the UI thread, [`NoteStore::begin_reload`],
//! [`ReloadTicket::scan`] and [`NoteStore::apply_reload`].
//!
//! **Bytes.** A note's text is held without its UTF-8 BOM, as Foundation reads
//! it, and the BOM goes back on when the file is written. A file that isn't
//! UTF-8 is listed with empty text and is never overwritten ([`SaveError::NotUtf8`]).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::external_sync;
use crate::library_list::{is_whitespace, is_whitespace_or_newline, last_path_component};
use crate::library_move::{self, Refusal};
use crate::link_rewrite;
use crate::memo_format::{self, Memo};
use crate::merge3;
use crate::note::{LibraryFile, Note};
use crate::tags;
use crate::title;

/// What a draft starts as: an empty title heading, the caret after it.
pub const DRAFT_CONTENT: &str = "# ";
/// How long after the last change a rescan waits, in seconds.
pub const RELOAD_DEBOUNCE: f64 = 0.5;
/// How long a write of ours is recognized as ours, in seconds.
pub const OWN_WRITE_LIFETIME: f64 = 5.0;
/// How many times a save starts over when the file keeps changing under it.
const SAVE_ATTEMPTS: usize = 4;

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The current moment: epoch seconds, and the local time's offset from UTC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Now {
    pub secs: f64,
    pub utc_offset: i32,
}

impl Now {
    pub fn new(secs: f64, utc_offset: i32) -> Self {
        Self { secs, utc_offset }
    }
}

// MARK: - Reading files

/// A note file as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteFile {
    /// The text, without the BOM.
    pub text: String,
    /// Whether the file starts with a UTF-8 BOM.
    pub has_bom: bool,
}

impl NoteFile {
    fn decode(bytes: Vec<u8>) -> Result<Self, ReadError> {
        let (has_bom, bytes) = match bytes.strip_prefix(UTF8_BOM) {
            Some(rest) => (true, rest.to_vec()),
            None => (false, bytes),
        };
        match String::from_utf8(bytes) {
            Ok(text) => Ok(Self { text, has_bom }),
            Err(_) => Err(ReadError::NotUtf8),
        }
    }

    fn encode(text: &str, has_bom: bool) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(text.len() + UTF8_BOM.len());
        if has_bom {
            bytes.extend_from_slice(UTF8_BOM);
        }
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }
}

/// Why a note file couldn't be read.
#[derive(Debug)]
pub enum ReadError {
    NotFound,
    /// The file is there but isn't UTF-8.
    NotUtf8,
    Io(io::Error),
}

/// Reads a note file.
pub fn read_note_file(path: &Path) -> Result<NoteFile, ReadError> {
    let bytes = fs::read(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => ReadError::NotFound,
        _ => ReadError::Io(error),
    })?;
    NoteFile::decode(bytes)
}

// MARK: - Writing files

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Replaces (or creates) the file at `path` with `bytes`, all at once: readers
/// see the old bytes or the new ones, and a crash or a failure leaves the old
/// file as it was. Keeps the file's permissions, and writes through a symlink
/// rather than over it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    match write_atomic_if(path, bytes, &mut || Ok(true))? {
        Commit::Written => Ok(()),
        Commit::Abandoned => Err(io::Error::other("write abandoned")),
    }
}

enum Commit {
    Written,
    Abandoned,
}

/// [`write_atomic`], asking `still_wanted` once the new bytes are safely on disk,
/// right before they replace the old ones.
fn write_atomic_if(path: &Path, bytes: &[u8], still_wanted: &mut dyn FnMut() -> io::Result<bool>) -> io::Result<Commit> {
    let target = resolve_symlink(path)?;
    let existing = fs::metadata(&target).ok();
    let (temp, mut file) = create_temp_beside(&target)?;
    let result = (|| {
        file.write_all(bytes)?;
        if let Some(existing) = &existing {
            fs::set_permissions(&temp, existing.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        if !still_wanted()? {
            return Ok(Commit::Abandoned);
        }
        fs::rename(&temp, &target)?;
        sync_folder(&target);
        Ok(Commit::Written)
    })();
    if !matches!(result, Ok(Commit::Written)) {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Writes a new file at `path` all at once, failing with `AlreadyExists` rather
/// than replacing anything there.
fn write_new_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (temp, mut file) = create_temp_beside(path)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        match fs::hard_link(&temp, path) {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
                // A disk without hard links (exFAT, some network volumes).
                if fs::symlink_metadata(path).is_ok() {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
                fs::rename(&temp, path)
            }
            other => other,
        }
    })();
    let _ = fs::remove_file(&temp);
    if result.is_ok() {
        sync_folder(path);
    }
    result
}

/// The file a write to `path` should land in: `path`, or what it links to.
fn resolve_symlink(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path),
        _ => Ok(path.to_path_buf()),
    }
}

/// A new hidden file in the folder of `target`, so the rename stays on one disk
/// and the library's scan and the watcher's filter pass over it.
fn create_temp_beside(target: &Path) -> io::Result<(PathBuf, fs::File)> {
    let folder = target.parent().unwrap_or(Path::new("/"));
    loop {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp = folder.join(format!(".fumi-tmp-{}-{counter}", std::process::id()));
        match fs::OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Makes a rename durable where the disk allows it; a failure changes nothing
/// about the file itself.
fn sync_folder(path: &Path) {
    if let Some(Ok(folder)) = path.parent().map(fs::File::open) {
        let _ = folder.sync_all();
    }
}

// MARK: - Own writes and debouncing

/// What this store wrote itself, by path, with the time of the write. Lets the
/// watcher ignore the events our own saves generate, so typing doesn't kick off a
/// full rescan on every debounced save, while a file written over from outside
/// right after is still seen.
#[derive(Debug, Default)]
pub struct OwnWrites {
    contents: HashMap<String, String>,
    times: HashMap<String, f64>,
}

impl OwnWrites {
    pub fn record(&mut self, path: &str, content: &str, now: f64) {
        self.contents.insert(path.to_string(), content.to_string());
        self.times.insert(path.to_string(), now);
    }

    /// Forgets writes older than [`OWN_WRITE_LIFETIME`].
    pub fn prune(&mut self, now: f64) {
        let times = &mut self.times;
        self.contents.retain(|path, _| {
            let fresh = times.get(path).is_some_and(|at| now - at < OWN_WRITE_LIFETIME);
            if !fresh {
                times.remove(path);
            }
            fresh
        });
    }

    pub fn contents(&self) -> &HashMap<String, String> {
        &self.contents
    }
}

/// Whether a batch of watcher events calls for a rescan: not when it's only the
/// echo of `own_writes` or inside hidden folders. Prunes `own_writes` first.
pub fn events_need_reload<S: AsRef<str>>(paths: &[S], library: Option<&str>, own_writes: &mut OwnWrites, now: f64) -> bool {
    own_writes.prune(now);
    let visible: Vec<String> = match library {
        Some(library) => external_sync::visible_paths(paths, library),
        None => paths.iter().map(|path| path.as_ref().to_string()).collect(),
    };
    !visible.is_empty()
        && external_sync::has_external_change(&visible, own_writes.contents(), |path| {
            read_note_file(Path::new(path)).ok().map(|file| file.text)
        })
}

/// A rescan waiting for things to settle: each new change pushes it back.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ReloadDebounce {
    deadline: Option<f64>,
}

impl ReloadDebounce {
    pub fn schedule(&mut self, now: f64) {
        self.deadline = Some(now + RELOAD_DEBOUNCE);
    }

    pub fn cancel(&mut self) {
        self.deadline = None;
    }

    /// When the rescan is due, for a timer to wait until.
    pub fn deadline(&self) -> Option<f64> {
        self.deadline
    }

    /// Whether the rescan is due now; true once, for each scheduling.
    pub fn take_due(&mut self, now: f64) -> bool {
        match self.deadline {
            Some(deadline) if now >= deadline => {
                self.deadline = None;
                true
            }
            _ => false,
        }
    }
}

// MARK: - Scanning

/// What a scan of a library found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScanResult {
    /// Pinned first, then most recently changed first.
    pub notes: Vec<Note>,
    pub files: Vec<LibraryFile>,
    /// Ids of the notes whose files couldn't be read as UTF-8: listed with
    /// empty text, and never saved over.
    pub unreadable: HashSet<String>,
}

/// Walks the library at `root`, skipping hidden files and folders. Symlinked
/// folders aren't followed. `now` stands in for dates a file doesn't give.
pub fn scan_library(root: &Path, pinned: &HashSet<String>, now: f64) -> ScanResult {
    let mut result = ScanResult::default();
    let root_path = root.to_string_lossy().into_owned();
    let mut pending = vec![root.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let Ok(entries) = fs::read_dir(&folder) else { continue };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        entries.sort();
        for path in entries {
            let Ok(link_metadata) = fs::symlink_metadata(&path) else { continue };
            if is_hidden(&path, &link_metadata) {
                continue;
            }
            if link_metadata.is_dir() {
                pending.push(path);
                continue;
            }
            let Ok(metadata) = fs::metadata(&path) else { continue };
            if metadata.is_dir() {
                continue;
            }
            let id = path.to_string_lossy().into_owned();
            let relative = relative_path_in(&id, &root_path);
            let updated_at = metadata.modified().map_or(now, epoch_seconds);
            if let Some(kind) = LibraryFile::kind_of(&path) {
                result.files.push(LibraryFile { id, path, kind, updated_at, relative_path: relative });
                continue;
            }
            if !is_note_extension(&path) {
                continue;
            }
            let content = match read_note_file(&path) {
                Ok(file) => file.text,
                Err(ReadError::NotFound) => continue,
                Err(_) => {
                    result.unreadable.insert(id.clone());
                    String::new()
                }
            };
            result.notes.push(Note {
                title: title::extract(&content),
                tags: tags::frontmatter_tags(&content),
                created_at: metadata.created().map_or(now, epoch_seconds),
                updated_at,
                pinned: pinned.contains(&id),
                relative_path: relative,
                id,
                path,
                content,
            });
        }
    }
    sort_notes(&mut result.notes);
    result
}

fn sort_notes(notes: &mut [Note]) {
    notes.sort_by(|a, b| b.pinned.cmp(&a.pinned).then_with(|| b.updated_at.total_cmp(&a.updated_at)));
}

fn is_note_extension(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        let ext = ext.to_string_lossy().to_lowercase();
        ext == "md" || ext == "markdown"
    })
}

/// `.skipsHiddenFiles`: a leading dot, or the Finder's hidden flag.
fn is_hidden(path: &Path, metadata: &fs::Metadata) -> bool {
    if path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        const UF_HIDDEN: u32 = 0x8000;
        if metadata.st_flags() & UF_HIDDEN != 0 {
            return true;
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = metadata;
    false
}

fn epoch_seconds(time: SystemTime) -> f64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs_f64(),
        Err(before) => -before.duration().as_secs_f64(),
    }
}

/// The folder of the file at `path`, relative to `root`: "/" at the root.
fn relative_path_in(path: &str, root: &str) -> String {
    let folder = path.rfind('/').map_or("", |index| &path[..index]);
    match folder.strip_prefix(root) {
        Some("") | None => "/".to_string(),
        Some(rest) => rest.to_string(),
    }
}

/// A file name made from a title: characters a file name can't hold become `-`,
/// spaces go from both ends, and nothing left is "Untitled".
pub fn sanitize_for_filename(title: &str) -> String {
    let replaced: String = title.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let sanitized = replaced.trim_matches(is_whitespace);
    if sanitized.is_empty() { "Untitled".to_string() } else { sanitized.to_string() }
}

// MARK: - Saving

/// What a save did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SaveResult {
    /// The save wrote a draft's file for the first time (the note is in the
    /// library from now on).
    pub adopted_draft: bool,
    /// What the file holds now: the content saved or, when the file was changed
    /// outside meanwhile, the two merged. `None` when nothing was saved.
    pub written: Option<String>,
    /// Where the outside version was kept, when it changed a paragraph the
    /// content saved changed too.
    pub conflict_copy: Option<Note>,
}

/// Why nothing was saved. The file is as it was, and so is the store's note.
#[derive(Debug)]
pub enum SaveError {
    /// The file isn't UTF-8: the text being saved can't have come from it, and
    /// writing would destroy it.
    NotUtf8,
    /// The file changed each of the times the save tried; the next save tries
    /// again.
    KeepsChanging,
    /// The outside version couldn't be kept, so writing over it would lose it.
    ConflictCopyFailed(io::Error),
    Io(io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::NotUtf8 => write!(f, "the file isn't UTF-8 text"),
            SaveError::KeepsChanging => write!(f, "the file keeps changing"),
            SaveError::ConflictCopyFailed(error) => write!(f, "couldn't keep the outside version: {error}"),
            SaveError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SaveError {}

// MARK: - The store

/// A scan to run, off the UI thread if need be.
#[derive(Debug, Clone)]
pub struct ReloadTicket {
    generation: u64,
    root: PathBuf,
    pinned: HashSet<String>,
}

impl ReloadTicket {
    pub fn scan(&self, now: f64) -> ScanResult {
        scan_library(&self.root, &self.pinned, now)
    }
}

/// What a reload changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReloadChange {
    /// False when a newer reload, or closing the library, overtook this one.
    pub applied: bool,
    pub notes_changed: bool,
    pub files_changed: bool,
}

/// The notes and files of the library that's open.
#[derive(Default)]
pub struct NoteStore {
    notes: Vec<Note>,
    files: Vec<LibraryFile>,
    workspace: Option<PathBuf>,
    has_loaded_notes: bool,
    /// The folder the last session used, when it can't be found at launch.
    /// Opening any folder clears it.
    pub missing_workspace: Option<PathBuf>,
    own_writes: OwnWrites,
    debounce: ReloadDebounce,
    pinned: HashSet<String>,
    pins_changed: bool,
    unreadable: HashSet<String>,
    reload_generation: u64,
    /// The id of the last draft handed out; its first save writes the file.
    draft_id: Option<String>,
    /// Item moves of this session, oldest first, so a debounced save bound to a
    /// note's old path still lands in its file after a move.
    recent_moves: Vec<(String, String)>,
    /// Runs when a save is about to replace a file, for tests to race it.
    #[cfg(test)]
    before_commit: Option<Box<dyn FnMut()>>,
}

impl NoteStore {
    pub fn new() -> Self {
        Self::default()
    }

    // MARK: Reading the state

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// Images, videos and PDFs in the library, shown in the sidebar too.
    pub fn files(&self) -> &[LibraryFile] {
        &self.files
    }

    pub fn workspace(&self) -> Option<&Path> {
        self.workspace.as_deref()
    }

    pub fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    /// False from opening a folder until its first scan lands, so the empty
    /// library page doesn't flash while the notes load.
    pub fn has_loaded_notes(&self) -> bool {
        self.has_loaded_notes
    }

    pub fn note(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|note| note.id == id)
    }

    /// Whether the note's file couldn't be read as UTF-8 at the last scan: its
    /// text here is empty and saving it is refused.
    pub fn is_unreadable(&self, id: &str) -> bool {
        self.unreadable.contains(id)
    }

    /// The pinned file paths, to keep between launches. The Swift app keeps them
    /// under `fumi.pinnedFiles` in `UserDefaults`.
    pub fn pinned_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.pinned.iter().cloned().collect();
        paths.sort();
        paths
    }

    /// Whether the pinned paths changed since this was last asked.
    pub fn take_pins_changed(&mut self) -> bool {
        std::mem::take(&mut self.pins_changed)
    }

    // MARK: Opening and closing

    /// Opens the library at `path`, with the pinned paths kept from before. Its
    /// notes arrive with the first reload.
    pub fn open_workspace(&mut self, path: &Path, pinned: impl IntoIterator<Item = String>) {
        self.close_workspace();
        let text = path.to_string_lossy();
        let trimmed = text.trim_end_matches('/');
        self.workspace = Some(PathBuf::from(if trimmed.is_empty() { "/" } else { trimmed }));
        self.missing_workspace = None;
        self.pinned = pinned.into_iter().collect();
    }

    pub fn close_workspace(&mut self) {
        self.debounce.cancel();
        self.reload_generation += 1;
        self.notes.clear();
        self.files.clear();
        self.unreadable.clear();
        self.has_loaded_notes = false;
        self.workspace = None;
    }

    // MARK: Reloading

    /// Starts a reload: run the ticket's scan anywhere, then hand it to
    /// [`NoteStore::apply_reload`]. `None` with no library open.
    pub fn begin_reload(&mut self) -> Option<ReloadTicket> {
        let root = self.workspace.clone()?;
        self.reload_generation += 1;
        Some(ReloadTicket { generation: self.reload_generation, root, pinned: self.pinned.clone() })
    }

    /// Puts a finished scan in place, unless a newer reload has started since or
    /// the library was closed.
    pub fn apply_reload(&mut self, ticket: &ReloadTicket, scan: ScanResult) -> ReloadChange {
        if self.reload_generation != ticket.generation || self.workspace.as_ref() != Some(&ticket.root) {
            return ReloadChange::default();
        }
        let change = ReloadChange { applied: true, notes_changed: self.notes != scan.notes, files_changed: self.files != scan.files };
        self.notes = scan.notes;
        self.files = scan.files;
        self.unreadable = scan.unreadable;
        self.has_loaded_notes = true;
        change
    }

    /// Scans the library now and puts the result in place.
    pub fn reload(&mut self, now: f64) -> ReloadChange {
        match self.begin_reload() {
            Some(ticket) => {
                let scan = ticket.scan(now);
                self.apply_reload(&ticket, scan)
            }
            None => ReloadChange::default(),
        }
    }

    /// Asks for a rescan once things settle.
    pub fn schedule_reload(&mut self, now: f64) {
        self.debounce.schedule(now);
    }

    /// When the scheduled rescan is due, if one is.
    pub fn reload_deadline(&self) -> Option<f64> {
        self.debounce.deadline()
    }

    /// Whether the scheduled rescan is due now; true once for each scheduling.
    pub fn reload_is_due(&mut self, now: f64) -> bool {
        self.debounce.take_due(now)
    }

    /// A batch of watcher events: drops the ones that are only the echo of our
    /// own saves, then debounces a reload. Returns whether one was scheduled.
    pub fn handle_file_events<S: AsRef<str>>(&mut self, paths: &[S], now: f64) -> bool {
        let library = self.workspace.as_ref().map(|path| path.to_string_lossy().into_owned());
        let needed = events_need_reload(paths, library.as_deref(), &mut self.own_writes, now);
        if needed {
            self.schedule_reload(now);
        }
        needed
    }

    // MARK: Drafts and new notes

    /// The page shown when no note is open: a blank note at the library's top
    /// level, named like New Note's, whose file is only written once something
    /// is typed — leave without writing and nothing is kept.
    pub fn draft_note(&mut self, now: Now) -> Option<Note> {
        let workspace = self.workspace.clone()?;
        let path = (1..)
            .map(|n| workspace.join(if n == 1 { "Untitled.md".to_string() } else { format!("Untitled {n}.md") }))
            .find(|path| fs::symlink_metadata(path).is_err())?;
        let id = path.to_string_lossy().into_owned();
        self.draft_id = Some(id.clone());
        Some(Note {
            relative_path: self.relative_path(&id),
            id,
            path,
            title: String::new(),
            content: DRAFT_CONTENT.to_string(),
            tags: Vec::new(),
            created_at: now.secs,
            updated_at: now.secs,
            pinned: false,
        })
    }

    /// A new note titled `title`, in `folder` (the library's top when `None`).
    /// `None` with no library open.
    pub fn create_note(&mut self, title: Option<&str>, folder: Option<&Path>, now: Now) -> io::Result<Option<Note>> {
        let Some(workspace) = self.workspace.clone() else { return Ok(None) };
        let folder = folder.unwrap_or(&workspace);
        let shown = title.unwrap_or("Untitled");
        let name = sanitize_for_filename(shown);
        let content = format!("# {shown}\n\n");
        let mut counter = 1;
        let path = loop {
            let file = if counter == 1 { format!("{name}.md") } else { format!("{name} {counter}.md") };
            let path = folder.join(file);
            counter += 1;
            match write_new_atomic(&path, content.as_bytes()) {
                Ok(()) => break path,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let note = self.new_note(path, &content, now.secs);
        self.notes.insert(0, note.clone());
        self.schedule_reload(now.secs);
        Ok(Some(note))
    }

    fn new_note(&self, path: PathBuf, content: &str, now: f64) -> Note {
        let id = path.to_string_lossy().into_owned();
        Note {
            relative_path: self.relative_path(&id),
            title: title::extract(content),
            tags: tags::frontmatter_tags(content),
            content: content.to_string(),
            created_at: now,
            updated_at: now,
            pinned: false,
            id,
            path,
        }
    }

    // MARK: Saving

    /// Saves `content`, edited from `base`: what the file held when the editing
    /// began (the store's last known content when `None`). A file changed outside
    /// since then is merged with `content`, never written over (`merge3`): edits
    /// to different paragraphs are both kept; where both changed the same
    /// paragraph `content` wins, and the outside version is kept as a copy beside
    /// the note.
    ///
    /// `note_id` names the only note this can write to. An id that names no note,
    /// no note moved this session and not the current draft writes nothing and
    /// returns `written: None`.
    pub fn save_content(&mut self, content: &str, note_id: &str, base: Option<&str>, now: Now) -> Result<SaveResult, SaveError> {
        let Some(target) = self.save_target(note_id, base) else {
            return self.save_draft(content, note_id, now);
        };
        // A save from before a move: its links still read from the old folder.
        let moved = target.id != note_id;
        let content = if moved { self.relinked(content, note_id) } else { content.to_string() };
        let base = match base {
            Some(base) if moved => self.relinked(base, note_id),
            Some(base) => base.to_string(),
            None => target.content.clone(),
        };

        let mut result = SaveResult::default();
        let mut kept_outside: Option<String> = None;
        for _ in 0..SAVE_ATTEMPTS {
            let snapshot = match fs::read(&target.path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(SaveError::Io(error)),
            };
            let disk = match snapshot.clone().map(NoteFile::decode).transpose() {
                Ok(disk) => disk,
                Err(ReadError::NotUtf8) => return Err(SaveError::NotUtf8),
                Err(ReadError::NotFound) => None,
                Err(ReadError::Io(error)) => return Err(SaveError::Io(error)),
            };

            let mut written = content.clone();
            if let Some(disk) = &disk
                && disk.text != base
                && disk.text != content
            {
                let merged = merge3::merge(&base, &content, &disk.text);
                written = merged.text;
                if merged.conflicts > 0 && kept_outside.as_deref() != Some(disk.text.as_str()) {
                    let copy = self.save_conflict_copy(disk, &target, now).map_err(SaveError::ConflictCopyFailed)?;
                    self.notes.insert(0, copy.clone());
                    result.conflict_copy = Some(copy);
                    kept_outside = Some(disk.text.clone());
                }
            }

            let unchanged = disk.as_ref().is_some_and(|disk| disk.text == written);
            if !unchanged {
                // Record before writing so the watcher event this triggers is
                // recognized as our own and doesn't cause a redundant rescan.
                self.own_writes.record(&target.id, &written, now.secs);
                let bytes = NoteFile::encode(&written, disk.as_ref().is_some_and(|disk| disk.has_bom));
                #[cfg(test)]
                let mut hook = self.before_commit.take();
                let commit = write_atomic_if(&target.path, &bytes, &mut || {
                    #[cfg(test)]
                    if let Some(hook) = &mut hook {
                        hook();
                    }
                    // A write from outside that landed while this one was on its
                    // way to the disk is merged too, not written over.
                    match fs::read(&target.path) {
                        Ok(current) => Ok(Some(current) == snapshot),
                        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(snapshot.is_none()),
                        Err(error) => Err(error),
                    }
                });
                #[cfg(test)]
                {
                    self.before_commit = hook;
                }
                match commit.map_err(SaveError::Io)? {
                    Commit::Written => {}
                    Commit::Abandoned => continue,
                }
            }

            // Update in memory without a full reload (avoids a cursor jump).
            if let Some(note) = self.notes.iter_mut().find(|note| note.id == target.id) {
                note.title = title::extract(&written);
                note.tags = tags::frontmatter_tags(&written);
                note.content = written.clone();
                note.updated_at = now.secs;
            }
            self.unreadable.remove(&target.id);
            result.written = Some(written);
            return Ok(result);
        }
        Err(SaveError::KeepsChanging)
    }

    /// The note a save for `note_id` belongs to. Normally the note with that id,
    /// or where it moved to this session. When another note has since taken the
    /// old path, a `base` that is the moved note's text says the save is the moved
    /// note's last one, not the new note's.
    fn save_target(&self, note_id: &str, base: Option<&str>) -> Option<Note> {
        let at_id = self.note(note_id);
        let moved = self.moved_note(note_id);
        match (at_id, moved) {
            (Some(at_id), Some(moved)) => {
                let stale = base.is_some_and(|base| base != at_id.content && self.relinked(base, note_id) == moved.content);
                Some(if stale { moved.clone() } else { at_id.clone() })
            }
            (Some(note), None) | (None, Some(note)) => Some(note.clone()),
            (None, None) => None,
        }
    }

    /// Keeps `disk`, the outside version of `note`, beside it as
    /// "<name> (outside edit 14-10).md", never over another file.
    fn save_conflict_copy(&mut self, disk: &NoteFile, note: &Note, now: Now) -> io::Result<Note> {
        let folder = note.path.parent().unwrap_or(Path::new("/")).to_path_buf();
        let ext = note.path.extension().map(|ext| ext.to_string_lossy().into_owned()).unwrap_or_default();
        let name = format!("{} (outside edit {})", note.filename(), clock_stamp(now));
        let bytes = NoteFile::encode(&disk.text, disk.has_bom);
        let mut counter = 1;
        let path = loop {
            let numbered = if counter == 1 { name.clone() } else { format!("{name} {counter}") };
            let path = folder.join(if ext.is_empty() { numbered } else { format!("{numbered}.{ext}") });
            counter += 1;
            self.own_writes.record(&path.to_string_lossy(), &disk.text, now.secs);
            match write_new_atomic(&path, &bytes) {
                Ok(()) => break path,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        Ok(self.new_note(path, &disk.text, now.secs))
    }

    fn save_draft(&mut self, content: &str, id: &str, now: Now) -> Result<SaveResult, SaveError> {
        let path = PathBuf::from(id);
        if self.draft_id.as_deref() != Some(id) || self.workspace.is_none() || fs::symlink_metadata(&path).is_ok() {
            return Ok(SaveResult::default());
        }
        self.own_writes.record(id, content, now.secs);
        match write_new_atomic(&path, content.as_bytes()) {
            Ok(()) => {}
            // Something took the name between the check and the write.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(SaveResult::default()),
            Err(error) => return Err(SaveError::Io(error)),
        }
        self.draft_id = None;
        let note = self.new_note(path, content, now.secs);
        self.notes.insert(0, note);
        Ok(SaveResult { adopted_draft: true, written: Some(content.to_string()), conflict_copy: None })
    }

    // MARK: Memos

    /// The day files of the Memos folder.
    pub fn memo_notes(&self) -> Vec<&Note> {
        let folder = format!("/{}", memo_format::FOLDER_NAME);
        self.notes.iter().filter(|note| note.relative_path == folder && memo_format::is_day(&note.filename())).collect()
    }

    /// Every memo, newest day first and, within a day, latest first.
    pub fn memos(&self) -> Vec<Memo> {
        let mut days = self.memo_notes();
        days.sort_by_key(|note| std::cmp::Reverse(note.filename()));
        days.into_iter()
            .flat_map(|note| {
                let mut memos = memo_format::parse(&note.content, &note.filename());
                memos.reverse();
                memos
            })
            .collect()
    }

    pub fn memo_note(&self, day: &str) -> Option<&Note> {
        self.memo_notes().into_iter().find(|note| note.filename() == day)
    }

    /// Adds a memo to today's file, creating it as needed.
    pub fn add_memo(&mut self, text: &str, now: Now) -> Result<(), SaveError> {
        let Some(workspace) = self.workspace.clone() else { return Ok(()) };
        if text.trim_matches(is_whitespace_or_newline).is_empty() {
            return Ok(());
        }
        let day = memo_format::day_of(now.secs, now.utc_offset);
        if let Some(note) = self.memo_note(&day) {
            let (id, updated) = (note.id.clone(), memo_format::appending(text, &memo_format::time_of(now.secs, now.utc_offset), &note.content));
            self.save_content(&updated, &id, None, now)?;
            return Ok(());
        }
        let path = memo_format::append_to_file(text, now.secs, now.utc_offset, &workspace).map_err(SaveError::Io)?;
        let content = match read_note_file(&path) {
            Ok(file) => file.text,
            Err(ReadError::NotUtf8) => return Err(SaveError::NotUtf8),
            Err(ReadError::NotFound) => return Err(SaveError::Io(io::ErrorKind::NotFound.into())),
            Err(ReadError::Io(error)) => return Err(SaveError::Io(error)),
        };
        self.own_writes.record(&path.to_string_lossy(), &content, now.secs);
        let note = self.new_note(path, &content, now.secs);
        self.notes.insert(0, note);
        Ok(())
    }

    pub fn update_memo(&mut self, memo: &Memo, text: &str, now: Now) -> Result<(), SaveError> {
        let Some(note) = self.memo_note(&memo.day) else { return Ok(()) };
        let (id, updated) = (note.id.clone(), memo_format::replacing(memo.index, text, &note.content));
        self.save_content(&updated, &id, None, now).map(|_| ())
    }

    /// Takes the memo out of its day file; a day left empty is deleted.
    pub fn delete_memo(&mut self, memo: &Memo, now: Now) -> Result<(), SaveError> {
        let Some(note) = self.memo_note(&memo.day) else { return Ok(()) };
        let (id, updated) = (note.id.clone(), memo_format::removing(memo.index, &note.content));
        if updated.is_empty() {
            return self.delete_note(&id).map(|_| ()).map_err(SaveError::Io);
        }
        self.save_content(&updated, &id, None, now).map(|_| ())
    }

    // MARK: Deleting and pinning

    /// Deletes the note's file. Returns whether a note with that id was there.
    /// When the file can't be deleted the note stays listed.
    pub fn delete_note(&mut self, note_id: &str) -> io::Result<bool> {
        let Some(note) = self.note(note_id) else { return Ok(false) };
        match fs::remove_file(&note.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.notes.retain(|note| note.id != note_id);
        self.unreadable.remove(note_id);
        Ok(true)
    }

    /// Pins or unpins the note and sorts the notes again. Returns whether it's
    /// pinned now, `None` when there's no such note.
    pub fn toggle_pin(&mut self, note_id: &str) -> Option<bool> {
        let note = self.notes.iter_mut().find(|note| note.id == note_id)?;
        note.pinned = !note.pinned;
        let (pinned, path) = (note.pinned, note.path.to_string_lossy().into_owned());
        if pinned {
            self.pinned.insert(path);
        } else {
            self.pinned.remove(&path);
        }
        self.pins_changed = true;
        sort_notes(&mut self.notes);
        Some(pinned)
    }

    // MARK: Renaming and moving

    /// Renames the note's file to `new_title` (sanitized). Returns the new note
    /// id (its new file path) on success, or `None` if nothing changed or it
    /// failed (name unchanged, a file with that name already exists).
    pub fn rename_note(&mut self, note_id: &str, new_title: &str, now: Now) -> Option<String> {
        let index = self.notes.iter().position(|note| note.id == note_id)?;
        let old_path = self.notes[index].path.clone();
        let new_path = old_path.parent()?.join(format!("{}.md", sanitize_for_filename(new_title)));
        if new_path == old_path || fs::symlink_metadata(&new_path).is_ok() {
            return None;
        }
        fs::rename(&old_path, &new_path).ok()?;
        let (old, new) = (old_path.to_string_lossy().into_owned(), new_path.to_string_lossy().into_owned());
        // Pin state is keyed by file path.
        if self.pinned.remove(&old) {
            self.pinned.insert(new.clone());
            self.pins_changed = true;
        }
        if self.unreadable.remove(&old) {
            self.unreadable.insert(new.clone());
        }
        let relative_path = self.relative_path(&new);
        let note = &mut self.notes[index];
        note.id = new.clone();
        note.path = new_path;
        note.title = new_title.to_string();
        note.updated_at = now.secs;
        note.relative_path = relative_path;
        // So a debounced save bound to the old name still lands in this file.
        self.recent_moves.push((old, new.clone()));
        Some(new)
    }

    /// Moves a note, file or folder into the folder with sidebar id `folder_id`
    /// ("/" for the library's top level). Never overwrites: a name already taken
    /// there refuses the move. Pins, and the notes and files in memory, follow at
    /// once (their ids are their paths), so a selection the caller remaps with
    /// `library_move::remap` finds its note straight away.
    pub fn move_item(&mut self, item: &Path, folder_id: &str, now: Now) -> Result<PathBuf, Refusal> {
        let Some(workspace) = self.workspace.clone() else { return Err(Refusal::OutsideLibrary) };
        let folder = library_move::folder_path(folder_id, &workspace);
        let new_path = library_move::destination(item, &folder, &workspace, |path| fs::symlink_metadata(path).is_ok())?;
        fs::rename(item, &new_path).map_err(|error| Refusal::Failed(error.to_string()))?;
        let (old, new) = (item.to_string_lossy().into_owned(), new_path.to_string_lossy().into_owned());

        // Pin state is keyed by file path.
        let moved_pins: Vec<(String, String)> = self
            .pinned
            .iter()
            .filter_map(|path| library_move::remap(path, &old, &new).map(|to| (path.clone(), to)))
            .collect();
        if !moved_pins.is_empty() {
            for (from, to) in moved_pins {
                self.pinned.remove(&from);
                self.pinned.insert(to);
            }
            self.pins_changed = true;
        }

        let root = workspace.to_string_lossy().into_owned();
        for note in &mut self.notes {
            if let Some(path) = library_move::remap(&note.id, &old, &new) {
                if self.unreadable.remove(&note.id) {
                    self.unreadable.insert(path.clone());
                }
                note.relative_path = relative_path_in(&path, &root);
                note.path = PathBuf::from(&path);
                note.id = path;
            }
        }
        for file in &mut self.files {
            if let Some(path) = library_move::remap(&file.id, &old, &new) {
                file.relative_path = relative_path_in(&path, &root);
                file.path = PathBuf::from(&path);
                file.id = path;
            }
        }
        self.recent_moves.push((old.clone(), new.clone()));
        self.relink_notes(&old, &new, &root, now);
        self.schedule_reload(now.secs);
        Ok(new_path)
    }

    /// Rewrites the links of the notes a move carried along — the note itself, or
    /// those in a moved folder — that would no longer find their images, videos
    /// and PDFs from the new folder. Saved like an edit, so the watcher takes the
    /// write as our own. A file that can't be read is left alone.
    fn relink_notes(&mut self, old: &str, new: &str, library: &str, now: Now) {
        let carried: Vec<(String, String, PathBuf)> = self
            .notes
            .iter()
            .filter_map(|note| library_move::remap(&note.id, new, old).map(|old_path| (note.id.clone(), old_path, note.path.clone())))
            .collect();
        for (id, old_path, path) in carried {
            let Ok(file) = read_note_file(&path) else { continue };
            let relinked = link_rewrite::rewrite(&file.text, &old_path, old, new, library, path_exists);
            if relinked != file.text {
                // The file keeps its old links when this fails; the move stands.
                let _ = self.save_content(&relinked, &id, Some(&file.text), now);
            }
        }
    }

    /// `content` saved for the note once at `id`, its links following each of this
    /// session's moves since — the editor's last save before a move.
    fn relinked(&self, content: &str, id: &str) -> String {
        let Some(library) = self.workspace.as_ref().map(|path| path.to_string_lossy().into_owned()) else {
            return content.to_string();
        };
        let mut path = id.to_string();
        let mut content = content.to_string();
        for (from, to) in &self.recent_moves {
            let Some(moved) = library_move::remap(&path, from, to) else { continue };
            content = link_rewrite::rewrite(&content, &path, from, to, &library, path_exists);
            path = moved;
        }
        content
    }

    /// Where the note once at `id` is now, following this session's moves.
    fn moved_note(&self, id: &str) -> Option<&Note> {
        let mut path = id.to_string();
        for (from, to) in &self.recent_moves {
            if let Some(moved) = library_move::remap(&path, from, to) {
                path = moved;
            }
        }
        if path == id {
            return None;
        }
        self.note(&path)
    }

    fn relative_path(&self, path: &str) -> String {
        match &self.workspace {
            Some(workspace) => relative_path_in(path, &workspace.to_string_lossy()),
            None => last_path_component(path).to_string(),
        }
    }
}

fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// `HH-mm` in local time.
fn clock_stamp(now: Now) -> String {
    let local = now.secs.floor() as i64 + now.utc_offset as i64;
    let seconds = local.rem_euclid(86_400);
    format!("{:02}-{:02}", seconds / 3600, seconds % 3600 / 60)
}

#[cfg(test)]
mod tests {
    // No Swift test exists for FileNoteStore; these are new.
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// 2026-10-17 03:30:05 UTC.
    const NOW: Now = Now { secs: 1_792_207_805.0, utc_offset: 0 };

    struct Library {
        dir: TempDir,
        store: NoteStore,
    }

    impl Library {
        /// A library with these files (paths relative to its root), scanned.
        fn with(files: &[(&str, &[u8])]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            for (name, bytes) in files {
                let path = dir.path().join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            }
            let mut store = NoteStore::new();
            store.open_workspace(dir.path(), Vec::new());
            store.reload(NOW.secs);
            Self { dir, store }
        }

        fn path(&self, name: &str) -> PathBuf {
            self.dir.path().join(name)
        }

        fn id(&self, name: &str) -> String {
            self.path(name).to_string_lossy().into_owned()
        }

        fn bytes(&self, name: &str) -> Vec<u8> {
            fs::read(self.path(name)).unwrap()
        }

        fn text(&self, name: &str) -> String {
            String::from_utf8(self.bytes(name)).unwrap()
        }

        fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
            fs::write(self.path(name), bytes).unwrap();
        }

        fn save(&mut self, name: &str, content: &str, base: Option<&str>) -> Result<SaveResult, SaveError> {
            let id = self.id(name);
            self.store.save_content(content, &id, base, NOW)
        }

        /// Every name in the library, hidden ones too, sorted.
        fn listing(&self) -> Vec<String> {
            fn walk(folder: &Path, root: &Path, out: &mut Vec<String>) {
                for entry in fs::read_dir(folder).unwrap().flatten() {
                    let path = entry.path();
                    out.push(path.strip_prefix(root).unwrap().to_string_lossy().into_owned());
                    if path.is_dir() {
                        walk(&path, root, out);
                    }
                }
            }
            let mut out = Vec::new();
            walk(self.dir.path(), self.dir.path(), &mut out);
            out.sort();
            out
        }
    }

    // MARK: Never one note's text in another note's file (commit 1197950)

    #[test]
    fn a_save_touches_only_the_file_it_names() {
        let mut lib = Library::with(&[("a.md", b"text of a"), ("b.md", b"text of b"), ("sub/c.md", b"text of c")]);
        let (ino_b, ino_c) = (fs::metadata(lib.path("b.md")).unwrap().ino(), fs::metadata(lib.path("sub/c.md")).unwrap().ino());
        lib.save("a.md", "new a", None).unwrap();
        assert_eq!(lib.text("a.md"), "new a");
        assert_eq!(lib.text("b.md"), "text of b");
        assert_eq!(lib.text("sub/c.md"), "text of c");
        assert_eq!(fs::metadata(lib.path("b.md")).unwrap().ino(), ino_b, "b wasn't replaced either");
        assert_eq!(fs::metadata(lib.path("sub/c.md")).unwrap().ino(), ino_c);
        assert_eq!(lib.store.note(&lib.id("b.md")).unwrap().content, "text of b");
    }

    #[test]
    fn debounced_saves_land_where_they_were_bound() {
        // The 1197950 path: the user edits a, switches to b, and a's debounced
        // save fires after b's. Each write carries the id it was scheduled with.
        let mut lib = Library::with(&[("a.md", b"A0"), ("b.md", b"B0")]);
        let (a, b) = (lib.id("a.md"), lib.id("b.md"));
        let pending = [(b.clone(), "B1", "B0"), (a.clone(), "A1", "A0"), (b, "B2", "B1"), (a, "A2", "A1")];
        for (id, content, base) in pending {
            lib.store.save_content(content, &id, Some(base), NOW).unwrap();
        }
        assert_eq!(lib.text("a.md"), "A2");
        assert_eq!(lib.text("b.md"), "B2");
        assert_eq!(lib.listing(), ["a.md", "b.md"], "and no conflict copies");
    }

    #[test]
    fn an_unknown_id_writes_nothing() {
        let mut lib = Library::with(&[("a.md", b"text of a")]);
        let result = lib.save("other.md", "stray text", None).unwrap();
        assert_eq!(result, SaveResult::default());
        assert_eq!(lib.listing(), ["a.md"]);
        assert_eq!(lib.text("a.md"), "text of a");
    }

    #[test]
    fn a_late_save_for_a_deleted_note_writes_nothing() {
        let mut lib = Library::with(&[("a.md", b"text of a"), ("b.md", b"text of b")]);
        assert!(lib.store.delete_note(&lib.id("a.md")).unwrap());
        let result = lib.save("a.md", "late text", Some("text of a")).unwrap();
        assert_eq!(result.written, None);
        assert_eq!(lib.listing(), ["b.md"], "not brought back");
        assert_eq!(lib.text("b.md"), "text of b");
    }

    #[test]
    fn only_the_current_draft_can_be_written() {
        let mut lib = Library::with(&[]);
        let first = lib.store.draft_note(NOW).unwrap();
        assert_eq!(first.content, DRAFT_CONTENT);
        assert_eq!(lib.listing(), Vec::<String>::new(), "a draft has no file until it's saved");
        // Taken outside, so the next draft gets the next name.
        lib.write("Untitled.md", "outside");
        let second = lib.store.draft_note(NOW).unwrap();
        assert_eq!(second.filename(), "Untitled 2");

        let stale = lib.store.save_content("for the first draft", &first.id, None, NOW).unwrap();
        assert_eq!(stale.written, None);
        assert_eq!(lib.text("Untitled.md"), "outside", "a draft never lands on a file that appeared");

        let saved = lib.store.save_content("# Second", &second.id, None, NOW).unwrap();
        assert!(saved.adopted_draft);
        assert_eq!(saved.written.as_deref(), Some("# Second"));
        assert_eq!(lib.text("Untitled 2.md"), "# Second");
        assert_eq!(lib.store.notes()[0].id, second.id);

        // From now on it's an ordinary note.
        let again = lib.store.save_content("# Second!", &second.id, None, NOW).unwrap();
        assert!(!again.adopted_draft);
        assert_eq!(lib.text("Untitled 2.md"), "# Second!");
    }

    #[test]
    fn a_draft_whose_name_was_taken_leaves_that_file_alone() {
        let mut lib = Library::with(&[]);
        let draft = lib.store.draft_note(NOW).unwrap();
        lib.write("Untitled.md", "outside");
        let result = lib.store.save_content("typed", &draft.id, None, NOW).unwrap();
        assert_eq!(result.written, None);
        assert_eq!(lib.text("Untitled.md"), "outside");
    }

    #[test]
    fn a_save_from_before_a_move_lands_in_the_moved_file() {
        let mut lib = Library::with(&[("a.md", b"old"), ("b.md", b"text of b"), ("sub/x.md", b"x")]);
        let old_id = lib.id("a.md");
        lib.store.move_item(&lib.path("a.md"), "/sub", NOW).unwrap();
        let result = lib.store.save_content("typed before the move", &old_id, Some("old"), NOW).unwrap();
        assert_eq!(result.written.as_deref(), Some("typed before the move"));
        assert_eq!(lib.text("sub/a.md"), "typed before the move");
        assert!(!lib.path("a.md").exists(), "not written back to the old path");
        assert_eq!(lib.text("b.md"), "text of b");
    }

    #[test]
    fn a_save_from_before_a_rename_lands_in_the_renamed_file() {
        let mut lib = Library::with(&[("a.md", b"old")]);
        let old_id = lib.id("a.md");
        lib.store.rename_note(&old_id, "c", NOW).unwrap();
        let result = lib.store.save_content("typed before the rename", &old_id, Some("old"), NOW).unwrap();
        assert_eq!(result.written.as_deref(), Some("typed before the rename"));
        assert_eq!(lib.text("c.md"), "typed before the rename");
        assert!(!lib.path("a.md").exists());
    }

    #[test]
    fn a_new_note_on_an_old_path_keeps_its_own_text() {
        // a.md moves away and a new note takes its path. a's debounced save, bound
        // to that path with a's text as its base, belongs to the moved file.
        let mut lib = Library::with(&[("a.md", b"text of a"), ("sub/x.md", b"x")]);
        let old_id = lib.id("a.md");
        lib.store.move_item(&lib.path("a.md"), "/sub", NOW).unwrap();
        lib.store.create_note(Some("a"), None, NOW).unwrap().unwrap();
        assert_eq!(lib.text("a.md"), "# a\n\n");

        lib.store.save_content("late text of a", &old_id, Some("text of a"), NOW).unwrap();
        assert_eq!(lib.text("sub/a.md"), "late text of a");
        assert_eq!(lib.text("a.md"), "# a\n\n", "the new note isn't touched");

        // The new note's own save, based on its own text, goes to the new note.
        lib.store.save_content("# a\n\nnew", &old_id, Some("# a\n\n"), NOW).unwrap();
        assert_eq!(lib.text("a.md"), "# a\n\nnew");
        assert_eq!(lib.text("sub/a.md"), "late text of a");
    }

    // MARK: A save racing an external edit

    #[test]
    fn edits_to_different_paragraphs_are_both_kept() {
        let base = "# W43\n\nDone\n\n## Next\n\n- find";
        let mut lib = Library::with(&[("a.md", base.as_bytes())]);
        lib.write("a.md", "# W43\n\nDone, three templates\n\n## Next\n\n- find");
        let result = lib.save("a.md", "# W43\n\nDone\n\n## Next\n\n- find and replace", Some(base)).unwrap();
        let merged = "# W43\n\nDone, three templates\n\n## Next\n\n- find and replace";
        assert_eq!(result.written.as_deref(), Some(merged));
        assert_eq!(result.conflict_copy, None);
        assert_eq!(lib.text("a.md"), merged);
        assert_eq!(lib.store.note(&lib.id("a.md")).unwrap().content, merged);
        assert_eq!(lib.listing(), ["a.md"]);
    }

    #[test]
    fn cjk_paragraphs_merge_on_disk() {
        let base = "本周完成\n\n下周计划";
        let mut lib = Library::with(&[("笔记.md", base.as_bytes())]);
        lib.write("笔记.md", "本周完成\n\n下周计划：查找替换");
        lib.save("笔记.md", "本周完成了三件事\n\n下周计划", Some(base)).unwrap();
        assert_eq!(lib.text("笔记.md"), "本周完成了三件事\n\n下周计划：查找替换");
    }

    #[test]
    fn the_same_paragraph_changed_both_ways_keeps_both_versions() {
        let base = "a\n\nb";
        let mut lib = Library::with(&[("a.md", base.as_bytes())]);
        lib.write("a.md", "a theirs\n\nb");
        let result = lib.save("a.md", "a mine\n\nb", Some(base)).unwrap();
        assert_eq!(result.written.as_deref(), Some("a mine\n\nb"));
        assert_eq!(lib.text("a.md"), "a mine\n\nb", "the note keeps what was typed");

        let copy = result.conflict_copy.unwrap();
        assert_eq!(copy.filename(), "a (outside edit 03-30)");
        assert_eq!(lib.text("a (outside edit 03-30).md"), "a theirs\n\nb", "the outside version is kept exactly");
        assert_eq!(copy.content, "a theirs\n\nb");
        assert_eq!(lib.store.notes()[0].id, copy.id, "and listed");

        // A second conflict in the same minute doesn't write over the first copy.
        lib.write("a.md", "a theirs again\n\nb");
        let again = lib.save("a.md", "a mine again\n\nb", Some("a mine\n\nb")).unwrap();
        assert_eq!(again.conflict_copy.unwrap().filename(), "a (outside edit 03-30) 2");
        assert_eq!(lib.text("a (outside edit 03-30).md"), "a theirs\n\nb");
        assert_eq!(lib.text("a (outside edit 03-30) 2.md"), "a theirs again\n\nb");
    }

    #[test]
    fn the_conflict_copy_is_named_in_local_time() {
        let mut lib = Library::with(&[("a.md", b"a")]);
        lib.write("a.md", "theirs");
        let id = lib.id("a.md");
        let result = lib.store.save_content("mine", &id, Some("a"), Now::new(NOW.secs, 8 * 3600)).unwrap();
        assert_eq!(result.conflict_copy.unwrap().filename(), "a (outside edit 11-30)");
    }

    #[test]
    fn without_a_base_the_store_knows_what_it_last_saw() {
        let mut lib = Library::with(&[("a.md", b"one\n\ntwo")]);
        lib.write("a.md", "one\n\ntwo outside");
        lib.save("a.md", "one here\n\ntwo", None).unwrap();
        assert_eq!(lib.text("a.md"), "one here\n\ntwo outside");
    }

    #[test]
    fn an_edit_landing_during_the_write_is_merged_too() {
        // The window between reading the file and replacing it, which holds the
        // slow flush to disk.
        let base = "a\n\nb";
        let mut lib = Library::with(&[("a.md", base.as_bytes())]);
        let target = lib.path("a.md");
        let mut fired = false;
        lib.store.before_commit = Some(Box::new(move || {
            if !fired {
                fired = true;
                fs::write(&target, "a\n\nb outside").unwrap();
            }
        }));
        let result = lib.save("a.md", "a here\n\nb", Some(base)).unwrap();
        assert_eq!(result.written.as_deref(), Some("a here\n\nb outside"));
        assert_eq!(lib.text("a.md"), "a here\n\nb outside", "neither side lost");
    }

    #[test]
    fn a_file_that_keeps_changing_is_not_written_over() {
        let base = "a\n\nb";
        let mut lib = Library::with(&[("a.md", base.as_bytes())]);
        let target = lib.path("a.md");
        let mut round = 0;
        lib.store.before_commit = Some(Box::new(move || {
            round += 1;
            fs::write(&target, format!("a\n\nb outside {round}")).unwrap();
        }));
        let result = lib.save("a.md", "a here\n\nb", Some(base));
        assert!(matches!(result, Err(SaveError::KeepsChanging)));
        assert_eq!(lib.text("a.md"), format!("a\n\nb outside {SAVE_ATTEMPTS}"), "the latest outside version stands");
        assert_eq!(lib.store.note(&lib.id("a.md")).unwrap().content, base, "the store's note is unchanged");
        // The next save, with no one writing, merges it.
        lib.store.before_commit = None;
        lib.save("a.md", "a here\n\nb", Some(base)).unwrap();
        assert_eq!(lib.text("a.md"), format!("a here\n\nb outside {SAVE_ATTEMPTS}"));
    }

    #[test]
    fn a_crlf_note_racing_an_edit_loses_neither_side() {
        // Swift treats CRLF as one Character, so the whole note is one "line" to
        // Merge3: a conflict, with both versions kept.
        let base = "a\r\n\r\nb\r\n";
        let mut lib = Library::with(&[("a.md", base.as_bytes())]);
        lib.write("a.md", "a\r\n\r\nb outside\r\n");
        let result = lib.save("a.md", "a here\r\n\r\nb\r\n", Some(base)).unwrap();
        assert_eq!(lib.bytes("a.md"), b"a here\r\n\r\nb\r\n");
        assert_eq!(fs::read(&result.conflict_copy.unwrap().path).unwrap(), b"a\r\n\r\nb outside\r\n");
    }

    #[test]
    fn a_failed_conflict_copy_stops_the_save() {
        // Swift goes on and writes over the outside version with no copy of it.
        let mut lib = Library::with(&[("sub/a.md", b"a")]);
        lib.write("sub/a.md", "theirs");
        let folder = lib.path("sub");
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
        let result = lib.save("sub/a.md", "mine", Some("a"));
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(result, Err(SaveError::ConflictCopyFailed(_))));
        assert_eq!(lib.text("sub/a.md"), "theirs");
    }

    #[test]
    fn a_file_deleted_outside_comes_back_with_the_save() {
        let mut lib = Library::with(&[("a.md", b"text")]);
        fs::remove_file(lib.path("a.md")).unwrap();
        let result = lib.save("a.md", "typed", Some("text")).unwrap();
        assert_eq!(result.written.as_deref(), Some("typed"));
        assert_eq!(lib.text("a.md"), "typed");
    }

    #[test]
    fn saving_what_the_file_already_holds_writes_nothing() {
        let mut lib = Library::with(&[("a.md", b"same")]);
        let ino = fs::metadata(lib.path("a.md")).unwrap().ino();
        lib.write("a.md", "converged");
        let ino_after_outside = fs::metadata(lib.path("a.md")).unwrap().ino();
        assert_eq!(ino, ino_after_outside);
        let result = lib.save("a.md", "converged", Some("same")).unwrap();
        assert_eq!(result.written.as_deref(), Some("converged"));
        assert_eq!(fs::metadata(lib.path("a.md")).unwrap().ino(), ino, "not replaced");
        assert_eq!(lib.store.note(&lib.id("a.md")).unwrap().updated_at, NOW.secs);
    }

    // MARK: Saved bytes are exact

    /// Bytes a note might hold, and what they read as.
    const EXACT: &[(&str, &[u8], &str)] = &[
        ("crlf", b"a\r\nb\r\n", "a\r\nb\r\n"),
        ("no final newline", b"a\nb", "a\nb"),
        ("trailing blank lines", b"a\n\n\n", "a\n\n\n"),
        ("cjk", "标题\n\n正文，还有全角空格\u{3000}\n".as_bytes(), "标题\n\n正文，还有全角空格\u{3000}\n"),
        ("line separator", "a\u{2028}b\u{2029}c".as_bytes(), "a\u{2028}b\u{2029}c"),
        ("unusual whitespace", b"a  \t\n\x0b\x0c\n b \n", "a  \t\n\x0b\x0c\n b \n"),
        ("lone cr", b"a\rb\r", "a\rb\r"),
        ("mixed line endings", b"a\nb\r\nc\rd", "a\nb\r\nc\rd"),
        ("nul", b"a\0b", "a\0b"),
        ("composed and decomposed", "é e\u{301}".as_bytes(), "é e\u{301}"),
        ("emoji", "👨‍👩‍👧 🇨🇳\n".as_bytes(), "👨‍👩‍👧 🇨🇳\n"),
        ("empty", b"", ""),
    ];

    #[test]
    fn text_reads_and_saves_byte_for_byte() {
        for (label, bytes, text) in EXACT {
            let mut lib = Library::with(&[("a.md", bytes)]);
            assert_eq!(lib.store.note(&lib.id("a.md")).unwrap().content, *text, "{label}: reads");

            // Saved as it is, the file isn't even replaced.
            let ino = fs::metadata(lib.path("a.md")).unwrap().ino();
            lib.save("a.md", text, None).unwrap();
            assert_eq!(lib.bytes("a.md"), *bytes, "{label}: unchanged save");
            assert_eq!(fs::metadata(lib.path("a.md")).unwrap().ino(), ino, "{label}: no write");

            // Saved after an edit, every other byte stays.
            let edited = format!("{text}!");
            lib.save("a.md", &edited, None).unwrap();
            let mut expected = bytes.to_vec();
            expected.push(b'!');
            assert_eq!(lib.bytes("a.md"), expected, "{label}: edited save");

            // A rescan sees just what was saved.
            lib.store.reload(NOW.secs);
            assert_eq!(lib.store.note(&lib.id("a.md")).unwrap().content, edited, "{label}: reread");
        }
    }

    #[test]
    fn merges_keep_the_bytes_of_lines_nobody_touched() {
        let base = "标题 \n\t正文\u{2028}\n\n结尾";
        let mut lib = Library::with(&[("a.md", base.as_bytes())]);
        lib.write("a.md", "标题 \n\t正文\u{2028}\n\n结尾！");
        lib.save("a.md", "标题！ \n\t正文\u{2028}\n\n结尾", Some(base)).unwrap();
        assert_eq!(lib.text("a.md"), "标题！ \n\t正文\u{2028}\n\n结尾！");
    }

    #[test]
    fn a_bom_stays_on_the_file_and_off_the_text() {
        // Foundation strips the BOM on reading, so Swift's save loses it; the
        // editor here sees the same text and the file keeps it.
        let mut lib = Library::with(&[("a.md", b"\xEF\xBB\xBF# Title\r\nbody")]);
        let id = lib.id("a.md");
        let note = lib.store.note(&id).unwrap();
        assert_eq!(note.content, "# Title\r\nbody");
        assert_eq!(note.title, title::extract("# Title\r\nbody"));

        let ino = fs::metadata(lib.path("a.md")).unwrap().ino();
        lib.save("a.md", "# Title\r\nbody", None).unwrap();
        assert_eq!(fs::metadata(lib.path("a.md")).unwrap().ino(), ino, "same text: no write");

        lib.save("a.md", "# Title\r\nbody!", None).unwrap();
        assert_eq!(lib.bytes("a.md"), b"\xEF\xBB\xBF# Title\r\nbody!");
    }

    #[test]
    fn a_bom_survives_a_merge_and_goes_with_the_conflict_copy() {
        let mut lib = Library::with(&[("a.md", b"\xEF\xBB\xBFa\n\nb"), ("c.md", b"\xEF\xBB\xBFa")]);
        lib.write("a.md", b"\xEF\xBB\xBFa\n\nb outside");
        lib.save("a.md", "a here\n\nb", Some("a\n\nb")).unwrap();
        assert_eq!(lib.bytes("a.md"), b"\xEF\xBB\xBFa here\n\nb outside");

        lib.write("c.md", b"\xEF\xBB\xBFtheirs");
        let result = lib.save("c.md", "mine", Some("a")).unwrap();
        assert_eq!(lib.bytes("c.md"), b"\xEF\xBB\xBFmine");
        assert_eq!(fs::read(result.conflict_copy.unwrap().path).unwrap(), b"\xEF\xBB\xBFtheirs");
    }

    #[test]
    fn only_a_leading_bom_is_the_files() {
        let mut lib = Library::with(&[("a.md", "\u{FEFF}\u{FEFF}a\u{FEFF}".as_bytes())]);
        assert_eq!(lib.store.note(&lib.id("a.md")).unwrap().content, "\u{FEFF}a\u{FEFF}");
        lib.save("a.md", "\u{FEFF}a\u{FEFF}b", None).unwrap();
        assert_eq!(lib.bytes("a.md"), "\u{FEFF}\u{FEFF}a\u{FEFF}b".as_bytes());
    }

    // MARK: Files that aren't UTF-8

    /// GBK for 你好, and UTF-8 cut off inside a character.
    const NOT_UTF8: &[&[u8]] = &[b"\xC4\xE3\xBA\xC3", b"ok \xE4\xB8", b"\xFF\xFE\x00a"];

    #[test]
    fn a_file_that_is_not_utf8_is_listed_empty_and_never_overwritten() {
        for bytes in NOT_UTF8 {
            let mut lib = Library::with(&[("gbk.md", bytes)]);
            let id = lib.id("gbk.md");
            let note = lib.store.note(&id).unwrap().clone();
            assert_eq!(note.content, "", "listed as Swift lists it");
            assert!(lib.store.is_unreadable(&id));

            // What Swift does: base "", disk nil, so it writes the typed text
            // over the file. Here each way of saving is refused.
            for base in [None, Some(""), Some("anything")] {
                let result = lib.save("gbk.md", "typed", base);
                assert!(matches!(result, Err(SaveError::NotUtf8)), "{base:?}");
                assert_eq!(lib.bytes("gbk.md"), *bytes, "{base:?}");
            }
            assert_eq!(lib.store.note(&id).unwrap(), &note, "the note in memory didn't change");
            assert_eq!(lib.listing(), ["gbk.md"], "no temporary file or copy left");
        }
    }

    #[test]
    fn a_file_turning_non_utf8_while_open_is_not_overwritten() {
        let mut lib = Library::with(&[("a.md", b"text")]);
        lib.write("a.md", b"\xC4\xE3\xBA\xC3");
        assert!(matches!(lib.save("a.md", "typed", Some("text")), Err(SaveError::NotUtf8)));
        assert_eq!(lib.bytes("a.md"), b"\xC4\xE3\xBA\xC3");
        // Or in the last moment of the write.
        lib.write("a.md", "text");
        let target = lib.path("a.md");
        lib.store.before_commit = Some(Box::new(move || fs::write(&target, b"\xC4\xE3\xBA\xC3").unwrap()));
        assert!(matches!(lib.save("a.md", "typed", Some("text")), Err(SaveError::NotUtf8)));
        assert_eq!(lib.bytes("a.md"), b"\xC4\xE3\xBA\xC3");
    }

    #[test]
    fn a_non_utf8_file_can_be_moved_and_renamed_with_its_bytes() {
        let mut lib = Library::with(&[("gbk.md", b"\xC4\xE3"), ("sub/x.md", b"x")]);
        lib.store.move_item(&lib.path("gbk.md"), "/sub", NOW).unwrap();
        assert_eq!(lib.bytes("sub/gbk.md"), b"\xC4\xE3", "relinking skips it");
        assert!(lib.store.is_unreadable(&lib.id("sub/gbk.md")));
        lib.store.rename_note(&lib.id("sub/gbk.md"), "renamed", NOW).unwrap();
        assert_eq!(lib.bytes("sub/renamed.md"), b"\xC4\xE3");
        assert!(lib.store.is_unreadable(&lib.id("sub/renamed.md")));
        assert!(matches!(lib.save("sub/renamed.md", "typed", None), Err(SaveError::NotUtf8)));
    }

    #[test]
    fn a_file_fixed_outside_can_be_edited_again() {
        let mut lib = Library::with(&[("a.md", b"\xC4\xE3")]);
        lib.write("a.md", "你好");
        lib.store.reload(NOW.secs);
        assert!(!lib.store.is_unreadable(&lib.id("a.md")));
        lib.save("a.md", "你好！", None).unwrap();
        assert_eq!(lib.text("a.md"), "你好！");
    }

    #[test]
    fn a_save_that_cannot_read_the_file_is_refused() {
        // Swift can't tell this from a missing file, and writes.
        let mut lib = Library::with(&[("a.md", b"secret")]);
        let path = lib.path("a.md");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        let result = lib.save("a.md", "typed", Some("secret"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(result, Err(SaveError::Io(_))));
        assert_eq!(lib.text("a.md"), "secret");
    }

    #[test]
    fn a_memo_on_a_non_utf8_day_file_is_refused() {
        let day = "Memos/2026-10-17.md";
        let mut lib = Library::with(&[(day, b"\xC4\xE3")]);
        assert!(lib.store.add_memo("hello", NOW).is_err());
        assert_eq!(lib.bytes(day), b"\xC4\xE3");
        // Before the scan has seen it, too.
        lib.store.close_workspace();
        lib.store.open_workspace(lib.dir.path(), Vec::new());
        assert!(lib.store.add_memo("hello", NOW).is_err());
        assert_eq!(lib.bytes(day), b"\xC4\xE3");
    }

    // MARK: Writes are atomic

    #[test]
    fn a_write_replaces_the_file_in_one_step_and_keeps_its_mode() {
        let mut lib = Library::with(&[("a.md", b"old")]);
        fs::set_permissions(lib.path("a.md"), fs::Permissions::from_mode(0o640)).unwrap();
        let before = fs::metadata(lib.path("a.md")).unwrap().ino();
        lib.save("a.md", "new", None).unwrap();
        let after = fs::metadata(lib.path("a.md")).unwrap();
        assert_ne!(after.ino(), before, "a new file renamed into place, not the old one rewritten");
        assert_eq!(after.permissions().mode() & 0o777, 0o640);
        assert_eq!(lib.listing(), ["a.md"], "no temporary file left");
    }

    #[test]
    fn readers_never_see_a_partial_write() {
        let big_a = "A".repeat(2_000_000);
        let big_b = "B".repeat(2_000_000);
        let mut lib = Library::with(&[("a.md", big_a.as_bytes())]);
        let path = lib.path("a.md");
        let stop = Arc::new(AtomicBool::new(false));
        let reader = {
            let (path, stop) = (path.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut reads = 0;
                while !stop.load(Ordering::Relaxed) {
                    let bytes = fs::read(&path).expect("the file is always there");
                    assert_eq!(bytes.len(), 2_000_000, "whole");
                    assert!(bytes.iter().all(|&byte| byte == bytes[0]), "one version");
                    reads += 1;
                }
                reads
            })
        };
        for round in 0..20 {
            lib.save("a.md", if round % 2 == 0 { &big_b } else { &big_a }, None).unwrap();
        }
        stop.store(true, Ordering::Relaxed);
        assert!(reader.join().unwrap() > 0);
    }

    #[test]
    fn a_failed_write_leaves_the_old_file_and_the_note() {
        let mut lib = Library::with(&[("sub/a.md", b"old")]);
        let folder = lib.path("sub");
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
        let result = lib.save("sub/a.md", "new", Some("old"));
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(result, Err(SaveError::Io(_))), "Swift ignores this and updates its note");
        assert_eq!(lib.text("sub/a.md"), "old");
        assert_eq!(lib.store.note(&lib.id("sub/a.md")).unwrap().content, "old");
        assert_eq!(lib.listing(), ["sub", "sub/a.md"]);
    }

    #[test]
    fn stopping_before_the_rename_leaves_the_old_file() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.md");
        fs::write(&path, "old").unwrap();
        let abandoned = write_atomic_if(&path, b"new", &mut || Ok(false)).unwrap();
        assert!(matches!(abandoned, Commit::Abandoned));
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert!(write_atomic_if(&path, b"new", &mut || Err(io::ErrorKind::Other.into())).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 1, "no temporary file left");
    }

    #[test]
    fn a_symlinked_note_is_written_through() {
        // Foundation's atomic write puts a regular file where the link was, and
        // the file it pointed at goes stale.
        let mut lib = Library::with(&[("real.md", b"old")]);
        std::os::unix::fs::symlink(lib.path("real.md"), lib.path("link.md")).unwrap();
        lib.store.reload(NOW.secs);
        lib.save("link.md", "new", None).unwrap();
        assert_eq!(lib.text("real.md"), "new");
        assert!(fs::symlink_metadata(lib.path("link.md")).unwrap().file_type().is_symlink());
    }

    #[test]
    fn new_files_never_replace_anything() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.md");
        write_new_atomic(&path, b"first").unwrap();
        let error = write_new_atomic(&path, b"second").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).unwrap(), b"first");
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 1, "no temporary file left");
    }

    #[test]
    fn creating_a_new_file_in_a_missing_folder_fails_cleanly() {
        let mut lib = Library::with(&[]);
        let missing = lib.path("nowhere");
        assert!(lib.store.create_note(Some("x"), Some(&missing), NOW).is_err());
        assert_eq!(lib.store.notes().len(), 0, "no note without a file");
    }

    // MARK: Scanning

    fn titles(store: &NoteStore) -> Vec<&str> {
        store.notes().iter().map(|note| note.title.as_str()).collect()
    }

    #[test]
    fn a_scan_finds_notes_and_files_and_skips_hidden_ones() {
        let lib = Library::with(&[
            ("a.md", b"# A"),
            ("b.MARKDOWN", b"# B"),
            ("产品思考/草稿/c.md", b"# C"),
            ("notes.txt", b"not a note"),
            ("attachments/shot.PNG", b""),
            ("attachments/clip.mov", b""),
            ("doc.pdf", b""),
            (".hidden.md", b"# H"),
            (".git/d.md", b"# D"),
            ("sub/.obsidian/e.md", b"# E"),
            (".DS_Store", b""),
        ]);
        let mut found: Vec<(&str, &str)> = lib.store.notes().iter().map(|note| (note.title.as_str(), note.relative_path.as_str())).collect();
        found.sort();
        assert_eq!(found, [("A", "/"), ("B", "/"), ("C", "/产品思考/草稿")]);
        let mut files: Vec<(String, &str)> = lib.store.files().iter().map(|file| (file.name(), file.relative_path.as_str())).collect();
        files.sort();
        assert_eq!(files, [("clip.mov".to_string(), "/attachments"), ("doc.pdf".to_string(), "/"), ("shot.PNG".to_string(), "/attachments")]);
        assert!(lib.store.has_loaded_notes());
    }

    #[test]
    fn a_scan_skips_what_the_finder_hides() {
        let lib = Library::with(&[("a.md", b"# A"), ("b.md", b"# B"), ("Folder/c.md", b"# C")]);
        for name in ["b.md", "Folder"] {
            let status = std::process::Command::new("chflags").arg("hidden").arg(lib.path(name)).status().unwrap();
            assert!(status.success());
        }
        let scan = scan_library(lib.dir.path(), &HashSet::new(), NOW.secs);
        assert_eq!(scan.notes.iter().map(|note| note.title.as_str()).collect::<Vec<_>>(), ["A"]);
    }

    #[test]
    fn a_folder_named_like_a_note_is_a_folder() {
        // Swift lists `x.md/` as an empty note as well.
        let lib = Library::with(&[("x.md/inner.md", b"# Inner"), ("y.png/z.md", b"# Z")]);
        let mut found = titles(&lib.store);
        found.sort();
        assert_eq!(found, ["Inner", "Z"]);
        assert_eq!(lib.store.files().len(), 0);
    }

    #[test]
    fn a_scan_does_not_follow_folder_symlinks() {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("o.md"), "# O").unwrap();
        let mut lib = Library::with(&[("a.md", b"# A")]);
        std::os::unix::fs::symlink(outside.path(), lib.path("linked")).unwrap();
        lib.store.reload(NOW.secs);
        assert_eq!(titles(&lib.store), ["A"]);
    }

    #[test]
    fn notes_sort_pinned_first_then_by_date() {
        let lib = Library::with(&[("old.md", b"# Old"), ("new.md", b"# New"), ("pin.md", b"# Pin")]);
        for (name, secs) in [("old.md", 1_000), ("new.md", 3_000), ("pin.md", 2_000)] {
            let file = fs::File::options().write(true).open(lib.path(name)).unwrap();
            file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(secs)).unwrap();
        }
        let pinned: HashSet<String> = [lib.id("pin.md")].into_iter().collect();
        let scan = scan_library(lib.dir.path(), &pinned, NOW.secs);
        assert_eq!(scan.notes.iter().map(|note| note.title.as_str()).collect::<Vec<_>>(), ["Pin", "New", "Old"]);
        assert!(scan.notes[0].pinned);
        assert_eq!(scan.notes[1].updated_at, 3_000.0);
    }

    #[test]
    fn notes_carry_titles_and_tags_from_their_text() {
        let lib = Library::with(&[("a.md", "---\ntags: [x, y]\n---\n# 标题\n".as_bytes())]);
        let note = lib.store.note(&lib.id("a.md")).unwrap();
        assert_eq!(note.title, "标题");
        assert_eq!(note.tags, ["x", "y"]);
        assert_eq!(note.filename(), "a");
    }

    #[test]
    fn a_save_updates_the_note_in_memory() {
        let mut lib = Library::with(&[("a.md", b"# Old")]);
        lib.save("a.md", "---\ntags: [t]\n---\n# New", None).unwrap();
        let note = lib.store.note(&lib.id("a.md")).unwrap();
        assert_eq!(note.title, "New");
        assert_eq!(note.tags, ["t"]);
        assert_eq!(note.updated_at, NOW.secs);
    }

    #[test]
    fn reload_says_what_changed() {
        let mut lib = Library::with(&[("a.md", b"# A")]);
        let same = lib.store.reload(NOW.secs);
        assert_eq!(same, ReloadChange { applied: true, notes_changed: false, files_changed: false });
        lib.write("b.png", "");
        assert!(lib.store.reload(NOW.secs).files_changed);
        lib.write("b.md", "# B");
        assert!(lib.store.reload(NOW.secs).notes_changed);
    }

    #[test]
    fn an_overtaken_scan_is_dropped() {
        let mut lib = Library::with(&[("a.md", b"# A")]);
        let stale = lib.store.begin_reload().unwrap();
        let stale_scan = stale.scan(NOW.secs);
        lib.write("b.md", "# B");
        let newer = lib.store.begin_reload().unwrap();
        let newer_scan = newer.scan(NOW.secs);
        assert!(lib.store.apply_reload(&newer, newer_scan).applied);
        assert!(!lib.store.apply_reload(&stale, stale_scan).applied);
        assert_eq!(lib.store.notes().len(), 2, "the newer scan stands");

        let ticket = lib.store.begin_reload().unwrap();
        let scan = ticket.scan(NOW.secs);
        lib.store.close_workspace();
        assert!(!lib.store.apply_reload(&ticket, scan).applied);
        assert_eq!(lib.store.notes().len(), 0);
        assert!(!lib.store.has_loaded_notes());
    }

    #[test]
    fn nothing_happens_without_a_library() {
        let mut store = NoteStore::new();
        assert!(!store.is_open());
        assert!(store.begin_reload().is_none());
        assert_eq!(store.reload(NOW.secs), ReloadChange::default());
        assert!(store.draft_note(NOW).is_none());
        assert!(store.create_note(None, None, NOW).unwrap().is_none());
        assert_eq!(store.move_item(Path::new("/a.md"), "/", NOW), Err(Refusal::OutsideLibrary));
        assert_eq!(store.save_content("x", "/a.md", None, NOW).unwrap(), SaveResult::default());
    }

    // MARK: Own writes and debouncing

    #[test]
    fn our_own_saves_do_not_ask_for_a_rescan() {
        let mut lib = Library::with(&[("a.md", b"old")]);
        let id = lib.id("a.md");
        lib.save("a.md", "ours", None).unwrap();
        assert!(!lib.store.handle_file_events(&[&id], NOW.secs + 1.0), "our own write");
        assert_eq!(lib.store.reload_deadline(), None);

        lib.write("a.md", "theirs");
        assert!(lib.store.handle_file_events(&[&id], NOW.secs + 1.0), "written over since");
        assert_eq!(lib.store.reload_deadline(), Some(NOW.secs + 1.0 + RELOAD_DEBOUNCE));
    }

    #[test]
    fn an_own_write_is_only_remembered_for_a_while() {
        let mut lib = Library::with(&[("a.md", b"old")]);
        let id = lib.id("a.md");
        lib.save("a.md", "ours", None).unwrap();
        assert!(!lib.store.handle_file_events(&[&id], NOW.secs + OWN_WRITE_LIFETIME - 0.1));
        assert!(lib.store.handle_file_events(&[&id], NOW.secs + OWN_WRITE_LIFETIME), "exactly at the limit it's gone");
    }

    #[test]
    fn events_filter_and_decide() {
        let mut lib = Library::with(&[("a.md", b"a")]);
        let git = lib.id(".git/index");
        assert!(!lib.store.handle_file_events(&[&git], NOW.secs), "only inside hidden folders");
        let temp = lib.id(".fumi-tmp-1-1");
        assert!(!lib.store.handle_file_events(&[&temp], NOW.secs), "our temporary files");
        let folder = lib.id("sub");
        assert!(lib.store.handle_file_events(&[&folder], NOW.secs), "a bare folder event reloads to be safe");
        let none: [&str; 0] = [];
        assert!(!lib.store.handle_file_events(&none, NOW.secs + 9.0), "no events");
    }

    #[test]
    fn the_copy_of_a_conflict_counts_as_our_own_write() {
        let mut lib = Library::with(&[("a.md", b"a")]);
        lib.write("a.md", "theirs");
        let copy = lib.save("a.md", "mine", Some("a")).unwrap().conflict_copy.unwrap();
        let events = [lib.id("a.md"), copy.id];
        assert!(!lib.store.handle_file_events(&events, NOW.secs + 1.0));
    }

    #[test]
    fn own_writes_compare_without_the_bom() {
        let mut lib = Library::with(&[("a.md", b"\xEF\xBB\xBFold")]);
        let id = lib.id("a.md");
        lib.save("a.md", "new", None).unwrap();
        assert!(!lib.store.handle_file_events(&[&id], NOW.secs));
    }

    #[test]
    fn a_non_utf8_rewrite_counts_as_external() {
        let mut lib = Library::with(&[("a.md", b"old")]);
        let id = lib.id("a.md");
        lib.save("a.md", "ours", None).unwrap();
        lib.write("a.md", b"\xFF");
        assert!(lib.store.handle_file_events(&[&id], NOW.secs));
    }

    #[test]
    fn the_debounce_waits_for_things_to_settle() {
        let mut debounce = ReloadDebounce::default();
        assert!(!debounce.take_due(100.0), "nothing scheduled");
        debounce.schedule(100.0);
        assert_eq!(debounce.deadline(), Some(100.0 + RELOAD_DEBOUNCE));
        assert!(!debounce.take_due(100.4));
        debounce.schedule(100.4);
        assert!(!debounce.take_due(100.5), "pushed back by the second change");
        assert!(debounce.take_due(100.9));
        assert!(!debounce.take_due(101.0), "once for each scheduling");
        debounce.schedule(200.0);
        debounce.cancel();
        assert!(!debounce.take_due(300.0));
    }

    #[test]
    fn closing_the_library_cancels_a_scheduled_reload() {
        let mut lib = Library::with(&[]);
        lib.store.schedule_reload(NOW.secs);
        lib.store.close_workspace();
        assert!(!lib.store.reload_is_due(NOW.secs + 9.0));
    }

    #[test]
    fn creating_and_moving_schedule_a_reload() {
        let mut lib = Library::with(&[("sub/x.md", b"x")]);
        lib.store.create_note(None, None, NOW).unwrap();
        assert!(lib.store.reload_is_due(NOW.secs + RELOAD_DEBOUNCE));
        lib.store.move_item(&lib.path("Untitled.md"), "/sub", NOW).unwrap();
        assert!(lib.store.reload_is_due(NOW.secs + RELOAD_DEBOUNCE));
    }

    // MARK: New notes

    #[test]
    fn new_notes_get_a_free_name() {
        let mut lib = Library::with(&[("Untitled.md", b"keep me")]);
        let note = lib.store.create_note(None, None, NOW).unwrap().unwrap();
        assert_eq!(note.filename(), "Untitled 2");
        assert_eq!(lib.text("Untitled 2.md"), "# Untitled\n\n");
        assert_eq!(lib.text("Untitled.md"), "keep me");
        assert_eq!(lib.store.notes()[0].id, note.id, "first in the list");
        assert_eq!(note.created_at, NOW.secs);

        let titled = lib.store.create_note(Some("a/b: 计划"), None, NOW).unwrap().unwrap();
        assert_eq!(titled.filename(), "a-b- 计划");
        assert_eq!(lib.text("a-b- 计划.md"), "# a/b: 计划\n\n");
        assert_eq!(titled.title, "a/b: 计划");
    }

    #[test]
    fn new_notes_go_in_the_folder_asked_for() {
        let mut lib = Library::with(&[("sub/x.md", b"x")]);
        let folder = lib.path("sub");
        let note = lib.store.create_note(Some("n"), Some(&folder), NOW).unwrap().unwrap();
        assert_eq!(note.relative_path, "/sub");
        assert!(lib.path("sub/n.md").exists());
    }

    #[test]
    fn file_names_are_sanitized_like_swift() {
        assert_eq!(sanitize_for_filename(r#"a/b\c:d*e?f"g<h>i|j"#), "a-b-c-d-e-f-g-h-i-j");
        assert_eq!(sanitize_for_filename("  x  "), "x");
        assert_eq!(sanitize_for_filename("\u{3000}\u{A0}\t\u{200B}x\u{3000}"), "x");
        assert_eq!(sanitize_for_filename("\nx\n"), "\nx\n", "newlines aren't in `.whitespaces`");
        assert_eq!(sanitize_for_filename("   "), "Untitled");
        assert_eq!(sanitize_for_filename(""), "Untitled");
        assert_eq!(sanitize_for_filename("计划 2026"), "计划 2026");
    }

    // MARK: Deleting

    #[test]
    fn deleting_removes_the_file_and_the_note() {
        let mut lib = Library::with(&[("a.md", b"a"), ("b.md", b"b")]);
        assert!(lib.store.delete_note(&lib.id("a.md")).unwrap());
        assert!(!lib.path("a.md").exists());
        assert!(lib.path("b.md").exists());
        assert_eq!(lib.store.notes().len(), 1);
        assert!(!lib.store.delete_note(&lib.id("a.md")).unwrap(), "no such note");
    }

    #[test]
    fn a_note_whose_file_cannot_be_deleted_stays_listed() {
        let mut lib = Library::with(&[("sub/a.md", b"a")]);
        let folder = lib.path("sub");
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
        let result = lib.store.delete_note(&lib.id("sub/a.md"));
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
        assert_eq!(lib.store.notes().len(), 1);
    }

    // MARK: Pinning

    #[test]
    fn pins_sort_first_and_report_their_changes() {
        let mut lib = Library::with(&[("a.md", b"# A"), ("b.md", b"# B")]);
        let b = lib.id("b.md");
        assert_eq!(lib.store.toggle_pin(&b), Some(true));
        assert_eq!(lib.store.notes()[0].id, b);
        assert_eq!(lib.store.pinned_paths(), [b.clone()]);
        assert!(lib.store.take_pins_changed());
        assert!(!lib.store.take_pins_changed(), "once");
        assert_eq!(lib.store.toggle_pin("/nope.md"), None);
        lib.store.reload(NOW.secs);
        assert!(lib.store.note(&b).unwrap().pinned, "a rescan keeps it");
        assert_eq!(lib.store.toggle_pin(&b), Some(false));
        assert!(lib.store.pinned_paths().is_empty());
    }

    #[test]
    fn pins_load_when_a_library_opens() {
        let lib = Library::with(&[("a.md", b"# A")]);
        let mut store = NoteStore::new();
        store.open_workspace(lib.dir.path(), vec![lib.id("a.md")]);
        store.reload(NOW.secs);
        assert!(store.notes()[0].pinned);
    }

    #[test]
    fn pins_follow_renames_and_moves() {
        let mut lib = Library::with(&[("a.md", b"# A"), ("folder/b.md", b"# B"), ("sub/x.md", b"x")]);
        let a = lib.id("a.md");
        let b = lib.id("folder/b.md");
        lib.store.toggle_pin(&a);
        lib.store.toggle_pin(&b);
        lib.store.take_pins_changed();

        let renamed = lib.store.rename_note(&a, "c", NOW).unwrap();
        assert!(lib.store.take_pins_changed());
        assert_eq!(lib.store.pinned_paths(), [lib.id("c.md"), b.clone()]);
        assert_eq!(renamed, lib.id("c.md"));

        lib.store.move_item(&lib.path("folder"), "/sub", NOW).unwrap();
        assert!(lib.store.take_pins_changed());
        assert_eq!(lib.store.pinned_paths(), [lib.id("c.md"), lib.id("sub/folder/b.md")]);
    }

    // MARK: Renaming

    #[test]
    fn renaming_moves_the_file_and_the_note() {
        let mut lib = Library::with(&[("sub/a.md", b"# A")]);
        let new_id = lib.store.rename_note(&lib.id("sub/a.md"), "计划: 下周", NOW).unwrap();
        assert_eq!(new_id, lib.id("sub/计划- 下周.md"));
        let note = lib.store.note(&new_id).unwrap();
        assert_eq!(note.title, "计划: 下周", "the title as typed, as in Swift");
        assert_eq!(note.content, "# A");
        assert_eq!(note.relative_path, "/sub");
        assert_eq!(note.updated_at, NOW.secs);
        assert_eq!(lib.text("sub/计划- 下周.md"), "# A");
    }

    #[test]
    fn renaming_refuses_a_taken_or_unchanged_name() {
        let mut lib = Library::with(&[("a.md", b"text of a"), ("b.md", b"text of b")]);
        assert_eq!(lib.store.rename_note(&lib.id("a.md"), "b", NOW), None);
        assert_eq!(lib.store.rename_note(&lib.id("a.md"), "a", NOW), None);
        assert_eq!(lib.store.rename_note("/nope.md", "c", NOW), None);
        // An empty name sanitizes to "Untitled" in Swift too, whatever its comment says.
        let untitled = lib.store.rename_note(&lib.id("b.md"), "  ", NOW).unwrap();
        assert_eq!(untitled, lib.id("Untitled.md"));
        lib.store.rename_note(&untitled, "b", NOW).unwrap();
        assert_eq!(lib.text("a.md"), "text of a");
        assert_eq!(lib.text("b.md"), "text of b");
    }

    // MARK: Moving

    #[test]
    fn moving_a_folder_carries_its_notes_and_files() {
        let mut lib = Library::with(&[("p/a.md", b"# A"), ("p/q/b.md", b"# B"), ("p/pic.png", b""), ("z/x.md", b"x")]);
        let result = lib.store.move_item(&lib.path("p"), "/z", NOW).unwrap();
        assert_eq!(result, lib.path("z/p"));
        let a = lib.store.note(&lib.id("z/p/a.md")).unwrap();
        assert_eq!(a.relative_path, "/z/p");
        assert_eq!(a.path, lib.path("z/p/a.md"));
        assert_eq!(lib.store.note(&lib.id("z/p/q/b.md")).unwrap().relative_path, "/z/p/q");
        let file = &lib.store.files()[0];
        assert_eq!(file.id, lib.id("z/p/pic.png"));
        assert_eq!(file.relative_path, "/z/p");
    }

    #[test]
    fn a_move_never_overwrites() {
        let mut lib = Library::with(&[("a.md", b"top"), ("sub/a.md", b"inner")]);
        let refused = lib.store.move_item(&lib.path("a.md"), "/sub", NOW);
        assert_eq!(refused, Err(Refusal::NameTaken("a.md".into())));
        assert_eq!(lib.text("a.md"), "top");
        assert_eq!(lib.text("sub/a.md"), "inner");
        assert_eq!(lib.store.move_item(&lib.path("a.md"), "/", NOW), Err(Refusal::AlreadyThere));
        assert_eq!(lib.store.move_item(&lib.path("sub"), "/sub", NOW), Err(Refusal::IntoItself));
    }

    #[test]
    fn a_move_rewrites_links_that_would_break() {
        let content = "# A\r\n\r\n![](pic.png) 中文\r\n";
        let mut lib = Library::with(&[("p/a.md", content.as_bytes()), ("p/pic.png", b"")]);
        lib.store.move_item(&lib.path("p/a.md"), "/", NOW).unwrap();
        let expected = "# A\r\n\r\n![](p/pic.png) 中文\r\n";
        assert_eq!(lib.text("a.md"), expected, "the rest of the file is byte for byte");
        let id = lib.id("a.md");
        assert_eq!(lib.store.note(&id).unwrap().content, expected);
        assert!(!lib.store.handle_file_events(&[&id], NOW.secs + 1.0), "the rewrite counts as our own");
    }

    #[test]
    fn a_save_from_before_a_move_has_its_links_rewritten() {
        let content = "![](pic.png)\n";
        let mut lib = Library::with(&[("p/a.md", content.as_bytes()), ("p/pic.png", b"")]);
        let old_id = lib.id("p/a.md");
        lib.store.move_item(&lib.path("p/a.md"), "/", NOW).unwrap();
        let result = lib.store.save_content("![](pic.png)\n\ntyped\n", &old_id, Some(content), NOW).unwrap();
        assert_eq!(result.written.as_deref(), Some("![](p/pic.png)\n\ntyped\n"));
        assert_eq!(lib.text("a.md"), "![](p/pic.png)\n\ntyped\n");
    }

    // MARK: Memos

    #[test]
    fn memos_are_added_to_the_days_file() {
        let mut lib = Library::with(&[]);
        lib.store.add_memo("  ", NOW).unwrap();
        assert_eq!(lib.listing(), Vec::<String>::new(), "a blank memo isn't kept");

        lib.store.add_memo("first", NOW).unwrap();
        let day = "Memos/2026-10-17.md";
        assert!(lib.text(day).contains("first"));
        assert_eq!(lib.store.memo_note("2026-10-17").unwrap().id, lib.id(day));
        let id = lib.id(day);
        assert!(!lib.store.handle_file_events(&[&id], NOW.secs), "our own write");

        lib.store.add_memo("second", Now::new(NOW.secs + 60.0, 0)).unwrap();
        let memos = lib.store.memos();
        assert_eq!(memos.iter().map(|memo| memo.text.as_str()).collect::<Vec<_>>(), ["second", "first"], "latest first");
    }

    #[test]
    fn memos_sort_newest_day_first_and_ignore_other_files() {
        let lib = Library::with(&[
            ("Memos/2026-10-16.md", b"## 09:00\nolder\n"),
            ("Memos/2026-10-17.md", b"## 09:00\nnewer\n"),
            ("Memos/notes.md", b"## 09:00\nnot a day\n"),
            ("Other/2026-10-18.md", b"## 09:00\nnot in Memos\n"),
        ]);
        assert_eq!(lib.store.memos().iter().map(|memo| memo.text.as_str()).collect::<Vec<_>>(), ["newer", "older"]);
    }

    #[test]
    fn memos_can_be_changed_and_removed() {
        let day = "Memos/2026-10-17.md";
        let mut lib = Library::with(&[(day, b"## 09:00\none\n\n## 10:00\ntwo\n")]);
        let two = lib.store.memos().into_iter().find(|memo| memo.text == "two").unwrap();
        lib.store.update_memo(&two, "two!", NOW).unwrap();
        assert!(lib.text(day).contains("two!"));

        let memos = lib.store.memos();
        lib.store.delete_memo(&memos[0], NOW).unwrap();
        lib.store.delete_memo(&lib.store.memos()[0].clone(), NOW).unwrap();
        assert!(!lib.path(day).exists(), "a day left empty goes");
        assert!(lib.store.memos().is_empty());
    }

    // MARK: Smaller pieces

    #[test]
    fn relative_paths() {
        assert_eq!(relative_path_in("/w/a.md", "/w"), "/");
        assert_eq!(relative_path_in("/w/sub/a.md", "/w"), "/sub");
        assert_eq!(relative_path_in("/w/产品/草稿/a.md", "/w"), "/产品/草稿");
        assert_eq!(relative_path_in("/elsewhere/a.md", "/w"), "/");
    }

    #[test]
    fn a_trailing_slash_on_the_library_changes_nothing() {
        let lib = Library::with(&[("sub/a.md", b"# A")]);
        let mut store = NoteStore::new();
        store.open_workspace(Path::new(&format!("{}/", lib.dir.path().display())), Vec::new());
        store.reload(NOW.secs);
        assert_eq!(store.notes()[0].relative_path, "/sub");
        assert_eq!(store.notes()[0].id, lib.id("sub/a.md"));
    }

    #[test]
    fn clock_stamps_wrap_around_midnight() {
        assert_eq!(clock_stamp(NOW), "03-30");
        assert_eq!(clock_stamp(Now::new(NOW.secs, 8 * 3600)), "11-30");
        assert_eq!(clock_stamp(Now::new(NOW.secs, -4 * 3600)), "23-30");
    }
}
