import AppKit
import SwiftUI

struct EditorView: View {
    let note: FileNote
    @Environment(FileNoteStore.self) var store
    @Environment(AppModel.self) var appModel
    @State private var displayedText: String
    /// The content the editor last loaded or saved — its view of what is on disk.
    /// Used to tell an unsaved local edit apart from an external change.
    @State private var lastSyncedContent: String
    @State private var liveWordCount: Int
    @State private var liveCharCount: Int
    @State private var saveTask: Task<Void, Never>? = nil
    /// The text as typed, kept without redrawing anything; `displayedText`
    /// (outline, counts) catches up after a pause in typing.
    @State private var live = LiveText()
    /// The text view, to merge a change from outside into it in place.
    @State private var editorHandle = EditorHandle()
    @State private var catchUpTask: Task<Void, Never>? = nil
    /// Bumped when `displayedText` must replace the editor's text.
    @State private var textRevision = 0

    @AppStorage("notely.editorFont") private var editorFontName: String = "System"
    @AppStorage("notely.editorWidth") private var editorWidth: String = "medium"
    @AppStorage("notely.editorLineHeight") private var editorLineHeight: Double = 1.7
    @AppStorage(AppSettings.fontSizeKey) private var editorFontSize = AppSettings.defaultEditorFontSize
    @AppStorage(AppSettings.spellCheckKey) private var spellCheck = AppSettings.defaultSpellCheck
    @AppStorage(AppSettings.smartPunctuationKey) private var smartPunctuation = AppSettings.defaultSmartPunctuation

    private var fontSize: CGFloat { CGFloat(editorFontSize) }
    private var lineHeight: CGFloat { CGFloat(editorLineHeight) }
    private var fontName: String { editorFontName }

    /// Outline derived from the live text. Uses the exact same heading detection
    /// as `WysiwygTextView.headingLineRanges`, so a row's `index` always maps to
    /// the correct heading when the rail posts `.scrollToHeading`.
    private var outlineHeadings: [OutlineHeading] {
        let ns = displayedText as NSString
        let ranges = WysiwygTextView.headingLineRanges(in: ns)
        return ranges.enumerated().map { i, range in
            let line = ns.substring(with: range).trimmingCharacters(in: .whitespacesAndNewlines)
            let level = line.prefix(while: { $0 == "#" }).count
            let text = line.dropFirst(level).trimmingCharacters(in: .whitespaces)
            let bodyStart = NSMaxRange(range)
            let bodyEnd = i + 1 < ranges.count ? ranges[i + 1].location : ns.length
            let body = ns.substring(with: NSRange(location: bodyStart, length: bodyEnd - bodyStart))
            return OutlineHeading(index: i, level: level, text: String(text),
                                  summary: OutlineSectionPreview.text(of: body))
        }
    }

    /// A blank page not yet on disk (design board ㉝ E): shows a placeholder,
    /// takes focus, and becomes a note in the library at its first save.
    var isDraft = false

    init(note: FileNote, isDraft: Bool = false) {
        self.note = note
        self.isDraft = isDraft
        let initialText = note.content
        _displayedText = State(initialValue: initialText)
        _lastSyncedContent = State(initialValue: initialText)
        _liveWordCount = State(initialValue: Self.wordCount(in: initialText))
        _liveCharCount = State(initialValue: initialText.count)
    }

    var body: some View {
        editor
            .background(Color.pageContentBg)
            .modifier(isDraft
                      ? NoteToolbar(emptyTitle: "New Note")
                      : NoteToolbar(note: note, wordCount: liveWordCount, charCount: liveCharCount,
                                    onExportPDF: exportPDF))
            .onAppear {
                loadNote()
            }
            .onChange(of: note.id) { _, _ in
                loadNote()
            }
            // The open file changed on disk (external editor → store reload). Pull it
            // in live, merged with anything typed here that isn't saved yet.
            .onChange(of: note.content) { _, newContent in
                reconcileExternalChange(newContent)
            }
    }

