import AppKit

// Standalone test runner for CompositeFont. There is no Xcode test target in
// this project, and CompositeFont.swift depends only on AppKit, so it
// compiles directly with its source file:
//
//     swiftc Fumi/Views/Editor/CompositeFont.swift Tests/CompositeFontTests.swift -o /tmp/composite-font-tests
//     /tmp/composite-font-tests
//
// Exits non-zero if any assertion fails.
//
// What this covers: the composite-font pairing that fixes mixed CJK/Latin
// rendering when the editor body is a Latin-only family (Settings →
// Appearance → Editor Font). Pins the character classification (which
// characters are claimed by the CJK face), the family pairing (serif ↔ 宋体,
// sans ↔ 苹方, and the cases left on the native cascade), and the storage
// pass itself — including that collapsed Markdown syntax, monospaced code and
// attachments are never touched, weight carries over, and Han never gets a
// synthesised slant.
//
// The storage exercises below mirror WysiwygEngine.applyStyle exactly: every
// attribute mutation — including the composite pass — happens inside one
// begin/endEditing block, so NSTextStorage's font fixing (which substitutes
// fonts for characters they cannot render) is deferred to endEditing, just
// as it is during a real restyle. Assertions read the raw in-block state.

@main
enum CompositeFontTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message) — expected \(expected), got \(actual)\n".data(using: .utf8)!)
        }
    }

    static func expect(_ condition: Bool, _ message: String) {
        checks += 1
        if !condition {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message)\n".data(using: .utf8)!)
        }
    }

    static func main() {
        testCategoryClassification()
        testSerifDetection()
        testPairingResolution()
        testApplyToStorage()
        testPunctuationAttribution()
        testUntouchedRuns()
        testInactiveLeavesStorageAlone()
        testSurrogatePairs()

        if failures == 0 {
            print("CompositeFontTests: \(checks) checks passed")
        } else {
            print("CompositeFontTests: \(failures)/\(checks) checks FAILED")
            exit(1)
        }
    }

    // MARK: - Storage helpers

    static func familyFont(_ family: String, size: CGFloat = 17) -> NSFont {
        NSFont(descriptor: NSFontDescriptor(fontAttributes: [.family: family]), size: size)!
    }

    static var newYork: NSFont {
        NSFont.systemFont(ofSize: 17).fontDescriptor.withDesign(.serif).flatMap { NSFont(descriptor: $0, size: 17) }!
    }

    /// Mirrors one WysiwygEngine restyle: seed `text` with `font`, run the
    /// composite pass, then hand the raw (pre-font-fixing) storage to
    /// `assertions`, all inside a single begin/endEditing block.
    static func exercise(_ text: String, font: NSFont, _ composite: CompositeFont,
                         _ assertions: (NSTextStorage) -> Void) {
        let storage = NSTextStorage(string: text)
        storage.beginEditing()
        storage.addAttribute(.font, value: font, range: NSRange(location: 0, length: (text as NSString).length))
        composite.apply(to: storage)
        assertions(storage)
        storage.endEditing()
    }

    static func fontName(at index: Int, in storage: NSTextStorage) -> String {
        (storage.attribute(.font, at: index, effectiveRange: nil) as? NSFont)?.fontName ?? "<none>"
    }

    static func pointSize(at index: Int, in storage: NSTextStorage) -> CGFloat {
        (storage.attribute(.font, at: index, effectiveRange: nil) as? NSFont)?.pointSize ?? -1
    }

    // MARK: - Classification

    static func testCategoryClassification() {
        func scalar(_ string: String) -> Unicode.Scalar {
            string.unicodeScalars.first!
        }
        // Claimed by the paired CJK face.
        expect(CompositeFont.category(of: scalar("一")), .cjk, "Han ideograph")
        expect(CompositeFont.category(of: scalar("、")), .cjk, "CJK punctuation")
        expect(CompositeFont.category(of: scalar("あ")), .cjk, "Hiragana")
        expect(CompositeFont.category(of: scalar("ア")), .cjk, "Katakana")
        expect(CompositeFont.category(of: scalar("한")), .cjk, "Hangul syllable")
        expect(CompositeFont.category(of: scalar("，")), .cjk, "Fullwidth comma")
        expect(CompositeFont.category(of: scalar("１")), .cjk, "Fullwidth digit")
        expect(CompositeFont.category(of: scalar("￥")), .cjk, "Fullwidth yen")
        expect(CompositeFont.category(of: scalar("𠀀")), .cjk, "CJK ext-B ideograph")
        // General punctuation — attributed by context.
        expect(CompositeFont.category(of: scalar("“")), .punctuation, "Left double quote")
        expect(CompositeFont.category(of: scalar("”")), .punctuation, "Right double quote")
        expect(CompositeFont.category(of: scalar("’")), .punctuation, "Right single quote")
        expect(CompositeFont.category(of: scalar("—")), .punctuation, "Em dash")
        expect(CompositeFont.category(of: scalar("…")), .punctuation, "Ellipsis")
        expect(CompositeFont.category(of: scalar("€")), .punctuation, "Euro sign")
        // Everything else keeps the run's own face.
        expect(CompositeFont.category(of: scalar("a")), .other, "Latin letter")
        expect(CompositeFont.category(of: scalar("Z")), .other, "Capital letter")
        expect(CompositeFont.category(of: scalar("7")), .other, "Digit")
        expect(CompositeFont.category(of: scalar(" ")), .other, "Space")
        expect(CompositeFont.category(of: scalar("¥")), .other, "Halfwidth yen")
        expect(CompositeFont.category(of: scalar("→")), .other, "Arrow")
        expect(CompositeFont.category(of: scalar("🙂")), .other, "Emoji")
    }

    // MARK: - Serif detection

    static func testSerifDetection() {
        expect(CompositeFont.isSerifFamily("New York"), true, "New York is serif")
        expect(CompositeFont.isSerifFamily("Georgia"), true, "Georgia is serif")
        expect(CompositeFont.isSerifFamily("Times New Roman"), true, "Times New Roman is serif")
        expect(CompositeFont.isSerifFamily("Charter"), true, "Charter is serif")
        expect(CompositeFont.isSerifFamily("Source Serif 4"), true, "Source Serif is serif")
        expect(CompositeFont.isSerifFamily("Songti SC"), true, "Songti reads serif")
        expect(CompositeFont.isSerifFamily("Helvetica"), false, "Helvetica is sans")
        expect(CompositeFont.isSerifFamily("Avenir Next"), false, "Avenir is sans")
        expect(CompositeFont.isSerifFamily("PingFang SC"), false, "PingFang is sans")
        expect(CompositeFont.isSerifFamily("Inter"), false, "Inter is sans")
        expect(CompositeFont.isSerifFamily("SF Mono"), false, "SF Mono is not serif")
    }

    // MARK: - Pairing resolution

    static func testPairingResolution() {
        // Serif Latin pairs with 宋体, scaled to Apple's SF ↔ PingFang balance.
        let serif = CompositeFont(baseFont: newYork, familyName: "New York")
        expect(serif.isActive, true, "New York composites")
        expect(serif.cjkFamily, "Songti SC", "New York pairs with Songti SC")
        expect(serif.cjkScale, 0.97, "Songti scale")

        let georgia = CompositeFont(baseFont: familyFont("Georgia"), familyName: "Georgia")
        expect(georgia.cjkFamily, "Songti SC", "Georgia pairs with Songti SC")

        // Sans Latin pairs with 苹方, nudged down a hair.
        let helvetica = CompositeFont(baseFont: familyFont("Helvetica"), familyName: "Helvetica")
        expect(helvetica.isActive, true, "Helvetica composites")
        expect(helvetica.cjkFamily, "PingFang SC", "Helvetica pairs with PingFang SC")
        expect(helvetica.cjkScale, 0.97, "PingFang scale")

        // System faces keep Apple's tuned native cascade.
        let system = CompositeFont(baseFont: NSFont.systemFont(ofSize: 17), familyName: "System")
        expect(system.isActive, false, "System stays native")
        let mono = CompositeFont(baseFont: NSFont.monospacedSystemFont(ofSize: 17, weight: .regular), familyName: "SF Mono")
        expect(mono.isActive, false, "SF Mono stays native")

        // Families that already cover Han set both scripts themselves.
        let pingfang = CompositeFont(baseFont: familyFont("PingFang SC"), familyName: "PingFang SC")
        expect(pingfang.isActive, false, "PingFang SC covers Han itself")
        let songti = CompositeFont(baseFont: familyFont("Songti SC"), familyName: "Songti SC")
        expect(songti.isActive, false, "Songti SC covers Han itself")
    }

    // MARK: - Storage pass

    static func testApplyToStorage() {
        let composite = CompositeFont(baseFont: familyFont("Helvetica"), familyName: "Helvetica")

        exercise("中文abc", font: familyFont("Helvetica"), composite) { storage in
            // Han runs take the paired face at the scaled size.
            expect(fontName(at: 0, in: storage), "PingFangSC-Regular", "Han run face")
            expect(pointSize(at: 0, in: storage), 17 * 0.97, "Han run scaled size")
            // Latin keeps the user's chosen face untouched.
            expect(fontName(at: 3, in: storage), "Helvetica", "Latin run face")
            expect(pointSize(at: 3, in: storage), 17, "Latin run size")
        }

        // A run's weight carries over into the CJK face.
        let boldFont = NSFontManager.shared.convert(familyFont("Helvetica"), toHaveTrait: .boldFontMask)
        exercise("粗体", font: boldFont, composite) { storage in
            expect(fontName(at: 0, in: storage), "PingFangSC-Semibold", "Bold run → semibold CJK face")
        }

        // Italic runs get an UPRIGHT CJK face — never a synthesised slant.
        let italicFont = NSFontManager.shared.convert(familyFont("Helvetica"), toHaveTrait: .italicFontMask)
        exercise("斜体", font: italicFont, composite) { storage in
            expect(fontName(at: 0, in: storage), "PingFangSC-Regular", "Italic run → upright CJK face")
        }

        // A smaller style (hashtag size) scales with it.
        exercise("标签", font: familyFont("Helvetica", size: 14.5), composite) { storage in
            expect(pointSize(at: 0, in: storage), 14.5 * 0.97, "Smaller run scales too")
        }

        // Serif pairing: New York body, Songti SC for Han, scaled.
        let serifComposite = CompositeFont(baseFont: newYork, familyName: "New York")
        exercise("中文abc", font: newYork, serifComposite) { storage in
            expect(fontName(at: 0, in: storage), "STSongti-SC-Regular", "New York → Songti SC")
            expect(pointSize(at: 0, in: storage), 17 * 0.97, "Songti scaled size")
            expect(fontName(at: 3, in: storage), ".NewYork-Regular", "Latin keeps New York")
        }
    }

    static func testPunctuationAttribution() {
        let composite = CompositeFont(baseFont: familyFont("Helvetica"), familyName: "Helvetica")

        // A quote pair follows what it quotes, not what precedes it: Latin
        // quoted after Han keeps both quotes Latin…
        exercise("中“a”", font: familyFont("Helvetica"), composite) { storage in
            expect(fontName(at: 1, in: storage), "Helvetica", "Opening quote before Latin → Latin face")
            expect(fontName(at: 3, in: storage), "Helvetica", "Closing quote after Latin → Latin face")
        }

        // …and Han quoted after a space gets both quotes in the CJK face.
        exercise("和 “中文”", font: familyFont("Helvetica"), composite) { storage in
            expect(fontName(at: 2, in: storage), "PingFangSC-Regular", "Opening quote before Han → CJK face")
            expect(fontName(at: 5, in: storage), "PingFangSC-Regular", "Closing quote after Han → CJK face")
        }

        // Apostrophes stay Latin.
        exercise("don’t", font: familyFont("Helvetica"), composite) { storage in
            expect(fontName(at: 3, in: storage), "Helvetica", "Apostrophe in Latin → Latin face")
        }

        // A newline resets the attribution: the quote after 中\n opens a new
        // line next to nothing CJK.
        exercise("中\n“x”", font: familyFont("Helvetica"), composite) { storage in
            expect(fontName(at: 2, in: storage), "Helvetica", "Quote after newline → Latin face")
        }

        // Em dash between two Han characters belongs to the CJK face.
        exercise("中—文", font: familyFont("Helvetica"), composite) { storage in
            expect(fontName(at: 1, in: storage), "PingFangSC-Regular", "Em dash between Han → CJK face")
        }
    }

    static func testUntouchedRuns() {
        let composite = CompositeFont(baseFont: familyFont("Helvetica"), familyName: "Helvetica")

        // Collapsed Markdown syntax (0.1pt) keeps its zero-width font, so the
        // raw markers stay invisible and take no space.
        exercise("**", font: NSFont.systemFont(ofSize: 0.1), composite) { storage in
            expect(pointSize(at: 0, in: storage), 0.1, "Collapsed syntax font untouched")
        }

        // Monospaced runs (inline code, code blocks, raw source) are left to
        // the mono font's own cascade — the pass sees the raw mono font before
        // font fixing substitutes anything.
        let mono = NSFont.monospacedSystemFont(ofSize: 14, weight: .regular)
        exercise("代码 code", font: mono, composite) { storage in
            expect(fontName(at: 0, in: storage), ".AppleSystemUIFontMonospaced-Regular", "Mono run untouched (Han)")
            expect(pointSize(at: 0, in: storage), 14, "Mono run size untouched")
            expect(fontName(at: 3, in: storage), ".AppleSystemUIFontMonospaced-Regular", "Mono run untouched (Latin)")
        }

        // Attachment characters are skipped, not re-faced.
        let attachment = NSTextAttachment()
        let attachmentStorage = NSTextStorage(attributedString: NSAttributedString(attachment: attachment))
        attachmentStorage.beginEditing()
        attachmentStorage.addAttribute(.font, value: familyFont("Helvetica"), range: NSRange(location: 0, length: 1))
        composite.apply(to: attachmentStorage)
        expect(fontName(at: 0, in: attachmentStorage), "Helvetica", "Attachment run untouched")
        attachmentStorage.endEditing()
    }

    static func testInactiveLeavesStorageAlone() {
        // System font: the pairing is inactive and the storage is unchanged —
        // Apple's native cascade already pairs SF with PingFang SC.
        let system = CompositeFont(baseFont: NSFont.systemFont(ofSize: 17), familyName: "System")
        let systemName = NSFont.systemFont(ofSize: 17).fontName
        exercise("中文abc", font: NSFont.systemFont(ofSize: 17), system) { storage in
            expect(fontName(at: 0, in: storage), systemName, "System pairing is a no-op")
            expect(pointSize(at: 0, in: storage), 17, "System pairing keeps native size")
        }

        // Same for a family that covers Han itself.
        let pingfang = CompositeFont(baseFont: familyFont("PingFang SC"), familyName: "PingFang SC")
        exercise("中文abc", font: familyFont("PingFang SC"), pingfang) { storage in
            expect(fontName(at: 0, in: storage), "PingFangSC-Regular", "Self-covering family untouched")
            expect(pointSize(at: 0, in: storage), 17, "Self-covering family keeps its size")
        }
    }

    static func testSurrogatePairs() {
        // CJK ext-B characters are surrogate pairs in UTF-16; the pair must be
        // classified as one scalar, not two broken halves.
        let composite = CompositeFont(baseFont: familyFont("Helvetica"), familyName: "Helvetica")
        exercise("𠀀abc", font: familyFont("Helvetica"), composite) { storage in
            expect(fontName(at: 0, in: storage), "PingFangSC-Regular", "Ext-B pair → CJK face")
            expect(pointSize(at: 0, in: storage), 17 * 0.97, "Ext-B pair scaled")
            expect(fontName(at: 2, in: storage), "Helvetica", "Latin after ext-B keeps face")
        }
    }
}
