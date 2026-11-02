import AppKit
import AVKit
import PDFKit
import SwiftUI

/// An image, video or PDF opened from the sidebar, shown on the page in place
/// of the editor: the image at its own size (shrunk to fit, never blown up),
/// a playable video, a scrolling PDF.
struct FilePreviewView: View {
    let file: LibraryFile
    @Environment(FileNoteStore.self) var store
    @Environment(AppModel.self) var appModel

    var body: some View {
        content
            .id(file.id)
            .padding(file.kind == .pdf ? 0 : 40)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.pageContentBg)
            .navigationTitle(file.name)
            .navigationSubtitle(subtitle)
            .modifier(RemovesSystemTitle())
            .toolbar {
                NoteTitleHeader(title: file.name, subtitle: subtitle)

                FlatToolbarItem {
                    Button {
                        appModel.newTab()
                    } label: {
                        Label {
                            Text("New Note")
                        } icon: {
                            Image(nsImage: ShiyeGlyph.newNote.templateImage())
                        }
                    }
                    .help("New Note (⌘N)")
                }

                FlatToolbarItem {
                    Menu {
                        Button("Copy Markdown Link") { copyMarkdownLink() }
                        Button("Open in Default App") { NSWorkspace.shared.open(file.url) }
                        Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([file.url]) }
                        Divider()
                        Button("Move to Trash", role: .destructive) { moveToTrash() }
                    } label: {
                        Label {
                            Text("More")
                        } icon: {
                            Image(nsImage: ShiyeGlyph.more.templateImage())
                        }
                    }
                    .menuIndicator(.hidden)
                    .help("More")
                }
            }
    }

    @ViewBuilder private var content: some View {
        switch file.kind {
        case .image: ImagePreview(url: file.url)
        case .video: SizedVideoPreview(url: file.url)
        case .pdf: PDFPreview(url: file.url)
        }
    }

    /// "attachments · 1.2 MB"
    private var subtitle: String {
        let folder = file.relativePath.split(separator: "/").joined(separator: " / ")
        let bytes = (try? file.url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize
        let size = bytes.map { ByteCountFormatter.string(fromByteCount: Int64($0), countStyle: .file) }
        return [folder, size ?? ""].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// `![](path)` from the library root, for pasting into a note.
    private func copyMarkdownLink() {
        guard let library = store.workspaceURL else { return }
        let path = AttachmentImport.relativePath(from: library, to: file.url)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(AttachmentImport.markdown(forPath: path), forType: .string)
    }

    private func moveToTrash() {
        do {
            try FileManager.default.trashItem(at: file.url, resultingItemURL: nil)
            appModel.removeTabs(showing: .file(file.id))
            store.reloadNotes()
        } catch {
            NSSound.beep()
        }
    }
}

/// NSImageView, so GIFs animate and SVGs draw; scaled down to fit only.
private struct ImagePreview: NSViewRepresentable {
    let url: URL

    func makeNSView(context: Context) -> NSImageView {
        let view = NSImageView()
        view.imageScaling = .scaleProportionallyDown
        view.animates = true
        view.isEditable = false
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        view.image = NSImage(contentsOf: url)
        return view
    }

    func updateNSView(_ view: NSImageView, context: Context) {}
}

/// The player at the video's own shape, so the page shows no black bars.
private struct SizedVideoPreview: View {
    let url: URL
    @State private var aspect: CGFloat?

    var body: some View {
        VideoPreview(url: url)
            .aspectRatio(aspect ?? 16.0 / 9.0, contentMode: .fit)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .opacity(aspect == nil ? 0 : 1)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .task(id: url) { aspect = await Self.aspect(of: url) ?? 16.0 / 9.0 }
    }

    static func aspect(of url: URL) async -> CGFloat? {
        guard let track = try? await AVURLAsset(url: url).loadTracks(withMediaType: .video).first,
              let (size, transform) = try? await track.load(.naturalSize, .preferredTransform) else { return nil }
        let shown = size.applying(transform)
        guard shown.width != 0, shown.height != 0 else { return nil }
        return abs(shown.width / shown.height)
    }
}

private struct VideoPreview: NSViewRepresentable {
    let url: URL

    func makeNSView(context: Context) -> AVPlayerView {
        let view = AVPlayerView()
        view.controlsStyle = .inline
        view.videoGravity = .resizeAspect
        view.showsFullScreenToggleButton = true
        view.player = AVPlayer(url: url)
        return view
    }

    func updateNSView(_ view: AVPlayerView, context: Context) {}

    static func dismantleNSView(_ view: AVPlayerView, coordinator: ()) {
        view.player?.pause()
        view.player = nil
    }
}

private struct PDFPreview: NSViewRepresentable {
    let url: URL

    func makeNSView(context: Context) -> PDFView {
        let view = PDFView()
        view.autoScales = true
        view.displayMode = .singlePageContinuous
        view.backgroundColor = .clear
        view.document = PDFDocument(url: url)
        return view
    }

    func updateNSView(_ view: PDFView, context: Context) {}
}
