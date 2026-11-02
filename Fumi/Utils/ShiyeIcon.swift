import AppKit
import SwiftUI

/// 拾页's own icons (design board ⑳, set C): line glyphs on a 24-unit grid,
/// stroked at 1.7 with round caps and joins — the same hand as the folder
/// glyph (⑱). Other generic actions (search, settings…) stay on SF Symbols.
enum ShiyeGlyph {
    /// Window with the sidebar split off: show / hide the sidebar.
    case sidebar
    /// A page with a plus.
    case newNote
    /// A page with a folded corner and two lines of text.
    case note
    /// A stack of notes: one page in front, the corner of another behind.
    case library
    /// `#`, as tags are written in the notes.
    case tag
    /// A speech bubble with two lines: a memo (design board ㉛).
    case memo
    /// A frame with a hill and a sun: an image file.
    case image
    /// A frame with a play triangle: a video file.
    case video
    /// A page with a folded corner and no text: any other file (PDF).
    case file
    /// A circled `i`: note info.
    case info
    /// Three dots: more actions.
    case more
    /// A phone: the WeChat article preview beside the page.
    case phone
    /// A chevron pointing left: back to the page before.
    case back
    /// A chevron pointing right: forward again.
    case forward

    /// Stroke width at `size` points, matching 1.7 on the 24-unit grid.
    static func lineWidth(forSize size: CGFloat) -> CGFloat { 1.7 * size / 24 }

