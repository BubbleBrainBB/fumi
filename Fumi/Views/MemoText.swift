import AppKit
import SwiftUI

/// Memo type, set like the editor's: the editor's font, with Chinese in its
/// paired CJK face (CompositeFont). Left to the system's fallback, Chinese
/// beside a Latin serif is scaled up ~10% and its full-width punctuation
/// opens wide gaps.
enum MemoTypography {
    static func font(size: CGFloat) -> NSFont {
        WysiwygEngine.resolveFont(name: AppSettings.editorFont, size: size)
    }

    /// The italic of the editor's font (dates and times beside memos).
    static func italic(size: CGFloat) -> NSFont {
        NSFontManager.shared.convert(font(size: size), toHaveTrait: .italicFontMask)
    }

    /// Sets the paired CJK face on the Chinese of `text`, and bold in the
    /// editor's bold color.
    static func styleRuns(_ text: inout AttributedString, size: CGFloat) {
        let family = AppSettings.editorFont
        let base = font(size: size)
        let bold = WysiwygEngine.resolveFont(name: family, size: size, weight: .bold)
        let strong = Color(nsColor: MarkdownPalette.strong(accentHex: AppSettings.accentColorHex))
        let composite = CompositeFont(baseFont: base, familyName: family)

        let indices = Array(text.characters.indices)
        let scalars = text.characters.map { $0.unicodeScalars.first ?? " " }
        for (i, index) in indices.enumerated() {
            let range = index..<text.characters.index(after: index)
            let isBold = text[range].inlinePresentationIntent?.contains(.stronglyEmphasized) ?? false
            if isBold { text[range].foregroundColor = strong }
            guard composite.isActive,
                  let face = composite.face(for: scalars[i], after: i > 0 ? scalars[i - 1] : nil,
                                            before: i + 1 < scalars.count ? scalars[i + 1] : nil,
                                            matching: isBold ? bold : base) else { continue }
            text[range].font = Font(face)
        }
    }
}

/// A plain-text editor for memos that grows with its text: an NSTextView
/// (SwiftUI's TextEditor can't set a font per character), styled like the
/// editor on every change.
struct MemoTextEditor: NSViewRepresentable {
    @Binding var text: String
    var size = TextSize.emphasis.rawValue
    var placeholder: String = ""
    var minHeight: CGFloat = 44
    /// Bump to move keyboard focus into the editor.
    var focusRequest: Int = 0
    /// ⌘↩ while typing.
    var onSubmit: (() -> Void)? = nil
    /// esc while typing.
    var onCancel: (() -> Void)? = nil

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> MemoNSTextView {
        let view = MemoNSTextView()
        view.isRichText = false
        view.importsGraphics = false
        view.drawsBackground = false
        view.allowsUndo = true
        view.isAutomaticQuoteSubstitutionEnabled = AppSettings.smartPunctuation
        view.isAutomaticDashSubstitutionEnabled = AppSettings.smartPunctuation
        view.isContinuousSpellCheckingEnabled = AppSettings.spellCheck
        view.textContainerInset = .zero
        view.textContainer?.lineFragmentPadding = 0
        view.textContainer?.widthTracksTextView = true
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = false
        view.insertionPointColor = ThemeManager.shared.accentNSColor
        view.delegate = context.coordinator
        view.onSubmit = onSubmit
        view.onCancel = onCancel
        view.string = text
        context.coordinator.restyle(view)
        return view
    }

