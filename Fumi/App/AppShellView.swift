import SwiftUI

struct AppShellView: View {
    @Environment(AppModel.self) var appModel
    @Environment(FileNoteStore.self) var store
    @AppStorage("fumi.editorWidth") private var editorWidth: String = "medium"
    @AppStorage(AppSettings.wechatExtensionKey) private var wechatEnabled = false
    @AppStorage(AppSettings.xhsExtensionKey) private var xhsEnabled = false
    @State private var detailWidth: CGFloat = 1200

    static let sidebarWidth: CGFloat = 240
    /// Gap between the editor's page and the window edges.
    static let pageInset: CGFloat = 8
    static let pageRadius: CGFloat = 12

    /// Minimum width for the editor detail column: the capped text column plus
    /// its side insets. Enforced as the detail column's min and folded into the
    /// window min width so the text column never reflows when the sidebar
    /// collapses/expands — the pane stays at or above the cap engagement point.
    private var detailMinWidth: CGFloat {
        WysiwygTextView.columnWidth(for: editorWidth) + WysiwygTextView.minSideInset * 2 + Self.pageInset * 2
    }

    /// The publish preview beside the page — WeChat or Xiaohongshu
    /// (Settings › Extensions; boards ㊻ B, ㊾).
    private var showsWeChatPreview: Bool {
        (wechatEnabled || xhsEnabled) && appModel.isPublishPreviewOpen && appModel.selectedNoteId != nil && !appModel.showMemos
    }

    /// Too narrow for the page and the preview side by side: the preview
    /// lies over the page's right side instead of squeezing the text.
    private var previewOverlaps: Bool {
        detailWidth < detailMinWidth + PublishPreviewPane.width + Self.pageInset
    }

    private func previewSheet(_ shape: RoundedRectangle) -> some View {
        PublishPreviewPane()
            .frame(width: PublishPreviewPane.width)
            .clipShape(shape)
            .background {
                shape.fill(Color.pageSheet)
                    .shadow(color: .black.opacity(previewOverlaps ? 0.12 : 0.05), radius: previewOverlaps ? 16 : 1.5, y: 1)
            }
            .overlay { shape.strokeBorder(Color.pageEdge, lineWidth: 1).allowsHitTesting(false) }
            .transition(.move(edge: .trailing).combined(with: .opacity))
    }

