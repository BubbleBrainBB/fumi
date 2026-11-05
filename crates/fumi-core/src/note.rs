//! A note and the other files in a library. Port of `Models/FileNote.swift`.
//!
//! Dates are seconds since the Unix epoch, so this crate needs no clock type
//! in its data model.

use std::path::PathBuf;

/// A Markdown file in a library. `id` is its path, as in the Swift app.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: f64,
    pub updated_at: f64,
    pub pinned: bool,
    /// The folder relative to the library root; `"/"` at the root.
    pub relative_path: String,
}

impl Note {
    /// The file name without its extension.
    pub fn filename(&self) -> String {
        self.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

/// What a non-note file in a library is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    Image,
    Video,
    Pdf,
}

/// An image, video or PDF in a library, shown beside the notes of its folder.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryFile {
    pub id: String,
    pub path: PathBuf,
    pub kind: FileKind,
    pub updated_at: f64,
    pub relative_path: String,
}

impl LibraryFile {
    /// The file name with its extension (`cover.jpg`).
    pub fn name(&self) -> String {
        self.path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// The kind of file a path names, by extension; `None` for anything else.
    pub fn kind_of(path: &std::path::Path) -> Option<FileKind> {
        let ext = path.extension()?.to_string_lossy().to_lowercase();
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "tiff" | "tif" | "bmp" | "svg" => Some(FileKind::Image),
            "mp4" | "mov" | "m4v" | "webm" => Some(FileKind::Video),
            "pdf" => Some(FileKind::Pdf),
            _ => None,
        }
    }
}
