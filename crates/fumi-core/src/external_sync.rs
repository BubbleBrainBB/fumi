//! Pure decisions for reconciling on-disk file changes (made by an external
//! editor) with what the in-app editor is showing. Port of
//! `Services/ExternalSync.swift`.

use std::collections::HashMap;
use std::path::Path;

/// What to do when the open note's on-disk content changes underneath the
/// editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing new from outside: on-disk content matches what's displayed, or
    /// is what the editor last loaded or saved (the echo of the app's own
    /// save). Nothing to load; just record that we're in sync.
    Ignore,
    /// The editor has no unsaved local edits, so the external change can be
    /// pulled in safely.
    Adopt,
    /// Both changed: the editor has unsaved local edits and the file was
    /// changed outside too. Merge the two (`merge3`) rather than letting either
    /// overwrite the other.
    Merge,
}

/// - `disk`: the latest content read from the file.
/// - `displayed`: what the editor is currently showing.
/// - `last_synced`: the content the editor last loaded or saved (its view of
///   what is on disk).
pub fn resolve(disk: &str, displayed: &str, last_synced: &str) -> Decision {
    if disk == displayed || disk == last_synced {
        return Decision::Ignore;
    }
    if displayed == last_synced {
        return Decision::Adopt;
    }
    Decision::Merge
}

/// The event paths that can touch the library: not the ones inside hidden
/// folders like `.git` or `.obsidian`, or hidden files like `.DS_Store`, which
/// the library's scan skips anyway.
pub fn visible_paths<S: AsRef<str>>(event_paths: &[S], library: &str) -> Vec<String> {
    let root = if library.ends_with('/') { library.to_string() } else { format!("{library}/") };
    event_paths
        .iter()
        .map(AsRef::as_ref)
        .filter(|path| match path.strip_prefix(root.as_str()) {
            None => true,
            Some(rest) => !rest.split('/').any(|part| part.starts_with('.')),
        })
        .map(str::to_string)
        .collect()
}

/// Whether a batch of file-system event paths represents a change the app did
/// not make itself. Used to skip the rescan that our own debounced saves would
/// otherwise trigger on every keystroke window.
///
/// A path counts as ours only while the file still holds what we wrote: a write
/// from outside that lands right after one of our saves (in the same batch of
/// events, even) is still seen.
///
/// - `event_paths`: paths reported by the watcher.
/// - `own_writes`: the content the app recently wrote, by path.
/// - `read`: the file's current content, `None` when it can't be read.
pub fn has_external_change<S: AsRef<str>>(
    event_paths: &[S],
    own_writes: &HashMap<String, String>,
    read: impl Fn(&str) -> Option<String>,
) -> bool {
    let mut edited = event_paths.iter().map(AsRef::as_ref).filter(|path| is_note_path(path)).peekable();
    // No note files in the batch (e.g. a bare directory event) → reload to be
    // safe; it might be a create/delete/rename we should reflect.
    if edited.peek().is_none() {
        return true;
    }
    edited.any(|path| match own_writes.get(path) {
        None => true,
        Some(written) => read(path).as_deref() != Some(written.as_str()),
    })
}

fn is_note_path(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|ext| {
        let ext = ext.to_string_lossy().to_lowercase();
        ext == "md" || ext == "markdown"
    })
}

#[cfg(test)]
mod tests {
    // Ported from Tests/ExternalSyncTests.swift.
    use super::*;

    fn own(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn resolve_cases() {
        // Disk equals what's shown — our own save echo / already in sync.
        assert_eq!(resolve("a", "a", "a"), Decision::Ignore, "in sync → ignore");
        assert_eq!(resolve("a", "a", "old"), Decision::Ignore, "disk == displayed even with stale lastSynced → ignore");

        // Clean editor (displayed == lastSynced) but disk moved → adopt.
        assert_eq!(resolve("new", "old", "old"), Decision::Adopt, "clean editor + external change → adopt");

        // Editor has unsaved local edits and the file changed outside → merge.
        assert_eq!(resolve("external", "myedit", "old"), Decision::Merge, "local edits + external change → merge");

        // Disk still holds what the editor last saved (our own save's echo, the
        // user typing on since) → nothing from outside.
        assert_eq!(resolve("saved", "saved and more", "saved"), Decision::Ignore, "own save echo while typing on → ignore");

        // Local edits that happen to match disk are caught by the first rule.
        assert_eq!(resolve("same", "same", "old"), Decision::Ignore, "local edit converged with disk → ignore");
    }

    #[test]
    fn visible_paths_cases() {
        let lib = "/w/Notes";
        assert_eq!(
            visible_paths(&["/w/Notes/.git/index", "/w/Notes/.obsidian/workspace.json", "/w/Notes/.DS_Store", "/w/Notes/sub/.hidden/a.md"], lib),
            Vec::<String>::new(),
            "inside hidden folders, or hidden themselves → dropped"
        );
        assert_eq!(
            visible_paths(&["/w/Notes/a.md", "/w/Notes/sub/b.png", "/w/Notes/sub", "/w/Notes"], lib),
            vec!["/w/Notes/a.md", "/w/Notes/sub/b.png", "/w/Notes/sub", "/w/Notes"],
            "visible files and folders → kept"
        );
        assert_eq!(
            visible_paths(&["/w/.vaults/Notes/a.md"], "/w/.vaults/Notes"),
            vec!["/w/.vaults/Notes/a.md"],
            "a hidden folder above the library doesn't count"
        );
        assert_eq!(
            visible_paths(&["/private/w/.git/x"], "/w"),
            vec!["/private/w/.git/x"],
            "outside the library as we name it (a symlink) → kept, to be safe"
        );
    }

    #[test]
    fn has_external_change_cases() {
        let disk = own(&[("/w/a.md", "mine"), ("/w/b.md", "theirs"), ("/w/a.markdown", "mine")]);
        let read = |path: &str| disk.get(path).cloned();

        // A note we just saved ourselves, still as we wrote it → not external.
        assert!(!has_external_change(&["/w/a.md"], &own(&[("/w/a.md", "mine")]), read), "only our own save → not external");

        // We saved it, but something wrote over it since → external.
        assert!(has_external_change(&["/w/b.md"], &own(&[("/w/b.md", "mine")]), read), "written over right after our save → external");

        // A note changed that we did not write → external.
        assert!(has_external_change(&["/w/a.md"], &own(&[]), read), "unknown md change → external");

        // Mixed: one ours, one external → external (don't suppress).
        assert!(has_external_change(&["/w/a.md", "/w/b.md"], &own(&[("/w/a.md", "mine")]), read), "one external among ours → external");

        // Our file deleted since → external.
        assert!(has_external_change(&["/w/gone.md"], &own(&[("/w/gone.md", "x")]), read), "our file gone → external");

        // Non-note files only → conservatively treat as external (covers dir
        // events for create/delete/rename we should reflect).
        assert!(has_external_change(&["/w/.git/index"], &own(&[]), read), "no md paths → reload to be safe");

        // .markdown extension is recognized too.
        assert!(!has_external_change(&["/w/a.markdown"], &own(&[("/w/a.markdown", "mine")]), read), ".markdown self-save → not external");
    }

    // ── Beyond the Swift test ──

    #[test]
    fn extension_case_and_shape() {
        assert!(is_note_path("/w/A.MD"));
        assert!(is_note_path("/w/a.Markdown"));
        assert!(!is_note_path("/w/.md"), "a hidden file named .md has no extension");
        assert!(!is_note_path("/w/x.md/y"));
    }
}
