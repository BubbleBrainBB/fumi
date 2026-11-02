import Foundation
import SwiftUI

/// Centralized user defaults for app preferences.
enum AppSettings {
    static let themeKey = "fumi.theme"
    static let fontSizeKey = "fumi.editorFontSize"
    static let lineHeightKey = "fumi.editorLineHeight"
    static let sortModeKey = "fumi.sortMode"
    static let sidebarWidthKey = "fumi.sidebarWidth"
    static let noteListWidthKey = "fumi.noteListWidth"
    static let launchAtLoginKey = "fumi.launchAtLogin"
    static let autoUpdatesKey = "fumi.autoUpdates"
    static let accentColorKey = "fumi.accentColor"
    static let workspacePathKey = "fumi.workspacePath"
    static let recentWorkspacesKey = "fumi.recentWorkspaces"
    /// Only read to migrate away from the removed theme presets.
    static let appThemeKey = "fumi.appTheme"
    static let editorFontKey = "fumi.editorFont"
    static let editorWidthKey = "fumi.editorWidth"
    static let spellCheckKey = "fumi.spellCheck"
    static let autoPairKey = "fumi.autoPair"
    static let smartPunctuationKey = "fumi.smartPunctuation"
    static let slashCommandsKey = "fumi.slashCommands"
    static let markdownSyntaxKey = "fumi.markdownSyntax"
    static let tabSizeKey = "fumi.tabSize"
    static let wordWrapKey = "fumi.wordWrap"
    static let typewriterKey = "fumi.typewriter"
    static let exportFormatKey = "fumi.exportFormat"
    static let includeFrontmatterKey = "fumi.includeFrontmatter"
    static let preserveTagsKey = "fumi.preserveTags"
    static let imageHandlingKey = "fumi.imageHandling"
    /// Extensions (Settings › Extensions), each off until turned on.
    static let wechatExtensionKey = "fumi.extensions.wechat"
    static let wechatThemeKey = "fumi.wechat.theme"

    static var wechatExtension: Bool {
        get { UserDefaults.standard.bool(forKey: wechatExtensionKey) }
        set { UserDefaults.standard.set(newValue, forKey: wechatExtensionKey) }
    }

    /// The Xiaohongshu Cards extension and its defaults (each note may
    /// choose its own template and size in the preview).
    static let xhsExtensionKey = "fumi.extensions.xiaohongshu"
    static let xhsTemplateKey = "fumi.xhs.template"
    static let xhsRatioKey = "fumi.xhs.ratio"
    static let xhsPageNumbersKey = "fumi.xhs.pageNumbers"
    static let xhsNameKey = "fumi.xhs.name"
    static let xhsHandleKey = "fumi.xhs.handle"
    static let xhsAvatarKey = "fumi.xhs.avatar"
    static let xhsEndCardKey = "fumi.xhs.endCard"
    static let xhsEndLineKey = "fumi.xhs.endLine"

    static var xhsExtension: Bool {
        get { UserDefaults.standard.bool(forKey: xhsExtensionKey) }
        set { UserDefaults.standard.set(newValue, forKey: xhsExtensionKey) }
    }

    /// Either publishing extension on: the toolbar's preview button shows.
    static var anyPublishExtension: Bool { wechatExtension || xhsExtension }

    static var theme: String {
        get { UserDefaults.standard.string(forKey: themeKey) ?? "system" }
        set { UserDefaults.standard.set(newValue, forKey: themeKey) }
    }

    static let defaultEditorFontSize: Double = 16

    static var editorFontSize: Double {
        get { UserDefaults.standard.object(forKey: fontSizeKey) as? Double ?? defaultEditorFontSize }
        set { UserDefaults.standard.set(newValue, forKey: fontSizeKey) }
    }

    static var editorLineHeight: Double {
        get { UserDefaults.standard.object(forKey: lineHeightKey) as? Double ?? 1.7 }
        set { UserDefaults.standard.set(newValue, forKey: lineHeightKey) }
    }

