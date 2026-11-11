//! The slash menu's items and the plain-text rules behind it: when a typed `/`
//! opens the menu, how the text after it filters the items, and what picking
//! one does to the line. Port of `Views/Editor/SlashCommand.swift`, without the
//! `Icon` each item's tile shows: that belongs to the UI, keyed by `id`.
//!
//! `at`, `line_start`, `query` and selections are UTF-8 byte offsets (UTF-16 in
//! Swift). `MenuResult::title_offsets` are grapheme offsets, as in `FuzzyMatch`.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

use crate::fuzzy_search::FuzzyMatch;
use crate::fuzzy_search::foundation::{
    floor_char_boundary, grapheme_count, has_prefix, has_suffix, line_range, lowercased,
    trim_whitespaces, trim_whitespaces_and_newlines,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Basic,
    Blocks,
    Insert,
    Format,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Basic, Group::Blocks, Group::Insert, Group::Format];

    pub fn title(self) -> &'static str {
        match self {
            Group::Basic => "Basic",
            Group::Blocks => "Blocks",
            Group::Insert => "Insert",
            Group::Format => "Format",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Makes the whole line this kind of block: replaces the Markdown prefix
    /// the line already has (`# `, `- `, `> `...) with this one.
    LinePrefix(&'static str),
    /// A block of its own lines; `selection` is where the caret lands (or what
    /// is selected) within `text`.
    Block { text: &'static str, selection: Range<usize> },
    /// Wraps the caret: `prefix|suffix`.
    Inline { prefix: &'static str, suffix: &'static str },
    /// Asks for files, which arrive as on a drop.
    ChooseImage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    pub id: &'static str,
    pub title: &'static str,
    pub group: Group,
    /// The gray hint on the right: the syntax it inserts, or its shortcut.
    pub hint: &'static str,
    /// Also matched when filtering: English synonyms, the Chinese name and its
    /// pinyin initials.
    pub keywords: &'static [&'static str],
    pub action: Action,
}

const fn command(
    id: &'static str,
    title: &'static str,
    group: Group,
    hint: &'static str,
    keywords: &'static [&'static str],
    action: Action,
) -> SlashCommand {
    SlashCommand { id, title, group, hint, keywords, action }
}

/// Every item, in menu order.
pub static ALL: [SlashCommand; 20] = [
    command("text", "Text", Group::Basic, "", &["p", "plain", "paragraph", "正文", "zw"], Action::LinePrefix("")),
    command("h1", "Heading 1", Group::Basic, "#", &["h1", "title", "标题", "bt"], Action::LinePrefix("# ")),
    command("h2", "Heading 2", Group::Basic, "##", &["h2", "标题", "bt"], Action::LinePrefix("## ")),
    command("h3", "Heading 3", Group::Basic, "###", &["h3", "标题", "bt"], Action::LinePrefix("### ")),
    command("bullets", "Bulleted List", Group::Basic, "-", &["ul", "bullet", "列表", "lb"], Action::LinePrefix("- ")),
    command("numbers", "Numbered List", Group::Basic, "1.", &["ol", "number", "编号", "bh"], Action::LinePrefix("1. ")),
    command("todo", "To-do", Group::Basic, "[ ]", &["todo", "task", "check", "待办", "db"], Action::LinePrefix("- [ ] ")),
    command("quote", "Quote", Group::Basic, ">", &["blockquote", "引用", "yy"], Action::LinePrefix("> ")),
    command("callout", "Callout", Group::Blocks, "[!]", &["note", "tip", "warning", "提示", "ts"], Action::LinePrefix("> [!note] ")),
    command("code", "Code Block", Group::Blocks, "```", &["code", "fence", "代码", "dm"], Action::Block { text: "```\n\n```", selection: 4..4 }),
    command("math", "Math Block", Group::Blocks, "$$", &["latex", "formula", "公式", "gs"], Action::Block { text: "$$\n\n$$", selection: 3..3 }),
    command("table", "Table", Group::Blocks, "|", &["grid", "表格", "bg"], Action::Block { text: "| Column 1 | Column 2 |\n| --- | --- |\n|  |  |", selection: 2..10 }),
    command("divider", "Divider", Group::Blocks, "---", &["hr", "line", "rule", "分割线", "fgx"], Action::Block { text: "---\n", selection: 4..4 }),
    command("image", "Image…", Group::Insert, "", &["photo", "picture", "video", "图片", "tp"], Action::ChooseImage),
    command("link", "Link", Group::Insert, "⌘K", &["url", "链接", "lj"], Action::Inline { prefix: "[", suffix: "](url)" }),
    command("bold", "Bold", Group::Format, "⌘B", &["b", "strong", "粗体", "ct"], Action::Inline { prefix: "**", suffix: "**" }),
    command("italic", "Italic", Group::Format, "⌘I", &["i", "em", "斜体", "xt"], Action::Inline { prefix: "*", suffix: "*" }),
    command("strike", "Strikethrough", Group::Format, "~~", &["del", "strike", "删除线", "scx"], Action::Inline { prefix: "~~", suffix: "~~" }),
    command("highlight", "Highlight", Group::Format, "⇧⌘H", &["mark", "高亮", "gl"], Action::Inline { prefix: "==", suffix: "==" }),
    command("inlineCode", "Inline Code", Group::Format, "`", &["code", "行内代码", "hndm"], Action::Inline { prefix: "`", suffix: "`" }),
];

