import SwiftUI
import AppKit

/// The sidebar's first level (design board 60 B ①): every library, one
/// 44pt row each — icon, name, where it is — the open one filled in;
/// Open Folder… under them adds one; Settings at the bottom as on the
/// second level.
struct LibrariesLevel: View {
    @Environment(AppModel.self) var appModel
    @Environment(FileNoteStore.self) var store
    @Environment(\.openWindow) private var openWindow
    private let libraries = LibraryList.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Libraries")
                .font(.fumi(.body, weight: .semibold))
                .foregroundColor(.primaryText)
                .padding(.horizontal, 10)
                .frame(height: 30)
                .padding(.leading, SidebarView.leadingInset)
                .padding(.top, 2)

            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(libraries.entries) { entry in
                        LibraryRow(entry: entry,
                                   isCurrent: isCurrent(entry),
                                   detail: detail(for: entry),
                                   noteCount: noteCount(for: entry),
                                   isMissing: LibraryList.missingReason(entry.url) != nil,
                                   enter: { enter(entry) },
                                   openInNewWindow: { LibraryWindows.open(entry.url, using: openWindow) })
                    }
                    OpenFolderRow {
                        guard let url = NotesFolderPanel.choose() else { return }
                        appModel.enterLibrary(url, store: store)
                    }
                    .padding(.top, 6)
                }
                .padding(.leading, SidebarView.leadingInset)
                .padding(.trailing, 8)
                .padding(.top, 4)
                .padding(.bottom, 16)
                .background(QuietScrollerInstaller(shortensKnob: true))
            }
            .scrollContentBackground(.hidden)

            SidebarSettingsRow()
                .padding(.leading, SidebarView.leadingInset)
                .padding(.trailing, 8)
                .padding(.vertical, 8)
        }
    }

    private func isCurrent(_ entry: LibraryEntry) -> Bool {
        store.workspaceURL?.standardizedFileURL.path == entry.path
    }

    /// Where it is; for one that can't be found, why.
    private func detail(for entry: LibraryEntry) -> String {
        LibraryList.missingReason(entry.url) ?? LibraryList.location(of: entry.url)
    }

    /// " · 516 notes", for the open library only.
    private func noteCount(for entry: LibraryEntry) -> String? {
        guard isCurrent(entry), store.hasLoadedNotes else { return nil }
        let count = store.notes.count
        return " · \(count) \(count == 1 ? "note" : "notes")"
    }

    /// A click goes in; ⌘-click opens the library in a window of its own.
    private func enter(_ entry: LibraryEntry) {
        if NSEvent.modifierFlags.contains(.command), !isCurrent(entry) {
            LibraryWindows.open(entry.url, using: openWindow)
        } else {
            appModel.enterLibrary(entry.url, store: store)
        }
    }
}

private struct LibraryRow: View {
    let entry: LibraryEntry
    let isCurrent: Bool
    let detail: String
    let noteCount: String?
    let isMissing: Bool
    let enter: () -> Void
    let openInNewWindow: () -> Void
    @State private var isHovering = false
    @State private var isDropTarget = false
    @State private var isRenaming = false
    @State private var isChangingIcon = false
    @State private var text = ""

    private var showsNewWindow: Bool { isHovering && !isCurrent && !isMissing }

    var body: some View {
        Button(action: enter) {
            HStack(spacing: 10) {
                LibraryTile(entry: entry, isCurrent: isCurrent, isMissing: isMissing)
                VStack(alignment: .leading, spacing: 1) {
                    Text(entry.displayName)
                        .font(.fumi(.body, weight: isCurrent ? .semibold : .regular))
                        .foregroundColor(isMissing ? .faintText : .primaryText)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    // A long location gives way; the count stays whole.
                    HStack(spacing: 0) {
                        Text(detail)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        if let noteCount {
                            Text(noteCount).fixedSize()
                        }
                    }
                    .font(.fumi(.caption))
                    .foregroundColor(isMissing ? .faintText : .tertiaryText)
                }
                Spacer(minLength: 0)
                if !isMissing {
                    // ↗ takes the chevron's place on hover (an overlay, so
                    // it's a button of its own).
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundColor(.faintIcon)
                        .opacity(showsNewWindow ? 0 : 1)
                        .frame(width: 12)
                        .padding(.leading, -4)
                }
            }
            .padding(.leading, 8)
            .padding(.trailing, 8)
            .frame(height: 44)
            .background(
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(isCurrent ? Color.groundSelectedBg : (isHovering ? Color.cardHover : Color.clear))
                    .shadow(color: isCurrent ? Color.selectedRowShadow : .clear, radius: 1.5, y: 0.5)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .strokeBorder(isDropTarget ? Color.accent.opacity(0.5) : Color.clear, lineWidth: 1)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .overlay(alignment: .trailing) {
            if showsNewWindow {
                NewWindowButton(action: openInNewWindow)
                    .padding(.trailing, 4)
            }
        }
        .onHover { isHovering = $0 }
        .help(isMissing ? entry.path : "\(entry.path)\n⌘-click to open in a new window")
        // Dragging a row onto another reorders the list.
        .draggable(entry.path) {
            HStack(spacing: 8) {
                LibraryTile(entry: entry, isMissing: isMissing)
                Text(entry.displayName)
                    .font(.fumi(.body, weight: .medium))
                    .foregroundColor(.primaryText)
            }
            .padding(6)
        }
        .dropDestination(for: String.self) { paths, _ in
            guard let path = paths.first, LibraryList.shared.entries.contains(where: { $0.path == path }) else { return false }
            withAnimation(.easeInOut(duration: 0.15)) { LibraryList.shared.move(path, to: entry.path) }
            return true
        } isTargeted: { isDropTarget = $0 }
        .contextMenu {
            Button("Open in New Window", action: openInNewWindow)
                .disabled(isCurrent || isMissing)
            Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([entry.url]) }
                .disabled(isMissing)
            Button("Rename…") {
                text = entry.displayName
                isRenaming = true
            }
            Button("Change Icon…") {
                text = entry.icon ?? ""
                isChangingIcon = true
            }
            Divider()
            Button("Remove from List") { LibraryList.shared.remove(entry.path) }
                .disabled(isCurrent)
        }
        .alert("Rename Library", isPresented: $isRenaming) {
            TextField("Name", text: $text)
            Button("Cancel", role: .cancel) {}
            Button("Rename") { LibraryList.shared.rename(entry.path, to: text) }
        } message: {
            Text("Only the name Fumi shows changes; the folder keeps its own. Leave it empty to use the folder’s name.")
        }
        .alert("Change Icon", isPresented: $isChangingIcon) {
            TextField("A letter or emoji", text: $text)
            Button("Cancel", role: .cancel) {}
            Button("Change") { LibraryList.shared.setIcon(entry.path, to: text) }
        } message: {
            Text("One character or emoji. Leave it empty for the plain library icon.")
        }
    }
}

/// A library's icon (design board 61 A): the sidebar's gray line library
/// icon — a step darker for the open library, faint for a missing one. A
/// character or emoji chosen with Change Icon… sits on one of the callouts'
/// six tints instead. `size` 28 on the first level, 16 beside the name on
/// the second; the line icon is 16 either way, centered in that room.
struct LibraryTile: View {
    let entry: LibraryEntry
    var isCurrent = false
    var isMissing = false
    var size: CGFloat = 28