    static var sortMode: String {
        get { UserDefaults.standard.string(forKey: sortModeKey) ?? "updated-desc" }
        set { UserDefaults.standard.set(newValue, forKey: sortModeKey) }
    }

    static var sidebarWidth: Double {
        get { UserDefaults.standard.object(forKey: sidebarWidthKey) as? Double ?? 220 }
        set { UserDefaults.standard.set(newValue, forKey: sidebarWidthKey) }
    }

    static var noteListWidth: Double {
        get { UserDefaults.standard.object(forKey: noteListWidthKey) as? Double ?? 300 }
        set { UserDefaults.standard.set(newValue, forKey: noteListWidthKey) }
    }

    static var launchAtLogin: Bool {
        get { UserDefaults.standard.bool(forKey: launchAtLoginKey) }
        set { UserDefaults.standard.set(newValue, forKey: launchAtLoginKey) }
    }

    static var autoUpdates: Bool {
        get { UserDefaults.standard.object(forKey: autoUpdatesKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: autoUpdatesKey) }
    }


    static let colorThemeKey = "fumi.colorTheme"
    static let appearanceModeKey = "fumi.appearanceMode"

    static var colorTheme: AppTheme {
        get { UserDefaults.standard.string(forKey: colorThemeKey).flatMap(AppTheme.init(rawValue:)) ?? .white }
        set { UserDefaults.standard.set(newValue.rawValue, forKey: colorThemeKey) }
    }

    static var appearanceMode: AppearanceMode {
        get { UserDefaults.standard.string(forKey: appearanceModeKey).flatMap(AppearanceMode.init(rawValue:)) ?? .light }
        set { UserDefaults.standard.set(newValue.rawValue, forKey: appearanceModeKey) }
    }

    /// The 拾页 design's blue.
    static let defaultAccentHex = "#1D6FC2"

    static var accentColorHex: String {
        get { UserDefaults.standard.string(forKey: accentColorKey) ?? defaultAccentHex }
        set { UserDefaults.standard.set(newValue, forKey: accentColorKey) }
    }

    static var workspacePath: String {
        get { UserDefaults.standard.string(forKey: workspacePathKey) ?? "" }
        set { UserDefaults.standard.set(newValue, forKey: workspacePathKey) }
    }

    /// Folders opened before, most recent first, for the welcome page's
    /// Recent list. Before the list existed only the last folder was kept,
    /// so that one seeds it.
    static var recentWorkspaces: [String] {
        get {
            if let list = UserDefaults.standard.stringArray(forKey: recentWorkspacesKey) { return list }
            return workspacePath.isEmpty ? [] : [workspacePath]
        }
        set { UserDefaults.standard.set(newValue, forKey: recentWorkspacesKey) }
    }

    static func noteRecentWorkspace(_ path: String) {
        recentWorkspaces = Array(([path] + recentWorkspaces.filter { $0 != path }).prefix(5))
    }

    // MARK: - Appearance

    static var editorFont: String {
        get { UserDefaults.standard.string(forKey: editorFontKey) ?? "System" }
        set { UserDefaults.standard.set(newValue, forKey: editorFontKey) }
    }

    /// Editor measure: "narrow", "medium", "wide".
    static var editorWidth: String {
        get { UserDefaults.standard.string(forKey: editorWidthKey) ?? "medium" }
        set { UserDefaults.standard.set(newValue, forKey: editorWidthKey) }
    }

    // MARK: - Editor behavior

    /// Off by default, as NSTextView ships: Markdown syntax and mixed
    /// Chinese / English text draw a lot of false underlines.
    static let defaultSpellCheck = false
    static let defaultSmartPunctuation = false

