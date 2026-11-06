//! What every Fumi screen shares: colors, preferences, AppKit plumbing.
#![allow(unexpected_cfgs)]

pub mod anim;
#[allow(deprecated)]
pub mod native;
#[allow(deprecated)]
pub mod settings;
pub mod text_field;
pub mod scroller;
pub mod theme;
pub mod tip;
pub mod window_frame;
pub mod fonts;
pub mod fsevents;
pub mod icons;

