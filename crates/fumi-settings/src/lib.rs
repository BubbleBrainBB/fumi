//! The Settings sheet, quick search, the welcome and empty-library pages and file
//! previews. Everything here reads and changes a window's `Workspace`.
#![allow(unexpected_cfgs, deprecated)]

pub mod anim;
pub mod controls;
pub mod file_preview;
pub mod font_picker;
#[allow(deprecated)]
pub mod mac;
pub mod scroller;
pub mod search;
pub mod sheet;
pub mod welcome;

use gpui::*;

actions!(
    fumi_settings,
    [
        /// ⇧⌘F: the root opens the search panel with `search::present`.
        ShowSearch,
        /// ⌘,: the root opens the sheet with `sheet::present`.
        ShowSettings,
        /// Esc or ⌘. inside the sheet.
        CloseSettings,
        /// ↩ on the welcome and missing-library pages.
        WelcomeDefault,
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-shift-f", ShowSearch, None),
        KeyBinding::new("cmd-,", ShowSettings, None),
        KeyBinding::new("escape", CloseSettings, Some("FumiSettings")),
        KeyBinding::new("cmd-.", CloseSettings, Some("FumiSettings")),
        KeyBinding::new("enter", WelcomeDefault, Some("FumiWelcome")),
    ]);
}
