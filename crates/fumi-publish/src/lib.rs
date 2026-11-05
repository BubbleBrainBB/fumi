//! Publishing: the WeChat article and Xiaohongshu cards beside the page, what
//! they put on the clipboard and in Photos, PDF and Markdown export. See PARITY.md,
//! G and C13.
#![allow(unexpected_cfgs, deprecated)]

pub mod cards;
pub mod export;
mod jsc;
pub mod mac;
pub mod pane;
pub mod pdf;
pub mod web;
pub mod wechat;
pub mod xhs;

pub use pane::{LiveText, PublishPane, WIDTH, beside_page};

/// The crate binds no keys of its own: ⌃⌘P and ⌥⌘C are the toolbar's, which sends
/// them on as events. The text fields' keys are `fumi_ui::text_field::bind_keys`.
pub fn bind_keys(_cx: &mut gpui::App) {}
