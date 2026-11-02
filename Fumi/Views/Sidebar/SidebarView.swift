import SwiftUI
import AppKit
import UniformTypeIdentifiers

/// Left column, two levels (design board 60 B). The first lists every
/// library; the second is inside one: ‹ Libraries back to the first, the
/// library's name with folder options (…) and New Note (+), Search, Memos,
/// the notes tree — and Settings pinned to the bottom of both.
struct SidebarView: View {
    @Environment(AppModel.self) var appModel
    @Environment(FileNoteStore.self) var store
    @State private var tree = NoteTreeState()

    /// Matches the trailing 8. (Was 22 while the hidden-title-bar window
    /// clipped ~14pt off the column's leading edge; the unified toolbar
    /// window doesn't.)
    static let leadingInset: CGFloat = 8

    /// The notes tree's notes: everything but the Memos folder, which has its
    /// own page (the Memos row) rather than showing up twice.
    private var sortedNotes: [FileNote] {
        let memoFolder = "/" + MemoFormat.folderName
        return NoteSorting.sorted(store.notes.filter {
            $0.relativePath != memoFolder && !$0.relativePath.hasPrefix(memoFolder + "/")
        }, by: appModel.sortMode)
    }

    var body: some View {
        levels
        .frame(minWidth: 0, maxWidth: .infinity)
        // On the window ground, no divider: the editor's floating page is
        // what separates the two columns (design board ㉗ D).
        .background(appModel.groundColor)
        // The hide button sits in the sidebar's part of the toolbar, on the
        // line of the traffic lights and the note title. It stays there when
        // the sidebar collapses, so it is always ONE item: swapping it for a
        // second one in the detail column made the toggle vanish and the new
        // one slide in from above mid-animation.
        .toolbar {
            SidebarToggleItem(placement: .automatic)
        }
        .background(
            SidebarSplitHook(workspaceKey: store.workspaceURL?.path,
                             contentWidth: { longestRowWidth() },
                             onCollapsedChange: { collapsed in
                                 // Dragging past the minimum collapses the column
                                 // without going through our binding; mirror it so
                                 // the toolbar shows the way back.
                                 let target: NavigationSplitViewVisibility = collapsed ? .detailOnly : .all
                                 if appModel.columnVisibility != target { appModel.columnVisibility = target }
                             })
                .frame(width: 0, height: 0)
        )
        .onChange(of: appModel.selectedNoteId, initial: true) { _, id in
            revealSelectedNote(id)
        }
        // Another library: its own folders, closed.
        .onChange(of: store.workspaceURL) {
            tree.expandedFolders = []
            revealSelectedNote(appModel.selectedNoteId)
        }
        .environment(tree)
        .alert(tree.failedMove?.title ?? "", isPresented: Binding(
            get: { tree.failedMove != nil }, set: { if !$0 { tree.failedMove = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(tree.failedMove?.message ?? "")
        }
    }

    /// The natural width of the widest note row (indent + file name + chrome),
    /// for double-click-to-fit on the divider.
    private func longestRowWidth() -> CGFloat {
        let font = NSFont.systemFont(ofSize: TextSize.body.rawValue, weight: .semibold)
        let outer = Self.leadingInset + 8
        return store.notes.reduce(SidebarWidthPolicy.minimum) { widest, note in
            let depth = CGFloat(note.relativePath.split(separator: "/").count)
            let indent = NoteTreeIndent.base + depth * NoteTreeIndent.perLevel
                + NoteTreeIndent.icon + NoteTreeIndent.iconGap
            let text = (note.filename as NSString).size(withAttributes: [.font: font]).width
            return Swift.max(widest, outer + indent + ceil(text) + 10 + (note.pinned ? 16 : 0))
        }
    }

    /// The two levels side by side, the sidebar showing one: going into a
    /// library pushes the list out to the left and the library's files in
    /// from the right, like a step in iPhone Settings; going back reverses it.
    private var levels: some View {
        GeometryReader { geometry in
            let width = geometry.size.width
            let atLibraries = appModel.showsLibraries
            HStack(spacing: 0) {
                LibrariesLevel()
                    .frame(width: width, height: geometry.size.height)
                    .allowsHitTesting(atLibraries)
                    .accessibilityHidden(!atLibraries)
                list
                    .frame(width: width, height: geometry.size.height)
                    .allowsHitTesting(!atLibraries)
                    .accessibilityHidden(atLibraries)
            }
            .offset(x: atLibraries ? 0 : -width)
            .animation(.easeInOut(duration: 0.25), value: atLibraries)
        }
        .clipped()
    }

    private var list: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 1) {
                LibrariesBackRow { appModel.showsLibraries = true }
                LibraryHeader()
                    .padding(.bottom, 4)
                SidebarSearchField()
                SidebarRow(icon: .glyph(.memo), title: "Memos", isSelected: appModel.showMemos) {
                    appModel.openMemos(inNewTab: NSEvent.modifierFlags.contains(.command))
                }
            }
            .padding(.leading, Self.leadingInset)
            .padding(.trailing, 8)
            .padding(.top, 2)

            Rectangle()
                .fill(Color.borderColor)
                .frame(height: 1)
                .padding(.horizontal, Self.leadingInset + 8)
                .padding(.vertical, 8)

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 1) {
                    notesSection
                }
                .padding(.leading, Self.leadingInset)
                .padding(.trailing, 8)
                .padding(.bottom, 16)
                .background(QuietScrollerInstaller(shortensKnob: true))
            }
            .scrollContentBackground(.hidden)
            // Dropped anywhere but on a row: out to the library's top level.
            .onDrop(of: [.url], delegate: TreeDropDelegate(
                folderId: "/", tree: tree, store: store, appModel: appModel, settles: true))

