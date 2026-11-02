import AppKit
import AVKit
import SwiftUI
import UniformTypeIdentifiers

/// Simple file-based diagnostics.
enum Diag {
    private static let enabled = false

    private static let path: String = {
        let home = NSHomeDirectory()
        return "\(home)/fumi_debug.log"
    }()

    /// The message is built only when logging is on (it often copies the text).
    static func log(_ message: @autoclosure () -> String) {
        guard enabled else { return }
        let message = message()
        let timestamp = DateFormatter.localizedString(from: Date(), dateStyle: .none, timeStyle: .medium)
        let line = "[\(timestamp)] \(message)\n"
        // Also print to stderr for immediate feedback
        FileHandle.standardError.write(line.data(using: .utf8) ?? Data())
        if let data = line.data(using: .utf8) {
            if FileManager.default.fileExists(atPath: path) {
                if let handle = FileHandle(forWritingAtPath: path) {
                    handle.seekToEndOfFile()
                    handle.write(data)
                    handle.closeFile()
                }
            } else {
                FileManager.default.createFile(atPath: path, contents: data)
            }
        }
    }
}

extension Notification.Name {
    static let imageDidLoad = Notification.Name("fumi.imageDidLoad")
    static let animatedImageFrameDidChange = Notification.Name("fumi.animatedImageFrameDidChange")
    /// Posted by the outline rail; userInfo["index"] = ordinal of the heading to
    /// scroll to (0-based, in document order).
    static let scrollToHeading = Notification.Name("fumi.scrollToHeading")
}

/// A video playing inside a note. The wheel scrolls the page, not the
/// timeline: AVPlayerView scrubs on scroll, so a note couldn't be scrolled
/// past a playing video. Seeking stays on the scrub bar.
final class InlinePlayerView: AVPlayerView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        // Opting out of hit-testing for wheel events lets the text view
        // underneath take them and hand them to its scroll view.
        if NSApp.currentEvent?.type == .scrollWheel { return nil }
        return super.hitTest(point)
    }
}

/// Custom scroll view that ensures mouse clicks properly activate the text
/// view as first responder, even inside SwiftUI's NavigationSplitView.
final class EditorScrollView: NSScrollView {
    override var acceptsFirstResponder: Bool { false }

    /// Always the overlay scroller (QuietScroller), even when the system
    /// would switch to the legacy, always-shown kind.
    override var scrollerStyle: NSScroller.Style {
        get { .overlay }
        set { super.scrollerStyle = .overlay }
    }

    /// Puts a note back where it was left (caret, scroll); returns false
    /// while the view is still too narrow to lay the text out. Run in the
    /// first layout pass that has the real width — before anything is drawn,
    /// so a tab switched back to doesn't flash its top first.
    var pendingRestore: (() -> Bool)?

    override func layout() {
        super.layout()
        if let restore = pendingRestore, restore() { pendingRestore = nil }
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        return true
    }

    override func mouseDown(with event: NSEvent) {
        if let textView = documentView as? NSTextView, textView.window?.firstResponder != textView {
            textView.window?.makeFirstResponder(textView)
        }
        super.mouseDown(with: event)
    }
}

/// The editor's text view, held by reference for its parent view.
final class EditorHandle {
    weak var textView: WysiwygTextView?

    /// See `WysiwygTextView.replaceMarkdown(with:)`.
    func replaceMarkdown(with markdown: String) -> Bool {
        textView?.replaceMarkdown(with: markdown) ?? false
    }
}

/// SwiftUI wrapper for the WYSIWYG text editor.
///
/// IMPORTANT: This wrapper does NOT use a @Binding for the text content.
/// Instead, the parent passes the initial text and observes changes via a
/// closure. This breaks the feedback loop where every keystroke would
/// trigger a SwiftUI re-render that resets the NSTextView.
struct WysiwygEditor: NSViewRepresentable {
    /// Bumped by the parent when `initialText` must replace what is in the
    /// editor (another note opened, a change from disk). Between bumps the
    /// parent's text may lag behind typing and is never pushed.
    var textRevision = 0
    let initialText: String
    let fontSize: CGFloat
    let lineHeight: CGFloat
    let fontName: String
    let editorWidth: String
    var spellCheck = AppSettings.defaultSpellCheck
    var smartPunctuation = AppSettings.defaultSmartPunctuation
    /// A draft's faint "Title" and hint, drawn while it is still untouched.
    var draftPlaceholder: WysiwygTextView.DraftPlaceholder? = nil
    /// Take keyboard focus, caret at the end, once the editor is on screen.
    var focusesOnAppear = false
    /// The note shown, so its caret and scroll position are put back when
    /// it is opened again — switching tabs away and back (board ㊴). Nil
    /// for a draft.
    var placeKey: String? = nil
    /// Lets the parent reach the text view, to merge a change from outside
    /// into it without moving the caret.
    var handle: EditorHandle? = nil
    let onTextChange: (String) -> Void
    /// A clicked link to a note: its id, a heading in it, in a new tab.
    var onOpenNote: ((String, String?, Bool) -> Void)? = nil
    /// A link to a note that doesn't exist, which the user chose to create.
    var onCreateNote: ((String) -> String?)? = nil

    /// Where each note was left: caret, scroll offset, whether it had focus.
    struct Place {
        var selection: NSRange
        /// The top of the visible text, from the top of the note: unlike the
        /// clip view's origin it means the same once the note is laid out
        /// again at another height.
        var scrollY: CGFloat
        var hadFocus: Bool
    }
    @MainActor static var places: [String: Place] = [:]

    /// Settings › Editor. Set only on change: assigning restarts the checker.
    static func applyTypingOptions(_ textView: NSTextView, spellCheck: Bool, smartPunctuation: Bool) {
        if textView.isContinuousSpellCheckingEnabled != spellCheck {
            textView.isContinuousSpellCheckingEnabled = spellCheck
        }
        if textView.isAutomaticQuoteSubstitutionEnabled != smartPunctuation {
            textView.isAutomaticQuoteSubstitutionEnabled = smartPunctuation
            textView.isAutomaticDashSubstitutionEnabled = smartPunctuation
        }
    }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = EditorScrollView()
        scrollView.verticalScroller = QuietScroller(frame: .zero)
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        scrollView.backgroundColor = .clear
        scrollView.focusRingType = .none
        scrollView.borderType = .noBorder

        let textStorage = NSTextStorage()
        let layoutManager = CodeBlockLayoutManager()
        let textContainer = NSTextContainer(containerSize: NSSize(width: 1, height: CGFloat.greatestFiniteMagnitude))
        textContainer.widthTracksTextView = true
        textContainer.lineFragmentPadding = 0
        layoutManager.addTextContainer(textContainer)
        textStorage.addLayoutManager(layoutManager)

        let textView = WysiwygTextView(
            frame: NSRect(origin: .zero, size: NSSize(width: 1, height: scrollView.contentSize.height)),
            textContainer: textContainer
        )
        textView.fontSize = fontSize
        textView.lineHeight = lineHeight
        textView.fontName = fontName
        textView.editorWidth = editorWidth
        textView.draftPlaceholder = draftPlaceholder
        textView.focusesOnAppear = focusesOnAppear
        Self.applyTypingOptions(textView, spellCheck: spellCheck, smartPunctuation: smartPunctuation)
        textView.delegate = context.coordinator
        handle?.textView = textView
        textView.onOpenNote = onOpenNote
        textView.onCreateNote = onCreateNote
        textView.onTextChange = { newText in
            context.coordinator.lastKnownText = newText
            context.coordinator.onTextChange(newText)
        }
        textView.string = initialText
        // Build the engine at the correct font/line-height up front so the first
        // `updateNSView` does not have to rebuild it.
        textView.rebuildEngineAndRestyle()