/// One item that matches the typed query, with the grapheme offsets of the
/// matched characters in its title (drawn bold).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuResult<'a> {
    pub command: &'a SlashCommand,
    pub title_offsets: Vec<usize>,
}

/// The items for `query` (the text typed after the slash), best first. An empty
/// query lists everything in menu order. A title that starts with the query
/// ranks first, then a keyword typed in full (`db`, `待办`), then a fuzzy match
/// in the title, then a keyword's start; ties keep menu order.
pub fn results(query: &str) -> Vec<MenuResult<'static>> {
    results_in(query, &ALL)
}

/// `results` over a list other than the whole menu.
pub fn results_in<'a>(query: &str, commands: &'a [SlashCommand]) -> Vec<MenuResult<'a>> {
    let query = query.replace('\'', "");
    if query.is_empty() {
        return commands.iter().map(|command| MenuResult { command, title_offsets: Vec::new() }).collect();
    }
    let lowered = lowercased(&query);
    let mut scored: Vec<(i64, usize, MenuResult<'a>)> = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        if has_prefix(&lowercased(command.title), &lowered) {
            let title_offsets = (0..grapheme_count(&query)).collect();
            scored.push((3000, index, MenuResult { command, title_offsets }));
        } else if command.keywords.iter().any(|k| lowercased(k) == lowered) {
            scored.push((2000, index, MenuResult { command, title_offsets: Vec::new() }));
        } else if let Some(m) = FuzzyMatch::find(&query, command.title) {
            scored.push((1000 + m.score, index, MenuResult { command, title_offsets: m.offsets }));
        } else if command.keywords.iter().any(|k| has_prefix(&lowercased(k), &lowered)) {
            scored.push((500, index, MenuResult { command, title_offsets: Vec::new() }));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, result)| result).collect()
}

/// The characters that open the menu: `/`, and `、`, which a Chinese input
/// method types for the same key.
pub fn is_trigger(character: char) -> bool {
    character == '/' || character == '\u{3001}'
}

/// Whether a slash typed at byte `at` in `text` (raw Markdown) opens the menu:
/// at the start of a line or after a space, and not inside code, math, a
/// table's source or the front matter.
pub fn opens_menu(at: usize, text: &str) -> bool {
    if at >= text.len() || !text.is_char_boundary(at) {
        return false;
    }
    if !text[at..].chars().next().is_some_and(is_trigger) {
        return false;
    }
    let line = line_range(text, at..at);
    if at > line.start {
        let previous = text[..at].chars().next_back();
        if !matches!(previous, Some(' ' | '\t' | '\u{3000}')) {
            return false;
        }
    }
    let before_slash = &text[line.start..at];
    if has_prefix(trim_whitespaces(&text[line.clone()]), "|") {
        return false;
    }
    // An open inline code span: an odd number of backticks before it.
    if before_slash.graphemes(true).filter(|g| *g == "`").count() % 2 == 1 {
        return false;
    }
    !is_inside_block(line.start, text)
}

