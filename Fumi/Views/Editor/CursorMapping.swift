import Foundation

/// Pure caret-coordinate mapping between the *display* storage — where images,
/// tables and math are each collapsed to a single attachment character — and the
/// *raw* Markdown string, where each attachment expands back to its full source.
///
/// This logic lives apart from `WysiwygTextView` so its boundary behaviour can be
/// unit-tested without an `NSTextView`. The instance methods on the text view
/// build a `[Segment]` from the live `NSTextStorage` and delegate here.
enum CursorMapping {
    /// One enumerated run of the display storage.
    struct Segment {
        /// Characters this run occupies in the display storage. Always 1 for a
        /// collapsed attachment; the run length for plain text.
        let displayLength: Int
        /// Characters this run occupies in the raw Markdown: the attachment's
        /// `markdownSource` length for an attachment, equal to `displayLength`
        /// for plain text.
        let rawLength: Int
        /// Whether this run is a collapsed attachment.
        let isAttachment: Bool

        static func text(_ length: Int) -> Segment {
            Segment(displayLength: length, rawLength: length, isAttachment: false)
        }

        static func attachment(rawLength: Int) -> Segment {
            Segment(displayLength: 1, rawLength: rawLength, isAttachment: true)
        }
    }

    /// Raw-Markdown offset corresponding to a display-storage offset.
    static func rawLocation(forDisplayLocation displayLocation: Int, segments: [Segment]) -> Int {
        guard displayLocation > 0 else { return 0 }
        var displayConsumed = 0
        var raw = 0
        for seg in segments {
            if displayConsumed + seg.displayLength <= displayLocation {
                raw += seg.rawLength
                displayConsumed += seg.displayLength
            } else {
                // The caret lands inside this run. Only plain text can straddle a
                // caret position (an attachment is a single character), and text
                // has rawLength == displayLength, so the partial offset maps 1:1.
                return raw + (displayLocation - displayConsumed)
            }
        }
        return raw
    }

    /// Display-storage offset corresponding to a raw-Markdown offset.
    ///
    /// A raw offset that lands *inside* a collapsed attachment's source snaps to
    /// just *before* that attachment. A raw offset at the attachment's *trailing*
    /// boundary maps to just *after* it — this is the boundary the caret must land
    /// on right after pasting/inserting an image, or after deleting the line below
    /// it. Mapping that boundary to "before" was the bug that teleported the caret
    /// to the left of the image.
    static func displayLocation(forRawLocation rawLocation: Int, segments: [Segment], displayTotal: Int) -> Int {
        var raw = 0
        var display = 0
        for seg in segments {
            if seg.isAttachment {
                // Strict `<`: the trailing boundary (rawLocation == raw + rawLength)
                // falls through so the caret lands AFTER the attachment, not before.
                if rawLocation < raw + seg.rawLength {
                    return display
                }
            } else if rawLocation <= raw + seg.rawLength {
                return display + (rawLocation - raw)
            }
            raw += seg.rawLength
            display += seg.displayLength
        }
        return displayTotal
    }
}