    func updateNSView(_ view: MemoNSTextView, context: Context) {
        context.coordinator.parent = self
        view.placeholder = placeholder
        view.onSubmit = onSubmit
        view.onCancel = onCancel
        if view.string != text {
            view.string = text
            context.coordinator.restyle(view)
        }
        if focusRequest != context.coordinator.focusedRequest {
            context.coordinator.focusedRequest = focusRequest
            view.requestFocus()
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView view: MemoNSTextView, context: Context) -> CGSize? {
        let width = proposal.width ?? 400
        guard let container = view.textContainer, let layout = view.layoutManager else { return nil }
        container.containerSize = NSSize(width: width, height: .greatestFiniteMagnitude)
        layout.ensureLayout(for: container)
        let used = layout.usedRect(for: container).height
        return CGSize(width: width, height: max(minHeight, ceil(used) + 2))
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: MemoTextEditor
        var focusedRequest = 0

        init(_ parent: MemoTextEditor) { self.parent = parent }

        func textDidChange(_ notification: Notification) {
            guard let view = notification.object as? MemoNSTextView else { return }
            restyle(view)
            parent.text = view.string
        }

        func restyle(_ view: MemoNSTextView) {
            let font = MemoTypography.font(size: parent.size)
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineSpacing = 6
            let attributes: [NSAttributedString.Key: Any] = [
                .font: font, .foregroundColor: NSColor.themed(\.text), .paragraphStyle: paragraph,
            ]
            view.placeholder = parent.placeholder
            view.placeholderFont = font
            view.typingAttributes = attributes
            guard let storage = view.textStorage else { return }
            storage.beginEditing()
            storage.setAttributes(attributes, range: NSRange(location: 0, length: storage.length))
            MemoTypography.highlight(storage, size: parent.size)
            CompositeFont(baseFont: font, familyName: AppSettings.editorFont).apply(to: storage)
            storage.endEditing()
            view.needsDisplay = true
        }
    }
}

extension MemoTypography {
    /// Colors the Markdown in a memo being written, the syntax left in
    /// place but quieter (MemoMarkdown.spans).
    static func highlight(_ storage: NSTextStorage, size: CGFloat) {
        let family = AppSettings.editorFont
        let base = font(size: size)
        let mono = NSFont.monospacedSystemFont(ofSize: WysiwygEngine.smallSize(size), weight: .regular)
        let struck: [NSAttributedString.Key: Any] = [
            .strikethroughStyle: NSUnderlineStyle.single.rawValue,
            .strikethroughColor: MarkdownPalette.faint,
        ]
        for span in MemoMarkdown.spans(in: storage.string) {
            let range = span.range
            guard NSMaxRange(range) <= storage.length else { continue }
            switch span.kind {
            case .marker:
                storage.addAttribute(.foregroundColor, value: MarkdownPalette.marker, range: range)
            case .strong:
                storage.addAttributes([
                    .font: WysiwygEngine.resolveFont(name: family, size: size, weight: .bold),
                    .foregroundColor: MarkdownPalette.strong(accentHex: AppSettings.accentColorHex),
                ], range: range)
            case .emphasis:
                // Spans come strong first, so italic keeps an enclosing bold.
                storage.enumerateAttribute(.font, in: range) { value, run, _ in
                    let font = (value as? NSFont) ?? base
                    storage.addAttribute(.font, value: NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask),
                                         range: run)
                }
            case .strike:
                storage.addAttributes(struck, range: range)
            case .code:
                storage.addAttributes([.font: mono, .foregroundColor: MarkdownPalette.inlineCode,
                                       .backgroundColor: MarkdownPalette.inlineBg], range: range)
            case .link, .tag:
                storage.addAttribute(.foregroundColor, value: ThemeManager.shared.accentNSColor, range: range)
            case .quote:
                storage.addAttribute(.foregroundColor, value: MarkdownPalette.secondary, range: range)
            case .codeBlock:
                storage.addAttribute(.font, value: mono, range: range)
            case .done:
                storage.addAttributes(struck, range: range)
                storage.addAttribute(.foregroundColor, value: MarkdownPalette.tertiary, range: range)
            }
        }
    }
}

final class MemoNSTextView: NSTextView {
    var placeholder = ""
    var placeholderFont = NSFont.systemFont(ofSize: TextSize.emphasis.rawValue)
    var onSubmit: (() -> Void)?
    var onCancel: (() -> Void)?
    private var wantsFocus = false

    override func keyDown(with event: NSEvent) {
        let isReturn = event.keyCode == 36 || event.keyCode == 76
        if isReturn, event.modifierFlags.contains(.command), let onSubmit {
            onSubmit()
            return
        }
        super.keyDown(with: event)
    }

    override func cancelOperation(_ sender: Any?) {
        if let onCancel { onCancel() } else { super.cancelOperation(sender) }
    }

    /// Focus now, or as soon as the view is in a window (a new view isn't yet).
    func requestFocus() {
        wantsFocus = true
        DispatchQueue.main.async { [weak self] in self?.takeFocusIfWanted() }
    }

    private func takeFocusIfWanted() {
        guard wantsFocus, let window else { return }
        wantsFocus = false
        window.makeFirstResponder(self)
        setSelectedRange(NSRange(location: (string as NSString).length, length: 0))
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        DispatchQueue.main.async { [weak self] in self?.takeFocusIfWanted() }
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard string.isEmpty, !placeholder.isEmpty else { return }
        (placeholder as NSString).draw(at: textContainerOrigin, withAttributes: [
            .font: placeholderFont, .foregroundColor: NSColor.themed(\.faintText),
        ])
    }

    override func didChangeText() {
        super.didChangeText()
        needsDisplay = true
    }
}
