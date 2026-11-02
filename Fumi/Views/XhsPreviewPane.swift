import AppKit
import SwiftUI
import WebKit

/// The publish preview beside the page: the WeChat article or the
/// Xiaohongshu cards. Each side draws the one bar (board 62 A), where the
/// platform names switch between them; each note keeps its choice (㊾).
struct PublishPreviewPane: View {
    @Environment(AppModel.self) private var appModel

    static let width: CGFloat = 420

    var body: some View {
        switch appModel.publishTarget(for: appModel.selectedNoteId) {
        case .wechat: WeChatPreviewPane()
        case .xiaohongshu: XhsPreviewPane()
        }
    }
}

/// The Xiaohongshu side of the preview (boards ㊾ ①, 62 A①): the template
/// and Export in the bar, then the post in the order it's read — the cards
/// three across, its title, its caption — each editable in place.
struct XhsPreviewPane: View {
    @Environment(AppModel.self) private var appModel
    @Environment(FileNoteStore.self) private var store
    // Read so the cards redraw when Settings changes.
    @AppStorage(AppSettings.xhsTemplateKey) private var defaultTemplate = XhsCards.Template.memo.rawValue
    @AppStorage(AppSettings.xhsRatioKey) private var defaultRatio = XhsCards.Ratio.portrait.rawValue
    @AppStorage(AppSettings.xhsPageNumbersKey) private var pageNumbers = true
    @AppStorage(AppSettings.xhsNameKey) private var name = ""
    @AppStorage(AppSettings.xhsHandleKey) private var handle = ""
    @AppStorage(AppSettings.xhsAvatarKey) private var avatar = ""
    @AppStorage(AppSettings.xhsEndCardKey) private var endCard = false
    @AppStorage(AppSettings.xhsEndLineKey) private var endLine = ""

    @State private var choices = XhsCards.NoteChoices()
    @State private var choicesNote: String?
    @State private var deck: XhsDeck?
    @State private var layoutWarnings: [String] = []
    @State private var cardCount = 0
    @State private var cardsHeight: CGFloat = 0
    @State private var status: String?
    @State private var isWorking = false
    @State private var exportAnchor: NSView?
    /// The warnings the reader hid; new ones show again.
    @State private var hiddenWarnings: [String]?

    private var noteId: String? { appModel.previewNote?.noteId }

    /// Read in the body so that a change in Settings lays the cards out again.
    private var settings: String {
        "\(defaultTemplate)|\(defaultRatio)|\(pageNumbers)|\(name)|\(handle)|\(avatar)|\(endCard)|\(endLine)"
    }

