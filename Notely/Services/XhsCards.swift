import AppKit
import Photos
import WebKit

/// The Xiaohongshu Cards extension (design boards ㊾, 51, 52): a note laid
/// out as image cards by Resources/Xiaohongshu/cards.js, shown in the
/// publish preview and exported as 1080-pixel-wide PNGs — to Photos (which
/// iCloud brings to the phone), by AirDrop, or to a folder.
enum XhsCards {
    enum Template: String, CaseIterable, Identifiable {
        case memo, grid, post, postDark

        var id: String { rawValue }

        var name: String {
            switch self {
            case .memo: "Memo"
            case .grid: "Grid"
            case .post: "Post"
            case .postDark: "Post Dark"
            }
        }
    }

    enum Ratio: String, CaseIterable, Identifiable {
        case portrait = "3:4"
        case square = "1:1"

        var id: String { rawValue }
    }

    /// Exported cards are this many pixels wide.
    static let exportWidth: CGFloat = 1080

    // MARK: - Each note's own choices

    /// What the preview changed for one note: its template and size, the
    /// post's title and caption when edited, and the blocks left out.
    struct NoteChoices: Codable, Equatable {
        var template: String?
        var ratio: String?
        var title: String?
        var caption: String?
        var leftOut: [String] = []
    }

    private static func choicesKey(_ noteId: String) -> String { "notely.xhs.note." + noteId }

    static func choices(for noteId: String) -> NoteChoices {
        UserDefaults.standard.data(forKey: choicesKey(noteId))
            .flatMap { try? JSONDecoder().decode(NoteChoices.self, from: $0) } ?? NoteChoices()
    }

    static func setChoices(_ choices: NoteChoices, for noteId: String) {
        if choices == NoteChoices() {
            UserDefaults.standard.removeObject(forKey: choicesKey(noteId))
        } else if let data = try? JSONEncoder().encode(choices) {
            UserDefaults.standard.set(data, forKey: choicesKey(noteId))
        }
    }

    static func template(for choices: NoteChoices) -> Template {
        choices.template.flatMap(Template.init(rawValue:))
            ?? Template(rawValue: UserDefaults.standard.string(forKey: AppSettings.xhsTemplateKey) ?? "") ?? .memo
    }

    static func ratio(for choices: NoteChoices) -> Ratio {
        choices.ratio.flatMap(Ratio.init(rawValue:))
            ?? Ratio(rawValue: UserDefaults.standard.string(forKey: AppSettings.xhsRatioKey) ?? "") ?? .portrait
    }

    // MARK: - Layout input

    struct Author: Encodable, Equatable {
        var name: String
        var handle: String
        var avatar: String?
    }

    struct EndCard: Encodable, Equatable {
        var on: Bool
        var line: String
    }

    /// What cards.js lays out.
    struct Input: Encodable, Equatable {
        var deck: XhsDeck
        var template: String
        var ratio: String
        var pageNumbers: Bool
        var author: Author
        var endCard: EndCard
        var accent: String
        var mode = "preview"

        var json: String {
            (try? JSONEncoder().encode(self)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
        }
    }

    static var author: Author {
        let defaults = UserDefaults.standard
        var handle = (defaults.string(forKey: AppSettings.xhsHandleKey) ?? "").trimmingCharacters(in: .whitespaces)
        if !handle.isEmpty, !handle.hasPrefix("@") { handle = "@" + handle }
        let name = (defaults.string(forKey: AppSettings.xhsNameKey) ?? "").trimmingCharacters(in: .whitespaces)
        let avatar = defaults.string(forKey: AppSettings.xhsAvatarKey)
            .flatMap { FileManager.default.fileExists(atPath: $0) ? XhsDeck.assetURL(forFile: $0) : nil }
        return Author(name: name.isEmpty ? NSFullUserName() : name, handle: handle, avatar: avatar)
    }

