import AppKit

/// A dependency-free renderer for a practical subset of LaTeX math:
/// super/subscripts, `\frac`, `\sqrt`, Greek letters and common operators.
/// It is NOT a full TeX engine — deeply nested matrices/alignments degrade
/// gracefully (unknown commands render as their literal name).
enum LatexMath {

    // MARK: - Span detection

    struct MathSpan {
        let range: NSRange   // includes the `$`/`$$` delimiters
        let latex: String    // inner LaTeX (delimiters stripped)
        let display: Bool    // true for `$$…$$`
    }

    /// Finds `$$…$$` (display) and `$…$` (inline) spans, skipping fenced code
    /// blocks, ranges already claimed by tables, and escaped `\$`.
    static func mathSpans(in text: NSString, excludingTableRanges tableRanges: [NSRange]) -> [MathSpan] {
        let fenced = fencedLineRanges(in: text)
        func excluded(_ r: NSRange) -> Bool {
            let loc = r.location
            return fenced.contains { NSLocationInRange(loc, $0) }
                || tableRanges.contains { NSIntersectionRange($0, r).length > 0 }
        }

        var spans: [MathSpan] = []
        var claimed: [NSRange] = []
        func overlapsClaimed(_ r: NSRange) -> Bool {
            claimed.contains { NSIntersectionRange($0, r).length > 0 }
        }

        let full = NSRange(location: 0, length: text.length)

        // Display first so `$$` is not mis-read as two empty inline spans.
        if let display = try? NSRegularExpression(pattern: #"(?<!\\)\$\$([\s\S]+?)\$\$"#) {
            display.enumerateMatches(in: text as String, range: full) { m, _, _ in
                guard let m = m else { return }
                if excluded(m.range) || overlapsClaimed(m.range) { return }
                let inner = text.substring(with: m.range(at: 1))
                spans.append(MathSpan(range: m.range, latex: inner, display: true))
                claimed.append(m.range)
            }
        }

        // Inline: no spaces just inside the delimiters, no `$`/newline inside.
        if let inline = try? NSRegularExpression(pattern: #"(?<!\\)\$(?!\s)([^$\n]*?[^$\s\\])\$"#) {
            inline.enumerateMatches(in: text as String, range: full) { m, _, _ in
                guard let m = m else { return }
                if excluded(m.range) || overlapsClaimed(m.range) { return }
                let inner = text.substring(with: m.range(at: 1))
                spans.append(MathSpan(range: m.range, latex: inner, display: false))
                claimed.append(m.range)
            }
        }

        return spans.sorted { $0.range.location < $1.range.location }
    }

    private static func fencedLineRanges(in text: NSString) -> [NSRange] {
        var ranges: [NSRange] = []
        var fenceStart: Int?
        text.enumerateSubstrings(in: NSRange(location: 0, length: text.length), options: [.byLines]) { sub, range, _, _ in
            let t = (sub ?? "").trimmingCharacters(in: .whitespaces)
            guard t.hasPrefix("```") || t.hasPrefix("~~~") else { return }
            if let start = fenceStart {
                ranges.append(NSRange(location: start, length: NSMaxRange(range) - start))
                fenceStart = nil
            } else {
                fenceStart = range.location
            }
        }
        return ranges
    }

    // MARK: - Parsing

    indirect enum Base {
        case sym(String)
        case row([Atom])
        case frac([Atom], [Atom])
        case sqrt([Atom])
    }

    struct Atom {
        var base: Base
        var sup: [Atom]?
        var sub: [Atom]?
    }

    static func parse(_ latex: String) -> [Atom] {
        var chars = Array(latex)
        var i = 0
        return parseAtoms(&chars, &i, stopAtBrace: false)
    }

    private static func parseAtoms(_ s: inout [Character], _ i: inout Int, stopAtBrace: Bool) -> [Atom] {
        var atoms: [Atom] = []
        while i < s.count {
            let c = s[i]
            if c == "}" && stopAtBrace { break }
            if c == "^" || c == "_" {
                i += 1
                let script = parseScriptArgument(&s, &i)
                if atoms.isEmpty { atoms.append(Atom(base: .sym(""), sup: nil, sub: nil)) }
                if c == "^" { atoms[atoms.count - 1].sup = script }
                else { atoms[atoms.count - 1].sub = script }
                continue
            }
            atoms.append(Atom(base: parseBase(&s, &i), sup: nil, sub: nil))
        }
        return atoms
    }

