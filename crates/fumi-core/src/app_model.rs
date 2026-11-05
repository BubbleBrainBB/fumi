//! A window's navigation state: its tabs, what each shows, Back and Forward, and
//! the overlays over the page. A port of `App/AppModel.swift`'s logic. Whoever
//! holds it redraws after calling; nothing here touches a window.

use crate::library_move;
use crate::tabs::{Content, NoteTab, SavedTabs, TabHistory};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    UpdatedDesc,
    CreatedDesc,
    TitleAsc,
}

impl SortMode {
    pub fn raw(self) -> &'static str {
        match self {
            Self::UpdatedDesc => "updated-desc",
            Self::CreatedDesc => "created-desc",
            Self::TitleAsc => "title-asc",
        }
    }

    pub fn from_raw(raw: &str) -> Self {
        match raw {
            "created-desc" => Self::CreatedDesc,
            "title-asc" => Self::TitleAsc,
            _ => Self::UpdatedDesc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::UpdatedDesc => "Date Modified",
            Self::CreatedDesc => "Date Created",
            Self::TitleAsc => "Title",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishTarget {
    Wechat,
    Xiaohongshu,
}

pub struct AppModel {
    /// The window's open pages, pinned first. Never empty: the last one closed
    /// leaves a blank draft.
    tabs: Vec<NoteTab>,
    active: String,
    closed: Vec<NoteTab>,
    histories: HashMap<String, TabHistory>,
    preview_tabs: HashSet<String>,
    tabs_workspace: Option<String>,
    new_id: Box<dyn FnMut() -> String>,

    pub search_presented: bool,
    pub show_settings: bool,
    pub sort_mode: SortMode,
    pub sidebar_visible: bool,
    /// The sidebar's first level, every library, instead of this one's files.
    pub shows_libraries: bool,
    /// False while a library opened from the welcome page hands over to the
    /// window: one plain sheet that settles into its frame.
    pub frame_revealed: bool,
}

impl AppModel {
    pub fn new(mut new_id: impl FnMut() -> String + 'static) -> Self {
        let first = NoteTab::new(new_id(), Content::Draft);
        let active = first.id.clone();
        AppModel {
            tabs: vec![first],
            active,
            closed: Vec::new(),
            histories: HashMap::new(),
            preview_tabs: HashSet::new(),
            tabs_workspace: None,
            new_id: Box::new(new_id),
            search_presented: false,
            show_settings: false,
            sort_mode: SortMode::UpdatedDesc,
            sidebar_visible: true,
            shows_libraries: false,
            frame_revealed: true,
        }
    }

    fn make_tab(&mut self, content: Content) -> NoteTab {
        NoteTab::new((self.new_id)(), content)
    }

    pub fn tabs(&self) -> &[NoteTab] {
        &self.tabs
    }

    pub fn active_id(&self) -> &str {
        &self.active
    }

    pub fn active_index(&self) -> usize {
        self.tabs.iter().position(|t| t.id == self.active).unwrap_or(0)
    }

    pub fn active_tab(&self) -> &NoteTab {
        &self.tabs[self.active_index()]
    }

    pub fn active_content(&self) -> &Content {
        &self.active_tab().content
    }

    pub fn shows_tab_strip(&self) -> bool {
        self.tabs.len() > 1
    }

    /// Changing what the active tab shows is a step in its history.
    fn set_active_content(&mut self, new: Content) {
        let i = self.active_index();
        let old = self.tabs[i].content.clone();
        if new == old {
            return;
        }
        self.histories.entry(self.active.clone()).or_default().visit(&old);
        self.tabs[i].content = new;
    }

    pub fn can_go_back(&self) -> bool {
        self.histories.get(&self.active).is_some_and(|h| !h.back.is_empty())
    }

    pub fn can_go_forward(&self) -> bool {
        self.histories.get(&self.active).is_some_and(|h| !h.forward.is_empty())
    }

    pub fn go_back(&mut self) {
        self.step(|h, c| h.back_from(c));
    }

    pub fn go_forward(&mut self) {
        self.step(|h, c| h.forward_from(c));
    }

    fn step(&mut self, mv: impl Fn(&mut TabHistory, &Content) -> Option<Content>) {
        let current = self.active_content().clone();
        let Some(history) = self.histories.get_mut(&self.active) else { return };
        let Some(page) = mv(history, &current) else { return };
        self.dismiss_overlays();
        let i = self.active_index();
        self.tabs[i].content = page;
    }

    pub fn is_preview_open(&self) -> bool {
        self.preview_tabs.contains(&self.active)
    }

    pub fn toggle_preview(&mut self) {
        if !self.preview_tabs.remove(&self.active) {
            self.preview_tabs.insert(self.active.clone());
        }
    }

    pub fn selected_note(&self) -> Option<&str> {
        match self.active_content() {
            Content::Note(id) => Some(id),
            _ => None,
        }
    }

    pub fn selected_file(&self) -> Option<&str> {
        match self.active_content() {
            Content::File(id) => Some(id),
            _ => None,
        }
    }

    pub fn shows_memos(&self) -> bool {
        *self.active_content() == Content::Memos
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }

    /// Opens a note: in the tab that already shows it, else in a new tab. Only a
    /// blank draft is filled in place, unless `in_new_tab` keeps it. `None` turns
    /// the active tab into a blank draft.
    pub fn open_note(&mut self, id: Option<&str>, in_new_tab: bool) {
        match id {
            None => {
                self.dismiss_overlays();
                self.set_active_content(Content::Draft);
            }
            Some(id) => self.open(Content::Note(id.to_string()), in_new_tab),
        }
    }

    pub fn open_memos(&mut self, in_new_tab: bool) {
        self.open(Content::Memos, in_new_tab);
    }

    pub fn open_file(&mut self, id: &str, in_new_tab: bool) {
        self.open(Content::File(id.to_string()), in_new_tab);
    }

    /// Follows a link: in this tab, a step Back returns from; with `in_new_tab`,
    /// in a new tab behind this one. A pinned tab keeps its page.
    pub fn follow_link(&mut self, id: &str, in_new_tab: bool) {
        self.dismiss_overlays();
        let content = Content::Note(id.to_string());
        if in_new_tab {
            if self.tabs.iter().any(|t| t.content == content) {
                return;
            }
            let tab = self.make_tab(content);
            self.insert_tab(tab, false);
        } else if self.active_tab().is_pinned {
            self.open(content, true);
        } else {
            self.set_active_content(content);
        }
    }

    fn open(&mut self, content: Content, in_new_tab: bool) {
        self.dismiss_overlays();
        if let Some(existing) = self.tabs.iter().find(|t| t.content == content) {
            self.active = existing.id.clone();
        } else if *self.active_content() == Content::Draft && !in_new_tab && !self.active_tab().is_pinned {
            self.set_active_content(content);
        } else {
            let tab = self.make_tab(content);
            self.insert_tab(tab, true);
        }
    }

    pub fn dismiss_overlays(&mut self) {
        self.show_settings = false;
        self.search_presented = false;
    }

    /// New Tab, New Note and the strip's + are one thing: a blank draft in a new
    /// tab. The active tab already being blank, it stays put.
    pub fn new_tab(&mut self) {
        self.dismiss_overlays();
        if *self.active_content() == Content::Draft {
            return;
        }
        let tab = self.make_tab(Content::Draft);
        self.insert_tab(tab, true);
    }

    fn insert_tab(&mut self, tab: NoteTab, activate: bool) {
        let first_unpinned = self.tabs.iter().position(|t| !t.is_pinned).unwrap_or(self.tabs.len());
        let at = (self.active_index() + 1).max(first_unpinned);
        if activate {
            self.active = tab.id.clone();
        }
        self.tabs.insert(at, tab);
    }

    pub fn select_tab(&mut self, id: &str) {
        if self.tabs.iter().any(|t| t.id == id) {
            self.dismiss_overlays();
            self.active = id.to_string();
        }
    }

    /// ⌘1…⌘8 pick that tab, ⌘9 always the last.
    pub fn select_tab_number(&mut self, number: usize) {
        let index = if number == 9 { self.tabs.len() - 1 } else { number.wrapping_sub(1) };
        if let Some(tab) = self.tabs.get(index) {
            let id = tab.id.clone();
            self.select_tab(&id);
        }
    }

    /// The next (or previous, with -1) tab, wrapping around.
    pub fn select_tab_offset(&mut self, offset: i64) {
        let n = self.tabs.len() as i64;
        if n > 1 {
            let index = (self.active_index() as i64 + offset + n).rem_euclid(n) as usize;
            let id = self.tabs[index].id.clone();
            self.select_tab(&id);
        }
    }

    pub fn close_tab(&mut self, id: &str) {
        let Some(index) = self.tabs.iter().position(|t| t.id == id) else { return };
        let tab = self.tabs.remove(index);
        self.histories.remove(id);
        self.preview_tabs.remove(id);
        if tab.content != Content::Draft {
            self.closed.push(tab);
            if self.closed.len() > 30 {
                self.closed.remove(0);
            }
        }
        if self.tabs.is_empty() {
            let blank = self.make_tab(Content::Draft);
            self.active = blank.id.clone();
            self.tabs = vec![blank];
        } else if id == self.active {
            // Focus goes to the tab on the right, else the one on the left.
            self.active = self.tabs[index.min(self.tabs.len() - 1)].id.clone();
        }
    }

    /// ⌘W: false when this is the window's only tab, so ⌘W closes the window. A
    /// pinned tab isn't closed: the first unpinned one comes forward instead.
    pub fn close_active_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        if self.active_tab().is_pinned {
            match self.tabs.iter().find(|t| !t.is_pinned).map(|t| t.id.clone()) {
                Some(other) => self.select_tab(&other),
                None => self.new_tab(),
            }
            return true;
        }
        let id = self.active.clone();
        self.close_tab(&id);
        true
    }

    pub fn close_other_tabs(&mut self, id: &str) {
        let others: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| t.id != id && !t.is_pinned)
            .map(|t| t.id.clone())
            .collect();
        for other in others {
            self.close_tab(&other);
        }
        self.select_tab(id);
    }

    pub fn close_tabs_to_the_right(&mut self, id: &str) {
        let Some(index) = self.tabs.iter().position(|t| t.id == id) else { return };
        let right: Vec<String> = self.tabs[index + 1..]
            .iter()
            .filter(|t| !t.is_pinned)
            .map(|t| t.id.clone())
            .collect();
        for other in right {
            self.close_tab(&other);
        }
    }

    pub fn can_reopen_closed_tab(&self) -> bool {
        !self.closed.is_empty()
    }

    /// ⇧⌘T: the last closed tab, back in a new tab, skipping ones whose note or
    /// file is gone.
    pub fn reopen_closed_tab(&mut self, exists: impl Fn(&Content) -> bool) {
        while let Some(tab) = self.closed.pop() {
            if !exists(&tab.content) {
                continue;
            }
            self.open(tab.content.clone(), true);
            if tab.is_pinned {
                let id = self.active.clone();
                self.toggle_pin(&id);
            }
            return;
        }
    }

    pub fn toggle_pin(&mut self, id: &str) {
        let Some(index) = self.tabs.iter().position(|t| t.id == id) else { return };
        let mut tab = self.tabs.remove(index);
        tab.is_pinned = !tab.is_pinned;
        let pinned = self.tabs.iter().filter(|t| t.is_pinned).count();
        self.tabs.insert(pinned, tab);
    }

    /// Drag to reorder, kept on its own side of the pinned boundary.
    pub fn move_tab(&mut self, id: &str, to: usize) {
        let Some(from) = self.tabs.iter().position(|t| t.id == id) else { return };
        let tab = self.tabs.remove(from);
        let pinned = self.tabs.iter().filter(|t| t.is_pinned).count();
        let (lo, hi) = if tab.is_pinned { (0, pinned) } else { (pinned, self.tabs.len()) };
        self.tabs.insert(to.clamp(lo, hi), tab);
    }

    /// A note or folder moved or renamed in the app: every tab inside follows.
    pub fn remap_tabs(&mut self, old: &str, new: &str) {
        let remapped = |content: Content| -> Content {
            match &content {
                Content::Note(id) => library_move::remap(id, old, new).map(Content::Note).unwrap_or(content),
                Content::File(id) => library_move::remap(id, old, new).map(Content::File).unwrap_or(content),
                _ => content,
            }
        };
        for tab in &mut self.tabs {
            tab.content = remapped(tab.content.clone());
        }
        for history in self.histories.values_mut() {
            history.remap(remapped);
        }
    }

    /// A note or file was deleted: its tabs close; the only tab turns blank.
    pub fn remove_tabs(&mut self, content: &Content) {
        for history in self.histories.values_mut() {
            history.remove(content);
        }
        let showing: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| t.content == *content)
            .map(|t| t.id.clone())
            .collect();
        for id in showing {
            if self.tabs.len() == 1 {
                self.tabs[0].content = Content::Draft;
            } else if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                self.tabs.remove(index);
                self.histories.remove(&id);
                if id == self.active {
                    self.active = self.tabs[index.min(self.tabs.len() - 1)].id.clone();
                }
            }
        }
    }

    /// The library changed on disk: tabs whose note or file is gone close.
    pub fn prune_tabs(&mut self, exists: impl Fn(&Content) -> bool) {
        let gone: Vec<Content> = self
            .tabs
            .iter()
            .filter(|t| !exists(&t.content))
            .map(|t| t.content.clone())
            .collect();
        for content in gone {
            self.remove_tabs(&content);
        }
        for history in self.histories.values_mut() {
            history.back.retain(|c| exists(c));
            history.forward.retain(|c| exists(c));
        }
    }

    /// A draft written to disk for the first time becomes that note, in the tab
    /// it was typed in, which may no longer be the active one.
    pub fn adopt_draft(&mut self, note_id: &str, tab_id: &str) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id && t.content == Content::Draft) {
            tab.content = Content::Note(note_id.to_string());
        }
    }

    /// Brings back the tabs last open in `workspace`. With none saved, tabs from
    /// another library close; a draft, Memos and what's inside the new folder stay.
    pub fn restore_tabs(&mut self, workspace: &str, saved: Option<SavedTabs>) {
        self.tabs_workspace = Some(workspace.to_string());
        self.closed.clear();
        self.histories.clear();
        if let Some(saved) = saved {
            self.active = saved.tabs[saved.active as usize].id.clone();
            self.tabs = saved.tabs;
            return;
        }
        let prefix = if workspace.ends_with('/') { workspace.to_string() } else { format!("{workspace}/") };
        let kept: Vec<NoteTab> = self
            .tabs
            .iter()
            .filter(|t| t.content.path().is_none_or(|p| p.starts_with(&prefix)))
            .cloned()
            .collect();
        self.tabs = if kept.is_empty() { vec![self.make_tab(Content::Draft)] } else { kept };
        if !self.tabs.iter().any(|t| t.id == self.active) {
            self.active = self.tabs[0].id.clone();
        }
    }

    /// The JSON to keep for `workspace`; `None` if the tabs belong to another
    /// folder (one still opening).
    pub fn saved_tabs(&self, workspace: &str) -> Option<Vec<u8>> {
        if self.tabs_workspace.as_deref() != Some(workspace) {
            return None;
        }
        SavedTabs::encode(&self.tabs, self.active_index() as i64)
    }

    /// How strongly the window is dimmed behind a floating card.
    pub fn scrim_strength(&self, glass: bool) -> f32 {
        if self.show_settings {
            return if glass { 0.45 } else { 1. };
        }
        if self.search_presented { 1. } else { 0. }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> AppModel {
        let mut n = 0;
        AppModel::new(move || {
            n += 1;
            format!("t{n}")
        })
    }

    fn note(s: &str) -> Content {
        Content::Note(s.to_string())
    }

    fn contents(m: &AppModel) -> Vec<Content> {
        m.tabs().iter().map(|t| t.content.clone()).collect()
    }

    #[test]
    fn a_draft_is_filled_in_place_and_a_note_opens_beside() {
        let mut m = model();
        m.open_note(Some("/a.md"), false);
        assert_eq!(contents(&m), [note("/a.md")]);
        m.open_note(Some("/b.md"), false);
        assert_eq!(contents(&m), [note("/a.md"), note("/b.md")]);
        // Already open: that tab comes forward.
        m.open_note(Some("/a.md"), false);
        assert_eq!(m.active_index(), 0);
        assert_eq!(m.tabs().len(), 2);
    }

    #[test]
    fn links_step_back_and_forward() {
        let mut m = model();
        m.open_note(Some("/a.md"), false);
        m.follow_link("/b.md", false);
        assert_eq!(m.selected_note(), Some("/b.md"));
        assert!(m.can_go_back());
        m.go_back();
        assert_eq!(m.selected_note(), Some("/a.md"));
        m.go_forward();
        assert_eq!(m.selected_note(), Some("/b.md"));
    }

    #[test]
    fn a_pinned_tab_keeps_its_page() {
        let mut m = model();
        m.open_note(Some("/a.md"), false);
        let id = m.active_id().to_string();
        m.toggle_pin(&id);
        m.follow_link("/b.md", false);
        assert_eq!(contents(&m), [note("/a.md"), note("/b.md")]);
        // ⌘W on a pinned tab brings an unpinned one forward instead.
        m.select_tab(&id);
        assert!(m.close_active_tab());
        assert_eq!(m.selected_note(), Some("/b.md"));
        assert_eq!(m.tabs().len(), 2);
    }

    #[test]
    fn closing_goes_right_then_left_and_never_empties() {
        let mut m = model();
        m.open_note(Some("/a.md"), false);
        m.open_note(Some("/b.md"), false);
        m.open_note(Some("/c.md"), false);
        m.select_tab_number(2);
        assert!(m.close_active_tab());
        assert_eq!(m.selected_note(), Some("/c.md"));
        assert!(m.close_active_tab());
        assert_eq!(m.selected_note(), Some("/a.md"));
        assert!(!m.close_active_tab());
        let id = m.active_id().to_string();
        m.close_tab(&id);
        assert_eq!(contents(&m), [Content::Draft]);
    }

    #[test]
    fn reopening_skips_what_is_gone() {
        let mut m = model();
        m.open_note(Some("/a.md"), false);
        m.open_note(Some("/b.md"), false);
        m.open_note(Some("/c.md"), false);
        m.close_active_tab();
        m.close_active_tab();
        m.reopen_closed_tab(|c| *c != note("/b.md"));
        assert_eq!(m.selected_note(), Some("/c.md"));
    }

    #[test]
    fn tab_numbers_and_wrapping() {
        let mut m = model();
        for p in ["/a.md", "/b.md", "/c.md"] {
            m.open_note(Some(p), false);
        }
        m.select_tab_number(9);
        assert_eq!(m.selected_note(), Some("/c.md"));
        m.select_tab_offset(1);
        assert_eq!(m.selected_note(), Some("/a.md"));
        m.select_tab_offset(-1);
        assert_eq!(m.selected_note(), Some("/c.md"));
        m.select_tab_number(7);
        assert_eq!(m.selected_note(), Some("/c.md"));
    }

    #[test]
    fn moves_stay_on_their_side_of_the_pins() {
        let mut m = model();
        for p in ["/a.md", "/b.md", "/c.md"] {
            m.open_note(Some(p), false);
        }
        let a = m.tabs()[0].id.clone();
        m.toggle_pin(&a);
        let c = m.tabs()[2].id.clone();
        m.move_tab(&c, 0);
        assert_eq!(contents(&m), [note("/a.md"), note("/c.md"), note("/b.md")]);
    }

    #[test]
    fn deleting_and_pruning() {
        let mut m = model();
        m.open_note(Some("/a.md"), false);
        m.remove_tabs(&note("/a.md"));
        assert_eq!(contents(&m), [Content::Draft]);
        m.open_note(Some("/b.md"), false);
        m.open_note(Some("/c.md"), false);
        m.prune_tabs(|c| *c != note("/b.md"));
        assert_eq!(contents(&m), [note("/c.md")]);
    }

    #[test]
    fn restoring_a_library_keeps_what_belongs_there() {
        let mut m = model();
        m.open_note(Some("/lib/a.md"), false);
        m.open_note(Some("/other/b.md"), false);
        m.restore_tabs("/lib", None);
        assert_eq!(contents(&m), [note("/lib/a.md")]);
        assert!(m.saved_tabs("/elsewhere").is_none());
        let saved = SavedTabs::decode(&m.saved_tabs("/lib").unwrap()).unwrap();
        m.open_note(Some("/lib/z.md"), false);
        m.restore_tabs("/lib", Some(saved));
        assert_eq!(contents(&m), [note("/lib/a.md")]);
    }

    #[test]
    fn a_draft_becomes_its_note_in_its_own_tab() {
        let mut m = model();
        let tab = m.active_id().to_string();
        m.open_note(Some("/a.md"), true);
        // Draft filled in place was skipped by in_new_tab; the draft tab stays.
        m.adopt_draft("/n.md", &tab);
        assert_eq!(contents(&m), [note("/n.md"), note("/a.md")]);
    }

    #[test]
    fn remapping_follows_moves_in_history_too() {
        let mut m = model();
        m.open_note(Some("/lib/old/a.md"), false);
        m.follow_link("/lib/b.md", false);
        m.remap_tabs("/lib/old", "/lib/new");
        m.go_back();
        assert_eq!(m.selected_note(), Some("/lib/new/a.md"));
    }
}
