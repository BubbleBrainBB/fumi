import Foundation

// Standalone test runner for SlashCommand (the slash menu's items and rules).
// SlashCommand is Foundation-only apart from FuzzyMatch, so this compiles
// directly with the sources:
//
//   swiftc Fumi/Views/Editor/SlashCommand.swift Fumi/Services/FuzzySearch.swift \
//     Fumi/Models/FileNote.swift Tests/SlashCommandTests.swift -o /tmp/slash-tests && /tmp/slash-tests

@main
enum SlashCommandTests {
    static var failures = 0

    static func check(_ condition: Bool, _ message: String, line: Int = #line) {
        if !condition {
            failures += 1
            print("FAIL (line \(line)): \(message)")
        }
    }

    static func command(_ id: String) -> SlashCommand {
        SlashCommand.all.first { $0.id == id }!
    }

    static func opens(_ text: String) -> Bool {
        let ns = text as NSString
        let slash = ns.range(of: "/", options: .backwards).location != NSNotFound
            ? ns.range(of: "/", options: .backwards).location
            : ns.range(of: "、", options: .backwards).location
        return SlashCommand.opensMenu(at: slash, in: ns)
    }

    static func edit(_ id: String, _ line: String) -> (String, NSRange) {
        let ns = line as NSString
        let slash = ns.range(of: "/").location != NSNotFound ? ns.range(of: "/").location : ns.range(of: "、").location
        let caret = (line as NSString).range(of: "|").location
        let clean = line.replacingOccurrences(of: "|", with: "")
        let result = command(id).edit(line: clean, query: NSRange(location: slash, length: caret - slash))
        return (result.text, result.selection)
    }

    static func rendered(_ result: (String, NSRange)) -> String {
        let ns = NSMutableString(string: result.0)
        if result.1.length > 0 {
            ns.insert("]", at: NSMaxRange(result.1))
            ns.insert("[", at: result.1.location)
        } else {
            ns.insert("|", at: result.1.location)
        }
        return ns as String
    }

    static func main() {
        // MARK: - When the menu opens

        check(opens("/"), "slash alone")
        check(opens("Hello\n/"), "slash at a line start")
        check(opens("Hello /"), "slash after a space")
        check(opens("- /"), "slash after a list marker")
        check(opens("、"), "the Chinese input method's slash")
        check(!opens("a/"), "slash after a letter")
        check(!opens("1/"), "slash after a digit")
        check(!opens("http:/"), "slash in a URL")
        check(!opens("苹果、"), "、 after Chinese text")
        check(!opens("```\n/"), "inside a code block")
        check(opens("```\ncode\n```\n/"), "after a closed code block")
        check(!opens("$$\n/"), "inside a math block")
        check(opens("$$\nx\n$$\n/"), "after a closed math block")
        check(!opens("---\ntitle: x\n/"), "inside the front matter")
        check(opens("---\ntitle: x\n---\n/"), "after the front matter")
        check(!opens("| a | /"), "in a table's source")
        check(!opens("see `code /"), "inside inline code")

        // MARK: - Filtering

        let all = SlashCommand.results(for: "")
        check(all.count == SlashCommand.all.count, "empty query lists everything")
        check(all.first?.command.id == "text", "menu order kept")

        let bo = SlashCommand.results(for: "bo").map(\.command.id)
        check(bo.first == "bold", "/bo puts Bold first, got \(bo)")
        check(bo.contains("code") && bo.contains("math"), "/bo fuzzy-matches Code Block and Math Block, got \(bo)")

        let h2 = SlashCommand.results(for: "h2").map(\.command.id)
        check(h2.first == "h2", "/h2 finds Heading 2 first, got \(h2)")

        check(SlashCommand.results(for: "db").first?.command.id == "todo", "pinyin initials: db → To-do")
        check(SlashCommand.results(for: "待办").first?.command.id == "todo", "Chinese name: 待办 → To-do")
        check(SlashCommand.results(for: "d'b").first?.command.id == "todo", "pinyin separator ignored")
        check(SlashCommand.results(for: "xyz").isEmpty, "no match")
        check(SlashCommand.results(for: "Bo").first?.titleOffsets == [0, 1], "prefix match marks its letters")

        // MARK: - Picking an item

        check(rendered(edit("h1", "/h1|")) == "# |", "heading on an empty line")
        check(rendered(edit("h2", "Hello /h2|")) == "## Hello |", "heading turns the whole line")
        check(rendered(edit("h2", "/h2|Hello")) == "## |Hello", "caret stays before the line's text")
        check(rendered(edit("h3", "# Title /h3|")) == "### Title |", "replaces an existing heading marker")
        check(rendered(edit("todo", "- item /todo|")) == "- [ ] item |", "list item to to-do")
        check(rendered(edit("bullets", "- [ ] task /ul|")) == "- task |", "to-do to bullet")
        check(rendered(edit("text", "> quoted /text|")) == "quoted |", "Text strips the block marker")
        check(rendered(edit("quote", "  - nested /quote|")) == "  > nested |", "indent kept")
        check(rendered(edit("callout", "> [!tip] hi /callout|")) == "> [!note] hi |", "callout marker replaced")
        check(rendered(edit("code", "/code|")) == "```\n|\n```", "code block on an empty line")
        check(rendered(edit("code", "Hello /code|")) == "Hello \n\n```\n|\n```", "code block below a line with text")
        check(rendered(edit("table", "/table|")) == "| [Column 1] | Column 2 |\n| --- | --- |\n|  |  |", "table selects its first header")
        check(rendered(edit("divider", "/---|")) == "---\n|", "divider")
        check(rendered(edit("bold", "Say /bo|")) == "Say **|**", "bold wraps the caret")
        check(rendered(edit("link", "/link| here")) == "[|](url) here", "link")
        check(rendered(edit("image", "Look /img|")) == "Look |", "image only removes the query")
        check(rendered(edit("todo", "、db|")) == "- [ ] |", "Chinese slash")

        if failures == 0 {
            print("All SlashCommand tests passed")
        } else {
            print("\(failures) failure(s)")
            exit(1)
        }
    }
}