    private var editor: some View {
        VStack(spacing: 0) {
            ZStack(alignment: .trailing) {
                WysiwygEditor(
                    textRevision: textRevision,
                    initialText: displayedText,
                    fontSize: fontSize,
                    lineHeight: lineHeight,
                    fontName: fontName,
                    editorWidth: editorWidth,
                    spellCheck: spellCheck,
                    smartPunctuation: smartPunctuation,
                    draftPlaceholder: isDraft
                        ? .init(title: "Title", hint: "Start typing — the note is saved once you write something.")
                        : nil,
                    focusesOnAppear: isDraft,
                    placeKey: isDraft ? nil : note.id,
                    handle: editorHandle,
                    onTextChange: handleTextChange,
                    onOpenNote: openLinkedNote,
                    onCreateNote: createLinkedNote
                )
                .frame(maxWidth: .infinity)
                .background(Color.pageContentBg)

                // Navigation lives at the right edge: a thin tick rail whose
                // ticks swell under the pointer, with a preview card for the
                // nearest heading. It never covers the centered text.
                OutlineRail(headings: outlineHeadings)
            }
            // Text frosts and fades as it scrolls out of the page's top.
            .overlay(alignment: .top) { PageEdgeBlur() }
        }
        .background(Color.pageContentBg)
    }

    private func loadNote() {
        saveTask?.cancel()
        catchUpTask?.cancel()
        ImageLoader.shared.setBaseURL(note.url.deletingLastPathComponent())
        live.text = note.content
        displayedText = note.content
        textRevision += 1
        lastSyncedContent = note.content
        updateCounts(from: note.content)
        publishPreview(note.content)
    }

    /// Reconcile an on-disk change to the currently open note with the editor.
    private func reconcileExternalChange(_ newContent: String) {
        switch ExternalSync.resolve(disk: newContent, displayed: live.text, lastSynced: lastSyncedContent) {
        case .ignore:
            lastSyncedContent = newContent
        case .adopt:
            // In place when the text view takes it: the caret stays put, and
            // ⌘Z takes the change back.
            let previous = lastSyncedContent
            lastSyncedContent = newContent
            if editorHandle.replaceMarkdown(with: newContent) { return }
            lastSyncedContent = previous
            catchUpTask?.cancel()
            live.text = newContent
            displayedText = newContent
            textRevision += 1
            lastSyncedContent = newContent
            updateCounts(from: newContent)
            publishPreview(newContent)
        case .merge:
            // Typed here and changed outside: save now, which merges the two
            // (FileNoteStore.saveContent) and puts the merge on screen.
            saveTask?.cancel()
            save(live.text, base: lastSyncedContent, to: note.id, tab: appModel.activeTabId)
        }
    }

    /// On every keystroke: only what can't wait. The views that read the
    /// whole text (outline, word count) update once typing
    /// pauses, so a keystroke never redraws them.
    private func handleTextChange(_ newText: String) {
        live.text = newText
        appModel.typedNote = noteText(newText)
        scheduleSave(newText)
        catchUpTask?.cancel()
        catchUpTask = Task {
            try? await Task.sleep(for: .milliseconds(300))
            guard !Task.isCancelled else { return }
            displayedText = newText
            updateCounts(from: newText)
            publishPreview(newText)
        }
    }

