//! Every command of the menu bar and the keyboard. Menu commands act on the
//! focused window: its root element carries the handlers, through
//! [`register_actions`]; a command unavailable right now has no handler, which is
//! what greys its menu item and lets its key fall through.

use crate::mac;
use crate::shell;
use crate::toolbar::{Toolbar, ToolbarEvent, publish_target};
use fumi_core::app_model::PublishTarget;
use fumi_core::library_list::{missing_reason_on_disk, standardized_path};
use fumi_core::tabs::Content;
use fumi_ui::{native, settings};
use fumi_workspace::{Workspace, libraries};
use gpui::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

actions!(
    fumi,
    [
        NewNote, NewTab, NewMemo, NewWindow, OpenFolder, ReopenClosedTab, CloseTab, ExportAllNotes, TogglePreview,
        CopyForPublish, OpenSettings, ToggleSidebar, GoBack, GoForward, SearchNotes, ShowLibraries, GoToLibrary1,
        GoToLibrary2, GoToLibrary3, GoToLibrary4, GoToLibrary5, GoToLibrary6, GoToLibrary7, GoToLibrary8,
        GoToLibrary9, NextTab, PreviousTab, TogglePinTab, SelectTab1, SelectTab2, SelectTab3, SelectTab4, SelectTab5,
        SelectTab6, SelectTab7, SelectTab8, SelectTab9, About, HideApp, HideOthers, ShowAll, Quit, Minimize, Zoom,
        BringAllToFront, ShowHelp, MenuUndo, MenuRedo, MenuCut, MenuCopy, MenuPaste, MenuSelectAll, MenuBold,
        MenuItalic, MenuInsertLink, MenuToggleTodo,
    ]
);

/// Every shortcut of the menu bar and the shell. No context: a focused editor or
/// field binds its own ⌘C and friends, and the deeper context wins.
pub fn key_bindings() -> Vec<KeyBinding> {
    macro_rules! keys {
        ($(($key:literal, $action:expr)),* $(,)?) => { vec![$(KeyBinding::new($key, $action, None)),*] };
    }
    keys![
        ("cmd-,", OpenSettings),
        ("cmd-h", HideApp),
        ("alt-cmd-h", HideOthers),
        ("cmd-q", Quit),
        ("cmd-n", NewNote),
        ("cmd-t", NewTab),
        ("cmd-shift-n", NewWindow),
        ("cmd-o", OpenFolder),
        ("cmd-shift-t", ReopenClosedTab),
        ("cmd-w", CloseTab),
        ("ctrl-cmd-p", TogglePreview),
        ("alt-cmd-c", CopyForPublish),
        ("alt-cmd-s", ToggleSidebar),
        ("cmd-[", GoBack),
        ("cmd-]", GoForward),
        ("cmd-shift-f", SearchNotes),
        ("ctrl-cmd-1", GoToLibrary1),
        ("ctrl-cmd-2", GoToLibrary2),
        ("ctrl-cmd-3", GoToLibrary3),
        ("ctrl-cmd-4", GoToLibrary4),
        ("ctrl-cmd-5", GoToLibrary5),
        ("ctrl-cmd-6", GoToLibrary6),
        ("ctrl-cmd-7", GoToLibrary7),
        ("ctrl-cmd-8", GoToLibrary8),
        ("ctrl-cmd-9", GoToLibrary9),
        ("ctrl-tab", NextTab),
        ("ctrl-shift-tab", PreviousTab),
        ("cmd-shift-]", NextTab),
        ("cmd-}", NextTab),
        ("cmd-shift-[", PreviousTab),
        ("cmd-{", PreviousTab),
        ("alt-cmd-p", TogglePinTab),
        ("cmd-1", SelectTab1),
        ("cmd-2", SelectTab2),
        ("cmd-3", SelectTab3),
        ("cmd-4", SelectTab4),
        ("cmd-5", SelectTab5),
        ("cmd-6", SelectTab6),
        ("cmd-7", SelectTab7),
        ("cmd-8", SelectTab8),
        ("cmd-9", SelectTab9),
        ("cmd-m", Minimize),
        ("cmd-z", MenuUndo),
        ("cmd-shift-z", MenuRedo),
        ("cmd-x", MenuCut),
        ("cmd-c", MenuCopy),
        ("cmd-v", MenuPaste),
        ("cmd-a", MenuSelectAll),
        ("cmd-b", MenuBold),
        ("cmd-i", MenuItalic),
        ("cmd-k", MenuInsertLink),
        ("cmd-l", MenuToggleTodo),
    ]
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(key_bindings());
    cx.bind_keys([KeyBinding::new("enter", crate::welcome::OpenDefault, Some(crate::welcome::CONTEXT))]);
}

