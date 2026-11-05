//! The libraries the sidebar lists, in the order the user dragged them into —
//! which is also the order of ⌃⌘1–9. Every folder opened joins it; only Remove
//! from List takes one out. Port of `Services/LibraryList.swift`.
//!
//! Where the list is kept is the caller's: anything that holds a blob
//! ([`ListStorage`]). The blob is the JSON Swift's `JSONEncoder` writes for
//! `[LibraryEntry]`, under the key [`KEY`] in the Swift app's `UserDefaults`.

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

/// The `UserDefaults` key the Swift app keeps this list under.
pub const KEY: &str = "fumi.libraries";

/// One library on the sidebar's first level: a folder, with the name and icon
/// Fumi shows for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub path: String,
    /// Set by Rename…: only what Fumi shows, the folder keeps its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// One character or emoji, set by Change Icon…; without one the row shows
    /// the plain library icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl LibraryEntry {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into(), name: None, icon: None }
    }

    /// The name set by Rename…, otherwise the folder's own.
    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| last_path_component(&self.path).to_string())
    }

    /// Which of the six callout tints a chosen icon sits on. Follows the path,
    /// so renaming keeps the color.
    pub fn tint(&self) -> usize {
        let hash = self.path.bytes().fold(5381u32, |hash, byte| (hash << 5).wrapping_add(hash).wrapping_add(byte as u32));
        (hash % 6) as usize
    }
}

/// Holds the encoded list between launches.
pub trait ListStorage {
    /// The bytes last saved, if any.
    fn load(&self) -> Option<Vec<u8>>;
    fn save(&mut self, data: &[u8]);
}

/// The ordered list of libraries.
pub struct LibraryList<S: ListStorage> {
    entries: Vec<LibraryEntry>,
    storage: S,
}

impl<S: ListStorage> LibraryList<S> {
    /// What `storage` holds or, when it holds nothing readable, one entry for
    /// each path of `seed`, first mention only.
    pub fn new(storage: S, seed: impl FnOnce() -> Vec<String>) -> Self {
        let saved = storage.load().and_then(|data| serde_json::from_slice::<Vec<LibraryEntry>>(&data).ok());
        let entries = saved.unwrap_or_else(|| {
            let mut paths: Vec<String> = Vec::new();
            for path in seed() {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
            paths.into_iter().map(LibraryEntry::new).collect()
        });
        Self { entries, storage }
    }

    pub fn entries(&self) -> &[LibraryEntry] {
        &self.entries
    }

    pub fn entry_for(&self, path: &str) -> Option<&LibraryEntry> {
        let path = standardized_path(path);
        self.entries.iter().find(|entry| entry.path == path)
    }

    /// A folder just opened: last in the list, unless it's already there.
    pub fn add(&mut self, path: &str) {
        let path = standardized_path(path);
        if self.entries.iter().any(|entry| entry.path == path) {
            return;
        }
        self.entries.push(LibraryEntry::new(path));
        self.save();
    }

    pub fn remove(&mut self, path: &str) {
        self.entries.retain(|entry| entry.path != path);
        self.save();
    }

    /// Drops `path` where `target` is, the rows between shifting over.
    pub fn move_entry(&mut self, path: &str, target: &str) {
        if path == target {
            return;
        }
        let from = self.entries.iter().position(|entry| entry.path == path);
        let to = self.entries.iter().position(|entry| entry.path == target);
        let (Some(from), Some(to)) = (from, to) else { return };
        let entry = self.entries.remove(from);
        self.entries.insert(to, entry);
        self.save();
    }

    /// An empty name goes back to the folder's own.
    pub fn rename(&mut self, path: &str, name: &str) {
        self.update(path, |entry| entry.name = trimmed(name));
    }

    /// Keeps only the first character (an emoji counts as one); empty goes back
    /// to the plain library icon.
    pub fn set_icon(&mut self, path: &str, icon: &str) {
        self.update(path, |entry| {
            entry.icon = trimmed(icon).map(|text| text.graphemes(true).next().unwrap_or_default().to_string());
        });
    }

    fn update(&mut self, path: &str, change: impl FnOnce(&mut LibraryEntry)) {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.path == path) else { return };
        change(entry);
        self.save();
    }

    fn save(&mut self) {
        if let Ok(data) = serde_json::to_vec(&self.entries) {
            self.storage.save(&data);
        }
    }
}

/// Where the folder lives, short: "~/Documents", "iCloud Drive", "T7/Notes" for
/// an external disk.
pub fn location(path: &str, home: &str) -> String {
    let parent = deleting_last_path_component(&standardized_path(path));
    let icloud = format!("{home}/Library/Mobile Documents/com~apple~CloudDocs");
    if parent == icloud {
        return "iCloud Drive".to_string();
    }
    if let Some(rest) = parent.strip_prefix(&format!("{icloud}/")) {
        return format!("iCloud Drive/{rest}");
    }
    if parent == home {
        return "~".to_string();
    }
    if let Some(rest) = parent.strip_prefix(&format!("{home}/")) {
        return format!("~/{rest}");
    }
    if let Some(rest) = parent.strip_prefix("/Volumes/") {
        return rest.to_string();
    }
    parent
}

