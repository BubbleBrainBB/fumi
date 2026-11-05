//! The app-level alerts.

use fumi_workspace::Workspace;
use gpui::*;

/// The conflict sheet's two lines.
pub fn conflict_text(copy_name: &str) -> (&'static str, String) {
    (
        "This note was also changed outside Fumi",
        format!(
            "Some paragraphs were changed both here and outside. The note keeps your version; the outside version was saved as “{copy_name}” in the same folder."
        ),
    )
}

/// Both sides changed the same paragraph: says where the outside version went, on
/// the window as a sheet. Open Copy opens the copy in a new tab.
pub fn show_conflict_copy(
    copy_note_id: String,
    copy_name: &str,
    workspace: &Entity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let (message, detail) = conflict_text(copy_name);
    let answer = window.prompt(PromptLevel::Info, message, Some(&detail), &["OK", "Open Copy"], cx);
    let workspace = workspace.downgrade();
    cx.spawn(async move |cx| {
        if answer.await == Ok(1) {
            let _ = workspace.update(cx, |w, cx| {
                w.model.open_note(Some(&copy_note_id), true);
                w.navigated(cx);
            });
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    #[core::prelude::v1::test]
    fn conflict_names_the_copy() {
        let (title, detail) = super::conflict_text("Plan (conflict)");
        assert_eq!(title, "This note was also changed outside Fumi");
        assert!(detail.ends_with("saved as “Plan (conflict)” in the same folder."));
    }
}