/// What the focused window can do right now. Whatever is false greys its menu
/// items and its shortcut does nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    pub has_note: bool,
    pub several_tabs: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub can_reopen: bool,
    pub library_open: bool,
    pub any_extension: bool,
    pub can_copy: bool,
}

impl Caps {
    pub fn compute(
        has_note: bool,
        several_tabs: bool,
        can_go_back: bool,
        can_go_forward: bool,
        can_reopen: bool,
        library_open: bool,
        wechat: bool,
        exports_cards: bool,
        xhs: bool,
    ) -> Self {
        Caps {
            has_note,
            several_tabs,
            can_go_back,
            can_go_forward,
            can_reopen,
            library_open,
            any_extension: wechat || xhs,
            can_copy: (wechat || xhs) && has_note && (exports_cards || wechat),
        }
    }

    pub fn of(w: &Workspace) -> Self {
        let m = &w.model;
        let (wechat, xhs) = (extension_wechat(), extension_xhs());
        let exports_cards = xhs && publish_target(m.selected_note()) == PublishTarget::Xiaohongshu;
        Self::compute(
            m.selected_note().is_some(),
            m.shows_tab_strip(),
            m.can_go_back(),
            m.can_go_forward(),
            m.can_reopen_closed_tab(),
            w.store.is_open(),
            wechat,
            exports_cards,
            xhs,
        )
    }
}

pub fn extension_wechat() -> bool {
    settings::get_bool(settings::keys::WECHAT_EXTENSION, false)
}

pub fn extension_xhs() -> bool {
    settings::get_bool(settings::keys::XHS_EXTENSION, false)
}

fn on_workspace(t: &Entity<Toolbar>, cx: &mut App, f: impl FnOnce(&mut Workspace, &mut Context<Workspace>)) {
    let workspace = t.read(cx).workspace.clone();
    workspace.update(cx, |w, cx| {
        f(w, cx);
        w.navigated(cx);
    });
}

fn emit(t: &Entity<Toolbar>, cx: &mut App, event: ToolbarEvent) {
    t.update(cx, |_, cx| cx.emit(event));
}

/// Hands a menu command to whichever of these actions the focused element takes.
/// The app's editor and text field keep their own, which this crate doesn't link.
fn forward(names: &[&str], window: &mut Window, cx: &mut App) {
    for name in names {
        if let Ok(action) = cx.build_action(name, None)
            && window.is_action_available(action.as_ref(), cx)
        {
            window.dispatch_action(action, cx);
            return;
        }
    }
}

fn select_tab(t: &Entity<Toolbar>, number: usize, cx: &mut App) {
    on_workspace(t, cx, |w, _| w.model.select_tab_number(number));
}

/// A library from the list, by its place in it.
fn go_to_library(t: &Entity<Toolbar>, index: usize, cx: &mut App) {
    let path = libraries::with(|list| list.entries().get(index).map(|entry| PathBuf::from(&entry.path)));
    if let Some(path) = path {
        let workspace = t.read(cx).workspace.clone();
        enter_library(&workspace, &path, cx);
    }
}

