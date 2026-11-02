import AppKit
import SwiftUI

/// The page's top edge as text scrolls past it (like macOS 26's soft scroll
/// edge effect, which only applies under a toolbar — our page floats below
/// it): the text frosts and fades out as it leaves, instead of being cut
/// off. A blur that strengthens toward the edge, under a wash of the page's
/// own color so no material tint shows.
struct PageEdgeBlur: View {
    var height: CGFloat = 30

    var body: some View {
        ZStack {
            EdgeBlurView()
            LinearGradient(stops: [
                .init(color: Color.pageContentBg.opacity(0.8), location: 0),
                .init(color: Color.pageContentBg.opacity(0.3), location: 0.4),
                .init(color: Color.pageContentBg.opacity(0), location: 0.75),
            ], startPoint: .top, endPoint: .bottom)
        }
        .frame(height: height)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// Blurs what the window shows behind it, fully at the top and not at all at
/// the bottom.
private struct EdgeBlurView: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.blendingMode = .withinWindow
        view.material = .contentBackground
        view.state = .active
        view.maskImage = Self.mask
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {}

    /// Opaque at the top, clear at the bottom; stretched to the view.
    private static let mask: NSImage = {
        let image = NSImage(size: NSSize(width: 1, height: 64), flipped: true) { rect in
            NSGradient(colors: [.black, .black, .black.withAlphaComponent(0.5), .clear],
                       atLocations: [0, 0.3, 0.65, 1], colorSpace: .sRGB)?
                .draw(in: rect, angle: 90)
            return true
        }
        image.resizingMode = .stretch
        return image
    }()
}
