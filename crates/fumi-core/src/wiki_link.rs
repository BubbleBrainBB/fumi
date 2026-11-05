//! Obsidian links between notes, `[[target#heading|alias]]`: shown as the
//! linked note's title, opened by a click. Port of `Services/WikiLink.swift`,
//! and of `FileNote.keepingUnchanged`, which `Tests/WikiLinkTests.swift` tests.
//!
//! The index is plain data: callers watch [`WikiLinkIndex::revision`] where
//! the Swift code posted a notification.

use crate::frontmatter::text;
use crate::note::Note;
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// `[[…]]`; [`find_all`] leaves out the embeds, `![[…]]`.
static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\[\]\n]+)\]\]").expect("wiki link pattern"));

/// A parsed link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    /// The note's name or library path, without `.md`.
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
}

/// Where a link sits in a text, in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLinkMatch {
    /// The whole `[[…]]`.
    pub range: Range<usize>,
    /// What is between the brackets.
    pub inner: Range<usize>,
}

/// Every `[[…]]` in `text` that isn't an embed.
pub fn find_all(text: &str) -> Vec<WikiLinkMatch> {
    PATTERN
        .captures_iter(text)
        .filter_map(|c| {
            let whole = c.get(0)?;
            if text.as_bytes()[..whole.start()].last() == Some(&b'!') {
                return None;
            }
            Some(WikiLinkMatch { range: whole.range(), inner: c.get(1)?.range() })
        })
        .collect()
}

impl WikiLink {
    /// Parses what sits between `[[` and `]]`; `None` when it names nothing.
    pub fn parse(inner: &str) -> Option<WikiLink> {
        let mut rest = inner;
        let mut alias = None;
        if let Some(bar) = text::find_char(rest, "|") {
            alias = non_empty(text::trim_ws(&rest[bar + 1..]));
            rest = &rest[..bar];
        }
        let mut heading = None;
        if let Some(hash) = text::find_char(rest, "#") {
            heading = non_empty(text::trim_ws(&rest[hash + 1..]));
            rest = &rest[..hash];
        }
        let mut target = text::trim_ws(rest);
        if text::has_suffix(&text::lowercased(target), ".md") {
            target = text::drop_last(target, 3);
        }
        if target.is_empty() {
            return None;
        }
        Some(WikiLink { target: target.to_string(), heading, alias })
    }

    /// What the link shows: its alias, else the note's title, with the heading
    /// after it (`Title › Heading`). A missing note shows its target's name.
    pub fn display_text(&self, title: Option<&str>) -> String {
        if let Some(alias) = &self.alias {
            return alias.clone();
        }
        let name = title.unwrap_or_else(|| text::last_path_component(&self.target));
        match &self.heading {
            Some(heading) => format!("{name} › {heading}"),
            None => name.to_string(),
        }
    }
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// A note the index knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub path: PathBuf,
    pub title: String,
}

/// The library's notes by the names links use, with their titles. Names match
/// across case and Unicode normalization.
#[derive(Debug, Default)]
pub struct WikiLinkIndex {
    /// Note file names and library paths (folded, without `.md`) to notes.
    by_name: HashMap<String, Vec<Entry>>,
    by_path: HashMap<String, Entry>,
    revision: u64,
}

impl WikiLinkIndex {
    pub fn new() -> WikiLinkIndex {
        WikiLinkIndex::default()
    }

    /// Bumped whenever a title, name or note changes, so styled links redraw.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Rebuilds from the library's notes, `(id, path, content)` each, with the
    /// library's folder to read paths against.
    pub fn update(&mut self, notes: &[(&str, &Path, &str)], library: Option<&Path>) {
        let mut names: HashMap<String, Vec<Entry>> = HashMap::new();
        let mut paths: HashMap<String, Entry> = HashMap::new();
        let root = library.map(|path| text::standardized_path(&path.to_string_lossy()));
        for (id, path, content) in notes {
            let raw = path.to_string_lossy();
            let file = deleting_path_extension(&raw);
            let file_name = text::last_path_component(file);
            let entry = Entry { id: id.to_string(), path: path.to_path_buf(), title: WikiLinkIndex::title(content, file_name) };
            names.entry(name_key(file_name)).or_default().push(entry.clone());
            let standardized = text::standardized_path(file);
            if let Some(root) = &root
                && let Some(rest) = text::strip_prefix_canon(&standardized, root)
                && text::has_prefix(rest, "/")
            {
                paths.insert(name_key(&rest[1..]), entry);
            }
        }
        if names != self.by_name || paths != self.by_path {
            self.by_name = names;
            self.by_path = paths;
            self.revision += 1;
        }
    }

