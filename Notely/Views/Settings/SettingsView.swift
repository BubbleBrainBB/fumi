import AppKit
import ServiceManagement
import SwiftUI
import UniformTypeIdentifiers

// Settings as one sheet of paper over the dimmed window
// (artifacts/settings-redesign): a title, then Appearance, Writing, Library
// and Publish read top to bottom, each a bold name over its rows, the groups
// split by a hairline. Switches for on / off, pop-up menus for the rest; the
// keyboard shortcuts fold out at the foot. Every row changes something real
// — Export lives in the File menu and About in the app menu.

// MARK: - Modal

/// The settings sheet. Shown by AppShellView over a dimmed window; esc, ⌘.
/// or a click outside closes it.
struct SettingsModal: View {
    @Environment(AppModel.self) private var appModel
    @Environment(ThemeManager.self) private var themeManager
    @Environment(\.colorScheme) private var colorScheme

    static let width: CGFloat = 560
    static let radius: CGFloat = 14

    private var isDark: Bool { colorScheme == .dark }
    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: Self.radius, style: .continuous) }

    var body: some View {
        GeometryReader { proxy in
            // As tall as its content, never closer than 40 to the window's
            // edges; past that the sheet scrolls inside.
            sheet(maxHeight: max(260, proxy.size.height - 80))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func sheet(maxHeight: CGFloat) -> some View {
        SettingsPage(maxHeight: maxHeight)
            .frame(width: Self.width)
            .modifier(SettingsSheetSurface(isGlass: themeManager.isGlass, isDark: isDark, shape: shape))
            // The whole sheet takes clicks: a click on an empty spot must not
            // fall through to the dimmed window and close it.
            .contentShape(shape)
            .shadow(color: .black.opacity(isDark ? 0.45 : 0.16), radius: 40, y: 30)
            .background {
                // esc and ⌘. close the sheet whichever control has focus.
                ZStack {
                    Button("Close Settings", action: close).keyboardShortcut(.cancelAction)
                    Button("Close Settings", action: close).keyboardShortcut(".", modifiers: .command)
                }
                .opacity(0)
                .accessibilityHidden(true)
            }
    }

    private func close() {
        appModel.showSettings = false
    }
}

/// The sheet's paper. Glass: frosted white (graphite) Liquid Glass — the
/// window behind shows through, the text still sits on white — with a
/// white highlight edge; older systems frost the window's own content.
/// Other themes: the theme's page color with a faint edge.
private struct SettingsSheetSurface<S: InsettableShape>: ViewModifier {
    let isGlass: Bool
    let isDark: Bool
    let shape: S

    func body(content: Content) -> some View {
        if isGlass {
            if #available(macOS 26, *) {
                content
                    .background(Color.editorBg.opacity(isDark ? 0.45 : 0.6))
                    .clipShape(shape)
                    .glassEffect(.regular, in: shape)
                    .overlay(shape.strokeBorder(Color.white.opacity(isDark ? 0.12 : 0.75), lineWidth: 1))
            } else {
                content
                    .background {
                        ZStack {
                            WithinWindowBlur()
                            Color.editorBg.opacity(isDark ? 0.55 : 0.7)
                        }
                    }
                    .clipShape(shape)
                    .overlay(shape.strokeBorder(Color.white.opacity(isDark ? 0.14 : 0.9), lineWidth: 1))
            }
        } else {
            content
                .background(Color.editorBg)
                .clipShape(shape)
                .overlay(shape.strokeBorder(isDark ? Color.white.opacity(0.07) : Color.black.opacity(0.05), lineWidth: 1))
        }
    }
}

/// Frosts what the window itself shows behind the sheet — never the
/// desktop, so no wallpaper color reaches the glass.
private struct WithinWindowBlur: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .popover
        view.blendingMode = .withinWindow
        view.state = .active
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {}
}

// MARK: - Page

/// The sheet: a fixed header, then the content — at its own height when it
/// fits, otherwise in a scroll view whose bottom edge fades to say there is
/// more, with the editor's slim scroller.
private struct SheetPage<Header: View, Content: View>: View {
    let maxHeight: CGFloat
    @ViewBuilder let header: () -> Header
    @ViewBuilder let content: () -> Content

