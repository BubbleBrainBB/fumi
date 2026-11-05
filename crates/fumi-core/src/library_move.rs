//! The path rules for moving a note, file or folder to another folder of the
//! library. No file system access: whether a name is taken is asked of
//! `exists`. Port of `Services/LibraryMove.swift`.
//!
//! Paths compare as Swift strings do, by canonical equivalence, so a name in
//! NFD (as macOS often hands them out) matches the same name in NFC.

use crate::frontmatter::text;
use std::path::{Path, PathBuf};

/// Why an item can't move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The item is already in that folder.
    AlreadyThere,
    /// A folder can't go into itself or a folder inside it.
    IntoItself,
    /// The item or the folder isn't inside the library (or is the library).
    OutsideLibrary,
    /// The folder already holds an item with this name.
    NameTaken(String),
    /// The move itself failed (the file system's message).
    Failed(String),
}

/// The folder a sidebar folder id stands for: `/` is the library itself,
/// `/Projects/Drafts` a folder inside it.
pub fn folder_path(id: &str, library: &Path) -> PathBuf {
    let mut path = library.to_path_buf();
    for part in text::split_on(id, "/", false) {
        path.push(part);
    }
    path
}

/// The sidebar folder id of a folder inside the library (`/` for the library
/// itself), or `None` for one outside it.
pub fn folder_id(folder: &Path, library: &Path) -> Option<String> {
    let path = normalized(folder);
    let root = normalized(library);
    if text::canon_eq(&path, &root) {
        return Some("/".to_string());
    }
    remap(&path, &root, "")
}

/// Where moving `item` into `folder` puts it, or why it can't go there.
pub fn destination(item: &Path, folder: &Path, library: &Path, exists: impl Fn(&Path) -> bool) -> Result<PathBuf, Refusal> {
    let item_path = normalized(item);
    let folder_path = normalized(folder);
    let root = normalized(library);
    if text::canon_eq(&item_path, &root) || !is_same_or_inside(&item_path, &root) || !is_same_or_inside(&folder_path, &root) {
        return Err(Refusal::OutsideLibrary);
    }
    if is_same_or_inside(&folder_path, &item_path) {
        return Err(Refusal::IntoItself);
    }
    let raw = item.to_string_lossy();
    if text::canon_eq(&text::standardized_path(text::deleting_last_path_component(&raw)), &folder_path) {
        return Err(Refusal::AlreadyThere);
    }
    let name = text::last_path_component(&raw);
    let target = Path::new(&folder_path).join(name);
    if exists(&target) {
        return Err(Refusal::NameTaken(name.to_string()));
    }
    Ok(target)
}

/// `path` after the item at `old` moved to `new`: `new` itself, or the same
/// place under `new` for a path inside a moved folder; `None` when the move
/// doesn't touch it. Works on file paths and folder ids alike.
pub fn remap(path: &str, old: &str, new: &str) -> Option<String> {
    if text::canon_eq(path, old) {
        return Some(new.to_string());
    }
    let rest = text::strip_prefix_canon(path, old)?;
    text::has_prefix(rest, "/").then(|| format!("{new}{rest}"))
}

/// Whether `path` is `ancestor` or lies inside it.
pub fn is_same_or_inside(path: &str, ancestor: &str) -> bool {
    if text::canon_eq(path, ancestor) {
        return true;
    }
    if ancestor == "/" {
        return text::has_prefix(path, "/");
    }
    text::strip_prefix_canon(path, ancestor).is_some_and(|rest| text::has_prefix(rest, "/"))
}

