import CoreServices
import Foundation
import Observation

@Observable
final class FileNoteStore {
    var notes: [FileNote] = [] {
        // Links between notes show their titles (WikiLinkIndex). Rebuilding
        // reads every note, too slow for each save while typing: only when
        // a link would show something else.
        didSet {
            guard !notes.elementsEqual(oldValue, by: WikiLinkIndex.showsSame) else { return }
            WikiLinkIndex.shared.update(notes: notes.map { ($0.id, $0.url, $0.content) }, library: workspaceURL)
        }
    }
    /// Images, videos and PDFs in the library, shown in the sidebar too.
    var files: [LibraryFile] = []
    var workspaceURL: URL?
    var isOpen: Bool { workspaceURL != nil }
    /// False from opening a folder until its first scan lands, so the empty
    /// library page doesn't flash while the notes load.
    var hasLoadedNotes = false
    /// The folder the last session used, when it can't be found at launch —
    /// moved, renamed, deleted, or on a disk that isn't connected (design
    /// board ㉞ ④). Opening any folder clears it.
    var missingWorkspace: URL?

    private var eventStream: FSEventStreamRef?
    /// What this store wrote itself, by path, with the time of the write. Lets
    /// us ignore the file-system events our own saves generate, so typing does
    /// not kick off a full rescan on every debounced save — while a file
    /// written over from outside right after is still seen.
    @ObservationIgnored private var ownWrites: [String: (content: String, at: Date)] = [:]
    private var pinnedFiles: Set<String> = []
    private let reloadQueue = DispatchQueue(label: "notely.fileNoteStore.reload", qos: .userInitiated)
    private var reloadGeneration = 0
    private var reloadDebounceItem: DispatchWorkItem?

    deinit {
        stopWatching()
    }

    // MARK: - Open / Close workspace

    func openWorkspace(at url: URL) {
        closeWorkspace()
        workspaceURL = url
        missingWorkspace = nil
        ImageLoader.shared.setWorkspaceURL(url)
        AppSettings.workspacePath = url.path
        AppSettings.noteRecentWorkspace(url.path)
        LibraryList.shared.add(url)
        loadPinnedFiles()
        reloadNotes()
        startWatching()
    }

    func closeWorkspace() {
        stopWatching()
        reloadDebounceItem?.cancel()
        reloadDebounceItem = nil
        reloadGeneration += 1
        notes = []
        files = []
        hasLoadedNotes = false
        workspaceURL = nil
        ImageLoader.shared.setWorkspaceURL(nil)
        ImageLoader.shared.setBaseURL(nil)
    }

    // MARK: - Load notes from disk

    func reloadNotes() {
        guard let workspaceURL else { return }
        let pinnedFiles = pinnedFiles
        let known = notes
        reloadGeneration += 1
        let generation = reloadGeneration

        reloadQueue.async { [weak self] in
            let result = Self.scanLibrary(in: workspaceURL, pinnedFiles: pinnedFiles)
            let scanned = FileNote.keepingUnchanged(known, in: result.notes)

            DispatchQueue.main.async {
                guard let self,
                      self.reloadGeneration == generation,
                      self.workspaceURL == workspaceURL else { return }
                if self.notes != scanned { self.notes = scanned }
                if self.files != result.files { self.files = result.files }
                if !self.hasLoadedNotes { self.hasLoadedNotes = true }
            }
        }
    }

    // MARK: - File operations

    // MARK: - Draft (design board ㉝ E)

    /// What a draft starts as: an empty title heading, the caret after it.
    static let draftContent = "# "
    /// The id of the last draft handed out; its first save writes the file.
    @ObservationIgnored private var draftId: String?

    /// The page shown when no note is open: a blank note at the library's
    /// top level, named like New Note's, whose file is only written once
    /// something is typed — leave without writing and nothing is kept.
    func draftNote() -> FileNote? {
        guard let workspaceURL else { return nil }
        var url = workspaceURL.appendingPathComponent("Untitled.md")
        var counter = 2
        while FileManager.default.fileExists(atPath: url.path) {
            url = workspaceURL.appendingPathComponent("Untitled \(counter).md")
            counter += 1
        }
        draftId = url.path
        return FileNote(id: url.path, url: url, title: "", content: Self.draftContent, tags: [],
                        createdAt: Date(), updatedAt: Date(), pinned: false,
                        relativePath: relativePath(of: url))
    }