            SidebarSettingsRow()
                .padding(.leading, Self.leadingInset)
                .padding(.trailing, 8)
                .padding(.vertical, 8)
        }
        // ⌘↑ and a two-finger swipe to the right go back to the libraries.
        .background(LibraryBackGestures(isActive: !appModel.showsLibraries && appModel.isSidebarVisible) {
            appModel.showsLibraries = true
        })
    }

    // MARK: Notes tree

    private var notesSection: some View {
        let root = NoteTreeBuilder.build(from: sortedNotes, files: store.files)
        return Group {
            if !store.hasLoadedNotes {
                // The first scan hasn't landed: not "No notes yet".
                EmptyView()
            } else if root.totalCount == 0 {
                Text("No notes yet")
                    .font(.fumi(.caption))
                    .foregroundColor(.tertiaryText)
                    .padding(.leading, NoteTreeIndent.base)
                    .frame(height: 28)
            } else {
                NoteFolderContents(folder: root, depth: 0)
            }
        }
        .onChange(of: NoteTreeState.folderIds(in: root), initial: true) { _, ids in tree.folderIds = ids }
    }

    /// Expands every folder on the way to the selected note so it is visible.
    private func revealSelectedNote(_ id: String?) {
        guard let id, let note = store.notes.first(where: { $0.id == id }) else { return }
        tree.expandedFolders.formUnion(NoteTreeBuilder.ancestorIds(ofRelativePath: note.relativePath))
    }
}

// MARK: - Sorting

enum NoteSorting {
    static func sorted(_ notes: [FileNote], by mode: SortMode) -> [FileNote] {
        notes.sorted { lhs, rhs in
            if lhs.pinned != rhs.pinned { return lhs.pinned }
            switch mode {
            case .updatedDesc: return lhs.updatedAt > rhs.updatedAt
            case .createdDesc: return lhs.createdAt > rhs.createdAt
            case .titleAsc: return lhs.filename.localizedStandardCompare(rhs.filename) == .orderedAscending
            }
        }
    }
}

// MARK: - Library heading and search

/// The open library's name, under ‹ Libraries: … opens the folder options
/// (open another folder, new window, reveal, sort); + makes a new note.
/// Notes dropped on it go to the library's top level.
private struct LibraryHeader: View {
    @Environment(FileNoteStore.self) var store
    @Environment(AppModel.self) var appModel
    @Environment(NoteTreeState.self) var tree
    @Environment(\.openWindow) private var openWindow
    @State private var isHoveringMore = false
    @State private var isHoveringAdd = false

    private var entry: LibraryEntry? {
        LibraryList.shared.entry(for: store.workspaceURL)
    }

    private var name: String {
        entry?.displayName ?? store.workspaceURL?.lastPathComponent ?? "Notes"
    }