    static func deck(for note: AppModel.NoteText, choices: NoteChoices) -> XhsDeck {
        XhsDeck.make(markdown: note.markdown, notePath: note.path,
                     fileName: (note.path as NSString).lastPathComponent, leftOut: Set(choices.leftOut))
    }

    static func input(for deck: XhsDeck, choices: NoteChoices) -> Input {
        let defaults = UserDefaults.standard
        return Input(deck: deck, template: template(for: choices).rawValue, ratio: ratio(for: choices).rawValue,
                     pageNumbers: defaults.object(forKey: AppSettings.xhsPageNumbersKey) as? Bool ?? true,
                     author: author,
                     endCard: EndCard(on: defaults.bool(forKey: AppSettings.xhsEndCardKey),
                                      line: defaults.string(forKey: AppSettings.xhsEndLineKey) ?? ""),
                     accent: AppSettings.accentColorHex)
    }

    /// The post's title and caption: the ones edited in the preview, else
    /// the note's own.
    static func title(_ deck: XhsDeck, _ choices: NoteChoices) -> String { choices.title ?? deck.title }
    static func caption(_ deck: XhsDeck, _ choices: NoteChoices) -> String { choices.caption ?? deck.caption }

    /// Characters, as the preview counts them for the title and caption.
    static func length(_ text: String) -> Int { text.count }

    // MARK: - Export

    struct ExportError: LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    /// The cards as PNG files in a folder of their own under the temporary
    /// directory, named 01.png, 02.png, …
    @MainActor
    static func renderFiles(_ input: Input, name: String) async throws -> [URL] {
        let images = try await XhsRenderer.shared.render(input)
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("Fumi Xiaohongshu", isDirectory: true)
            .appendingPathComponent(safeName(name), isDirectory: true)
        try? FileManager.default.removeItem(at: folder)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return try write(images, to: folder)
    }

    static func write(_ images: [Data], to folder: URL) throws -> [URL] {
        try images.enumerated().map { index, data in
            let url = folder.appendingPathComponent(String(format: "%02d.png", index + 1))
            try data.write(to: url)
            return url
        }
    }

