//! The Memos page and quick capture: `PARITY.md` section E, from the Swift app's
//! `MemosView.swift`, `MemoText.swift` and `QuickCapture.swift`.
#![allow(unexpected_cfgs, deprecated)]

pub mod capture;
pub mod field;
pub mod inline;
pub mod logic;
pub mod rich;
pub mod typo;
mod calendar;
mod list_row;
mod view;

pub use capture::{NewMemo, QuickCapture};
pub use view::MemosView;

/// Key bindings for the page and the capture panel. Call once at launch, with
/// `fumi_ui::text_field::bind_keys`.
pub fn bind_keys(cx: &mut gpui::App) {
    view::bind_keys(cx);
    capture::bind_keys(cx);
}