    /// A new note titled `title`, in `folder` (the library's top when nil).
    @discardableResult
    func createNote(title: String? = nil, in folder: URL? = nil) -> FileNote? {
        guard let workspaceURL else { return nil }
        let folder = folder ?? workspaceURL
        let name = (title ?? "Untitled").sanitizeForFilename()
        var url = folder.appendingPathComponent("\(name).md")

        // Avoid overwrite
        var counter = 2
        while FileManager.default.fileExists(atPath: url.path) {
            url = folder.appendingPathComponent("\(name) \(counter).md")
            counter += 1
        }

        let initialContent = "# \(title ?? "Untitled")\n\n"
        try? initialContent.write(to: url, atomically: true, encoding: .utf8)

        let attrs: [FileAttributeKey: Any] = [
            .creationDate: Date(),
            .modificationDate: Date()
        ]
        try? FileManager.default.setAttributes(attrs, ofItemAtPath: url.path)

        let note = FileNote(
            id: url.path,
            url: url,
            title: TitleExtractor.extract(from: initialContent),
            content: initialContent,
            tags: TagExtractor.frontmatterTags(from: initialContent),
            createdAt: attrs[.creationDate] as? Date ?? Date(),
            updatedAt: attrs[.modificationDate] as? Date ?? Date(),
            pinned: false,
            relativePath: relativePath(of: url)
        )
        notes.insert(note, at: 0)
        scheduleReload()
        return note
    }

    /// What a save did.
    struct SaveResult {
        /// The save wrote a draft's file for the first time (the note is in
        /// the library from now on).
        var adoptedDraft = false
        /// What the file holds now: the content saved or, when the file was
        /// changed outside meanwhile, the two merged. Nil when nothing was saved.
        var written: String?
        /// Where the outside version was kept, when it changed a paragraph
        /// the content saved changed too.
        var conflictCopy: FileNote?
    }

    /// Saves `content`, edited from `base`: what the file held when the
    /// editing began (the store's last known content when nil). A file
    /// changed outside since then is merged with `content`, never written
    /// over (`Merge3`): edits to different paragraphs are both kept; where
    /// both changed the same paragraph `content` wins, and the outside
    /// version is kept as a copy beside the note.
    @discardableResult
    func saveContent(_ content: String, to noteId: String, basedOn base: String? = nil) -> SaveResult {
        guard let note = notes.first(where: { $0.id == noteId }) ?? movedNote(noteId) else {
            let adopted = saveDraft(content, id: noteId)
            return SaveResult(adoptedDraft: adopted, written: adopted ? content : nil)
        }
        // A save from before a move: its links still read from the old folder.
        let moved = note.id != noteId
        let content = moved ? relinked(content, savedFor: noteId) : content
        let base = base.map { moved ? relinked($0, savedFor: noteId) : $0 } ?? note.content
        var result = SaveResult(written: content)
        let disk = try? String(contentsOf: note.url, encoding: .utf8)
        if let disk, disk != base, disk != content {
            let merged = Merge3.merge(base: base, mine: content, theirs: disk)
            result.written = merged.text
            if merged.conflicts > 0 { result.conflictCopy = saveConflictCopy(disk, of: note) }
        }
        let written = result.written ?? content
        if disk != written {
            // Record before writing so the watcher event this triggers is recognized
            // as our own and does not cause a redundant rescan.
            recordOwnWrite(written, at: note.url)
            try? written.write(to: note.url, atomically: true, encoding: .utf8)
        }
        // Update in-memory without full reload (avoids cursor jump)
        if let idx = notes.firstIndex(where: { $0.id == note.id }) {
            var saved = notes[idx]
            saved.content = written
            saved.title = TitleExtractor.extract(from: written)
            saved.tags = TagExtractor.frontmatterTags(from: written)
            saved.updatedAt = Date()
            notes[idx] = saved
        }
        return result
    }