/// `enterLibrary`: into the library, in this window. Already here, it just leaves
/// the libraries level; gone, the missing page says so; open in another window,
/// that one comes forward.
pub fn enter_library(workspace: &Entity<Workspace>, path: &Path, cx: &mut App) {
    let wanted = standardized_path(&path.to_string_lossy());
    let here = workspace.read(cx).workspace_path().map(|p| standardized_path(&p.to_string_lossy()));
    if here.as_deref() == Some(wanted.as_str()) {
        workspace.update(cx, |w, cx| {
            w.model.shows_libraries = false;
            cx.notify();
        });
        return;
    }
    if missing_reason_on_disk(&wanted).is_some() {
        workspace.update(cx, |w, cx| {
            w.close_workspace(cx);
            w.store.missing_workspace = Some(path.to_path_buf());
            w.model.shows_libraries = false;
            w.navigated(cx);
        });
        return;
    }
    if shell::bring_forward(path, Some(workspace), cx) {
        return;
    }
    workspace.update(cx, |w, cx| {
        w.open_workspace(path, cx);
        w.model.shows_libraries = false;
        cx.notify();
    });
}

/// The Open Notes Folder panel, then into what was chosen.
pub fn open_folder(workspace: Option<Entity<Workspace>>, cx: &mut App) {
    cx.spawn(async move |cx| {
        // In a task: the panel runs a modal loop, see `native::popup_menu`.
        let Some(path) = native::choose_notes_folder(None) else { return };
        cx.update(|cx| match workspace {
            Some(workspace) => enter_library(&workspace, &path, cx),
            None => shell::open_window(fumi_workspace::WindowStart::Library(path), cx),
        });
    })
    .detach();
}

/// ⇧⌘T, skipping tabs whose note or file is gone.
pub fn reopen_closed_tab(w: &mut Workspace) {
    let notes: HashSet<String> = w.store.notes().iter().map(|note| note.id.clone()).collect();
    let files: HashSet<String> = w.store.files().iter().map(|file| file.id.clone()).collect();
    w.model.reopen_closed_tab(|content| match content {
        Content::Note(id) => notes.contains(id),
        Content::File(id) => files.contains(id),
        Content::Draft | Content::Memos => true,
    });
}

fn toggle_preview(t: &Entity<Toolbar>, cx: &mut App) {
    on_workspace(t, cx, |w, _| w.model.toggle_preview());
    emit(t, cx, ToolbarEvent::PreviewToggled);
}