    var body: some View {
        HeightCap(maxHeight: maxHeight) {
            VStack(spacing: 0) {
                header()
                    .padding(.top, 24)
                    .padding(.horizontal, 28)
                ViewThatFits(in: .vertical) {
                    paddedContent
                    ScrollView(.vertical) {
                        paddedContent
                            .background(QuietScrollerInstaller())
                    }
                    .mask {
                        VStack(spacing: 0) {
                            Color.black
                            LinearGradient(colors: [.black, .clear], startPoint: .top, endPoint: .bottom)
                                .frame(height: 32)
                        }
                    }
                }
            }
        }
    }

    private var paddedContent: some View {
        content()
            .padding(.horizontal, 28)
            .padding(.bottom, 20)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Offers its content at most `maxHeight` and takes the height the content
/// asks for — a plain `.frame(maxHeight:)` would grow to the cap.
private struct HeightCap: Layout {
    let maxHeight: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let height = min(proposal.height ?? maxHeight, maxHeight)
        return subviews.first?.sizeThatFits(ProposedViewSize(width: proposal.width, height: height)) ?? .zero
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        subviews.first?.place(at: bounds.origin, proposal: ProposedViewSize(bounds.size))
    }
}

private struct SettingsPage: View {
    let maxHeight: CGFloat

    private var version: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0"
    }

    var body: some View {
        SheetPage(maxHeight: maxHeight) {
            Text("Settings")
                .font(.notely(.title, weight: .semibold))
                .foregroundColor(.primaryText)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.bottom, 4)
                .accessibilityAddTraits(.isHeader)
        } content: {
            VStack(alignment: .leading, spacing: 0) {
                SettingsSection(title: "Appearance", isFirst: true) { LookSettings() }
                SettingsSection(title: "Writing") { WritingSettings() }
                SettingsSection(title: "Library") { LibrarySettings() }
                SettingsSection(title: "Publish") { PublishSettings() }
                ShortcutsDisclosure()
                    .padding(.vertical, 12)
                    .overlay(alignment: .top) { SettingsHairline() }
                Text("Fumi \(version)")
                    .font(.notely(.caption))
                    .foregroundColor(.faintText)
                    .padding(.top, 4)
            }
        }
    }
}

// MARK: - Building blocks

/// A group: its name in bold over its rows, a hairline above every group
/// but the first.
private struct SettingsSection<Content: View>: View {
    let title: String
    var isFirst = false
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.notely(.body, weight: .semibold))
                .foregroundColor(.primaryText)
                .frame(height: 32)
                .accessibilityAddTraits(.isHeader)
            content()
        }
        .padding(.vertical, 12)
        .overlay(alignment: .top) {
            if !isFirst { SettingsHairline() }
        }
    }
}

/// One device pixel, between groups.
private struct SettingsHairline: View {
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        Rectangle().fill(Color.primaryText.opacity(0.09)).frame(height: 1 / displayScale)
    }
}

/// One setting: its name left (with what it points at under it, where
/// there is something), the control right.
private struct SettingRow<Control: View>: View {
    let title: String
    var detail: String? = nil
    @ViewBuilder let control: () -> Control

    var body: some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.notely(.body))
                    .foregroundColor(.primaryText)
                    .lineLimit(1)
                if let detail {
                    Text(detail)
                        .font(.notely(.caption))
                        .foregroundColor(.faintText)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            Spacer(minLength: 12)
            control()
        }
        .frame(minHeight: 36)
        .accessibilityElement(children: .contain)
    }
}

/// On / off: the system switch, small, in the accent.
private struct SettingsSwitch: View {
    @Binding var isOn: Bool
    let label: String

    var body: some View {
        Toggle(label, isOn: $isOn)
            .toggleStyle(.switch)
            .controlSize(.small)
            .tint(.accent)
            .labelsHidden()
    }
}

/// The sheet's controls share one look: a light gray fill, no outline, a
/// step darker under the pointer (or while its menu is open).
private struct ControlFill: ViewModifier {
    var isActive = false
    @State private var isHovering = false

