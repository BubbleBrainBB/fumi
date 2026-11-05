//! A window's tabs and where each has been. Port of `App/NoteTabs.swift`,
//! without `UserDefaults`: [`SavedTabs`] turns tabs into the JSON Swift's
//! `JSONEncoder` writes and back, and where that's kept is the caller's. The
//! Swift app keeps it per library folder under the key [`KEY`].

use serde::{Deserialize, Serialize};

/// The `UserDefaults` key the Swift app keeps saved tabs under.
pub const KEY: &str = "fumi.tabs";

/// What a tab shows: a note, a blank draft, the Memos page or a file preview.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "ContentWire", into = "ContentWire")]
pub enum Content {
    /// A blank page not yet on disk.
    Draft,
    /// A note, by its id (the file path).
    Note(String),
    Memos,
    /// An image, video or PDF, by its id (the file path).
    File(String),
}

impl Content {
    /// The note or file path this tab shows, if any.
    pub fn path(&self) -> Option<&str> {
        match self {
            Content::Note(id) | Content::File(id) => Some(id),
            Content::Draft | Content::Memos => None,
        }
    }
}

/// How Swift's synthesized `Codable` writes the enum:
/// `{"draft":{}}`, `{"note":{"_0":"/a.md"}}`, `{"memos":{}}`, `{"file":{"_0":"/a.png"}}`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ContentWire {
    Draft {},
    Note {
        #[serde(rename = "_0")]
        id: String,
    },
    Memos {},
    File {
        #[serde(rename = "_0")]
        id: String,
    },
}

impl From<ContentWire> for Content {
    fn from(wire: ContentWire) -> Self {
        match wire {
            ContentWire::Draft {} => Content::Draft,
            ContentWire::Note { id } => Content::Note(id),
            ContentWire::Memos {} => Content::Memos,
            ContentWire::File { id } => Content::File(id),
        }
    }
}

impl From<Content> for ContentWire {
    fn from(content: Content) -> Self {
        match content {
            Content::Draft => ContentWire::Draft {},
            Content::Note(id) => ContentWire::Note { id },
            Content::Memos => ContentWire::Memos {},
            Content::File(id) => ContentWire::File { id },
        }
    }
}

/// One open page in a window. A window holds one or more.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteTab {
    /// A UUID string; the caller makes it, as this crate has no random source.
    pub id: String,
    pub content: Content,
    /// Pinned tabs show only their icon and stay at the leading end.
    #[serde(rename = "isPinned")]
    pub is_pinned: bool,
}

impl NoteTab {
    pub fn new(id: impl Into<String>, content: Content) -> Self {
        Self { id: id.into(), content, is_pinned: false }
    }
}

/// The tabs of a library, kept between launches: one set per library folder, so
/// reopening a folder brings its tabs back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedTabs {
    pub tabs: Vec<NoteTab>,
    pub active: i64,
}

impl SavedTabs {
    /// The saved tabs, with `active` inside them; `None` when `data` can't be
    /// read or holds no tabs.
    pub fn decode(data: &[u8]) -> Option<Self> {
        let saved: Self = serde_json::from_slice(data).ok()?;
        if saved.tabs.is_empty() {
            return None;
        }
        let active = saved.active.max(0).min(saved.tabs.len() as i64 - 1);
        Some(Self { active, ..saved })
    }

    /// The JSON to keep.
    pub fn encode(tabs: &[NoteTab], active: i64) -> Option<Vec<u8>> {
        serde_json::to_vec(&Self { tabs: tabs.to_vec(), active }).ok()
    }
}

/// Where a tab has been, as in a browser: each tab keeps its own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabHistory {
    pub back: Vec<Content>,
    pub forward: Vec<Content>,
}

impl TabHistory {
    pub const LIMIT: usize = 50;

    /// A new page replaces whatever was ahead. A blank draft isn't a place to
    /// come back to.
    pub fn visit(&mut self, leaving: &Content) {
        self.forward.clear();
        if *leaving == Content::Draft {
            return;
        }
        self.back.retain(|content| content != leaving);
        self.back.push(leaving.clone());
        if self.back.len() > Self::LIMIT {
            self.back.remove(0);
        }
    }

    /// The page before `current`, which goes ahead of it; `None` if none.
    pub fn back_from(&mut self, current: &Content) -> Option<Content> {
        let previous = self.back.pop()?;
        if *current != Content::Draft {
            self.forward.push(current.clone());
        }
        Some(previous)
    }

    /// The page Back left, `current` going behind it; `None` if none.
    pub fn forward_from(&mut self, current: &Content) -> Option<Content> {
        let next = self.forward.pop()?;
        if *current != Content::Draft {
            self.back.push(current.clone());
        }
        Some(next)
    }

    pub fn remap(&mut self, transform: impl Fn(Content) -> Content) {
        for list in [&mut self.back, &mut self.forward] {
            *list = std::mem::take(list).into_iter().map(&transform).collect();
        }
    }

    pub fn remove(&mut self, content: &Content) {
        self.back.retain(|item| item != content);
        self.forward.retain(|item| item != content);
    }
}

#[cfg(test)]
mod tests {
    // Ported from Tests/TabHistoryTests.swift.
    use super::*;

    fn note(path: &str) -> Content {
        Content::Note(path.to_string())
    }