    var body: some View {
        let _ = settings
        VStack(spacing: 0) {
            toolbar
            notice
            if let deck {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        XhsCardsView(input: XhsCards.input(for: deck, choices: choices), onLayout: { count, warnings, height in
                            cardCount = count
                            layoutWarnings = warnings
                            if height > 0 { cardsHeight = height }
                        }, menu: cardMenu)
                        .frame(height: max(cardsHeight, 160))
                        postFields(deck)
                    }
                }
            } else {
                Spacer()
            }
        }
        .task(id: appModel.previewNote) { rebuild() }
        .onChange(of: choices) { _, value in
            if let noteId, noteId == choicesNote { XhsCards.setChoices(value, for: noteId) }
        }
        .onReceive(NotificationCenter.default.publisher(for: XhsCards.statusNotification)) { notification in
            if let message = notification.object as? String { flash(message) }
        }
    }

    private func rebuild() {
        guard let note = appModel.previewNote else { deck = nil; return }
        if choicesNote != note.noteId {
            choices = XhsCards.choices(for: note.noteId)
            choicesNote = note.noteId
        }
        deck = XhsCards.deck(for: note, choices: choices)
    }

    // MARK: - Header

    private var template: XhsCards.Template { XhsCards.template(for: choices) }
    private var ratio: XhsCards.Ratio { XhsCards.ratio(for: choices) }

    private var warnings: [String] { (deck?.warnings ?? []) + layoutWarnings }

    private var toolbar: some View {
        PublishToolbar {
            if !warnings.isEmpty && hiddenWarnings == warnings { NoticeDot { hiddenWarnings = nil } }
            PopUpButton(help: "Template and card size", menu: templateMenu) {
                TemplateGlyph(template: template)
                Text("\(template.name) · \(ratio.rawValue)")
            }
            Button {
                showExportMenu()
            } label: {
                HStack(spacing: 5) {
                    Text(isWorking ? "Exporting…" : "Export")
                    Image(systemName: "chevron.down").font(.system(size: 8, weight: .semibold))
                }
            }
            .buttonStyle(PaneButtonStyle())
            .disabled(deck == nil || isWorking)
            .background(AnchorView(view: $exportAnchor))
            .help("Export the cards (⌥⌘C adds them to Photos)")
        }
    }

    private func templateMenu() -> NSMenu {
        let menu = NSMenu()
        menu.addItem(.sectionHeader(title: "Template"))
        for option in XhsCards.Template.allCases {
            let item = ClosureMenuItem(option.name) {
                choices.template = option == XhsCards.template(for: XhsCards.NoteChoices()) ? nil : option.rawValue
                rebuild()
            }
            item.state = option == template ? .on : .off
            menu.addItem(item)
        }
        menu.addItem(.separator())
        menu.addItem(.sectionHeader(title: "Card Size"))
        for option in XhsCards.Ratio.allCases {
            let item = ClosureMenuItem(option == .portrait ? "3:4 Portrait" : "1:1 Square") {
                choices.ratio = option == XhsCards.ratio(for: XhsCards.NoteChoices()) ? nil : option.rawValue
                rebuild()
            }
            item.state = option == ratio ? .on : .off
            menu.addItem(item)
        }
        return menu
    }

    @ViewBuilder
    private var notice: some View {
        let warnings = warnings
        let shown = hiddenWarnings == warnings ? [] : warnings
        let leftOut = deck?.leftOutCount ?? 0
        if status != nil || !shown.isEmpty || leftOut > 0 {
            PaneNotice(header: shown.isEmpty ? nil : PaneNotice<EmptyView>.header(shown.count, before: "posting"),
                       items: shown,
                       hide: shown.isEmpty ? nil : { hiddenWarnings = warnings }) {
                if leftOut > 0 {
                    HStack(spacing: 8) {
                        Text(leftOut == 1 ? "1 part left out of the cards." : "\(leftOut) parts left out of the cards.")
                            .foregroundColor(.secondaryText)
                        QuietLink(title: "Put Back") {
                            choices.leftOut = []
                            rebuild()
                        }
                    }
                }
                if let status {
                    Text(status).foregroundColor(.primaryText)
                }
            }
        }
    }

    /// The post's title and caption under the cards, as Xiaohongshu shows
    /// them. Changing them here doesn't change the note.
    private func postFields(_ deck: XhsDeck) -> some View {
        let title = XhsCards.title(deck, choices)
        let caption = XhsCards.caption(deck, choices)
        return VStack(alignment: .leading, spacing: 16) {
            PostField(label: "Title", count: XhsCards.length(title), limit: 20,
                      reset: choices.title == nil ? nil : { choices.title = nil }) {
                TextField(deck.title, text: Binding(
                    get: { title },
                    set: { choices.title = $0 == deck.title ? nil : $0 }
                ), axis: .vertical)
                .textFieldStyle(.plain)
                .font(.fumi(.emphasis, weight: .semibold))
                .foregroundColor(.primaryText)
                .help("The post's title. Changing it here doesn't change the note.")
            }
            PostField(label: "Caption", count: XhsCards.length(caption), limit: 1000,
                      reset: choices.caption == nil ? nil : { choices.caption = nil }) {
                GrowingTextEditor(text: Binding(
                    get: { caption },
                    set: { choices.caption = $0 == deck.caption ? nil : $0 }
                ))
            }
        }
        .padding(.horizontal, 22)
        .padding(.top, 18)
        .padding(.bottom, 28)
    }

    // MARK: - Export

    private func showExportMenu() {
        let count = max(cardCount, 1)
        let menu = NSMenu()
        let photos = ClosureMenuItem("Add \(count) Images to Photos") { export(.photos) }
        photos.keyEquivalent = "c"
        photos.keyEquivalentModifierMask = [.command, .option]
        menu.addItem(photos)
        menu.addItem(ClosureMenuItem("AirDrop…") { export(.airDrop) })
        menu.addItem(ClosureMenuItem("Save Images to Folder…") { export(.folder) })
        menu.addItem(.separator())
        menu.addItem(ClosureMenuItem("Copy Caption") { copyCaption() })
        menu.addItem(ClosureMenuItem("Copy Title") { copyTitle() })
        menu.popUp(below: exportAnchor)
    }

    private func copyCaption() {
        guard let deck else { return }
        XhsCards.copyText(XhsCards.caption(deck, choices))
        flash("Caption copied.")
    }

    private func copyTitle() {
        guard let deck else { return }
        XhsCards.copyText(XhsCards.title(deck, choices))
        flash("Title copied.")
    }

    private enum Destination { case photos, airDrop, folder }

    private func export(_ destination: Destination) {
        guard let deck, !isWorking else { return }
        let input = XhsCards.input(for: deck, choices: choices)
        let name = XhsCards.title(deck, choices)
        let caption = XhsCards.caption(deck, choices)
        let library = store.workspaceURL
        // Ask where first, so the cards render only once there's a place.
        var folder: URL?
        if destination == .folder {
            guard let chosen = XhsCards.chooseFolder(noteName: name, library: library) else { return }
            folder = chosen
        }
        isWorking = true
        Task { @MainActor in
            defer { isWorking = false }
            do {
                let files = try await XhsCards.renderFiles(input, name: name)
                switch destination {
                case .photos:
                    try await XhsCards.addToPhotos(files)
                    XhsCards.copyText(caption)
                    flash("\(files.count) images added to Photos. Caption copied.")
                case .airDrop:
                    if XhsCards.airDrop(files) {
                        XhsCards.copyText(caption)
                        flash("Caption copied.")
                    } else {
                        flash("AirDrop isn't available on this Mac.")
                    }
                case .folder:
                    guard let folder else { return }
                    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                    for file in files {
                        let target = folder.appendingPathComponent(file.lastPathComponent)
                        try? FileManager.default.removeItem(at: target)
                        try FileManager.default.copyItem(at: file, to: target)
                    }
                    NSWorkspace.shared.activateFileViewerSelecting([folder])
                    flash("\(files.count) images saved to \(folder.lastPathComponent).")
                }
            } catch {
                flash(error.localizedDescription)
            }
        }
    }

    private func flash(_ message: String) {
        withAnimation(.easeOut(duration: 0.15)) { status = message }
        Task {
            try? await Task.sleep(for: .seconds(4))
            if status == message { withAnimation(.easeOut(duration: 0.2)) { status = nil } }
        }
    }

    // MARK: - A card's menu

    private func cardMenu(index: Int, ids: [String]) -> NSMenu {
        let menu = NSMenu()
        menu.addItem(ClosureMenuItem("Copy Image") { cardImage(index) { XhsCards.copyImage($0); flash("Image copied.") } })
        menu.addItem(ClosureMenuItem("Save Image…") {
            cardImage(index) { png in
                let panel = NSSavePanel()
                panel.allowedContentTypes = [.png]
                panel.nameFieldStringValue = String(format: "%02d.png", index + 1)
                guard panel.runModal() == .OK, let url = panel.url else { return }
                do { try png.write(to: url) } catch { flash(error.localizedDescription) }
            }
        })
        if !ids.isEmpty {
            menu.addItem(.separator())
            menu.addItem(ClosureMenuItem("Leave Out of Cards") {
                choices.leftOut = Array(Set(choices.leftOut).union(ids)).sorted()
                rebuild()
            })
        }
        return menu
    }

    private func cardImage(_ index: Int, then use: @escaping (Data) -> Void) {
        guard let deck else { return }
        let input = XhsCards.input(for: deck, choices: choices)
        Task { @MainActor in
            do {
                if let png = try await XhsRenderer.shared.render(input, only: index).first { use(png) }
            } catch {
                flash(error.localizedDescription)
            }
        }
    }
}