    /// Keeps `text`, the outside version of `note`, beside it as
    /// "<name> (outside edit 14-10).md", never over another file.
    private func saveConflictCopy(_ text: String, of note: FileNote) -> FileNote? {
        let folder = note.url.deletingLastPathComponent()
        let ext = note.url.pathExtension
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "HH-mm"
        let name = "\(note.url.deletingPathExtension().lastPathComponent) (outside edit \(formatter.string(from: Date())))"
        var url = folder.appendingPathComponent(name).appendingPathExtension(ext)
        var counter = 2
        while FileManager.default.fileExists(atPath: url.path) {
            url = folder.appendingPathComponent("\(name) \(counter)").appendingPathExtension(ext)
            counter += 1
        }
        recordOwnWrite(text, at: url)
        guard (try? text.write(to: url, atomically: true, encoding: .utf8)) != nil else { return nil }
        let copy = FileNote(id: url.path, url: url, title: TitleExtractor.extract(from: text),
                            content: text, tags: TagExtractor.frontmatterTags(from: text),
                            createdAt: Date(), updatedAt: Date(), pinned: false,
                            relativePath: relativePath(of: url))
        notes.insert(copy, at: 0)
        return copy
    }

    private func saveDraft(_ content: String, id: String) -> Bool {
        guard id == draftId, workspaceURL != nil,
              !FileManager.default.fileExists(atPath: id) else { return false }
        draftId = nil
        let url = URL(fileURLWithPath: id)
        recordOwnWrite(content, at: url)
        guard (try? content.write(to: url, atomically: true, encoding: .utf8)) != nil else { return false }
        notes.insert(FileNote(id: url.path, url: url, title: TitleExtractor.extract(from: content),
                              content: content, tags: TagExtractor.frontmatterTags(from: content),
                              createdAt: Date(), updatedAt: Date(), pinned: false,
                              relativePath: relativePath(of: url)), at: 0)
        return true
    }

    // MARK: - Memos (design board ㉛)

    /// The day files of the Memos folder.
    var memoNotes: [FileNote] {
        notes.filter { $0.relativePath == "/" + MemoFormat.folderName && MemoFormat.isDay($0.filename) }
    }

    /// Every memo, newest day first and, within a day, latest first.
    var memos: [Memo] {
        memoNotes
            .sorted { $0.filename > $1.filename }
            .flatMap { MemoFormat.parse($0.content, day: $0.filename).reversed() }
    }

    func memoNote(forDay day: String) -> FileNote? {
        memoNotes.first { $0.filename == day }
    }

    /// Adds a memo to today's file, creating it as needed.
    func addMemo(_ text: String, at date: Date = Date()) {
        guard let workspaceURL, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        let day = MemoFormat.day(of: date)
        if let note = memoNote(forDay: day) {
            saveContent(MemoFormat.appending(text, at: MemoFormat.time(of: date), to: note.content), to: note.id)
            return
        }
        guard let url = try? MemoFile.append(text, at: date, in: workspaceURL) else { return }
        let content = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
        recordOwnWrite(content, at: url)
        notes.insert(FileNote(id: url.path, url: url, title: TitleExtractor.extract(from: content),
                              content: content, tags: TagExtractor.frontmatterTags(from: content),
                              createdAt: date, updatedAt: date, pinned: false,
                              relativePath: relativePath(of: url)), at: 0)
    }

    func updateMemo(_ memo: Memo, text: String) {
        guard let note = memoNote(forDay: memo.day) else { return }
        saveContent(MemoFormat.replacing(memo: memo.index, with: text, in: note.content), to: note.id)
    }

    func deleteMemo(_ memo: Memo) {
        guard let note = memoNote(forDay: memo.day) else { return }
        let content = MemoFormat.removing(memo: memo.index, from: note.content)
        if content.isEmpty {
            deleteNote(note.id)
        } else {
            saveContent(content, to: note.id)
        }
    }

    func deleteNote(_ noteId: String) {
        guard let note = notes.first(where: { $0.id == noteId }) else { return }
        try? FileManager.default.removeItem(at: note.url)
        notes.removeAll(where: { $0.id == noteId })
    }