    /// Argument of `^` or `_`: a braced group or a single base.
    private static func parseScriptArgument(_ s: inout [Character], _ i: inout Int) -> [Atom] {
        guard i < s.count else { return [] }
        if s[i] == "{" {
            i += 1
            let inner = parseAtoms(&s, &i, stopAtBrace: true)
            if i < s.count, s[i] == "}" { i += 1 }
            return inner
        }
        return [Atom(base: parseBase(&s, &i), sup: nil, sub: nil)]
    }

    private static func parseGroup(_ s: inout [Character], _ i: inout Int) -> [Atom] {
        guard i < s.count, s[i] == "{" else {
            if i < s.count { return [Atom(base: parseBase(&s, &i), sup: nil, sub: nil)] }
            return []
        }
        i += 1
        let inner = parseAtoms(&s, &i, stopAtBrace: true)
        if i < s.count, s[i] == "}" { i += 1 }
        return inner
    }

    private static func parseBase(_ s: inout [Character], _ i: inout Int) -> Base {
        let c = s[i]
        if c == "{" {
            return .row(parseGroup(&s, &i))
        }
        if c == "\\" {
            i += 1
            // Spacing / escaped punctuation.
            if i < s.count, !s[i].isLetter {
                let ch = s[i]; i += 1
                switch ch {
                case ",", ";", " ", "!", "quad".first!: return .sym(" ")
                case "{": return .sym("{")
                case "}": return .sym("}")
                case "$": return .sym("$")
                case "%": return .sym("%")
                case "\\": return .sym(" ")   // line break → space (inline)
                default: return .sym(String(ch))
                }
            }
            var name = ""
            while i < s.count, s[i].isLetter { name.append(s[i]); i += 1 }
            switch name {
            case "frac":
                let n = parseGroup(&s, &i)
                let d = parseGroup(&s, &i)
                return .frac(n, d)
            case "sqrt":
                if i < s.count, s[i] == "[" {   // skip optional index
                    while i < s.count, s[i] != "]" { i += 1 }
                    if i < s.count { i += 1 }
                }
                return .sqrt(parseGroup(&s, &i))
            case "left", "right":
                if i < s.count {
                    let delim = s[i]; i += 1
                    return .sym(delim == "." ? "" : String(delim))
                }
                return .sym("")
            case "quad", "qquad", "", ":": return .sym("  ")
            default:
                if let sym = symbols[name] { return .sym(sym) }
                return .sym(name)   // unknown command → its name (e.g. function names)
            }
        }
        // Skip raw source whitespace collapse: keep single spaces visible.
        i += 1
        return .sym(String(c))
    }

    static let symbols: [String: String] = [
        "alpha": "α", "beta": "β", "gamma": "γ", "delta": "δ", "epsilon": "ε", "varepsilon": "ε",
        "zeta": "ζ", "eta": "η", "theta": "θ", "vartheta": "ϑ", "iota": "ι", "kappa": "κ",
        "lambda": "λ", "mu": "μ", "nu": "ν", "xi": "ξ", "pi": "π", "varpi": "ϖ", "rho": "ρ",
        "sigma": "σ", "tau": "τ", "upsilon": "υ", "phi": "φ", "varphi": "φ", "chi": "χ",
        "psi": "ψ", "omega": "ω",
        "Gamma": "Γ", "Delta": "Δ", "Theta": "Θ", "Lambda": "Λ", "Xi": "Ξ", "Pi": "Π",
        "Sigma": "Σ", "Upsilon": "Υ", "Phi": "Φ", "Psi": "Ψ", "Omega": "Ω",
        "times": "×", "div": "÷", "pm": "±", "mp": "∓", "cdot": "·", "ast": "∗", "star": "⋆",
        "leq": "≤", "le": "≤", "geq": "≥", "ge": "≥", "neq": "≠", "ne": "≠", "approx": "≈",
        "equiv": "≡", "sim": "∼", "simeq": "≃", "cong": "≅", "propto": "∝", "ll": "≪", "gg": "≫",
        "infty": "∞", "partial": "∂", "nabla": "∇", "sum": "∑", "prod": "∏", "coprod": "∐",
        "int": "∫", "iint": "∬", "oint": "∮", "sqrt": "√",
        "rightarrow": "→", "to": "→", "leftarrow": "←", "gets": "←", "Rightarrow": "⇒",
        "Leftarrow": "⇐", "leftrightarrow": "↔", "Leftrightarrow": "⇔", "mapsto": "↦",
        "uparrow": "↑", "downarrow": "↓",
        "in": "∈", "notin": "∉", "ni": "∋", "subset": "⊂", "supset": "⊃", "subseteq": "⊆",
        "supseteq": "⊇", "cup": "∪", "cap": "∩", "setminus": "∖", "emptyset": "∅", "varnothing": "∅",
        "forall": "∀", "exists": "∃", "nexists": "∄", "neg": "¬", "land": "∧", "lor": "∨",
        "angle": "∠", "perp": "⊥", "parallel": "∥", "cdots": "⋯", "ldots": "…", "dots": "…",
        "vdots": "⋮", "ddots": "⋱", "prime": "′", "deg": "°", "circ": "∘", "bullet": "•",
        "oplus": "⊕", "otimes": "⊗", "wedge": "∧", "vee": "∨", "Re": "ℜ", "Im": "ℑ",
        "hbar": "ℏ", "ell": "ℓ", "aleph": "ℵ", "pm_": "±", "leftroot": "", "lfloor": "⌊",
        "rfloor": "⌋", "lceil": "⌈", "rceil": "⌉", "langle": "⟨", "rangle": "⟩",
        "mathbb": "", "mathcal": "", "mathrm": "", "left": "", "right": "",
    ]
}

