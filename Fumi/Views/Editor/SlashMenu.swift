import AppKit
import SwiftUI
import Observation

/// What the slash menu shows: the items matching the query and the one the
/// arrow keys are on.
@Observable
final class SlashMenuModel {
    var query = ""
    var results: [SlashCommand.Result] = SlashCommand.results(for: "")
    var selectedIndex = 0
    /// Clicking an item.
    @ObservationIgnored var onPick: (SlashCommand) -> Void = { _ in }

    var selected: SlashCommand? {
        results.indices.contains(selectedIndex) ? results[selectedIndex].command : nil
    }

    /// Group labels only while nothing is typed; a filtered list is one run.
    var showsGroups: Bool { query.isEmpty }

    static let rowHeight: CGFloat = 30
    static let groupLabelHeight: CGFloat = 24
    static let footerHeight: CGFloat = 29
    static let padding: CGFloat = 5
    static let width: CGFloat = 248
    /// Nine rows, then the list scrolls.
    static let maxListHeight: CGFloat = 9 * rowHeight + groupLabelHeight

    var listHeight: CGFloat {
        guard !results.isEmpty else { return Self.rowHeight }
        let groups = showsGroups ? Set(results.map(\.command.group)).count : 0
        let natural = CGFloat(results.count) * Self.rowHeight + CGFloat(groups) * Self.groupLabelHeight
        return min(natural, Self.maxListHeight)
    }

    var height: CGFloat {
        listHeight + Self.padding * 2 + (results.isEmpty ? 0 : Self.footerHeight)
    }
}

/// The slash menu's window (design board 56 A): a borderless panel that
/// never takes focus, so the text view keeps the caret and the keys while it
/// shows, child of the editor's window so it moves with it.
final class SlashMenuPanel: NSPanel {
    let model = SlashMenuModel()

    init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: SlashMenuModel.width, height: 100),
                   styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        level = .popUpMenu
        hidesOnDeactivate = true
        isReleasedWhenClosed = false
        let host = NSHostingView(rootView: SlashMenuView(model: model))
        host.sizingOptions = []
        contentView = host
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    /// Shows the panel under (or, without room, above) `anchor` — the
    /// slash's rect in screen coordinates — kept inside `bounds`.
    func show(below anchor: NSRect, within bounds: NSRect, parent: NSWindow) {
        let size = NSSize(width: SlashMenuModel.width, height: model.height)
        // Rows are inset 5 + 8: line the item names' tiles up with the slash.
        var x = anchor.minX - SlashMenuModel.padding - 4
        x = min(x, bounds.maxX - size.width)
        x = max(x, bounds.minX)
        let gap: CGFloat = 6
        var y = anchor.minY - gap - size.height
        if y < bounds.minY, anchor.maxY + gap + size.height <= bounds.maxY {
            y = anchor.maxY + gap
        }
        setFrame(NSRect(origin: NSPoint(x: x, y: y), size: size), display: true)
        if parent != self.parent {
            self.parent?.removeChildWindow(self)
            parent.addChildWindow(self, ordered: .above)
        }
        orderFront(nil)
        invalidateShadow()
    }

    func hide() {
        parent?.removeChildWindow(self)
        orderOut(nil)
    }
}

/// Watches one text view's slash menu: opens it when a `/` is typed where
/// it may, filters as the query grows, and closes it when the caret leaves.
final class SlashMenuController {
    private weak var textView: NSTextView?
    private lazy var panel = SlashMenuPanel()
    /// The slash, in raw-Markdown coordinates (stable across restyles).
    private(set) var anchor: Int?
    /// Query length when there were last any results, so a run of misses
    /// closes the menu (three letters past the last match).
    private var lastMatchedLength = 0
    private var scrollObserver: NSObjectProtocol?

    init(textView: NSTextView) {
        self.textView = textView
    }

    deinit {
        if let scrollObserver { NotificationCenter.default.removeObserver(scrollObserver) }
    }

    var isOpen: Bool { anchor != nil }
    var model: SlashMenuModel { panel.model }

    func open(at rawAnchor: Int, onPick: @escaping (SlashCommand) -> Void) {
        anchor = rawAnchor
        lastMatchedLength = 0
        model.query = ""
        model.results = SlashCommand.results(for: "")
        model.selectedIndex = 0
        model.onPick = onPick
        if scrollObserver == nil, let clip = textView?.enclosingScrollView?.contentView {
            clip.postsBoundsChangedNotifications = true
            scrollObserver = NotificationCenter.default.addObserver(
                forName: NSView.boundsDidChangeNotification, object: clip, queue: .main
            ) { [weak self] _ in self?.reposition() }
        }
        reposition()
    }

    /// Filters for the text now typed after the slash; false (and closed)
    /// when the menu has run out of reasons to stay.
    @discardableResult
    func update(query: String) -> Bool {
        guard isOpen else { return false }
        let results = SlashCommand.results(for: query)
        let length = (query as NSString).length
        if !results.isEmpty {
            lastMatchedLength = length
        } else if length - lastMatchedLength >= 3 {
            close()
            return false
        }
        let keep = model.selected?.id
        model.query = query
        model.results = results
        model.selectedIndex = query.isEmpty ? (results.firstIndex { $0.command.id == keep } ?? 0) : 0
        reposition()
        return true
    }

    func moveSelection(by step: Int) {
        guard !model.results.isEmpty else { return }
        model.selectedIndex = min(max(0, model.selectedIndex + step), model.results.count - 1)
    }