    var body: some View {
        @Bindable var appModel = appModel
        let framed = appModel.frameRevealed
        let flat = ThemeManager.shared.theme == .white
        let pageShape = RoundedRectangle(cornerRadius: framed ? Self.pageRadius : 0, style: .continuous)
        Group {
            if !store.isOpen {
                FolderOpenView()
                    .transition(.opacity)
            } else {
                ZStack(alignment: .top) {
                    NavigationSplitView(columnVisibility: $appModel.columnVisibility) {
                        SidebarView()
                            // Draggable from 200 to 420; opens at 240; dragging
                            // narrower collapses it. Snapping: SidebarWidthPolicy /
                            // SidebarSplitHook. The minimum sits a little under 200:
                            // positioning the divider exactly at the minimum makes the
                            // split view collapse.
                            .navigationSplitViewColumnWidth(min: SidebarWidthPolicy.minimum - 8, ideal: Self.sidebarWidth,
                                                            max: SidebarWidthPolicy.max)
                            // Our own toggle instead (SidebarView adds it; it stays
                            // beside the traffic lights when the sidebar collapses).
                            .toolbar(removing: .sidebarToggle)
                    } detail: {
                        // The page floats on the window ground (design board ㉘ D6):
                        // the title and toolbar icons sit on the ground above it,
                        // and the page, inset 8 on the other three sides, holds
                        // only the text. The WeChat preview, when open, is a
                        // second sheet to its right.
                        HStack(spacing: Self.pageInset) {
                            Group {
                                if appModel.showMemos {
                                    MemosView()
                                } else if appModel.selectedNoteId == nil, let id = appModel.selectedFileId,
                                          let file = store.files.first(where: { $0.id == id }) {
                                    FilePreviewView(file: file)
                                } else {
                                    EditorContainerView()
                                }
                            }
                                .clipShape(pageShape)
                                .background {
                                    pageShape
                                        .fill(framed ? Color.pageSheet : Color.editorBg)
                                        .shadow(color: .black.opacity(framed && !flat ? 0.05 : 0), radius: 1.5, y: 1)
                                        .shadow(color: .black.opacity(framed && !flat ? 0.04 : 0), radius: 8, y: 4)
                                }
                                .overlay {
                                    pageShape
                                        .strokeBorder(Color.pageEdge.opacity(framed ? 1 : 0), lineWidth: 1)
                                        .allowsHitTesting(false)
                                }
                            if showsWeChatPreview && !previewOverlaps { previewSheet(pageShape) }
                        }
                            .overlay(alignment: .trailing) {
                                if showsWeChatPreview && previewOverlaps { previewSheet(pageShape) }
                            }
                            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { detailWidth = $0 }
                            // Opened from the welcome page, the page starts as the
                            // whole white window and settles into its frame.
                            .padding([.leading, .trailing, .bottom], framed ? Self.pageInset : 0)
                            // The tab strip spans the page, less the toolbar's
                            // trailing icons — and the traffic lights and the
                            // sidebar toggle where the page slides under them.
                            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frame in
                                appModel.pageFrame = frame
                            }
                            .background(appModel.groundColor)
                            .toolbarBackground(.hidden, for: .windowToolbar)
                        .navigationSplitViewColumnWidth(min: detailMinWidth, ideal: detailMinWidth, max: .infinity)
                    }
                    .navigationSplitViewStyle(.balanced)
                    .frame(minWidth: Self.sidebarWidth + detailMinWidth, minHeight: 600)

                    // Spotlight-style quick search, floating over the window.
                    if appModel.isSearchPresented {
                        Color.scrim
                            .ignoresSafeArea()
                            .onTapGesture { appModel.dismissSearch() }
                            .transition(.opacity)
                        SearchPopover()
                            .padding(.top, 92)
                            .transition(.opacity.combined(with: .scale(scale: 0.96, anchor: .top)))
                    }

                    // Settings: a sheet of paper over the dimmed window (design board ㊶).
                    if appModel.showSettings {
                        // Lighter behind Glass, so the note reads through the sheet.
                        Color.scrim.opacity(appModel.scrimStrength)
                            .ignoresSafeArea()
                            .onTapGesture { appModel.showSettings = false }
                            .transition(.opacity)
                        SettingsModal()
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                            .transition(.opacity.combined(with: .scale(scale: 0.97)))
                    }
                }
                // Glass: the window itself is see-through (board ㊳).
                .background {
                    if ThemeManager.shared.isGlass {
                        BehindWindowBlur().ignoresSafeArea()
                    }
                }
                .animation(.easeOut(duration: 0.16), value: appModel.isSearchPresented)
                .animation(.easeOut(duration: 0.16), value: appModel.showSettings)
                .transition(.opacity)
                .onAppear {
                    // Let the shell lay out as the plain sheet first, then frame it.
                    if !appModel.frameRevealed {
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) { appModel.revealFrame() }
                    }
                }
            }
        }
        .animation(.easeOut(duration: 0.2), value: store.isOpen)
        .background { TabShortcuts() }
        // Each library keeps its own tabs between launches (board ㊴).
        .onChange(of: store.workspaceURL, initial: true) { _, url in
            if let url { appModel.restoreTabs(workspace: url.path) }
        }
        .onChange(of: appModel.tabs) { saveTabs() }
        .onChange(of: appModel.activeTabId) { saveTabs() }
        // A note or file gone from the library takes its tabs with it.
        .onChange(of: store.notes.map(\.id)) { pruneTabs() }
        .onChange(of: store.files.map(\.id)) { pruneTabs() }
        .onChange(of: store.hasLoadedNotes) { pruneTabs() }
    }

    private func saveTabs() {
        guard let path = store.workspaceURL?.path else { return }
        appModel.saveTabs(workspace: path)
    }

    private func pruneTabs() {
        guard store.hasLoadedNotes else { return }
        let notes = Set(store.notes.map(\.id))
        let files = Set(store.files.map(\.id))
        appModel.pruneTabs { content in
            switch content {
            case .note(let id): notes.contains(id)
            case .file(let id): files.contains(id)
            case .draft, .memos: true
            }
        }
    }
}

