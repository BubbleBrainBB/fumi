import SwiftUI

/// A heading in the document outline. `index` is the heading's ordinal in
/// document order, which is the payload used to scroll the editor.
struct OutlineHeading: Identifiable {
    let index: Int
    let level: Int
    let text: String
    /// Plain-text excerpt of the section body, shown in the preview card.
    let summary: String
    var id: Int { index }
}

/// Right-edge navigation, modelled on Coworker's chat outline rail: one short
/// tick per heading, vertically centered. Under the pointer the ticks swell
/// like a dock — springs, so they ease instead of snapping — and a single
/// preview card glides alongside to the nearest heading. Clicking a tick or
/// the card scrolls the editor there via `.scrollToHeading`.
struct OutlineRail: View {
    let headings: [OutlineHeading]

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Pointer position in tick units; nil when the rail is idle. Every hover
    /// frame updates it, and each tick derives its swell from it.
    @State private var position: CGFloat?
    /// Nearest heading, which the card shows. Changes only at a new heading.
    @State private var selected: Int?
    @State private var cardVisible = false
    @State private var cardHeight: CGFloat = 0
    @State private var overCard = false
    @State private var openTask: Task<Void, Never>?
    @State private var closeTask: Task<Void, Never>?

    private static let pitch: CGFloat = 12
    private static let maxRailHeight: CGFloat = 520
    private static let railWidth: CGFloat = 30
    private static let edgeInset: CGFloat = 14
    private static let cardWidth: CGFloat = 280
    private static let cardGap: CGFloat = 8
    private static let cardMargin: CGFloat = 12
    private static let hoverDelay: Duration = .milliseconds(120)

    private static func ease(_ duration: Double) -> Animation {
        .timingCurve(0.22, 1, 0.36, 1, duration: duration)
    }

