//! What's on the page: one editor per open note tab, kept alive so a tab switched
//! back to shows exactly what it showed, or the Memos and file pages. Saving and
//! taking in outside changes happen here. A port of `EditorView`'s host logic.

use fumi_memos::MemosView;
use fumi_settings::file_preview::FilePreview;
use fumi_settings::welcome::{EmptyLibraryPage, EmptyPage, empty_page};
use fumi_workspace::{Workspace, WorkspaceEvent, now};
use fumi_core::note_store::{ReadError, read_note_file};
use fumi_core::tabs::Content;
use fumi_core::wiki_link::WikiLink;
use fumi_editor::display::{LinkContext, Typography};
use fumi_editor::editor::{Editor, EditorEvent, ViewState};
use fumi_ui::theme::{PAGE_INNER_RADIUS, t};
use fumi_editor::element::CaretSlot;
use gpui::*;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::path::Path;
use std::time::Duration;

/// One note open in a tab.
pub struct NotePage {
    pub editor: Entity<Editor>,
    /// The note this editor's text belongs to: the id given when it opened, so a
    /// late save can't land in a note switched to since.
    pub note_id: String,
    pub is_draft: bool,
    /// What the file held when the text being edited was last in step with it.
    pub base: String,
    pub error: Option<String>,
    /// Whether the editor had the keyboard when its tab was last left.
    had_focus: bool,
    _sub: Subscription,
}

pub struct PageHost {
    pub workspace: Entity<Workspace>,
    window: AnyWindowHandle,
    pub pages: HashMap<String, NotePage>,
    /// The file on the active tab. Dropped otherwise: its native view would stay
    /// on screen.
    file: Option<(String, Entity<FilePreview>)>,
    empty: Entity<EmptyLibraryPage>,
    /// The Memos page while its tab is active; leaving loses the draft and
    /// filters, as in Swift.
    memos: Option<Entity<MemosView>>,
    typo: Typography,
    /// The window's caret layer, which every editor here draws its caret on.
    pub caret: Option<(Rc<CaretSlot>, Rc<dyn Fn(&mut App)>)>,
    /// Where each note was left, for opening it again within this launch: what
    /// was selected, how far it was scrolled, whether it had the keyboard.
    places: HashMap<String, (ViewState, bool)>,
    /// The tab and note last on the page.
    shown: Option<(String, String)>,
    _subs: Vec<Subscription>,
}

impl PageHost {
    pub fn new(workspace: Entity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subs = vec![
            cx.observe_in(&workspace, window, |this, _, window, cx| {
                this.drop_closed_tabs(window, cx);
                cx.notify();
            }),
            cx.subscribe(&workspace, |this, _, e: &WorkspaceEvent, cx| {
                if matches!(e, WorkspaceEvent::LibraryChanged) {
                    this.take_outside_changes(cx);
                }
            }),
            // Quitting inside the half second before a save would lose what was
            // typed in it: write every tab first.
            cx.on_app_quit(|this, cx| {
                this.flush_all(cx);
                async {}
            }),
        ];
        let empty = cx.new(|cx| EmptyLibraryPage::new(workspace.clone(), cx));
        PageHost {
            workspace,
            window: window.window_handle(),
            pages: HashMap::new(),
            file: None,
            empty,
            memos: None,
            typo: Typography::from_settings(),
            caret: None,
            places: HashMap::new(),
            shown: None,
            _subs: subs,
        }
    }

    pub fn theme_changed(&mut self, cx: &mut Context<Self>) {
        self.typo = Typography::from_settings();
        for page in self.pages.values() {
            let typo = self.typo.clone();
            page.editor.update(cx, |e, cx| {
                e.settings_changed(typo, cx);
            });
        }
        cx.notify();
    }