    var body: some View {
        HStack(spacing: 2) {
            HStack(spacing: 9) {
                // The same icon as the library's row on the first level.
                if let entry {
                    LibraryTile(entry: entry, size: 16)
                } else {
                    ShiyeIcon(.library, size: NoteTreeIndent.icon)
                        .foregroundColor(.iconMuted)
                        .frame(width: 16, height: NoteTreeIndent.icon)
                }
                Text(name)
                    .font(.fumi(.body, weight: .semibold))
                    .foregroundColor(.primaryText)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
            }
            .padding(.leading, 10)
            .frame(maxHeight: .infinity)

            Menu {
                Button("Open Folder…") {
                    guard let url = NotesFolderPanel.choose() else { return }
                    appModel.enterLibrary(url, store: store)
                }
                Button("Open in New Window") { openWindow(id: "main") }
                if let url = store.workspaceURL {
                    Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([url]) }
                }
                Divider()
                Picker("Sort By", selection: Binding(get: { appModel.sortMode }, set: { appModel.setSortMode($0) })) {
                    ForEach(SortMode.allCases, id: \.self) { Text($0.label).tag($0) }
                }
            } label: {
                ShiyeIcon(.more, size: 15)
                    .foregroundColor(isHoveringMore ? .primaryText : .iconMuted)
                    .frame(width: 24, height: 22)
                    .background(
                        RoundedRectangle(cornerRadius: 5, style: .continuous)
                            .fill(isHoveringMore ? Color.cardHover : Color.clear)
                    )
                    .contentShape(Rectangle())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("Folder Options")
            .accessibilityLabel("Folder Options")
            .onHover { isHoveringMore = $0 }

            Button {
                appModel.openNote(store.createNote()?.id)
            } label: {
                Image(systemName: "plus")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(isHoveringAdd ? .primaryText : .iconMuted)
                    .frame(width: 24, height: 22)
                    .background(
                        RoundedRectangle(cornerRadius: 5, style: .continuous)
                            .fill(isHoveringAdd ? Color.cardHover : Color.clear)
                    )
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("New Note (⌘N)")
            .onHover { isHoveringAdd = $0 }
        }
        .padding(.trailing, 4)
        .frame(height: 32)
        .dropHighlight(tree.dropTarget == "/")
        .treeDropTarget("/")
    }
}

/// "‹ Libraries", above the library's name: back to the first level.
private struct LibrariesBackRow: View {
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: "chevron.left")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundColor(isHovering ? .secondaryText : .iconMuted)
                Text("Libraries")
                    .font(.fumi(.caption))
                    .foregroundColor(isHovering ? .secondaryText : .tertiaryText)
            }
            .padding(.horizontal, 8)
            .frame(height: 26)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(isHovering ? Color.cardHover : Color.clear)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .help("Show Libraries (⌘↑)")
        .padding(.leading, 2)
    }
}

/// Search styled as a plain sidebar row. Clicking it opens the Spotlight-style
/// quick search popover (SearchPopover), which owns the query and the fuzzy
/// matching — the sidebar itself no longer filters inline.
private struct SidebarSearchField: View {
    @Environment(AppModel.self) private var appModel
    @State private var isHovering = false

    var body: some View {
        Button { appModel.presentSearch() } label: {
            HStack(spacing: 9) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 13, weight: .regular))
                    .foregroundColor(.iconMuted)
                    .frame(width: 16)
                Text("Search")
                    .font(.fumi(.body))
                    .foregroundColor(.tertiaryText)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .frame(height: 30)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(isHovering ? Color.cardHover : Color.clear)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .strokeBorder(isHovering ? Color.controlBorder : Color.clear, lineWidth: 1)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .help("Search Notes (⇧⌘F)")
    }
}

struct ShortcutHint: View {
    let text: String
    var boxed = false

    var body: some View {
        Text(text)
            .font(.fumi(.caption, weight: .medium))
            .foregroundColor(.tertiaryText)
            .padding(.horizontal, boxed ? 5 : 0)
            .frame(height: 18)
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(boxed ? Color.surface : Color.clear)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .strokeBorder(boxed ? Color.controlBorder : Color.clear, lineWidth: 1)
            )
    }
}

// MARK: - Rows

struct SidebarSectionLabel: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.fumi(.caption, weight: .semibold))
            .foregroundColor(.tertiaryText)
            .padding(.horizontal, 10)
            .padding(.top, 4)
            .padding(.bottom, 3)
    }
}

/// A top-level navigation row (icon, title).
/// A sidebar icon: one of 拾页's glyphs, or an SF Symbol for the generic
/// actions (settings, search…).
enum SidebarIcon {
    case glyph(ShiyeGlyph)
    case symbol(String)
}

private struct SidebarIconImage: View {
    let icon: SidebarIcon
    var size: CGFloat = 16
    var symbolFont: Font = .system(size: 13, weight: .regular)

    var body: some View {
        switch icon {
        case .glyph(let glyph):
            ShiyeIcon(glyph, size: size)
        case .symbol(let name):
            Image(systemName: name).font(symbolFont)
        }
    }
}

struct SidebarRow: View {
    let icon: SidebarIcon
    let title: String
    var isSelected = false
    /// Bold text without the selected fill — the row's subtree holds the
    /// selection (the design's "笔记" row while a note is open).
    var isEmphasized = false
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                SidebarIconImage(icon: icon)
                    .foregroundColor(isSelected || isEmphasized ? .primaryText : .iconMuted)
                    .frame(width: 16)
                Text(title)
                    .font(.fumi(.body, weight: isSelected || isEmphasized ? .semibold : .regular))
                    .foregroundColor(isSelected || isEmphasized ? .primaryText : .secondaryText)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .frame(height: 30)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(isSelected ? Color.groundSelectedBg : (isHovering ? Color.cardHover : Color.clear))
                    .shadow(color: isSelected ? Color.selectedRowShadow : .clear, radius: 1.5, y: 0.5)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
    }
}

