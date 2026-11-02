import AppKit

/// Composite (中西文复合) font — the native port of the css-composite-font
/// technique (https://github.com/charlie5188/css-composite-font).
///
/// On the web, one `font-family` gets several `@font-face` rules, each
/// claiming a `unicode-range`; the browser hands every character to the font
/// that claims it, and `size-adjust` equalises the apparent size across
/// scripts. `NSTextStorage` has no such per-character dispatch, so this type
/// plays that role at restyle time: Han / kana / hangul runs are re-set in a
/// paired CJK family, scaled to sit visually even beside the Latin face,
/// while every other character keeps the face the editor already chose.
///
/// Why it exists here: the editor body can be ANY installed family
/// (Settings → Appearance → Editor Font). A Latin-only family — New York,
/// Charter, Helvetica, anything downloaded — leaves Han characters to the
/// system cascade, which always answers with the sans 苹方. Serif Latin
/// beside sans Han is exactly the mismatch the composite font removes.
///
/// Scope: "System" and "SF Mono" stay on the native cascade — Apple already
/// ships a tuned SF ↔ PingFang SC pairing through it. Families that cover
/// Han themselves (PingFang SC, Songti SC, Noto Sans CJK…) set both scripts
/// already and are likewise left untouched.
struct CompositeFont {

    // MARK: - Character categories

    /// Sorted the way the CSS original sorts its `unicode-range` claims: the
    /// CJK claim, the context-dependent punctuation claim, and everything
    /// else falling through to the run's own face.
    enum Category {
        /// Han, kana, hangul, fullwidth forms, CJK punctuation — rendered by
        /// the paired CJK face.
        case cjk
        /// General punctuation (curly quotes, dashes, ellipsis, currency):
        /// follows the preceding character, so “引文” takes the CJK face
        /// while "latin" quotes stay with the Latin face.
        case punctuation
        /// Everything else — keeps the run's own face.
        case other
    }

    /// `一` — the probe character for "does this family cover Han".
    private static let hanProbe = Unicode.Scalar(0x4E00)!

    // MARK: - Pairing

    /// CJK partner candidates for a sans Latin family, most preferred first.
    private static let sansPartners = ["PingFang SC", "Hiragino Sans GB"]
    /// …and for a serif family. Songti SC (宋体) is the serif answer to New
    /// York; STSong covers older systems, Songti TC as a last resort.
    private static let serifPartners = ["Songti SC", "STSong", "Songti TC"]

    /// Apparent-size ratio of the CJK face relative to the run's own point
    /// size — the native stand-in for CSS `size-adjust`, tuned so a mixed
    /// line reads as one texture. The reference is Apple's own SF ↔ PingFang
    /// pairing, where a Han glyph's ink is 1.23× the Latin cap height.
    /// Songti SC beside New York measures 1.27× at 1:1, so it comes down a
    /// touch to match. These two constants are the first thing to adjust if
    /// a pairing feels off.
    static let scaleWithSerifPartner: CGFloat = 0.97
    static let scaleWithSansPartner: CGFloat = 0.97

    /// Family names (or fragments) that read as serif and therefore pair
    /// with 宋体 rather than 苹方. Covers the macOS system serifs plus the
    /// common Google Fonts / designer serif names; unknown families pair
    /// with the sans 苹方, the safer default.
    private static let serifHints = [
        "new york", "times", "georgia", "charter", "palatino", "baskerville",
        "didot", "hoefler", "iowan", "garamond", "caslon", "minion", "athelas",
        "serif", "song", "ming", "宋", "slab",
        "literata", "lora", "merriweather", "playfair", "crimson", "spectral",
        "newsreader", "gelasio", "tinos", "bitter", "domine", "alegreya",
        "cormorant", "vollkorn", "arvo",
    ]

    static func isSerifFamily(_ family: String) -> Bool {
        let name = family.lowercased()
        return serifHints.contains { name.contains($0) }
    }

    // MARK: - State

    /// When false, `apply(to:)` is a no-op.
    let isActive: Bool
    /// The paired CJK family, e.g. "Songti SC". Nil when inactive.
    let cjkFamily: String?
    /// The `size-adjust` equivalent applied to the paired face.
    let cjkScale: CGFloat