    fn drop_closed_tabs(&mut self, window: &Window, cx: &mut Context<Self>) {
        let w = self.workspace.read(cx);
        let tabs: HashMap<String, Content> = w.model.tabs().iter().map(|t| (t.id.clone(), t.content.clone())).collect();
        let gone: Vec<String> = self
            .pages
            .iter()
            .filter(|(id, page)| match tabs.get(*id) {
                Some(Content::Note(note)) => *note != page.note_id,
                Some(Content::Draft) => !page.is_draft,
                _ => true,
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in gone {
            // Anything typed in the last half second goes to its own note first.
            self.flush(&id, cx);
            if let Some(page) = self.pages.remove(&id)
                && !page.is_draft
            {
                let editor = page.editor.read(cx);
                let place = (editor.view_state(), editor.focus.is_focused(window));
                self.places.insert(page.note_id, place);
            }
        }
    }

    /// Opens the editor for the tab, if it hasn't one.
    fn ensure_page(&mut self, tab_id: &str, content: &Content, window: &mut Window, cx: &mut Context<Self>) {
        if self.pages.contains_key(tab_id) {
            return;
        }
        let (note_id, text, is_draft, error) = match content {
            Content::Note(id) => match self.read_note(id, cx) {
                Ok(text) => (id.clone(), text, false, None),
                Err(message) => (id.clone(), String::new(), false, Some(message)),
            },
            Content::Draft => {
                let draft = self.workspace.update(cx, |w, _| w.store.draft_note(now()));
                let Some(draft) = draft else { return };
                (draft.id, draft.content, true, None)
            }
            _ => return,
        };
        let path = Path::new(&note_id).to_path_buf();
        let bytes = text.clone().into_bytes();
        let links = self.link_context(&path, cx);
        let typo = self.typo.clone();
        let library = self.workspace.read(cx).workspace_path();
        // Laid out at the page's size from its first frame, so what's restored
        // below is what's drawn.
        let pane = self.pages.values().find_map(|p| {
            let e = p.editor.read(cx);
            Some((e.pane_width?, e.pane_height?))
        });
        let place = if is_draft { None } else { self.places.get(&note_id).copied() };
        let editor = cx.new(move |cx| {
            let mut e = Editor::new(bytes, Some(path), std::env::var("FUMI_NO_BLINK").is_err(), cx);
            e.typo = typo;
            e.links = links;
            e.library = library;
            e.draft_placeholder = is_draft;
            if let Some((width, height)) = pane {
                e.pane_width = Some(width);
                e.pane_height = Some(height);
            }
            if let Some((state, _)) = place {
                e.restore_view(state);
            }
            e
        });
        let tab = tab_id.to_string();
        let sub = cx.subscribe_in(&editor, window, move |this, _, event: &EditorEvent, window, cx| {
            this.on_editor_event(&tab, event, window, cx);
        });
        self.pages.insert(
            tab_id.to_string(),
            NotePage {
                editor,
                note_id,
                is_draft,
                base: text,
                error,
                had_focus: place.is_some_and(|(_, focused)| focused),
                _sub: sub,
            },
        );
    }

    /// What changes when another tab or note comes up. The editor left keeps its
    /// state and notes whether it had the keyboard. The one arriving gets it if it
    /// had it then, and a draft always does, with the caret at its end.
    fn page_shown(&mut self, tab: &str, window: &mut Window, cx: &mut Context<Self>) {
        let note = self.pages.get(tab).map(|page| page.note_id.clone());
        let arriving = note.map(|note| (tab.to_string(), note));
        if self.shown == arriving {
            return;
        }
        let mut was_focused = false;
        if let Some((old_tab, old_note)) = self.shown.take()
            && let Some(old) = self.pages.get_mut(&old_tab)
            && old.note_id == old_note
        {
            old.had_focus = old.editor.read(cx).focus.is_focused(window);
            was_focused = old.had_focus;
            old.editor.update(cx, |e, cx| e.left_view(window, cx));
        }
        self.shown = arriving;
        let Some(page) = self.pages.get(tab) else {
            if was_focused {
                window.blur(cx);
            }
            return;
        };
        if page.is_draft {
            page.editor.update(cx, |e, _| e.caret_to_end());
        }
        if page.is_draft || page.had_focus {
            window.focus(&page.editor.read(cx).focus.clone(), cx);
        } else if was_focused {
            window.blur(cx);
        }
    }

    fn link_context(&self, note: &Path, cx: &App) -> Option<LinkContext> {
        let w = self.workspace.read(cx);
        Some(LinkContext { index: w.links.clone(), folder: note.parent().map(Path::to_path_buf) })
    }

    fn read_note(&self, id: &str, cx: &App) -> Result<String, String> {
        let w = self.workspace.read(cx);
        if w.store.is_unreadable(id) {
            return Err("This file isn't UTF-8 text, so Fumi won't edit it.".to_string());
        }
        match read_note_file(Path::new(id)) {
            Ok(file) => Ok(file.text),
            Err(ReadError::NotFound) => Ok(w.note(id).map(|n| n.content.clone()).unwrap_or_default()),
            Err(ReadError::NotUtf8) => Err("This file isn't UTF-8 text, so Fumi won't edit it.".to_string()),
            Err(ReadError::Io(error)) => Err(format!("Couldn't read this note: {error}")),
        }
    }

    fn on_editor_event(&mut self, tab: &str, event: &EditorEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            EditorEvent::SaveRequested | EditorEvent::SaveNow => self.flush(tab, cx),
            EditorEvent::ContentSettled => cx.notify(),
            EditorEvent::OpenWiki { inner, new_tab, .. } => {
                let Some(page) = self.pages.get(tab) else { return };
                let folder = Path::new(&page.note_id).parent().map(Path::to_path_buf);
                let Some(link) = WikiLink::parse(inner) else { return };
                let target = self.workspace.read(cx).links.resolve(&link.target, folder.as_deref()).map(|e| e.id.clone());
                if let Some(id) = target {
                    let new_tab = *new_tab;
                    self.workspace.update(cx, |w, cx| {
                        w.model.follow_link(&id, new_tab);
                        w.navigated(cx);
                    });
                    // In this tab, at the heading it names, once its editor is up.
                    if let Some(heading) = link.heading.filter(|_| !new_tab) {
                        cx.spawn(async move |this: WeakEntity<PageHost>, cx| {
                            cx.background_executor().timer(Duration::from_millis(350)).await;
                            let _ = this.update(cx, |host, cx| {
                                let active = host.workspace.read(cx).model.active_id().to_string();
                                if let Some(page) = host.pages.get(&active) {
                                    page.editor.update(cx, |e, cx| e.scroll_to_heading_named(&heading, cx));
                                }
                            });
                        })
                        .detach();
                    }
                }
            }
            EditorEvent::CreateNote { name, new_tab } => {
                let Some(page) = self.pages.get(tab) else { return };
                let folder = Path::new(&page.note_id).parent().map(Path::to_path_buf);
                let new_tab = *new_tab;
                let created = self.workspace.update(cx, |w, _| w.store.create_note(Some(name), folder.as_deref(), now()));
                if let Ok(Some(note)) = created {
                    self.workspace.update(cx, |w, cx| {
                        w.model.follow_link(&note.id, new_tab);
                        w.library_changed(cx);
                        w.navigated(cx);
                    });
                }
            }
            _ => {}
        }
        let _ = window;
    }

    /// Writes the tab's text if it has unsaved changes. Refused saves leave the
    /// file alone and say why.
    pub fn flush_all(&mut self, cx: &mut Context<Self>) {
        let tabs: Vec<String> = self.pages.keys().cloned().collect();
        for tab in tabs {
            self.flush(&tab, cx);
        }
    }

    pub fn flush(&mut self, tab: &str, cx: &mut Context<Self>) {
        let Some(page) = self.pages.get_mut(tab) else { return };
        let editor = page.editor.read(cx);
        if !editor.dirty || page.error.is_some() {
            return;
        }
        let text = editor.text();
        let note_id = page.note_id.clone();
        let base = page.base.clone();
        let result = self.workspace.update(cx, |w, _| w.store.save_content(&text, &note_id, Some(&base), now()));
        let page = self.pages.get_mut(tab).expect("still here");
        match result {
            Ok(saved) => {
                page.editor.update(cx, |e, _| e.dirty = false);
                if let Some(written) = saved.written {
                    // Not while an input method is composing: the file keeps the
                    // merge, and the next save merges again from the old base.
                    let taken = written == text || page.editor.update(cx, |e, cx| e.replace_from_outside(&written, cx));
                    if taken {
                        page.base = written;
                    }
                }
                if let Some(copy) = saved.conflict_copy {
                    let workspace = self.workspace.clone();
                    let (id, name) = (copy.id.clone(), copy.filename());
                    // On this page's window, whether or not it's the key one.
                    let window = self.window;
                    cx.defer(move |cx| {
                        let _ = window.update(cx, |_, window, cx| {
                            fumi_toolbar::alerts::show_conflict_copy(id, &name, &workspace, window, cx);
                        });
                    });
                }
                if saved.adopted_draft {
                    page.is_draft = false;
                    page.editor.update(cx, |e, _| e.draft_placeholder = false);
                    let tab = tab.to_string();
                    self.workspace.update(cx, |w, cx| {
                        w.model.adopt_draft(&note_id, &tab);
                        w.library_changed(cx);
                    });
                } else {
                    self.workspace.update(cx, |w, cx| {
                        w.revision += 1;
                        cx.notify();
                    });
                }
            }
            Err(error) => page.error = Some(format!("Couldn't save: {error}")),
        }
        cx.notify();
    }

    /// The library changed on disk: open notes pull in the new text, merged with
    /// anything typed here that isn't saved yet.
    fn take_outside_changes(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.pages.keys().cloned().collect();
        for tab in ids {
            let page = &self.pages[&tab];
            if page.is_draft {
                continue;
            }
            let disk = self.workspace.read(cx).note(&page.note_id).map(|n| n.content.clone());
            let Some(disk) = disk else { continue };
            if disk == page.base {
                continue;
            }
            if page.editor.read(cx).dirty {
                // Typed here and changed outside: save now, which merges the two
                // and puts the merge on screen.
                self.flush(&tab, cx);
                continue;
            }
            let page = self.pages.get_mut(&tab).expect("still here");
            page.base = disk.clone();
            page.editor.update(cx, |e, cx| {
                // No edits to keep: an input method composing loses its text, and
                // the note comes in all the same.
                e.marked = None;
                e.replace_from_outside(&disk, cx);
            });
        }
        self.refresh_links(cx);
    }

    fn refresh_links(&mut self, cx: &mut Context<Self>) {
        let index = self.workspace.read(cx).links.clone();
        for page in self.pages.values() {
            let index = index.clone();
            page.editor.update(cx, |e, cx| {
                if let Some(l) = &mut e.links {
                    l.index = index;
                }
                e.links_changed(cx);
            });
        }
    }

    /// The active tab's text as typed, for the toolbar's counts.
    pub fn active_text(&self, cx: &App) -> Option<String> {
        let w = self.workspace.read(cx);
        let page = self.pages.get(w.model.active_id())?;
        Some(page.editor.read(cx).text())
    }

    /// A note opened from search with a match in it: once the page is up, its
    /// editor scrolls to the match's line and flashes the match. The delay is
    /// the wiki-link heading jump's, giving a newly opened editor a frame to
    /// lay its lines out.
    pub fn reveal_match(&mut self, note_id: &str, range: Option<Range<usize>>, cx: &mut Context<Self>) {
        let Some(range) = range else { return };
        let note_id = note_id.to_string();
        cx.spawn(async move |this: WeakEntity<PageHost>, cx| {
            cx.background_executor().timer(Duration::from_millis(350)).await;
            let _ = this.update(cx, |host, cx| {
                let active = host.workspace.read(cx).model.active_id().to_string();
                if let Some(page) = host.pages.get(&active)
                    && page.note_id == note_id
                {
                    page.editor.update(cx, |e, cx| e.reveal_match(range, cx));
                }
            });
        })
        .detach();
    }
}

impl Render for PageHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        // A page without an editor leaves no caret behind; an editor's own render
        // puts its caret back.
        if let Some((slot, _)) = &self.caret {
            slot.rect.set(None);
        }
        let (tab, content) = {
            let w = self.workspace.read(cx);
            let active = w.model.active_tab();
            (active.id.clone(), active.content.clone())
        };
        // A library with no notes shows a page about that instead of a draft.
        if content == Content::Draft {
            let (loaded, count) = {
                let w = self.workspace.read(cx);
                (w.store.has_loaded_notes(), w.store.notes().len())
            };
            match empty_page(loaded, count) {
                EmptyPage::NoNotes => return div().size_full().rounded(px(PAGE_INNER_RADIUS)).bg(c.page_content).child(self.empty.clone()).into_any_element(),
                EmptyPage::Blank => return div().size_full().rounded(px(PAGE_INNER_RADIUS)).bg(c.page_content).into_any_element(),
                EmptyPage::Draft => {}
            }
        }
        self.ensure_page(&tab, &content, window, cx);
        self.page_shown(&tab, window, cx);
        let body = div().size_full().rounded(px(PAGE_INNER_RADIUS)).bg(c.page_content);
        match &content {
            Content::File(id) => {
                let current = self.file.as_ref().is_some_and(|(shown, _)| shown == id);
                if !current {
                    let file = self.workspace.read(cx).store.files().iter().find(|f| f.id == *id).cloned();
                    self.file = match file {
                        Some(file) => {
                            let workspace = self.workspace.clone();
                            Some((id.clone(), cx.new(|cx| FilePreview::new(workspace, &file, window, cx))))
                        }
                        None => None,
                    };
                }
            }
            _ => self.file = None,
        }
        if content == Content::Memos {
            if self.memos.is_none() {
                let workspace = self.workspace.clone();
                self.memos = Some(cx.new(|cx| MemosView::new(workspace, window, cx)));
            }
        } else {
            self.memos = None;
        }
        match content {
            Content::Draft | Content::Note(_) => match self.pages.get(&tab) {
                Some(page) if page.error.is_some() => body
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(c.text3)
                    .child(page.error.clone().unwrap_or_default())
                    .into_any_element(),
                Some(page) => body.child(page.editor.clone()).into_any_element(),
                None => body.into_any_element(),
            },
            Content::Memos => match &self.memos {
                Some(memos) => body.child(memos.clone()).into_any_element(),
                None => body.into_any_element(),
            },
            Content::File(_) => match &self.file {
                Some((_, preview)) => body.child(preview.clone()).into_any_element(),
                None => body.into_any_element(),
            },
        }
    }
}