    /// A link clicked in the text (design board 58): the note opens here,
    /// or in a tab behind this one, scrolled to the heading it names.
    private func openLinkedNote(_ id: String, heading: String?, inNewTab: Bool) {
        appModel.followLink(to: id, inNewTab: inNewTab)
        guard let heading, !inNewTab,
              let content = store.notes.first(where: { $0.id == id })?.content else { return }
        let text = content as NSString
        let headings = WysiwygTextView.headingLineRanges(in: text).map {
            text.substring(with: $0).trimmingCharacters(in: CharacterSet(charactersIn: "# ").union(.newlines))
        }
        guard let index = headings.firstIndex(where: { $0.caseInsensitiveCompare(heading) == .orderedSame }) else { return }
        // Once the note's editor is up.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
            NotificationCenter.default.post(name: .scrollToHeading, object: nil, userInfo: ["index": index])
        }
    }

    /// A link to a note that doesn't exist, created beside this one.
    private func createLinkedNote(_ name: String) -> String? {
        store.createNote(title: name, in: note.url.deletingLastPathComponent())?.id
    }

    private func noteText(_ markdown: String) -> AppModel.NoteText {
        AppModel.NoteText(noteId: note.id, path: note.url.path, markdown: markdown)
    }

    /// Feeds the WeChat preview beside the page (and Copy for WeChat).
    private func publishPreview(_ markdown: String) {
        let text = noteText(markdown)
        appModel.typedNote = text
        if appModel.previewNote != text { appModel.previewNote = text }
    }

    private func updateCounts(from text: String) {
        liveWordCount = Self.wordCount(in: text)
        liveCharCount = text.count
    }

    /// Exports the note as a PDF. The sheet holds the text as it is on screen,
    /// so a note that is still being typed is exported as it stands.
    private func exportPDF() {
        let content = live.text.isEmpty ? note.content : live.text
        Task { await PDFExporter.export(note: note, markdown: content) }
    }

    private static func wordCount(in text: String) -> Int {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? 0 : trimmed.components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty }.count
    }

    private func scheduleSave(_ content: String) {
        // Bind the destination to the note this text came from, at schedule time,
        // so a debounced write can never be redirected to a note switched-to
        // during the 500ms window.
        let targetId = note.id
        // Only the active tab has an editor, so this is the tab being typed in.
        let tabId = appModel.activeTabId
        // What the file held when this text was typed, for the save to tell
        // a change from outside meanwhile.
        let base = lastSyncedContent
        saveTask?.cancel()
        saveTask = Task {
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled else { return }
            save(content, base: base, to: targetId, tab: tabId)
        }
    }

    private func save(_ content: String, base: String, to targetId: String, tab tabId: UUID) {
        let result = store.saveContent(content, to: targetId, basedOn: base)
        // A draft's first save puts it in the library: its tab now shows
        // that note, and the sidebar selects it.
        if result.adoptedDraft {
            appModel.adoptDraft(targetId, inTab: tabId)
        }
        guard note.id == targetId, let written = result.written else { return }
        if written == content {
            lastSyncedContent = written
        } else if live.text == content {
            // Merged with a change from outside: show the merge. When the
            // text view can't take it now (an input method composing), the
            // file keeps the merge and the next save merges again.
            let previous = lastSyncedContent
            lastSyncedContent = written
            if !editorHandle.replaceMarkdown(with: written) { lastSyncedContent = previous }
        }
        if let copy = result.conflictCopy { showConflictCopy(copy) }
    }

    /// Both sides changed the same paragraph: the note keeps what was typed
    /// here; say where the outside version went.
    private func showConflictCopy(_ copy: FileNote) {
        let alert = NSAlert()
        alert.messageText = "This note was also changed outside Fumi"
        alert.informativeText = "Some paragraphs were changed both here and outside. The note keeps your version; "
            + "the outside version was saved as “\(copy.filename)” in the same folder."
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: "Open Copy")
        let respond = { (response: NSApplication.ModalResponse) in
            if response == .alertSecondButtonReturn { appModel.openNote(copy.id, inNewTab: true) }
        }
        if let window = NSApp.keyWindow {
            alert.beginSheetModal(for: window, completionHandler: respond)
        } else {
            respond(alert.runModal())
        }
    }

}

/// The latest typed text, held by reference so updating it redraws nothing.
final class LiveText {
    var text = ""
}

/// The window toolbar for the editor: the note's title and location in the
/// title area, note actions as system toolbar buttons (Liquid Glass on
/// macOS 26+). With `note == nil` only New Note is offered.
struct NoteToolbar: ViewModifier {
    var note: FileNote? = nil
    var wordCount = 0
    var charCount = 0
    /// The title shown when there's no note.
    var emptyTitle = "Notes"
    /// Exports the note as a PDF. Set by the editor, so the sheet holds the
    /// note as it is on screen — even while it is still being typed.
    var onExportPDF: (() -> Void)? = nil
    @Environment(AppModel.self) var appModel
    @Environment(FileNoteStore.self) var store
    @State private var showsInfo = false
    @AppStorage(AppSettings.wechatExtensionKey) private var wechatEnabled = false
    @AppStorage(AppSettings.xhsExtensionKey) private var xhsEnabled = false

    /// The preview's name for the extensions that are on.
    private var previewName: String {
        switch (wechatEnabled, xhsEnabled) {
        case (true, false): "WeChat Preview"
        case (false, true): "Xiaohongshu Preview"
        default: "Publish Preview"
        }
    }

    /// The file name: notes are files, and clipped articles often have no
    /// heading, so their first line makes a poor title.
    private var title: String {
        note?.filename ?? emptyTitle
    }

    /// "Clippings / AI · Edited Oct 9"
    private var subtitle: String {
        guard let note else { return "" }
        let folder = note.relativePath.split(separator: "/").joined(separator: " / ")
        let edited = Self.editedLabel(for: note.updatedAt)
        return folder.isEmpty ? edited : "\(folder) · \(edited)"
    }