/// Tree geometry (design board ⑱ option A): every row leads with a 16pt
/// icon — folder, open folder, or document — and each level indents by one
/// icon plus its gap, so children line up under their parent's name.
enum NoteTreeIndent {
    /// Leading inset of a top-level row's icon.
    static let base: CGFloat = 14
    static let icon: CGFloat = 16
    static let iconGap: CGFloat = 8
    static var perLevel: CGFloat { icon + iconGap }
}

/// Line-drawn folder, closed or open (no open-folder SF Symbol exists).
/// Drawn on a 24-unit grid like the design's SVG.
struct FolderGlyph: Shape {
    var open: Bool

    func path(in rect: CGRect) -> Path {
        let k = min(rect.width, rect.height) / 24
        func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: rect.minX + x * k, y: rect.minY + y * k) }
        var path = Path()
        if !open {
            path.move(to: p(3, 7))
            path.addQuadCurve(to: p(5, 5), control: p(3, 5))
            path.addLine(to: p(9, 5))
            path.addLine(to: p(11, 7))
            path.addLine(to: p(19, 7))
            path.addQuadCurve(to: p(21, 9), control: p(21, 7))
            path.addLine(to: p(21, 17))
            path.addQuadCurve(to: p(19, 19), control: p(21, 19))
            path.addLine(to: p(5, 19))
            path.addQuadCurve(to: p(3, 17), control: p(3, 19))
            path.closeSubpath()
        } else {
            // Back panel with the tab…
            path.move(to: p(3, 19))
            path.addLine(to: p(3, 7))
            path.addQuadCurve(to: p(5, 5), control: p(3, 5))
            path.addLine(to: p(9, 5))
            path.addLine(to: p(11, 7))
            path.addLine(to: p(18, 7))
            path.addQuadCurve(to: p(20, 9), control: p(20, 7))
            path.addLine(to: p(20, 10))
            // …and the front flap, tilted open.
            path.move(to: p(3, 19))
            path.addLine(to: p(5.6, 11.4))
            path.addQuadCurve(to: p(7.5, 10), control: p(6.1, 10))
            path.addLine(to: p(21, 10))
            path.addLine(to: p(18.6, 17.6))
            path.addQuadCurve(to: p(16.7, 19), control: p(18.2, 19))
            path.closeSubpath()
        }
        return path
    }
}

/// The folders, then the notes, of one folder in the tree.
private struct NoteFolderContents: View {
    let folder: NoteFolder
    let depth: Int
    var baseIndent: CGFloat = NoteTreeIndent.base
    @Environment(NoteTreeState.self) var tree

    var body: some View {
        ForEach(folder.folders) { child in
            FolderRow(folder: child, depth: depth, isExpanded: tree.expandedFolders.contains(child.id), baseIndent: baseIndent) {
                withAnimation(.easeInOut(duration: 0.15)) {
                    if tree.expandedFolders.contains(child.id) {
                        tree.expandedFolders.remove(child.id)
                    } else {
                        tree.expandedFolders.insert(child.id)
                    }
                }
            }
            if tree.expandedFolders.contains(child.id) {
                NoteFolderContents(folder: child, depth: depth + 1, baseIndent: baseIndent)
            }
        }
        ForEach(folder.notes) { note in
            // Note titles line up with sibling folder names (past the chevron).
            NoteRow(note: note, leading: leading(for: depth))
        }
        ForEach(folder.files) { file in
            FileRow(file: file, leading: leading(for: depth))
        }
    }

    private func leading(for depth: Int) -> CGFloat {
        baseIndent + CGFloat(depth) * NoteTreeIndent.perLevel
    }
}

private struct FolderRow: View {
    let folder: NoteFolder
    let depth: Int
    let isExpanded: Bool
    var baseIndent: CGFloat = NoteTreeIndent.base
    let toggle: () -> Void
    @Environment(FileNoteStore.self) var store
    @Environment(NoteTreeState.self) var tree
    @State private var isHovering = false

    private var url: URL? {
        store.workspaceURL.map { LibraryMove.folderURL(forId: folder.id, in: $0) }
    }

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: NoteTreeIndent.iconGap) {
                // No disclosure arrow: the folder itself shows the state,
                // open while expanded and closed while folded.
                FolderGlyph(open: isExpanded)
                    .stroke(isExpanded ? Color.secondaryText : Color.iconMuted,
                            style: StrokeStyle(lineWidth: 1.15, lineJoin: .round))
                    .frame(width: NoteTreeIndent.icon, height: NoteTreeIndent.icon)
                Text(folder.name)
                    .font(.fumi(.body, weight: isExpanded ? .medium : .regular))
                    .foregroundColor(isExpanded || isHovering ? .primaryText : .secondaryText)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.leading, baseIndent + CGFloat(depth) * NoteTreeIndent.perLevel)
            .padding(.trailing, 10)
            .frame(height: 28)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(isHovering ? Color.cardHover : Color.clear)
            )
            .dropHighlight(tree.dropTarget == folder.id)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .draggable(url ?? URL(fileURLWithPath: "/")) {
            SidebarDragPreview(title: folder.name) {
                FolderGlyph(open: false)
                    .stroke(Color.secondaryText, style: StrokeStyle(lineWidth: 1.15, lineJoin: .round))
            }
        }
        .treeDropTarget(folder.id, springLoads: !isExpanded)
        .contextMenu {
            if let url {
                MoveToMenu(item: url, parent: (folder.id as NSString).deletingLastPathComponent, folderId: folder.id)
                Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([url]) }
            }
        }
    }
}