    /// Build the pairing for the editor's chosen body family. `baseFont` is
    /// that family resolved at any size — only its coverage is inspected.
    init(baseFont: NSFont, familyName: String) {
        switch familyName {
        case "", "System", "SF Mono":
            // Apple's tuned SF ↔ PingFang pairing already rides the native
            // font cascade; compositing on top would only second-guess it.
            self.init(isActive: false, cjkFamily: nil, cjkScale: 1)
            return
        default:
            break
        }
        // A family that covers Han (PingFang SC, Songti SC, Noto Sans CJK…)
        // renders both scripts itself.
        guard !baseFont.coveredCharacterSet.contains(Self.hanProbe) else {
            self.init(isActive: false, cjkFamily: nil, cjkScale: 1)
            return
        }
        let serif = Self.isSerifFamily(familyName)
        let partner = Self.firstInstalled(serif ? Self.serifPartners : Self.sansPartners)
        self.init(
            isActive: partner != nil,
            cjkFamily: partner,
            cjkScale: serif ? Self.scaleWithSerifPartner : Self.scaleWithSansPartner
        )
    }

    private init(isActive: Bool, cjkFamily: String?, cjkScale: CGFloat) {
        self.isActive = isActive
        self.cjkFamily = cjkFamily
        self.cjkScale = cjkScale
    }

    private static func firstInstalled(_ families: [String]) -> String? {
        let installed = Set(NSFontManager.shared.availableFontFamilies)
        return families.first { installed.contains($0) }
    }

    // MARK: - Classification

    static func category(of scalar: Unicode.Scalar) -> Category {
        switch scalar.value {
        case 0x2E80...0x2EFF, 0x2F00...0x2FDF,                  // CJK radicals
             0x3000...0x303F,                                    // 、。〈〉《》…
             0x3040...0x30FF,                                    // かな・カタ
             0x3100...0x312F, 0x31A0...0x31BF,                  // Bopomofo
             0x3130...0x318F,                                    // Hangul jamo
             0x31F0...0x31FF,                                    // Katakana phonetic ext
             0x3400...0x4DBF, 0x4E00...0x9FFF,                  // CJK ideographs
             0xA960...0xA97F, 0xAC00...0xD7AF, 0xD7B0...0xD7FF, // Hangul
             0xF900...0xFAFF,                                    // Compatibility ideographs
             0xFE30...0xFE4F,                                    // Vertical CJK forms
             0xFF01...0xFFEE,                                    // Fullwidth + halfwidth forms
             0x20000...0x2FA1F, 0x30000...0x3134F:              // CJK ext B–I, G–H
            return .cjk
        case 0x2010...0x2027,                                    // – — “ ” ‘ ’ • …
             0x2030...0x205E,                                    // ‰ ※ ⁄
             0x2070...0x209F,                                    // ⁰ ½ sub/superscripts
             0x20A0...0x20CF:                                    // ¥ $ € £
            return .punctuation
        default:
            return .other
        }
    }

    // MARK: - Face resolution

    /// The face that should render `scalar` inside a run styled with `font`,
    /// or nil to keep the run's own face. `previous` is the preceding scalar
    /// in document order (nil at the start), used for punctuation
    /// attribution.
    func face(for scalar: Unicode.Scalar, after previous: Unicode.Scalar?, before next: Unicode.Scalar? = nil,
              matching font: NSFont) -> NSFont? {
        Self.takesCJKFace(scalar, after: previous, before: { next }) ? cjkFace(matching: font) : nil
    }

    /// Opening quotes, which belong to the text they open.
    private static let openingQuotes: Set<UInt32> = [0x2018, 0x201C]  // ‘ “

    /// Whether `scalar` is rendered by the paired CJK face. `next` is only
    /// consulted for opening quotes.
    private static func takesCJKFace(_ scalar: Unicode.Scalar, after previous: Unicode.Scalar?,
                                     before next: () -> Unicode.Scalar?) -> Bool {
        switch category(of: scalar) {
        case .cjk:
            return true
        case .punctuation:
            // A quote belongs to what it quotes: an opening quote follows
            // the character after it, so “中文” and “English” each get a
            // matching pair whatever precedes them. Closing quotes, dashes
            // and the rest follow the character before them.
            let neighbor = openingQuotes.contains(scalar.value) ? next() : previous
            return neighbor.map { category(of: $0) == .cjk } ?? false
        case .other:
            return false
        }
    }

    /// The first scalar at or after `index` that is visible — collapsed
    /// Markdown syntax (`**`, `[`…) sits between a quote and its text.
    private static func visibleScalar(from index: Int, in storage: NSTextStorage) -> Unicode.Scalar? {
        let string = storage.string as NSString
        var index = index
        while index < string.length {
            var run = NSRange()
            if let font = storage.attribute(.font, at: index, effectiveRange: &run) as? NSFont, font.pointSize < 1 {
                index = NSMaxRange(run)
                continue
            }
            let unit = string.character(at: index)
            if unit >= 0xD800, unit <= 0xDBFF, index + 1 < string.length {
                let low = string.character(at: index + 1)
                return Unicode.Scalar(0x10000 + (UInt32(unit - 0xD800) << 10) + UInt32(low &- 0xDC00))
            }
            return Unicode.Scalar(UInt32(unit))
        }
        return nil
    }