    func body(content: Content) -> some View {
        content
            .navigationTitle(title)
            .navigationSubtitle(subtitle)
            .modifier(RemovesSystemTitle())
            .toolbar {
                NoteTitleHeader(title: title, subtitle: subtitle)

                // Flat 拾页 glyphs, each its own item with the shared glass
                // capsule turned off — the same treatment as the sidebar
                // toggle, so the page's corner holds plain icons, not a pill.
                // The same icons with one page or several.
                if note != nil, wechatEnabled || xhsEnabled {
                    FlatToolbarItem {
                        Button {
                            appModel.togglePublishPreview()
                        } label: {
                            Label {
                                Text(previewName)
                            } icon: {
                                // In the accent while open, like a switch that's on.
                                if appModel.isPublishPreviewOpen {
                                    Image(nsImage: ShiyeGlyph.phone.templateImage()).foregroundStyle(Color.accent)
                                } else {
                                    Image(nsImage: ShiyeGlyph.phone.templateImage())
                                }
                            }
                        }
                        .help(appModel.isPublishPreviewOpen ? "Hide \(previewName) (⌃⌘P)" : "\(previewName) (⌃⌘P)")
                    }
                }

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

                if let note {
                    FlatToolbarItem {
                        Button {
                            showsInfo.toggle()
                        } label: {
                            Label {
                                Text("Info")
                            } icon: {
                                Image(nsImage: ShiyeGlyph.info.templateImage())
                            }
                        }
                        .help("Note Info")
                        .popover(isPresented: $showsInfo, arrowEdge: .bottom) {
                            DocumentInfoPopover(note: note, wordCount: wordCount, charCount: charCount)
                        }
                    }

                    FlatToolbarItem {
                        Menu {
                            Button(note.pinned ? "Unpin" : "Pin") { store.togglePin(note.id) }
                            Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([note.url]) }
                            Button("Export as PDF…") { onExportPDF?() }
                            if wechatEnabled || xhsEnabled {
                                Button(appModel.isPublishPreviewOpen ? "Hide \(previewName)" : previewName) {
                                    appModel.togglePublishPreview()
                                }
                            }
                            if wechatEnabled {
                                Button("Copy for WeChat") { Task { await WeChatCopy.copyOrShowWhy(appModel) } }
                            }
                            if xhsEnabled {
                                Button("Add Xiaohongshu Cards to Photos") { Task { await XhsCards.addOrShowWhy(appModel) } }
                            }
                            Divider()
                            Button("Delete", role: .destructive) {
                                appModel.removeTabs(showing: .note(note.id))
                                store.deleteNote(note.id)
                            }
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
    }

    /// "Edited 14:20" today, "Edited yesterday 09:12", else "Edited Oct 8".
    static func editedLabel(for date: Date, now: Date = Date(), calendar: Calendar = .current) -> String {
        let time = date.formatted(date: .omitted, time: .shortened)
        if calendar.isDate(date, inSameDayAs: now) { return "Edited \(time)" }
        if calendar.isDateInYesterday(date) { return "Edited yesterday \(time)" }
        let sameYear = calendar.component(.year, from: date) == calendar.component(.year, from: now)
        let day = sameYear
            ? date.formatted(.dateTime.month(.abbreviated).day())
            : date.formatted(.dateTime.month(.abbreviated).day().year())
        return "Edited \(day)"
    }
}

/// A toolbar item drawn bare: on macOS 26 the shared Liquid Glass
/// background is hidden (see SidebarToggleItem) — except for the icons in
/// the Glass theme, which sit in white glass capsules (board ㊳).
struct FlatToolbarItem<Content: View>: ToolbarContent {
    /// `.automatic` lines up at the trailing edge, after the title's spacer;
    /// `.primaryAction` would pull the icons to the leading side on macOS.
    var placement: ToolbarItemPlacement = .automatic
    /// Whether the Glass theme may put this item in a glass capsule. The
    /// title stays bare text in every theme.
    var glassInGlassTheme = true
    @ViewBuilder let content: () -> Content

    var body: some ToolbarContent {
        if #available(macOS 26, *) {
            ToolbarItem(placement: placement) { content().modifier(DimsWithWindow()) }
                .sharedBackgroundVisibility(glassInGlassTheme && ThemeManager.shared.isGlass ? .visible : .hidden)
        } else {
            ToolbarItem(placement: placement) { content().modifier(DimsWithWindow()) }
        }
    }
}

/// The note's file name over the page (design board ㉙ B + H3) is
/// `NoteTitleHeader`, in TabStrip.swift: set in the editor's own body face
/// at medium weight in secondary gray, so it reads as the page's running
/// head; with more than one tab open the tab strip takes its place.

/// Text set in a body face — the editor's, New York with 宋体 by default —
/// with Han runs re-set in the paired CJK face exactly as the editor does,
/// so serif UI text (the header title, the empty pages) reads like the page.
enum BodyFace {
    static func text(_ string: String, fontName: String, size: CGFloat,
                     weight: NSFont.Weight = .regular) -> AttributedString {
        let font = WysiwygEngine.resolveFont(name: fontName, size: size, weight: weight)
        let storage = NSTextStorage(string: string, attributes: [.font: font])
        CompositeFont(baseFont: font, familyName: fontName).apply(to: storage)

        var result = AttributedString()
        storage.enumerateAttribute(.font, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            var run = AttributedString(storage.attributedSubstring(from: range).string)
            run.font = Font((value as? NSFont ?? font) as CTFont)
            result += run
        }
        return result
    }
}

/// Hides the system's toolbar title where NoteTitleHeader replaces it.
struct RemovesSystemTitle: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 26, *) {
            content.toolbar(removing: .title)
        } else {
            content
        }
    }
}

