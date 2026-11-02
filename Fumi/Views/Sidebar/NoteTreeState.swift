import SwiftUI
import AppKit

/// What the sidebar tree's rows share: which folders are open, the folder a
/// dragged row is over, and moving notes, files and folders between folders
/// (by drag and drop or Move To).
@Observable
final class NoteTreeState {
    var expandedFolders: Set<String> = []
    /// The folder a drop would land in ("/" for the library's top level).
    var dropTarget: String?
    /// Every folder of the tree, in tree order, for Move To.
    var folderIds: [String] = []
    /// A refused move to explain.
    var failedMove: FailedMove?

    struct FailedMove {
        let title: String
        let message: String
    }

    @ObservationIgnored private var pendingTarget: Task<Void, Never>?
    @ObservationIgnored private var springLoad: (folder: String, task: Task<Void, Never>)?

    /// How long the pointer rests on a closed folder before it opens.
    private static let springLoadDelay: Duration = .milliseconds(650)
    /// Rows sit nearly edge to edge, so leaving one usually means entering
    /// the next a moment later — and crossing the hairline between them
    /// briefly lands on the list behind. Changes wait this long, so the
    /// highlight goes straight to the next row instead of flashing off or
    /// onto the library heading on the way.
    private static let settleDelay: Duration = .milliseconds(70)

    /// The drop highlight. A row takes it at once; the list behind the rows
    /// (`settles`) and a target turning off only take effect if nothing else
    /// claims it within `settleDelay`.
    func setTargeted(_ targeted: Bool, folder id: String, settles: Bool = false) {
        if targeted && !settles {
            pendingTarget?.cancel()
            pendingTarget = nil
            if dropTarget != id { dropTarget = id }
            return
        }
        if !targeted && dropTarget != id { return }
        pendingTarget?.cancel()
        pendingTarget = Task { @MainActor [weak self] in
            try? await Task.sleep(for: Self.settleDelay)
            guard !Task.isCancelled, let self else { return }
            self.dropTarget = targeted ? id : nil
            self.pendingTarget = nil
        }
    }

    /// Opens the closed folder `id` once a drag has rested on it for a
    /// moment, as Finder does, so an item can go into a folder inside it.
    func springLoad(_ id: String) {
        cancelSpringLoad()
        guard !expandedFolders.contains(id) else { return }
        springLoad = (id, Task { @MainActor [weak self] in
            try? await Task.sleep(for: Self.springLoadDelay)
            guard !Task.isCancelled, let self, self.dropTarget == id else { return }
            _ = withAnimation(.easeInOut(duration: 0.18)) { self.expandedFolders.insert(id) }
        })
    }

    func cancelSpringLoad(_ id: String? = nil) {
        guard let current = springLoad, id == nil || current.folder == id else { return }
        current.task.cancel()
        springLoad = nil
    }

    /// Moves `urls` into the folder `id`. The open note or file and the open
    /// folders follow what moved, and the folder opens to show it. Returns
    /// whether anything moved.
    @discardableResult
    func move(_ urls: [URL], toFolder id: String, store: FileNoteStore, appModel: AppModel) -> Bool {
        pendingTarget?.cancel()
        pendingTarget = nil
        cancelSpringLoad()
        dropTarget = nil
        var moved = false
        for url in urls {
            let result = withAnimation(.easeInOut(duration: 0.15)) {
                store.moveItem(at: url, toFolder: id)
            }
            switch result {
            case .success(let newURL):
                moved = true
                follow(from: url, to: newURL, store: store, appModel: appModel)
            case .failure(.nameTaken(let name)):
                failedMove = FailedMove(title: "Couldn't Move “\(name)”",
                                        message: "An item named “\(name)” already exists in “\(folderName(id, store: store))”.")
            case .failure(.failed(let reason)):
                failedMove = FailedMove(title: "Couldn't Move “\(url.lastPathComponent)”", message: reason)
            case .failure(.intoItself):
                NSSound.beep()
            case .failure(.alreadyThere), .failure(.outsideLibrary):
                break
            }
        }
        if moved {
            withAnimation(.easeInOut(duration: 0.15)) {
                expandedFolders.formUnion(NoteTreeBuilder.ancestorIds(ofRelativePath: id))
            }
        }
        return moved
    }

    /// The name Move To and the alerts use for a folder.
    func folderName(_ id: String, store: FileNoteStore) -> String {
        id == "/" ? (store.workspaceURL?.lastPathComponent ?? "Notes") : (id as NSString).lastPathComponent
    }

    private func follow(from old: URL, to new: URL, store: FileNoteStore, appModel: AppModel) {
        // Every tab showing something inside it follows, open or not.
        appModel.remapTabs(from: old.path, to: new.path)
        // A moved folder stays open, and so do the folders inside it.
        if let library = store.workspaceURL,
           let oldId = LibraryMove.folderId(of: old, in: library),
           let newId = LibraryMove.folderId(of: new, in: library) {
            expandedFolders = Set(expandedFolders.map { LibraryMove.remap($0, from: oldId, to: newId) ?? $0 })
        }
    }

    /// Every folder below `folder`, depth first in tree order.
    static func folderIds(in folder: NoteFolder) -> [String] {
        folder.folders.flatMap { [$0.id] + folderIds(in: $0) }
    }
}
