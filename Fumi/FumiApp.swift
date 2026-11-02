import SwiftUI

@main
struct FumiApp: App {
    @NSApplicationDelegateAdaptor(FumiAppDelegate.self) private var appDelegate
    // Theme is global across all windows; the per-window data lives in WindowRoot.
    @State private var themeManager = ThemeManager.shared

    init() {
        // Don't have macOS restore every window from the last session on relaunch
        // (which would resurrect extra/empty windows). Launch opens a single
        // window restored to the last folder; opening more windows is manual
        // (⇧⌘N).
        UserDefaults.standard.set(false, forKey: "NSQuitAlwaysKeepsWindows")
    }

    var body: some Scene {
        WindowGroup(id: "main") {
            WindowRoot()
                .environment(themeManager)
                .task { themeManager.applyAppearance() }
        }
        // A standard unified toolbar window, so macOS 26+ draws the native
        // Liquid Glass toolbar and floating sidebar.
        .windowToolbarStyle(.unified)
        // A fresh install's first window; later launches restore the last size.
        .defaultSize(width: 1200, height: 800)
        .commands { AppCommands() }
    }
}

/// One window's root. Each window owns its OWN store / app model, so different
/// windows can open different folders and be edited in parallel.
struct WindowRoot: View {
    @Environment(ThemeManager.self) private var themeManager
    @State private var store = FileNoteStore()
    @State private var appModel = AppModel()

    var body: some View {
        AppShellView()
            .background(WindowCloseBehavior(store: store, appModel: appModel).frame(width: 0, height: 0))
            .environment(store)
            .environment(appModel)
            // One accent everywhere: Fumi's own (Settings › Appearance),
            // not the system accent the native controls would otherwise pick.
            .tint(Color.accent)
            // Publish this window's store/model so menu commands act on the
            // focused window, not a shared global.
            .focusedSceneValue(\.noteStore, store)
            .focusedSceneValue(\.appModel, appModel)
            // Rebuild the shell (not the store) when the accent changes so the
            // accent-driven Color tokens are re-read everywhere. The @State above
            // lives on WindowRoot, which keeps its identity, so the open folder
            // and notes survive an accent switch.
            .modifier(ClearWindowContainer(isGlass: themeManager.isGlass))
            .id(themeManager.renderKey)
    }
}

// MARK: - Focused window values (so commands target the key window)

private struct NoteStoreFocusedKey: FocusedValueKey { typealias Value = FileNoteStore }
private struct AppModelFocusedKey: FocusedValueKey { typealias Value = AppModel }

extension FocusedValues {
    var noteStore: FileNoteStore? {
        get { self[NoteStoreFocusedKey.self] }
        set { self[NoteStoreFocusedKey.self] = newValue }
    }
    var appModel: AppModel? {
        get { self[AppModelFocusedKey.self] }
        set { self[AppModelFocusedKey.self] = newValue }
    }
}

// MARK: - Menu commands

struct AppCommands: Commands {
    @FocusedValue(\.noteStore) private var store
    @FocusedValue(\.appModel) private var appModel
    @Environment(\.openWindow) private var openWindow
    @AppStorage(AppSettings.wechatExtensionKey) private var wechatEnabled = false
    @AppStorage(AppSettings.xhsExtensionKey) private var xhsEnabled = false

    /// ⌥⌘C follows the preview: WeChat copies, Xiaohongshu adds the cards to Photos.
    private var exportsCards: Bool {
        guard xhsEnabled, let appModel else { return false }
        return appModel.publishTarget(for: appModel.selectedNoteId) == .xiaohongshu
    }