    /// Whether links to `old` show the same once it's `new`: the same note in
    /// the same place, with the same title.
    pub fn shows_same(old: &Note, new: &Note) -> bool {
        if old.id != new.id || old.path != new.path {
            return false;
        }
        let file_name = new.filename();
        text::canon_eq(&old.content, &new.content)
            || text::canon_eq(&WikiLinkIndex::title(&old.content, &file_name), &WikiLinkIndex::title(&new.content, &file_name))
    }

    /// The note `target` names: by library path when it has one, else by file
    /// name: one in `folder` (the linking note's) first, then the one nearest
    /// the library's top, as Obsidian picks.
    pub fn resolve(&self, target: &str, folder: Option<&Path>) -> Option<&Entry> {
        let key = text::lowercased(target);
        if text::contains(&key, "/")
            && let Some(entry) = self.by_path.get(&text::canonical(&key))
        {
            return Some(entry);
        }
        let candidates = self.by_name.get(&text::canonical(text::last_path_component(&key)))?;
        if let [only] = candidates.as_slice() {
            return Some(only);
        }
        if let Some(folder) = folder {
            let folder = text::standardized_path(&folder.to_string_lossy());
            let near = candidates.iter().find(|entry| {
                let path = entry.path.to_string_lossy();
                text::canon_eq(&text::standardized_path(text::deleting_last_path_component(&path)), &folder)
            });
            if near.is_some() {
                return near;
            }
        }
        candidates.iter().min_by_key(|entry| path_component_count(&entry.path))
    }

    /// A note's title as links show it: the frontmatter's `title`, else a `# `
    /// heading on its first line, else its file name. Only the head of the
    /// note is read.
    pub fn title(content: &str, file_name: &str) -> String {
        let lines: Vec<&str> = text::lf_lines_limited(content, 300).collect();
        let mut rest = &lines[..];
        if rest.first().is_some_and(|line| text::trim_ws(line) == "---") {
            rest = &rest[1..];
            while let Some((line, tail)) = rest.split_first() {
                rest = tail;
                let trimmed = text::trim_ws(line);
                if trimmed == "---" || trimmed == "..." {
                    break;
                }
                if text::has_prefix(trimmed, "title:") {
                    let value = text::unquote(text::trim_ws(&trimmed[6..]));
                    if !value.is_empty() {
                        return value.to_string();
                    }
                }
            }
        }
        if let Some(first) = rest.iter().find(|line| !text::trim_ws(line).is_empty()) {
            let trimmed = text::trim_ws(first);
            if text::has_prefix(trimmed, "# ") {
                let heading = text::trim_ws(&trimmed[2..]);
                if !heading.is_empty() {
                    return heading.to_string();
                }
            }
        }
        file_name.to_string()
    }
}

/// `scanned`, a fresh read of the library, holding on to the `known` notes it
/// found unchanged. Port of `FileNote.keepingUnchanged`. The Swift code does
/// this to keep one copy of each text, which makes comparing them fast; here
/// the result always equals `scanned`, byte for byte.
pub fn keeping_unchanged(known: &[Note], scanned: Vec<Note>) -> Vec<Note> {
    let mut by_id: HashMap<&str, &Note> = HashMap::new();
    for note in known {
        by_id.entry(note.id.as_str()).or_insert(note);
    }
    scanned
        .into_iter()
        .map(|note| match by_id.get(note.id.as_str()) {
            Some(old) if **old == note => (*old).clone(),
            _ => note,
        })
        .collect()
}

/// The key of a name or path: `lowercased()`, compared as Swift compares
/// dictionary keys, by canonical equivalence.
fn name_key(name: &str) -> String {
    text::canonical(&text::lowercased(name))
}

