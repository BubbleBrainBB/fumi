//! The menu bar. What SwiftUI gives the Swift app for free, the app, Edit, Window
//! and Help menus, is spelled out here, with Fumi's additions where they stand.
//! `mac::polish_main_menu` adds the Edit rows that are only AppKit selectors; AppKit
//! itself adds Fill, Center and Move & Resize to the menu it's told is Window.

use crate::actions::*;
use gpui::{Menu, MenuItem, OsAction, SystemMenuType};

/// What the bar's words depend on. Compared on every workspace change; the bar is
/// rebuilt only when this differs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuState {
    pub any_extension: bool,
    pub preview_open: bool,
    /// ⌥⌘C adds Xiaohongshu cards, following the preview's side.
    pub exports_cards: bool,
    pub tab_pinned: bool,
    /// The first nine libraries, in list order.
    pub libraries: Vec<String>,
}

fn item(name: &str, action: impl gpui::Action) -> MenuItem {
    MenuItem::action(name.to_string(), action)
}

fn library_item(index: usize, name: &str) -> MenuItem {
    let name = name.to_string();
    match index {
        0 => MenuItem::action(name, GoToLibrary1),
        1 => MenuItem::action(name, GoToLibrary2),
        2 => MenuItem::action(name, GoToLibrary3),
        3 => MenuItem::action(name, GoToLibrary4),
        4 => MenuItem::action(name, GoToLibrary5),
        5 => MenuItem::action(name, GoToLibrary6),
        6 => MenuItem::action(name, GoToLibrary7),
        7 => MenuItem::action(name, GoToLibrary8),
        _ => MenuItem::action(name, GoToLibrary9),
    }
}