    static var spellCheck: Bool {
        get { UserDefaults.standard.object(forKey: spellCheckKey) as? Bool ?? defaultSpellCheck }
        set { UserDefaults.standard.set(newValue, forKey: spellCheckKey) }
    }

    static var autoPair: Bool {
        get { UserDefaults.standard.object(forKey: autoPairKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: autoPairKey) }
    }

    static var smartPunctuation: Bool {
        get { UserDefaults.standard.object(forKey: smartPunctuationKey) as? Bool ?? defaultSmartPunctuation }
        set { UserDefaults.standard.set(newValue, forKey: smartPunctuationKey) }
    }

    /// `/` at the start of a line opens the menu of blocks to insert.
    static let defaultSlashCommands = true

    static var slashCommands: Bool {
        get { UserDefaults.standard.object(forKey: slashCommandsKey) as? Bool ?? defaultSlashCommands }
        set { UserDefaults.standard.set(newValue, forKey: slashCommandsKey) }
    }

    /// When to reveal raw Markdown markers: "always", "focus", "hidden".
    static var markdownSyntax: String {
        get { UserDefaults.standard.string(forKey: markdownSyntaxKey) ?? "focus" }
        set { UserDefaults.standard.set(newValue, forKey: markdownSyntaxKey) }
    }

    static var tabSize: Int {
        get { UserDefaults.standard.object(forKey: tabSizeKey) as? Int ?? 4 }
        set { UserDefaults.standard.set(newValue, forKey: tabSizeKey) }
    }

    static var wordWrap: Bool {
        get { UserDefaults.standard.object(forKey: wordWrapKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: wordWrapKey) }
    }

    static var typewriter: Bool {
        get { UserDefaults.standard.object(forKey: typewriterKey) as? Bool ?? false }
        set { UserDefaults.standard.set(newValue, forKey: typewriterKey) }
    }

    // MARK: - Export

    /// Default export format: "markdown", "pdf", "html".
    static var exportFormat: String {
        get { UserDefaults.standard.string(forKey: exportFormatKey) ?? "markdown" }
        set { UserDefaults.standard.set(newValue, forKey: exportFormatKey) }
    }

    static var includeFrontmatter: Bool {
        get { UserDefaults.standard.object(forKey: includeFrontmatterKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: includeFrontmatterKey) }
    }

    static var preserveTags: Bool {
        get { UserDefaults.standard.object(forKey: preserveTagsKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: preserveTagsKey) }
    }

    /// How images are exported: "copy", "link".
    static var imageHandling: String {
        get { UserDefaults.standard.string(forKey: imageHandlingKey) ?? "copy" }
        set { UserDefaults.standard.set(newValue, forKey: imageHandlingKey) }
    }
}

/// Line-spacing presets mapped to the editor line-height multiplier.
enum LineSpacingPreset: String, CaseIterable {
    case compact, cozy, relaxed

    var multiplier: Double {
        switch self {
        case .compact: return 1.4
        case .cozy: return 1.7
        case .relaxed: return 2.0
        }
    }

    var label: String { rawValue.capitalized }

    /// Resolve the preset closest to a stored multiplier.
    static func from(multiplier: Double) -> LineSpacingPreset {
        allCases.min(by: { abs($0.multiplier - multiplier) < abs($1.multiplier - multiplier) }) ?? .cozy
    }
}

/// Sort modes for the note list.
enum SortMode: String, CaseIterable {
    case updatedDesc = "updated-desc"
    case createdDesc = "created-desc"
    case titleAsc = "title-asc"

    var label: String {
        switch self {
        case .updatedDesc: return "Date Modified"
        case .createdDesc: return "Date Created"
        case .titleAsc: return "Title"
        }
    }
}

/// Theme preferences.
enum ThemePreference: String, CaseIterable {
    case light = "light"
    case dark = "dark"
    case system = "system"

    var label: String {
        switch self {
        case .light: return "Light"
        case .dark: return "Dark"
        case .system: return "System"
        }
    }
}