    static func safeName(_ name: String) -> String {
        let cleaned = name.components(separatedBy: CharacterSet(charactersIn: "/:\\?%*|\"<>")).joined(separator: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return cleaned.isEmpty ? "Cards" : String(cleaned.prefix(80))
    }

    /// Adds the cards to Photos in order, one second apart, so they sort the
    /// way they read. Asks only to add: Notely never sees the library.
    static func addToPhotos(_ files: [URL]) async throws {
        let status = await PHPhotoLibrary.requestAuthorization(for: .addOnly)
        guard status == .authorized || status == .limited else {
            throw ExportError(message: "Fumi can't add to Photos. Allow it in System Settings › Privacy & Security › Photos.")
        }
        let start = Date()
        try await PHPhotoLibrary.shared().performChanges {
            for (index, url) in files.enumerated() {
                let request = PHAssetCreationRequest.forAsset()
                request.creationDate = start.addingTimeInterval(Double(index))
                request.addResource(with: .photo, fileURL: url, options: nil)
            }
        }
    }

    @MainActor
    static func airDrop(_ files: [URL]) -> Bool {
        guard let service = NSSharingService(named: .sendViaAirDrop), service.canPerform(withItems: files) else { return false }
        service.perform(withItems: files)
        return true
    }

    /// Asks where to save, suggesting attachments/xhs/<note> in the library;
    /// nil if the person cancels.
    @MainActor
    static func chooseFolder(noteName: String, library: URL?) -> URL? {
        let panel = NSSavePanel()
        panel.title = "Save Cards"
        panel.prompt = "Save"
        panel.message = "The cards are saved as 01.png, 02.png, … in a folder with this name."
        panel.nameFieldStringValue = safeName(noteName)
        panel.canCreateDirectories = true
        if let library {
            let fileManager = FileManager.default
            let candidates = [library.appendingPathComponent("attachments/xhs"), library.appendingPathComponent("attachments"), library]
            panel.directoryURL = candidates.first { fileManager.fileExists(atPath: $0.path) }
        }
        guard panel.runModal() == .OK, let url = panel.url else { return nil }
        return url
    }

    static func copyText(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    static func copyImage(_ png: Data) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setData(png, forType: .png)
        if let image = NSImage(data: png), let tiff = image.tiffRepresentation { pasteboard.setData(tiff, forType: .tiff) }
    }

    /// ⌥⌘C with the preview on Xiaohongshu: the cards into Photos and the
    /// caption onto the clipboard. Says what happened, for the preview.
    @MainActor
    static func addOpenNoteToPhotos(_ appModel: AppModel) async -> Result<String, Error> {
        guard let note = appModel.typedNote ?? appModel.previewNote, note.noteId == appModel.selectedNoteId else {
            return .failure(ExportError(message: "Open a note first."))
        }
        let choices = choices(for: note.noteId)
        let deck = deck(for: note, choices: choices)
        do {
            let files = try await renderFiles(input(for: deck, choices: choices), name: title(deck, choices))
            try await addToPhotos(files)
            copyText(caption(deck, choices))
            return .success("\(files.count) images added to Photos. Caption copied.")
        } catch {
            return .failure(error)
        }
    }

    /// What the preview shows after an export it didn't start (⌥⌘C).
    static let statusNotification = Notification.Name("notely.xhs.status")

    /// Adds the cards to Photos; if that fails, beeps and opens the preview,
    /// which says why.
    @MainActor
    static func addOrShowWhy(_ appModel: AppModel) async {
        let result = await addOpenNoteToPhotos(appModel)
        switch result {
        case .success(let message):
            NotificationCenter.default.post(name: statusNotification, object: message)
        case .failure(let error):
            NSSound.beep()
            if !appModel.isPublishPreviewOpen { appModel.togglePublishPreview() }
            NotificationCenter.default.post(name: statusNotification, object: error.localizedDescription)
        }
    }
}

// MARK: - Assets

/// `notely-xhs://` for the card view: `app/…` is the layout in the bundle,
/// `local/<path>` an image file on this Mac (a note's image, the avatar).
final class XhsAssets: NSObject, WKURLSchemeHandler {
    static let scheme = "notely-xhs"
    private static let imageTypes: [String: String] = [
        "png": "image/png", "jpg": "image/jpeg", "jpeg": "image/jpeg", "gif": "image/gif", "webp": "image/webp",
        "heic": "image/heic", "tif": "image/tiff", "tiff": "image/tiff", "bmp": "image/bmp", "svg": "image/svg+xml",
    ]
    private static let appTypes = ["html": "text/html", "css": "text/css", "js": "text/javascript"]

    static var pageURL: URL { URL(string: "\(scheme)://app/cards.html")! }

    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        guard let url = task.request.url else { return }
        let path = url.path.removingPercentEncoding ?? url.path
        var file: URL?
        var type: String?
        if url.host == "app", let folder = Bundle.main.url(forResource: "Xiaohongshu", withExtension: nil) {
            let name = (path as NSString).lastPathComponent
            file = folder.appendingPathComponent(name)
            type = Self.appTypes[(name as NSString).pathExtension]
        } else if url.host == "local" {
            type = Self.imageTypes[(path as NSString).pathExtension.lowercased()]
            if type != nil { file = URL(fileURLWithPath: path) }
        }
        guard let file, let type, let data = try? Data(contentsOf: file) else {
            task.didFailWithError(URLError(.fileDoesNotExist))
            return
        }
        task.didReceive(URLResponse(url: url, mimeType: type, expectedContentLength: data.count, textEncodingName: nil))
        task.didReceive(data)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {}

