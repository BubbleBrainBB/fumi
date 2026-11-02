import SwiftUI

/// No folder open (artifacts/welcome-redesign): the app icon, the name, one
/// line, and Open Folder… — plus the folders opened before. When the last
/// session's folder can't be found, it says so instead and offers to find
/// it. The name is a placeholder until the product is named.
struct FolderOpenView: View {
    @Environment(FileNoteStore.self) var store
    @Environment(AppModel.self) var appModel
    @State private var appeared = false
    @State private var errorMessage: String?

    static let productName = "Fumi"

    var body: some View {
        Group {
            if let missing = store.missingWorkspace {
                MissingFolderPanel(url: missing,
                                   locate: { locate(missing) },
                                   openOther: openExistingFolder,
                                   newNotebook: startNewNotebook,
                                   reappeared: { open(missing) })
            } else {
                welcome
            }
        }
        .opacity(appeared ? 1 : 0)
        .animation(.easeOut(duration: 0.2), value: appeared)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.editorBg)
        .ignoresSafeArea()
        .navigationTitle("")
        // One surface up to the top edge: no toolbar strip, no separator line
        // under the traffic lights.
        .toolbarBackground(.hidden, for: .windowToolbar)
        .onAppear { appeared = true }
    }

    /// The brand set tight (16 / 4), the action a step below (28), recents a
    /// larger step below that (40); the group sits a little above center.
    private var welcome: some View {
        VStack(spacing: 0) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 64, height: 64)
                .accessibilityHidden(true)
            Text(Self.productName)
                .font(.fumi(.title, weight: .semibold))
                .foregroundColor(.primaryText)
                .padding(.top, 16)
            Text("Your notes, as plain files.")
                .font(.fumi(.body))
                .foregroundColor(.tertiaryText)
                .padding(.top, 4)

            Button(action: openExistingFolder) {
                HStack(spacing: 6) {
                    FolderIcon()
                    Text("Open Folder…")
                        .font(.fumi(.body, weight: .medium))
                        .foregroundColor(.primaryText)
                    KeyCaps(keys: ["⌘", "O"])
                        .padding(.leading, 6)
                }
                .padding(.horizontal, 10)
                .frame(height: 28)
            }
            .buttonStyle(HoverFillButtonStyle())
            .keyboardShortcut(.defaultAction)
            .padding(.top, 28)

            // Folders opened before; never shown on a first launch.
            let recents = RecentFolder.existing()
            if !recents.isEmpty {
                RecentFolderList(folders: recents, open: open)
                    .padding(.top, 40)
            }

            if let errorMessage {
                Text(errorMessage)
                    .font(.fumi(.caption))
                    .foregroundColor(.tertiaryText)
                    .padding(.top, 20)
            }
        }
        .padding(.bottom, 48)
    }

    /// Opens a library from this page, handing over to the main window with
    /// the frame-in transition.
    private func open(_ url: URL) {
        appModel.beginFrameIntro()
        store.openWorkspace(at: url)
    }

    /// The folder panel, starting where the missing folder used to be.
    private func locate(_ missing: URL) {
        guard let url = NotesFolderPanel.choose(startingAt: missing) else { return }
        open(url)
    }

    /// Makes (or reuses) Documents/Fumi and opens a first, blank note.
    private func startNewNotebook() {
        let fm = FileManager.default
        guard let documents = fm.urls(for: .documentDirectory, in: .userDomainMask).first else { return }
        let folder = documents.appendingPathComponent(Self.productName, isDirectory: true)
        var isDirectory: ObjCBool = false
        do {
            if !fm.fileExists(atPath: folder.path, isDirectory: &isDirectory) {
                try fm.createDirectory(at: folder, withIntermediateDirectories: true)
            } else if !isDirectory.boolValue {
                errorMessage = "A file named \(Self.productName) is already in Documents."
                return
            }
        } catch {
            errorMessage = "Couldn't create the folder: \(error.localizedDescription)"
            return
        }
        open(folder)
        appModel.openNote(store.createNote()?.id)
    }

    private func openExistingFolder() {
        guard let url = NotesFolderPanel.choose() else { return }
        open(url)
    }
}