// MARK: - Parts

/// A template's paper in miniature, beside its name in the bar.
private struct TemplateGlyph: View {
    let template: XhsCards.Template

    var body: some View {
        let paper: UInt32 = switch template {
        case .memo, .post: 0xFFFFFF
        case .grid: 0xFBFBF8
        case .postDark: 0x1C1C1E
        }
        RoundedRectangle(cornerRadius: 2, style: .continuous)
            .fill(Color(nsColor: .adaptive(light: paper, dark: paper)))
            .overlay(alignment: .top) {
                if template == .memo { Rectangle().fill(Color(nsColor: .adaptive(light: 0xFFE070, dark: 0xFFE070))).frame(height: 3) }
            }
            .clipShape(RoundedRectangle(cornerRadius: 2, style: .continuous))
            .overlay { RoundedRectangle(cornerRadius: 2, style: .continuous).strokeBorder(Color.black.opacity(0.18), lineWidth: 0.5) }
            .frame(width: 9, height: 12)
    }
}

/// A label and its count over a value: the post's title, its caption.
private struct PostField<Value: View>: View {
    let label: String
    let count: Int
    let limit: Int
    var reset: (() -> Void)?
    @ViewBuilder let value: Value

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(label)
                Spacer(minLength: 8)
                if let reset { QuietLink(title: "Reset", action: reset) }
                Text(count > limit ? "\(count) / \(limit)" : "\(count)")
                    .monospacedDigit()
                    .foregroundColor(count > limit ? Color(nsColor: .adaptive(light: 0xC8701A, dark: 0xE89A4A)) : .tertiaryText)
                    .help(count > limit ? "Xiaohongshu takes \(limit) characters at most" : "Characters")
            }
            .font(.fumi(.caption))
            .foregroundColor(.tertiaryText)
            value
        }
    }
}