/// Lightweight, glanceable document info shown in a popover anchored to the
/// toolbar info button — stats and tags only (navigation lives in the rail).
struct DocumentInfoPopover: View {
    let note: FileNote
    let wordCount: Int
    let charCount: Int

    private var readingTime: String {
        let minutes = max(1, wordCount / 200)
        return "\(minutes) min"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(spacing: 8) {
                InspectorStatRow(label: "Words", value: "\(wordCount)")
                InspectorStatRow(label: "Characters", value: "\(charCount)")
                InspectorStatRow(label: "Reading time", value: readingTime)
                InspectorStatRow(label: "Created", value: note.createdAt.formatted(.dateTime.month().day().year()))
                InspectorStatRow(label: "Updated", value: note.updatedAt.formatted(.relative(presentation: .named)))
            }
            .padding(.horizontal, 16)
            .padding(.top, 16)
            .padding(.bottom, 14)

            if !note.tags.isEmpty {
                InspectorDivider().padding(.horizontal, 0)
                VStack(alignment: .leading, spacing: 8) {
                    Text("Tags")
                        .font(.notely(.caption, weight: .semibold))
                        .foregroundColor(.tertiaryText)
                        .tracking(0.4)
                        .textCase(.uppercase)
                    FlowLayout(spacing: 6) {
                        ForEach(note.tags, id: \.self) { tag in
                            Text("#\(tag)")
                                .font(.notely(.caption, weight: .medium))
                                .foregroundColor(.accent)
                                .padding(.horizontal, 8)
                                .padding(.vertical, 3)
                                .background(Capsule().fill(Color.accent.opacity(0.08)))
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
            }
        }
        .frame(width: 240)
    }
}

struct InspectorStatRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .font(.notely(.body))
                .foregroundColor(.secondaryText)
            Spacer()
            Text(value)
                .font(.notely(.body, weight: .medium))
                .foregroundColor(.primaryText)
                .lineLimit(1)
        }
    }
}

struct InspectorDivider: View {
    var body: some View {
        Rectangle()
            .fill(Color.primaryText.opacity(0.06))
            .frame(height: 1)
            .padding(.horizontal, 20)
    }
}

/// Simple flow layout for wrapping tag pills.
struct FlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let maxWidth = proposal.width ?? .infinity
        var totalHeight: CGFloat = 0
        var totalWidth: CGFloat = 0
        var lineWidth: CGFloat = 0
        var lineHeight: CGFloat = 0

        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if lineWidth + size.width > maxWidth && lineWidth > 0 {
                totalWidth = max(totalWidth, lineWidth)
                totalHeight += lineHeight + spacing
                lineWidth = size.width + spacing
                lineHeight = size.height
            } else {
                lineWidth += size.width + spacing
                lineHeight = max(lineHeight, size.height)
            }
        }
        totalWidth = max(totalWidth, lineWidth)
        totalHeight += lineHeight
        return CGSize(width: totalWidth, height: totalHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x: CGFloat = bounds.minX
        var y: CGFloat = bounds.minY
        var lineHeight: CGFloat = 0

        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX && x > bounds.minX {
                x = bounds.minX
                y += lineHeight + spacing
                lineHeight = 0
            }
            subview.place(at: CGPoint(x: x, y: y), proposal: .init(size))
            x += size.width + spacing
            lineHeight = max(lineHeight, size.height)
        }
    }
}