/// `URL.deletingPathExtension().path`: a name with nothing before its last dot
/// but dots (`.md`, `..md`), or nothing after it (`a.`), keeps it.
fn deleting_path_extension(path: &str) -> &str {
    let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
    let name_start = path.rfind('/').map_or(0, |slash| slash + 1);
    let name = &path[name_start..];
    match name.rfind('.') {
        Some(dot) if dot + 1 < name.len() && !name[..dot].chars().all(|c| c == '.') => &path[..name_start + dot],
        _ => path,
    }
}

/// `URL.pathComponents.count` for an absolute path.
fn path_component_count(path: &Path) -> usize {
    1 + path.to_string_lossy().split('/').filter(|part| !part.is_empty()).count()
}

#[cfg(test)]
mod tests {
    //! Ported from `Tests/WikiLinkTests.swift`.
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static CLOCK: AtomicU64 = AtomicU64::new(1);

    /// A new note, each with a later date, as the Swift test's `Date()` gives.
    fn note(path: &str, content: &str) -> Note {
        let now = CLOCK.fetch_add(1, Ordering::Relaxed) as f64;
        Note {
            id: path.to_string(),
            path: PathBuf::from(path),
            title: String::new(),
            content: content.to_string(),
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
            pinned: false,
            relative_path: "/".to_string(),
        }
    }

    #[test]
    fn parsing() {
        let plain = WikiLink::parse("智谱 GLM-4.5v").unwrap();
        assert_eq!(plain.target, "智谱 GLM-4.5v", "plain target");
        assert_eq!(plain.heading, None, "plain target");
        assert_eq!(plain.alias, None, "plain target");
        let full = WikiLink::parse("folder/Note.md#效果有多好？|上次那篇").unwrap();
        assert_eq!(full.target, "folder/Note", "path, .md, heading and alias");
        assert_eq!(full.heading.as_deref(), Some("效果有多好？"), "path, .md, heading and alias");
        assert_eq!(full.alias.as_deref(), Some("上次那篇"), "path, .md, heading and alias");
        assert_eq!(WikiLink::parse("  "), None, "nothing to link to");
        assert_eq!(WikiLink::parse("#Heading"), None, "nothing to link to");
    }

    #[test]
    fn what_a_link_shows() {
        let plain = WikiLink::parse("智谱 GLM-4.5v").unwrap();
        let full = WikiLink::parse("folder/Note.md#效果有多好？|上次那篇").unwrap();
        assert_eq!(plain.display_text(Some("GLM 那篇")), "GLM 那篇", "the note's title");
        assert_eq!(full.display_text(Some("Note")), "上次那篇", "the alias wins");
        assert_eq!(WikiLink::parse("Note#Part").unwrap().display_text(Some("A Note")), "A Note › Part", "title › heading");
        assert_eq!(WikiLink::parse("a/b/Missing").unwrap().display_text(None), "Missing", "a missing note shows its name");
    }

    #[test]
    fn titles() {
        assert_eq!(WikiLinkIndex::title("---\ntitle: \"2025年7月4日\"\ndate: x\n---\n# Other\n", "f"), "2025年7月4日", "front matter title first");
        assert_eq!(WikiLinkIndex::title("---\ntags: [a]\n---\n\n# 标题\n正文", "f"), "标题", "then a first-line heading");
        assert_eq!(WikiLinkIndex::title("正文第一行\n\n# 中间的标题\n", "文件名"), "文件名", "a heading further down isn't the title");
        assert_eq!(WikiLinkIndex::title("", "文件名"), "文件名", "else the file name");
    }