/// Gives the window's root element every command. Call in root's `render`; root
/// redraws when the workspace notifies, which keeps the greying current.
pub fn register_actions<E: InteractiveElement>(mut el: E, toolbar: &Entity<Toolbar>, cx: &App) -> E {
    let caps = Caps::of(toolbar.read(cx).workspace.read(cx));

    macro_rules! on {
        ($action:ty, $body:expr) => {{
            let t = toolbar.clone();
            let body: fn(&Entity<Toolbar>, &mut Window, &mut App) = $body;
            el = el.on_action(move |_: &$action, window: &mut Window, cx: &mut App| body(&t, window, cx));
        }};
    }
    macro_rules! on_tab_number {
        ($($action:ty => $n:expr),*) => { $(on!($action, |t, _, cx| select_tab(t, $n, cx));)* };
    }
    macro_rules! on_library {
        ($($action:ty => $n:expr),*) => {
            $(if libraries::with(|list| list.entries().len()) > $n {
                on!($action, |t, _, cx| go_to_library(t, $n, cx));
            })*
        };
    }
    macro_rules! forward_to {
        ($action:ty, $($name:literal),*) => { on!($action, |_, window, cx| forward(&[$($name),*], window, cx)); };
    }

    on!(NewNote, |t, _, cx| on_workspace(t, cx, |w, _| w.model.new_tab()));
    on!(NewTab, |t, _, cx| on_workspace(t, cx, |w, _| w.model.new_tab()));
    on!(OpenFolder, |t, _, cx| {
        let workspace = t.read(cx).workspace.clone();
        open_folder(Some(workspace), cx);
    });
    if caps.can_reopen {
        on!(ReopenClosedTab, |t, _, cx| {
            let workspace = t.read(cx).workspace.clone();
            workspace.update(cx, |w, cx| {
                reopen_closed_tab(w);
                w.navigated(cx);
            });
        });
    }
    on!(CloseTab, |t, window, cx| {
        let workspace = t.read(cx).workspace.clone();
        let closed = workspace.update(cx, |w, cx| {
            let closed = w.model.close_active_tab();
            w.navigated(cx);
            closed
        });
        if !closed {
            shell::hide_window(window, cx);
        }
    });
    on!(ExportAllNotes, |t, _, cx| emit(t, cx, ToolbarEvent::ExportAllNotes));
    if caps.any_extension && caps.has_note {
        on!(TogglePreview, |t, _, cx| toggle_preview(t, cx));
    }
    if caps.can_copy {
        on!(CopyForPublish, |t, _, cx| {
            let target = {
                let w = t.read(cx).workspace.read(cx);
                extension_xhs() && publish_target(w.model.selected_note()) == PublishTarget::Xiaohongshu
            };
            emit(t, cx, if target { ToolbarEvent::AddXhsCards } else { ToolbarEvent::CopyForWeChat });
        });
    }
    on!(OpenSettings, |t, _, cx| on_workspace(t, cx, |w, _| w.model.show_settings = true));
    on!(ToggleSidebar, |t, _, cx| on_workspace(t, cx, |w, _| w.model.toggle_sidebar()));
    if caps.can_go_back {
        on!(GoBack, |t, _, cx| on_workspace(t, cx, |w, _| w.model.go_back()));
    }
    if caps.can_go_forward {
        on!(GoForward, |t, _, cx| on_workspace(t, cx, |w, _| w.model.go_forward()));
    }
    on!(SearchNotes, |t, _, cx| on_workspace(t, cx, |w, _| w.model.search_presented = true));
    if caps.library_open {
        on!(ShowLibraries, |t, _, cx| {
            on_workspace(t, cx, |w, _| {
                w.model.sidebar_visible = true;
                w.model.shows_libraries = true;
            });
        });
    }
    on_library!(
        GoToLibrary1 => 0, GoToLibrary2 => 1, GoToLibrary3 => 2, GoToLibrary4 => 3, GoToLibrary5 => 4,
        GoToLibrary6 => 5, GoToLibrary7 => 6, GoToLibrary8 => 7, GoToLibrary9 => 8
    );
    if caps.several_tabs {
        on!(NextTab, |t, _, cx| on_workspace(t, cx, |w, _| w.model.select_tab_offset(1)));
        on!(PreviousTab, |t, _, cx| on_workspace(t, cx, |w, _| w.model.select_tab_offset(-1)));
        on!(TogglePinTab, |t, _, cx| {
            on_workspace(t, cx, |w, _| {
                let id = w.model.active_id().to_string();
                w.model.toggle_pin(&id);
            });
        });
    }
    on_tab_number!(
        SelectTab1 => 1, SelectTab2 => 2, SelectTab3 => 3, SelectTab4 => 4, SelectTab5 => 5,
        SelectTab6 => 6, SelectTab7 => 7, SelectTab8 => 8, SelectTab9 => 9
    );
    on!(Minimize, |_, window, _| window.minimize_window());
    on!(Zoom, |_, window, _| window.zoom_window());

    forward_to!(MenuUndo, "editor::Undo", "text_field::Undo");
    forward_to!(MenuRedo, "editor::Redo", "text_field::Redo");
    forward_to!(MenuCut, "editor::Cut", "text_field::Cut");
    forward_to!(MenuCopy, "editor::Copy", "text_field::Copy");
    forward_to!(MenuPaste, "editor::Paste", "text_field::Paste");
    forward_to!(MenuSelectAll, "editor::SelectAll", "text_field::SelectAll");
    forward_to!(MenuBold, "editor::Bold");
    forward_to!(MenuItalic, "editor::Italic");
    forward_to!(MenuInsertLink, "editor::InsertLink");
    forward_to!(MenuToggleTodo, "editor::ToggleTaskLine");
    el
}