/// The "Open Notes Folder" panel, shared by the empty state, the sidebar's
/// workspace menu and the ⌘O command.
enum NotesFolderPanel {
    /// `startingAt`: open the panel at this folder, or at its nearest parent
    /// that still exists.
    static func choose(startingAt start: URL? = nil) -> URL? {
        let panel = NSOpenPanel()
        if var dir = start {
            while dir.pathComponents.count > 1, !FileManager.default.fileExists(atPath: dir.path) {
                dir.deleteLastPathComponent()
            }
            panel.directoryURL = dir
        }
        panel.title = "Open Notes Folder"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Open"

        guard panel.runModal() == .OK else { return nil }
        return panel.url
    }
}

// MARK: - Recent folders

struct RecentFolder: Identifiable {
    let url: URL
    var id: String { url.path }
    var name: String { url.lastPathComponent }
    /// Where it lives, as Finder names it: "Documents", "iCloud Drive"…
    var location: String {
        FileManager.default.displayName(atPath: url.deletingLastPathComponent().path)
    }

    /// The recent folders that are still on disk, most recent first.
    static func existing() -> [RecentFolder] {
        AppSettings.recentWorkspaces
            .filter { FileManager.default.fileExists(atPath: $0) }
            .map { RecentFolder(url: URL(fileURLWithPath: $0)) }
    }
}

private struct RecentFolderList: View {
    let folders: [RecentFolder]
    let open: (URL) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("Recent")
                .font(.fumi(.caption, weight: .semibold))
                .foregroundColor(.faintText)
                .padding(.horizontal, 10)
                .padding(.bottom, 4)
            ForEach(folders) { folder in
                Button { open(folder.url) } label: {
                    HStack(spacing: 10) {
                        FolderIcon()
                        Text(folder.name)
                            .font(.fumi(.body))
                            .foregroundColor(.primaryText)
                            .lineLimit(1)
                        Spacer(minLength: 12)
                        Text(folder.location)
                            .font(.fumi(.caption))
                            .foregroundColor(.faintText)
                            .lineLimit(1)
                    }
                    .padding(.horizontal, 10)
                    .frame(height: 30)
                }
                .buttonStyle(HoverFillButtonStyle())
            }
        }
        .frame(width: 320)
    }
}

/// The sidebar's folder, in its quiet gray.
private struct FolderIcon: View {
    var body: some View {
        FolderGlyph(open: false)
            .stroke(Color.iconMuted, style: StrokeStyle(lineWidth: 1.15, lineJoin: .round))
            .frame(width: 16, height: 16)
    }
}

/// A shortcut as light key caps: a fill, no outline.
private struct KeyCaps: View {
    let keys: [String]

    var body: some View {
        HStack(spacing: 2) {
            ForEach(keys, id: \.self) { key in
                Text(key)
                    .font(.fumi(.caption))
                    .foregroundColor(.tertiaryText)
                    .padding(.horizontal, 4)
                    .frame(minWidth: 18, minHeight: 18)
                    .background(RoundedRectangle(cornerRadius: 4, style: .continuous).fill(Color.inlineFill))
            }
        }
        .accessibilityHidden(true)
    }
}

/// No fill until the pointer is over it, a deeper one while pressed.
private struct HoverFillButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Fill(configuration: configuration)
    }

    private struct Fill: View {
        let configuration: ButtonStyleConfiguration
        @State private var isHovering = false

        var body: some View {
            configuration.label
                .background(
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .fill(Color.primaryText.opacity(configuration.isPressed ? 0.09 : isHovering ? 0.05 : 0))
                )
                .contentShape(Rectangle())
                .onHover { isHovering = $0 }
                .animation(.easeOut(duration: 0.12), value: isHovering)
        }
    }
}

// MARK: - Missing folder

/// The last session's folder can't be found (design board ㉞ ④). On a disk
/// that isn't connected it waits, and opens the folder once the disk mounts;
/// otherwise it offers to find the folder where it went.
private struct MissingFolderPanel: View {
    let url: URL
    let locate: () -> Void
    let openOther: () -> Void
    let newNotebook: () -> Void
    /// The folder is back (its disk mounted): open it.
    let reappeared: () -> Void
    @State private var isHoveringLocate = false
    @State private var hovered: String?