    /// The paired CJK face for a run, carrying over its weight and — like the
    /// CSS original, which feeds Han an upright file to stop the browser
    /// synthesising a slanted 汉字 — deliberately NOT its slant: neither
    /// paired family has an italic face, and a synthesised slant on Han is
    /// the exact failure mode the repo warns about.
    private func cjkFace(matching font: NSFont) -> NSFont? {
        guard let family = cjkFamily else { return nil }
        let size = font.pointSize * cjkScale
        // NSFontManager's 0–15 weight scale (5 = regular, 8 = semibold,
        // 9 = bold) → the font descriptor's −1…1 weight scale.
        let raw = CGFloat(NSFontManager.shared.weight(of: font))
        let weight = min(1, max(-1, (raw - 5) / 10))
        let descriptor = NSFontDescriptor(fontAttributes: [
            .family: family,
            .traits: [NSFontDescriptor.TraitKey.weight: weight],
        ])
        if let face = NSFont(descriptor: descriptor, size: size),
           face.coveredCharacterSet.contains(Self.hanProbe) {
            return face
        }
        // Descriptor matching can silently land on a fallback face; retry
        // through the font manager, bolded when the source run reads bold.
        let regular = NSFont(descriptor: NSFontDescriptor(fontAttributes: [.family: family]), size: size)
            ?? NSFont.systemFont(ofSize: size)
        return raw >= 7 ? NSFontManager.shared.convert(regular, toHaveTrait: .boldFontMask) : regular
    }

    // MARK: - Application

    /// Re-sets CJK runs of `storage` in the paired face.
    ///
    /// Every run keeps its own size, weight and color — only the family (and
    /// its scale) changes, per character. Runs that are hidden (collapsed
    /// Markdown syntax, set at 0.1pt), monospaced (inline code, code blocks,
    /// raw source) or belong to an attachment are left completely alone.
    ///
    /// Call this after all other font attributes are in place — the run's
    /// final font is the input the pairing scales from.
    func apply(to storage: NSTextStorage) {
        guard isActive, cjkFamily != nil, storage.length > 0 else { return }
        let string = storage.string as NSString
        let fullRange = NSRange(location: 0, length: string.length)

        // Punctuation attribution looks across run boundaries, so the
        // previous scalar lives outside the enumeration.
        var previousScalar: Unicode.Scalar?
        // Collected first, applied after: mutating the very attribute being
        // enumerated mid-flight would corrupt the walk.
        var changes: [(range: NSRange, font: NSFont)] = []

        storage.enumerateAttribute(.font, in: fullRange, options: []) { font, runRange, _ in
            guard let font = font as? NSFont else { return }
            if font.pointSize < 1 || font.isFixedPitch { return }  // hidden syntax / code
            if storage.attribute(.attachment, at: runRange.location, effectiveRange: nil) != nil { return }

            var index = runRange.location
            let end = NSMaxRange(runRange)
            // Consecutive characters resolving to the same face are set in
            // one attribute call — a paragraph costs a handful of calls, not
            // one per character.
            var groupStart = index
            var groupFont: NSFont?
            // The run has one font, so its CJK partner is resolved once, on
            // the first CJK character — building it per character made a
            // restyle of a long note several times slower.
            var runCJKFace: NSFont??

            func flush(upTo stop: Int) {
                guard stop > groupStart, let groupFont else { return }
                changes.append((NSRange(location: groupStart, length: stop - groupStart), groupFont))
            }

            while index < end {
                let unit = string.character(at: index)
                var scalar = Unicode.Scalar(UInt32(unit)) ?? Unicode.Scalar(0xFFFD)!
                var width = 1
                if unit >= 0xD800, unit <= 0xDBFF, index + 1 < end {
                    let low = string.character(at: index + 1)
                    if low >= 0xDC00, low <= 0xDFFF {
                        // Surrogate pair (emoji, CJK ext-B): classify the
                        // combined scalar, not its halves.
                        scalar = Unicode.Scalar(0x10000 + (UInt32(unit - 0xD800) << 10) + UInt32(low - 0xDC00))!
                        width = 2
                    }
                }
                var face: NSFont?
                let nextIndex = index + width
                if Self.takesCJKFace(scalar, after: previousScalar,
                                     before: { Self.visibleScalar(from: nextIndex, in: storage) }) {
                    if runCJKFace == nil { runCJKFace = .some(self.cjkFace(matching: font)) }
                    face = runCJKFace ?? nil
                }
                if face !== groupFont {
                    flush(upTo: index)
                    groupStart = index
                    groupFont = face
                }
                previousScalar = scalar
                index += width
            }
            flush(upTo: end)
        }

        for change in changes {
            storage.addAttribute(.font, value: change.font, range: change.range)
        }
    }
}