    #[test]
    fn tab_history() {
        let (a, b, c) = (note("/a.md"), note("/b.md"), note("/c.md"));

        // ── Visiting ──
        let mut history = TabHistory::default();
        history.visit(&Content::Draft);
        assert_eq!(history.back, [], "a blank draft isn't kept");
        history.visit(&a);
        history.visit(&Content::Memos);
        assert_eq!(history.back, [a.clone(), Content::Memos], "pages left, oldest first");

        // ── Back and forward ──
        assert_eq!(history.back_from(&b), Some(Content::Memos), "back to Memos");
        assert_eq!(history.back_from(&Content::Memos), Some(a.clone()), "back again");
        assert_eq!(history.back_from(&a), None, "nothing before the first page");
        assert_eq!(history.forward, [b.clone(), Content::Memos], "both ahead");
        assert_eq!(history.forward_from(&a), Some(Content::Memos), "forward");
        assert_eq!(history.forward_from(&Content::Memos), Some(b.clone()), "forward again");
        assert_eq!(history.forward_from(&b), None, "nothing ahead");

        let _ = history.back_from(&b);
        history.visit(&Content::Memos);
        assert_eq!(history.forward, [], "a new page clears what was ahead");

        // ── No repeats ──
        let mut ping_pong = TabHistory::default();
        for _ in 0..5 {
            ping_pong.visit(&a);
            ping_pong.visit(&b);
        }
        assert_eq!(ping_pong.back, [a.clone(), b.clone()], "going back and forth keeps each page once");

        let mut long = TabHistory::default();
        for i in 0..80 {
            long.visit(&note(&format!("/{i}.md")));
        }
        assert_eq!(long.back.len(), TabHistory::LIMIT, "capped");
        assert_eq!(long.back.first(), Some(&note("/30.md")), "oldest dropped");

        // ── Renames and deletes ──
        let mut moved = TabHistory { back: vec![a.clone(), c.clone()], forward: vec![b.clone()] };
        moved.remap(|content| if content == note("/a.md") { note("/x/a.md") } else { content });
        assert_eq!(moved.back, [note("/x/a.md"), c.clone()], "renamed pages follow");
        moved.remove(&b);
        assert_eq!(moved.forward, [], "deleted pages drop out");

        let mut from_draft = TabHistory { back: vec![a.clone()], forward: vec![] };
        assert_eq!(from_draft.back_from(&Content::Draft), Some(a.clone()), "back from a blank draft");
        assert_eq!(from_draft.forward, [], "the draft isn't kept ahead");
    }

    // ── Beyond the Swift test ──

    /// Captured from Swift's `JSONEncoder` for `SavedTabs.Entry`.
    const SWIFT_JSON: &str = r#"{"tabs":[{"id":"314E0413-271D-455D-AEBD-49C3D1296813","content":{"draft":{}},"isPinned":false},{"id":"B273CF03-4096-4F87-83B0-CDFEB8A8290E","content":{"note":{"_0":"\/a\/b.md"}},"isPinned":false},{"id":"9A626073-5638-4139-BD6C-0F2D6D37FF9D","content":{"memos":{}},"isPinned":false},{"id":"A950B9DD-90E1-4581-A710-1A660DC8CC0C","content":{"file":{"_0":"\/c.png"}},"isPinned":true}],"active":1}"#;

    #[test]
    fn decodes_what_swift_wrote() {
        let saved = SavedTabs::decode(SWIFT_JSON.as_bytes()).unwrap();
        assert_eq!(saved.active, 1);
        let contents: Vec<_> = saved.tabs.iter().map(|tab| tab.content.clone()).collect();
        assert_eq!(contents, [Content::Draft, note("/a/b.md"), Content::Memos, Content::File("/c.png".into())]);
        assert_eq!(saved.tabs[0].id, "314E0413-271D-455D-AEBD-49C3D1296813");
        assert!(saved.tabs[3].is_pinned && !saved.tabs[0].is_pinned);
    }

    #[test]
    fn writes_the_same_shape() {
        let saved = SavedTabs::decode(SWIFT_JSON.as_bytes()).unwrap();
        let data = SavedTabs::encode(&saved.tabs, saved.active).unwrap();
        let unescaped = SWIFT_JSON.replace("\\/", "/");
        assert_eq!(String::from_utf8(data).unwrap(), unescaped);
    }

    #[test]
    fn active_stays_inside_the_tabs() {
        let tabs = [NoteTab::new("id", Content::Memos)];
        for active in [-3, 0, 9] {
            let data = SavedTabs::encode(&tabs, active).unwrap();
            assert_eq!(SavedTabs::decode(&data).unwrap().active, 0);
        }
    }

    #[test]
    fn nothing_to_restore() {
        assert_eq!(SavedTabs::decode(br#"{"tabs":[],"active":0}"#), None, "no tabs");
        assert_eq!(SavedTabs::decode(b"junk"), None, "unreadable");
    }

    #[test]
    fn paths_of_contents() {
        assert_eq!(note("/a.md").path(), Some("/a.md"));
        assert_eq!(Content::File("/a.png".into()).path(), Some("/a.png"));
        assert_eq!(Content::Draft.path(), None);
        assert_eq!(Content::Memos.path(), None);
    }
}
