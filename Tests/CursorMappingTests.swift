import Foundation

// Standalone test runner for CursorMapping. There is no Xcode test target in
// this project, and CursorMapping has no AppKit dependency, so this compiles
// directly with the source file:
//
//     swiftc Fumi/Views/Editor/CursorMapping.swift Tests/CursorMappingTests.swift -o /tmp/cursormap-tests
//     /tmp/cursormap-tests
//
// Exits non-zero if any assertion fails.

typealias Seg = CursorMapping.Segment

@main
enum CursorMappingTests {
    static var failures = 0
    static var checks = 0

    static func expect(_ actual: Int, _ expected: Int, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message) — expected \(expected), got \(actual)\n".data(using: .utf8)!)
        }
    }

    static func main() {

// A representative image markdown source. Its exact length is what matters.
let img = "![alt](https://host/x.gif)"
let imgLen = (img as NSString).length // 26

// MARK: - displayLocation (raw -> display)

// 1. A document that is a single collapsed image. Caret at the END of the image
//    source (raw == imgLen) must map AFTER the attachment (display 1), not before.
//    This is the paste regression: after pasting a GIF the caret belongs to its
//    right.
do {
    let segs = [Seg.attachment(rawLength: imgLen)]
    expect(CursorMapping.displayLocation(forRawLocation: imgLen, segments: segs, displayTotal: 1), 1,
           "caret after a sole pasted image lands to its right")
    expect(CursorMapping.displayLocation(forRawLocation: 0, segments: segs, displayTotal: 1), 0,
           "caret before a sole image lands to its left")
    // Inside the source snaps before the attachment.
    expect(CursorMapping.displayLocation(forRawLocation: 5, segments: segs, displayTotal: 1), 0,
           "caret inside image source snaps before the image")
}

// 2. image + "\n" + "X". After deleting the newline on the line below, the caret
//    sits at raw == imgLen (right after the image). It must stay AFTER the image
//    (display 1), not teleport to its left (display 0).
do {
    // display: [img][\n][X] -> raw: img(26) + "\nX"(2)
    let segs = [Seg.attachment(rawLength: imgLen), Seg.text(2)]
    let total = 3
    expect(CursorMapping.displayLocation(forRawLocation: imgLen, segments: segs, displayTotal: total), 1,
           "caret stays right of image after deleting the line below")
    // Caret further into the trailing text maps 1:1.
    expect(CursorMapping.displayLocation(forRawLocation: imgLen + 1, segments: segs, displayTotal: total), 2,
           "raw offset inside trailing text maps to display offset")
    expect(CursorMapping.displayLocation(forRawLocation: imgLen + 2, segments: segs, displayTotal: total), 3,
           "raw offset at end of document maps to end of display")
}

// 3. "hi" + image. Caret at the boundary between the text and the image
//    (raw == 2) lands just BEFORE the image (display 2).
do {
    let segs = [Seg.text(2), Seg.attachment(rawLength: imgLen)]
    let total = 3
    expect(CursorMapping.displayLocation(forRawLocation: 2, segments: segs, displayTotal: total), 2,
           "caret at end of text before an image lands before the image")
    expect(CursorMapping.displayLocation(forRawLocation: 2 + imgLen, segments: segs, displayTotal: total), 3,
           "caret after a trailing image lands at end of display")
}

// 4. Two back-to-back images. The boundary between them (raw == imgLen) lands
//    between the two attachments (display 1), not before the first.
do {
    let segs = [Seg.attachment(rawLength: imgLen), Seg.attachment(rawLength: imgLen)]
    let total = 2
    expect(CursorMapping.displayLocation(forRawLocation: imgLen, segments: segs, displayTotal: total), 1,
           "caret between two images lands between them")
    expect(CursorMapping.displayLocation(forRawLocation: 2 * imgLen, segments: segs, displayTotal: total), 2,
           "caret after the second image lands at end")
}

// MARK: - rawLocation (display -> raw)

do {
    let segs = [Seg.attachment(rawLength: imgLen), Seg.text(2)]
    expect(CursorMapping.rawLocation(forDisplayLocation: 0, segments: segs), 0,
           "display 0 -> raw 0")
    expect(CursorMapping.rawLocation(forDisplayLocation: 1, segments: segs), imgLen,
           "display after image -> raw end of image source")
    expect(CursorMapping.rawLocation(forDisplayLocation: 2, segments: segs), imgLen + 1,
           "display inside trailing text -> raw maps 1:1")
    expect(CursorMapping.rawLocation(forDisplayLocation: 3, segments: segs), imgLen + 2,
           "display end -> raw end")
}

// MARK: - round trip (display -> raw -> display) is stable at every caret slot

do {
    let segs = [Seg.text(2), Seg.attachment(rawLength: imgLen), Seg.text(3),
                Seg.attachment(rawLength: imgLen)]
    let total = 2 + 1 + 3 + 1 // 7
    for display in 0...total {
        let raw = CursorMapping.rawLocation(forDisplayLocation: display, segments: segs)
        let back = CursorMapping.displayLocation(forRawLocation: raw, segments: segs, displayTotal: total)
        expect(back, display, "round trip stable at display slot \(display)")
    }
}

// MARK: - revealed image (source shown as text, image after it with no raw text)

// display: [source text][img][\n][X] -> raw: source + "\nX". Both edges of the
// image map to the end of the source, which maps back to before the image —
// so the caret sits at the end of the visible source, not past the image.
do {
    let segs = [Seg.text(imgLen), Seg.attachment(rawLength: 0), Seg.text(2)]
    let total = imgLen + 1 + 2
    expect(CursorMapping.rawLocation(forDisplayLocation: imgLen + 1, segments: segs), imgLen,
           "after a revealed image is still the end of its source")
    expect(CursorMapping.displayLocation(forRawLocation: imgLen, segments: segs, displayTotal: total), imgLen,
           "end of a revealed source lands before its image")
    expect(CursorMapping.displayLocation(forRawLocation: imgLen + 1, segments: segs, displayTotal: total), imgLen + 2,
           "the next line skips over the revealed image")
}

        if failures == 0 {
            print("OK — all \(checks) CursorMapping checks passed")
            exit(0)
        } else {
            FileHandle.standardError.write("\(failures)/\(checks) CursorMapping checks FAILED\n".data(using: .utf8)!)
            exit(1)
        }
    }
}
