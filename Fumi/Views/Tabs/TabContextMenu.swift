import AppKit
import SwiftUI

/// A right-click (or control-click) menu that works inside the toolbar.
/// SwiftUI's `.contextMenu` never gets the click there, and neither does an
/// NSView's `rightMouseDown`: the toolbar keeps secondary clicks for itself
/// (it offered "Icon and Text / Icon Only"). So the click is caught as it
/// arrives in the app and goes to the smallest menu region under the
/// pointer — a tab before the strip around it. That toolbar menu is also
/// turned off for the window.
struct RightClickMenu: NSViewRepresentable {
    let makeMenu: () -> NSMenu

    func makeNSView(context: Context) -> MenuView {
        let view = MenuView()
        view.makeMenu = makeMenu
        return view
    }

    func updateNSView(_ view: MenuView, context: Context) {
        view.makeMenu = makeMenu
    }

    final class MenuView: NSView {
        var makeMenu: (() -> NSMenu)?

        /// Never in the way of SwiftUI's own clicks, hover and drags.
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if window != nil {
                RightClickRouter.shared.register(self)
                if #available(macOS 15, *) {
                    window?.toolbar?.allowsDisplayModeCustomization = false
                }
            } else {
                RightClickRouter.shared.unregister(self)
            }
        }
    }
}

/// Routes secondary clicks to the RightClickMenu regions on screen.
@MainActor
private final class RightClickRouter {
    static let shared = RightClickRouter()

    private var views = NSHashTable<RightClickMenu.MenuView>.weakObjects()
    private var monitor: Any?

    func register(_ view: RightClickMenu.MenuView) {
        views.add(view)
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.rightMouseDown, .leftMouseDown]) { [weak self] event in
            guard let self else { return event }
            return self.handle(event) ? nil : event
        }
    }

    func unregister(_ view: RightClickMenu.MenuView) {
        views.remove(view)
    }

    /// True when a menu was shown for the click.
    private func handle(_ event: NSEvent) -> Bool {
        let isSecondary = event.type == .rightMouseDown
            || (event.type == .leftMouseDown && event.modifierFlags.contains(.control))
        guard isSecondary, let window = event.window else { return false }
        let target = views.allObjects
            .filter { view in
                guard view.window === window, !view.isHiddenOrHasHiddenAncestor else { return false }
                return view.bounds.contains(view.convert(event.locationInWindow, from: nil))
            }
            .min { $0.bounds.width * $0.bounds.height < $1.bounds.width * $1.bounds.height }
        guard let target, let menu = target.makeMenu?() else { return false }
        NSMenu.popUpContextMenu(menu, with: event, for: target)
        return true
    }
}

/// A menu item that runs a closure.
final class ClosureMenuItem: NSMenuItem {
    private let handler: () -> Void

    init(_ title: String, enabled: Bool = true, handler: @escaping () -> Void) {
        self.handler = handler
        super.init(title: title, action: #selector(run), keyEquivalent: "")
        target = self
        isEnabled = enabled
    }

    @available(*, unavailable)
    required init(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    @objc private func run() { handler() }
}

extension AppModel {
    /// What right-clicking a tab offers. Pinning leads: it puts the tab at
    /// the front of the strip, and takes it off again.
    func tabMenu(for tab: NoteTab, url: URL?) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.addItem(ClosureMenuItem(tab.isPinned ? "Unpin Tab" : "Pin Tab") { [weak self] in
            self?.togglePin(tab.id)
        })
        if tabs.count > 1 {
            menu.addItem(.separator())
            menu.addItem(ClosureMenuItem("Close Tab", enabled: !tab.isPinned) { [weak self] in
                self?.closeTab(tab.id)
            })
            menu.addItem(ClosureMenuItem("Close Other Tabs",
                                         enabled: !tabs.allSatisfy { $0.id == tab.id || $0.isPinned }) { [weak self] in
                self?.closeOtherTabs(tab.id)
            })
            menu.addItem(ClosureMenuItem("Close Tabs to the Right", enabled: tabs.last?.id != tab.id) { [weak self] in
                self?.closeTabsToTheRight(of: tab.id)
            })
        }
        if let url {
            menu.addItem(.separator())
            menu.addItem(ClosureMenuItem("Reveal in Sidebar") { [weak self] in
                guard let self else { return }
                selectTab(tab.id)
                if !isSidebarVisible { toggleSidebar() }
            })
            menu.addItem(ClosureMenuItem("Reveal in Finder") {
                NSWorkspace.shared.activateFileViewerSelecting([url])
            })
        }
        return menu
    }
}