    var body: some View {
        if headings.isEmpty {
            Color.clear.frame(width: 0)
        } else {
            GeometryReader { geo in
                rail(paneHeight: geo.size.height)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
            .frame(width: Self.railWidth + Self.edgeInset)
            .onDisappear(perform: close)
        }
    }

    private func rail(paneHeight: CGFloat) -> some View {
        let count = headings.count
        let railHeight = min(CGFloat(count - 1) * Self.pitch, Self.maxRailHeight, max(0, paneHeight - 40))
        let railTop = (paneHeight - railHeight) / 2
        let tickY = { (i: Int) in count > 1 ? CGFloat(i) / CGFloat(count - 1) * railHeight : 0 }
        let index = { (y: CGFloat) -> CGFloat in
            let fraction = min(1, max(0, (y - Self.pitch / 2) / max(railHeight, 1)))
            return fraction * CGFloat(count - 1)
        }

        return ZStack(alignment: .topTrailing) {
            ForEach(headings) { h in
                OutlineTick(level: h.level, proximity: proximity(of: h.index),
                            animation: reduceMotion ? nil : .interpolatingSpring(mass: 0.5, stiffness: 700, damping: 45))
                    .offset(y: Self.pitch / 2 + tickY(h.index) - 1)
            }
        }
        .frame(width: Self.railWidth, height: railHeight + Self.pitch, alignment: .topTrailing)
        .contentShape(Rectangle())
        .onContinuousHover { phase in
            switch phase {
            case .active(let point): select(index(point.y))
            case .ended: scheduleClose()
            }
        }
        .onTapGesture { point in
            navigate(to: Int(index(point.y).rounded()))
        }
        .overlay(alignment: .topLeading) {
            if let i = selected.map({ min($0, count - 1) }) {
                let cardTop = min(max(railTop + tickY(i) - cardHeight / 2, Self.cardMargin),
                                  paneHeight - cardHeight - Self.cardMargin)
                card(for: headings[i])
                    .offset(x: -(Self.cardWidth + Self.cardGap) + (cardVisible ? 0 : 4),
                            y: cardTop - (railTop - Self.pitch / 2))
                    .animation(cardVisible && !reduceMotion ? Self.ease(0.18) : nil, value: cardTop)
            }
        }
        .padding(.top, railTop - Self.pitch / 2)
    }

    private func card(for heading: OutlineHeading) -> some View {
        Button {
            navigate(to: heading.index)
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                Text(heading.text)
                    .font(.fumi(.caption, weight: .medium))
                    .foregroundColor(.primaryText)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if !heading.summary.isEmpty {
                    Text(heading.summary)
                        .font(.fumi(.caption))
                        .foregroundColor(.tertiaryText)
                        .lineSpacing(3)
                        .lineLimit(3)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .frame(width: Self.cardWidth, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            // New content arrives at 45% and settles, rather than a hard swap.
            .keyframeAnimator(initialValue: 1.0, trigger: heading.index) { content, opacity in
                content.opacity(reduceMotion ? 1 : opacity)
            } keyframes: { _ in
                MoveKeyframe(0.45)
                LinearKeyframe(1.0, duration: 0.09)
            }
            .onGeometryChange(for: CGFloat.self, of: \.size.height) { cardHeight = $0 }
            .frame(width: Self.cardWidth, height: cardHeight, alignment: .top)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color.surface)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(Color.borderColor.opacity(0.65), lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
        .buttonStyle(.plain)
        .shadow(color: .black.opacity(0.12), radius: 12, x: 0, y: 6)
        .shadow(color: .black.opacity(0.04), radius: 2.5, x: 0, y: 2)
        .opacity(cardVisible ? 1 : 0)
        .allowsHitTesting(cardVisible)
        .onHover { inside in
            overCard = inside
            if inside { cancelClose() } else { scheduleClose() }
        }
        .animation(reduceMotion ? nil : Self.ease(0.16), value: cardVisible)
        .animation(cardVisible && !reduceMotion ? Self.ease(0.16) : nil, value: cardHeight)
    }

    private func proximity(of index: Int) -> CGFloat {
        guard let position else { return 0 }
        let t = max(0, 1 - abs(CGFloat(index) - position) / 3.5)
        return t * t
    }

    private func select(_ next: CGFloat) {
        cancelClose()
        position = next
        let nearest = min(headings.count - 1, max(0, Int(next.rounded())))
        if selected != nearest { selected = nearest }
        if !cardVisible && openTask == nil {
            openTask = Task { @MainActor in
                try? await Task.sleep(for: Self.hoverDelay)
                guard !Task.isCancelled else { return }
                openTask = nil
                cardVisible = true
            }
        }
    }

    private func scheduleClose() {
        openTask?.cancel()
        openTask = nil
        cancelClose()
        closeTask = Task { @MainActor in
            try? await Task.sleep(for: Self.hoverDelay)
            guard !Task.isCancelled, !overCard else { return }
            close()
        }
    }

    private func cancelClose() {
        closeTask?.cancel()
        closeTask = nil
    }

    private func close() {
        openTask?.cancel()
        openTask = nil
        cancelClose()
        cardVisible = false
        position = nil
    }

    private func navigate(to index: Int) {
        guard headings.indices.contains(index) else { return }
        NotificationCenter.default.post(
            name: .scrollToHeading, object: nil, userInfo: ["index": headings[index].index]
        )
    }
}

/// One tick. It grows leftward, toward the text, as the pointer nears it;
/// higher-level headings get a longer resting tick so the hierarchy reads.
private struct OutlineTick: View {
    let level: Int
    let proximity: CGFloat
    let animation: Animation?

    private var width: CGFloat {
        switch level {
        case 1: return 7
        case 2: return 5
        default: return 3.5
        }
    }

    var body: some View {
        Capsule()
            .fill(Color.primaryText)
            .frame(width: width, height: 2)
            .scaleEffect(x: 1 + proximity * 3, y: 1, anchor: .trailing)
            .opacity(0.22 + proximity * 0.78)
            .animation(animation, value: proximity)
    }
}