/// The bar for `state`.
pub fn build(state: &MenuState) -> Vec<Menu> {
    let app = Menu::new("Fumi").items([
        item("About Fumi", About),
        MenuItem::separator(),
        item("Settings…", OpenSettings),
        MenuItem::separator(),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        item("Hide Fumi", HideApp),
        item("Hide Others", HideOthers),
        item("Show All", ShowAll),
        MenuItem::separator(),
        item("Quit Fumi", Quit),
    ]);

    let mut file = vec![
        item("New Note", NewNote),
        item("New Tab", NewTab),
        item("New Memo", NewMemo),
        item("New Window", NewWindow),
        item("Open Folder…", OpenFolder),
        MenuItem::separator(),
        item("Reopen Closed Tab", ReopenClosedTab),
        MenuItem::separator(),
        item("Close", CloseTab),
        MenuItem::separator(),
        item("Export All Notes…", ExportAllNotes),
    ];
    if state.any_extension {
        file.push(MenuItem::separator());
        file.push(item(if state.preview_open { "Hide Preview" } else { "Show Preview" }, TogglePreview));
        file.push(item(
            if state.exports_cards { "Add Xiaohongshu Cards to Photos" } else { "Copy for WeChat" },
            CopyForPublish,
        ));
    }

    let edit = Menu::new("Edit").items([
        MenuItem::os_action("Undo", MenuUndo, OsAction::Undo),
        MenuItem::os_action("Redo", MenuRedo, OsAction::Redo),
        MenuItem::separator(),
        MenuItem::os_action("Cut", MenuCut, OsAction::Cut),
        MenuItem::os_action("Copy", MenuCopy, OsAction::Copy),
        MenuItem::os_action("Paste", MenuPaste, OsAction::Paste),
        MenuItem::os_action("Select All", MenuSelectAll, OsAction::SelectAll),
    ]);

    let libraries: Vec<MenuItem> =
        state.libraries.iter().take(9).enumerate().map(|(index, name)| library_item(index, name)).collect();
    let view = Menu::new("View").items([
        item("Toggle Sidebar", ToggleSidebar),
        item("Back", GoBack),
        item("Forward", GoForward),
        item("Search Notes", SearchNotes),
        MenuItem::separator(),
        item("Show Libraries", ShowLibraries),
        MenuItem::submenu(Menu::new("Go to Library").items(libraries)),
    ]);

    let format = Menu::new("Format").items([
        item("Bold", MenuBold),
        item("Italic", MenuItalic),
        item("Insert Link", MenuInsertLink),
        item("Toggle Todo", MenuToggleTodo),
    ]);

    let window = Menu::new("Window").items([
        item("Minimize", Minimize),
        item("Zoom", Zoom),
        MenuItem::separator(),
        item("Show Next Tab", NextTab),
        item("Show Previous Tab", PreviousTab),
        item(if state.tab_pinned { "Unpin Tab" } else { "Pin Tab" }, TogglePinTab),
        MenuItem::separator(),
        item("Bring All to Front", BringAllToFront),
    ]);

    let help = Menu::new("Help").items([item("Fumi Help", ShowHelp)]);

    vec![app, Menu::new("File").items(file), edit, view, format, window, help]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A menu as "title=action" lines, "-" for separators.
    fn lines(menu: &Menu) -> Vec<String> {
        menu.items
            .iter()
            .map(|item| match item {
                MenuItem::Separator => "-".to_string(),
                MenuItem::Action { name, action, .. } => format!("{name}={}", action.name()),
                MenuItem::Submenu(sub) => format!("{}>{}", sub.name, sub.items.len()),
                MenuItem::SystemMenu(sys) => format!("{}*", sys.name),
            })
            .collect()
    }

    fn menu<'a>(bar: &'a [Menu], name: &str) -> &'a Menu {
        bar.iter().find(|m| m.name.as_ref() == name).expect(name)
    }

    #[core::prelude::v1::test]
    fn the_bar_in_order() {
        let bar = build(&MenuState::default());
        let names: Vec<&str> = bar.iter().map(|m| m.name.as_ref()).collect();
        assert_eq!(names, ["Fumi", "File", "Edit", "View", "Format", "Window", "Help"]);
    }

    #[core::prelude::v1::test]
    fn the_file_menu_without_extensions() {
        let bar = build(&MenuState::default());
        assert_eq!(
            lines(menu(&bar, "File")),
            [
                "New Note=fumi::NewNote",
                "New Tab=fumi::NewTab",
                "New Memo=fumi::NewMemo",
                "New Window=fumi::NewWindow",
                "Open Folder…=fumi::OpenFolder",
                "-",
                "Reopen Closed Tab=fumi::ReopenClosedTab",
                "-",
                "Close=fumi::CloseTab",
                "-",
                "Export All Notes…=fumi::ExportAllNotes",
            ]
        );
    }

    #[core::prelude::v1::test]
    fn publish_items_follow_state() {
        let bar = build(&MenuState { any_extension: true, ..Default::default() });
        let file = lines(menu(&bar, "File"));
        assert_eq!(&file[11..], ["-", "Show Preview=fumi::TogglePreview", "Copy for WeChat=fumi::CopyForPublish"]);

        let bar = build(&MenuState { any_extension: true, preview_open: true, exports_cards: true, ..Default::default() });
        let file = lines(menu(&bar, "File"));
        assert_eq!(
            &file[12..],
            ["Hide Preview=fumi::TogglePreview", "Add Xiaohongshu Cards to Photos=fumi::CopyForPublish"]
        );
    }

    #[core::prelude::v1::test]
    fn view_and_window() {
        let libraries = (1..=12).map(|n| format!("Lib {n}")).collect();
        let bar = build(&MenuState { tab_pinned: true, libraries, ..Default::default() });
        let view = lines(menu(&bar, "View"));
        assert_eq!(view[0], "Toggle Sidebar=fumi::ToggleSidebar");
        assert_eq!(view[6], "Go to Library>9", "only the first nine");
        assert!(lines(menu(&bar, "Window")).contains(&"Unpin Tab=fumi::TogglePinTab".to_string()));
    }

    #[core::prelude::v1::test]
    fn format_commands() {
        let bar = build(&MenuState::default());
        assert_eq!(
            lines(menu(&bar, "Format")),
            ["Bold=fumi::MenuBold", "Italic=fumi::MenuItalic", "Insert Link=fumi::MenuInsertLink", "Toggle Todo=fumi::MenuToggleTodo"]
        );
    }
}