/// One note in the tree. Carries the note actions that used to live on the
/// note list's cards (pin, rename, reveal, delete).
private struct NoteRow: View {
    let note: FileNote
    /// Leading inset of the row's icon, measured from the sidebar row's edge.
    let leading: CGFloat
    @Environment(FileNoteStore.self) var store
    @Environment(AppModel.self) var appModel
    @State private var isHovering = false
    @State private var isRenaming = false
    @State private var renameText = ""

    private var isSelected: Bool {
        appModel.selectedNoteId == note.id && !appModel.showSettings
    }

    /// Rows show the file name (not the first line, which for clipped
    /// articles is just their opening sentence).
    private var displayTitle: String { note.filename }

    var body: some View {
        Button {
            // A new tab, the open ones kept; only a blank draft is filled in
            // place, and ⌘-click keeps that too (design board ㊴).
            appModel.openNote(note.id, inNewTab: NSEvent.modifierFlags.contains(.command))
        } label: {
            HStack(spacing: NoteTreeIndent.iconGap) {
                ShiyeIcon(.note, size: 15)
                    .foregroundColor(isSelected ? .secondaryText : .faintIcon)
                    .frame(width: NoteTreeIndent.icon)
                Text(displayTitle)
                    .font(.fumi(.body, weight: isSelected ? .semibold : .regular))
                    .foregroundColor(isSelected ? .primaryText : .secondaryText)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
                if note.pinned {
                    Image(systemName: "pin.fill")
                        .font(.system(size: 9))
                        .foregroundColor(.iconMuted)
                }
            }
            .padding(.leading, leading)
            .padding(.trailing, 10)
            .frame(height: 28)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(isSelected ? Color.groundSelectedBg : (isHovering ? Color.cardHover : Color.clear))
                    .shadow(color: isSelected ? Color.selectedRowShadow : .clear, radius: 1.5, y: 0.5)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .draggable(note.url) {
            SidebarDragPreview(title: displayTitle) { ShiyeIcon(.note, size: 15) }
        }
        // Dropped on a note: into the note's folder, as in Finder's list view.
        .treeDropTarget(note.relativePath)
        .contextMenu {
            Button(note.pinned ? "Unpin" : "Pin") { store.togglePin(note.id) }
            Button("Rename…") {
                renameText = note.filename
                isRenaming = true
            }
            MoveToMenu(item: note.url, parent: note.relativePath)
            Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([note.url]) }
            Divider()
            Button("Delete", role: .destructive) {
                appModel.removeTabs(showing: .note(note.id))
                store.deleteNote(note.id)
            }
        }
        .alert("Rename Note", isPresented: $isRenaming) {
            TextField("File name", text: $renameText)
            Button("Cancel", role: .cancel) {}
            Button("Rename") { performRename() }
        } message: {
            Text("Enter a new file name (without the .md extension).")
        }
    }

    private func performRename() {
        if let newId = store.renameNote(note.id, to: renameText) {
            appModel.remapTabs(from: note.id, to: newId)
        }
    }
}

/// An image, video or PDF in the tree, after the notes of its folder. Opens
/// in the preview page.
private struct FileRow: View {
    let file: LibraryFile
    let leading: CGFloat
    @Environment(FileNoteStore.self) var store
    @Environment(AppModel.self) var appModel
    @State private var isHovering = false

    private var isSelected: Bool {
        appModel.selectedFileId == file.id && appModel.selectedNoteId == nil
            && !appModel.showSettings && !appModel.showMemos
    }

    private var glyph: ShiyeGlyph {
        switch file.kind {
        case .image: .image
        case .video: .video
        case .pdf: .file
        }
    }