    func body(content: Content) -> some View {
        content
            .frame(height: 24)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(Color.inlineFill)
                    .overlay(
                        RoundedRectangle(cornerRadius: 6, style: .continuous)
                            .fill(Color.primaryText.opacity(isHovering || isActive ? 0.05 : 0))
                    )
            )
            .contentShape(Rectangle())
            .onHover { isHovering = $0 }
            .animation(.easeOut(duration: 0.12), value: isHovering)
    }
}

/// A pop-up closed: an optional swatch, the value, one small down chevron.
private struct PopUpLabel: View {
    let title: String
    var font: Font = .notely(.body)
    var icon: NSImage? = nil
    var isActive = false

    var body: some View {
        HStack(spacing: 8) {
            if let icon { Image(nsImage: icon) }
            Text(title)
                .font(font)
                .foregroundColor(.primaryText)
                .lineLimit(1)
            Spacer(minLength: 0)
            Image(systemName: "chevron.down")
                .font(.system(size: 9, weight: .semibold))
                .foregroundColor(.tertiaryText)
        }
        .padding(.horizontal, 10)
        .frame(minWidth: 120)
        .modifier(ControlFill(isActive: isActive))
    }
}

/// A pop-up menu: the choices open as a system menu, the current one
/// checked.
private struct PopUpChoice<Value: Hashable>: View {
    let options: [(label: String, value: Value)]
    @Binding var selection: Value
    var icon: ((Value) -> NSImage)? = nil
    let accessibilityLabel: String

    var body: some View {
        let current = options.first { $0.value == selection }
        Menu {
            Picker(accessibilityLabel, selection: $selection) {
                ForEach(options, id: \.value) { option in
                    if let icon {
                        Label { Text(option.label) } icon: { Image(nsImage: icon(option.value)) }
                            .tag(option.value)
                    } else {
                        Text(option.label).tag(option.value)
                    }
                }
            }
            .pickerStyle(.inline)
            .labelsHidden()
        } label: {
            PopUpLabel(title: current?.label ?? "", icon: current.flatMap { icon?($0.value) })
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .accessibilityLabel(accessibilityLabel)
    }
}

/// A small action on the sheet.
private struct FillButton: View {
    let title: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.notely(.body))
                .foregroundColor(.primaryText)
                .padding(.horizontal, 10)
                .modifier(ControlFill())
        }
        .buttonStyle(.plain)
        .fixedSize()
    }
}

/// A plain text field on the sheet, on the controls' gray fill.
private struct SettingsTextField: View {
    let placeholder: String
    @Binding var text: String
    var width: CGFloat = 180

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(.notely(.body))
            .foregroundColor(.primaryText)
            .padding(.horizontal, 8)
            .frame(width: width, height: 24)
            .background(RoundedRectangle(cornerRadius: 6, style: .continuous).fill(Color.inlineFill))
            .accessibilityLabel(placeholder)
    }
}

/// A theme as a tiny desk — its window ground with a page on the right —
/// for the Theme pop-up and its menu.
enum ThemeSwatchImage {
    static func make(_ theme: AppTheme, dark: Bool) -> NSImage {
        NSImage(size: NSSize(width: 18, height: 13), flipped: false) { rect in
            let p = ThemePalette.palette(theme, dark: dark)
            let desk = NSBezierPath(roundedRect: rect, xRadius: 3, yRadius: 3)
            if theme == .glass {
                NSGradient(starting: .hex(dark ? 0x2B3038 : 0xE3EAF1), ending: .hex(dark ? 0x302C35 : 0xF5F2F7))?
                    .draw(in: desk, angle: -45)
            } else {
                NSColor.hex(p.ground).setFill()
                desk.fill()
            }
            let page = NSBezierPath(roundedRect: NSRect(x: rect.maxX - 12, y: 2, width: 10, height: rect.height - 4),
                                    xRadius: 1, yRadius: 1)
            NSColor.hex(p.page, alpha: theme == .glass ? 0.75 : 1).setFill()
            page.fill()
            let edge = dark ? NSColor.white.withAlphaComponent(0.16) : NSColor.black.withAlphaComponent(0.14)
            edge.setStroke()
            let outline = NSBezierPath(roundedRect: rect.insetBy(dx: 0.25, dy: 0.25), xRadius: 2.75, yRadius: 2.75)
            outline.lineWidth = 0.5
            outline.stroke()
            if p.page == p.ground {
                page.lineWidth = 0.5
                page.stroke()
            }
            return true
        }
    }
}