extension AppModel {
    /// How strongly the window is dimmed behind a floating card: fully
    /// behind search; lighter behind Settings in Glass, so the note reads
    /// through the sheet; 0 with neither showing.
    var scrimStrength: Double {
        if showSettings { return ThemeManager.shared.isGlass ? 0.45 : 1 }
        return isSearchPresented ? 1 : 0
    }
}

/// Toolbar items dim with the window behind search and Settings. They sit
/// above the scrim, so the active tab stayed bright white over a dimmed
/// page; multiplying their colors by the scrim's shade darkens them exactly
/// as much as what lies under it, translucent parts included.
struct DimsWithWindow: ViewModifier {
    @Environment(AppModel.self) private var appModel

    func body(content: Content) -> some View {
        content
            .colorMultiply(Color.scrimShade(appModel.scrimStrength))
            .animation(.easeOut(duration: 0.16), value: appModel.scrimStrength)
    }
}

/// Tab shortcuts that have no menu item of their own: ⌘1…⌘9 and ⇧⌘] / ⇧⌘[
/// (the menu has ⌃Tab / ⌃⇧Tab for next and previous).
private struct TabShortcuts: View {
    @Environment(AppModel.self) private var appModel

    var body: some View {
        ZStack {
            ForEach(1...9, id: \.self) { number in
                Button("") { appModel.selectTab(number: number) }
                    .keyboardShortcut(KeyEquivalent(Character("\(number)")), modifiers: .command)
            }
            Button("") { appModel.selectTab(offset: 1) }
                .keyboardShortcut("]", modifiers: [.command, .shift])
            Button("") { appModel.selectTab(offset: -1) }
                .keyboardShortcut("[", modifiers: [.command, .shift])
        }
        .buttonStyle(.plain)
        .opacity(0)
        .frame(width: 0, height: 0)
        .accessibilityHidden(true)
    }
}

/// The Glass theme's window: whatever is behind it (desktop, other windows)
/// shows through. On macOS 26 it is native Liquid Glass (regular: clearly
/// see-through, blurred just enough to read on); earlier systems get the
/// sidebar's behind-window blur, kept active so it doesn't turn opaque gray
/// when the window loses focus. Either way the window draws no opaque
/// background of its own while it shows; leaving (another theme) puts it back.
private struct BehindWindowBlur: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let host = SeeThroughHostView()
        let backdrop: NSView
        if #available(macOS 26, *) {
            let glass = NSGlassEffectView()
            glass.style = .regular
            backdrop = glass
        } else {
            let blur = NSVisualEffectView()
            blur.material = .sidebar
            blur.blendingMode = .behindWindow
            blur.state = .active
            backdrop = blur
        }
        backdrop.frame = host.bounds
        backdrop.autoresizingMask = [.width, .height]
        host.addSubview(backdrop)
        return host
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}

private final class SeeThroughHostView: NSView {
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window else { return }
        window.isOpaque = false
        window.backgroundColor = .clear
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        if newWindow == nil, let window {
            window.isOpaque = true
            window.backgroundColor = .windowBackgroundColor
        }
        super.viewWillMove(toWindow: newWindow)
    }
}

/// Glass: SwiftUI paints no window background of its own (macOS 15+), so
/// the see-through blur under the shell isn't covered.
struct ClearWindowContainer: ViewModifier {
    let isGlass: Bool

    func body(content: Content) -> some View {
        if isGlass, #available(macOS 15, *) {
            content.containerBackground(.clear, for: .window)
        } else {
            content
        }
    }
}