    func togglePin(_ noteId: String) {
        guard let idx = notes.firstIndex(where: { $0.id == noteId }) else { return }
        notes[idx].pinned.toggle()
        let path = notes[idx].url.path
        if notes[idx].pinned {
            pinnedFiles.insert(path)
        } else {
            pinnedFiles.remove(path)
        }
        savePinnedFiles()
        // Re-sort
        notes.sort { lhs, rhs in
            if lhs.pinned != rhs.pinned { return lhs.pinned }
            return lhs.updatedAt > rhs.updatedAt
        }
    }

    /// Renames the note's file to `newTitle` (sanitized). Returns the new note id
    /// (its new file path) on success, or nil if nothing changed or it failed
    /// (e.g. empty name, name unchanged, or a file with that name already exists).
    @discardableResult
    func renameNote(_ noteId: String, to newTitle: String) -> String? {
        guard let idx = notes.firstIndex(where: { $0.id == noteId }) else { return nil }
        let base = newTitle.sanitizeForFilename()
        guard !base.isEmpty else { return nil }
        let oldURL = notes[idx].url
        let newFilename = base + ".md"
        let newURL = oldURL.deletingLastPathComponent().appendingPathComponent(newFilename)

        guard newURL != oldURL else { return nil }
        guard !FileManager.default.fileExists(atPath: newURL.path) else { return nil }

        do {
            try FileManager.default.moveItem(at: oldURL, to: newURL)
            // Migrate pin state, which is keyed by file path.
            if pinnedFiles.remove(oldURL.path) != nil {
                pinnedFiles.insert(newURL.path)
                savePinnedFiles()
            }
            notes[idx] = FileNote(
                id: newURL.path,
                url: newURL,
                title: newTitle,
                content: notes[idx].content,
                tags: notes[idx].tags,
                createdAt: notes[idx].createdAt,
                updatedAt: Date(),
                pinned: notes[idx].pinned,
                relativePath: relativePath(of: newURL)
            )
            return newURL.path
        } catch {
            print("Rename failed: \(error)")
            return nil
        }
    }

    /// Item moves of this session, oldest first, so a debounced save bound to
    /// a note's old path still lands in its file after a move.
    @ObservationIgnored private var recentMoves: [(from: String, to: String)] = []

    /// Moves a note, file or folder into the folder with sidebar id `folderId`
    /// ("/" for the library's top level). Never overwrites: a name already
    /// taken there refuses the move. Pins, and the notes and files in memory,
    /// follow at once (their ids are their paths), so a selection the caller
    /// remaps with `LibraryMove.remap` finds its note straight away.
    func moveItem(at url: URL, toFolder folderId: String) -> Result<URL, LibraryMove.Refusal> {
        guard let workspaceURL else { return .failure(.outsideLibrary) }
        let folder = LibraryMove.folderURL(forId: folderId, in: workspaceURL)
        let newURL: URL
        switch LibraryMove.destination(moving: url, into: folder, library: workspaceURL,
                                       exists: { FileManager.default.fileExists(atPath: $0.path) }) {
        case .success(let target): newURL = target
        case .failure(let refusal): return .failure(refusal)
        }

        do {
            try FileManager.default.moveItem(at: url, to: newURL)
        } catch {
            return .failure(.failed(error.localizedDescription))
        }
        let old = url.path, new = newURL.path

        // Migrate pin state, which is keyed by file path.
        let movedPins = pinnedFiles.compactMap { path in LibraryMove.remap(path, from: old, to: new).map { (path, $0) } }
        if !movedPins.isEmpty {
            for (from, to) in movedPins {
                pinnedFiles.remove(from)
                pinnedFiles.insert(to)
            }
            savePinnedFiles()
        }

        notes = notes.map { note in
            guard let path = LibraryMove.remap(note.id, from: old, to: new) else { return note }
            let url = URL(fileURLWithPath: path)
            return FileNote(id: path, url: url, title: note.title, content: note.content, tags: note.tags,
                            createdAt: note.createdAt, updatedAt: note.updatedAt, pinned: note.pinned,
                            relativePath: relativePath(of: url))
        }
        files = files.map { file in
            guard let path = LibraryMove.remap(file.id, from: old, to: new) else { return file }
            let url = URL(fileURLWithPath: path)
            return LibraryFile(id: path, url: url, kind: file.kind, updatedAt: file.updatedAt,
                               relativePath: relativePath(of: url))
        }
        recentMoves.append((old, new))
        relinkNotes(movedFrom: old, to: new, in: workspaceURL)
        scheduleReload()
        return .success(newURL)
    }

