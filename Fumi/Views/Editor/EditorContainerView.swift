import SwiftUI

/// Right column: wraps the editor or shows empty state.
struct EditorContainerView: View {
    @Environment(AppModel.self) var appModel
    @Environment(FileNoteStore.self) var store

    var body: some View {
        let selected = appModel.selectedNoteId.flatMap { id in store.notes.first { $0.id == id } }
        // No note open in a library that has notes: the page is a blank draft
        // (design board ㉝ E). Same id before and after its first save, so the
        // editor — and the caret — carry straight on as it becomes a note.
        let draft = selected == nil && appModel.selectedNoteId == nil
            && store.hasLoadedNotes && !store.notes.isEmpty ? store.draftNote() : nil
        Group {
            if let note = selected ?? draft {
                EditorView(note: note, isDraft: selected == nil)
                    .id(note.id)
            } else {
                EmptyEditorView()
            }
        }
        .background(Color.pageContentBg)
    }
}

/// Shown when there's no note and no draft (design board ㉞ ②): while the
/// library loads, nothing; in a library with no notes yet, a line about the
/// folder and a way to write the first one.
struct EmptyEditorView: View {
    @Environment(FileNoteStore.self) var store

    var body: some View {
        Group {
            if store.hasLoadedNotes {
                EmptyLibraryPage()
            } else {
                Color.clear
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Color.pageContentBg)
        .modifier(NoteToolbar())
    }
}

/// A library folder with no notes in it yet.
private struct EmptyLibraryPage: View {
    @Environment(AppModel.self) var appModel
    @Environment(FileNoteStore.self) var store
    @AppStorage("fumi.editorFont") private var editorFontName: String = "System"
    @State private var isHoveringReveal = false

    private var name: String { store.workspaceURL?.lastPathComponent ?? "This folder" }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(BodyFace.text("No notes here yet", fontName: editorFontName, size: TextSize.title.rawValue, weight: .semibold))
                .foregroundColor(.primaryText)
            Text("“\(name)” is a plain folder. Write here, or drop .md files into it — they show up on the left.")
                .font(.fumi(.body))
                .lineSpacing(4)
                .foregroundColor(.tertiaryText)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 8)

            HStack(alignment: .firstTextBaseline, spacing: 22) {
                Button { appModel.openNote(store.createNote()?.id) } label: {
                    HStack(spacing: 7) {
                        ShiyeIcon(.newNote, size: 15)
                        Text("Write the First Note").fontWeight(.medium)
                        Text("⌘N").foregroundColor(.faintText)
                    }
                    .font(.fumi(.body))
                    .foregroundColor(.primaryText)
                    .padding(.bottom, 3)
                    .overlay(alignment: .bottom) { Rectangle().fill(Color.primaryText).frame(height: 1) }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)

                if let url = store.workspaceURL {
                    Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([url]) }
                        .buttonStyle(.plain)
                        .font(.fumi(.body))
                        .foregroundColor(isHoveringReveal ? .primaryText : .tertiaryText)
                        .onHover { isHoveringReveal = $0 }
                }
            }
            .padding(.top, 24)
        }
        .frame(width: 440, alignment: .leading)
        .padding(.top, 140)
    }
}
