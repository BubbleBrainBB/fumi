import AppKit
import Foundation
import ImageIO

struct AnimatedImageFrame {
    let image: NSImage
    let duration: TimeInterval
}

struct LoadedMarkdownImage {
    let image: NSImage
    let frames: [AnimatedImageFrame]

    var isAnimated: Bool {
        frames.count > 1
    }
}

/// A text attachment that displays an image inline in the text view.
final class ImageTextAttachment: NSTextAttachment, MarkdownBackedAttachment {
    let markdownSource: String
    private let frames: [AnimatedImageFrame]
    /// The image as it was decoded, kept so the attachment can be re-made at
    /// another size (see `scaled(toFitHeight:)`).
    private let content: LoadedMarkdownImage
    /// The alt text drawn under the image.
    private let caption: String
    /// The width the image was laid out for.
    private let maxWidth: CGFloat
    private var currentFrameIndex = 0
    private var animationTimer: Timer?
    /// The animated image's display width; the bounds are wider when the
    /// attachment fills the line (see `fillsLine`).
    private var frameWidth: CGFloat = 0

    static let cornerRadius: CGFloat = 8
    static let captionHeight: CGFloat = 26

    /// - Parameter fillsLine: Reserve the full column width so the image never
    ///   shares a line with text — used while its source is revealed in front
    ///   of it (live preview), so the image always starts on the next line.
    init(content: LoadedMarkdownImage, markdownSource: String, caption: String = "", maxWidth: CGFloat = 680,
         fillsLine: Bool = false) {
        self.markdownSource = markdownSource
        // Images are already downsampled at decode time (off the main thread via
        // ImageIO), so attachment creation only computes display bounds — no
        // main-thread lockFocus redraw, which used to cost ~60ms per image.
        self.frames = content.frames
        self.content = content
        self.caption = caption
        self.maxWidth = maxWidth
        super.init(data: nil, ofType: nil)

        if content.isAnimated {
            let size = Self.displaySize(for: frames.first?.image.size ?? .zero, maxWidth: maxWidth)
            frameWidth = size.width
            image = frames.first?.image
            bounds = NSRect(origin: .zero, size: NSSize(width: fillsLine ? max(size.width, maxWidth) : size.width,
                                                        height: size.height))
            startAnimation()
        } else {
            // Static images are wrapped in a lazily drawn image that clips to
            // rounded corners and draws the alt text underneath as a caption.
            let size = Self.displaySize(for: content.image.size, maxWidth: maxWidth)
            let trimmed = caption.trimmingCharacters(in: .whitespacesAndNewlines)
            let source = content.image
            let total = NSSize(width: fillsLine ? max(size.width, maxWidth) : size.width,
                               height: size.height + (trimmed.isEmpty ? 0 : Self.captionHeight))
            image = NSImage(size: total, flipped: true) { _ in
                let imageRect = NSRect(origin: .zero, size: size)
                NSGraphicsContext.saveGraphicsState()
                NSBezierPath(roundedRect: imageRect, xRadius: Self.cornerRadius, yRadius: Self.cornerRadius).addClip()
                source.draw(in: imageRect, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
                NSGraphicsContext.restoreGraphicsState()
                if !trimmed.isEmpty {
                    let para = NSMutableParagraphStyle()
                    para.alignment = .center
                    para.lineBreakMode = .byTruncatingTail
                    (trimmed as NSString).draw(
                        in: NSRect(x: 0, y: size.height + 7, width: size.width, height: Self.captionHeight - 7),
                        withAttributes: [
                            .font: NSFont.systemFont(ofSize: TextSize.caption.rawValue),
                            .foregroundColor: MarkdownPalette.tertiary,
                            .paragraphStyle: para,
                        ])
                }
                return true
            }
            bounds = NSRect(origin: .zero, size: total)
        }
    }

    /// A copy of this image scaled to fit within `maxHeight` — the PDF export
    /// uses it so a tall screenshot is shrunk instead of running off the page.
    /// The image is re-made at the smaller size, so it stays sharp.
    ///
    /// Returns nil when the image already fits.
    func scaled(toFitHeight maxHeight: CGFloat) -> ImageTextAttachment? {
        let natural = content.image.size
        guard natural.width > 0, natural.height > 0 else { return nil }
        let shown = Self.displaySize(for: natural, maxWidth: maxWidth)
        guard shown.height > maxHeight else { return nil }
        // The caption sits under the image and takes its own room.
        let room = maxHeight - (caption.isEmpty ? 0 : Self.captionHeight)
        guard room > 0 else { return nil }
        let width = shown.width * room / shown.height
        guard width > 0, width < shown.width - 0.5 else { return nil }
        return ImageTextAttachment(content: content, markdownSource: markdownSource,
                                   caption: caption, maxWidth: width, fillsLine: false)
    }

    deinit {
        animationTimer?.invalidate()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func image(
        forBounds imageBounds: NSRect,
        textContainer: NSTextContainer?,
        characterIndex charIndex: Int
    ) -> NSImage? {
        if frames.count > 1 {
            let frame = frames[currentFrameIndex].image
            guard imageBounds.width > frameWidth + 0.5 else { return frame }
            // Filling the line: draw the frame at its own size on the left.
            let frameWidth = frameWidth
            return NSImage(size: imageBounds.size, flipped: true) { _ in
                frame.draw(in: NSRect(x: 0, y: 0, width: frameWidth, height: imageBounds.height),
                           from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
                return true
            }
        }
        return super.image(forBounds: imageBounds, textContainer: textContainer, characterIndex: charIndex)
    }

    private func startAnimation() {
        guard frames.count > 1 else { return }
        scheduleNextFrame()
    }

    private func scheduleNextFrame() {
        animationTimer?.invalidate()
        let duration = max(frames[currentFrameIndex].duration, 0.02)
        let timer = Timer(timeInterval: duration, repeats: false) { [weak self] _ in
            guard let self else { return }
            self.currentFrameIndex = (self.currentFrameIndex + 1) % self.frames.count
            self.image = self.frames[self.currentFrameIndex].image
            NotificationCenter.default.post(name: .animatedImageFrameDidChange, object: self)
            self.scheduleNextFrame()
        }
        animationTimer = timer
        RunLoop.main.add(timer, forMode: .common)
    }

    /// Display size in points, capped to `maxWidth`. Cheap arithmetic only — the
    /// pixel downsampling already happened in `ImageLoader.decodeImage`.
    private static func displaySize(for size: NSSize, maxWidth: CGFloat) -> NSSize {
        guard size.width > 0, size.height > 0 else { return size }
        let ratio = size.height / size.width
        let width = min(size.width, maxWidth)
        return NSSize(width: width, height: width * ratio)
    }
}

/// Manages asynchronous image loading and caching for the editor.
final class ImageLoader {
    static let shared = ImageLoader()

    private var cache: [String: LoadedMarkdownImage] = [:]
    private var loading: Set<String> = []
    private var failedUntil: [String: Date] = [:]
    private(set) var workspaceURL: URL?
    private(set) var baseURL: URL?
    private let session: URLSession = {
        let config = URLSessionConfiguration.default
        // timeoutIntervalForRequest is the max gap between bytes; resource is the
        // ceiling for the whole transfer. The old 30s resource cap was below the
        // download time of multi-MB images (e.g. a 3.5 MB GIF over a ~150 KB/s
        // link takes ~25s), so they timed out and fell back to showing the raw
        // link. Give large/slow images room to finish.
        config.timeoutIntervalForRequest = 30
        config.timeoutIntervalForResource = 120
        // The previous `.ephemeral` config had diskCapacity == 0, so every app
        // relaunch (and any in-memory eviction) re-downloaded every image over
        // the network. A persistent on-disk URLCache serves repeat views in
        // single-digit milliseconds instead of hundreds.
        let cachesDir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
        let cacheURL = cachesDir?.appendingPathComponent("FumiImageCache", isDirectory: true)
        config.urlCache = URLCache(memoryCapacity: 32 * 1024 * 1024,
                                   diskCapacity: 512 * 1024 * 1024,
                                   directory: cacheURL)
        config.requestCachePolicy = .returnCacheDataElseLoad
        config.httpMaximumConnectionsPerHost = 8
        return URLSession(configuration: config)
    }()

    /// Diagnostics: on-disk byte capacity of the HTTP cache (0 = no disk cache).
    var diskCacheCapacity: Int { session.configuration.urlCache?.diskCapacity ?? 0 }

    func setWorkspaceURL(_ url: URL?) {
        workspaceURL = url
        embedIndex = nil
    }

    // MARK: - Obsidian embeds

    /// File name (lowercased) → file in the workspace, for `![[name]]` embeds.
    /// Built lazily on the first embed and dropped when the workspace changes.
    private var embedIndex: [String: URL]?

    /// Resolves an Obsidian-style embed target (`![[clip.mp4]]`,
    /// `![[img.gif|300]]`, `![[sub/a.png]]`) to a local file: first as a path
    /// relative to the note or workspace, then by file name anywhere in the
    /// workspace, the way Obsidian finds attachments.
    func resolveEmbed(_ target: String) -> URL? {
        var name = target
        if let cut = name.firstIndex(where: { $0 == "|" || $0 == "#" }) { name = String(name[..<cut]) }
        name = name.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return nil }

        let direct = resolveLocalPath(name)
        if FileManager.default.fileExists(atPath: direct.path) { return direct }

        if embedIndex == nil { embedIndex = buildEmbedIndex() }
        return embedIndex?[(name as NSString).lastPathComponent.lowercased()]
    }

    private func buildEmbedIndex() -> [String: URL] {
        guard let workspaceURL,
              let enumerator = FileManager.default.enumerator(
                at: workspaceURL, includingPropertiesForKeys: [.isRegularFileKey],
                options: [.skipsHiddenFiles, .skipsPackageDescendants]) else { return [:] }
        var index: [String: URL] = [:]
        for case let url as URL in enumerator where url.pathExtension.lowercased() != "md" {
            let key = url.lastPathComponent.lowercased()
            // Prefer the shallowest match, like Obsidian's shortest-path rule.
            if let existing = index[key], existing.pathComponents.count <= url.pathComponents.count { continue }
            index[key] = url
        }
        return index
    }

    func setBaseURL(_ url: URL?) {
        baseURL = url
    }

    /// Load an image synchronously if possible (local files), returns nil for remote URLs.
    func loadSync(urlString: String) -> LoadedMarkdownImage? {
        let key = normalizedKey(urlString)
        if let cached = cache[key] { return cached }

        // Local file path
        if !isRemoteURL(urlString) {
            let resolvedURL = resolveLocalPath(urlString)
            if FileManager.default.fileExists(atPath: resolvedURL.path) {
                if let data = try? Data(contentsOf: resolvedURL),
                   let loaded = decodeImage(data) {
                    cache[key] = loaded
                    return loaded
                }
            }
            log("local image not found: \(resolvedURL.path)")
            return nil
        }

        // Remote: check cache only (actual loading is async)
        return cache[key]
    }

    /// Load an image asynchronously (for remote URLs).
    func loadAsync(urlString: String, completion: @escaping (LoadedMarkdownImage?) -> Void) {
        let key = normalizedKey(urlString)

        if let cached = cache[key] {
            completion(cached)
            return
        }

        if let retryDate = failedUntil[key], retryDate > Date() {
            completion(nil)
            return
        }

        guard isRemoteURL(key), let url = makeRemoteURL(from: key) else {
            log("invalid remote image url: \(urlString)")
            markFailed(key)
            completion(nil)
            return
        }

        guard !loading.contains(key) else {
            return
        }

        loading.insert(key)

        var request = URLRequest(url: url)
        request.setValue("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15", forHTTPHeaderField: "User-Agent")
        request.setValue("image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8", forHTTPHeaderField: "Accept")
        // Feishu/Lark hosts require a matching Referer, but sending a Feishu
        // Referer to an unrelated 图床 can trigger hotlink throttling or 403s, so
        // scope it to Feishu/Lark domains only.
        if let host = url.host?.lowercased(),
           host.contains("feishu") || host.contains("larksuite") || host.contains("larkoffice") {
            request.setValue("https://www.feishu.cn/", forHTTPHeaderField: "Referer")
        }

        session.dataTask(with: request) { [weak self] data, response, error in
            guard let self else { return }

            if let error {
                DispatchQueue.main.async {
                    self.loading.remove(key)
                    // A timeout / dropped connection is transient — retry soon
                    // instead of hiding the image (showing the raw link) for the
                    // full 5-minute cooldown that hard failures get.
                    self.markFailed(key, retryAfter: Self.isTransient(error) ? 15 : 300)
                    self.log("remote image request failed: \(key), error=\(error.localizedDescription)")
                    completion(nil)
                }
                return
            }

            if let http = response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
                DispatchQueue.main.async {
                    self.loading.remove(key)
                    // 408/429/5xx are server-side transient; 403/404 are not.
                    let transient = http.statusCode == 408 || http.statusCode == 429 || (500...599).contains(http.statusCode)
                    self.markFailed(key, retryAfter: transient ? 30 : 300)
                    self.log("remote image HTTP \(http.statusCode): \(key)")
                    completion(nil)
                }
                return
            }

            guard let data, let img = decodeImage(data) else {
                let mime = response?.mimeType ?? "unknown"
                let size = data?.count ?? 0
                DispatchQueue.main.async {
                    self.loading.remove(key)
                    self.markFailed(key)
                    self.log("remote image decode failed: \(key), mime=\(mime), bytes=\(size)")
                    completion(nil)
                }
                return
            }

            DispatchQueue.main.async {
                self.cache[key] = img
                self.failedUntil.removeValue(forKey: key)
                self.loading.remove(key)
                completion(img)
            }
        }.resume()
    }

    /// Maximum pixel dimension produced by decode. 500pt display width × 2 for
    /// Retina, with headroom — keeps memory bounded and decode fast while staying
    /// crisp. Originals from a 图床 are commonly 3000–4000px, which is wasteful to
    /// keep in memory and slow to draw.
    static let decodeMaxPixelSize: CGFloat = 1200

    private func decodeImage(_ data: Data) -> LoadedMarkdownImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else {
            return nil
        }

        let count = CGImageSourceGetCount(source)
        if count > 1 {
            var frames: [AnimatedImageFrame] = []
            for index in 0..<count {
                guard let cgImage = downsampledImage(source, at: index) else { continue }
                let image = NSImage(cgImage: cgImage, size: NSSize(width: cgImage.width, height: cgImage.height))
                frames.append(AnimatedImageFrame(image: image, duration: frameDuration(from: source, at: index)))
            }
            if let first = frames.first {
                return LoadedMarkdownImage(image: first.image, frames: frames)
            }
        }

        guard let cgImage = downsampledImage(source, at: 0) else {
            return nil
        }
        let image = NSImage(cgImage: cgImage, size: NSSize(width: cgImage.width, height: cgImage.height))
        return LoadedMarkdownImage(image: image, frames: [])
    }