    var body: some View {
        Button {
            appModel.openFile(file.id, inNewTab: NSEvent.modifierFlags.contains(.command))
        } label: {
            HStack(spacing: NoteTreeIndent.iconGap) {
                ShiyeIcon(glyph, size: 15)
                    .foregroundColor(isSelected ? .secondaryText : .faintIcon)
                    .frame(width: NoteTreeIndent.icon)
                Text(file.name)
                    .font(.fumi(.body, weight: isSelected ? .semibold : .regular))
                    .foregroundColor(isSelected ? .primaryText : .tertiaryText)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 0)
            }
            .padding(.leading, leading)
            .padding(.trailing, 10)
            .frame(height: 28)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(isSelected ? Color.groundSelectedBg : (isHovering ? Color.cardHover : Color.clear))
                    .shadow(color: isSelected ? Color.selectedRowShadow : .clear, radius: 1.5, y: 0.5)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .draggable(file.url) {
            SidebarDragPreview(title: file.name) { ShiyeIcon(glyph, size: 15) }
        }
        .treeDropTarget(file.relativePath)
        .contextMenu {
            Button("Copy Markdown Link") {
                guard let library = store.workspaceURL else { return }
                let path = AttachmentImport.relativePath(from: library, to: file.url)
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(AttachmentImport.markdown(forPath: path), forType: .string)
            }
            Button("Open in Default App") { NSWorkspace.shared.open(file.url) }
            MoveToMenu(item: file.url, parent: file.relativePath)
            Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([file.url]) }
            Divider()
            Button("Move to Trash", role: .destructive) {
                if (try? FileManager.default.trashItem(at: file.url, resultingItemURL: nil)) != nil {
                    appModel.removeTabs(showing: .file(file.id))
                    store.reloadNotes()
                }
            }
        }
    }
}

// MARK: - Moving

/// Move To: the library's top level and every folder of the tree, but not
/// the item's own folder — nor, for a folder, itself or a folder inside it.
private struct MoveToMenu: View {
    let item: URL
    /// The folder the item is in now.
    let parent: String
    /// For a folder, its own id.
    var folderId: String? = nil
    @Environment(NoteTreeState.self) var tree
    @Environment(FileNoteStore.self) var store
    @Environment(AppModel.self) var appModel

    private var destinations: [String] {
        (["/"] + tree.folderIds).filter { id in
            id != parent && !(folderId.map { LibraryMove.isSameOrInside(id, $0) } ?? false)
        }
    }

    var body: some View {
        Menu("Move To") {
            ForEach(destinations, id: \.self) { id in
                Button(label(for: id)) {
                    tree.move([item], toFolder: id, store: store, appModel: appModel)
                }
            }
        }
        .disabled(destinations.isEmpty)
    }

    /// The library's name, or the folder's path below it ("Travel › Japan").
    private func label(for id: String) -> String {
        id == "/" ? tree.folderName("/", store: store) : id.split(separator: "/").joined(separator: " › ")
    }
}

/// Lands dragged sidebar rows in the folder `folderId`. Proposes a move, so
/// the pointer shows no copy badge.
private struct TreeDropDelegate: DropDelegate {
    let folderId: String
    let tree: NoteTreeState
    let store: FileNoteStore
    let appModel: AppModel
    /// The list behind the rows: claims the highlight only after a beat.
    var settles = false
    /// A closed folder: opens if the drag rests on it.
    var springLoads = false

    func validateDrop(info: DropInfo) -> Bool {
        info.hasItemsConforming(to: [.url])
    }

    func dropEntered(info: DropInfo) {
        tree.setTargeted(true, folder: folderId, settles: settles)
        if springLoads { tree.springLoad(folderId) }
    }

    func dropExited(info: DropInfo) {
        tree.setTargeted(false, folder: folderId)
        tree.cancelSpringLoad(folderId)
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        let providers = info.itemProviders(for: [.url])
        guard !providers.isEmpty else { return false }
        Task { @MainActor in
            var urls: [URL] = []
            for provider in providers {
                if let url = await Self.loadURL(provider) { urls.append(url) }
            }
            tree.move(urls, toFolder: folderId, store: store, appModel: appModel)
        }
        return true
    }

    private static func loadURL(_ provider: NSItemProvider) async -> URL? {
        await withCheckedContinuation { continuation in
            _ = provider.loadObject(ofClass: URL.self) { url, _ in
                continuation.resume(returning: url?.isFileURL == true ? url : nil)
            }
        }
    }
}

/// What follows the pointer while a sidebar row is dragged: the row's icon
/// and name on a small raised label, legible over whatever it passes.
private struct SidebarDragPreview<Icon: View>: View {
    let title: String
    @ViewBuilder let icon: Icon

    var body: some View {
        HStack(spacing: NoteTreeIndent.iconGap) {
            icon
                .foregroundColor(.secondaryText)
                .frame(width: NoteTreeIndent.icon, height: NoteTreeIndent.icon)
            Text(title)
                .font(.fumi(.body, weight: .medium))
                .foregroundColor(.primaryText)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .padding(.horizontal, 10)
        .frame(height: 28)
        .frame(maxWidth: 220, alignment: .leading)
        .fixedSize()
        .background(
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .fill(Color.surface)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .strokeBorder(Color.borderColor, lineWidth: 1)
        )
    }
}

private struct TreeDropTarget: ViewModifier {
    let folderId: String
    let springLoads: Bool
    @Environment(NoteTreeState.self) var tree
    @Environment(FileNoteStore.self) var store
    @Environment(AppModel.self) var appModel