/// Why a library can't be opened, or `None` when its folder is there, asking
/// `exists` whether a path is a folder.
pub fn missing_reason(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let path = standardized_path(path);
    if exists(&path) {
        return None;
    }
    let parts = path_components(&path);
    if parts.len() >= 3 && parts[1] == "Volumes" && !exists(&format!("/Volumes/{}", parts[2])) {
        return Some(format!("Disk “{}” isn’t connected", parts[2]));
    }
    Some("Moved or deleted".to_string())
}

/// [`missing_reason`], looking on the disk.
pub fn missing_reason_on_disk(path: &str) -> Option<String> {
    missing_reason(path, |path| std::path::Path::new(path).is_dir())
}

fn trimmed(text: &str) -> Option<String> {
    let value = text.trim_matches(is_whitespace_or_newline);
    if value.is_empty() { None } else { Some(value.to_string()) }
}

/// Foundation's `CharacterSet.whitespacesAndNewlines`. Unlike `char::is_whitespace`
/// it holds U+200B, as `.whitespaces` does, and not U+180E or U+FEFF.
pub(crate) fn is_whitespace_or_newline(c: char) -> bool {
    is_whitespace(c) || matches!(c, '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Foundation's `CharacterSet.whitespaces`: spaces and tab, without newlines.
pub(crate) fn is_whitespace(c: char) -> bool {
    matches!(c, '\t' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200B}' | '\u{202F}' | '\u{205F}' | '\u{3000}')
}

/// `URL(fileURLWithPath:).standardizedFileURL.path` for an absolute path: no
/// `.`, `..`, repeated or trailing slashes, and `/private/tmp` written `/tmp`
/// (Foundation does that for `/private/tmp` only, not `/private/var`).
pub fn standardized_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    if parts.len() >= 2 && parts[0] == "private" && parts[1] == "tmp" {
        parts.remove(0);
    }
    format!("/{}", parts.join("/"))
}

/// `URL.pathComponents` of a standardized path: `"/"`, then each name.
pub fn path_components(path: &str) -> Vec<&str> {
    let mut parts = vec!["/"];
    parts.extend(path.split('/').filter(|part| !part.is_empty()));
    parts
}

/// `URL.lastPathComponent`: the last name, `"/"` for the root.
pub fn last_path_component(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return if path.is_empty() { "" } else { "/" };
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

fn deleting_last_path_component(path: &str) -> String {
    match path.trim_end_matches('/').rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(index) => path[..index].to_string(),
    }
}

#[cfg(test)]
mod tests {
    // Ported from Tests/LibraryListTests.swift.
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Memory(Rc<RefCell<Option<Vec<u8>>>>);

    impl ListStorage for Memory {
        fn load(&self) -> Option<Vec<u8>> {
            self.0.borrow().clone()
        }
        fn save(&mut self, data: &[u8]) {
            *self.0.borrow_mut() = Some(data.to_vec());
        }
    }

    fn paths<S: ListStorage>(list: &LibraryList<S>) -> Vec<&str> {
        list.entries().iter().map(|entry| entry.path.as_str()).collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn list_behaviour() {
        let defaults = Memory::default();

        // Seeding and adding
        let mut list = LibraryList::new(defaults.clone(), || strings(&["/a/Garden", "/b/Work", "/a/Garden"]));
        assert_eq!(paths(&list), ["/a/Garden", "/b/Work"], "seeded once each");
        list.add("/c/Reading");
        list.add("/b/Work/");
        assert_eq!(paths(&list), ["/a/Garden", "/b/Work", "/c/Reading"], "new ones last, no repeats");

        // Order, names, icons
        list.move_entry("/c/Reading", "/a/Garden");
        assert_eq!(paths(&list), ["/c/Reading", "/a/Garden", "/b/Work"], "dropped on the first row");
        list.move_entry("/c/Reading", "/b/Work");
        assert_eq!(paths(&list), ["/a/Garden", "/b/Work", "/c/Reading"], "dropped on the last row");

        list.rename("/b/Work", "  Work Notes ");
        assert_eq!(list.entry_for("/b/Work").map(LibraryEntry::display_name).as_deref(), Some("Work Notes"), "renamed");
        let tint = list.entries()[1].tint();
        list.rename("/b/Work", "");
        assert!(list.entries()[1].display_name() == "Work" && list.entries()[1].tint() == tint, "back to the folder's name, same tint");
        list.set_icon("/a/Garden", "📚 books");
        assert_eq!(list.entries()[0].icon.as_deref(), Some("📚"), "one emoji");
        list.set_icon("/a/Garden", " ");
        assert_eq!(list.entries()[0].icon, None, "empty: back to the plain icon");
        list.remove("/c/Reading");

        // Saved: a new list reads the same back, and ignores the seed.
        let reread = LibraryList::new(defaults.clone(), || strings(&["/x"]));
        assert_eq!(reread.entries(), list.entries(), "saved");
    }

    #[test]
    fn locations() {
        let home = "/Users/me";
        assert_eq!(location("/Users/me/Documents/Garden", home), "~/Documents", "home");
        assert_eq!(location("/Users/me/Garden", home), "~", "home itself");
        assert_eq!(location("/Users/me/Library/Mobile Documents/com~apple~CloudDocs/Reading", home), "iCloud Drive", "iCloud Drive");
        assert_eq!(location("/Volumes/T7/Notes/Archive", home), "T7/Notes", "a disk");
    }

    #[test]
    fn missing() {
        let none = |_: &str| false;
        assert_eq!(missing_reason("/Volumes/T7/Archive", none).as_deref(), Some("Disk “T7” isn’t connected"), "disk");
        assert_eq!(missing_reason("/Users/me/Gone", none).as_deref(), Some("Moved or deleted"), "gone");
        assert_eq!(missing_reason_on_disk("/tmp"), None, "there");
    }

    // ── Beyond the Swift test ──

    #[test]
    fn decodes_what_swift_wrote() {
        // Captured from Swift's JSONEncoder: `/` escaped, nil keys left out,
        // keys in whatever order.
        let json = r#"[{"path":"\/a\/b 中","icon":"📚"},{"name":"n","path":"\/x"}]"#;
        let defaults = Memory(Rc::new(RefCell::new(Some(json.as_bytes().to_vec()))));
        let list = LibraryList::new(defaults, || strings(&["/seed"]));
        assert_eq!(
            list.entries(),
            [
                LibraryEntry { path: "/a/b 中".into(), name: None, icon: Some("📚".into()) },
                LibraryEntry { path: "/x".into(), name: Some("n".into()), icon: None },
            ]
        );
        // An explicit null decodes too, as Swift's decodeIfPresent does.
        let nulls: Vec<LibraryEntry> = serde_json::from_str(r#"[{"path":"/y","name":null,"icon":null}]"#).unwrap();
        assert_eq!(nulls[0], LibraryEntry::new("/y"));
    }

    #[test]
    fn writes_what_swift_can_read() {
        let json = serde_json::to_string(&[LibraryEntry::new("/x")]).unwrap();
        assert_eq!(json, r#"[{"path":"/x"}]"#, "nil keys are left out");
    }

    #[test]
    fn unreadable_storage_falls_back_to_the_seed() {
        let defaults = Memory(Rc::new(RefCell::new(Some(b"not json".to_vec()))));
        let list = LibraryList::new(defaults.clone(), || strings(&["/seed"]));
        assert_eq!(paths(&list), ["/seed"]);
        assert_eq!(defaults.load().as_deref(), Some(&b"not json"[..]), "not written until something changes");
    }

    #[test]
    fn tint_matches_swift() {
        // Values printed by the Swift hash.
        for (path, tint) in [("/a/Garden", 5), ("/b/Work", 4), ("/c/Reading", 0), ("/产品/笔记 中文", 4), ("", 5)] {
            assert_eq!(LibraryEntry::new(path).tint(), tint, "{path}");
        }
    }

    #[test]
    fn icon_keeps_one_whole_grapheme() {
        let mut list = LibraryList::new(Memory::default(), || strings(&["/a"]));
        for (input, expected) in [("👨‍👩‍👧 family", "👨‍👩‍👧"), ("🇨🇳x", "🇨🇳"), ("e\u{301}x", "e\u{301}"), ("字典", "字")] {
            list.set_icon("/a", input);
            assert_eq!(list.entries()[0].icon.as_deref(), Some(expected));
        }
    }

    #[test]
    fn rename_trims_foundations_whitespace_set() {
        let mut list = LibraryList::new(Memory::default(), || strings(&["/a"]));
        list.rename("/a", "\u{3000}\u{200B}\n 笔记\u{2028}\u{A0}");
        assert_eq!(list.entries()[0].name.as_deref(), Some("笔记"));
        list.rename("/a", "\u{FEFF}x");
        assert_eq!(list.entries()[0].name.as_deref(), Some("\u{FEFF}x"), "U+FEFF isn't whitespace to Foundation");
    }

    #[test]
    fn paths_standardize_like_foundation() {
        // Each expected value printed by `standardizedFileURL.path`.
        for (input, expected) in [
            ("/b/Work/", "/b/Work"),
            ("/a/../b/./c", "/b/c"),
            ("/", "/"),
            ("/a//b", "/a/b"),
            ("/private/tmp/x", "/tmp/x"),
            ("/private/tmp", "/tmp"),
            ("/private/var/x", "/private/var/x"),
            ("/private", "/private"),
            ("/tmp/../private/tmp/x", "/tmp/x"),
            ("/a/b/../../..", "/"),
            ("/..", "/"),
            ("/./a/.", "/a"),
        ] {
            assert_eq!(standardized_path(input), expected, "{input}");
        }
        assert_eq!(last_path_component("/a.md/"), "a.md");
        assert_eq!(last_path_component("/"), "/");
        assert_eq!(path_components("/"), ["/"]);
        assert_eq!(path_components("/b/c"), ["/", "b", "c"]);
        assert_eq!(location("/x", "/Users/me"), "/");
        assert_eq!(location("/Volumes/T7/Archive", "/Users/me"), "T7");
    }
}
