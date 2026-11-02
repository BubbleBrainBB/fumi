import AppKit
import AVFoundation

/// File extensions rendered as video cards.
enum VideoFile {
    static let extensions: Set<String> = ["mp4", "mov", "m4v", "webm"]

    static func isVideo(_ name: String) -> Bool {
        extensions.contains((name as NSString).pathExtension.lowercased())
    }
}

/// First-frame posters and durations for local videos. Generated off the main
/// thread; posts `.imageDidLoad` when one is ready so the editor restyles.
final class VideoPosterCache {
    static let shared = VideoPosterCache()

    struct Info {
        let poster: NSImage?
        let duration: TimeInterval
    }

    private var cache: [URL: Info] = [:]
    private var pending: Set<URL> = []

    func info(for url: URL) -> Info? {
        if let info = cache[url] { return info }
        guard !pending.contains(url) else { return nil }
        pending.insert(url)

        let asset = AVURLAsset(url: url)
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 1400, height: 1400)
        Task {
            let duration = (try? await asset.load(.duration)).map(CMTimeGetSeconds) ?? 0
            let cgImage = try? await generator.image(at: CMTime(seconds: min(0.5, max(0, duration / 2)), preferredTimescale: 600)).image
            let poster = cgImage.map { NSImage(cgImage: $0, size: NSSize(width: $0.width, height: $0.height)) }
            await MainActor.run {
                self.cache[url] = Info(poster: poster, duration: duration.isFinite ? duration : 0)
                self.pending.remove(url)
                NotificationCenter.default.post(name: .imageDidLoad, object: nil)
            }
        }
        return nil
    }
}

/// A local video shown as a poster card (first frame, play button, duration,
/// file name). Clicking it plays the video inline (see `WysiwygTextView`).
final class VideoAttachment: NSTextAttachment, MarkdownBackedAttachment {
    let markdownSource: String
    let fileURL: URL
    /// The poster area within the attachment's bounds; the file name sits
    /// below it. The inline player is placed over this rect.
    private(set) var videoRect: NSRect = .zero

    static let captionHeight: CGFloat = 26

    init(fileURL: URL, markdownSource: String, maxWidth: CGFloat) {
        self.fileURL = fileURL
        self.markdownSource = markdownSource
        super.init(data: nil, ofType: nil)

        let info = VideoPosterCache.shared.info(for: fileURL)
        let poster = info?.poster
        let aspect: CGFloat = {
            guard let size = poster?.size, size.width > 0, size.height > 0 else { return 9.0 / 16.0 }
            return min(size.height / size.width, 1.25)
        }()
        let width = max(1, maxWidth)
        let videoSize = NSSize(width: width, height: (width * aspect).rounded())
        videoRect = NSRect(origin: .zero, size: videoSize)
        let total = NSSize(width: width, height: videoSize.height + Self.captionHeight)
        let name = fileURL.lastPathComponent
        let duration = info?.duration ?? 0

        image = NSImage(size: total, flipped: true) { _ in
            let rect = NSRect(origin: .zero, size: videoSize)
            NSGraphicsContext.saveGraphicsState()
            NSBezierPath(roundedRect: rect, xRadius: 8, yRadius: 8).addClip()
            NSColor(white: 0.12, alpha: 1).setFill()
            rect.fill()
            if let poster {
                poster.draw(in: Self.aspectFit(poster.size, in: rect), from: .zero, operation: .sourceOver,
                            fraction: 1, respectFlipped: true, hints: nil)
            }
            NSColor.black.withAlphaComponent(0.12).setFill()
            rect.fill(using: .sourceOver)
            NSGraphicsContext.restoreGraphicsState()

            // Play button.
            let d: CGFloat = 52
            let circle = NSRect(x: rect.midX - d / 2, y: rect.midY - d / 2, width: d, height: d)
            NSColor.black.withAlphaComponent(0.45).setFill()
            NSBezierPath(ovalIn: circle).fill()
            let tri = NSBezierPath()
            tri.move(to: NSPoint(x: circle.midX - 7, y: circle.midY - 11))
            tri.line(to: NSPoint(x: circle.midX + 12, y: circle.midY))
            tri.line(to: NSPoint(x: circle.midX - 7, y: circle.midY + 11))
            tri.close()
            NSColor.white.setFill()
            tri.fill()

            // Duration pill.
            if duration > 0 {
                let text = Self.format(duration) as NSString
                let attrs: [NSAttributedString.Key: Any] = [
                    .font: NSFont.monospacedDigitSystemFont(ofSize: TextSize.caption.rawValue, weight: .medium),
                    .foregroundColor: NSColor.white,
                ]
                let size = text.size(withAttributes: attrs)
                let pill = NSRect(x: rect.minX + 10, y: rect.maxY - 10 - size.height - 4,
                                  width: size.width + 10, height: size.height + 4)
                NSColor.black.withAlphaComponent(0.6).setFill()
                NSBezierPath(roundedRect: pill, xRadius: 4, yRadius: 4).fill()
                text.draw(at: NSPoint(x: pill.minX + 5, y: pill.minY + 2), withAttributes: attrs)
            }

            // File name caption.
            let para = NSMutableParagraphStyle()
            para.alignment = .center
            para.lineBreakMode = .byTruncatingMiddle
            (name as NSString).draw(
                in: NSRect(x: 0, y: videoSize.height + 7, width: width, height: Self.captionHeight - 7),
                withAttributes: [.font: NSFont.systemFont(ofSize: TextSize.caption.rawValue),
                                 .foregroundColor: MarkdownPalette.tertiary,
                                 .paragraphStyle: para])
            return true
        }
        bounds = NSRect(origin: .zero, size: total)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    private static func aspectFit(_ size: NSSize, in rect: NSRect) -> NSRect {
        guard size.width > 0, size.height > 0 else { return rect }
        let scale = min(rect.width / size.width, rect.height / size.height)
        let w = size.width * scale, h = size.height * scale
        return NSRect(x: rect.midX - w / 2, y: rect.midY - h / 2, width: w, height: h)
    }

    private static func format(_ seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded())
        return total >= 3600
            ? String(format: "%d:%02d:%02d", total / 3600, (total % 3600) / 60, total % 60)
            : String(format: "%d:%02d", total / 60, total % 60)
    }
}
