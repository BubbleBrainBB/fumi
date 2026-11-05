//! The left column: two levels side by side, every library and inside one, with
//! its toolbar button and its divider. Port of `Views/Sidebar/*`. See PARITY.md,
//! C5 to C10.
//!
//! Root puts the view in its row and gives it the full height; the view sizes its
//! own width, including 0 while collapsed. See [`SidebarView`].

mod chrome;
mod libraries_level;
mod library_level;
pub mod logic;
mod view;

pub use view::{SidebarEvent, SidebarView, TreeDrag, wire_actions};

use gpui::{App, KeyBinding, actions};

actions!(
    fumi_sidebar,
    [
        /// ⌘↑: back to the libraries, unless a text view has focus.
        BackToLibraries,
        /// ⌥⌘S.
        ToggleSidebar,
        /// The menu command: the Libraries level, showing the sidebar first.
        ShowLibraries,
        /// ⌘O: the Open Notes Folder panel.
        OpenFolder,
    ]
);

/// The editor and text fields bind ⌘↑ themselves, deeper in the context stack, so
/// those keep it, as `LibraryBackGestures` leaves it to the first responder.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-up", BackToLibraries, None),
        KeyBinding::new("cmd-alt-s", ToggleSidebar, None),
        KeyBinding::new("cmd-o", OpenFolder, None),
    ]);
}
