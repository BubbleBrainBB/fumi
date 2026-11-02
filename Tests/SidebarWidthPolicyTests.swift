import Foundation

// Standalone test runner for SidebarWidthPolicy. No Xcode test
// target exists; the policy is Foundation-only, so it compiles directly:
//
//     swiftc Fumi/Views/Sidebar/SidebarWidthPolicy.swift Tests/SidebarWidthPolicyTests.swift -o /tmp/sidebarwidth-tests
//     /tmp/sidebarwidth-tests
//
// Exits non-zero if any assertion fails.

@main
enum SidebarWidthPolicyTests {
    static var failures = 0
    static var checks = 0

    static func expect<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            print("FAIL: \(message)\n  expected: \(expected)\n  actual:   \(actual)")
        }
    }

    static func main() {
        // ── Settling on release ──
        expect(SidebarWidthPolicy.settle(170), 200, "under the minimum springs to 200")
        expect(SidebarWidthPolicy.settle(234), 240, "near 240 snaps to it")
        expect(SidebarWidthPolicy.settle(226), 226, "standard tier rests anywhere else")
        expect(SidebarWidthPolicy.settle(285), 280, "transition: closer to 280")
        expect(SidebarWidthPolicy.settle(295), 300, "transition: closer to 300")
        expect(SidebarWidthPolicy.settle(325), 320, "near 320 snaps to it")
        expect(SidebarWidthPolicy.settle(360), 360, "wide tier rests anywhere else")
        expect(SidebarWidthPolicy.settle(500), 420, "never wider than the max")

        // ── Haptic snap points ──
        expect(SidebarWidthPolicy.crossedSnapPoint(from: 230, to: 245), 240, "crossing 240 rightwards")
        expect(SidebarWidthPolicy.crossedSnapPoint(from: 330, to: 318), 320, "crossing 320 leftwards")
        expect(SidebarWidthPolicy.crossedSnapPoint(from: 250, to: 260), nil, "no snap point in between")

        // ── Double-click ──
        expect(SidebarWidthPolicy.doubleClickTarget(current: 240, contentWidth: 351.2), 352, "fits the longest row")
        expect(SidebarWidthPolicy.doubleClickTarget(current: 352, contentWidth: 351.2), 240, "second double-click resets")
        expect(SidebarWidthPolicy.doubleClickTarget(current: 240, contentWidth: 900), 420, "fit is capped at the max")
        expect(SidebarWidthPolicy.doubleClickTarget(current: 260, contentWidth: 120), 200, "fit never goes below the standard min")

        if failures == 0 {
            print("All \(checks) SidebarWidthPolicy checks passed.")
        } else {
            print("\(failures) of \(checks) SidebarWidthPolicy checks failed.")
            exit(1)
        }
    }
}