    #[test]
    fn resolving() {
        let root = Path::new("/lib");
        let mut index = WikiLinkIndex::new();
        index.update(
            &[
                ("/lib/a/Note.md", Path::new("/lib/a/Note.md"), "# In A"),
                ("/lib/b/c/Note.md", Path::new("/lib/b/c/Note.md"), "# In C"),
                ("/lib/Top.md", Path::new("/lib/Top.md"), "text"),
            ],
            Some(root),
        );
        let title = |target: &str, folder: Option<&str>| index.resolve(target, folder.map(Path::new)).map(|e| e.title.clone());
        assert_eq!(title("top", None).as_deref(), Some("Top"), "by name, any case");
        assert_eq!(title("Note", Some("/lib/b/c")).as_deref(), Some("In C"), "same folder first");
        assert_eq!(title("Note", Some("/lib/x")).as_deref(), Some("In A"), "else nearest the top");
        assert_eq!(title("b/c/Note", None).as_deref(), Some("In C"), "by library path");
        assert_eq!(title("Gone", None), None, "missing");

        let revision = index.revision();
        let top: &[(&str, &Path, &str)] = &[("/lib/Top.md", Path::new("/lib/Top.md"), "text")];
        index.update(top, Some(root));
        index.update(top, Some(root));
        assert_eq!(index.revision(), revision + 1, "an unchanged library isn't a change");
    }

    #[test]
    fn what_a_save_changes_for_links() {
        let titled = note("/lib/A.md", "# Title\nbody");
        assert!(WikiLinkIndex::shows_same(&titled, &note("/lib/A.md", "# Title\nbody and more")), "typing in the body");
        assert!(!WikiLinkIndex::shows_same(&titled, &note("/lib/A.md", "# New title\nbody")), "a new heading");
        assert!(!WikiLinkIndex::shows_same(&titled, &note("/lib/A.md", "---\ntitle: T\n---\n# Title\nbody")), "a front matter title");
        assert!(!WikiLinkIndex::shows_same(&titled, &note("/lib/A.md", "body")), "no title left: the file name shows");
        assert!(!WikiLinkIndex::shows_same(&titled, &note("/lib/B.md", "# Title\nbody")), "moved or renamed");
    }

    #[test]
    fn keeping_what_a_rescan_found_unchanged() {
        let saved = note("/lib/A.md", "# 标题\n正文");
        let other = note("/lib/B.md", "正文");
        let mut rescanned_a = note("/lib/A.md", &format!("# 标题\n{}", "正文"));
        rescanned_a.created_at = saved.created_at;
        rescanned_a.updated_at = saved.updated_at;
        rescanned_a.title = saved.title.clone();
        let edited = note("/lib/B.md", "外部改过");
        let added = note("/lib/C.md", "新的");
        let merged = keeping_unchanged(&[saved.clone(), other], vec![added.clone(), edited.clone(), rescanned_a]);
        assert_eq!(merged.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), [added.id.as_str(), edited.id.as_str(), saved.id.as_str()], "in the scan's order, without what's gone");
        assert_eq!(merged[0], added, "what's new or changed comes from the scan");
        assert_eq!(merged[1], edited, "what's new or changed comes from the scan");
        assert_eq!(merged[2], saved, "what's unchanged stays");
        assert_eq!(keeping_unchanged(&[saved.clone()], vec![saved.clone()]), [saved.clone()], "nothing changed, nothing to do");
        let touched = note("/lib/A.md", &saved.content);
        assert_eq!(keeping_unchanged(&[saved.clone()], vec![touched.clone()])[0].updated_at, touched.updated_at, "a new date still lands");
        assert_ne!(touched.updated_at, saved.updated_at);
        let nfd = note("/lib/D.md", "e\u{301}");
        assert_eq!(keeping_unchanged(&[note("/lib/D.md", "\u{e9}")], vec![nfd])[0].content.len(), 3, "the same text in another normalization is a change on disk");
    }
}

#[cfg(test)]
mod extra_tests {
    //! Cases the Swift suite lacks. Expected values were checked against the
    //! Swift code unless a comment says the port differs on purpose.
    use super::*;

    fn inners(text: &str) -> Vec<&str> {
        find_all(text).into_iter().map(|m| &text[m.inner]).collect()
    }

