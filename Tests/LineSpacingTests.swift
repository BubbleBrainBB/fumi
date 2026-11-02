import Foundation

// Standalone test runner for LineSpacingPreset. There is no Xcode test target
// in this project, and LineSpacingPreset (defined in AppSettings.swift) has no
// AppKit dependency, so it compiles directly with its source file:
//
//     swiftc Fumi/Models/AppSettings.swift Tests/LineSpacingTests.swift -o /tmp/linespacing-tests
//     /tmp/linespacing-tests
//
// Exits non-zero if any assertion fails.
//
// What this covers: the user-facing "Line Spacing" setting (Compact / Cozy /
// Relaxed) maps to a line-height multiplier, which the editor's paragraph style
// turns into `lineSpacing = fontSize * (multiplier - 1.0)`. These tests pin the
// preset → multiplier mapping and the reverse resolution so the Settings UI and
// the editor always agree on what each option means.

@main
enum LineSpacingTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message) — expected \(expected), got \(actual)\n".data(using: .utf8)!)
        }
    }

    static func main() {
        testPresetMultipliers()
        testLabelsAndRawValues()
        testFromMultiplierExactRoundTrip()
        testFromMultiplierClosest()
        testAllCasesOrder()

        if failures == 0 {
            print("OK — all \(checks) LineSpacing checks passed")
            exit(0)
        } else {
            FileHandle.standardError.write("\(failures)/\(checks) LineSpacing checks FAILED\n".data(using: .utf8)!)
            exit(1)
        }
    }

    // MARK: - preset → multiplier

    static func testPresetMultipliers() {
        // Each preset must map to a distinct, increasing multiplier. These are
        // the exact values the editor renders; drifting one silently changes
        // every note's vertical rhythm.
        expect(LineSpacingPreset.compact.multiplier, 1.4, "compact multiplier")
        expect(LineSpacingPreset.cozy.multiplier, 1.7, "cozy multiplier")
        expect(LineSpacingPreset.relaxed.multiplier, 2.0, "relaxed multiplier")

        // Strictly increasing: compact < cozy < relaxed.
        expect(LineSpacingPreset.compact.multiplier < LineSpacingPreset.cozy.multiplier, true,
               "compact < cozy")
        expect(LineSpacingPreset.cozy.multiplier < LineSpacingPreset.relaxed.multiplier, true,
               "cozy < relaxed")
    }

    // MARK: - label / rawValue

    static func testLabelsAndRawValues() {
        expect(LineSpacingPreset.compact.label, "Compact", "compact label")
        expect(LineSpacingPreset.cozy.label, "Cozy", "cozy label")
        expect(LineSpacingPreset.relaxed.label, "Relaxed", "relaxed label")

        // rawValue is what the Settings segmented control persists/selects on.
        expect(LineSpacingPreset.compact.rawValue, "compact", "compact rawValue")
        expect(LineSpacingPreset.cozy.rawValue, "cozy", "cozy rawValue")
        expect(LineSpacingPreset.relaxed.rawValue, "relaxed", "relaxed rawValue")

        // rawValue must round-trip back through the initializer — the Settings
        // view relies on `LineSpacingPreset(rawValue:)` to decode the selection.
        expect(LineSpacingPreset(rawValue: "compact"), .compact, "rawValue compact round-trip")
        expect(LineSpacingPreset(rawValue: "cozy"), .cozy, "rawValue cozy round-trip")
        expect(LineSpacingPreset(rawValue: "relaxed"), .relaxed, "rawValue relaxed round-trip")
        expect(LineSpacingPreset(rawValue: "bogus"), nil, "unknown rawValue → nil")
    }

    // MARK: - from(multiplier:) exact round-trip

    static func testFromMultiplierExactRoundTrip() {
        // A multiplier that exactly matches a preset resolves to that preset.
        // This is the path taken on app launch: the stored Double (written from
        // a preset) is decoded back into the segmented control's selection.
        expect(LineSpacingPreset.from(multiplier: 1.4), .compact, "1.4 → compact")
        expect(LineSpacingPreset.from(multiplier: 1.7), .cozy, "1.7 → cozy")
        expect(LineSpacingPreset.from(multiplier: 2.0), .relaxed, "2.0 → relaxed")
    }

    // MARK: - from(multiplier:) closest match

    static func testFromMultiplierClosest() {
        // A multiplier between two presets resolves to the nearer one. This
        // guards against a hand-edited / migrated UserDefaults value landing on
        // the wrong segment.
        // Values chosen clearly off each preset-pair midpoint so the result is
        // unambiguous regardless of floating-point rounding.
        expect(LineSpacingPreset.from(multiplier: 1.5), .compact, "1.5 nearer compact")
        expect(LineSpacingPreset.from(multiplier: 1.6), .cozy, "1.6 nearer cozy")
        expect(LineSpacingPreset.from(multiplier: 1.8), .cozy, "1.8 nearer cozy")
        expect(LineSpacingPreset.from(multiplier: 1.9), .relaxed, "1.9 nearer relaxed")

        // Out-of-range values clamp to the nearest end preset rather than
        // crashing or returning the default.
        expect(LineSpacingPreset.from(multiplier: 1.0), .compact, "1.0 below range → compact")
        expect(LineSpacingPreset.from(multiplier: 3.0), .relaxed, "3.0 above range → relaxed")
    }

    // MARK: - allCases

    static func testAllCasesOrder() {
        // The Settings segmented control renders `allCases` in order; the order
        // is also what makes "Compact / Cozy / Relaxed" read left-to-right as
        // increasing spacing.
        expect(LineSpacingPreset.allCases, [.compact, .cozy, .relaxed], "allCases order")
    }
}
