//! Fumi's logic with no UI, ported from the Swift app's `Services/` and pure
//! view helpers. Nothing here may depend on GPUI, AppKit or any window.
//!
//! Text offsets are byte offsets into UTF-8 strings unless a function says
//! otherwise. See PORTING.md for how the Swift code and tests are carried over.

pub mod doc;
pub mod markdown;
pub mod note;

// Group A: note text.
pub mod frontmatter;
pub mod inline_link;
pub mod library_move;
pub mod link_rewrite;
pub mod note_card_summary;
pub mod outline_preview;
pub mod tags;
pub mod title;
pub mod wiki_link;

// Group B: files, sync and libraries.
pub mod attachment_import;
pub mod external_sync;
pub mod library_list;
pub mod merge3;
pub mod note_store;
pub mod note_tree;
pub mod tabs;

// Group C: memos, search and decks.
pub mod fuzzy_search;
pub mod memo_format;
pub mod memo_markdown;
pub mod sidebar_width;
pub mod slash_command;
pub mod xhs_deck;
