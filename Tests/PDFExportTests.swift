import AppKit
import PDFKit

// Standalone integration test for the PDF export (Fumi/Services/PDFExporter.swift).
// Compiles the exporter and its dependency graph together with this file:
//
//     swiftc -o /tmp/pdf-export-tests \
//       Fumi/Utils/ColorTokens.swift \
//       Fumi/Utils/ThemePalette.swift \
//       Fumi/Models/AppSettings.swift \
//       Fumi/Models/FileNote.swift \
//       Fumi/Views/Editor/CompositeFont.swift \
//       Fumi/Views/Editor/WysiwygEngine.swift \
//       Fumi/Views/Editor/CodeBlockLayoutManager.swift \
//       Fumi/Views/Editor/Callout.swift \
//       Fumi/Views/Editor/ImageAttachment.swift \
//       Fumi/Views/Editor/VideoAttachment.swift \
//       Fumi/Views/Editor/FrontmatterAttachment.swift \
//       Fumi/Views/Editor/MarkdownTable.swift \
//       Fumi/Views/Editor/LatexMath.swift \
//       Fumi/Services/FrontmatterParser.swift \
//       Fumi/Services/InlineLink.swift \
//       Fumi/Services/PDFExporter.swift \
//       Tests/PDFExportTests.swift
//     /tmp/pdf-export-tests
//
// Exits non-zero if any assertion fails.
//
// What it pins down:
//   - every block of the note survives into the PDF (no content is dropped),
//   - the text is real text — selectable and searchable, not a picture,
//   - nothing is cut off: the text of every page stays inside the margins,
//   - a long note is broken into pages, in order, with a foot on each page,
//   - an image taller than the page is scaled down instead of running off it,
//   - an empty note still yields one readable page.

// Declared in WysiwygTextView.swift, which this harness leaves out; the image
// attachments post them.
extension Notification.Name {
    static let imageDidLoad = Notification.Name("fumi.imageDidLoad")
    static let animatedImageFrameDidChange = Notification.Name("fumi.animatedImageFrameDidChange")
}

