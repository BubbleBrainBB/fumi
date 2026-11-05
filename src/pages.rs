//! What's on the page: one editor per open note tab, kept alive so a tab switched
//! back to shows exactly what it showed, or the Memos and file pages. Saving and
//! taking in outside changes happen here. A port of `EditorView`'s host logic.

use fumi_workspace::{Workspace, WorkspaceEvent, now};
use fumi_core::note_store::{ReadError, read_note_file};
use fumi_core::tabs::Content;
use fumi_core::wiki_link::WikiLink;
use fumi_editor::display::{LinkContext, Typography};
use fumi_editor::editor::{Editor, EditorEvent};
use fumi_ui::theme::t;
use gpui::*;
use std::collections::HashMap;
use std::path::Path;

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
    _sub: Subscription,
}

pub struct ToolbarContent {
    pub title: SharedString,
    pub subtitle: Option<SharedString>,
}

pub struct PageHost {
    pub workspace: Entity<Workspace>,
    pub pages: HashMap<String, NotePage>,
    typo: Typography,
    _subs: Vec<Subscription>,
}

impl PageHost {
    pub fn new(workspace: Entity<Workspace>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subs = vec![
            cx.observe(&workspace, |this, _, cx| {
                this.drop_closed_tabs(cx);
                cx.notify();
            }),
            cx.subscribe(&workspace, |this, _, e: &WorkspaceEvent, cx| {
                if matches!(e, WorkspaceEvent::LibraryChanged) {
                    this.take_outside_changes(cx);
                }
            }),
        ];
        PageHost {
            workspace,
            pages: HashMap::new(),
            typo: Typography::from_settings(),
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

    fn drop_closed_tabs(&mut self, cx: &mut Context<Self>) {
        let w = self.workspace.read(cx);
        let tabs: HashMap<String, Content> = w.model.tabs().iter().map(|t| (t.id.clone(), t.content.clone())).collect();
        let gone: Vec<String> = self
            .pages
            .iter()
            .filter(|(id, page)| match tabs.get(*id) {
                Some(Content::Note(note)) => *note != page.note_id && !page.is_draft,
                Some(Content::Draft) => !page.is_draft,
                _ => true,
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in gone {
            // Anything typed in the last half second goes to its own note first.
            self.flush(&id, cx);
            self.pages.remove(&id);
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
        let editor = cx.new(move |cx| {
            let mut e = Editor::new(bytes, Some(path), true, cx);
            e.typo = typo;
            e.links = links;
            e
        });
        if is_draft {
            window.focus(&editor.read(cx).focus.clone(), cx);
        }
        let tab = tab_id.to_string();
        let sub = cx.subscribe_in(&editor, window, move |this, _, event: &EditorEvent, window, cx| {
            this.on_editor_event(&tab, event, window, cx);
        });
        self.pages.insert(
            tab_id.to_string(),
            NotePage { editor, note_id, is_draft, base: text, error, _sub: sub },
        );
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
                }
            }
            _ => {}
        }
        let _ = window;
    }

    /// Writes the tab's text if it has unsaved changes. Refused saves leave the
    /// file alone and say why.
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
                    page.base = written.clone();
                    if written != text {
                        page.editor.update(cx, |e, cx| e.replace_from_outside(&written, cx));
                    }
                }
                if saved.adopted_draft {
                    page.is_draft = false;
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
            let editor = page.editor.read(cx);
            if disk == page.base || editor.marked.is_some() {
                continue;
            }
            if editor.dirty {
                // The next save merges; nothing to do until then.
                continue;
            }
            let page = self.pages.get_mut(&tab).expect("still here");
            page.base = disk.clone();
            page.editor.update(cx, |e, cx| e.replace_from_outside(&disk, cx));
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

    pub fn toolbar(&self, cx: &App) -> ToolbarContent {
        let w = self.workspace.read(cx);
        let title: SharedString = match w.model.active_content() {
            Content::Draft => "New Note".into(),
            Content::Memos => "Memos".into(),
            Content::Note(id) => w.note(id).map(|n| n.filename()).unwrap_or_default().into(),
            Content::File(id) => Path::new(id)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into(),
        };
        ToolbarContent { title, subtitle: None }
    }
}

impl Render for PageHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let (tab, content) = {
            let w = self.workspace.read(cx);
            let active = w.model.active_tab();
            (active.id.clone(), active.content.clone())
        };
        self.ensure_page(&tab, &content, window, cx);
        let body = div().size_full().bg(c.page_content);
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
            Content::Memos => body.child("Memos").into_any_element(),
            Content::File(id) => body.child(id).into_any_element(),
        }
    }
}
