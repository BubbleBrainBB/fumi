import Foundation

/// Pure decision helpers for reconciling on-disk file changes (made by an
/// external editor) with what the in-app editor is showing. Kept free of AppKit
/// / SwiftUI so it can be unit-tested standalone.
enum ExternalSync {

    // MARK: - Editor reconciliation

    /// What to do when the open note's on-disk content changes underneath the
    /// editor.
    enum Decision: Equatable {
        /// Nothing new from outside: on-disk content matches what's displayed,
        /// or is what the editor last loaded or saved (the echo of the app's
        /// own save). Nothing to load; just record that we're in sync.
        case ignore
        /// The editor has no unsaved local edits, so the external change can be
        /// pulled in safely.
        case adopt
        /// Both changed: the editor has unsaved local edits and the file was
        /// changed outside too. Merge the two (`Merge3`) rather than letting
        /// either overwrite the other.
        case merge
    }

    /// - Parameters:
    ///   - disk: the latest content read from the file.
    ///   - displayed: what the editor is currently showing.
    ///   - lastSynced: the content the editor last loaded or saved (its view of
    ///     what is on disk).
    static func resolve(disk: String, displayed: String, lastSynced: String) -> Decision {
        if disk == displayed || disk == lastSynced { return .ignore }
        if displayed == lastSynced { return .adopt }
        return .merge
    }

    // MARK: - File-system event filtering

    /// The event paths that can touch the library: not the ones inside hidden
    /// folders like `.git` or `.obsidian`, or hidden files like `.DS_Store`,
    /// which the library's scan skips anyway.
    static func visiblePaths(_ eventPaths: [String], in library: String) -> [String] {
        let root = library.hasSuffix("/") ? library : library + "/"
        return eventPaths.filter { path in
            guard path.hasPrefix(root) else { return true }
            return !path.dropFirst(root.count).split(separator: "/").contains { $0.hasPrefix(".") }
        }
    }

    /// Whether a batch of file-system event paths represents a change the app did
    /// not make itself. Used to skip the rescan that our own debounced saves would
    /// otherwise trigger on every keystroke window.
    ///
    /// A path counts as ours only while the file still holds what we wrote:
    /// a write from outside that lands right after one of our saves (in the
    /// same batch of events, even) is still seen.
    ///
    /// - Parameters:
    ///   - eventPaths: paths reported by the watcher.
    ///   - ownWrites: the content the app recently wrote, by path.
    ///   - read: the file's current content, nil when it can't be read.
    static func hasExternalChange(eventPaths: [String], ownWrites: [String: String],
                                  read: (String) -> String?) -> Bool {
        let edited = eventPaths.filter {
            let ext = ($0 as NSString).pathExtension.lowercased()
            return ext == "md" || ext == "markdown"
        }
        // No note files in the batch (e.g. a bare directory event) → reload to be
        // safe; it might be a create/delete/rename we should reflect.
        guard !edited.isEmpty else { return true }
        return edited.contains { path in
            guard let written = ownWrites[path] else { return true }
            return read(path) != written
        }
    }
}
