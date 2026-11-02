import AppKit
import Carbon.HIToolbox
import SwiftUI

/// Write a memo from anywhere (design board ㉛ ②): a global hotkey (⌃⌥N)
/// or the menu bar icon shows a small floating panel near the top of the
/// screen. It never brings Fumi's windows forward; ⌘↩ saves to today's
/// file in the last opened library; esc, or a click anywhere else, puts it
/// away and keeps the draft.
final class QuickCapture: NSObject {
    static let shared = QuickCapture()

    private var panel: CapturePanel?
    private var statusItem: NSStatusItem?
    private var hotKey: EventHotKeyRef?
    private var clickMonitors: [Any] = []
    private let model = CaptureModel()

    static let panelSize = NSSize(width: 600, height: 420)

    func install() {
        installHotKey()
        installStatusItem()
    }

    func toggle() {
        if panel?.isVisible == true { hide() } else { show() }
    }

    func show() {
        let panel = panel ?? makePanel()
        self.panel = panel
        model.message = nil
        position(panel)
        panel.makeKeyAndOrderFront(nil)
        // Opened from a menu, the menu's close hands focus back to the main
        // window right after; take it again.
        DispatchQueue.main.async { panel.makeKey() }
        model.focusRequest += 1
        watchClicksOutside(panel)
    }

    func hide() {
        panel?.orderOut(nil)
        clickMonitors.forEach(NSEvent.removeMonitor)
        clickMonitors = []
    }

    /// A click anywhere but the panel puts it away (the draft stays).
    private func watchClicksOutside(_ panel: NSPanel) {
        guard clickMonitors.isEmpty else { return }
        let mask: NSEvent.EventTypeMask = [.leftMouseDown, .rightMouseDown]
        if let local = NSEvent.addLocalMonitorForEvents(matching: mask, handler: { [weak self, weak panel] event in
            if event.window !== panel { self?.hide() }
            return event
        }) { clickMonitors.append(local) }
        if let global = NSEvent.addGlobalMonitorForEvents(matching: mask, handler: { [weak self] _ in
            self?.hide()
        }) { clickMonitors.append(global) }
    }

    private func save(_ text: String) {
        let path = AppSettings.workspacePath
        guard !path.isEmpty else {
            model.message = "Open a notebook in Fumi first."
            return
        }
        do {
            try MemoFile.append(text, in: URL(fileURLWithPath: path))
            model.text = ""
            hide()
        } catch {
            model.message = "Couldn't save: \(error.localizedDescription)"
        }
    }

    // MARK: Panel

    private func makePanel() -> CapturePanel {
        let panel = CapturePanel(contentRect: NSRect(origin: .zero, size: Self.panelSize),
                                 styleMask: [.borderless, .nonactivatingPanel],
                                 backing: .buffered, defer: false)
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false // the card draws its own
        panel.hidesOnDeactivate = false
        panel.contentView = NSHostingView(rootView: CaptureView(
            model: model,
            onSave: { [weak self] text in self?.save(text) },
            onCancel: { [weak self] in self?.hide() }
        ))
        return panel
    }

    /// Centered on the screen with the pointer, its card a fifth of the way
    /// down from the top.
    private func position(_ panel: NSPanel) {
        let mouse = NSEvent.mouseLocation
        guard let screen = NSScreen.screens.first(where: { $0.frame.contains(mouse) }) ?? NSScreen.main else { return }
        let area = screen.visibleFrame
        let origin = NSPoint(x: area.midX - Self.panelSize.width / 2,
                             y: area.maxY - area.height * 0.2 - Self.panelSize.height)
        panel.setFrameOrigin(origin)
    }

    // MARK: Hotkey (Carbon: works without Accessibility access)

    private func installHotKey() {
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        InstallEventHandler(GetApplicationEventTarget(), { _, _, _ in
            DispatchQueue.main.async { QuickCapture.shared.toggle() }
            return noErr
        }, 1, &spec, nil, nil)
        let id = EventHotKeyID(signature: OSType(0x4E54_4C59), id: 1) // 'NTLY'
        RegisterEventHotKey(UInt32(kVK_ANSI_N), UInt32(controlKey | optionKey), id,
                            GetApplicationEventTarget(), 0, &hotKey)
    }

    // MARK: Menu bar

    private func installStatusItem() {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        item.button?.image = ShiyeGlyph.memo.templateImage(size: 18)
        item.button?.toolTip = "New Memo (⌃⌥N)"
        item.button?.target = self
        item.button?.action = #selector(statusItemClicked(_:))
        item.button?.sendAction(on: [.leftMouseUp, .rightMouseUp])
        statusItem = item
    }

    @objc private func statusItemClicked(_ sender: NSStatusBarButton) {
        guard NSApp.currentEvent?.type == .rightMouseUp else {
            toggle()
            return
        }
        let menu = NSMenu()
        menu.addItem(withTitle: "New Memo", action: #selector(newMemo), keyEquivalent: "").target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: "Open Fumi", action: #selector(openApp), keyEquivalent: "").target = self
        menu.addItem(withTitle: "Quit Fumi", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "")
        statusItem?.menu = menu
        sender.performClick(nil)
        statusItem?.menu = nil
    }

    @objc private func newMemo() { show() }

    @objc private func openApp() {
        NSApp.activate(ignoringOtherApps: true)
        NSApp.windows.first { $0.canBecomeMain }?.makeKeyAndOrderFront(nil)
    }
}

/// Borderless, but able to take keyboard focus without activating Fumi.
final class CapturePanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

@Observable
final class CaptureModel {
    /// The draft survives the panel closing.
    var text = ""
    var message: String?
    /// Bumped on every show, to move focus into the editor.
    var focusRequest = 0
}

private struct CaptureView: View {
    @Bindable var model: CaptureModel
    let onSave: (String) -> Void
    let onCancel: () -> Void

    private var isEmpty: Bool { model.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            MemoTextEditor(text: $model.text, placeholder: "Write it down…", focusRequest: model.focusRequest,
                           onSubmit: { if !isEmpty { onSave(model.text) } }, onCancel: onCancel)
            HStack(spacing: 10) {
                Text(model.message ?? "Memos · Today")
                    .font(.fumi(.caption))
                    .foregroundColor(model.message == nil ? .faintText : .secondaryText)
                Spacer(minLength: 0)
                Button {
                    onSave(model.text)
                } label: {
                    HStack(spacing: 8) {
                        Text("Save").font(.fumi(.caption)).foregroundColor(.secondaryText)
                        Image(systemName: "arrow.up")
                            .font(.system(size: 11, weight: .bold))
                            .foregroundColor(.white)
                            .frame(width: 24, height: 24)
                            .background(Circle().fill(isEmpty ? Color.accent.opacity(0.22) : Color.accent))
                    }
                }
                .buttonStyle(.plain)
                .disabled(isEmpty)
                .keyboardShortcut(.return, modifiers: .command)
            }
        }
        .padding(.horizontal, 18)
        .padding(.top, 16)
        .padding(.bottom, 12)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color.surface)
                .shadow(color: .black.opacity(0.22), radius: 26, y: 12)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .strokeBorder(Color.pageEdge, lineWidth: 1)
        )
        .background(
            Button("Cancel", action: onCancel)
                .keyboardShortcut(.cancelAction)
                .opacity(0)
        )
        .frame(width: 560)
        .padding(.horizontal, 20)
        .padding(.top, 20)
        .frame(width: QuickCapture.panelSize.width, height: QuickCapture.panelSize.height, alignment: .top)
        .environment(\.colorScheme, .light)
    }
}