    private static let kinds: [CalloutKind] = [.note, .tip, .important, .warning, .danger, .plain]

    var body: some View {
        if let icon = entry.icon, !icon.isEmpty {
            let kind = Self.kinds[entry.tint]
            let isEmoji = icon.unicodeScalars.first?.properties.isEmojiPresentation == true
            let scale = size / 28
            Text(icon)
                .font(.system(size: (isEmoji ? 16 : 13) * scale, weight: .semibold))
                .foregroundColor(isMissing ? .faintText : Color(nsColor: kind.color))
                .frame(width: size, height: size)
                .background(RoundedRectangle(cornerRadius: 7 * scale, style: .continuous)
                    .fill(isMissing ? Color.inlineFill : Color(nsColor: kind.background)))
                .opacity(isMissing ? 0.6 : 1)
        } else {
            ShiyeIcon(.library, size: 16)
                .foregroundColor(isMissing ? .faintIcon : isCurrent ? .secondaryText : .iconMuted)
                .frame(width: size, height: size)
        }
    }
}

private struct NewWindowButton: View {
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: "arrow.up.forward.square")
                .font(.system(size: 12, weight: .regular))
                .foregroundColor(isHovering ? .primaryText : .iconMuted)
                .frame(width: 24, height: 24)
                .background(
                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                        .fill(isHovering ? Color.cardHover : Color.clear)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .help("Open in New Window")
        .accessibilityLabel("Open in New Window")
    }
}

/// The one way to add a library: pick a folder.
private struct OpenFolderRow: View {
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                FolderGlyph(open: false)
                    .stroke(Color.iconMuted, style: StrokeStyle(lineWidth: 1.15, lineJoin: .round))
                    .frame(width: 16, height: 16)
                Text("Open Folder…")
                    .font(.fumi(.body))
                    .foregroundColor(isHovering ? .primaryText : .tertiaryText)
                Spacer(minLength: 0)
                Text("⌘O")
                    .font(.fumi(.caption))
                    .foregroundColor(.faintText)
            }
            .padding(.horizontal, 10)
            .frame(height: 30)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(isHovering ? Color.cardHover : Color.clear)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
    }
}

/// ⌘↑ (as in Finder) and a two-finger swipe to the right over the sidebar
/// go back to the libraries. ⌘↑ is left to the editor while typing, where
/// it moves to the top of the note.
struct LibraryBackGestures: NSViewRepresentable {
    let isActive: Bool
    let goBack: () -> Void

    func makeNSView(context: Context) -> GestureView {
        let view = GestureView()
        view.isActive = isActive
        view.goBack = goBack
        return view
    }

    func updateNSView(_ view: GestureView, context: Context) {
        view.isActive = isActive
        view.goBack = goBack
    }

    final class GestureView: NSView {
        var isActive = false
        var goBack: () -> Void = {}
        private var monitor: Any?
        /// How far this swipe has gone to the right, and whether it already
        /// went back.
        private var swipe: CGFloat = 0
        private var hasFired = false

        deinit { removeMonitor() }

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            removeMonitor()
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .scrollWheel]) { [weak self] event in
                self?.handle(event) ?? event
            }
        }

        private func removeMonitor() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }

        private func handle(_ event: NSEvent) -> NSEvent? {
            guard isActive, event.window === window else { return event }
            if event.type == .keyDown {
                let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
                    .subtracting([.numericPad, .function])
                guard flags == .command, event.keyCode == 126,
                      !(window?.firstResponder is NSTextView) else { return event }
                goBack()
                return nil
            }
            guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return event }
            if event.phase == .began {
                swipe = 0
                hasFired = false
            }
            guard event.phase == .began || event.phase == .changed, !hasFired,
                  abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY) * 1.5 else { return event }
            // Fingers to the right, whichever way scrolling is set.
            swipe += event.isDirectionInvertedFromDevice ? event.scrollingDeltaX : -event.scrollingDeltaX
            if swipe > 60 {
                hasFired = true
                goBack()
            }
            return event
        }
    }
}