        scrollView.documentView = textView
        context.coordinator.textView = textView
        context.coordinator.lastKnownText = initialText
        context.coordinator.textRevision = textRevision
        context.coordinator.placeKey = placeKey
        if let placeKey, let place = Self.places[placeKey] {
            scrollView.pendingRestore = { [weak textView, weak scrollView] in
                guard let textView, let scrollView else { return true }
                // The text view takes the clip view's width as the scroll view
                // is laid out; until then the text can't be measured.
                guard scrollView.contentSize.width > 50, textView.frame.width > 50 else { return false }
                // Style at the real width first: the note was first styled at
                // the minimum width, with images to match, and anything that
                // restyles it later (taking focus does) would grow them and
                // push the text down after the scroll was set.
                textView.restyle()
                let length = (textView.string as NSString).length
                let location = min(place.selection.location, length)
                textView.setSelectedRange(NSRange(location: location,
                                                  length: min(place.selection.length, length - location)))
                if place.hadFocus { textView.window?.makeFirstResponder(textView) }
                // The scroll last, so nothing moves it again.
                textView.fitFrameToContent()
                let maxY = max(0, textView.frame.height - scrollView.contentSize.height)
                textView.scroll(NSPoint(x: 0, y: min(place.scrollY, maxY)))
                scrollView.reflectScrolledClipView(scrollView.contentView)
                return true
            }
        }
        Diag.log("makeNSView: textView frame=\(textView.frame) isEditable=\(textView.isEditable)")
        return scrollView
    }

    static func dismantleNSView(_ nsView: NSScrollView, coordinator: Coordinator) {
        guard let key = coordinator.placeKey, let textView = nsView.documentView as? NSTextView else { return }
        places[key] = Place(selection: textView.selectedRange(),
                            scrollY: textView.visibleRect.minY,
                            hadFocus: textView.window?.firstResponder === textView)
    }

    func updateNSView(_ nsView: NSScrollView, context: Context) {
        guard let textView = nsView.documentView as? WysiwygTextView else { return }
        Self.applyTypingOptions(textView, spellCheck: spellCheck, smartPunctuation: smartPunctuation)
        if textView.draftPlaceholder != draftPlaceholder {
            textView.draftPlaceholder = draftPlaceholder
            textView.needsDisplay = true
        }

        // Re-bind the text-change callback to the CURRENT note's EditorView. The
        // Coordinator is created once (makeCoordinator), so without this a reused
        // editor would keep routing saves through the FIRST note's closure —
        // writing the displayed note's text into the wrong note's file.
        context.coordinator.onTextChange = onTextChange
        handle?.textView = textView
        textView.onOpenNote = onOpenNote
        textView.onCreateNote = onCreateNote

        // Only sync text when switching notes (external content change),
        // never during typing. We detect this by comparing initialText against
        // the coordinator's lastKnownText. If they match, the change came from
        // user typing (which already updated the text view). If they differ,
        // it's an external change (note switch) that needs to be pushed in.
        // The parent's text may lag behind typing (it catches up after a
        // pause), so it is pushed only when the parent says it replaces the
        // editor's: another note, or a change from disk.
        let replaces = textRevision != context.coordinator.textRevision
        context.coordinator.textRevision = textRevision
        if replaces, initialText != context.coordinator.lastKnownText {
            context.coordinator.lastKnownText = initialText
            let selectedRange = textView.selectedRange()
            textView.string = initialText
            textView.restyle()
            let clampedLocation = min(selectedRange.location, (initialText as NSString).length)
            textView.setSelectedRange(NSRange(location: clampedLocation, length: 0))
            Diag.log("updateNSView: text synced for note switch, len=\(initialText.count)")
        }

        // The scroll view fills the full width of the pane, so its vertical
        // scroller sits flush against the right edge. The readable text column is
        // centered and capped via textContainerInset rather than by shrinking the
        // scroll view (which is what pushed the scroller inward before).
        textView.editorWidth = editorWidth
        let available = max(nsView.contentSize.width, 1)
        let sideInset = textView.sideInset(forAvailableWidth: available)
        let widthChanged = abs(textView.frame.width - available) > 0.5
        let insetChanged = abs(textView.textContainerInset.width - sideInset) > 0.5
        if widthChanged || insetChanged {
            textView.setFrameSize(NSSize(width: available, height: max(textView.frame.height, nsView.contentSize.height)))
            textView.textContainerInset = NSSize(width: sideInset, height: textView.textContainerInset.height)
            textView.textContainer?.containerSize = NSSize(width: max(1, available - sideInset * 2), height: CGFloat.greatestFiniteMagnitude)
        }

        textView.fontSize = fontSize
        textView.lineHeight = lineHeight
        textView.fontName = fontName
        // Keep the caret colored by the live theme accent (it can change at
        // runtime via Settings without the text view being recreated).
        textView.insertionPointColor = ThemeManager.shared.accentNSColor
        textView.selectedTextAttributes = [.backgroundColor: ThemeManager.shared.accentNSColor.withAlphaComponent(0.18)]
        textView.rebuildEngineAndRestyleIfNeeded()
        textView.fitFrameToContent()
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onTextChange: onTextChange)
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var onTextChange: (String) -> Void
        weak var textView: NSTextView?
        var lastKnownText: String = ""
        /// The parent's `textRevision` last seen.
        var textRevision = 0
        var placeKey: String?

        init(onTextChange: @escaping (String) -> Void) {
            self.onTextChange = onTextChange
        }
    }
}

/// The WYSIWYG-capable NSTextView subclass.
final class WysiwygTextView: NSTextView {
    var fontSize: CGFloat = 16
    var lineHeight: CGFloat = 1.75
    var fontName: String = "System"
    var onTextChange: ((String) -> Void)?
    var draftPlaceholder: DraftPlaceholder?
    var focusesOnAppear = false
    private var engine: WysiwygEngine?

    /// What an untouched draft shows (design board ㉝ E): `title` faint in the
    /// title heading's face right where the typed title will go, and `hint`
    /// on the line below. Drawn only while the text is still the draft's
    /// empty heading, so it disappears with the first keystroke.
    struct DraftPlaceholder: Equatable {
        var title: String
        var hint: String
    }
    private var isRestyling = false

    /// Font/line-height the cached `engine` was built for. The engine compiles
    /// ~9 regular expressions, so we rebuild it only when these actually change
    /// instead of on every SwiftUI `updateNSView` pass.
    private var engineFontSize: CGFloat = .nan
    private var engineLineHeight: CGFloat = .nan
    private var engineFontName: String = ""

    /// Set when a coalesced restyle is already queued for this runloop turn, so
    /// a burst of async triggers (e.g. many remote images finishing) collapses
    /// into a single full restyle instead of one per event.
    private var restyleScheduled = false

    /// Test instrumentation: number of full restyles and engine rebuilds.
    private(set) var restyleCount = 0
    private(set) var engineBuildCount = 0

    // MARK: - Layout

    /// The user's "Editor Width" preference ("narrow" / "medium" / "wide"),
    /// driving `maxColumnWidth` so the readable column width is configurable and
    /// stays stable across sidebar toggles.
    var editorWidth: String = "medium"

    /// Maximum readable text column width in points, resolved from the
    /// `editorWidth` preference. Exposed as a static resolver so the split-view
    /// detail column can enforce a matching minimum width (see `AppShellView`).
    static func columnWidth(for setting: String) -> CGFloat {
        switch setting {
        case "narrow": return 600
        case "wide": return 860
        default: return 680
        }
    }

    var maxColumnWidth: CGFloat { Self.columnWidth(for: editorWidth) }

    /// Minimum horizontal inset on each side of the text column. Lowered from
    /// 90 → 40 so the capped column engages at a smaller pane width
    /// (cap + 2×inset), keeping the text column stable across sidebar toggles
    /// on more window sizes.
    static let minSideInset: CGFloat = 40

    /// Horizontal text inset that centers a `maxColumnWidth`-capped column in a
    /// text view of the given available width. The scroll view itself spans the
    /// full width (so its scroller hugs the right edge); this inset reproduces
    /// the centered column purely inside the text view.
    func sideInset(forAvailableWidth width: CGFloat) -> CGFloat {
        max(Self.minSideInset, (width - maxColumnWidth) / 2)
    }