// MARK: - Appearance

private struct LookSettings: View {
    @Environment(ThemeManager.self) private var themeManager
    @Environment(\.colorScheme) private var colorScheme

    private let accentColors: [(name: String, hex: String)] = [
        ("Blue", "#1D6FC2"),
        ("Ink", ThemeManager.inkAccentHex),
        ("Green", "#448361"),
        ("Purple", "#9065B0"),
        ("Red", "#C4402F"),
    ]

    private var theme: Binding<AppTheme> {
        Binding(get: { themeManager.theme }, set: { themeManager.setTheme($0) })
    }

    private var mode: Binding<AppearanceMode> {
        Binding(get: { themeManager.mode }, set: { themeManager.setMode($0) })
    }

    var body: some View {
        let dark = colorScheme == .dark
        SettingRow(title: "Theme") {
            PopUpChoice(options: AppTheme.allCases.map { ($0.title, $0) }, selection: theme,
                        icon: { ThemeSwatchImage.make($0, dark: dark) }, accessibilityLabel: "Theme")
        }
        SettingRow(title: "Mode") {
            PopUpChoice(options: AppearanceMode.allCases.map { ($0 == .auto ? "Match System" : $0.title, $0) },
                        selection: mode, accessibilityLabel: "Mode")
        }
        SettingRow(title: "Accent") {
            HStack(spacing: 11) {
                ForEach(accentColors, id: \.hex) { color in
                    AccentDot(hex: color.hex, isSelected: themeManager.accentHex == color.hex) {
                        themeManager.setAccent(color.hex)
                    }
                    .help(color.name)
                    .accessibilityLabel(color.name)
                }
            }
        }
    }
}

private struct AccentDot: View {
    let hex: String
    let isSelected: Bool
    let action: () -> Void

    /// Ink is the theme's own ink, so its dot follows light and dark.
    private var fill: Color {
        hex == ThemeManager.inkAccentHex ? .primaryText : Color.fromHex(hex)
    }

