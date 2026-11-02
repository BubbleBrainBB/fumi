import Foundation

// Standalone test runner for the theme palettes. Compile and run:
//
//     swiftc Fumi/Utils/ThemePalette.swift Tests/ThemePaletteTests.swift -o /tmp/theme-palette-tests
//     /tmp/theme-palette-tests
//
// Checks that every theme, light and dark, keeps its text readable on both
// the page and the ground. Exits non-zero if any check fails.

@main
enum ThemePaletteTests {
    static var failures = 0
    static var checks = 0

    static func expect(_ ratio: Double, atLeast minimum: Double, _ message: String) {
        checks += 1
        if ratio < minimum {
            failures += 1
            FileHandle.standardError.write(String(format: "FAIL: %@ — %.2f < %.1f\n", message, ratio, minimum).data(using: .utf8)!)
        }
    }

    static func main() {
        for theme in AppTheme.allCases {
            for dark in [false, true] {
                let p = ThemePalette.palette(theme, dark: dark)
                let name = "\(theme.title) \(dark ? "dark" : "light")"
                for (surface, bg) in [("page", p.page), ("ground", p.ground), ("card", p.card)] {
                    expect(ThemePalette.contrast(p.text, bg), atLeast: 7, "\(name) text on \(surface)")
                    expect(ThemePalette.contrast(p.text2, bg), atLeast: 4.5, "\(name) text2 on \(surface)")
                    expect(ThemePalette.contrast(p.text3, bg), atLeast: 4.5, "\(name) text3 on \(surface)")
                }
                expect(ThemePalette.contrast(p.text, p.groundSelected), atLeast: 7, "\(name) text on selected row")
                expect(ThemePalette.contrast(p.text, p.codeBlock), atLeast: 7, "\(name) text on code block")
                expect(ThemePalette.contrast(p.text, p.highlight), atLeast: 4.5, "\(name) text on highlight")
            }
        }
        // Light and dark of a theme must actually differ.
        for theme in AppTheme.allCases {
            checks += 1
            if ThemePalette.palette(theme, dark: false) == ThemePalette.palette(theme, dark: true) {
                failures += 1
                FileHandle.standardError.write("FAIL: \(theme.title) light and dark are identical\n".data(using: .utf8)!)
            }
        }

        print("\(checks - failures)/\(checks) checks passed")
        exit(failures == 0 ? 0 : 1)
    }
}
