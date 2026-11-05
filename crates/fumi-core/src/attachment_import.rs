//! Images and videos pasted or dropped into a note: copied into the library's
//! `attachments/` folder and linked with `![](…)` relative to the note, so the
//! files travel with the library and other Markdown apps find them too. Port of
//! `Views/Editor/AttachmentImport.swift`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use unicode_segmentation::UnicodeSegmentation;

use crate::library_list::{path_components, standardized_path};
use crate::note::{FileKind, LibraryFile};
use crate::note_store::write_atomic;

pub const FOLDER_NAME: &str = "attachments";

/// Images and videos; other files (PDFs too) paste and drop as before.
pub fn is_media(path: &Path) -> bool {
    matches!(LibraryFile::kind_of(path), Some(FileKind::Image | FileKind::Video))
}

/// The file's place in the library: the file itself when it already lives
/// inside it, otherwise a copy in `attachments/`.
pub fn import_file(file: &Path, library: &Path) -> io::Result<PathBuf> {
    let file = PathBuf::from(standardized_path(&file.to_string_lossy()));
    if is_inside(&file, library) {
        return Ok(file);
    }
    let folder = attachments_folder(library)?;
    let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let target = unique_path(&name, &folder);
    fs::copy(&file, &target)?;
    Ok(target)
}

/// Saves pasted image data (a screenshot, an image copied from a browser), named
/// for the moment `now` (epoch seconds) at `utc_offset` seconds from UTC.
pub fn save_image(data: &[u8], ext: &str, library: &Path, now: f64, utc_offset: i32) -> io::Result<PathBuf> {
    let folder = attachments_folder(library)?;
    let target = unique_path(&format!("Pasted image {}.{ext}", stamp(now, utc_offset)), &folder);
    write_atomic(&target, data)?;
    Ok(target)
}

/// `name`, or `name 2`, `name 3` … — the first that isn't taken.
pub fn unique_path(name: &str, folder: &Path) -> PathBuf {
    let (base, ext) = split_extension(name);
    let mut candidate = folder.join(name);
    let mut n = 2;
    while fs::symlink_metadata(&candidate).is_ok() {
        let numbered = match ext {
            Some(ext) => format!("{base} {n}.{ext}"),
            None => format!("{base} {n}"),
        };
        candidate = folder.join(numbered);
        n += 1;
    }
    candidate
}

/// `file`'s path from `directory`, with `../` where needed.
pub fn relative_path(directory: &Path, file: &Path) -> String {
    let directory = standardized_path(&directory.to_string_lossy());
    let file = standardized_path(&file.to_string_lossy());
    let from = path_components(&directory);
    let to = path_components(&file);
    let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts = vec![".."; from.len() - shared];
    parts.extend_from_slice(&to[shared..]);
    parts.join("/")
}

/// `![](path)`. Only what would break the link is encoded (spaces, parentheses,
/// `%`); other names, Chinese ones too, stay readable.
pub fn markdown_for_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for grapheme in path.graphemes(true) {
        match grapheme {
            "%" => encoded.push_str("%25"),
            " " => encoded.push_str("%20"),
            "(" => encoded.push_str("%28"),
            ")" => encoded.push_str("%29"),
            _ => encoded.push_str(grapheme),
        }
    }
    format!("![]({encoded})")
}

fn attachments_folder(library: &Path) -> io::Result<PathBuf> {
    let folder = library.join(FOLDER_NAME);
    fs::create_dir_all(&folder)?;
    Ok(folder)
}

fn is_inside(file: &Path, folder: &Path) -> bool {
    let folder = standardized_path(&folder.to_string_lossy());
    let file = file.to_string_lossy();
    let parent = path_components(&folder);
    let child = path_components(&file);
    child.len() > parent.len() && child[..parent.len()] == parent[..]
}

/// `NSString.deletingPathExtension` and `pathExtension`: `.hidden` and `name.`
/// have no extension.
fn split_extension(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(index) if index > 0 && index + 1 < name.len() => (&name[..index], Some(&name[index + 1..])),
        _ => (name, None),
    }
}

/// `yyyyMMddHHmmss` in the local time `utc_offset` seconds from UTC.
fn stamp(now: f64, utc_offset: i32) -> String {
    let local = now.floor() as i64 + utc_offset as i64;
    let (year, month, day) = civil_from_days(local.div_euclid(86_400));
    let seconds = local.rem_euclid(86_400);
    format!("{year:04}{month:02}{day:02}{:02}{:02}{:02}", seconds / 3600, seconds % 3600 / 60, seconds % 60)
}

