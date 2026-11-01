import AppKit
import SwiftUI

// The parts both sides of the publish preview are made of (board 62 A):
// one toolbar with the platform names as the switch, a pop-up for the look,
// one bordered button, and a quiet notice under the bar.

/// The preview's one bar: WeChat · Xiaohongshu on the left, the side's own
/// controls on the right.
struct PublishToolbar<Trailing: View>: View {
    @ViewBuilder let trailing: Trailing

    var body: some View {
        HStack(spacing: 8) {
            PlatformNames()
            Spacer(minLength: 8)
            trailing
        }
        .padding(.leading, 16)
        .padding(.trailing, 12)
        .frame(height: 44)
        .overlay(alignment: .bottom) { Rectangle().fill(Color.primaryText.opacity(0.06)).frame(height: 1) }
    }
}

/// The names are the switch, as the Settings title is (board ㊼). With one
/// extension on, only its name shows and it isn't a button.
private struct PlatformNames: View {
    @Environment(AppModel.self) private var appModel
    @AppStorage(AppSettings.wechatExtensionKey) private var wechatEnabled = false
    @AppStorage(AppSettings.xhsExtensionKey) private var xhsEnabled = false

    var body: some View {
        let selection = appModel.publishTarget(for: appModel.selectedNoteId)
        let isSwitch = wechatEnabled && xhsEnabled
        HStack(spacing: 14) {
            ForEach(AppModel.PublishTarget.allCases, id: \.self) { target in
                if isSwitch || target == selection {
                    PlatformName(target: target, isSelected: target == selection, isSwitch: isSwitch) {
                        appModel.setPublishTarget(target, for: appModel.selectedNoteId)
                    }
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Publish to")
    }
}

private struct PlatformName: View {
    let target: AppModel.PublishTarget
    let isSelected: Bool
    let isSwitch: Bool
    let choose: () -> Void
    @State private var isHovering = false

    var body: some View {
        let title = Text(target == .wechat ? "WeChat" : "Xiaohongshu")
            .font(.notely(.body, weight: .semibold))
        if isSwitch {
            Button(action: choose) {
                title
                    .foregroundColor(isSelected ? .primaryText : isHovering ? .secondaryText : .faintText)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { isHovering = $0 }
            .animation(.easeOut(duration: 0.15), value: isSelected)
            .accessibilityAddTraits(isSelected ? .isSelected : [])
        } else {
            title.foregroundColor(.primaryText)
        }
    }
}

/// A word with a ⌄ that opens a menu below itself: the theme, the template.
struct PopUpButton<Label: View>: View {
    let help: String
    let menu: () -> NSMenu
    @ViewBuilder let label: Label
    @State private var anchor: NSView?
    @State private var isHovering = false
    @State private var isOpen = false

    var body: some View {
        Button {
            isOpen = true
            menu().popUp(below: anchor)
            isOpen = false
        } label: {
            HStack(spacing: 6) {
                label
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundColor(.iconMuted)
            }
            .font(.notely(.caption))
            .foregroundColor(.secondaryText)
            .padding(.leading, 8)
            .padding(.trailing, 7)
            .frame(height: 24)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(isHovering || isOpen ? Color.inlineFill : Color.clear)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .background(AnchorView(view: $anchor))
        .help(help)
    }
}

/// The side's one button, Copy or Export: a plain bordered button, so the
/// color on the sheet is the article's own.
struct PaneButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    @Environment(\.colorScheme) private var colorScheme

    func makeBody(configuration: Configuration) -> some View {
        let shape = RoundedRectangle(cornerRadius: 6, style: .continuous)
        configuration.label
            .font(.notely(.caption, weight: .medium))
            .foregroundColor(.primaryText)
            .padding(.horizontal, 11)
            .frame(height: 24)
            .background {
                shape.fill(colorScheme == .dark ? Color.white.opacity(0.1) : Color.white)
                    .overlay { if configuration.isPressed { shape.fill(Color.inlineFill) } }
                    .shadow(color: .black.opacity(colorScheme == .dark ? 0 : 0.05), radius: 0.5, y: 0.5)
            }
            .overlay { shape.strokeBorder(Color.controlBorder, lineWidth: 1) }
            .opacity(isEnabled ? 1 : 0.45)
            .contentShape(Rectangle())
    }
}

/// What to look at before posting: an amber dot and one line, the details
/// under it, Hide to fold it into a dot in the bar.
struct PaneNotice<Extra: View>: View {
    let header: String?
    let items: [String]
    var hide: (() -> Void)?
    @ViewBuilder var extra: Extra

    static var amber: Color { Color(nsColor: .adaptive(light: 0xD99A1E, dark: 0xE2B04A)) }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let header {
                HStack(spacing: 7) {
                    Circle().fill(Self.amber).frame(width: 6, height: 6)
                    Text(header)
                        .font(.notely(.caption, weight: .medium))
                        .foregroundColor(.primaryText)
                    Spacer(minLength: 8)
                    if let hide { QuietLink(title: "Hide", action: hide) }
                }
            }
            ForEach(items, id: \.self) { item in
                Text(item)
                    .foregroundColor(.tertiaryText)
                    .padding(.leading, header == nil ? 0 : 13)
            }
            extra
        }
        .font(.notely(.caption))
        .lineSpacing(2)
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
        .padding(.top, 9)
        .padding(.bottom, 10)
        .overlay(alignment: .bottom) { Rectangle().fill(Color.primaryText.opacity(0.06)).frame(height: 1) }
    }

    static func header(_ count: Int, before action: String) -> String {
        count == 1 ? "1 thing to check before \(action)" : "\(count) things to check before \(action)"
    }
}

extension PaneNotice where Extra == EmptyView {
    init(header: String?, items: [String], hide: (() -> Void)? = nil) {
        self.init(header: header, items: items, hide: hide) { EmptyView() }
    }
}

/// A hidden notice, left in the bar as its dot; clicking shows it again.
struct NoticeDot: View {
    let show: () -> Void

    var body: some View {
        Button(action: show) {
            Circle().fill(PaneNotice<EmptyView>.amber).frame(width: 6, height: 6)
                .frame(width: 18, height: 24)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Show what to check")
    }
}

struct QuietLink: View {
    let title: String
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.notely(.caption))
                .foregroundColor(isHovering ? .primaryText : .tertiaryText)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
    }
}

/// Hands back the NSView behind a SwiftUI view, to anchor a menu to it.
struct AnchorView: NSViewRepresentable {
    @Binding var view: NSView?

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { self.view = view }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}

extension NSMenu {
    /// Opens just below `anchor`, its left edges lined up; at the mouse
    /// when there's no anchor yet.
    func popUp(below anchor: NSView?) {
        if let anchor {
            popUp(positioning: nil, at: NSPoint(x: 0, y: anchor.isFlipped ? anchor.bounds.maxY + 4 : -4), in: anchor)
        } else {
            popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
        }
    }
}

extension WeChatRenderer.Theme {
    /// The theme's own accent: its bold text and heading marks.
    var accent: NSColor {
        switch self {
        case .blackRed: .adaptive(light: 0x8F1D22, dark: 0x8F1D22)
        case .blueprint: .adaptive(light: 0x2455D6, dark: 0x2455D6)
        case .songti: .adaptive(light: 0xBD3038, dark: 0xBD3038)
        case .motionJournal: .adaptive(light: 0x435924, dark: 0x435924)
        }
    }

    /// A page in miniature for the theme menu: an accent heading, two lines.
    var swatch: NSImage {
        let accent = accent
        let image = NSImage(size: NSSize(width: 20, height: 14), flipped: true) { rect in
            let page = NSBezierPath(roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: 3, yRadius: 3)
            NSColor.white.setFill()
            page.fill()
            NSColor.black.withAlphaComponent(0.2).setStroke()
            page.lineWidth = 0.5
            page.stroke()
            accent.setFill()
            NSBezierPath(roundedRect: NSRect(x: 3, y: 3, width: 10, height: 2.5), xRadius: 1, yRadius: 1).fill()
            NSColor(white: 0.78, alpha: 1).setFill()
            NSBezierPath(rect: NSRect(x: 3, y: 7, width: 14, height: 1.5)).fill()
            NSBezierPath(rect: NSRect(x: 3, y: 10, width: 11, height: 1.5)).fill()
            return true
        }
        return image
    }
}