    func path(in rect: CGRect) -> Path {
        let k = min(rect.width, rect.height) / 24
        func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: rect.minX + x * k, y: rect.minY + y * k) }
        func line(_ path: inout Path, _ points: (CGFloat, CGFloat)...) {
            path.move(to: p(points[0].0, points[0].1))
            for point in points.dropFirst() { path.addLine(to: p(point.0, point.1)) }
        }
        func roundedRect(_ path: inout Path, _ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, _ r: CGFloat) {
            path.addRoundedRect(in: CGRect(x: rect.minX + x * k, y: rect.minY + y * k, width: w * k, height: h * k),
                                cornerSize: CGSize(width: r * k, height: r * k), style: .continuous)
        }

        var path = Path()
        switch self {
        case .sidebar:
            roundedRect(&path, 3, 4.5, 18, 15, 3)
            line(&path, (9.5, 4.5), (9.5, 19.5))

        case .newNote:
            // Page, left open at the bottom right for the plus.
            path.move(to: p(12, 20.5))
            path.addLine(to: p(7, 20.5))
            path.addQuadCurve(to: p(5, 18.5), control: p(5, 20.5))
            path.addLine(to: p(5, 5.5))
            path.addQuadCurve(to: p(7, 3.5), control: p(5, 3.5))
            path.addLine(to: p(13, 3.5))
            path.addLine(to: p(18, 8.5))
            path.addLine(to: p(18, 12))
            line(&path, (13, 3.5), (13, 8.5), (18, 8.5))
            line(&path, (18, 15.5), (18, 20.5))
            line(&path, (15.5, 18), (20.5, 18))

        case .note:
            path.move(to: p(14, 3.5))
            path.addLine(to: p(7, 3.5))
            path.addQuadCurve(to: p(5, 5.5), control: p(5, 3.5))
            path.addLine(to: p(5, 18.5))
            path.addQuadCurve(to: p(7, 20.5), control: p(5, 20.5))
            path.addLine(to: p(17, 20.5))
            path.addQuadCurve(to: p(19, 18.5), control: p(19, 20.5))
            path.addLine(to: p(19, 8.5))
            path.closeSubpath()
            line(&path, (14, 3.5), (14, 8.5), (19, 8.5))
            line(&path, (8.5, 13), (15.5, 13))
            line(&path, (8.5, 16.5), (13, 16.5))

        case .library:
            // The page behind shows only its top-right corner.
            path.move(to: p(9, 3.5))
            path.addLine(to: p(17.5, 3.5))
            path.addQuadCurve(to: p(19.5, 5.5), control: p(19.5, 3.5))
            path.addLine(to: p(19.5, 16))
            roundedRect(&path, 4.5, 6.5, 12, 14, 2)
            line(&path, (7.5, 11.5), (13.5, 11.5))
            line(&path, (7.5, 15), (11.5, 15))

        case .memo:
            path.move(to: p(7, 4.5))
            path.addLine(to: p(17, 4.5))
            path.addQuadCurve(to: p(19, 6.5), control: p(19, 4.5))
            path.addLine(to: p(19, 14.5))
            path.addQuadCurve(to: p(17, 16.5), control: p(19, 16.5))
            path.addLine(to: p(11, 16.5))
            path.addLine(to: p(7, 19.5))
            path.addLine(to: p(7, 16.5))
            path.addQuadCurve(to: p(5, 14.5), control: p(5, 16.5))
            path.addLine(to: p(5, 6.5))
            path.addQuadCurve(to: p(7, 4.5), control: p(5, 4.5))
            path.closeSubpath()
            line(&path, (8.5, 9), (15.5, 9))
            line(&path, (8.5, 12), (13, 12))

        case .tag:
            line(&path, (9.5, 4), (8, 20))
            line(&path, (16, 4), (14.5, 20))
            line(&path, (4.5, 9), (19.5, 9))
            line(&path, (4, 15), (19, 15))

        case .image:
            roundedRect(&path, 3.5, 5, 17, 14, 2.5)
            path.move(to: p(3.5, 16.5))
            path.addLine(to: p(8.5, 11.5))
            path.addLine(to: p(13, 16))
            path.move(to: p(11.5, 14.5))
            path.addLine(to: p(14, 12))
            path.addLine(to: p(20.5, 18))
            path.addEllipse(in: CGRect(origin: p(14.6, 7.6), size: CGSize(width: 2.4 * k, height: 2.4 * k)))

        case .video:
            roundedRect(&path, 3.5, 5, 17, 14, 2.5)
            path.move(to: p(10.5, 9.2))
            path.addLine(to: p(15, 12))
            path.addLine(to: p(10.5, 14.8))
            path.closeSubpath()

        case .file:
            path.move(to: p(14, 3.5))
            path.addLine(to: p(7, 3.5))
            path.addQuadCurve(to: p(5, 5.5), control: p(5, 3.5))
            path.addLine(to: p(5, 18.5))
            path.addQuadCurve(to: p(7, 20.5), control: p(5, 20.5))
            path.addLine(to: p(17, 20.5))
            path.addQuadCurve(to: p(19, 18.5), control: p(19, 20.5))
            path.addLine(to: p(19, 8.5))
            path.closeSubpath()
            line(&path, (14, 3.5), (14, 8.5), (19, 8.5))

        case .info:
            path.addEllipse(in: CGRect(origin: p(3, 3), size: CGSize(width: 18 * k, height: 18 * k)))
            line(&path, (12, 11), (12, 16.5))
            // A stroke this short draws as a round dot.
            line(&path, (12, 7.6), (12, 7.65))

        case .phone:
            roundedRect(&path, 6.5, 2.5, 11, 19, 2.5)
            line(&path, (10.5, 18.5), (13.5, 18.5))

        case .back:
            line(&path, (14.5, 6), (8.5, 12), (14.5, 18))

        case .forward:
            line(&path, (9.5, 6), (15.5, 12), (9.5, 18))

        case .more:
            for x in [5.5, 12, 18.5] as [CGFloat] {
                path.addEllipse(in: CGRect(origin: p(x - 0.6, 11.4), size: CGSize(width: 1.2 * k, height: 1.2 * k)))
            }
        }
        return path
    }

    /// A template image for AppKit-drawn places — toolbar buttons, which
    /// tint it themselves.
    func templateImage(size: CGFloat = 18) -> NSImage {
        let image = NSImage(size: NSSize(width: size, height: size), flipped: true) { rect in
            guard let context = NSGraphicsContext.current?.cgContext else { return false }
            context.addPath(self.path(in: rect).cgPath)
            context.setLineWidth(Self.lineWidth(forSize: size))
            context.setLineCap(.round)
            context.setLineJoin(.round)
            context.setStrokeColor(NSColor.black.cgColor)
            context.strokePath()
            return true
        }
        image.isTemplate = true
        return image
    }
}

/// A 拾页 glyph stroked in the current foreground style.
struct ShiyeIcon: View {
    let glyph: ShiyeGlyph
    var size: CGFloat = 16

    init(_ glyph: ShiyeGlyph, size: CGFloat = 16) {
        self.glyph = glyph
        self.size = size
    }

    var body: some View {
        GlyphShape(glyph: glyph)
            .stroke(style: StrokeStyle(lineWidth: ShiyeGlyph.lineWidth(forSize: size),
                                       lineCap: .round, lineJoin: .round))
            .frame(width: size, height: size)
    }

    private struct GlyphShape: Shape {
        let glyph: ShiyeGlyph
        func path(in rect: CGRect) -> Path { glyph.path(in: rect) }
    }
}