    func close() {
        guard isOpen else { return }
        anchor = nil
        if let scrollObserver {
            NotificationCenter.default.removeObserver(scrollObserver)
            self.scrollObserver = nil
        }
        panel.hide()
    }

    /// Display location of the slash; the text view maps the raw anchor.
    var displayAnchor: (() -> Int?)?

    func reposition() {
        guard isOpen, let textView, let window = textView.window,
              let location = displayAnchor?(), location < (textView.string as NSString).length else { return }
        let slash = textView.firstRect(forCharacterRange: NSRange(location: location, length: 1), actualRange: nil)
        guard slash != .zero else { return }
        // Keep it on the page: the editor's scroll view, in screen space.
        let pageView: NSView = textView.enclosingScrollView ?? textView
        let page = window.convertToScreen(pageView.convert(pageView.bounds, to: nil))
        panel.show(below: slash, within: page.insetBy(dx: 8, dy: 8), parent: window)
    }
}

// MARK: - View

struct SlashMenuView: View {
    @Bindable var model: SlashMenuModel
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if model.results.isEmpty {
                Text("No results")
                    .font(.fumi(.body))
                    .foregroundColor(.tertiaryText)
                    .padding(.horizontal, 8)
                    .frame(maxWidth: .infinity, minHeight: SlashMenuModel.rowHeight, alignment: .leading)
            } else {
                list
                footer
            }
        }
        .padding(SlashMenuModel.padding)
        .frame(width: SlashMenuModel.width, height: model.height, alignment: .top)
        .background { background }
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private var list: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical, showsIndicators: model.listHeight >= SlashMenuModel.maxListHeight) {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(model.results.enumerated()), id: \.element.command.id) { index, result in
                        if model.showsGroups, index == 0 || model.results[index - 1].command.group != result.command.group {
                            Text(result.command.group.rawValue)
                                .font(.system(size: 11, weight: .medium))
                                .foregroundColor(.tertiaryText)
                                .padding(.horizontal, 8)
                                .padding(.top, index == 0 ? 2 : 6)
                                .frame(height: SlashMenuModel.groupLabelHeight, alignment: .bottomLeading)
                                .padding(.bottom, 0)
                        }
                        SlashMenuRow(result: result, isSelected: index == model.selectedIndex)
                            .id(result.command.id)
                            .contentShape(Rectangle())
                            .onHover { if $0 { model.selectedIndex = index } }
                            .onTapGesture { model.onPick(result.command) }
                    }
                }
            }
            .frame(height: model.listHeight)
            .onChange(of: model.selectedIndex) { _, index in
                guard model.results.indices.contains(index) else { return }
                proxy.scrollTo(model.results[index].command.id)
            }
        }
    }

    private var footer: some View {
        HStack(spacing: 12) {
            Text("↑↓ Move")
            Text("↵ Insert")
            Text("esc Close")
        }
        .font(.system(size: 11))
        .foregroundColor(.tertiaryText)
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, minHeight: SlashMenuModel.footerHeight - 1, alignment: .leading)
        .overlay(alignment: .top) { Rectangle().fill(Palette.border).frame(height: 1) }
        .padding(.top, 0)
    }

    @ViewBuilder
    private var background: some View {
        let shape = RoundedRectangle(cornerRadius: 10, style: .continuous)
        if ThemeManager.shared.isGlass, #available(macOS 26, *) {
            Color.clear.glassEffect(.regular, in: shape)
        } else {
            shape.fill(Palette.surface)
                .overlay(shape.strokeBorder(Color.primary.opacity(colorScheme == .dark ? 0.12 : 0.08)))
        }
    }
}

private struct SlashMenuRow: View {
    let result: SlashCommand.Result
    let isSelected: Bool
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        HStack(spacing: 9) {
            tile
            title
            Spacer(minLength: 6)
            if !result.command.hint.isEmpty {
                Text(result.command.hint)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(.tertiaryText)
            }
        }
        .padding(.horizontal, 8)
        .frame(height: SlashMenuModel.rowHeight)
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(isSelected ? ThemeManager.shared.accentColor.opacity(colorScheme == .dark ? 0.24 : 0.12) : .clear)
        )
    }

    /// The title with the matched letters bold.
    private var title: Text {
        let characters = Array(result.command.title.utf16)
        let matched = Set(result.titleOffsets)
        var text = Text("")
        var run: [UInt16] = []
        var runBold = false
        func flush() {
            guard !run.isEmpty else { return }
            let piece = Text(String(decoding: run, as: UTF16.self))
            text = text + (runBold ? piece.fontWeight(.semibold) : piece)
            run = []
        }
        for (offset, unit) in characters.enumerated() {
            let bold = matched.contains(offset)
            if bold != runBold { flush(); runBold = bold }
            run.append(unit)
        }
        flush()
        return text.font(.fumi(.body)).foregroundColor(.primaryText)
    }

    private var tile: some View {
        let shape = RoundedRectangle(cornerRadius: 5, style: .continuous)
        return Group {
            switch result.command.icon {
            case .letters(let letters):
                Text(letters).font(.system(size: 9.5, weight: .semibold))
            case .serifLetters(let letters):
                Text(letters).font(.system(size: 11, weight: .medium, design: .serif))
            case .symbol(let name):
                Image(systemName: name).font(.system(size: 10.5, weight: .medium))
            }
        }
        .foregroundColor(.secondaryText)
        .frame(width: 22, height: 22)
        .background(shape.fill(Palette.surface.opacity(ThemeManager.shared.isGlass ? 0.6 : 1)))
        .overlay(shape.strokeBorder(isSelected ? ThemeManager.shared.accentColor.opacity(0.35) : Palette.controlBorder))
    }
}