/// A text editor as tall as its text, so the pane's own scroll carries it.
private struct GrowingTextEditor: View {
    @Binding var text: String

    var body: some View {
        ZStack(alignment: .topLeading) {
            // Sizes the editor: TextEditor itself won't grow with its text.
            Text(text + " ")
                .font(.fumi(.body))
                .lineSpacing(4)
                .padding(.horizontal, 5)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .opacity(0)
                .accessibilityHidden(true)
            TextEditor(text: $text)
                .font(.fumi(.body))
                .lineSpacing(4)
                .foregroundColor(.primaryText)
                .scrollContentBackground(.hidden)
                .scrollDisabled(true)
        }
        .padding(.horizontal, -5)
    }
}

/// The cards, three across, on a web view running cards.js, as tall as
/// they are: the pane scrolls them with the title and caption under them. A new input lays
/// them out again; so does a new width.
private struct XhsCardsView: NSViewRepresentable {
    let input: XhsCards.Input
    let onLayout: (Int, [String], CGFloat) -> Void
    let menu: (Int, [String]) -> NSMenu

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> CardsWebView {
        let configuration = XhsAssets.configuration()
        configuration.userContentController.add(context.coordinator, name: "xhs")
        let view = CardsWebView(frame: .zero, configuration: configuration)
        view.setValue(false, forKey: "drawsBackground")
        view.navigationDelegate = context.coordinator
        view.onWidthChange = { [weak coordinator = context.coordinator] in coordinator?.relayout() }
        context.coordinator.webView = view
        view.load(URLRequest(url: XhsAssets.pageURL))
        return view
    }