/// Whether the line starting at byte `line_start` is inside a fenced code block,
/// a `$$` math block or the front matter.
pub fn is_inside_block(line_start: usize, text: &str) -> bool {
    let mut fence: Option<&str> = None;
    let mut in_math = false;
    let mut in_frontmatter = false;
    let mut location = 0;
    let mut line_index = 0;
    let stop = line_start.min(text.len());
    while location < stop {
        let range = line_range(text, location..location);
        let trimmed = trim_whitespaces_and_newlines(&text[range.clone()]);
        if line_index == 0 && trimmed == "---" {
            in_frontmatter = true;
        } else if in_frontmatter {
            if trimmed == "---" || trimmed == "..." {
                in_frontmatter = false;
            }
        } else if let Some(open) = fence {
            if has_prefix(trimmed, open) {
                fence = None;
            }
        } else if has_prefix(trimmed, "```") {
            fence = Some("```");
        } else if has_prefix(trimmed, "~~~") {
            fence = Some("~~~");
        } else if trimmed == "$$" {
            in_math = !in_math;
        } else if has_prefix(trimmed, "$$") && !(grapheme_count(trimmed) > 2 && has_suffix(trimmed, "$$")) {
            in_math = !in_math;
        } else if in_math && has_suffix(trimmed, "$$") {
            in_math = false;
        }
        location = range.end;
        line_index += 1;
    }
    fence.is_some() || in_math || in_frontmatter
}

impl SlashCommand {
    /// What picking this item does to one line: `line` is the line's text (no
    /// line break) and `query` the byte range of the slash and the text typed
    /// after it. Returns the line's new text, which may run over several lines,
    /// and the selection in it.
    pub fn edit(&self, line: &str, query: Range<usize>) -> (String, Range<usize>) {
        let query_end = floor_char_boundary(line, query.end);
        let query_start = floor_char_boundary(line, query.start.min(query_end));
        let before = &line[..query_start];
        let after = &line[query_end..];
        let rest = format!("{before}{after}");
        let caret = before.len();

        match &self.action {
            Action::LinePrefix(prefix) => {
                let (indent, old_prefix) = block_prefix(&rest);
                let content_start = indent + old_prefix;
                let text = format!("{}{}{}", &rest[..indent], prefix, &rest[content_start..]);
                let location = indent + prefix.len() + caret.saturating_sub(content_start);
                (text, location..location)
            }
            Action::Block { text, selection } => {
                if trim_whitespaces(&rest).is_empty() {
                    return ((*text).to_string(), selection.clone());
                }
                // Below the line's own text, after a blank line, so a `---`
                // under a paragraph doesn't turn it into a heading.
                let lead = format!("{rest}\n\n");
                let offset = lead.len();
                (format!("{lead}{text}"), offset + selection.start..offset + selection.end)
            }
            Action::Inline { prefix, suffix } => {
                let location = caret + prefix.len();
                (format!("{before}{prefix}{suffix}{after}"), location..location)
            }
            Action::ChooseImage => (rest, caret..caret),
        }
    }
}

static BLOCK_PREFIXES: LazyLock<[Regex; 6]> = LazyLock::new(|| {
    [
        r"^#{1,6} ",
        r"^> \[![A-Za-z-]+\][+-]? ?",
        r"^[-*+] \[[ xX]\] ",
        r"^[-*+] ",
        r"^\d+[.)] ",
        r"^> ?",
    ]
    .map(|pattern| Regex::new(pattern).expect("valid block prefix pattern"))
});

/// The byte length of a line's leading indentation and of the block marker
/// after it (`### `, `- [ ] `, `1. `, `> [!tip] `...).
pub fn block_prefix(line: &str) -> (usize, usize) {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let body = &line[indent..];
    let prefix = BLOCK_PREFIXES.iter().find_map(|re| re.find(body)).map_or(0, |m| m.end());
    (indent, prefix)
}

#[cfg(test)]
mod tests {
    // Ported from Tests/SlashCommandTests.swift. Offsets in the Swift helpers
    // are UTF-16; here they are bytes, which `edit` and `rendered` agree on.
    use super::*;