/// Registers what works with no window at all.
pub(crate) fn register_global_actions(cx: &mut App) {
    cx.on_action(|_: &NewWindow, cx| shell::open_window(fumi_workspace::WindowStart::Empty, cx));
    cx.on_action(|_: &OpenFolder, cx| open_folder(None, cx));
    cx.on_action(|_: &NewMemo, cx| shell::new_memo(cx));
    cx.on_action(|_: &About, _| mac::about_panel());
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Quit, cx| shell::quit(cx));
    cx.on_action(|_: &BringAllToFront, _| mac::bring_all_to_front());
    cx.on_action(|_: &ShowHelp, _| mac::show_help());
}

/// Keeps keyboard focus inside the window's root, whose element carries the
/// commands: with the focused element gone (its tab closed), GPUI would otherwise
/// dispatch from above root, and every shortcut and menu item would go dead. Call
/// in root's `render` with root's own focus handle.
pub fn keep_focus(root: &FocusHandle, window: &mut Window) {
    let root = root.clone();
    window.on_next_frame(move |window, cx| {
        if !root.contains_focused(window, cx) {
            window.focus(&root, cx);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action_of(key: &'static str) -> Option<String> {
        key_bindings()
            .into_iter()
            .find(|b| {
                let typed = gpui::Keystroke::parse(key).unwrap();
                b.match_keystrokes(std::slice::from_ref(&typed)) == Some(false)
            })
            .map(|b| b.action().name().to_string())
    }

    #[core::prelude::v1::test]
    fn shortcuts_match_the_checklist() {
        for (key, action) in [
            ("cmd-,", "fumi::OpenSettings"),
            ("cmd-n", "fumi::NewNote"),
            ("cmd-t", "fumi::NewTab"),
            ("cmd-shift-n", "fumi::NewWindow"),
            ("cmd-o", "fumi::OpenFolder"),
            ("cmd-shift-t", "fumi::ReopenClosedTab"),
            ("cmd-w", "fumi::CloseTab"),
            ("alt-cmd-s", "fumi::ToggleSidebar"),
            ("cmd-[", "fumi::GoBack"),
            ("cmd-]", "fumi::GoForward"),
            ("cmd-shift-f", "fumi::SearchNotes"),
            ("ctrl-cmd-1", "fumi::GoToLibrary1"),
            ("ctrl-cmd-9", "fumi::GoToLibrary9"),
            ("ctrl-tab", "fumi::NextTab"),
            ("ctrl-shift-tab", "fumi::PreviousTab"),
            ("alt-cmd-p", "fumi::TogglePinTab"),
            ("ctrl-cmd-p", "fumi::TogglePreview"),
            ("alt-cmd-c", "fumi::CopyForPublish"),
            ("cmd-9", "fumi::SelectTab9"),
            ("cmd-b", "fumi::MenuBold"),
            ("cmd-l", "fumi::MenuToggleTodo"),
        ] {
            assert_eq!(action_of(key).as_deref(), Some(action), "{key}");
        }
    }

    #[core::prelude::v1::test]
    fn copy_follows_the_side() {
        let caps = |has_note, wechat, cards, xhs| Caps::compute(has_note, false, false, false, false, true, wechat, cards, xhs);
        assert!(!caps(false, true, false, false).can_copy, "no note");
        assert!(caps(true, true, false, false).can_copy, "WeChat on");
        assert!(!caps(true, false, false, true).can_copy, "only Xiaohongshu, side not on it");
        assert!(caps(true, false, true, true).can_copy, "Xiaohongshu's side");
        assert!(!caps(true, false, false, false).any_extension);
    }
}
