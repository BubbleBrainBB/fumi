//! What the toolbar's native menus offer, as data: rows and the command each runs.

use fumi_core::tabs::NoteTab;
use fumi_ui::native::MenuRow;

/// A pop-up menu's rows, each with what choosing it does.
pub struct MenuSpec<C> {
    pub rows: Vec<MenuRow>,
    pub commands: Vec<C>,
}

impl<C> Default for MenuSpec<C> {
    fn default() -> Self {
        MenuSpec { rows: Vec::new(), commands: Vec::new() }
    }
}

impl<C> MenuSpec<C> {
    pub fn item(&mut self, title: impl Into<String>, command: C) {
        self.rows.push(MenuRow::item(title));
        self.commands.push(command);
    }

    pub fn item_if(&mut self, title: impl Into<String>, enabled: bool, command: C) {
        let title = title.into();
        self.rows.push(if enabled { MenuRow::item(title) } else { MenuRow::disabled(title) });
        self.commands.push(command);
    }

    pub fn destructive(&mut self, title: impl Into<String>, command: C) {
        self.rows.push(MenuRow::destructive(title));
        self.commands.push(command);
    }

    pub fn choice(&mut self, title: impl Into<String>, checked: bool, command: C) {
        self.rows.push(MenuRow::Choice { title: title.into(), checked, image: None });
        self.commands.push(command);
    }

    pub fn separator(&mut self) {
        self.rows.push(MenuRow::Separator);
    }