    var body: some View {
        Button(action: action) {
            Circle()
                .fill(fill)
                .frame(width: 14, height: 14)
                .overlay {
                    if isSelected {
                        Circle().strokeBorder(fill, lineWidth: 1.5).frame(width: 21, height: 21)
                    }
                }
                .frame(width: 21, height: 21)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, -3.5)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

// MARK: - Writing

private struct WritingSettings: View {
    @AppStorage(AppSettings.editorFontKey) private var editorFont = "System"
    @AppStorage(AppSettings.fontSizeKey) private var fontSize = AppSettings.defaultEditorFontSize
    @AppStorage(AppSettings.lineHeightKey) private var lineHeight = 1.7
    @AppStorage(AppSettings.editorWidthKey) private var editorWidth = "medium"
    @AppStorage(AppSettings.spellCheckKey) private var spellCheck = AppSettings.defaultSpellCheck
    @AppStorage(AppSettings.smartPunctuationKey) private var smartPunctuation = AppSettings.defaultSmartPunctuation
    @AppStorage(AppSettings.slashCommandsKey) private var slashCommands = AppSettings.defaultSlashCommands

    private var lineSpacing: Binding<LineSpacingPreset> {
        Binding(get: { LineSpacingPreset.from(multiplier: lineHeight) },
                set: { lineHeight = $0.multiplier })
    }

    var body: some View {
        SettingRow(title: "Typeface") {
            FontPicker(selection: editorFont) { editorFont = $0 }
        }
        SettingRow(title: "Size") {
            SizeStepper(value: $fontSize, range: 12...24)
        }
        SettingRow(title: "Line spacing") {
            PopUpChoice(options: LineSpacingPreset.allCases.map { ($0.label, $0) },
                        selection: lineSpacing, accessibilityLabel: "Line spacing")
        }
        SettingRow(title: "Line length") {
            PopUpChoice(options: [("Narrow", "narrow"), ("Medium", "medium"), ("Wide", "wide")],
                        selection: $editorWidth, accessibilityLabel: "Line length")
        }
        SettingRow(title: "Check spelling") {
            SettingsSwitch(isOn: $spellCheck, label: "Check spelling")
        }
        SettingRow(title: "Smart quotes and dashes") {
            SettingsSwitch(isOn: $smartPunctuation, label: "Smart quotes and dashes")
        }
        SettingRow(title: "Slash commands") {
            SettingsSwitch(isOn: $slashCommands, label: "Slash commands")
        }
    }
}

/// − 16 +: one gray control; each end steps the size.
private struct SizeStepper: View {
    @Binding var value: Double
    let range: ClosedRange<Double>

    var body: some View {
        HStack(spacing: 0) {
            StepButton(symbol: "minus", label: "Smaller", isEnabled: value > range.lowerBound) {
                value = max(range.lowerBound, value - 1)
            }
            Text("\(Int(value))")
                .font(.notely(.body))
                .monospacedDigit()
                .foregroundColor(.primaryText)
                .frame(minWidth: 24)
                .accessibilityLabel("Size \(Int(value))")
            StepButton(symbol: "plus", label: "Larger", isEnabled: value < range.upperBound) {
                value = min(range.upperBound, value + 1)
            }
        }
        .frame(height: 24)
        .background(RoundedRectangle(cornerRadius: 6, style: .continuous).fill(Color.inlineFill))
    }
}

private struct StepButton: View {
    let symbol: String
    let label: String
    let isEnabled: Bool
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
                .foregroundColor(.secondaryText)
                .frame(width: 26, height: 24)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(isHovering && isEnabled ? Color.primaryText.opacity(0.06) : .clear)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
        .opacity(isEnabled ? 1 : 0.35)
        .onHover { isHovering = $0 }
        .accessibilityLabel(label)
    }
}

// MARK: - Font picker

/// The editor's typeface, written in itself, as a pop-up. Opens a
/// searchable list: System / New York / SF Mono (which NSFontManager does
/// not expose as regular families) first, then every installed family, each
/// in its own face.
struct FontPicker: View {
    let selection: String
    let onSelect: (String) -> Void

    @State private var isPresented = false
    @State private var searchText = ""
    @State private var hovered: String?

    private let pinnedFonts = ["System", "New York", "SF Mono"]

    private var installedFonts: [String] {
        NSFontManager.shared.availableFontFamilies
            .filter { !$0.hasPrefix(".") && !pinnedFonts.contains($0) }
            .sorted()
    }

    private var query: String {
        searchText.trimmingCharacters(in: .whitespaces).lowercased()
    }

    private func matching(_ fonts: [String]) -> [String] {
        query.isEmpty ? fonts : fonts.filter { $0.lowercased().contains(query) }
    }

    private func previewFont(for name: String) -> Font {
        Font(WysiwygEngine.resolveFont(name: name, size: TextSize.body.rawValue))
    }

    var body: some View {
        Button {
            isPresented.toggle()
        } label: {
            PopUpLabel(title: selection, font: previewFont(for: selection), isActive: isPresented)
        }
        .buttonStyle(.plain)
        .fixedSize()
        .accessibilityLabel("Typeface, \(selection)")
        .popover(isPresented: $isPresented, arrowEdge: .bottom) { list }
    }

