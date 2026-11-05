//! The app across its windows: what a new window opens on, closing as hiding, the
//! Dock, the menu bar's upkeep. Port of `WindowCloseBehavior.swift`.

use crate::actions::register_global_actions;
use crate::{mac, menus};
use fumi_ui::{native, settings};
use fumi_workspace::{WindowStart, Workspace};
use gpui::*;
use std::path::Path;
use std::rc::Rc;

/// What root gives the app shell at launch.
pub struct AppHooks {
    /// Opens a window on this start: `open_window` with a root using
    /// `Workspace::starting(start, cx)`.
    pub open_window: Rc<dyn Fn(WindowStart, &mut App)>,
    /// File › New Memo: the quick capture panel.
    pub new_memo: Rc<dyn Fn(&mut App)>,
}

struct Shell {
    hooks: AppHooks,
    windows: Vec<(AnyWindowHandle, WeakEntity<Workspace>)>,
    hidden: Vec<AnyWindowHandle>,
    /// The first window of a launch restores the last library, once.
    restored: bool,
    quitting: bool,
    menus: Option<menus::MenuState>,
}

impl Global for Shell {}

/// Sets up the menu bar, the shortcuts and the window behavior. Call once, first
/// thing in `Application::run`, before opening a window.
pub fn install(cx: &mut App, hooks: AppHooks) {
    // macOS doesn't resurrect windows: launch opens one.
    settings::set_bool("NSQuitAlwaysKeepsWindows", false);
    crate::actions::bind_keys(cx);
    fumi_ui::text_field::bind_keys(cx);
    cx.set_global(Shell { hooks, windows: Vec::new(), hidden: Vec::new(), restored: false, quitting: false, menus: None });
    register_global_actions(cx);
    cx.on_app_quit(|cx| {
        cx.global_mut::<Shell>().quitting = true;
        async {}
    })
    .detach();
    sync_menus(menus::MenuState::default(), cx);
}

/// The launch's one window.
pub fn open_first_window(cx: &mut App) {
    let start = if cx.global::<Shell>().restored { WindowStart::Empty } else { WindowStart::Restore };
    cx.global_mut::<Shell>().restored = true;
    open_window(start, cx);
}

pub fn open_window(start: WindowStart, cx: &mut App) {
    let open = cx.global::<Shell>().hooks.open_window.clone();
    open(start, cx);
}

pub(crate) fn new_memo(cx: &mut App) {
    let show = cx.global::<Shell>().hooks.new_memo.clone();
    show(cx);
}

pub(crate) fn quit(cx: &mut App) {
    cx.global_mut::<Shell>().quitting = true;
    cx.quit();
}

/// Takes a new window under the shell's care: the red button hides it, and menus
/// and Dock can find it. `Toolbar::new` calls this.
pub(crate) fn attach(window: &mut Window, workspace: &Entity<Workspace>, cx: &mut App) {
    mac::set_not_restorable(native::ns_window_of(window));
    if cx.has_global::<Shell>() {
        let handle = window.window_handle();
        let weak = workspace.downgrade();
        cx.global_mut::<Shell>().windows.push((handle, weak));
    }
    window.on_window_should_close(cx, |window, cx| {
        if !cx.has_global::<Shell>() || cx.global::<Shell>().quitting {
            return true;
        }
        hide_window(window, cx);
        false
    });
}

/// A closed window is really hidden: the Dock brings it back at once, without
/// rescanning.
pub fn hide_window(window: &mut Window, cx: &mut App) {
    if cx.has_global::<Shell>() {
        let handle = window.window_handle();
        let shell = cx.global_mut::<Shell>();
        if !shell.hidden.contains(&handle) {
            shell.hidden.push(handle);
        }
    }
    mac::order_out(native::ns_window_of(window));
}

/// The Dock icon clicked with no visible window: hidden windows come back.
/// Wire to `Application::on_reopen`.
pub fn handle_dock_reopen(cx: &mut App) {
    let hidden = std::mem::take(&mut cx.global_mut::<Shell>().hidden);
    let mut shown = false;
    for handle in hidden {
        shown |= handle.update(cx, |_, window, _| window.activate_window()).is_ok();
    }
    if !shown {
        // Nothing to bring back, only possible if every window was really closed.
        open_window(WindowStart::Empty, cx);
    }
    cx.activate(true);
}

/// Brings forward the window showing `path`, hidden or not, other than the one
/// with `except`. False when there is none.
pub fn bring_forward(path: &Path, except: Option<&Entity<Workspace>>, cx: &mut App) -> bool {
    if !cx.has_global::<Shell>() {
        return false;
    }
    let windows = cx.global::<Shell>().windows.clone();
    let wanted = fumi_core::library_list::standardized_path(&path.to_string_lossy());
    for (handle, weak) in windows {
        let Some(workspace) = weak.upgrade() else { continue };
        if except.is_some_and(|other| *other == workspace) {
            continue;
        }
        let shows = workspace
            .read(cx)
            .workspace_path()
            .is_some_and(|open| fumi_core::library_list::standardized_path(&open.to_string_lossy()) == wanted);
        if shows && handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            cx.global_mut::<Shell>().hidden.retain(|hidden| *hidden != handle);
            cx.activate(true);
            return true;
        }
    }
    false
}

/// Rebuilds the menu bar when what it says changed: `Pin Tab` to `Unpin Tab`, the
/// extensions, the libraries. Cheap otherwise.
pub(crate) fn sync_menus(state: menus::MenuState, cx: &mut App) {
    if !cx.has_global::<Shell>() {
        return;
    }
    let shell = cx.global_mut::<Shell>();
    if shell.menus.as_ref() == Some(&state) {
        return;
    }
    shell.menus = Some(state.clone());
    cx.set_menus(menus::build(&state));
    mac::polish_main_menu();
}