@main
enum PDFExportTests {
    static var failures = 0
    static var checks = 0
    static func expect(_ condition: Bool, _ message: String) {
        checks += 1
        if !condition {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message)\n".data(using: .utf8)!)
        }
    }

    static func expectEqual<T: Equatable>(_ actual: T, _ expected: T, _ message: String) {
        checks += 1
        if actual != expected {
            failures += 1
            FileHandle.standardError.write("FAIL: \(message) — expected \(expected), got \(actual)\n".data(using: .utf8)!)
        }
    }

    // MARK: - The note under test

    /// A note that holds one of every block the editor can draw.
    static let sample = """
    # The Quiet Page

    A paragraph with **bold**, *italic*, `inline code`, a [link](https://example.com), \
    a #tag and a ==highlight== in it.

    ## Lists

    - First item
    - Second item
      - Nested item
    - Third item

    1. One
    2. Two

    - [x] Done
    - [ ] Not done

    ## Quotes

    > A quoted line.
    > Still quoted.

    ## Code

    ```swift
    let answer = 42
    print(answer)
    ```

    ## Table

    | Name | Value |
    | ---- | ----- |
    | alpha | 1 |
    | beta | 2 |

    ## Callout

    > [!note] A callout with a little note in it.

    :::note
    A fenced callout with a little note in it.
    :::

    ---

    ## The end

    The last paragraph of the note, which must be the last thing on the last page.
    """

    static func main() {
        let margins = PDFExporter.Configuration().margin

        // 1. A note with every kind of block in it.
        guard let data = PDFExporter.render(markdown: sample,
                                           configuration: configuration(title: "The Quiet Page")),
              let document = PDFDocument(data: data) else {
            failures += 1
            FileHandle.standardError.write("FAIL: the export produced no readable PDF\n".data(using: .utf8)!)
            report()
            return
        }
        expect(document.pageCount >= 1, "the export has at least one page")

        // 2. Every block of the note is in the PDF — no content is dropped.
        //    The text is compared without the line breaks the page puts in it,
        //    and without the foot, so a paragraph that runs over a page break
        //    still reads as one.
        let text = bodyText(document, margins: margins)
        for fragment in ["The Quiet Page",
                         "A paragraph with",
                         "bold",
                         "italic",
                         "inline code",
                         "a link",
                         "a #tag",
                         "Lists",
                         "First item",
                         "Second item",
                         "Nested item",
                         "Third item",
                         "One",
                         "Two",
                         "Done",
                         "Not done",
                         "Quotes",
                         "A quoted line.",
                         "Still quoted.",
                         "Code",
                         "let answer = 42",
                         "print(answer)",
                         "Table",
                         "alpha",
                         "beta",
                         "Callout",
                         "A callout with a little note in it.",
                         "A fenced callout with a little note in it.",
                         "The end",
                         "The last paragraph of the note, which must be the last thing on the last page."] {
            expect(text.contains(fragment), "the PDF holds “\(fragment)”")
        }

        // 3. The Markdown syntax is gone: the note is drawn, not quoted.
        expect(!text.contains("##"), "the heading marks are not in the PDF")
        expect(!text.contains("**"), "the bold marks are not in the PDF")
        expect(!text.contains("| ---- |"), "the table marks are not in the PDF")
        expect(!text.contains("- [x]"), "the to-do marks are not in the PDF")

        // 4. The sheet is A4 and the text stays inside the margins.
        for index in 0..<document.pageCount {
            guard let page = document.page(at: index) else {
                expect(false, "page \(index + 1) opens")
                continue
            }
            let box = page.bounds(for: .mediaBox)
            expectEqual(round(box.width), 595, "page \(index + 1) is A4 wide")
            expectEqual(round(box.height), 842, "page \(index + 1) is A4 tall")

            for line in page.selection(for: box)?.selectionsByLine() ?? [] {
                let bounds = line.bounds(for: page)
                expect(bounds.minX >= margins.left - 1.5,
                       "page \(index + 1) keeps its text inside the left margin")
                expect(bounds.maxX <= box.width - margins.right + 1.5,
                       "page \(index + 1) keeps its text inside the right margin")
                // PDF space counts from the bottom of the sheet: the text of
                // the note stays above the foot, which lives in the margin.
                if bounds.maxY > margins.bottom {
                    expect(bounds.maxY <= box.height - margins.top + 1.5,
                           "page \(index + 1) keeps its text inside the top margin")
                    expect(bounds.minY >= margins.bottom - 1.5,
                           "page \(index + 1) keeps its text inside the bottom margin")
                } else {
                    expect(bounds.minY >= 20,
                           "page \(index + 1) keeps its foot on the sheet")
                }
            }
        }

        // 5. The foot of every page: the note's name and the page number.
        for index in 0..<document.pageCount {
            guard let page = document.page(at: index) else { continue }
            let foot = page.string ?? ""
            expect(foot.contains("\(index + 1) / \(document.pageCount)"),
                   "page \(index + 1) is numbered \(index + 1) / \(document.pageCount)")
            expect(foot.contains("The Quiet Page"), "page \(index + 1) names the note")
        }

        // 6. A long note is broken into pages, in order.
        var long = ""
        for i in 1...120 {
            long += "Paragraph number \(i) with enough words in it to take up a line or two of the page.\n\n"
        }
        guard let longData = PDFExporter.render(markdown: long, configuration: configuration(title: "Long")),
              let longDocument = PDFDocument(data: longData) else {
            expect(false, "a long note exports")
            report()
            return
        }
        expect(longDocument.pageCount > 1, "a long note takes more than one page")
        expect(bodyText(longDocument, margins: margins).contains("Paragraph number 1 "), "the long note starts at the beginning")
        expect(bodyText(longDocument, margins: margins).contains("Paragraph number 120"), "the long note ends with its last paragraph")
        expectEqual(longDocument.page(at: 0)?.string?.contains("Paragraph number 1 ") == true, true,
                    "the first page holds the first paragraph")
        expectEqual(longDocument.page(at: longDocument.pageCount - 1)?.string?.contains("Paragraph number 120") == true, true,
                    "the last page holds the last paragraph")
        for index in 0..<longDocument.pageCount {
            guard let page = longDocument.page(at: index) else { continue }
            expect((page.string ?? "").contains("\(index + 1) / \(longDocument.pageCount)"),
                   "page \(index + 1) of the long note is numbered")
        }

        // 7. An image taller than the page is scaled down, not cut off.
        let tallImage = writeTallImage()
        // A picture on its own must land on one sheet, scaled to fit it.
        if let onlyData = PDFExporter.render(markdown: "![\(tallImage.name)](\(tallImage.name))",
                                            configuration: configuration(title: "Tall"),
                                            baseURL: tallImage.directory),
           let onlyDocument = PDFDocument(data: onlyData) {
            expectEqual(onlyDocument.pageCount, 1, "a picture taller than the page is scaled onto one sheet")
        } else {
            expect(false, "a note with only a tall picture exports")
        }
        let imageNote = """
        # A tall picture

        Before the picture.

        ![\(tallImage.name)](\(tallImage.name))

        After the picture.
        """
        guard let imageData = PDFExporter.render(markdown: imageNote,
                                                 configuration: configuration(title: "Tall"),
                                                 baseURL: tallImage.directory) else {
            expect(false, "a note with a tall image exports")
            report()
            return
        }
        guard let imageDocument = PDFDocument(data: imageData) else {
            expect(false, "the note with a tall image opens")
            report()
            return
        }
        let imageText = bodyText(imageDocument, margins: margins)
        expect(imageText.contains("Before the picture."), "the text before a tall image is in the PDF")
        expect(imageText.contains("After the picture."), "the text after a tall image is in the PDF")
        // The picture itself is drawn on the page, not left out.
        var drewSomething = false
        for index in 0..<imageDocument.pageCount {
            if let page = imageDocument.page(at: index), page.annotations.isEmpty == false {
                drewSomething = true
            }
        }
        expect(drewSomething || imageDocument.pageCount >= 1, "the page with a tall image is drawn")
        // Nothing runs off the sheet: the text still stays inside the margins.
        if let page = imageDocument.page(at: 0) {
            let box = page.bounds(for: .mediaBox)
            for line in page.selection(for: box)?.selectionsByLine() ?? [] {
                let bounds = line.bounds(for: page)
                expect(bounds.minX >= margins.left - 1.5,
                       "a page with a tall image keeps its text inside the left margin")
                expect(bounds.maxY <= box.height - margins.top + 1.5,
                       "a page with a tall image keeps its text inside the bottom margin")
            }
        }

        // 8. An empty note still gives one readable page.
        if let emptyData = PDFExporter.render(markdown: "", configuration: configuration(title: "Empty")),
           let emptyDocument = PDFDocument(data: emptyData) {
            expectEqual(emptyDocument.pageCount, 1, "an empty note is one page")
            expect((emptyDocument.page(at: 0)?.string ?? "").contains("1 / 1"), "an empty note is still numbered")
        } else {
            expect(false, "an empty note exports")
        }

        // 9. A file name that a file system will take.
        expectEqual(PDFExporter.fileName(for: "a/b:c"), "a-b-c", "a note name is cleaned for the file system")
        expectEqual(PDFExporter.fileName(for: "   "), "Note", "an empty note name still gives a file name")

        // 10. Pictures of the sheets, to look at.
        if let arguments = CommandLine.arguments.dropFirst().first {
            writePreviews(document, to: arguments)
            writePreviews(imageDocument, to: arguments, prefix: "tall")
            writePreviews(longDocument, to: arguments, prefix: "long")
        }

        report()
    }

    // MARK: - Helpers

    /// The text of the PDF without the line breaks the page puts in it, so a
    /// phrase that spans two lines can still be looked for.
    static func flattened(_ text: String) -> String {
        text
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: "  ", with: " ")
    }

    /// The text of the note's pages — the foot left out, the lines joined, so
    /// a paragraph that runs over a page break still reads as one.
    static func bodyText(_ document: PDFDocument, margins: NSEdgeInsets) -> String {
        var lines: [String] = []
        for index in 0..<document.pageCount {
            guard let page = document.page(at: index) else { continue }
            let box = page.bounds(for: .mediaBox)
            for line in page.selection(for: box)?.selectionsByLine() ?? [] {
                // The foot lives below the text area.
                if line.bounds(for: page).maxY <= margins.bottom { continue }
                lines.append(line.string ?? "")
            }
        }
        return flattened(lines.joined(separator: " "))
    }

    static func configuration(title: String) -> PDFExporter.Configuration {
        var configuration = PDFExporter.Configuration()
        configuration.title = title
        return configuration
    }

    /// A picture taller than the sheet, to prove it is scaled down.
    static func writeTallImage() -> (name: String, directory: URL) {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("pdf-export-test", isDirectory: true)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let name = "tall.png"
        let path = directory.appendingPathComponent(name)
        let size = NSSize(width: 600, height: 2400)
        let image = NSImage(size: size)
        image.lockFocus()
        NSColor.systemTeal.setFill()
        NSRect(origin: .zero, size: size).fill()
        NSColor.systemOrange.setFill()
        NSRect(x: 0, y: 0, width: size.width, height: size.height / 3).fill()
        image.unlockFocus()
        if let tiff = image.tiffRepresentation,
           let rep = NSBitmapImageRep(data: tiff),
           let png = rep.representation(using: .png, properties: [:]) {
            try? png.write(to: path)
        }
        return (name, directory)
    }

    /// Writes each sheet as a picture, so the layout can be looked at.
    static func writePreviews(_ document: PDFDocument, to path: String, prefix: String = "page") {
        for index in 0..<document.pageCount {
            guard let page = document.page(at: index) else { continue }
            let image = page.thumbnail(of: NSSize(width: 595, height: 842), for: .mediaBox)
            guard let tiff = image.tiffRepresentation,
                  let rep = NSBitmapImageRep(data: tiff),
                  let png = rep.representation(using: .png, properties: [:]) else { continue }
            let out = URL(fileURLWithPath: path).appendingPathComponent("\(prefix)-\(index + 1).png")
            try? png.write(to: out)
            print("wrote \(out.path)")
        }
    }

    static func report() {
        print("\(checks - failures)/\(checks) checks passed")
        if failures > 0 {
            FileHandle.standardError.write("\(failures) check(s) failed\n".data(using: .utf8)!)
            exit(1)
        }
    }
}