    static func configuration() -> WKWebViewConfiguration {
        let configuration = WKWebViewConfiguration()
        configuration.setURLSchemeHandler(XhsAssets(), forURLScheme: scheme)
        configuration.suppressesIncrementalRendering = true
        return configuration
    }
}

// MARK: - Rendering

/// Renders cards to PNG off screen: the layout at full size in a web view
/// in a window nobody sees, snapshotted card by card at 1080 pixels wide.
@MainActor
final class XhsRenderer: NSObject, WKNavigationDelegate {
    static let shared = XhsRenderer()

    private let window: NSWindow
    private let webView: WKWebView
    private var loaded = false
    private var waiting: [CheckedContinuation<Void, Never>] = []

    private override init() {
        webView = WKWebView(frame: NSRect(x: 0, y: 0, width: 300, height: 400), configuration: XhsAssets.configuration())
        webView.setValue(false, forKey: "drawsBackground")
        window = NSWindow(contentRect: NSRect(x: -20000, y: -20000, width: 300, height: 400),
                          styleMask: [.borderless], backing: .buffered, defer: false)
        super.init()
        window.isReleasedWhenClosed = false
        window.ignoresMouseEvents = true
        window.isExcludedFromWindowsMenu = true
        window.collectionBehavior = [.transient, .ignoresCycle, .stationary]
        window.contentView = webView
        webView.navigationDelegate = self
        webView.load(URLRequest(url: XhsAssets.pageURL))
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        loaded = true
        waiting.forEach { $0.resume() }
        waiting = []
    }

    private func ready() async {
        guard !loaded else { return }
        await withCheckedContinuation { waiting.append($0) }
    }

    /// Every card, or only the one at `index`, as PNG data.
    func render(_ input: XhsCards.Input, only index: Int? = nil) async throws -> [Data] {
        await ready()
        var input = input
        input.mode = "export"
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        let result = try await webView.callAsyncJavaScript("return await NotelyXhs.layout(JSON.parse(json))",
                                                           arguments: ["json": input.json], in: nil, contentWorld: .page)
        guard let object = result as? [String: Any], let rects = object["rects"] as? [[String: Double]],
              let height = object["height"] as? Double else {
            throw XhsCards.ExportError(message: "The cards couldn't be laid out.")
        }
        let size = NSSize(width: 300, height: max(400, ceil(height)))
        window.setContentSize(size)
        webView.frame = NSRect(origin: .zero, size: size)
        // Let the resized page paint before the snapshots.
        try? await Task.sleep(for: .milliseconds(120))

        var images: [Data] = []
        for (position, rect) in rects.enumerated() where index == nil || index == position {
            let frame = CGRect(x: rect["x"] ?? 0, y: rect["y"] ?? 0, width: rect["width"] ?? 300, height: rect["height"] ?? 400)
            let configuration = WKSnapshotConfiguration()
            configuration.rect = webView.isFlipped ? frame : CGRect(x: frame.minX, y: size.height - frame.maxY,
                                                                    width: frame.width, height: frame.height)
            let scale = window.backingScaleFactor
            configuration.snapshotWidth = NSNumber(value: Double(XhsCards.exportWidth / scale))
            let image = try await webView.takeSnapshot(configuration: configuration)
            images.append(try Self.png(image, width: Int(XhsCards.exportWidth),
                                       height: Int((XhsCards.exportWidth * frame.height / frame.width).rounded())))
        }
        return images
    }

    /// `image` redrawn at exactly `width` × `height` pixels.
    private static func png(_ image: NSImage, width: Int, height: Int) throws -> Data {
        guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
                                            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else {
            throw XhsCards.ExportError(message: "The card couldn't be drawn.")
        }
        bitmap.size = NSSize(width: width, height: height)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
        NSGraphicsContext.current?.imageInterpolation = .high
        image.draw(in: NSRect(x: 0, y: 0, width: width, height: height))
        NSGraphicsContext.restoreGraphicsState()
        guard let sRGB = bitmap.converting(to: .sRGB, renderingIntent: .default),
              let data = sRGB.representation(using: .png, properties: [:]) else {
            throw XhsCards.ExportError(message: "The card couldn't be saved as PNG.")
        }
        return data
    }
}