/// The Gregorian date of a day count from 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    // Ported from Tests/AttachmentImportTests.swift.
    use super::*;

    /// 2026-10-17 03:30:05 UTC.
    const PASTE_TIME: f64 = 1_792_207_805.0;

    fn name_of(path: &Path) -> String {
        path.file_name().unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn attachment_import() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("Library");
        let outside = root.path().join("Desktop");
        fs::create_dir_all(library.join("Notes/Deep")).unwrap();
        fs::create_dir_all(&outside).unwrap();

        // ── Media types ──
        assert!(is_media(Path::new("/a/clip.MOV")), "video, any case");
        assert!(is_media(Path::new("/a/shot.png")), "image");
        assert!(!is_media(Path::new("/a/doc.pdf")), "not media");

        // ── Copying in ──
        let clip = outside.join("my clip.mp4");
        fs::write(&clip, "v1").unwrap();
        let first = import_file(&clip, &library).unwrap();
        assert_eq!(name_of(&first), "my clip.mp4", "copied under its own name");
        assert_eq!(name_of(first.parent().unwrap()), "attachments", "into attachments/");
        assert!(clip.exists(), "original left in place");
        let second = import_file(&clip, &library).unwrap();
        assert_eq!(name_of(&second), "my clip 2.mp4", "name taken: numbered");

        // A file already in the library is linked, not copied.
        let in_library = library.join("Notes/shot.png");
        fs::write(&in_library, "p").unwrap();
        assert_eq!(
            import_file(&in_library, &library).unwrap().to_string_lossy(),
            standardized_path(&in_library.to_string_lossy()),
            "file inside the library is used as is"
        );

        // Pasted image data.
        let pasted = save_image(b"img", "png", &library, PASTE_TIME, 0).unwrap();
        assert_eq!(name_of(&pasted), "Pasted image 20261017033005.png", "pasted image name");

        // ── Paths ──
        let attachments = library.join("attachments");
        let file = attachments.join("my clip.mp4");
        assert_eq!(relative_path(&library, &file), "attachments/my clip.mp4", "note at the root");
        assert_eq!(relative_path(&library.join("Notes/Deep"), &file), "../../attachments/my clip.mp4", "note two folders down");
        assert_eq!(
            markdown_for_path("../attachments/my clip (1).mp4"),
            "![](../attachments/my%20clip%20%281%29.mp4)",
            "spaces and parentheses encoded"
        );
        assert_eq!(markdown_for_path("attachments/截图.png"), "![](attachments/截图.png)", "Chinese names stay readable");
    }

    // ── Beyond the Swift test ──

    #[test]
    fn copies_bytes_and_leaves_existing_files_alone() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("Library");
        let source = root.path().join("clip.mp4");
        fs::write(&source, [0u8, 0xFF, 0xFE, 1]).unwrap();
        let first = import_file(&source, &library).unwrap();
        fs::write(&source, "changed").unwrap();
        let second = import_file(&source, &library).unwrap();
        assert_eq!(fs::read(&first).unwrap(), [0u8, 0xFF, 0xFE, 1], "the first copy is untouched");
        assert_eq!(fs::read(&second).unwrap(), b"changed");
    }

    #[test]
    fn unique_names_follow_nsstring_rules() {
        let folder = tempfile::tempdir().unwrap();
        for name in ["a b.tar.gz", ".hidden", "name.", "noext"] {
            fs::write(folder.path().join(name), "").unwrap();
        }
        let unique = |name| name_of(&unique_path(name, folder.path()));
        assert_eq!(unique("a b.tar.gz"), "a b.tar 2.gz");
        assert_eq!(unique(".hidden"), ".hidden 2");
        assert_eq!(unique("name."), "name. 2");
        assert_eq!(unique("noext"), "noext 2");
        assert_eq!(unique("free.png"), "free.png");
    }

    #[test]
    fn stamp_uses_the_given_offset() {
        assert_eq!(stamp(PASTE_TIME, 8 * 3600), "20261017113005");
        assert_eq!(stamp(PASTE_TIME, -4 * 3600), "20261016233005", "across midnight");
        assert_eq!(stamp(0.0, 0), "19700101000000");
        assert_eq!(stamp(951_782_400.0, 0), "20000229000000", "a leap day");
    }

    #[test]
    fn paths_are_compared_standardized() {
        assert_eq!(relative_path(Path::new("/l/a/../b/"), Path::new("/l/b/c.png")), "c.png");
        assert_eq!(relative_path(Path::new("/l/b"), Path::new("/l/b")), "");
    }

    #[test]
    fn a_character_built_on_a_parenthesis_is_not_encoded() {
        // Swift walks Characters, and "(" with a combining mark isn't "(".
        assert_eq!(markdown_for_path("a(\u{301}b%"), "![](a(\u{301}b%25)");
    }
}