    override init(frame frameRect: NSRect, textContainer container: NSTextContainer?) {
        super.init(frame: frameRect, textContainer: container)
        setupEditor()
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupEditor() {
        isEditable = true
        isSelectable = true
        isRichText = true
        allowsUndo = true
        usesAdaptiveColorMappingForDarkAppearance = true
        font = WysiwygEngine.resolveFont(name: fontName, size: fontSize)
        textColor = MarkdownPalette.primary
        backgroundColor = .clear
        insertionPointColor = ThemeManager.shared.accentNSColor
        autoresizingMask = [.width]
        // Top/bottom breathing room for the text column; the scroll view adds
        // the toolbar's height on top when it extends under it.
        textContainerInset = NSSize(width: 0, height: 30)
        textContainer?.widthTracksTextView = true
        textContainer?.containerSize = NSSize(width: max(bounds.width, 1), height: CGFloat.greatestFiniteMagnitude)
        textContainer?.lineFragmentPadding = 0
        isHorizontallyResizable = false
        isVerticallyResizable = true
        minSize = NSSize(width: 0, height: 0)
        maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)

        rebuildEngine()
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleImageDidLoad),
            name: .imageDidLoad,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleAnimatedImageFrameDidChange(_:)),
            name: .animatedImageFrameDidChange,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleScrollToHeading(_:)),
            name: .scrollToHeading,
            object: nil
        )
        // A linked note renamed or retitled: its links show the new title.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleLinkIndexChange),
            name: WikiLinkIndex.didChange,
            object: nil
        )
    }

    // MARK: - Heading navigation

    /// Ensures the text view's frame is tall enough to contain all laid-out
    /// content. Without this, `NSClipView.scroll(to:)` is clamped to the
    /// document view's bounds — if the frame only matches the visible height,
    /// no scrolling is possible and every heading jump lands at the top.
    func fitFrameToContent() {
        guard let layoutManager, let textContainer else { return }
        layoutManager.ensureLayout(for: textContainer)
        let usedRect = layoutManager.usedRect(for: textContainer)
        let requiredHeight = usedRect.height + textContainerInset.height * 2
        let minHeight = enclosingScrollView?.contentSize.height ?? bounds.height
        let targetHeight = max(requiredHeight, minHeight)
        if abs(frame.height - targetHeight) > 0.5 {
            setFrameSize(NSSize(width: frame.width, height: targetHeight))
        }
    }

    /// Last frame width we applied the column inset for. The `setFrameSize`
    /// override uses this to recompute the capping inset only when the width
    /// actually changes — not on every height tick from `fitFrameToContent`,
    /// which would otherwise loop.
    private var lastLayoutWidth: CGFloat = -1

    /// Recomputes the centered/capped side inset whenever the text view's width
    /// changes. `widthTracksTextView = true` auto-tracks the text container's
    /// width to the text view bounds, but it does NOT recompute the capping
    /// inset — so without this, collapsing/expanding the sidebar reflows the
    /// text against a stale inset (the column width changes even though only
    /// the pane width did). SwiftUI does not call `updateNSView` on a pure pane
    /// resize, so this override is the only path that fires on sidebar toggle.
    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        let width = newSize.width
        guard abs(width - lastLayoutWidth) > 0.5 else { return }
        lastLayoutWidth = width
        recomputeColumnInset()
    }

    private func recomputeColumnInset() {
        let width = max(bounds.width, 1)
        let inset = sideInset(forAvailableWidth: width)
        guard abs(textContainerInset.width - inset) > 0.5 else { return }
        textContainerInset = NSSize(width: inset, height: textContainerInset.height)
        // The container auto-tracks to bounds.width - 2*inset; relayout and grow
        // the frame height to fit the (possibly re-wrapped) content.
        fitFrameToContent()
    }

    /// Character ranges of every Markdown heading line (`#`…`######`), in
    /// document order, skipping fenced code blocks. The N-th element is the
    /// scroll target for outline row N. Static + pure so it can be unit-tested.
    static func headingLineRanges(in string: NSString) -> [NSRange] {
        var ranges: [NSRange] = []
        var inCodeBlock = false
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length), options: [.byLines]) { sub, range, _, _ in
            let trimmed = (sub ?? "").trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                inCodeBlock.toggle()
                return
            }
            if inCodeBlock { return }
            if trimmed.range(of: #"^#{1,6}\s+\S"#, options: .regularExpression) != nil {
                ranges.append(range)
            }
        }
        return ranges
    }

    @objc private func handleScrollToHeading(_ notification: Notification) {
        // Only the key window's editor responds to outline navigation. The
        // `.scrollToHeading` notification is posted with `object: nil`, so
        // without this guard every open window's editor would scroll when an
        // outline row is clicked in any one of them. Clicking the rail makes
        // the clicked window key, so this filters to exactly that window.
        guard window?.isKeyWindow == true else { return }
        guard let index = notification.userInfo?["index"] as? Int,
              let storage = textStorage,
              let layoutManager = layoutManager,
              let textContainer = textContainer else { return }

        let ranges = Self.headingLineRanges(in: storage.string as NSString)
        guard index >= 0, index < ranges.count else { return }
        let target = ranges[index]

        // Grow the text view to fit all content, then force layout so the
        // heading's rect is valid before we measure it.
        fitFrameToContent()
        layoutManager.ensureLayout(for: textContainer)

        // Heading rect in the text view's coordinate space.
        let glyphRange = layoutManager.glyphRange(forCharacterRange: target, actualCharacterRange: nil)
        var rect = layoutManager.boundingRect(forGlyphRange: glyphRange, in: textContainer)
        rect.origin.x += textContainerOrigin.x
        rect.origin.y += textContainerOrigin.y

        // Bias the heading toward the TOP of the viewport: reveal a viewport-tall
        // region starting just above it. scrollToVisible top-aligns an oversized
        // rect and clamps at the document edges on its own, so this works
        // regardless of clip-view flippedness (the manual clip math did not).
        let viewportHeight = enclosingScrollView?.contentView.bounds.height ?? rect.height
        let topMargin: CGFloat = 12
        let revealRect = NSRect(x: rect.minX,
                                y: max(0, rect.minY - topMargin),
                                width: max(1, rect.width),
                                height: max(rect.height, viewportHeight))
        scrollToVisible(revealRect)
    }

    /// AppKit draws the caret as tall as the whole line, and body lines are
    /// set at ~1.7× the font size, so it towered over the letters. Draw it
    /// the height of the font instead (ascender to descender), standing on
    /// the line's baseline. Next to collapsed Markdown syntax the neighboring
    /// font is tiny, so never go below the body font.
    override func drawInsertionPoint(in rect: NSRect, color: NSColor, turnedOn flag: Bool) {
        super.drawInsertionPoint(in: fittedCaretRect(rect), color: color, turnedOn: flag)
    }

    private func fittedCaretRect(_ rect: NSRect) -> NSRect {
        let typed = (typingAttributes[.font] as? NSFont) ?? font ?? .systemFont(ofSize: fontSize)
        let caretFont = typed.pointSize < fontSize * 0.8 ? (NSFont(descriptor: typed.fontDescriptor, size: fontSize) ?? typed) : typed
        let height = ceil(caretFont.ascender - caretFont.descender)
        guard height < rect.height else { return rect }

        var fitted = rect
        fitted.size.height = height
        // The baseline of the line the caret is on: from the glyph at the
        // caret, or the one before it at the end of a wrapped line.
        if let baseline = baselineY(near: rect) {
            fitted.origin.y = baseline - caretFont.ascender
        } else {
            fitted.origin.y = rect.maxY - height
        }
        // Stay inside the line AppKit gave us.
        fitted.origin.y = min(max(fitted.origin.y, rect.minY), rect.maxY - height)
        fitted.origin.y = floor(fitted.origin.y)
        return fitted
    }

    private func baselineY(near rect: NSRect) -> CGFloat? {
        guard let layoutManager, let textStorage, textStorage.length > 0 else { return nil }
        let location = selectedRange().location
        for index in [location, location - 1] where index >= 0 && index < textStorage.length {
            let glyph = layoutManager.glyphIndexForCharacter(at: index)
            guard glyph < layoutManager.numberOfGlyphs else { continue }
            var line = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            line.origin.y += textContainerOrigin.y
            guard line.minY <= rect.midY, rect.midY <= line.maxY else { continue }
            return line.minY + layoutManager.location(forGlyphAt: glyph).y
        }
        return nil
    }

    override func setNeedsDisplay(_ invalidRect: NSRect, avoidAdditionalLayout flag: Bool) {
        // Widen caret invalidation to match the taller caret drawn above, so
        // blinking off erases all of it.
        var rect = invalidRect
        if rect.width <= 3 {
            rect = rect.insetBy(dx: 0, dy: -fontSize)
        }
        super.setNeedsDisplay(rect, avoidAdditionalLayout: flag)
    }

    // MARK: - Pasting and dropping media

    /// Pasted or dropped images and videos are copied into the library's
    /// `attachments/` folder and linked on their own line (AttachmentImport);
    /// anything else pastes and drops as before.
    override func paste(_ sender: Any?) {
        if insertMedia(from: .general, replacing: selectedRange()) { return }
        super.paste(sender)
    }

    override var acceptableDragTypes: [NSPasteboard.PasteboardType] {
        super.acceptableDragTypes + [.fileURL]
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        let operation = super.draggingEntered(sender)
        return mediaFiles(on: sender.draggingPasteboard).isEmpty ? operation : .copy
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        let operation = super.draggingUpdated(sender)
        return mediaFiles(on: sender.draggingPasteboard).isEmpty ? operation : .copy
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let point = convert(sender.draggingLocation, from: nil)
        let location = characterIndexForInsertion(at: point)
        if insertMedia(from: sender.draggingPasteboard, replacing: NSRange(location: location, length: 0)) {
            return true
        }
        return super.performDragOperation(sender)
    }

    private func mediaFiles(on pasteboard: NSPasteboard) -> [URL] {
        let urls = pasteboard.readObjects(forClasses: [NSURL.self],
                                          options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        return urls.filter(AttachmentImport.isMedia)
    }

    /// Image data on the pasteboard (a screenshot, an image copied from a
    /// browser) in its own format where it has one, else as PNG. Skipped when
    /// there is text too: Word and Pages put a picture of copied text there.
    private func pastedImage(on pasteboard: NSPasteboard) -> (data: Data, ext: String)? {
        guard pasteboard.string(forType: .string) == nil else { return nil }
        let native: [(NSPasteboard.PasteboardType, String)] = [
            (.png, "png"), (NSPasteboard.PasteboardType("public.jpeg"), "jpg"),
            (NSPasteboard.PasteboardType("com.compuserve.gif"), "gif"),
        ]
        for (type, ext) in native {
            if let data = pasteboard.data(forType: type) { return (data, ext) }
        }
        guard let tiff = pasteboard.data(forType: .tiff),
              let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) else { return nil }
        return (png, "png")
    }

    private func insertMedia(from pasteboard: NSPasteboard, replacing range: NSRange) -> Bool {
        guard let library = ImageLoader.shared.workspaceURL else { return false }
        let hasFiles = pasteboard.canReadObject(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true])
        var imported: [URL] = []
        if hasFiles {
            imported = mediaFiles(on: pasteboard).compactMap { try? AttachmentImport.importFile($0, into: library) }
        } else if let image = pastedImage(on: pasteboard),
                  let url = try? AttachmentImport.saveImage(image.data, ext: image.ext, into: library) {
            imported = [url]
        }
        return insertImported(imported, replacing: range)
    }

    /// Links files already copied into the library, each on a line of its own.
    private func insertImported(_ imported: [URL], replacing range: NSRange) -> Bool {
        guard let library = ImageLoader.shared.workspaceURL, !imported.isEmpty else { return false }

        let noteFolder = ImageLoader.shared.baseURL ?? library
        var text = imported
            .map { AttachmentImport.markdown(forPath: AttachmentImport.relativePath(from: noteFolder, to: $0)) }
            .joined(separator: "\n")
        // Each embed on a line of its own.
        let ns = string as NSString
        let start = min(range.location, ns.length)
        let end = min(NSMaxRange(range), ns.length)
        if start > 0, ns.character(at: start - 1) != 0x0A { text = "\n" + text }
        if end < ns.length, ns.character(at: end) != 0x0A { text += "\n" }
        window?.makeFirstResponder(self)
        insertText(text, replacementRange: NSRange(location: start, length: end - start))
        return true
    }

    // MARK: - First responder

    override var acceptsFirstResponder: Bool { true }

    override func becomeFirstResponder() -> Bool {
        let result = super.becomeFirstResponder()
        if result { restyle() }
        return result
    }

    override func resignFirstResponder() -> Bool {
        let result = super.resignFirstResponder()
        if result { slashMenu.close() }
        return result
    }

    /// Check if this text view is currently the window's first responder.
    func isFirstResponder() -> Bool {
        return window?.firstResponder == self
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        Diag.log("viewDidMoveToWindow window=\(window != nil) frame=\(frame) isEditable=\(isEditable)")
        if window != nil { restyle() } else { slashMenu.close() }
        if window != nil, focusesOnAppear {
            DispatchQueue.main.async { [weak self] in
                guard let self, let window = self.window else { return }
                window.makeFirstResponder(self)
                self.setSelectedRange(NSRange(location: (self.string as NSString).length, length: 0))
            }
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard let placeholder = draftPlaceholder, string == FileNoteStore.draftContent,
              let layoutManager, let textContainer else { return }
        // Just past the (possibly hidden) "# ", on the heading's line.
        let glyphs = layoutManager.glyphRange(forCharacterRange: NSRange(location: 0, length: 2), actualCharacterRange: nil)
        let line = layoutManager.lineFragmentRect(forGlyphAt: 0, effectiveRange: nil)
        let marks = layoutManager.boundingRect(forGlyphRange: glyphs, in: textContainer)
        let origin = textContainerOrigin
        let faint = NSColor.themed(\.faintIcon)

        let titleFont = WysiwygEngine.resolveFont(name: fontName, size: (fontSize * WysiwygEngine.titleScale).rounded(),
                                                  weight: .semibold)
        let title = NSAttributedString(string: placeholder.title,
                                       attributes: [.font: titleFont, .foregroundColor: faint])
        let titleY = origin.y + line.minY + max(0, (line.height - titleFont.ascender + titleFont.descender) / 2)
        title.draw(at: NSPoint(x: origin.x + marks.maxX, y: titleY))

        let hintFont = WysiwygEngine.resolveFont(name: fontName, size: fontSize)
        let hint = NSAttributedString(string: placeholder.hint,
                                      attributes: [.font: hintFont, .foregroundColor: faint])
        hint.draw(at: NSPoint(x: origin.x + line.minX, y: origin.y + line.maxY + 14))
    }

    private func rebuildEngine() {
        // The accent is read here; an accent change rebuilds the window
        // (WindowRoot's .id), so the editor picks up the new color.
        let style = WysiwygEngine.makeStyle(fontSize: fontSize, lineHeight: lineHeight, fontName: fontName,
                                            strongColor: MarkdownPalette.strong(accentHex: AppSettings.accentColorHex))
        engine = WysiwygEngine(style: style)
        engineFontSize = fontSize
        engineLineHeight = lineHeight
        engineFontName = fontName
        engineBuildCount += 1
        typingAttributes = style.baseParagraph
    }

    func rebuildEngineAndRestyle() {
        rebuildEngine()
        restyle()
    }

    /// Light ↔ dark. Colors in the text follow by themselves, but tables,
    /// math and front matter are drawn into cached images: rebuild those.
    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        guard engine != nil else { return }
        rebuildEngineAndRestyle()
    }

    /// Rebuild the styling engine + restyle only when the font size or line
    /// height actually changed. Called from `updateNSView`, which SwiftUI may
    /// invoke many times for reasons unrelated to editor styling.
    func rebuildEngineAndRestyleIfNeeded() {
        if engine == nil || engineFontSize != fontSize || engineLineHeight != lineHeight || engineFontName != fontName {
            rebuildEngineAndRestyle()
        }
    }

    // MARK: - Text changes

    override func didChangeText() {
        super.didChangeText()
        if isRestyling { return }
        Diag.log("didChangeText string='\(string.prefix(30))'")
        let edit = pendingEdit
        let edited = pendingEditLocation
        let typedSlash = pendingSlash
        pendingEdit = nil
        pendingEditLocation = nil
        pendingSlash = false
        defer { updateSlashMenu(typedSlash: typedSlash) }
        let caret = selectedRange().location
        // The edited line as well as the caret's: they differ when the edit
        // isn't typed at the caret (a checkbox clicked on another line).
        let lines: [Int]? = switch edit {
        case .withinLine?, .joinedLines?: [caret] + (edited.map { [$0] } ?? [])
        case .newLine?: [caret, max(0, caret - 1)]
        default: nil
        }
        if !(lines.map { restyleLinesAlone(at: $0) } ?? false) {
            restyle()
        }
        onTextChange?(markdownString())
    }

    // MARK: - Restyling one line

    /// What the edit under way does to lines, set by `shouldChangeText`:
    /// typing within a line, ↩ (a line break alone), or ⌫ joining two lines.
    /// Anything else — a paste over several lines, an attachment touched,
    /// several edits before `didChangeText` — restyles the whole note.
    private enum LineEdit { case withinLine, newLine, joinedLines, other }
    private var pendingEdit: LineEdit?
    /// Where the edit under way starts, for restyling its line.
    private var pendingEditLocation: Int?
    /// The edit under way types a slash, which may open the slash menu.
    private var pendingSlash = false

    override func shouldChangeText(in range: NSRange, replacementString: String?) -> Bool {
        let allowed = super.shouldChangeText(in: range, replacementString: replacementString)
        if allowed, !isRestyling {
            let ns = string as NSString
            let removed = NSMaxRange(range) <= ns.length ? ns.substring(with: range) : "\u{FFFC}"
            let inserted = replacementString ?? ""
            let kind: LineEdit
            if removed.contains("\u{FFFC}") {
                kind = .other
            } else if !removed.contains(where: \.isNewline) && !inserted.contains(where: \.isNewline) {
                kind = .withinLine
            } else if !removed.contains(where: \.isNewline) && inserted == "\n" {
                kind = .newLine
            } else if removed == "\n" && inserted.isEmpty {
                kind = .joinedLines
            } else {
                kind = .other
            }
            if pendingEdit == nil { pendingEditLocation = range.location }
            pendingEdit = pendingEdit == nil ? kind : (pendingEdit == .withinLine && kind == .withinLine ? .withinLine : .other)
            // A lone slash typed (or committed by an input method in place
            // of what it was composing), not one replacing a selection.
            let typed = inserted as NSString
            pendingSlash = typed.length == 1 && SlashCommand.isTrigger(typed.character(at: 0))
                && (range.length == 0 || range == markedRange())
        }
        return allowed
    }

    /// After the caret moved from display location `before`: restyle the
    /// line it left and the one it is on — just those lines when they can be
    /// styled alone, else the whole note (a table, image or block reveals its
    /// source as the caret enters and collapses as it leaves).
    private func restyleCaretLines(from before: Int) {
        if !restyleLinesAlone(at: [before, selectedRange().location]) { restyle() }
    }

    /// The typing fast path: restyles only the lines holding the display
    /// `locations`, when every one of them can be styled alone
    /// (`WysiwygEngine.canStyleAlone`) — the same result as a whole-note
    /// pass, at the cost of a line. Returns false, changing nothing, when
    /// any line needs the whole note.
    @discardableResult
    func restyleLinesAlone(at locations: [Int]) -> Bool {
        guard !isRestyling, let storage = textStorage, let engine, storage.length > 0 else { return false }
        let ns = storage.string as NSString
        var lines: [NSRange] = []
        for location in locations {
            let line = ns.lineRange(for: NSRange(location: min(max(0, location), ns.length), length: 0))
            if !lines.contains(line) { lines.append(line) }
        }
        guard lines.allSatisfy({ WysiwygEngine.canStyleAlone(lineRange: $0, in: storage) })
        else { return false }

        isRestyling = true
        defer { isRestyling = false }
        let caret = selectedRange().location
        let width = max(120, (textContainer?.size.width ?? bounds.width))
        let styled = lines.map { line -> NSAttributedString in
            let text = ns.substring(with: line)
            let contentEnd = line.location + (text.trimmingCharacters(in: .newlines) as NSString).length
            let cursor = caret >= line.location && caret <= contentEnd ? caret - line.location : -1
            return engine.styledLine(text, cursorInLine: cursor, maxWidth: width)
        }
        storage.beginEditing()
        for (line, attributed) in zip(lines, styled) {
            attributed.enumerateAttributes(in: NSRange(location: 0, length: attributed.length)) { attributes, range, _ in
                storage.setAttributes(attributes, range: NSRange(location: line.location + range.location, length: range.length))
            }
        }
        storage.endEditing()
        fitFrameToContent()
        if let font = typingAttributes[.font] as? NSFont, font.pointSize < 1 {
            typingAttributes = WysiwygEngine.makeStyle(fontSize: fontSize, lineHeight: lineHeight, fontName: fontName).baseParagraph
        }
        return true
    }

    /// Request a restyle that is coalesced to at most once per runloop turn.
    /// Used by asynchronous triggers (remote image loads) that can fire in
    /// rapid bursts while the user is scrolling. Without coalescing, each event
    /// would run a full O(document) restyle on the main thread → beachball.
    func setNeedsRestyle() {
        if restyleScheduled { return }
        restyleScheduled = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.restyleScheduled = false
            self.restyle()
        }
    }

    func restyle() {
        if isRestyling { return }
        guard let storage = textStorage else { return }
        setHoveredLink(nil)
        isRestyling = true
        restyleCount += 1
        defer { isRestyling = false }

        // Restyling briefly expands every image / video / table attachment
        // back to its (much shorter) Markdown source, which shrinks the
        // document and lets the clip view clamp the scroll offset. Remember
        // where the reader was and put them back afterwards.
        let clipView = enclosingScrollView?.contentView
        let savedScrollOrigin = clipView?.bounds.origin
        // Where the caret's line sits in the viewport. A restyle can change
        // heights above it (an image's source revealed, a table opening as
        // source); keeping this line still keeps the text under the pointer.
        let caretOffset: CGFloat? = {
            guard let clipView, let y = caretLineY() else { return nil }
            let offset = clipView.convert(NSPoint(x: 0, y: y), from: self).y - clipView.bounds.minY
            return (0...clipView.bounds.height).contains(offset) ? offset : nil
        }()

        // Map the caret into raw-markdown coordinates BEFORE collapsing/expanding
        // attachments, so it survives the coordinate change (image syntax ↔
        // attachment, table source ↔ grid).
        let rawCursor = rawLocation(forDisplayLocation: selectedRange().location)
        let rawMarkdown = markdownString()
        let rawNS = rawMarkdown as NSString
        let clampedRaw = min(rawCursor, rawNS.length)

        // The table / math span the caret is inside (by ordinal index in
        // document order) stays editable source; all others render. Ordinal
        // indexing is stable because image collapsing never adds or removes
        // tables or math spans.
        let tables = MarkdownTableParser.tables(in: rawNS)
        var activeTableIndex = -1
        for (i, t) in tables.enumerated() where NSLocationInRange(clampedRaw, t.range) || clampedRaw == NSMaxRange(t.range) {
            activeTableIndex = i
            break
        }

        let mathSpans = LatexMath.mathSpans(in: rawNS, excludingTableRanges: tables.map { $0.range })
        var activeMathIndex = -1
        for (i, s) in mathSpans.enumerated() where NSLocationInRange(clampedRaw, s.range) || clampedRaw == NSMaxRange(s.range) {
            activeMathIndex = i
            break
        }

        // Style a copy away from the layout manager, then write back only
        // what changed, so unchanged parts keep their layout.
        let contentWidth = max(120, (textContainer?.size.width ?? bounds.width))
        let styled = NSTextStorage(string: rawMarkdown)
        engine?.applyStyle(to: styled, cursorLocation: clampedRaw,
                           activeTableIndex: activeTableIndex, activeMathIndex: activeMathIndex,
                           maxWidth: contentWidth)
        WysiwygEngine.applyChanges(from: styled, to: storage)

        // Map the caret back into the (re-collapsed) display coordinates.
        let displayCursor = displayLocation(forRawLocation: clampedRaw)
        setSelectedRange(NSRange(location: min(displayCursor, storage.length), length: 0))

        if let clipView, var target = savedScrollOrigin {
            fitFrameToContent()
            if let caretOffset, let y = caretLineY() {
                target.y = clipView.convert(NSPoint(x: 0, y: y), from: self).y - caretOffset
            }
            if clipView.bounds.origin != target {
                clipView.setBoundsOrigin(clipView.constrainBoundsRect(
                    NSRect(origin: target, size: clipView.bounds.size)).origin)
                enclosingScrollView?.reflectScrolledClipView(clipView)
            }
        }

        repositionVideoPlayers()

        // Text typed next inherits the attributes before the caret; if that is
        // collapsed syntax (a 0.1pt font), fall back to the body style.
        if let font = typingAttributes[.font] as? NSFont, font.pointSize < 1 {
            typingAttributes = WysiwygEngine.makeStyle(fontSize: fontSize, lineHeight: lineHeight, fontName: fontName).baseParagraph
        }
    }

    /// Top of the caret's line fragment, in view coordinates.
    private func caretLineY() -> CGFloat? {
        guard let layoutManager, let storage = textStorage else { return nil }
        let location = selectedRange().location
        let rect: NSRect
        if location >= storage.length, !layoutManager.extraLineFragmentRect.isEmpty {
            rect = layoutManager.extraLineFragmentRect
        } else if storage.length > 0 {
            let glyph = layoutManager.glyphIndexForCharacter(at: min(location, storage.length - 1))
            rect = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
        } else {
            return nil
        }
        return rect.minY + textContainerOrigin.y
    }

    @objc private func handleImageDidLoad() {
        setNeedsRestyle()
    }

    @objc private func handleLinkIndexChange() {
        setNeedsRestyle()
    }

    @objc private func handleAnimatedImageFrameDidChange(_ notification: Notification) {
        guard let animatedAttachment = notification.object as? ImageTextAttachment,
              let storage = textStorage,
              let layoutManager = layoutManager,
              let textContainer = textContainer else {
            return
        }
        // Redraw only the rect occupied by the animated attachment, and only if
        // it is currently on-screen. Avoids invalidating the entire text view /
        // scroll content on every GIF frame, which would compete with scrolling.
        storage.enumerateAttribute(.attachment, in: NSRange(location: 0, length: storage.length)) { value, range, stop in
            guard let attachment = value as? ImageTextAttachment, attachment === animatedAttachment else { return }
            let glyphRange = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            var rect = layoutManager.boundingRect(forGlyphRange: glyphRange, in: textContainer)
            rect.origin.x += textContainerInset.width
            rect.origin.y += textContainerInset.height
            if rect.intersects(visibleRect) {
                setNeedsDisplay(rect)
            }
            stop.pointee = true
        }
    }

    private func markdownString() -> String {
        guard let storage = textStorage else { return string }
        let result = NSMutableString(string: storage.string)
        var replacements: [(range: NSRange, markdown: String)] = []

        storage.enumerateAttribute(.attachment, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let attachment = value as? MarkdownBackedAttachment else { return }
            replacements.append((range, attachment.markdownSource))
        }

        for replacement in replacements.reversed() {
            result.replaceCharacters(in: replacement.range, with: replacement.markdown)
        }

        return result as String
    }

    // MARK: - Replacing the text

    /// Puts `markdown` in place of the note's text as one undoable edit — a
    /// change from outside merged in — keeping the caret on the text it was
    /// on and its line where it was in the window. Refused (false) while an
    /// input method is composing, which a replacement would break.
    func replaceMarkdown(with markdown: String) -> Bool {
        guard !hasMarkedText(), let storage = textStorage else { return false }
        slashMenu.close()
        let old = markdownString()
        guard old != markdown else { return true }
        let caret = Merge3.mapLocation(rawLocation(forDisplayLocation: selectedRange().location),
                                       from: old, to: markdown)
        let clipView = enclosingScrollView?.contentView
        let caretOffset: CGFloat? = clipView.flatMap { clip in
            caretLineY().map { clip.convert(NSPoint(x: 0, y: $0), from: self).y - clip.bounds.minY }
        }

        let all = NSRange(location: 0, length: storage.length)
        guard shouldChangeText(in: all, replacementString: markdown) else { return false }
        // Plain body text, not the attributes of whatever came first (an
        // image attachment would spread over the whole note); the restyle
        // in didChangeText styles it, collapsing images, tables and math.
        let body = WysiwygEngine.makeStyle(fontSize: fontSize, lineHeight: lineHeight, fontName: fontName).baseParagraph
        storage.replaceCharacters(in: all, with: NSAttributedString(string: markdown, attributes: body))
        // The storage holds raw Markdown until the restyle, so raw = display.
        setSelectedRange(NSRange(location: min(caret, storage.length), length: 0))
        didChangeText()

        if let clipView, let caretOffset, let y = caretLineY() {
            fitFrameToContent()
            let target = NSPoint(x: clipView.bounds.minX,
                                 y: clipView.convert(NSPoint(x: 0, y: y), from: self).y - caretOffset)
            clipView.setBoundsOrigin(clipView.constrainBoundsRect(
                NSRect(origin: target, size: clipView.bounds.size)).origin)
            enclosingScrollView?.reflectScrolledClipView(clipView)
        }
        // Lines can move up past where the old text ended; redraw all of it
        // so none is left behind on screen.
        needsDisplay = true
        return true
    }

    // MARK: - Raw ↔ display coordinate mapping
    //
    // The storage holds collapsed attachments (images / table grids), each one
    // character standing in for a multi-character markdown source. These helpers
    // convert between the displayed storage offsets and the raw-markdown offsets
    // so the caret survives the collapse/expand that happens on every restyle.

    /// Raw-markdown offset corresponding to a location in the current storage.
    func rawLocation(forDisplayLocation displayLocation: Int) -> Int {
        guard let storage = textStorage else { return displayLocation }
        let clamped = min(max(0, displayLocation), storage.length)
        return CursorMapping.rawLocation(forDisplayLocation: clamped, segments: segments(from: storage))
    }

    /// Display offset (in the collapsed storage) corresponding to a raw-markdown
    /// offset. A raw location inside a collapsed attachment snaps to just before
    /// it; one at the attachment's trailing boundary maps to just after it.
    func displayLocation(forRawLocation rawLocation: Int) -> Int {
        guard let storage = textStorage else { return rawLocation }
        return CursorMapping.displayLocation(forRawLocation: rawLocation,
                                             segments: segments(from: storage),
                                             displayTotal: storage.length)
    }

    /// Break the live storage into the run model `CursorMapping` operates on:
    /// each collapsed attachment becomes one `.attachment` segment (display
    /// length 1, raw length = its markdown source), and every other run becomes a
    /// `.text` segment.
    private func segments(from storage: NSTextStorage) -> [CursorMapping.Segment] {
        var result: [CursorMapping.Segment] = []
        storage.enumerateAttribute(.attachment, in: NSRange(location: 0, length: storage.length), options: []) { value, range, _ in
            if let attachment = value as? MarkdownBackedAttachment {
                result.append(.attachment(rawLength: (attachment.markdownSource as NSString).length))
            } else {
                result.append(.text(range.length))
            }
        }
        return result
    }

    // MARK: - Mouse / cursor tracking

    /// Moving the caret with the keyboard re-renders too, so the syntax of the
    /// line it lands on (an image's source, a heading's `##`) is revealed the
    /// same way a click reveals it.
    override func keyDown(with event: NSEvent) {
        if slashMenu.isOpen, !hasMarkedText(), handleSlashMenuKey(event) { return }
        let before = selectedRange()
        super.keyDown(with: event)
        let navigationKeys: Set<NSEvent.SpecialKey> = [.upArrow, .downArrow, .leftArrow, .rightArrow,
                                                       .home, .end, .pageUp, .pageDown]
        guard let key = event.specialKey, navigationKeys.contains(key),
              !event.modifierFlags.contains(.shift), let storage = textStorage else { return }
        var after = selectedRange()
        guard after.length == 0 else { return }

        // A revealed image follows its source but stands for no raw text, so
        // both of its edges map back to the end of the source. Moving forward
        // from there steps over it (and its line break) to the next line.
        func revealedImage(at index: Int) -> Bool {
            guard index >= 0, index < storage.length,
                  let attachment = storage.attribute(.attachment, at: index, effectiveRange: nil)
                      as? MarkdownBackedAttachment else { return false }
            return attachment.markdownSource.isEmpty
        }
        // Right arrow: from just before the image. Down arrow: from anywhere
        // on its source line, landing on either edge of the image.
        let image: Int? = switch key {
        case .rightArrow where revealedImage(at: before.location) && after.location <= before.location + 1:
            before.location
        case .downArrow where revealedImage(at: after.location) && after.location >= before.location:
            after.location
        case .downArrow where revealedImage(at: after.location - 1) && after.location > before.location:
            after.location - 1
        default:
            nil
        }
        if let image {
            var next = image + 1
            if next < storage.length, (storage.string as NSString).character(at: next) == 0x0A {
                next += 1
            }
            after = NSRange(location: next, length: 0)
            setSelectedRange(after)
        }
        // Up into the properties panel: open it for editing rather than
        // leaving the caret at the panel's foot.
        if after.location == 0, after != before, let panel = frontmatterPanel() {
            openFrontmatter(panel)
            return
        }
        if after != before {
            restyleCaretLines(from: before.location)
        }
    }

    override func mouseDown(with event: NSEvent) {
        slashMenu.close()
        if event.clickCount == 1, !event.modifierFlags.contains(.shift),
           let hit = link(at: convert(event.locationInWindow, from: nil)), follows(hit, clickedWith: event) {
            return
        }
        if let window = window, window.firstResponder != self {
            window.makeFirstResponder(self)
        }
        if toggleTaskIfCheckboxClicked(event) { return }
        if playVideoIfClicked(event) { return }
        if revealImageSourceIfClicked(event) { return }
        if revealFrontmatterIfClicked(event) { return }
        let before = selectedRange()
        super.mouseDown(with: event)
        // A simple click (no drag-selection): re-render so the table the caret
        // landed in opens as editable source, and the one it left re-renders as a
        // grid. Skipped when a range is selected so a drag-selection isn't
        // clobbered. The caret survives the storage rebuild via the raw↔display
        // remap in restyle().
        if selectedRange().length == 0 {
            if selectedRange().location == 0, let panel = frontmatterPanel() {
                openFrontmatter(panel)
            } else {
                restyleCaretLines(from: before.location)
            }
        }
    }

    /// To-do checkboxes are drawn in the list indent (the `[ ]` syntax is
    /// collapsed), so a click on one is handled here: it flips `[ ]` ↔ `[x]`
    /// on that line instead of placing the caret.
    private func toggleTaskIfCheckboxClicked(_ event: NSEvent) -> Bool {
        guard let layoutManager, let textContainer, let storage = textStorage, storage.length > 0 else { return false }
        let point = convert(event.locationInWindow, from: nil)
        let containerPoint = NSPoint(x: point.x - textContainerOrigin.x, y: point.y - textContainerOrigin.y)
        let glyphIndex = layoutManager.glyphIndex(for: containerPoint, in: textContainer)
        let charIndex = min(layoutManager.characterIndexForGlyph(at: glyphIndex), storage.length - 1)
        let string = storage.string as NSString
        let lineRange = string.lineRange(for: NSRange(location: charIndex, length: 0))
        guard let marker = storage.attribute(.listMarker, at: lineRange.location, effectiveRange: nil) as? ListMarker,
              case .task = marker.kind else { return false }

        let firstGlyph = layoutManager.glyphIndexForCharacter(at: lineRange.location)
        let fragment = layoutManager.lineFragmentRect(forGlyphAt: firstGlyph, effectiveRange: nil)
        let baseline = fragment.minY + layoutManager.location(forGlyphAt: firstGlyph).y
        guard marker.checkboxRect(baselineY: baseline).insetBy(dx: -4, dy: -4).contains(containerPoint) else { return false }

        let line = string.substring(with: lineRange) as NSString
        guard let regex = try? NSRegularExpression(pattern: #"\[([ xX])\]"#),
              let match = regex.firstMatch(in: line as String, range: NSRange(location: 0, length: line.length)) else { return false }
        let mark = line.substring(with: match.range(at: 1))
        let target = NSRange(location: lineRange.location + match.range(at: 1).location, length: 1)
        guard shouldChangeText(in: target, replacementString: mark == " " ? "x" : " ") else { return true }
        storage.replaceCharacters(in: target, with: mark == " " ? "x" : " ")
        didChangeText()
        return true
    }

    // MARK: - Inline video

    /// Players opened over video cards, keyed by attachment. The engine reuses
    /// a card's attachment object across restyles, so a player can follow its
    /// card as the text re-lays out.
    private var videoPlayers: [ObjectIdentifier: (attachment: VideoAttachment, view: InlinePlayerView)] = [:]

    private func playVideoIfClicked(_ event: NSEvent) -> Bool {
        guard let layoutManager, let textContainer, let storage = textStorage, storage.length > 0 else { return false }
        let point = convert(event.locationInWindow, from: nil)
        let containerPoint = NSPoint(x: point.x - textContainerOrigin.x, y: point.y - textContainerOrigin.y)
        let glyphIndex = layoutManager.glyphIndex(for: containerPoint, in: textContainer)
        let charIndex = layoutManager.characterIndexForGlyph(at: glyphIndex)
        guard charIndex < storage.length,
              let attachment = storage.attribute(.attachment, at: charIndex, effectiveRange: nil) as? VideoAttachment,
              let frame = videoFrame(at: charIndex, attachment: attachment), frame.contains(point) else { return false }

        let id = ObjectIdentifier(attachment)
        if let existing = videoPlayers[id] {
            existing.view.player?.play()
            return true
        }
        let playerView = InlinePlayerView(frame: frame)
        playerView.controlsStyle = .inline
        playerView.showsFullScreenToggleButton = true
        playerView.wantsLayer = true
        playerView.layer?.cornerRadius = 8
        playerView.layer?.masksToBounds = true
        playerView.player = AVPlayer(url: attachment.fileURL)
        addSubview(playerView)
        videoPlayers[id] = (attachment, playerView)
        playerView.player?.play()
        return true
    }

    /// A click on an image puts the caret at the end of its Markdown source
    /// (restyle then shows the source above the image, as in Obsidian's live
    /// preview) instead of selecting the attachment character — which tinted
    /// the whole image with the selection color.
    private func revealImageSourceIfClicked(_ event: NSEvent) -> Bool {
        guard event.clickCount == 1, !event.modifierFlags.contains(.shift),
              let layoutManager, let textContainer, let storage = textStorage, storage.length > 0 else { return false }
        let point = convert(event.locationInWindow, from: nil)
        let containerPoint = NSPoint(x: point.x - textContainerOrigin.x, y: point.y - textContainerOrigin.y)
        let glyphIndex = layoutManager.glyphIndex(for: containerPoint, in: textContainer)
        let charIndex = layoutManager.characterIndexForGlyph(at: glyphIndex)
        guard charIndex < storage.length,
              let attachment = storage.attribute(.attachment, at: charIndex, effectiveRange: nil) as? ImageTextAttachment
        else { return false }
        let rect = layoutManager.boundingRect(forGlyphRange: NSRange(location: glyphIndex, length: 1), in: textContainer)
            .offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
        guard rect.contains(point) else { return false }

        // A collapsed image stands for its whole source, so "after it" is the
        // source's end; a revealed one follows its source, so "before it" is.
        let caret = attachment.markdownSource.isEmpty ? charIndex : charIndex + 1
        setSelectedRange(NSRange(location: caret, length: 0))
        restyle()
        return true
    }

    /// The properties panel, when the note starts with one (collapsed).
    private func frontmatterPanel() -> FrontmatterAttachment? {
        guard let storage = textStorage, storage.length > 0 else { return nil }
        return storage.attribute(.attachment, at: 0, effectiveRange: nil) as? FrontmatterAttachment
    }

    /// A click on the properties panel opens the frontmatter as its source,
    /// as in Obsidian's live preview, instead of leaving the caret before
    /// the panel — where it drew at the panel's foot and opened nothing.
    private func revealFrontmatterIfClicked(_ event: NSEvent) -> Bool {
        guard event.clickCount == 1, !event.modifierFlags.contains(.shift),
              let layoutManager, let textContainer, let panel = frontmatterPanel() else { return false }
        let glyph = layoutManager.glyphIndexForCharacter(at: 0)
        let rect = layoutManager.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: textContainer)
            .offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
        guard rect.contains(convert(event.locationInWindow, from: nil)) else { return false }
        openFrontmatter(panel)
        return true
    }

    /// Shows the frontmatter as source with the caret at the end of its last
    /// property, ready to edit. The caret goes after the panel first — the
    /// source's end, which opens it — then, the source now standing as itself
    /// at the top, back to the line before the closing `---`.
    /// The page stays where it was: restyle keeps the caret's line still,
    /// and the caret jumps from the panel's foot to the source's last line,
    /// which scrolled the page — but the panel is the top of the note.
    private func openFrontmatter(_ panel: FrontmatterAttachment) {
        let clipView = enclosingScrollView?.contentView
        let origin = clipView?.bounds.origin
        setSelectedRange(NSRange(location: 1, length: 0))
        restyle()
        let source = panel.markdownSource as NSString
        let closing = source.range(of: "\n", options: .backwards)
        let caret = closing.location == NSNotFound ? source.length : closing.location
        setSelectedRange(NSRange(location: min(caret, textStorage?.length ?? 0), length: 0))
        if let clipView, let origin, clipView.bounds.origin != origin {
            clipView.setBoundsOrigin(clipView.constrainBoundsRect(NSRect(origin: origin, size: clipView.bounds.size)).origin)
            enclosingScrollView?.reflectScrolledClipView(clipView)
        }
    }

    /// The poster area of the video card at `charIndex`, in view coordinates.
    /// Attachments sit on the baseline, so the card's top is baseline − height.
    private func videoFrame(at charIndex: Int, attachment: VideoAttachment) -> NSRect? {
        guard let layoutManager else { return nil }
        let glyph = layoutManager.glyphIndexForCharacter(at: charIndex)
        let fragment = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
        let location = layoutManager.location(forGlyphAt: glyph)
        let size = layoutManager.attachmentSize(forGlyphAt: glyph)
        guard size.width > 0 else { return nil }
        let origin = NSPoint(x: fragment.minX + location.x + textContainerOrigin.x,
                             y: fragment.minY + location.y - size.height + textContainerOrigin.y)
        return NSRect(origin: origin, size: attachment.videoRect.size)
    }

    /// Keeps open players on their cards after a restyle; closes players whose
    /// card is gone (deleted, or switched to its raw source).
    private func repositionVideoPlayers() {
        guard !videoPlayers.isEmpty, let storage = textStorage else { return }
        var located: [ObjectIdentifier: Int] = [:]
        storage.enumerateAttribute(.attachment, in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            if let video = value as? VideoAttachment { located[ObjectIdentifier(video)] = range.location }
        }
        if let layoutManager, let textContainer { layoutManager.ensureLayout(for: textContainer) }
        for (id, entry) in videoPlayers {
            if let index = located[id], let frame = videoFrame(at: index, attachment: entry.attachment) {
                entry.view.frame = frame
            } else {
                entry.view.player?.pause()
                entry.view.removeFromSuperview()
                videoPlayers[id] = nil
            }
        }
    }

    // MARK: - Links (design board 58)

    /// Opens the note a link points to: its id, the heading to scroll to,
    /// and whether in a new tab behind this one.
    var onOpenNote: ((String, String?, Bool) -> Void)?
    /// Creates the note a link names that doesn't exist yet, beside this
    /// one; returns its id.
    var onCreateNote: ((String) -> String?)?

    /// A link under the pointer: its characters, and what it opens.
    private struct LinkHit {
        let range: NSRange
        var wiki: String?
        var web: String?
        var attachment: WikiLinkAttachment?
    }

    private func link(at point: NSPoint) -> LinkHit? {
        guard let layoutManager, let textContainer, let storage = textStorage, storage.length > 0 else { return nil }
        let inContainer = NSPoint(x: point.x - textContainerOrigin.x, y: point.y - textContainerOrigin.y)
        let glyph = layoutManager.glyphIndex(for: inContainer, in: textContainer)
        let box = layoutManager.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: textContainer)
        guard box.insetBy(dx: -1, dy: -2).contains(inContainer) else { return nil }
        let index = layoutManager.characterIndexForGlyph(at: glyph)
        guard index < storage.length else { return nil }
        let all = NSRange(location: 0, length: storage.length)
        var range = NSRange()
        if let wiki = storage.attribute(.wikiLink, at: index, longestEffectiveRange: &range, in: all) as? String {
            return LinkHit(range: range, wiki: wiki,
                           attachment: storage.attribute(.attachment, at: index, effectiveRange: nil) as? WikiLinkAttachment)
        }
        if let web = storage.attribute(.webLink, at: index, longestEffectiveRange: &range, in: all) as? String {
            return LinkHit(range: range, web: web)
        }
        return nil
    }

    // Hover: the pointing hand, a faint fill behind the link and a darker
    // underline; a web link's address shows at the page's foot.

    private var hoveredLink: LinkHit?
    private var linkTrackingArea: NSTrackingArea?

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let linkTrackingArea { removeTrackingArea(linkTrackingArea) }
        let area = NSTrackingArea(rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                                  owner: self)
        addTrackingArea(area)
        linkTrackingArea = area
    }

    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        let hit = link(at: convert(event.locationInWindow, from: nil))
        let wasOnLink = hoveredLink != nil
        setHoveredLink(hit)
        if hit != nil {
            NSCursor.pointingHand.set()
        } else if wasOnLink {
            NSCursor.iBeam.set()
        }
    }

    override func mouseExited(with event: NSEvent) {
        super.mouseExited(with: event)
        setHoveredLink(nil)
    }

    private func setHoveredLink(_ hit: LinkHit?) {
        guard hit?.range != hoveredLink?.range else { return }
        if let old = hoveredLink { showHover(old, on: false) }
        hoveredLink = hit
        if let hit { showHover(hit, on: true) }
        showAddress(hit?.web)
    }

    private func showHover(_ hit: LinkHit, on: Bool) {
        guard let layoutManager = layoutManager as? CodeBlockLayoutManager,
              NSMaxRange(hit.range) <= (textStorage?.length ?? 0) else { return }
        if let attachment = hit.attachment {
            attachment.isHovered = on
        } else if on {
            layoutManager.hoveredLink = hit.range
            layoutManager.addTemporaryAttribute(.underlineColor, value: MarkdownPalette.primary.withAlphaComponent(0.7),
                                                forCharacterRange: hit.range)
        } else {
            layoutManager.hoveredLink = nil
            layoutManager.removeTemporaryAttribute(.underlineColor, forCharacterRange: hit.range)
        }
        layoutManager.invalidateDisplay(forCharacterRange: hit.range)
    }

    private lazy var addressLabel: NSTextField = {
        let label = NSTextField(labelWithString: "")
        label.font = .systemFont(ofSize: 11)
        label.textColor = MarkdownPalette.secondary
        label.lineBreakMode = .byTruncatingMiddle
        label.wantsLayer = true
        label.layer?.backgroundColor = MarkdownPalette.inlineBg.cgColor
        label.layer?.cornerRadius = 5
        return label
    }()

    /// Where a web link goes, at the page's lower left as in a browser.
    private func showAddress(_ address: String?) {
        guard let address, let scrollView = enclosingScrollView else {
            addressLabel.removeFromSuperview()
            return
        }
        addressLabel.stringValue = "  \(address)  "
        addressLabel.layer?.backgroundColor = MarkdownPalette.inlineBg.cgColor
        let width = min(addressLabel.intrinsicContentSize.width, scrollView.bounds.width - 24)
        addressLabel.frame = NSRect(x: 12, y: scrollView.isFlipped ? scrollView.bounds.height - 32 : 12,
                                    width: width, height: 20)
        if addressLabel.superview !== scrollView { scrollView.addSubview(addressLabel) }
    }

    /// A click on a link opens it — unless the caret is on its line, where
    /// a plain click edits (⌘ still opens), or the pointer drags off to
    /// select. ⌘ opens a note in a new tab.
    private func follows(_ hit: LinkHit, clickedWith event: NSEvent) -> Bool {
        let command = event.modifierFlags.contains(.command)
        let ns = string as NSString
        if !command, isFirstResponder() {
            let caretLine = ns.lineRange(for: NSRange(location: min(selectedRange().location, ns.length), length: 0))
            if NSLocationInRange(hit.range.location, caretLine) { return false }
        }
        guard let window else { return false }
        let start = event.locationInWindow
        while let next = window.nextEvent(matching: [.leftMouseUp, .leftMouseDragged]) {
            if next.type == .leftMouseUp { break }
            if hypot(next.locationInWindow.x - start.x, next.locationInWindow.y - start.y) > 3 { return false }
        }
        open(hit, inNewTab: command)
        return true
    }

    private func open(_ hit: LinkHit, inNewTab: Bool) {
        if let web = hit.web {
            let url = URL(string: web) ?? web.addingPercentEncoding(withAllowedCharacters: .urlFragmentAllowed).flatMap(URL.init)
            if let url { NSWorkspace.shared.open(url) }
            return
        }
        guard let inner = hit.wiki, let link = WikiLink(inner) else { return }
        if let entry = WikiLinkIndex.shared.resolve(link.target, from: ImageLoader.shared.baseURL) {
            setHoveredLink(nil)
            onOpenNote?(entry.id, link.heading, inNewTab)
        } else {
            offerToCreate(link, at: hit.range, inNewTab: inNewTab)
        }
    }

    private var createPopover: NSPopover?

    /// A link to a note that doesn't exist: offers to create it.
    private func offerToCreate(_ link: WikiLink, at range: NSRange, inNewTab: Bool) {
        guard let layoutManager, let textContainer, onCreateNote != nil else { return }
        let glyphs = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        let rect = layoutManager.boundingRect(forGlyphRange: glyphs, in: textContainer)
            .offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
        let name = (link.target as NSString).lastPathComponent
        let popover = NSPopover()
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(rootView: CreateLinkedNoteView(
            name: name,
            onCancel: { [weak popover] in popover?.close() },
            onCreate: { [weak self, weak popover] in
                popover?.close()
                guard let self, let id = self.onCreateNote?(name) else { return }
                self.onOpenNote?(id, nil, inNewTab)
            }
        ))
        popover.show(relativeTo: rect, of: self, preferredEdge: .maxY)
        createPopover = popover
    }

    // MARK: - Slash menu

    /// Typing `/` at the start of a line or after a space opens a menu of
    /// blocks and formats to insert as Markdown (design board 56 A).
    private lazy var slashMenu: SlashMenuController = {
        let controller = SlashMenuController(textView: self)
        controller.displayAnchor = { [weak self] in
            guard let self, let anchor = self.slashMenu.anchor else { return nil }
            return self.displayLocation(forRawLocation: anchor)
        }
        return controller
    }()

    override func setSelectedRanges(_ ranges: [NSValue], affinity: NSSelectionAffinity, stillSelecting: Bool) {
        super.setSelectedRanges(ranges, affinity: affinity, stillSelecting: stillSelecting)
        if !isRestyling, slashMenu.isOpen { refreshSlashQuery() }
    }

    /// After an edit: opens the menu for a slash just typed, or filters the
    /// open one for the text typed after its slash.
    private func updateSlashMenu(typedSlash: Bool) {
        if slashMenu.isOpen {
            refreshSlashQuery()
            return
        }
        let caret = selectedRange()
        guard typedSlash, AppSettings.slashCommands, !hasMarkedText(), caret.length == 0, caret.location > 0,
              SlashCommand.isTrigger((string as NSString).character(at: caret.location - 1)) else { return }
        let slash = rawLocation(forDisplayLocation: caret.location - 1)
        guard SlashCommand.opensMenu(at: slash, in: markdownString() as NSString) else { return }
        slashMenu.open(at: slash) { [weak self] command in self?.applySlashCommand(command) }
    }

    /// Filters the open menu for the text between its slash and the caret,
    /// or closes it once the caret has left that text, the slash is gone or
    /// a space was typed.
    private func refreshSlashQuery() {
        guard let anchor = slashMenu.anchor else { return }
        let ns = string as NSString
        let slash = displayLocation(forRawLocation: anchor)
        let caret = selectedRange()
        guard caret.length == 0, slash < ns.length, caret.location > slash,
              SlashCommand.isTrigger(ns.character(at: slash)) else {
            slashMenu.close()
            return
        }
        var query = ns.substring(with: NSRange(location: slash + 1, length: caret.location - slash - 1))
        if hasMarkedText() {
            // Pinyin being composed may show separators between syllables.
            query = query.filter { !$0.isWhitespace && $0 != "'" }
        }
        guard query.count <= 32, !query.contains(where: { $0.isWhitespace || $0.isNewline }) else {
            slashMenu.close()
            return
        }
        slashMenu.update(query: query)
    }

    /// ↑ ↓ (and ⌃P ⌃N) move, ↵ or ⇥ insert, esc closes. Everything else
    /// types as usual and filters.
    private func handleSlashMenuKey(_ event: NSEvent) -> Bool {
        let modifiers = event.modifierFlags.intersection([.command, .shift, .option, .control])
        let control = modifiers == .control
        switch (event.keyCode, modifiers.isEmpty) {
        case (126, true):
            slashMenu.moveSelection(by: -1)
            return true
        case (125, true):
            slashMenu.moveSelection(by: 1)
            return true
        case (36, true), (76, true), (48, true):
            if let command = slashMenu.model.selected {
                applySlashCommand(command)
                return true
            }
            slashMenu.close()
            return false
        case (53, _):
            slashMenu.close()
            return true
        default:
            break
        }
        switch (control, event.charactersIgnoringModifiers) {
        case (true, "p"?):
            slashMenu.moveSelection(by: -1)
            return true
        case (true, "n"?):
            slashMenu.moveSelection(by: 1)
            return true
        default:
            return false
        }
    }

    /// Replaces the slash and its query with what `command` inserts. Line
    /// commands rewrite the whole line, so the edit spans it; a line holding
    /// an attachment (an image, a rendered table) keeps it, and only the
    /// slash and query are replaced.
    private func applySlashCommand(_ command: SlashCommand) {
        guard let anchor = slashMenu.anchor else { return }
        let ns = string as NSString
        let slash = displayLocation(forRawLocation: anchor)
        let caret = selectedRange().location
        slashMenu.close()
        guard slash < ns.length, caret > slash else { return }

        let lineRange = ns.lineRange(for: NSRange(location: slash, length: 0))
        var contentEnd = NSMaxRange(lineRange)
        while contentEnd > lineRange.location, ns.character(at: contentEnd - 1) == 0x0A || ns.character(at: contentEnd - 1) == 0x0D {
            contentEnd -= 1
        }
        var span = NSRange(location: lineRange.location, length: contentEnd - lineRange.location)
        if ns.substring(with: span).contains("\u{FFFC}") {
            span = NSRange(location: slash, length: caret - slash)
        }
        let query = NSRange(location: slash - span.location, length: caret - slash)
        let edit = command.edit(line: ns.substring(with: span), query: query)

        let rawStart = rawLocation(forDisplayLocation: span.location)
        insertText(edit.text, replacementRange: span)
        // Place the caret, restyle around it (a code block or table opens as
        // source), then select within it where the item asks.
        setSelectedRange(NSRange(location: displayLocation(forRawLocation: rawStart + edit.selection.location), length: 0))
        restyle()
        if edit.selection.length > 0 {
            let start = displayLocation(forRawLocation: rawStart + edit.selection.location)
            setSelectedRange(NSRange(location: start, length: edit.selection.length))
        }
        scrollRangeToVisible(selectedRange())
        if command.action == .chooseImage { chooseMediaFiles() }
    }

    /// Picks images or videos to link at the caret, copied into the library
    /// as a drop would be.
    private func chooseMediaFiles() {
        guard let window, let library = ImageLoader.shared.workspaceURL else { return }
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        panel.allowedContentTypes = [.image, .movie]
        panel.prompt = "Insert"
        let range = selectedRange()
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let self else { return }
            let imported = panel.urls.compactMap { try? AttachmentImport.importFile($0, into: library) }
            _ = self.insertImported(imported, replacing: range)
        }
    }

    // MARK: - Formatting commands

    @objc func toggleBold(_ sender: Any?) {
        wrapSelection(prefix: "**", suffix: "**")
    }

    @objc func toggleItalic(_ sender: Any?) {
        wrapSelection(prefix: "*", suffix: "*")
    }

    @objc func wrapSelection(prefix: String, suffix: String) {
        guard let storage = textStorage else { return }
        let range = selectedRange()
        if range.length == 0 {
            storage.insert(NSAttributedString(string: "\(prefix)\(suffix)"), at: range.location)
            setSelectedRange(NSRange(location: range.location + prefix.count, length: 0))
        } else {
            let selectedText = (storage.string as NSString).substring(with: range)
            let replacement = NSAttributedString(string: "\(prefix)\(selectedText)\(suffix)")
            storage.replaceCharacters(in: range, with: replacement)
            setSelectedRange(NSRange(location: range.location, length: replacement.length))
        }
        didChangeText()
    }

    @objc func insertLink() {
        wrapSelection(prefix: "[", suffix: "](url)")
    }

    @objc func toggleTodoOnCurrentLine() {
        guard let storage = textStorage else { return }
        let cursor = selectedRange().location
        let lineRange = (storage.string as NSString).lineRange(for: NSRange(location: cursor, length: 0))
        let line = (storage.string as NSString).substring(with: lineRange)

        guard let regex = try? NSRegularExpression(pattern: #"\[([ xX])\]"#) else { return }
        let nsLine = line as NSString
        guard let match = regex.firstMatch(in: line, range: NSRange(location: 0, length: nsLine.length)) else { return }

        let checkboxChar = nsLine.substring(with: match.range(at: 1))
        let newChar = (checkboxChar == " ") ? "x" : " "
        let absRange = NSRange(location: lineRange.location + match.range(at: 1).location, length: 1)
        storage.replaceCharacters(in: absRange, with: newChar)
        didChangeText()
    }
}

/// Asks before creating the note a link names (design board 58 ⑥).
private struct CreateLinkedNoteView: View {
    let name: String
    let onCancel: () -> Void
    let onCreate: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("No note named “\(name)” yet.")
                .font(.fumi(.body))
                .foregroundColor(.secondaryText)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Spacer()
                Button("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Button("Create Note", action: onCreate)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(14)
        .frame(width: 280)
    }
}