    private var list: some View {
        VStack(spacing: 6) {
            TextField("Search fonts", text: $searchText)
                .textFieldStyle(.plain)
                .font(.notely(.caption))
                .foregroundColor(.primaryText)
                .padding(.horizontal, 9)
                .frame(height: 28)
                .background(RoundedRectangle(cornerRadius: 7, style: .continuous).fill(Color.searchFieldBg))
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    let pinned = matching(pinnedFonts)
                    let installed = matching(installedFonts)
                    ForEach(pinned, id: \.self) { row($0) }
                    if !pinned.isEmpty && !installed.isEmpty {
                        SettingsHairline()
                            .padding(.horizontal, 9).padding(.vertical, 5)
                    }
                    ForEach(installed, id: \.self) { row($0) }
                }
                .background(QuietScrollerInstaller())
            }
        }
        .padding(8)
        .frame(width: 250, height: 340)
        // The popover's own material follows the system's light or dark,
        // not Notely's mode; the theme's page keeps the text readable.
        .presentationBackground(Color.editorBg)
    }

    private func row(_ name: String) -> some View {
        let isSelected = name == selection
        return HStack {
            Text(name)
                .font(previewFont(for: name))
                .foregroundColor(.primaryText)
                .lineLimit(1)
            Spacer()
            if isSelected {
                Text("✓")
                    .font(.notely(.caption))
                    .foregroundColor(.accent)
            }
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(isSelected ? Color.accent.opacity(0.08) : (hovered == name ? Color.cardHover : .clear))
        )
        .contentShape(Rectangle())
        .onHover { hovered = $0 ? name : (hovered == name ? nil : hovered) }
        .onTapGesture {
            onSelect(name)
            isPresented = false
        }
        .accessibilityAddTraits(isSelected ? [.isButton, .isSelected] : .isButton)
    }
}

// MARK: - Library

private struct LibrarySettings: View {
    @Environment(FileNoteStore.self) private var store
    @Environment(AppModel.self) private var appModel
    @State private var openAtLogin = SMAppService.mainApp.status == .enabled
    @State private var loginItemError: String?

    private var folderName: String {
        store.workspaceURL?.lastPathComponent ?? "No folder open"
    }

    private var folderPath: String? {
        guard let url = store.workspaceURL else { return nil }
        return (url.path as NSString).abbreviatingWithTildeInPath
    }

    var body: some View {
        SettingRow(title: folderName, detail: folderPath) {
            HStack(spacing: 8) {
                if let url = store.workspaceURL {
                    FillButton(title: "Show in Finder") {
                        NSWorkspace.shared.activateFileViewerSelecting([url])
                    }
                }
                FillButton(title: "Change…") {
                    guard let url = NotesFolderPanel.choose() else { return }
                    store.openWorkspace(at: url)
                    appModel.openNote(nil)
                }
            }
        }
        SettingRow(title: "Open at login", detail: loginItemError) {
            SettingsSwitch(isOn: Binding(get: { openAtLogin }, set: setOpenAtLogin), label: "Open at login")
        }
    }

    private func setOpenAtLogin(_ enabled: Bool) {
        guard enabled != openAtLogin else { return }
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
            loginItemError = nil
        } catch {
            loginItemError = "Couldn't change this: \(error.localizedDescription)"
        }
        openAtLogin = SMAppService.mainApp.status == .enabled
    }
}

// MARK: - Publish

/// The optional publishing tools, each off until switched on; its options
/// fold out under it.
private struct PublishSettings: View {
    @AppStorage(AppSettings.wechatExtensionKey) private var wechat = false
    @AppStorage(AppSettings.wechatThemeKey) private var theme = WeChatRenderer.Theme.blackRed.rawValue
    @AppStorage(AppSettings.xhsExtensionKey) private var xhs = false

    var body: some View {
        SettingRow(title: "WeChat Article") {
            SettingsSwitch(isOn: $wechat.animation(.easeOut(duration: 0.2)), label: "WeChat Article")
        }
        if wechat {
            SettingRow(title: "Theme") {
                PopUpChoice(options: WeChatRenderer.Theme.allCases.map { ($0.name, $0.rawValue) },
                            selection: $theme, accessibilityLabel: "WeChat theme")
            }
            .padding(.leading, 16)
            .transition(.opacity)
        }
        SettingRow(title: "Xiaohongshu Cards") {
            SettingsSwitch(isOn: $xhs.animation(.easeOut(duration: 0.2)), label: "Xiaohongshu Cards")
        }
        if xhs {
            XhsSettings()
                .padding(.leading, 16)
                .transition(.opacity)
        }
    }
}

