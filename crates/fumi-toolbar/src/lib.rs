//! The note toolbar, the tab strip, the menu bar and what
//! happens when windows close. See PARITY.md, C4, C11-C15 and H.
#![allow(unexpected_cfgs, deprecated)]

pub mod actions;
pub mod alerts;
pub mod labels;
pub mod layout;
mod mac;
pub mod menu_rows;
pub mod menus;
pub mod shell;
mod tab_strip;
pub mod toolbar;

pub use actions::{bind_keys, keep_focus, register_actions};
pub use shell::{AppHooks, handle_dock_reopen, install, open_first_window};
pub use toolbar::{LiveText, Toolbar, ToolbarEvent};