    var body: some Commands {
        CommandGroup(after: .newItem) {
            // New Note and New Tab are one thing: a blank draft in a new
            // tab, written to the library once something is typed (㉝ E, ㊴).
            Button("New Note") { appModel?.newTab() }
                .keyboardShortcut("n", modifiers: .command)
            Button("New Tab") { appModel?.newTab() }
                .keyboardShortcut("t", modifiers: .command)

            // The quick memo panel; ⌃⌥N opens it from anywhere (QuickCapture).
            Button("New Memo") { QuickCapture.shared.show() }

            Button("New Window") {
                openWindow(id: "main")
            }
            .keyboardShortcut("n", modifiers: [.command, .shift])

            Button("Open Folder…") {
                openFolderInFocusedWindow()
            }
            .keyboardShortcut("o", modifiers: .command)

            Divider()

            // ⌘W closes the tab, or the window on its last one:
            // WindowCloseController.
            Button("Reopen Closed Tab") {
                guard let store, let appModel else { return }
                appModel.reopenClosedTab { content in
                    switch content {
                    case .note(let id): store.notes.contains { $0.id == id }
                    case .file(let id): store.files.contains { $0.id == id }
                    case .draft, .memos: true
                    }
                }
            }
            .keyboardShortcut("t", modifiers: [.command, .shift])
            .disabled(appModel?.canReopenClosedTab != true)
        }

        CommandGroup(after: .windowArrangement) {
            Divider()
            Button("Show Next Tab") { appModel?.selectTab(offset: 1) }
                .keyboardShortcut(.tab, modifiers: .control)
                .disabled(appModel?.showsTabStrip != true)
            Button("Show Previous Tab") { appModel?.selectTab(offset: -1) }
                .keyboardShortcut(.tab, modifiers: [.control, .shift])
                .disabled(appModel?.showsTabStrip != true)
            Button(appModel?.activeTab.isPinned == true ? "Unpin Tab" : "Pin Tab") {
                guard let appModel else { return }
                appModel.togglePin(appModel.activeTabId)
            }
            // The tab's own right-click menu is swallowed by the system's
            // toolbar menu on macOS 26, so the shortcut is the way in.
            .keyboardShortcut("p", modifiers: [.command, .option])
            .disabled(appModel?.showsTabStrip != true)
        }

        CommandGroup(after: .importExport) {
            Button("Export All Notes…") {
                guard let store else { return }
                NoteExporter.exportAll(store.notes)
            }
            .disabled(store == nil)
            // The publishing extensions (Settings › Extensions).
            if wechatEnabled || xhsEnabled {
                Divider()
                Button(appModel?.isPublishPreviewOpen == true ? "Hide Preview" : "Show Preview") {
                    appModel?.togglePublishPreview()
                }
                .keyboardShortcut("p", modifiers: [.command, .control])
                .disabled(appModel?.selectedNoteId == nil)
                Button(exportsCards ? "Add Xiaohongshu Cards to Photos" : "Copy for WeChat") {
                    guard let appModel else { return }
                    if exportsCards {
                        Task { await XhsCards.addOrShowWhy(appModel) }
                    } else {
                        Task { await WeChatCopy.copyOrShowWhy(appModel) }
                    }
                }
                .keyboardShortcut("c", modifiers: [.command, .option])
                .disabled(appModel?.selectedNoteId == nil || !(exportsCards || wechatEnabled))
            }
        }

        CommandGroup(replacing: .appSettings) {
            Button("Settings…") { appModel?.showSettings = true }
                .keyboardShortcut(",", modifiers: .command)
                .disabled(appModel == nil)
        }

        CommandGroup(replacing: .sidebar) {
            Button("Toggle Sidebar") { appModel?.toggleSidebar() }
                .keyboardShortcut("s", modifiers: [.command, .option])
            Button("Back") { appModel?.goBack() }
                .keyboardShortcut("[", modifiers: .command)
                .disabled(appModel?.canGoBack != true)
            Button("Forward") { appModel?.goForward() }
                .keyboardShortcut("]", modifiers: .command)
                .disabled(appModel?.canGoForward != true)
            Button("Search Notes") {
                guard let appModel else { return }
                appModel.presentSearch()
            }
            .keyboardShortcut("f", modifiers: [.command, .shift])
            Divider()
            // The sidebar's first level, and ⌃⌘1–9 straight into a library
            // in the list's order (design board 60 B ④).
            Button("Show Libraries") { appModel?.showLibraries() }
                .disabled(store?.isOpen != true)
            Menu("Go to Library") {
                ForEach(Array(LibraryList.shared.entries.prefix(9).enumerated()), id: \.element.id) { index, entry in
                    Button(entry.displayName) {
                        guard let store, let appModel else { return }
                        appModel.enterLibrary(entry.url, store: store)
                    }
                    .keyboardShortcut(KeyEquivalent(Character("\(index + 1)")), modifiers: [.control, .command])
                }
            }
            .disabled(appModel == nil)
            Divider()
        }

        CommandMenu("Format") {
            // Send straight to the first responder (the focused window's text
            // view) so the action fires exactly once. Broadcasting via
            // NotificationCenter would hit every open window's editor.
            Button("Bold") { NSApp.sendAction(#selector(WysiwygTextView.toggleBold(_:)), to: nil, from: nil) }
                .keyboardShortcut("b", modifiers: .command)
            Button("Italic") { NSApp.sendAction(#selector(WysiwygTextView.toggleItalic(_:)), to: nil, from: nil) }
                .keyboardShortcut("i", modifiers: .command)
            Button("Insert Link") { NSApp.sendAction(#selector(WysiwygTextView.insertLink), to: nil, from: nil) }
                .keyboardShortcut("k", modifiers: .command)
            // ⌘L, as in Obsidian: ⇧⌘T reopens a closed tab.
            Button("Toggle Todo") { NSApp.sendAction(#selector(WysiwygTextView.toggleTodoOnCurrentLine), to: nil, from: nil) }
                .keyboardShortcut("l", modifiers: .command)
        }
    }

    /// Opens a folder in the currently focused window (replacing its workspace).
    /// If no window is focused, opens a new window for the chosen folder instead.
    private func openFolderInFocusedWindow() {
        guard let url = NotesFolderPanel.choose() else { return }
        if let store, let appModel {
            appModel.enterLibrary(url, store: store)
        } else {
            openWindow(id: "main")
        }
    }
}

extension Notification.Name {
    static let newNote = Notification.Name("fumi.newNote")
    static let formatBold = Notification.Name("fumi.formatBold")
    static let formatItalic = Notification.Name("fumi.formatItalic")
    static let formatLink = Notification.Name("fumi.formatLink")
    static let toggleTodo = Notification.Name("fumi.toggleTodo")
}
