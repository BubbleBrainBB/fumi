import AppKit
import SwiftUI

extension LibraryList {
    /// Starts from the welcome page's recent folders, the ones still there.
    static let shared = LibraryList(defaults: .standard, seed: {
        AppSettings.recentWorkspaces.filter { FileManager.default.fileExists(atPath: $0) }
    })
}

extension AppModel {
    /// Makes `url` this window's library and shows its files (design board
    /// 60 B). A library already open in another window brings that window
    /// forward instead; one that can't be found shows the missing-folder page.
    func enterLibrary(_ url: URL, store: FileNoteStore) {
        if store.workspaceURL?.standardizedFileURL.path == url.standardizedFileURL.path {
            showsLibraries = false
            return
        }
        if LibraryList.missingReason(url) != nil {
            store.closeWorkspace()
            store.missingWorkspace = url
            showsLibraries = false
            return
        }
        if LibraryWindows.bringForward(showing: url, except: store) { return }
        store.openWorkspace(at: url)
        showsLibraries = false
    }

    /// The sidebar's first level, showing the sidebar first if it's hidden.
    func showLibraries() {
        if !isSidebarVisible { toggleSidebar() }
        showsLibraries = true
    }
}

/// Finds and opens windows by the library they show.
enum LibraryWindows {
    /// Brings forward the window showing `url`, other than the one with
    /// `store`; false when there is none.
    static func bringForward(showing url: URL, except store: FileNoteStore?) -> Bool {
        WindowCloseController.shared.bringForward(showing: url, except: store)
    }

    /// `url` in a window of its own: the one already showing it, or a new one.
    static func open(_ url: URL, using openWindow: OpenWindowAction) {
        guard !bringForward(showing: url, except: nil) else { return }
        WindowCloseController.shared.pendingLibrary = url
        openWindow(id: "main")
    }
}