    /// The disk's name when the folder lives on a volume that isn't mounted.
    private var unmountedVolume: String? {
        let parts = url.pathComponents
        guard parts.count > 2, parts[1] == "Volumes" else { return nil }
        return FileManager.default.fileExists(atPath: "/Volumes/" + parts[2]) ? nil : parts[2]
    }

    /// "Documents › My Notes", "iCloud Drive › My Notes", "T7 › Notes › My Notes".
    private var breadcrumb: String {
        var parts = url.pathComponents
        let home = FileManager.default.homeDirectoryForCurrentUser.pathComponents
        if parts.starts(with: home) {
            parts.removeFirst(home.count)
            let iCloud = ["Library", "Mobile Documents", "com~apple~CloudDocs"]
            if parts.starts(with: iCloud) { parts.replaceSubrange(0..<iCloud.count, with: ["iCloud Drive"]) }
        } else if parts.count > 2, parts[1] == "Volumes" {
            parts.removeFirst(2)
        } else {
            parts.removeFirst()
        }
        return parts.suffix(3).joined(separator: " › ")
    }

    var body: some View {
        let volume = unmountedVolume
        VStack(alignment: .leading, spacing: 0) {
            Image(systemName: volume == nil ? "questionmark.folder" : "externaldrive")
                .font(.system(size: 19, weight: .light))
                .foregroundColor(.iconMuted)
                .frame(width: 44, height: 44)
                .background(RoundedRectangle(cornerRadius: 11, style: .continuous).fill(Color.inlineFill))
                .padding(.bottom, 20)

            Text(BodyFace.text(volume == nil
                                ? "Can’t find “\(url.lastPathComponent)”"
                                : "“\(url.lastPathComponent)” is on a disk that isn’t connected",
                               fontName: "New York", size: TextSize.title.rawValue, weight: .semibold))
                .foregroundColor(.primaryText)
                .fixedSize(horizontal: false, vertical: true)
            Text(breadcrumb)
                .font(.fumi(.caption))
                .foregroundColor(.faintText)
                .padding(.top, 6)
            Text(volume.map { "It opens once “\($0)” is connected." } ?? "It may have been moved, renamed or deleted.")
                .font(.fumi(.body))
                .foregroundColor(.tertiaryText)
                .padding(.top, 12)

            if volume != nil {
                HStack(spacing: 8) {
                    Circle()
                        .fill(Color.fromHex("#E0A93B"))
                        .frame(width: 6, height: 6)
                        .background(Circle().fill(Color.fromHex("#E0A93B").opacity(0.18)).frame(width: 12, height: 12))
                    Text("Waiting for the disk…")
                        .font(.fumi(.caption))
                        .foregroundColor(.faintText)
                }
                .padding(.top, 14)
            }

            HStack(alignment: .firstTextBaseline, spacing: 24) {
                if volume == nil {
                    Button(action: locate) {
                        HStack(spacing: 6) {
                            Text("Locate…").font(.fumi(.body, weight: .medium))
                            Image(systemName: "arrow.right")
                                .font(.system(size: 11, weight: .medium))
                                .offset(x: isHoveringLocate ? 2 : 0)
                        }
                        .foregroundColor(.primaryText)
                        .padding(.bottom, 3)
                        .overlay(alignment: .bottom) { Rectangle().fill(Color.primaryText).frame(height: 1) }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .keyboardShortcut(.defaultAction)
                    .onHover { isHoveringLocate = $0 }
                    .animation(.easeOut(duration: 0.15), value: isHoveringLocate)
                }
                quietButton("Open Another Folder…", action: openOther)
                quietButton("New Notebook", action: newNotebook)
            }
            .padding(.top, 28)
        }
        .frame(width: 420, alignment: .leading)
        .onReceive(NSWorkspace.shared.notificationCenter.publisher(for: NSWorkspace.didMountNotification)) { _ in
            if FileManager.default.fileExists(atPath: url.path) { reappeared() }
        }
    }

    private func quietButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.fumi(.body))
                .foregroundColor(hovered == title ? .primaryText : .tertiaryText)
                .padding(.bottom, 4)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 ? title : (hovered == title ? nil : hovered) }
    }
}