/// Xiaohongshu Cards' defaults (board ㊾ ⑤). The preview may choose another
/// template or size for one note; that stays with the note.
private struct XhsSettings: View {
    @AppStorage(AppSettings.xhsTemplateKey) private var template = XhsCards.Template.memo.rawValue
    @AppStorage(AppSettings.xhsRatioKey) private var ratio = XhsCards.Ratio.portrait.rawValue
    @AppStorage(AppSettings.xhsPageNumbersKey) private var pageNumbers = true
    @AppStorage(AppSettings.xhsNameKey) private var name = ""
    @AppStorage(AppSettings.xhsHandleKey) private var handle = ""
    @AppStorage(AppSettings.xhsAvatarKey) private var avatar = ""
    @AppStorage(AppSettings.xhsEndCardKey) private var endCard = false
    @AppStorage(AppSettings.xhsEndLineKey) private var endLine = ""

    var body: some View {
        VStack(spacing: 0) {
            SettingRow(title: "Template") {
                PopUpChoice(options: XhsCards.Template.allCases.map { ($0.name, $0.rawValue) },
                            selection: $template, accessibilityLabel: "Xiaohongshu template")
            }
            SettingRow(title: "Size") {
                PopUpChoice(options: XhsCards.Ratio.allCases.map { ($0.rawValue, $0.rawValue) },
                            selection: $ratio, accessibilityLabel: "Card size")
            }
            SettingRow(title: "Avatar") {
                HStack(spacing: 8) {
                    AvatarDot(path: avatar, name: name)
                    if !avatar.isEmpty {
                        FillButton(title: "Remove") { removeAvatar() }
                    }
                    FillButton(title: "Choose…") { chooseAvatar() }
                }
            }
            SettingRow(title: "Name") {
                SettingsTextField(placeholder: NSFullUserName(), text: $name)
            }
            SettingRow(title: "Handle") {
                SettingsTextField(placeholder: "@name", text: $handle)
            }
            SettingRow(title: "Page numbers") {
                SettingsSwitch(isOn: $pageNumbers, label: "Page numbers")
            }
            SettingRow(title: "End card") {
                SettingsSwitch(isOn: $endCard.animation(.easeOut(duration: 0.2)), label: "End card")
            }
            if endCard {
                SettingRow(title: "End card line") {
                    SettingsTextField(placeholder: "One line to close with", text: $endLine, width: 220)
                }
                .padding(.leading, 16)
                .transition(.opacity)
            }
        }
    }

    /// The picture is copied into Notely's own folder, so moving or
    /// deleting the original doesn't take it off the cards.
    private func chooseAvatar() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.png, .jpeg, .heic, .webP, .tiff]
        panel.allowsMultipleSelection = false
        panel.message = "Choose a picture for your avatar on Post cards."
        guard panel.runModal() == .OK, let source = panel.url,
              let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else { return }
        let folder = support.appendingPathComponent("Notely", isDirectory: true)
        let target = folder.appendingPathComponent("xhs-avatar-\(Int(Date().timeIntervalSince1970)).\(source.pathExtension.lowercased())")
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try FileManager.default.copyItem(at: source, to: target)
            removeAvatar()
            avatar = target.path
        } catch {
            NSSound.beep()
        }
    }

    private func removeAvatar() {
        if !avatar.isEmpty { try? FileManager.default.removeItem(atPath: avatar) }
        avatar = ""
    }
}

/// The avatar as the Post template shows it: the picture, or the name's
/// first letter on a quiet circle.
private struct AvatarDot: View {
    let path: String
    let name: String

    var body: some View {
        Group {
            if !path.isEmpty, let image = NSImage(contentsOfFile: path) {
                Image(nsImage: image).resizable().scaledToFill()
            } else {
                Text(String((name.isEmpty ? NSFullUserName() : name).prefix(1)).uppercased())
                    .font(.notely(.caption, weight: .semibold))
                    .foregroundColor(Color(nsColor: .adaptive(light: 0x6B6457, dark: 0xC9C9C6)))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color(nsColor: .adaptive(light: 0xE4DFD4, dark: 0x34363B)))
            }
        }
        .frame(width: 24, height: 24)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }
}

// MARK: - Shortcuts