    /// Rewrites the links of the notes a move carried along — the note itself,
    /// or those in a moved folder — that would no longer find their images,
    /// videos and PDFs from the new folder. Saved like an edit, so the watcher
    /// takes the write as our own.
    private func relinkNotes(movedFrom old: String, to new: String, in library: URL) {
        let fm = FileManager.default
        for note in notes {
            guard let oldPath = LibraryMove.remap(note.id, from: new, to: old),
                  let content = try? String(contentsOf: note.url, encoding: .utf8) else { continue }
            let relinked = LinkRewrite.rewrite(content, noteAt: oldPath, movedFrom: old, to: new,
                                               library: library.path, exists: { fm.fileExists(atPath: $0) })
            if relinked != content { saveContent(relinked, to: note.id, basedOn: content) }
        }
    }

    /// `content` saved for the note once at `id`, its links following each of
    /// this session's moves since — the editor's last save before a move.
    private func relinked(_ content: String, savedFor id: String) -> String {
        guard let library = workspaceURL?.path else { return content }
        var path = id, content = content
        for move in recentMoves {
            guard let moved = LibraryMove.remap(path, from: move.from, to: move.to) else { continue }
            content = LinkRewrite.rewrite(content, noteAt: path, movedFrom: move.from, to: move.to, library: library,
                                          exists: { FileManager.default.fileExists(atPath: $0) })
            path = moved
        }
        return content
    }

    /// Where the note once at `id` is now, following this session's moves.
    private func movedNote(_ id: String) -> FileNote? {
        let path = recentMoves.reduce(id) { path, move in LibraryMove.remap(path, from: move.from, to: move.to) ?? path }
        guard path != id else { return nil }
        return notes.first { $0.id == path }
    }

    // MARK: - File system watching

    /// Watches the whole workspace subtree with FSEvents. Unlike a directory-level
    /// `DispatchSource` (which only fires when entries are added/removed/renamed),
    /// FSEvents reports *content* modifications to existing files and survives the
    /// atomic rename external editors use to save — so edits made outside the app
    /// are picked up live.
    private func startWatching() {
        guard let workspaceURL else { return }
        stopWatching()

        var context = FSEventStreamContext(
            version: 0,
            info: Unmanaged.passUnretained(self).toOpaque(),
            retain: nil,
            release: nil,
            copyDescription: nil
        )
        let callback: FSEventStreamCallback = { _, info, _, eventPaths, _, _ in
            guard let info else { return }
            let store = Unmanaged<FileNoteStore>.fromOpaque(info).takeUnretainedValue()
            let paths = unsafeBitCast(eventPaths, to: NSArray.self) as? [String] ?? []
            store.handleFileSystemEvents(paths)
        }
        let flags = FSEventStreamCreateFlags(
            kFSEventStreamCreateFlagFileEvents |
            kFSEventStreamCreateFlagNoDefer |
            kFSEventStreamCreateFlagUseCFTypes
        )
        guard let stream = FSEventStreamCreate(
            kCFAllocatorDefault,
            callback,
            &context,
            [workspaceURL.path] as CFArray,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            0.2,
            flags
        ) else { return }

        FSEventStreamSetDispatchQueue(stream, .main)
        FSEventStreamStart(stream)
        eventStream = stream
    }

    private func stopWatching() {
        guard let stream = eventStream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
        eventStream = nil
    }