// MARK: - Box-model layout

/// A laid-out math box: width plus distances above/below the baseline.
private final class MathBox {
    let width: CGFloat
    let ascent: CGFloat
    let descent: CGFloat
    /// Draw with the box's left edge at `x` and baseline at `baselineY` (flipped, y-down).
    let drawAt: (_ x: CGFloat, _ baselineY: CGFloat) -> Void
    var height: CGFloat { ascent + descent }

    init(width: CGFloat, ascent: CGFloat, descent: CGFloat, drawAt: @escaping (CGFloat, CGFloat) -> Void) {
        self.width = width; self.ascent = ascent; self.descent = descent; self.drawAt = drawAt
    }
}

enum MathRenderer {

    /// Renders LaTeX to an image. Returns the image and its descent (pixels
    /// below the baseline) so it can be aligned to the surrounding text baseline.
    static func render(latex: String, display: Bool, font: NSFont, color: NSColor, maxWidth: CGFloat) -> (image: NSImage, descent: CGFloat) {
        let atoms = LatexMath.parse(latex)
        let size = display ? font.pointSize + 2 : font.pointSize
        let baseFont = NSFont.systemFont(ofSize: size)
        let box = row(atoms, font: baseFont, color: color)

        let pad: CGFloat = 2
        let imgSize = NSSize(width: max(1, ceil(box.width) + pad * 2),
                             height: max(1, ceil(box.height) + pad * 2))
        let descent = box.descent + pad
        let image = NSImage(size: imgSize, flipped: true) { _ in
            box.drawAt(pad, box.ascent + pad)
            return true
        }
        return (image, descent)
    }

    private static func italicizedIfVariable(_ s: String, _ font: NSFont) -> NSFont {
        // Italicise single Latin letters (math variables); leave numbers,
        // operators and multi-char tokens upright.
        if s.count == 1, let ch = s.first, ch.isLetter, ch.isASCII {
            return NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
        }
        return font
    }

    private static func textBox(_ s: String, font: NSFont, color: NSColor) -> MathBox {
        let f = italicizedIfVariable(s, font)
        let attrs: [NSAttributedString.Key: Any] = [.font: f, .foregroundColor: color]
        let attr = NSAttributedString(string: s, attributes: attrs)
        let width = attr.size().width
        let ascent = f.ascender
        let descent = -f.descender
        return MathBox(width: width, ascent: ascent, descent: descent) { x, baseY in
            attr.draw(at: NSPoint(x: x, y: baseY - ascent))
        }
    }

    private static func hbox(_ boxes: [MathBox]) -> MathBox {
        guard !boxes.isEmpty else { return MathBox(width: 0, ascent: 0, descent: 0) { _, _ in } }
        let ascent = boxes.map(\.ascent).max() ?? 0
        let descent = boxes.map(\.descent).max() ?? 0
        let width = boxes.reduce(0) { $0 + $1.width }
        return MathBox(width: width, ascent: ascent, descent: descent) { x, baseY in
            var cx = x
            for b in boxes { b.drawAt(cx, baseY); cx += b.width }
        }
    }

    private static func row(_ atoms: [LatexMath.Atom], font: NSFont, color: NSColor) -> MathBox {
        hbox(atoms.map { atom($0, font: font, color: color) })
    }

    private static func atom(_ a: LatexMath.Atom, font: NSFont, color: NSColor) -> MathBox {
        let baseBox = base(a.base, font: font, color: color)
        guard a.sup != nil || a.sub != nil else { return baseBox }
        let scriptFont = NSFont.systemFont(ofSize: max(7, font.pointSize * 0.7))
        let sup = a.sup.map { row($0, font: scriptFont, color: color) }
        let sub = a.sub.map { row($0, font: scriptFont, color: color) }
        return script(base: baseBox, sup: sup, sub: sub, font: font)
    }