    func body(content: Content) -> some View {
        content.onDrop(of: [.url], delegate: TreeDropDelegate(
            folderId: folderId, tree: tree, store: store, appModel: appModel, springLoads: springLoads))
    }
}

private extension View {
    func treeDropTarget(_ folderId: String, springLoads: Bool = false) -> some View {
        modifier(TreeDropTarget(folderId: folderId, springLoads: springLoads))
    }

    /// The tint on a folder row (or the library heading) a drop would land in.
    func dropHighlight(_ isTargeted: Bool) -> some View {
        overlay(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(isTargeted ? Color.accentHover : Color.clear)
                .overlay(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .strokeBorder(isTargeted ? Color.accent.opacity(0.5) : Color.clear, lineWidth: 1)
                )
                .allowsHitTesting(false)
                .animation(.easeOut(duration: 0.12), value: isTargeted)
        )
    }
}

// MARK: - Bottom: Settings

struct SidebarSettingsRow: View {
    @Environment(AppModel.self) var appModel

    var body: some View {
        SidebarRow(icon: .symbol("slider.horizontal.3"), title: "Settings", isSelected: appModel.showSettings) {
            appModel.showSettings.toggle()
        }
    }
}

/// The sidebar button as a toolbar item: a flat icon, without the Liquid
/// Glass circle macOS 26 puts behind toolbar buttons. Alone: the sidebar's
/// part of the toolbar keeps only one button's room when the sidebar
/// collapses, and anything more goes into ».
struct SidebarToggleItem: ToolbarContent {
    let placement: ToolbarItemPlacement

    var body: some ToolbarContent {
        if #available(macOS 26, *) {
            ToolbarItem(placement: placement) { SidebarToggleButton().modifier(DimsWithWindow()) }
                .sharedBackgroundVisibility(.hidden)
        } else {
            ToolbarItem(placement: placement) { SidebarToggleButton().modifier(DimsWithWindow()) }
        }
    }
}

/// Shows / hides the sidebar; stands in for the system toggle (see
/// AppShellView for why that one is removed).
struct SidebarToggleButton: View {
    @Environment(AppModel.self) var appModel

    var body: some View {
        Button {
            appModel.toggleSidebar()
        } label: {
            Label {
                Text(appModel.isSidebarVisible ? "Hide Sidebar" : "Show Sidebar")
            } icon: {
                Image(nsImage: ShiyeGlyph.sidebar.templateImage())
            }
        }
        .help(appModel.isSidebarVisible ? "Hide Sidebar (⌥⌘S)" : "Show Sidebar (⌥⌘S)")
    }
}

/// Hooks the split view hosting the sidebar to add the ⑰ behaviors the
/// SwiftUI column API lacks: snap on release, haptic ticks at snap points,
/// double-click the divider to fit / reset, and a remembered width per
/// workspace. (Dragging past the minimum to collapse is the split view's own.)
private struct SidebarSplitHook: NSViewRepresentable {
    let workspaceKey: String?
    let contentWidth: () -> CGFloat
    let onCollapsedChange: (Bool) -> Void

    func makeNSView(context: Context) -> HookView {
        let view = HookView()
        view.contentWidth = contentWidth
        view.onCollapsedChange = onCollapsedChange
        view.workspaceKey = workspaceKey
        return view
    }

    func updateNSView(_ view: HookView, context: Context) {
        view.contentWidth = contentWidth
        view.onCollapsedChange = onCollapsedChange
        view.workspaceKey = workspaceKey
    }

    final class HookView: NSView {
        var contentWidth: () -> CGFloat = { SidebarWidthPolicy.standardDefault }
        var onCollapsedChange: (Bool) -> Void = { _ in }
        var workspaceKey: String? {
            didSet { if workspaceKey != oldValue { restoreSavedWidth() } }
        }

        private weak var splitView: NSSplitView?
        private var monitor: Any?
        private var observers: [NSObjectProtocol] = []
        private var isDraggingDivider = false
        private var lastWidth: CGFloat = 0
        /// A saved width that arrived while the sidebar was collapsed (a
        /// library opened from the welcome page starts that way); applied
        /// once it has expanded.
        private var pendingWidth: CGFloat?
        private var wasCollapsed = false

        deinit { detach() }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            detach()
            guard window != nil else { return }
            DispatchQueue.main.async { [weak self] in self?.attach() }
        }