    fn command(id: &str) -> &'static SlashCommand {
        ALL.iter().find(|c| c.id == id).unwrap()
    }

    fn opens(text: &str) -> bool {
        let slash = text.rfind('/').or_else(|| text.rfind('、')).unwrap();
        opens_menu(slash, text)
    }

    fn edit(id: &str, line: &str) -> (String, Range<usize>) {
        let slash = line.find('/').or_else(|| line.find('、')).unwrap();
        let caret = line.find('|').unwrap();
        let clean = line.replace('|', "");
        command(id).edit(&clean, slash..caret)
    }

    fn rendered(result: (String, Range<usize>)) -> String {
        let (mut text, selection) = result;
        if !selection.is_empty() {
            text.insert(selection.end, ']');
            text.insert(selection.start, '[');
        } else {
            text.insert(selection.start, '|');
        }
        text
    }

    fn ids(query: &str) -> Vec<&'static str> {
        results(query).iter().map(|r| r.command.id).collect()
    }

    #[test]
    fn when_the_menu_opens() {
        assert!(opens("/"), "slash alone");
        assert!(opens("Hello\n/"), "slash at a line start");
        assert!(opens("Hello /"), "slash after a space");
        assert!(opens("- /"), "slash after a list marker");
        assert!(opens("、"), "the Chinese input method's slash");
        assert!(!opens("a/"), "slash after a letter");
        assert!(!opens("1/"), "slash after a digit");
        assert!(!opens("http:/"), "slash in a URL");
        assert!(!opens("苹果、"), "、 after Chinese text");
        assert!(!opens("```\n/"), "inside a code block");
        assert!(opens("```\ncode\n```\n/"), "after a closed code block");
        assert!(!opens("$$\n/"), "inside a math block");
        assert!(opens("$$\nx\n$$\n/"), "after a closed math block");
        assert!(!opens("---\ntitle: x\n/"), "inside the front matter");
        assert!(opens("---\ntitle: x\n---\n/"), "after the front matter");
        assert!(!opens("| a | /"), "in a table's source");
        assert!(!opens("see `code /"), "inside inline code");
    }

    #[test]
    fn filtering() {
        let all = results("");
        assert_eq!(all.len(), ALL.len(), "empty query lists everything");
        assert_eq!(all.first().map(|r| r.command.id), Some("text"), "menu order kept");

        let bo = ids("bo");
        assert_eq!(bo.first(), Some(&"bold"), "/bo puts Bold first, got {bo:?}");
        assert!(bo.contains(&"code") && bo.contains(&"math"), "/bo fuzzy-matches Code Block and Math Block, got {bo:?}");

        let h2 = ids("h2");
        assert_eq!(h2.first(), Some(&"h2"), "/h2 finds Heading 2 first, got {h2:?}");

        assert_eq!(ids("db").first(), Some(&"todo"), "pinyin initials: db → To-do");
        assert_eq!(ids("待办").first(), Some(&"todo"), "Chinese name: 待办 → To-do");
        assert_eq!(ids("d'b").first(), Some(&"todo"), "pinyin separator ignored");
        assert!(results("xyz").is_empty(), "no match");
        assert_eq!(results("Bo").first().map(|r| r.title_offsets.clone()), Some(vec![0, 1]), "prefix match marks its letters");
    }

    #[test]
    fn picking_an_item() {
        assert_eq!(rendered(edit("h1", "/h1|")), "# |", "heading on an empty line");
        assert_eq!(rendered(edit("h2", "Hello /h2|")), "## Hello |", "heading turns the whole line");
        assert_eq!(rendered(edit("h2", "/h2|Hello")), "## |Hello", "caret stays before the line's text");
        assert_eq!(rendered(edit("h3", "# Title /h3|")), "### Title |", "replaces an existing heading marker");
        assert_eq!(rendered(edit("todo", "- item /todo|")), "- [ ] item |", "list item to to-do");
        assert_eq!(rendered(edit("bullets", "- [ ] task /ul|")), "- task |", "to-do to bullet");
        assert_eq!(rendered(edit("text", "> quoted /text|")), "quoted |", "Text strips the block marker");
        assert_eq!(rendered(edit("quote", "  - nested /quote|")), "  > nested |", "indent kept");
        assert_eq!(rendered(edit("callout", "> [!tip] hi /callout|")), "> [!note] hi |", "callout marker replaced");
        assert_eq!(rendered(edit("code", "/code|")), "```\n|\n```", "code block on an empty line");
        assert_eq!(rendered(edit("code", "Hello /code|")), "Hello \n\n```\n|\n```", "code block below a line with text");
        assert_eq!(rendered(edit("table", "/table|")), "| [Column 1] | Column 2 |\n| --- | --- |\n|  |  |", "table selects its first header");
        assert_eq!(rendered(edit("divider", "/---|")), "---\n|", "divider");
        assert_eq!(rendered(edit("bold", "Say /bo|")), "Say **|**", "bold wraps the caret");
        assert_eq!(rendered(edit("link", "/link| here")), "[|](url) here", "link");
        assert_eq!(rendered(edit("image", "Look /img|")), "Look |", "image only removes the query");
        assert_eq!(rendered(edit("todo", "、db|")), "- [ ] |", "Chinese slash");
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    #[test]
    fn menu_table_has_the_swift_order() {
        let ids: Vec<_> = ALL.iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            [
                "text", "h1", "h2", "h3", "bullets", "numbers", "todo", "quote", "callout", "code", "math", "table",
                "divider", "image", "link", "bold", "italic", "strike", "highlight", "inlineCode",
            ]
        );
    }

    #[test]
    fn selection_offsets_stay_in_bytes_after_cjk() {
        let bold = ALL.iter().find(|c| c.id == "bold").unwrap();
        let line = "你好 /bo世界";
        let slash = line.find('/').unwrap();
        let (text, selection) = bold.edit(line, slash..slash + 3);
        assert_eq!(text, "你好 ****世界");
        assert_eq!(&text[..selection.start], "你好 **");
        let table = ALL.iter().find(|c| c.id == "table").unwrap();
        let (text, selection) = table.edit("中文 /table", "中文 ".len().."中文 /table".len());
        assert_eq!(&text[selection], "Column 1");
    }

    #[test]
    fn opens_menu_with_cjk_and_ideographic_space() {
        let text = "苹果　/";
        assert!(opens_menu(text.find('/').unwrap(), text), "after an ideographic space");
        let text = "😀 、";
        assert!(opens_menu(text.find('、').unwrap(), text), "after an emoji and a space");
        assert!(!opens_menu(1, "😀"), "inside a character is not a slash");
    }

    #[test]
    fn crlf_lines_are_lines() {
        assert!(opens_menu("a\r\n/".len() - 1, "a\r\n/"));
        let text = "---\r\ntitle: x\r\n---\r\n/";
        assert!(opens_menu(text.len() - 1, text), "front matter closed under CRLF");
        let text = "```\r\n/";
        assert!(!opens_menu(text.len() - 1, text), "fence open under CRLF");
    }

    #[test]
    fn a_backtick_with_a_combining_mark_opens_no_code_span() {
        let text = "`\u{301} /";
        assert!(opens_menu(text.len() - 1, text));
    }

    #[test]
    fn prefix_match_offsets_are_graphemes() {
        static EMOJI: [SlashCommand; 1] = [SlashCommand {
            id: "e",
            title: "😀 Smile",
            group: Group::Basic,
            hint: "",
            keywords: &[],
            action: Action::ChooseImage,
        }];
        let found = results_in("😀 s", &EMOJI);
        assert_eq!(found[0].title_offsets, vec![0, 1, 2], "one offset per character, not per UTF-16 unit");
    }

    #[test]
    fn edit_never_slices_inside_a_character() {
        let bold = ALL.iter().find(|c| c.id == "bold").unwrap();
        let (text, _) = bold.edit("中", 1..2);
        assert_eq!(text, "****中");
    }

    #[test]
    fn block_prefix_lengths() {
        assert_eq!(block_prefix("  ### a"), (2, 4));
        assert_eq!(block_prefix("\t- [x] a"), (1, 6));
        assert_eq!(block_prefix("12) a"), (0, 4));
        assert_eq!(block_prefix("> [!tip]- a"), (0, 10));
        assert_eq!(block_prefix(">a"), (0, 1));
        assert_eq!(block_prefix("####### a"), (0, 0));
        assert_eq!(block_prefix("中文"), (0, 0));
    }
}
