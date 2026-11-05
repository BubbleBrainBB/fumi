//! Saving out of the app: the PDF's panel and failure alert, Export All Notes. A
//! port of `PDFExporter.export` and `NoteExporter`.

use crate::mac;
use crate::pdf;
use fumi_core::note::Note;
use fumi_core::wiki_link::WikiLinkIndex;
use gpui::{AppContext, AsyncApp};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// A file name that is safe to write: the note's name without the characters a
/// file name may not hold.
pub fn pdf_file_name(title: &str) -> String {
    let cleaned = title.replace(['/', ':'], "-");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() { "Note".to_string() } else { cleaned.to_string() }
}

/// Asks where to put the PDF and writes it there. Nothing opens afterwards.
pub async fn export_pdf(note: &Note, markdown: String, library: Option<PathBuf>, links: Rc<WikiLinkIndex>, cx: &AsyncApp) {
    let title = note.filename();
    let folder = note.path.parent().map(Path::to_path_buf);
    let name = format!("{}.pdf", pdf_file_name(&title));
    let target = mac::run_save_panel(mac::SavePanel {
        title: Some("Export as PDF"),
        prompt: Some("Export"),
        message: None,
        name: &name,
        directory: folder.as_deref(),
        content_type: Some("com.adobe.pdf"),
    });
    let Some(target) = target else { return };

    // The sheet is drawn from the note as it is on screen, so the images still on
    // their way are fetched first.
    let wanted = pdf::remote_images(&markdown);
    if !wanted.is_empty() {
        cx.background_spawn(async move { pdf::fetch_remote_images(&wanted) }).await;
    }
    let config = pdf::Config::from_settings(title);
    let rendered = pdf::render(&markdown, &config, folder.as_deref(), library.as_deref(), Some(links));
    let Some(data) = rendered else {
        failure("The note could not be laid out for printing.");
        return;
    };
    if let Err(error) = write_atomically(&target, &data) {
        failure(&error.to_string());
    }
}

fn failure(message: &str) {
    mac::warning_alert("The PDF could not be exported", message);
}

fn write_atomically(target: &Path, data: &[u8]) -> std::io::Result<()> {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temporary = target.with_file_name(format!(".{name}.fumi-tmp"));
    std::fs::write(&temporary, data)?;
    std::fs::rename(&temporary, target).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// One flat `.md` per note, named from its title or file name.
pub fn export_name(title: &str, filename: &str) -> String {
    let title = if title.is_empty() { filename } else { title };
    let safe: String = title.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let safe = crate::wechat::trim(&safe);
    format!("{}.md", if safe.is_empty() { "Untitled" } else { safe })
}

/// Same names overwrite each other; failures are silent, as in Swift.
pub fn write_all_notes(notes: &[(String, String, String)], folder: &Path) {
    for (title, filename, content) in notes {
        let _ = write_atomically(&folder.join(export_name(title, filename)), content.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_names() {
        assert_eq!(pdf_file_name("a/b:c"), "a-b-c");
        assert_eq!(pdf_file_name("   "), "Note");
        assert_eq!(pdf_file_name(""), "Note");
    }

    #[test]
    fn export_names() {
        assert_eq!(export_name("My: Note?", "x"), "My- Note-.md");
        assert_eq!(export_name("", "file"), "file.md");
        assert_eq!(export_name("  ", "file"), "Untitled.md");
        assert_eq!(export_name("", ""), "Untitled.md");
    }

    #[test]
    fn notes_write_flat_and_overwrite() {
        let dir = std::env::temp_dir().join(format!("fumi-publish-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let notes = vec![
            ("A".to_string(), "a".to_string(), "one".to_string()),
            ("A".to_string(), "b".to_string(), "two".to_string()),
            ("B/C".to_string(), "c".to_string(), "three".to_string()),
        ];
        write_all_notes(&notes, &dir);
        assert_eq!(std::fs::read_to_string(dir.join("A.md")).unwrap(), "two");
        assert_eq!(std::fs::read_to_string(dir.join("B-C.md")).unwrap(), "three");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