    /// Takes the command for a chosen index, which counts items only.
    pub fn take(mut self, index: usize) -> Option<C> {
        (index < self.commands.len()).then(|| self.commands.swap_remove(index))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabCommand {
    TogglePin(String),
    Close(String),
    CloseOthers(String),
    CloseRight(String),
    RevealInSidebar(String),
    RevealInFinder(std::path::PathBuf),
}

/// What right-clicking a tab offers.
pub fn tab_menu(tab: &NoteTab, tabs: &[NoteTab], file: Option<&std::path::Path>) -> MenuSpec<TabCommand> {
    let mut menu = MenuSpec::default();
    let id = tab.id.clone();
    menu.item(if tab.is_pinned { "Unpin Tab" } else { "Pin Tab" }, TabCommand::TogglePin(id.clone()));
    if tabs.len() > 1 {
        menu.separator();
        menu.item_if("Close Tab", !tab.is_pinned, TabCommand::Close(id.clone()));
        let others_open = !tabs.iter().all(|other| other.id == tab.id || other.is_pinned);
        menu.item_if("Close Other Tabs", others_open, TabCommand::CloseOthers(id.clone()));
        let not_last = tabs.last().is_some_and(|last| last.id != tab.id);
        menu.item_if("Close Tabs to the Right", not_last, TabCommand::CloseRight(id.clone()));
    }
    if let Some(path) = file {
        menu.separator();
        menu.item("Reveal in Sidebar", TabCommand::RevealInSidebar(id));
        menu.item("Reveal in Finder", TabCommand::RevealInFinder(path.to_path_buf()));
    }
    menu
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteCommand {
    TogglePin,
    RevealInFinder,
    ExportPdf,
    TogglePreview,
    CopyForWeChat,
    AddXhsCards,
    Delete,
}

/// The More menu on a note.
pub fn note_menu(pinned: bool, wechat: bool, xhs: bool, preview_open: bool) -> MenuSpec<NoteCommand> {
    let mut menu = MenuSpec::default();
    menu.item(if pinned { "Unpin" } else { "Pin" }, NoteCommand::TogglePin);
    menu.item("Reveal in Finder", NoteCommand::RevealInFinder);
    menu.item("Export as PDF…", NoteCommand::ExportPdf);
    if wechat || xhs {
        let name = crate::labels::preview_name(wechat, xhs);
        menu.item(if preview_open { format!("Hide {name}") } else { name.to_string() }, NoteCommand::TogglePreview);
    }
    if wechat {
        menu.item("Copy for WeChat", NoteCommand::CopyForWeChat);
    }
    if xhs {
        menu.item("Add Xiaohongshu Cards to Photos", NoteCommand::AddXhsCards);
    }
    menu.separator();
    menu.destructive("Delete", NoteCommand::Delete);
    menu
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileCommand {
    CopyMarkdownLink,
    OpenInDefaultApp,
    RevealInFinder,
    MoveToTrash,
}

/// The More menu on a file preview.
pub fn file_menu() -> MenuSpec<FileCommand> {
    let mut menu = MenuSpec::default();
    menu.item("Copy Markdown Link", FileCommand::CopyMarkdownLink);
    menu.item("Open in Default App", FileCommand::OpenInDefaultApp);
    menu.item("Reveal in Finder", FileCommand::RevealInFinder);
    menu.separator();
    menu.destructive("Move to Trash", FileCommand::MoveToTrash);
    menu
}

#[cfg(test)]
mod tests {
    use super::*;
    use fumi_core::tabs::Content;
    use std::path::Path;

    fn titles<C>(menu: &MenuSpec<C>) -> Vec<(String, bool)> {
        menu.rows
            .iter()
            .map(|row| match row {
                MenuRow::Item { title, enabled, .. } => (title.clone(), *enabled),
                MenuRow::Separator => ("---".to_string(), false),
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    fn tab(id: &str, pinned: bool) -> NoteTab {
        NoteTab { id: id.to_string(), content: Content::Note(format!("/{id}.md")), is_pinned: pinned }
    }

    fn row(title: &str, enabled: bool) -> (String, bool) {
        (title.to_string(), enabled)
    }

    #[test]
    fn a_lone_tab_offers_only_its_pin() {
        let only = tab("a", false);
        let menu = tab_menu(&only, std::slice::from_ref(&only), None);
        assert_eq!(titles(&menu), [row("Pin Tab", true)]);
    }

    #[test]
    fn several_tabs_offer_closing() {
        let tabs = [tab("a", true), tab("b", false), tab("c", false)];
        let menu = tab_menu(&tabs[1], &tabs, Some(Path::new("/b.md")));
        assert_eq!(
            titles(&menu),
            [
                row("Pin Tab", true),
                row("---", false),
                row("Close Tab", true),
                row("Close Other Tabs", true),
                row("Close Tabs to the Right", true),
                row("---", false),
                row("Reveal in Sidebar", true),
                row("Reveal in Finder", true),
            ]
        );
        // Index counts items, not separators.
        assert_eq!(menu.take(5), Some(TabCommand::RevealInFinder("/b.md".into())));
    }

    #[test]
    fn what_cannot_be_closed_is_disabled() {
        let tabs = [tab("a", true), tab("b", true)];
        let menu = tab_menu(&tabs[1], &tabs, None);
        assert_eq!(
            titles(&menu),
            [
                row("Unpin Tab", true),
                row("---", false),
                row("Close Tab", false),
                row("Close Other Tabs", false),
                row("Close Tabs to the Right", false),
            ]
        );
    }

    #[test]
    fn the_note_menu_follows_the_extensions() {
        let plain: Vec<_> = titles(&note_menu(false, false, false, false));
        assert_eq!(plain.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["Pin", "Reveal in Finder", "Export as PDF…", "---", "Delete"]);

        let menu = note_menu(true, true, true, true);
        let names: Vec<String> = menu
            .rows
            .iter()
            .map(|row| match row {
                MenuRow::Item { title, .. } => title.clone(),
                _ => "---".to_string(),
            })
            .collect();
        assert_eq!(
            names,
            [
                "Unpin",
                "Reveal in Finder",
                "Export as PDF…",
                "Hide Publish Preview",
                "Copy for WeChat",
                "Add Xiaohongshu Cards to Photos",
                "---",
                "Delete"
            ]
        );
        assert!(matches!(menu.rows.last(), Some(MenuRow::Item { destructive: true, .. })));
        assert_eq!(menu.take(6), Some(NoteCommand::Delete));
    }

    #[test]
    fn the_file_menu() {
        let menu = file_menu();
        assert_eq!(menu.commands.len(), 4);
        assert_eq!(menu.take(3), Some(FileCommand::MoveToTrash));
    }
}