    /// Runs on the main queue (the stream's dispatch queue). Drops events that are
    /// only the echo of our own saves, then debounces a reload.
    private func handleFileSystemEvents(_ paths: [String]) {
        let now = Date()
        ownWrites = ownWrites.filter { now.timeIntervalSince($0.value.at) < 5 }
        let paths = workspaceURL.map { ExternalSync.visiblePaths(paths, in: $0.path) } ?? paths
        guard !paths.isEmpty,
              ExternalSync.hasExternalChange(eventPaths: paths, ownWrites: ownWrites.mapValues { $0.content },
                                             read: { try? String(contentsOfFile: $0, encoding: .utf8) })
        else { return }
        scheduleReload()
    }

    private func recordOwnWrite(_ content: String, at url: URL) {
        ownWrites[url.path] = (content, Date())
    }

    private func scheduleReload() {
        reloadDebounceItem?.cancel()
        let item = DispatchWorkItem { [weak self] in
            self?.reloadNotes()
        }
        reloadDebounceItem = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5, execute: item)
    }

    // MARK: - Pinned files persistence

    private func loadPinnedFiles() {
        let raw = UserDefaults.standard.stringArray(forKey: "notely.pinnedFiles") ?? []
        pinnedFiles = Set(raw)
    }

    private func savePinnedFiles() {
        UserDefaults.standard.set(Array(pinnedFiles), forKey: "notely.pinnedFiles")
    }

    // MARK: - Helpers

    private func relativePath(of url: URL) -> String {
        guard let workspaceURL else { return url.lastPathComponent }
        return Self.relativePath(of: url, in: workspaceURL)
    }

    private static func scanLibrary(in workspaceURL: URL,
                                    pinnedFiles: Set<String>) -> (notes: [FileNote], files: [LibraryFile]) {
        let fm = FileManager.default
        var result: [FileNote] = []
        var files: [LibraryFile] = []

        guard let enumerator = fm.enumerator(
            at: workspaceURL,
            includingPropertiesForKeys: [.creationDateKey, .contentModificationDateKey, .isDirectoryKey],
            options: [.skipsHiddenFiles]
        ) else {
            return ([], [])
        }

        for case let fileURL as URL in enumerator {
            let ext = fileURL.pathExtension.lowercased()
            if let kind = LibraryFile.kind(of: fileURL) {
                let updatedAt = (try? fileURL.resourceValues(forKeys: [.contentModificationDateKey]))?
                    .contentModificationDate ?? Date()
                files.append(LibraryFile(id: fileURL.path, url: fileURL, kind: kind, updatedAt: updatedAt,
                                         relativePath: relativePath(of: fileURL, in: workspaceURL)))
                continue
            }
            guard ext == "md" || ext == "markdown" else { continue }

            let attrs = try? fm.attributesOfItem(atPath: fileURL.path)
            let content = (try? String(contentsOf: fileURL, encoding: .utf8)) ?? ""
            let createdAt = attrs?[.creationDate] as? Date ?? Date()
            let updatedAt = attrs?[.modificationDate] as? Date ?? Date()

            result.append(FileNote(
                id: fileURL.path,
                url: fileURL,
                title: TitleExtractor.extract(from: content),
                content: content,
                tags: TagExtractor.frontmatterTags(from: content),
                createdAt: createdAt,
                updatedAt: updatedAt,
                pinned: pinnedFiles.contains(fileURL.path),
                relativePath: relativePath(of: fileURL, in: workspaceURL)
            ))
        }

        let notes = result.sorted { lhs, rhs in
            if lhs.pinned != rhs.pinned { return lhs.pinned }
            return lhs.updatedAt > rhs.updatedAt
        }
        return (notes, files)
    }

    private static func relativePath(of url: URL, in workspaceURL: URL) -> String {
        let path = url.deletingLastPathComponent().path
        let root = workspaceURL.path
        if path.hasPrefix(root) {
            let rel = String(path.dropFirst(root.count))
            return rel.isEmpty ? "/" : rel
        }
        return "/"
    }
}

// MARK: - String sanitization

extension String {
    func sanitizeForFilename() -> String {
        let invalid = CharacterSet(charactersIn: "/\\:*?\"<>|")
        let sanitized = self
            .components(separatedBy: invalid)
            .joined(separator: "-")
            .trimmingCharacters(in: .whitespaces)
        return sanitized.isEmpty ? "Untitled" : sanitized
    }
}