    #[test]
    fn finding_links() {
        assert_eq!(inners("见 [[笔记😀]] 和 [[甲|乙]]"), ["笔记😀", "甲|乙"]);
        assert_eq!(inners("![[图.png]] [[文]]"), ["文"], "an embed is not a link");
        assert_eq!(inners("![[[文]]"), ["文"], "the `!` must be right before");
        assert_eq!(inners("[[a\nb]] [[]] [[a]b]]"), Vec::<&str>::new());
        assert_eq!(inners("[[甲]]\r\n[[乙]]\r\n"), ["甲", "乙"]);
        assert_eq!(inners("[[a\r\nb]]"), Vec::<&str>::new());
        assert_eq!(inners("[[a\rb]]"), ["a\rb"], "only `\\n` stops it");
        let text = "中文😀[[甲]]";
        let found = find_all(text);
        assert_eq!(found[0].range, "中文😀".len()..text.len());
        assert_eq!(&text[found[0].inner.clone()], "甲");
    }

    #[test]
    fn parsing_edges() {
        let link = WikiLink::parse("\u{3000}文件夹/笔记.MD # 标题 😀 | 别名 🎉 ").unwrap();
        assert_eq!(link.target, "文件夹/笔记");
        assert_eq!(link.heading.as_deref(), Some("标题 😀"));
        assert_eq!(link.alias.as_deref(), Some("别名 🎉"));
        assert_eq!(WikiLink::parse("a|").unwrap().alias, None);
        assert_eq!(WikiLink::parse("a#").unwrap().heading, None);
        assert_eq!(WikiLink::parse("a#b#c").unwrap().heading.as_deref(), Some("b#c"));
        assert_eq!(WikiLink::parse("a|b|c").unwrap().alias.as_deref(), Some("b|c"));
        assert_eq!(WikiLink::parse("a#b|c#d").unwrap().heading.as_deref(), Some("b"));
        assert_eq!(WikiLink::parse(".md"), None);
        assert_eq!(WikiLink::parse("a .md").unwrap().target, "a ", "not trimmed again");
        assert_eq!(WikiLink::parse("a.md\u{301}").unwrap().target, "a.md\u{301}", "a marked d is no `.md`");
        let keycap = "#\u{FE0F}\u{20E3}";
        assert_eq!(WikiLink::parse(&format!("a{keycap}b")).unwrap().target, format!("a{keycap}b"), "a keycap has no #");
        assert_eq!(WikiLink::parse("a/b/").unwrap().display_text(None), "b");
        assert_eq!(WikiLink::parse("a/b#H").unwrap().display_text(None), "b › H");
    }

    /// The Swift code differs here on purpose: its `split(separator: "\n")`
    /// doesn't split at `\r\n`, so a CRLF note's title was the whole note
    /// after `# `, or the file name when its frontmatter wasn't seen.
    #[test]
    fn crlf_titles() {
        assert_eq!(WikiLinkIndex::title("# 标题\r\n正文\r\n", "f"), "标题");
        assert_eq!(WikiLinkIndex::title("---\r\ntitle: \"甲\"\r\n---\r\n", "f"), "甲");
        assert_eq!(WikiLinkIndex::title("---\r\ntags: [a]\r\n---\r\n\r\n# 乙\r\n", "f"), "乙");
    }

    #[test]
    fn titles_at_file_boundaries() {
        assert_eq!(WikiLinkIndex::title("---\ntitle: x", "f"), "x", "no closing fence still gives its title");
        assert_eq!(WikiLinkIndex::title("---\ntags: a\n# T\n", "f"), "f", "no closing fence: everything is frontmatter");
        assert_eq!(WikiLinkIndex::title("---\ntitle:\n---\n# T", "f"), "T");
        assert_eq!(WikiLinkIndex::title("---\ntitle: \"\"\n---\n# T", "f"), "T");
        assert_eq!(WikiLinkIndex::title("\u{FEFF}---\ntitle: x\n---\n", "f"), "f", "a BOM hides the block");
        assert_eq!(WikiLinkIndex::title("\u{FEFF}# T", "f"), "f", "and the heading");
        assert_eq!(WikiLinkIndex::title("# ", "f"), "f");
        assert_eq!(WikiLinkIndex::title("#\u{FE0F}\u{20E3} T", "f"), "f", "a keycap is no heading marker");
        assert_eq!(WikiLinkIndex::title("\n\n  \n# **粗** 😀", "f"), "**粗** 😀", "the heading isn't stripped of markers");
    }