/// The app's real commands (see AppCommands), read-only and folded away
/// until asked for. Keys are plain faint text, no keycaps.
private struct ShortcutsDisclosure: View {
    @State private var isOpen = false

    private let groups: [(title: String, items: [(name: String, keys: String)])] = [
        ("Notes", [
            ("New Note", "⌘N"),
            ("New Window", "⇧⌘N"),
            ("Open Folder…", "⌘O"),
            ("Search Notes", "⇧⌘F"),
            ("Toggle Sidebar", "⌥⌘S"),
            ("Back", "⌘["),
            ("Forward", "⌘]"),
            ("Settings", "⌘,"),
        ]),
        ("Tabs", [
            ("New Tab", "⌘T"),
            ("Close Tab", "⌘W"),
            ("Reopen Closed Tab", "⇧⌘T"),
            ("Show Next Tab", "⌃⇥"),
            ("Show Previous Tab", "⌃⇧⇥"),
            ("Pin / Unpin Tab", "⌥⌘P"),
        ]),
        ("Format", [
            ("Bold", "⌘B"),
            ("Italic", "⌘I"),
            ("Insert Link", "⌘K"),
            ("Toggle To-do", "⌘L"),
        ]),
        ("Publish", [
            ("Show Preview", "⌃⌘P"),
            ("Copy for WeChat / Add Cards to Photos", "⌥⌘C"),
        ]),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(.easeOut(duration: 0.2)) { isOpen.toggle() }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundColor(.tertiaryText)
                        .rotationEffect(.degrees(isOpen ? 90 : 0))
                    Text("Keyboard shortcuts")
                        .font(.notely(.body, weight: .semibold))
                        .foregroundColor(.primaryText)
                }
                .frame(height: 32)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityAddTraits(.isHeader)

            if isOpen {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(groups, id: \.title) { group in
                        Text(group.title)
                            .font(.notely(.caption))
                            .foregroundColor(.faintText)
                            .padding(.top, 8)
                            .padding(.bottom, 2)
                        ForEach(Array(group.items.enumerated()), id: \.offset) { _, item in
                            HStack {
                                Text(item.name)
                                    .font(.notely(.body))
                                    .foregroundColor(.primaryText)
                                Spacer()
                                Text(item.keys)
                                    .font(.notely(.body))
                                    .foregroundColor(.faintText)
                            }
                            .frame(height: 28)
                            .accessibilityElement(children: .combine)
                        }
                    }
                }
                .padding(.leading, 16)
                .transition(.opacity)
            }
        }
    }
}

// MARK: - Memo editor buttons

/// A small raised button (the memo editor's Cancel / Save).
struct SettingsButton: View {
    let title: String
    let action: () -> Void
    @State private var isHovering = false
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.notely(.caption))
                .foregroundColor(.primaryText)
                .padding(.horizontal, 12)
                .frame(height: 26)
                .background(
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .fill(colorScheme == .dark ? Color.white.opacity(isHovering ? 0.16 : 0.12) : (isHovering ? Color.quietSurface : Color.white))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .strokeBorder(colorScheme == .dark ? Color.white.opacity(0.08) : Color.black.opacity(0.12), lineWidth: 0.5)
                )
                .shadow(color: .black.opacity(0.05), radius: 1, y: 1)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .fixedSize()
        .onHover { isHovering = $0 }
    }
}

// MARK: - Export (File menu)

enum NoteExporter {
    /// Writes every note as a Markdown file into a folder the user picks.
    static func exportAll(_ notes: [FileNote]) {
        let panel = NSOpenPanel()
        panel.title = "Choose Export Folder"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = "Export"
        guard panel.runModal() == .OK, let dir = panel.url else { return }

        for note in notes {
            let title = note.title.isEmpty ? note.filename : note.title
            let safe = title
                .components(separatedBy: CharacterSet(charactersIn: "/\\:*?\"<>|"))
                .joined(separator: "-")
                .trimmingCharacters(in: .whitespaces)
            let name = (safe.isEmpty ? "Untitled" : safe) + ".md"
            try? note.content.write(to: dir.appendingPathComponent(name), atomically: true, encoding: .utf8)
        }
    }
}