    func updateNSView(_ view: CardsWebView, context: Context) {
        context.coordinator.onLayout = onLayout
        context.coordinator.menu = menu
        context.coordinator.show(input)
    }

    static func dismantleNSView(_ view: CardsWebView, coordinator: Coordinator) {
        view.configuration.userContentController.removeScriptMessageHandler(forName: "xhs")
    }

    final class CardsWebView: WKWebView {
        var onWidthChange: (() -> Void)?
        private var lastWidth: CGFloat = 0

        override func setFrameSize(_ newSize: NSSize) {
            super.setFrameSize(newSize)
            if abs(newSize.width - lastWidth) > 0.5 {
                lastWidth = newSize.width
                onWidthChange?()
            }
        }

        // The cards' own menu replaces the web view's.
        override func willOpenMenu(_ menu: NSMenu, with event: NSEvent) { menu.removeAllItems() }

        // The view is as tall as the cards; the pane around it scrolls.
        override func scrollWheel(with event: NSEvent) { nextResponder?.scrollWheel(with: event) }
    }

    @MainActor
    final class Coordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
        weak var webView: WKWebView?
        var onLayout: ((Int, [String], CGFloat) -> Void)?
        var menu: ((Int, [String]) -> NSMenu)?
        private var isLoaded = false
        private var shown: XhsCards.Input?
        private var pending: XhsCards.Input?
        private var isLayingOut = false

        func show(_ input: XhsCards.Input) {
            guard input != shown || pending != nil else { return }
            pending = input
            run()
        }

        func relayout() {
            guard let shown, pending == nil else { return }
            pending = shown
            run()
        }

        /// One layout at a time; the newest input waits for the one running.
        private func run() {
            guard isLoaded, !isLayingOut, let input = pending, let webView else { return }
            pending = nil
            shown = input
            isLayingOut = true
            Task { @MainActor in
                do {
                    let result = try await webView.callAsyncJavaScript("return await FumiXhs.layout(JSON.parse(json))",
                                                                       arguments: ["json": input.json], in: nil, contentWorld: .page)
                    let object = result as? [String: Any] ?? [:]
                    onLayout?(object["count"] as? Int ?? 0, object["warnings"] as? [String] ?? [],
                              CGFloat(object["height"] as? Double ?? 0))
                } catch {
                    let detail = (error as NSError).userInfo["WKJavaScriptExceptionMessage"] as? String ?? error.localizedDescription
                    onLayout?(0, ["The cards couldn't be laid out: \(detail)"], 0)
                }
                isLayingOut = false
                run()
            }
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            isLoaded = true
            run()
        }

        /// Links never open inside the preview.
        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                     decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
            decisionHandler(action.navigationType == .linkActivated ? .cancel : .allow)
        }

        func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
            guard let body = message.body as? [String: Any] else { return }
            // The cards laid out again once their images arrived.
            if body["type"] as? String == "layout" {
                onLayout?(body["count"] as? Int ?? 0, body["warnings"] as? [String] ?? [],
                          CGFloat(body["height"] as? Double ?? 0))
                return
            }
            guard body["type"] as? String == "menu", let index = body["index"] as? Int, let webView, let menu else { return }
            let ids = body["ids"] as? [String] ?? []
            let x = body["x"] as? Double ?? 0
            let y = body["y"] as? Double ?? 0
            let point = NSPoint(x: x, y: webView.isFlipped ? y : webView.bounds.height - y)
            menu(index, ids).popUp(positioning: nil, at: point, in: webView)
            webView.evaluateJavaScript("FumiXhs.menuClosed()")
        }
    }
}