    /// Kept from the Swift code: only 300 `\n`s are cut, so the 301st piece
    /// keeps the rest of the note.
    #[test]
    fn title_reads_the_first_300_lines() {
        let far = format!("{}# 远\n还有", "\n".repeat(299));
        assert_eq!(WikiLinkIndex::title(&far, "f"), "远");
        let farther = format!("{}# 远\n还有", "\n".repeat(300));
        assert_eq!(WikiLinkIndex::title(&farther, "f"), "远\n还有");
    }

    #[test]
    fn resolve_across_case_and_normalization() {
        let mut index = WikiLinkIndex::new();
        let nfd = "/lib/が/cafe\u{301}.md";
        let decomposed = "/lib/か\u{3099}/cafe\u{301}.md";
        index.update(
            &[
                (decomposed, Path::new(decomposed), "# 咖啡"),
                ("/lib/笔记/ΣΑΣ.md", Path::new("/lib/笔记/ΣΑΣ.md"), "# 希腊"),
                ("/lib/😀.MD", Path::new("/lib/😀.MD"), "表情"),
            ],
            Some(Path::new("/lib")),
        );
        let title = |target: &str| index.resolve(target, None).map(|e| e.title.as_str());
        assert_eq!(title("CAF\u{C9}"), Some("咖啡"), "NFC finds NFD");
        assert_eq!(title("が/caf\u{E9}"), Some("咖啡"), "by library path too");
        assert_eq!(title("σασ"), Some("希腊"), "Σ lowers to σ, final or not");
        assert_eq!(title("😀"), Some("😀"), "the file name is the title, `.MD` dropped");
        assert_eq!(title("no/such/😀"), Some("😀"), "a path that misses falls back to the name");
        assert_eq!(nfd.len(), "/lib/が/cafe\u{301}.md".len());
    }

    #[test]
    fn resolve_ties_and_folders() {
        let mut index = WikiLinkIndex::new();
        index.update(
            &[
                ("1", Path::new("/lib/x/N.md"), "# 一"),
                ("2", Path::new("/lib/y/N.md"), "# 二"),
                ("3", Path::new("/lib/n.md"), "# 三"),
            ],
            Some(Path::new("/lib/")),
        );
        assert_eq!(index.resolve("N", None).unwrap().id, "3", "nearest the top");
        assert_eq!(index.resolve("N", Some(Path::new("/lib/y/"))).unwrap().id, "2", "folder standardized");
        let mut tie = WikiLinkIndex::new();
        tie.update(&[("1", Path::new("/lib/x/N.md"), ""), ("2", Path::new("/lib/y/N.md"), "")], None);
        assert_eq!(tie.resolve("N", None).unwrap().id, "1", "the first of equals");
        assert_eq!(tie.resolve("x/N", None).unwrap().id, "1", "no library, no paths: by name");
    }

    #[test]
    fn revision_counts_changes() {
        let mut index = WikiLinkIndex::new();
        assert_eq!(index.revision(), 0);
        index.update(&[], None);
        assert_eq!(index.revision(), 0, "still empty");
        index.update(&[("a", Path::new("/a.md"), "x")], None);
        assert_eq!(index.revision(), 1);
        index.update(&[("a", Path::new("/a.md"), "# 新")], None);
        assert_eq!(index.revision(), 2, "a new title");
    }

    #[test]
    fn path_extensions() {
        assert_eq!(deleting_path_extension("/lib/a.md"), "/lib/a");
        assert_eq!(deleting_path_extension("/lib/a.b.md"), "/lib/a.b");
        assert_eq!(deleting_path_extension("/lib/.md"), "/lib/.md");
        assert_eq!(deleting_path_extension("/lib/..md"), "/lib/..md");
        assert_eq!(deleting_path_extension("/lib/a."), "/lib/a.");
        assert_eq!(deleting_path_extension("/lib/a.b c"), "/lib/a");
        assert_eq!(deleting_path_extension("/lib/a"), "/lib/a");
        assert_eq!(deleting_path_extension("/l.b/a"), "/l.b/a");
        assert_eq!(deleting_path_extension("/lib/中文.MD"), "/lib/中文");
        assert_eq!(deleting_path_extension("/lib/a.md/"), "/lib/a");
        assert_eq!(deleting_path_extension("/"), "/");
    }
}