    private static func base(_ b: LatexMath.Base, font: NSFont, color: NSColor) -> MathBox {
        switch b {
        case .sym(let s): return textBox(s, font: font, color: color)
        case .row(let atoms): return row(atoms, font: font, color: color)
        case .frac(let n, let d):
            return frac(num: row(n, font: font, color: color), den: row(d, font: font, color: color), font: font, color: color)
        case .sqrt(let r):
            return sqrt(radicand: row(r, font: font, color: color), font: font, color: color)
        }
    }

    private static func script(base: MathBox, sup: MathBox?, sub: MathBox?, font: NSFont) -> MathBox {
        let supShift = base.ascent * 0.55
        let subShift = base.descent + font.pointSize * 0.1
        let scriptWidth = max(sup?.width ?? 0, sub?.width ?? 0)
        var ascent = base.ascent
        var descent = base.descent
        if let sup = sup { ascent = max(ascent, supShift + sup.ascent) }
        if let sub = sub { descent = max(descent, subShift + sub.descent) }
        return MathBox(width: base.width + scriptWidth, ascent: ascent, descent: descent) { x, baseY in
            base.drawAt(x, baseY)
            let sx = x + base.width
            if let sup = sup { sup.drawAt(sx, baseY - supShift) }
            if let sub = sub { sub.drawAt(sx, baseY + subShift) }
        }
    }

    private static func frac(num: MathBox, den: MathBox, font: NSFont, color: NSColor) -> MathBox {
        let gap: CGFloat = 3
        let bar: CGFloat = max(1, font.pointSize * 0.06)
        let axis = font.xHeight * 0.5            // fraction centre sits on the math axis
        let width = max(num.width, den.width) + 6
        let ascent = axis + bar / 2 + gap + num.height
        let descent = den.height + gap + bar / 2 - axis
        return MathBox(width: width, ascent: ascent, descent: descent) { x, baseY in
            let barY = baseY - axis
            color.setStroke()
            let p = NSBezierPath()
            p.lineWidth = bar
            p.move(to: NSPoint(x: x + 1, y: barY))
            p.line(to: NSPoint(x: x + width - 1, y: barY))
            p.stroke()
            num.drawAt(x + (width - num.width) / 2, barY - bar / 2 - gap - num.descent)
            den.drawAt(x + (width - den.width) / 2, barY + bar / 2 + gap + den.ascent)
        }
    }

    private static func sqrt(radicand: MathBox, font: NSFont, color: NSColor) -> MathBox {
        let gapTop: CGFloat = max(2, font.pointSize * 0.12)
        let bar: CGFloat = max(1, font.pointSize * 0.06)
        let signWidth = max(8, radicand.height * 0.55)
        let ascent = radicand.ascent + gapTop + bar
        let descent = radicand.descent
        let width = signWidth + radicand.width + 3
        return MathBox(width: width, ascent: ascent, descent: descent) { x, baseY in
            let top = baseY - ascent + bar / 2
            let bottom = baseY + descent
            color.setStroke()
            let p = NSBezierPath()
            p.lineWidth = bar
            // Radical check mark.
            p.move(to: NSPoint(x: x, y: bottom - radicand.height * 0.35))
            p.line(to: NSPoint(x: x + signWidth * 0.35, y: bottom))
            p.line(to: NSPoint(x: x + signWidth * 0.7, y: top))
            // Overline across the radicand.
            p.line(to: NSPoint(x: x + width, y: top))
            p.stroke()
            radicand.drawAt(x + signWidth + 1, baseY)
        }
    }
}

/// A non-editable attachment that renders a LaTeX span. Double-/clicking it (see
/// `WysiwygTextView`) reveals the `$…$` source for editing.
final class MathAttachment: NSTextAttachment, MarkdownBackedAttachment {
    let markdownSource: String

    init(latex: String, display: Bool, markdownSource: String, font: NSFont, color: NSColor, maxWidth: CGFloat) {
        self.markdownSource = markdownSource
        super.init(data: nil, ofType: nil)
        let (rendered, descent) = MathRenderer.render(latex: latex, display: display, font: font, color: color, maxWidth: maxWidth)
        image = rendered
        // Align the formula's baseline with the surrounding text baseline.
        bounds = NSRect(x: 0, y: -descent, width: rendered.size.width, height: rendered.size.height)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
}
