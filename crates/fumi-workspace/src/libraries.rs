//! The list of libraries, shared by every window. `LibraryList.shared` of
//! `App/Libraries.swift`, kept in `NSUserDefaults` under the Swift app's key.

use fumi_core::library_list::{KEY, LibraryList, ListStorage};
use fumi_ui::settings;
use std::cell::RefCell;
use std::path::Path;

/// The list's blob in `NSUserDefaults`. A run under the `FUMI_WORKSPACE` dev
/// override keeps its changes in memory, so tests never touch the saved list.
pub struct Defaults;

impl ListStorage for Defaults {
    fn load(&self) -> Option<Vec<u8>> {
        settings::get_data(KEY)
    }

    fn save(&mut self, data: &[u8]) {
        if std::env::var_os("FUMI_WORKSPACE").is_none() {
            settings::set_data(KEY, data);
        }
    }
}

thread_local! {
    static LIST: RefCell<LibraryList<Defaults>> = RefCell::new(LibraryList::new(Defaults, || {
        // Starts from the welcome page's recent folders, the ones still there.
        settings::recent_workspaces().into_iter().filter(|path| Path::new(path).exists()).collect()
    }));
}

/// Reads or changes the list. Call `cx.refresh_windows()` after a change, since
/// every window shows it.
pub fn with<R>(f: impl FnOnce(&mut LibraryList<Defaults>) -> R) -> R {
    LIST.with(|list| f(&mut list.borrow_mut()))
}
