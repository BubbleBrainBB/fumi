import AppKit
import SwiftUI

/// The app's scroller: no track, only a slim, faint rounded thumb in the
/// theme's own gray, and always the overlay kind that shows while
/// scrolling — whatever "Show scroll bars" is set to — so a page never
/// carries a heavy gray bar.
final class QuietScroller: NSScroller {
    /// The sidebar draws a short thumb that reads as a mark; the editor
    /// keeps the true length, which shows where you are in a long note.
    var shortensKnob = false

    override class var isCompatibleWithOverlayScrollers: Bool { true }
    override class func scrollerWidth(for controlSize: NSControl.ControlSize, scrollerStyle: NSScroller.Style) -> CGFloat { 7 }

    override func drawKnob() {
        let slotRect = rect(for: .knobSlot)
        let rawKnobRect = rect(for: .knob).insetBy(dx: 0, dy: 2)
        let height = shortensKnob ? min(88, max(28, rawKnobRect.height * 0.45)) : max(24, rawKnobRect.height)
        // Keep a short thumb inside the slot: it travels the whole way from
        // top to bottom as the true one does.
        let travel = max(1, slotRect.height - rawKnobRect.height)
        let progress = (rawKnobRect.minY - slotRect.minY) / travel
        let y = shortensKnob
            ? slotRect.minY + 2 + (slotRect.height - 4 - height) * min(1, max(0, progress))
            : rawKnobRect.midY - height / 2
        let knobRect = NSRect(x: slotRect.midX - 2, y: y, width: 4, height: height)
        NSColor.themed(\.faintIcon).withAlphaComponent(0.6).setFill()
        NSBezierPath(roundedRect: knobRect, xRadius: 2, yRadius: 2).fill()
    }

    override func drawKnobSlot(in slotRect: NSRect, highlight flag: Bool) {
        // No track: only the thumb.
    }
}

/// Gives the enclosing SwiftUI scroll view a QuietScroller and keeps it:
/// SwiftUI and AppKit put the system scroller back (and the legacy style)
/// when the scroll view is rebuilt or "Show scroll bars" changes, so this
/// watches for that and quietly swaps it again.
struct QuietScrollerInstaller: NSViewRepresentable {
    var shortensKnob = false

    func makeNSView(context: Context) -> Probe {
        let probe = Probe()
        probe.shortensKnob = shortensKnob
        return probe
    }

    func updateNSView(_ probe: Probe, context: Context) {
        probe.shortensKnob = shortensKnob
        DispatchQueue.main.async { probe.install() }
    }

    final class Probe: NSView {
        var shortensKnob = false
        private weak var watched: NSScrollView?
        private var observations: [NSKeyValueObservation] = []
        private var styleObserver: NSObjectProtocol?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            DispatchQueue.main.async { [weak self] in self?.install() }
        }

        func install() {
            guard let scrollView = enclosingScrollView else { return }
            apply(to: scrollView)
            guard watched !== scrollView else { return }
            watched = scrollView
            observations = [
                scrollView.observe(\.verticalScroller) { [weak self] scrollView, _ in
                    DispatchQueue.main.async { self?.apply(to: scrollView) }
                },
                scrollView.observe(\.scrollerStyle) { [weak self] scrollView, _ in
                    DispatchQueue.main.async { self?.apply(to: scrollView) }
                },
            ]
            styleObserver = NotificationCenter.default.addObserver(
                forName: NSScroller.preferredScrollerStyleDidChangeNotification, object: nil, queue: .main
            ) { [weak self, weak scrollView] _ in
                guard let scrollView else { return }
                self?.apply(to: scrollView)
            }
        }

        private func apply(to scrollView: NSScrollView) {
            if let scroller = scrollView.verticalScroller as? QuietScroller {
                if scroller.shortensKnob != shortensKnob {
                    scroller.shortensKnob = shortensKnob
                    scroller.needsDisplay = true
                }
            } else {
                let scroller = QuietScroller(frame: .zero)
                scroller.shortensKnob = shortensKnob
                scroller.scrollerStyle = .overlay
                scrollView.verticalScroller = scroller
            }
            if scrollView.scrollerStyle != .overlay { scrollView.scrollerStyle = .overlay }
            if !scrollView.hasVerticalScroller { scrollView.hasVerticalScroller = true }
            if !scrollView.autohidesScrollers { scrollView.autohidesScrollers = true }
        }

        deinit {
            if let styleObserver { NotificationCenter.default.removeObserver(styleObserver) }
        }
    }
}