    /// Decode and downsample in a single ImageIO pass. This runs on the network
    /// completion thread (off the main thread) and is far faster than the old
    /// main-thread `NSImage.lockFocus` resize.
    private func downsampledImage(_ source: CGImageSource, at index: Int) -> CGImage? {
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: ImageLoader.decodeMaxPixelSize,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, index, options as CFDictionary)
    }

    private func frameDuration(from source: CGImageSource, at index: Int) -> TimeInterval {
        guard let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any],
              let gifProperties = properties[kCGImagePropertyGIFDictionary] as? [CFString: Any] else {
            return 0.1
        }

        let unclamped = gifProperties[kCGImagePropertyGIFUnclampedDelayTime] as? TimeInterval
        let clamped = gifProperties[kCGImagePropertyGIFDelayTime] as? TimeInterval
        let duration = unclamped ?? clamped ?? 0.1
        return duration < 0.02 ? 0.1 : duration
    }

    private func isRemoteURL(_ path: String) -> Bool {
        let value = normalizedKey(path).lowercased()
        return value.hasPrefix("http://") || value.hasPrefix("https://")
    }

    private func cleanPath(_ path: String) -> String {
        var value = path.trimmingCharacters(in: .whitespacesAndNewlines)
        if value.hasPrefix("<"), value.hasSuffix(">") {
            value = String(value.dropFirst().dropLast())
        }
        return value.removingPercentEncoding ?? value
    }

    private func normalizedKey(_ path: String) -> String {
        var value = path.trimmingCharacters(in: .whitespacesAndNewlines)
        if value.hasPrefix("<"), value.hasSuffix(">") {
            value = String(value.dropFirst().dropLast())
        }
        value = value
            .replacingOccurrences(of: "&amp;", with: "&")
            .replacingOccurrences(of: "\u{00a0}", with: "")
        if value.lowercased().hasPrefix("http://") || value.lowercased().hasPrefix("https://") {
            value = value.components(separatedBy: .whitespacesAndNewlines).joined()
        }
        return value
    }

    private func makeRemoteURL(from string: String) -> URL? {
        if let url = URL(string: string) { return url }
        var allowed = CharacterSet.urlQueryAllowed
        allowed.insert(charactersIn: ":/#[]@!$&'()*+,;=%")
        guard let encoded = string.addingPercentEncoding(withAllowedCharacters: allowed) else { return nil }
        return URL(string: encoded)
    }

    private func resolveLocalPath(_ rawPath: String) -> URL {
        let path = cleanPath(rawPath)

        if path.lowercased().hasPrefix("file://"), let url = URL(string: path) {
            return url
        }

        // Absolute path
        if path.hasPrefix("/") {
            return URL(fileURLWithPath: path)
        }

        // Expand ~ paths
        if path.hasPrefix("~") {
            let expanded = NSString(string: path).expandingTildeInPath
            return URL(fileURLWithPath: expanded)
        }

        // Prefer resolving relative to the current note's folder.
        if let baseURL {
            let noteRelative = baseURL.appendingPathComponent(path)
            if FileManager.default.fileExists(atPath: noteRelative.path) {
                return noteRelative
            }
        }

        // Then resolve relative to workspace root.
        if let workspace = workspaceURL {
            return workspace.appendingPathComponent(path)
        }

        return URL(fileURLWithPath: path)
    }

    func clearCache() {
        cache.removeAll()
        failedUntil.removeAll()
    }

    private func markFailed(_ key: String, retryAfter seconds: TimeInterval = 300) {
        failedUntil[key] = Date().addingTimeInterval(seconds)
    }

    /// Whether a URL load error is transient (worth retrying soon) rather than a
    /// hard failure like a bad URL or a 404.
    private static func isTransient(_ error: Error) -> Bool {
        guard let urlError = error as? URLError else { return false }
        switch urlError.code {
        case .timedOut, .networkConnectionLost, .cannotConnectToHost,
             .cannotFindHost, .dnsLookupFailed, .notConnectedToInternet,
             .resourceUnavailable, .requestBodyStreamExhausted:
            return true
        default:
            return false
        }
    }

    private func log(_ message: String) {
        let line = "[ImageLoader] \(message)\n"
        FileHandle.standardError.write(line.data(using: .utf8) ?? Data())
        let path = "\(NSHomeDirectory())/fumi_image_debug.log"
        guard let data = line.data(using: .utf8) else { return }
        if FileManager.default.fileExists(atPath: path),
           let handle = FileHandle(forWritingAtPath: path) {
            handle.seekToEndOfFile()
            handle.write(data)
            handle.closeFile()
        } else {
            FileManager.default.createFile(atPath: path, contents: data)
        }
    }
}