fn normalized(path: &Path) -> String {
    text::standardized_path(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    //! Ported from `Tests/LibraryMoveTests.swift`.
    use super::*;
    use std::collections::HashSet;

    const LIBRARY: &str = "/ws";

    fn url(path: &str) -> PathBuf {
        PathBuf::from(format!("/ws{path}"))
    }

    /// The destination's path, or the refusal, as one comparable value.
    fn move_(item: &str, folder_id: &str, taken: &[&str]) -> String {
        let library = Path::new(LIBRARY);
        let folder = folder_path(folder_id, library);
        let taken: HashSet<&str> = taken.iter().copied().collect();
        match destination(&url(item), &folder, library, |p| taken.contains(p.to_str().unwrap())) {
            Ok(target) => target.to_str().unwrap().to_string(),
            Err(Refusal::AlreadyThere) => "alreadyThere".to_string(),
            Err(Refusal::IntoItself) => "intoItself".to_string(),
            Err(Refusal::OutsideLibrary) => "outsideLibrary".to_string(),
            Err(Refusal::NameTaken(name)) => format!("nameTaken(\"{name}\")"),
            Err(Refusal::Failed(message)) => format!("failed(\"{message}\")"),
        }
    }

    #[test]
    fn folder_ids_and_paths() {
        let library = Path::new(LIBRARY);
        assert_eq!(folder_path("/", library).to_str(), Some("/ws"), "root id is the library");
        assert_eq!(folder_path("/产品思考/草稿", library).to_str(), Some("/ws/产品思考/草稿"), "nested id");
        assert_eq!(folder_id(&url(""), library).as_deref(), Some("/"), "library is the root id");
        assert_eq!(folder_id(&url("/Travel/Kyoto"), library).as_deref(), Some("/Travel/Kyoto"), "nested folder id");
        assert_eq!(folder_id(Path::new("/ws/Travel/"), library).as_deref(), Some("/Travel"), "trailing slash ignored");
        assert_eq!(folder_id(Path::new("/wsx/Travel"), library), None, "sibling with a shared prefix is outside");
    }

    #[test]
    fn notes_and_files() {
        assert_eq!(move_("/Inbox.md", "/Travel", &[]), "/ws/Travel/Inbox.md", "root note into a folder");
        assert_eq!(move_("/Travel/Kyoto.md", "/", &[]), "/ws/Kyoto.md", "note back out to the root");
        assert_eq!(move_("/Travel/cover.jpg", "/Archive/2025", &[]), "/ws/Archive/2025/cover.jpg", "file keeps its extension");
        assert_eq!(move_("/Travel/Kyoto.md", "/Travel", &[]), "alreadyThere", "same folder");
        assert_eq!(move_("/Kyoto.md", "/", &[]), "alreadyThere", "root note dropped on the root");
        assert_eq!(move_("/Inbox.md", "/Travel", &["/ws/Travel/Inbox.md"]), "nameTaken(\"Inbox.md\")", "name collision");
        assert_eq!(move_("/Inbox.md", "/Travel", &["/ws/Travel/Other.md"]), "/ws/Travel/Inbox.md", "other names don't collide");
    }

    #[test]
    fn folders() {
        assert_eq!(move_("/Travel", "/Archive", &[]), "/ws/Archive/Travel", "folder into a folder");
        assert_eq!(move_("/Archive/Travel", "/", &[]), "/ws/Travel", "folder back out to the root");
        assert_eq!(move_("/Travel", "/Travel", &[]), "intoItself", "folder onto itself");
        assert_eq!(move_("/Travel", "/Travel/Japan/Kyoto", &[]), "intoItself", "folder into its own descendant");
        assert_eq!(move_("/Travel", "/Travel Notes", &[]), "/ws/Travel Notes/Travel", "shared name prefix is not a descendant");
        assert_eq!(move_("/Archive/Travel", "/Archive", &[]), "alreadyThere", "folder already in that folder");
        assert_eq!(move_("/Travel", "/Archive", &["/ws/Archive/Travel"]), "nameTaken(\"Travel\")", "folder name collision");
    }

    #[test]
    fn outside_the_library() {
        let library = Path::new(LIBRARY);
        let outside = destination(Path::new("/Users/me/Desktop/a.md"), library, library, |_| false);
        assert_eq!(outside.err(), Some(Refusal::OutsideLibrary), "item from outside the library");
        let into_outside = destination(&url("/a.md"), Path::new("/tmp"), library, |_| false);
        assert_eq!(into_outside.err(), Some(Refusal::OutsideLibrary), "folder outside the library");
        let itself = destination(library, &url("/Travel"), library, |_| false);
        assert_eq!(itself.err(), Some(Refusal::OutsideLibrary), "the library itself can't move");
    }

    #[test]
    fn following_a_move() {
        assert_eq!(remap("/ws/Travel/Kyoto.md", "/ws/Travel/Kyoto.md", "/ws/Kyoto.md").as_deref(), Some("/ws/Kyoto.md"), "the moved item");
        assert_eq!(
            remap("/ws/Travel/Japan/Kyoto.md", "/ws/Travel", "/ws/Archive/Travel").as_deref(),
            Some("/ws/Archive/Travel/Japan/Kyoto.md"),
            "an item inside a moved folder"
        );
        assert_eq!(remap("/ws/Travel Notes/a.md", "/ws/Travel", "/ws/Archive/Travel"), None, "shared name prefix is untouched");
        assert_eq!(remap("/Travel/Japan", "/Travel", "/Archive/Travel").as_deref(), Some("/Archive/Travel/Japan"), "folder ids remap too");
        assert_eq!(remap("/Other", "/Travel", "/Archive/Travel"), None, "unrelated id");
    }
}

#[cfg(test)]
mod extra_tests {
    //! Cases the Swift suite lacks. Expected values were checked against the
    //! Swift code.
    use super::*;

    #[test]
    fn cjk_and_emoji_paths() {
        assert_eq!(remap("/库/旅行😀/京都.md", "/库/旅行😀", "/库/归档/旅行😀").as_deref(), Some("/库/归档/旅行😀/京都.md"));
        assert_eq!(remap("/库/旅行笔记/a.md", "/库/旅行", "/x"), None, "a CJK name sharing a prefix");
        assert_eq!(folder_id(Path::new("/库/旅行/京都"), Path::new("/库")).as_deref(), Some("/旅行/京都"));
    }

    #[test]
    fn normalization_does_not_matter_and_the_rest_keeps_its_bytes() {
        let nfc = "/ws/caf\u{E9}";
        let nfd = "/ws/cafe\u{301}";
        assert_eq!(remap(nfd, nfc, "/x").as_deref(), Some("/x"));
        assert_eq!(remap("/ws/cafe\u{301}/e\u{301}.md", nfc, "/x").as_deref(), Some("/x/e\u{301}.md"));
        assert_eq!(remap("/ws/cafe\u{301}x/a.md", nfc, "/x"), None);
        assert!(is_same_or_inside("/ws/cafe\u{301}/a", nfc));
        assert_eq!(folder_id(Path::new(nfd), Path::new("/ws")).as_deref(), Some("/cafe\u{301}"));
    }

    #[test]
    fn paths_are_standardized() {
        let library = Path::new("/ws");
        assert_eq!(folder_id(Path::new("/ws/a/../b/./c//"), library).as_deref(), Some("/b/c"));
        assert_eq!(folder_id(Path::new("/ws/.."), library), None);
        assert_eq!(folder_id(Path::new("/ws/"), library).as_deref(), Some("/"));
        assert_eq!(destination(Path::new("/ws/a/../b.md"), library, library, |_| false).err(), Some(Refusal::AlreadyThere));
    }

    #[test]
    fn is_same_or_inside_edges() {
        assert!(is_same_or_inside("/a/b", "/"));
        assert!(is_same_or_inside("/", "/"));
        assert!(is_same_or_inside("/a", "/a"));
        assert!(!is_same_or_inside("/ab", "/a"));
    }

    /// Kept from the Swift code: `remap` looks for `//`, so a library at the
    /// volume's root has no folder ids below itself.
    #[test]
    fn library_at_the_root() {
        assert_eq!(folder_id(Path::new("/a"), Path::new("/")), None);
        assert_eq!(folder_id(Path::new("/"), Path::new("/")).as_deref(), Some("/"));
    }
}