        private func attach() {
            var view: NSView? = superview
            while let current = view, !(current is NSSplitView) { view = current.superview }
            guard let split = view as? NSSplitView else { return }
            splitView = split
            lastWidth = sidebarWidth
            enforceMinimum()
            removeTitlebarSeparators()

            observers.append(NotificationCenter.default.addObserver(
                forName: NSSplitView.didResizeSubviewsNotification, object: split, queue: .main) { [weak self] _ in
                    self?.didResize()
                })
            monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .leftMouseUp]) { [weak self] event in
                self?.handle(event) ?? event
            }
            restoreSavedWidth()
        }

        private func detach() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
            observers.forEach(NotificationCenter.default.removeObserver)
            observers = []
        }

        private var sidebarView: NSView? { splitView?.arrangedSubviews.first }
        private var sidebarWidth: CGFloat { sidebarView?.frame.width ?? 0 }
        private var isCollapsed: Bool {
            guard let split = splitView, let sidebar = sidebarView else { return true }
            return split.isSubviewCollapsed(sidebar) || sidebar.frame.width < 1
        }

        private func isOnDivider(_ event: NSEvent) -> Bool {
            guard event.window === window, let split = splitView, !isCollapsed else { return false }
            let point = split.convert(event.locationInWindow, from: nil)
            return split.bounds.contains(point) && abs(point.x - sidebarWidth) <= 5
        }

        private func handle(_ event: NSEvent) -> NSEvent? {
            switch event.type {
            case .leftMouseDown where isOnDivider(event):
                enforceMinimum()
                if event.clickCount == 2 {
                    let target = SidebarWidthPolicy.doubleClickTarget(current: sidebarWidth, contentWidth: contentWidth())
                    setWidth(target, save: true)
                    return nil
                }
                isDraggingDivider = true
            case .leftMouseUp where isDraggingDivider:
                isDraggingDivider = false
                DispatchQueue.main.async { [weak self] in self?.settle() }
            default:
                break
            }
            return event
        }

        private func didResize() {
            if isCollapsed != wasCollapsed {
                wasCollapsed = isCollapsed
                let collapsed = wasCollapsed
                DispatchQueue.main.async { [weak self] in self?.onCollapsedChange(collapsed) }
            }
            if !isCollapsed, let pending = pendingWidth {
                pendingWidth = nil
                // After the expand animation, so the two don't fight.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.55) { [weak self] in
                    self?.setWidth(pending, save: false)
                }
            }
            let width = sidebarWidth
            if isDraggingDivider, SidebarWidthPolicy.crossedSnapPoint(from: lastWidth, to: width) != nil {
                NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
            }
            lastWidth = width
        }

        private func settle() {
            guard !isCollapsed else { return }
            setWidth(SidebarWidthPolicy.settle(sidebarWidth), save: true)
        }

        /// No hard line under the title bar: the title and the page read as
        /// one surface, and text scrolling under the toolbar gets the system's
        /// soft edge instead.
        private func removeTitlebarSeparators() {
            window?.titlebarSeparatorStyle = .none
            (splitView?.delegate as? NSSplitViewController)?.splitViewItems.forEach {
                $0.titlebarSeparatorStyle = .none
            }
        }

        /// Keep the split view item's own minimum just under the narrowest
        /// width (positioning the divider at or under the item's minimum
        /// collapses it), whatever SwiftUI derived from the content.
        private func enforceMinimum() {
            guard let item = (splitView?.delegate as? NSSplitViewController)?.splitViewItems.first else { return }
            let floor = SidebarWidthPolicy.minimum - 8
            if item.minimumThickness != floor { item.minimumThickness = floor }
        }

        private func setWidth(_ width: CGFloat, save: Bool) {
            enforceMinimum()
            guard let split = splitView, !isCollapsed else { return }
            if abs(sidebarWidth - width) > 0.5 {
                NSAnimationContext.runAnimationGroup { context in
                    context.duration = 0.2
                    context.allowsImplicitAnimation = true
                    split.setPosition(width, ofDividerAt: 0)
                    split.layoutSubtreeIfNeeded()
                }
            }
            if save, let key = workspaceKey {
                UserDefaults.standard.set(Double(width), forKey: "fumi.sidebarWidth." + key)
            }
        }

        /// The folder's saved width, or the default for a folder opened for
        /// the first time — without one the split view keeps whatever width
        /// it started at (140 after the welcome page).
        private func restoreSavedWidth() {
            guard let key = workspaceKey else { return }
            let saved = UserDefaults.standard.object(forKey: "fumi.sidebarWidth." + key) as? Double
            // Widths saved while the icon rail existed (64) count as unset.
            let width = saved.map { CGFloat($0) }.flatMap { $0 >= SidebarWidthPolicy.minimum ? $0 : nil }
                ?? SidebarWidthPolicy.standardDefault
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                if self.isCollapsed { self.pendingWidth = width } else { self.setWidth(width, save: false) }
            }
        }
    }
}
