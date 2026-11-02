import Foundation

/// Where a dragged sidebar settles, and the widths it snaps to (design board
/// ⑰, without the icon rail: narrower than the minimum collapses it).
enum SidebarWidthPolicy {
    static let minimum: CGFloat = 200
    static let standardDefault: CGFloat = 240
    static let standardMax: CGFloat = 280
    static let wideMin: CGFloat = 300
    static let wideSnap: CGFloat = 320
    static let max: CGFloat = 420

    /// Released this close to a snap point, the width lands exactly on it.
    static let snapDistance: CGFloat = 8
    /// Widths that give a haptic tick when the divider passes them.
    static let snapPoints: [CGFloat] = [standardDefault, wideSnap]

    /// The width the sidebar animates to when the divider is released at
    /// `width`. (Collapsing past the minimum is handled by the split view.)
    static func settle(_ width: CGFloat) -> CGFloat {
        switch width {
        case ..<minimum:
            return minimum
        case ...standardMax:
            return abs(width - standardDefault) <= snapDistance ? standardDefault : width.rounded()
        case ..<wideMin:
            // The 280–300 gap is a transition, not a resting place.
            return width - standardMax < wideMin - width ? standardMax : wideMin
        default:
            let clamped = Swift.min(width, max)
            return abs(clamped - wideSnap) <= snapDistance ? wideSnap : clamped.rounded()
        }
    }

    /// The snap point crossed when the width moves from `old` to `new`, if any.
    static func crossedSnapPoint(from old: CGFloat, to new: CGFloat) -> CGFloat? {
        snapPoints.first { point in
            (old < point && new >= point) || (old > point && new <= point)
        }
    }

    /// Double-click on the divider: fit the longest row, or go back to the
    /// default when already fitted (or already at the default for a list that
    /// fits). `contentWidth` is the widest row's natural width.
    static func doubleClickTarget(current: CGFloat, contentWidth: CGFloat) -> CGFloat {
        let fit = Swift.min(Swift.max(contentWidth.rounded(.up), minimum), max)
        return abs(current - fit) < 1 ? standardDefault : fit
    }
}
