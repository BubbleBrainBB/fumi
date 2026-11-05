//! A note's tags and the sidebar's tag tree and search. Port of
//! `Services/TagExtractor.swift` and `Views/Sidebar/TagSearch.swift`.

use crate::frontmatter::text;
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

/// A hashtag; [`extract_raw`] checks what comes before it, which must be the
/// start of the text or whitespace. ICU's `(?m)(?<=^|\s)` comes to the same:
/// every line ending ICU's `^` follows is also `\s`.
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#[A-Za-z][\w/-]*").expect("tag pattern"));
static TOP_LEVEL_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^tags[ \t]*:[ \t]?").expect("tags key pattern"));

/// HTML/CSS/JS (web clippings embed raw SVG/HTML whose id refs must not become
/// tags) and inline code. Order matters: style and script bodies before
/// generic tags, inline code last.
static MASKS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [r"(?is)<style\b[\s\S]*?</style>", r"(?is)<script\b[\s\S]*?</script>", r"<!--[\s\S]*?-->", r"</?[A-Za-z][^>]*>", r"`[^`\n]+`"]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("mask pattern"))
        .collect()
});

/// Raw `#tag` strings, `#` included, from the body of `content`, without
/// duplicates. Code, HTML and hex colours don't count.
pub fn extract_raw(content: &str) -> Vec<String> {
    let masked = masking_code(content);
    let tags = TAG.find_iter(&masked).filter(|m| masked[..m.start()].chars().next_back().is_none_or(char::is_whitespace));
    let mut seen = HashSet::new();
    tags.map(|m| m.as_str())
        .filter(|tag| !is_hex_color(tag))
        .filter(|tag| seen.insert(text::canonical(tag)))
        .map(str::to_string)
        .collect()
}

/// [`extract_raw`] without the `#`.
pub fn extract(content: &str) -> Vec<String> {
    extract_raw(content).into_iter().map(|tag| text::drop_first(&tag, 1).to_string()).collect()
}

/// Tags declared in the note's frontmatter `tags:` field, the sole source of a
/// note's tags. Block sequences (`- a`), flow lists (`[a, b]`), comma or single
/// scalar values, with optional quotes and a leading `#`.
pub fn frontmatter_tags(content: &str) -> Vec<String> {
    let Some(body) = frontmatter_body(content) else { return Vec::new() };
    let mut seen = HashSet::new();
    tag_values(&body).into_iter().map(clean_tag).filter(|tag| !tag.is_empty() && seen.insert(text::canonical(tag))).map(str::to_string).collect()
}

/// The lines between the opening `---` and the next `---`/`...`, or `None`
/// when the content has no leading frontmatter block.
fn frontmatter_body(content: &str) -> Option<Vec<&str>> {
    let mut lines = text::lines(content).map(|line| &content[line.content]);
    if text::trim_ws(lines.next()?) != "---" {
        return None;
    }
    let mut body = Vec::new();
    for line in lines {
        let trimmed = text::trim_ws(line);
        if trimmed == "---" || trimmed == "..." {
            return Some(body);
        }
        body.push(line);
    }
    None
}

fn tag_values<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    for (i, line) in lines.iter().enumerate() {
        let Some(key) = TOP_LEVEL_TAGS.find(line) else { continue };
        let inline = text::trim_ws(&line[key.end()..]);
        if !inline.is_empty() {
            return parse_flow_or_scalar(inline);
        }
        // Block sequence: the `- item` lines after it.
        let mut items = Vec::new();
        for next in &lines[i + 1..] {
            let trimmed = text::trim_ws(next);
            if trimmed.is_empty() {
                continue;
            }
            if !text::has_prefix(trimmed, "-") {
                break; // Next key or dedent ends the list.
            }
            items.push(text::trim_ws(&trimmed[1..]));
        }
        return items;
    }
    Vec::new()
}

fn parse_flow_or_scalar(value: &str) -> Vec<&str> {
    let value = if text::has_prefix(value, "[") && text::has_suffix(value, "]") { &value[1..value.len() - 1] } else { value };
    text::split_on(value, ",", true)
}

fn clean_tag(raw: &str) -> &str {
    let mut s = text::trim_ws(raw);
    for quote in ["\"", "'"] {
        if text::has_prefix(s, quote) && text::has_suffix(s, quote) && text::count(s) >= 2 {
            s = &s[1..s.len() - 1];
        }
    }
    if text::has_prefix(s, "#") {
        s = &s[1..];
    }
    text::trim_ws(s)
}

/// A `#RRGGBB` / `#RRGGBBAA` hex colour. Lengths 3 and 4 are not colours,
/// because words like `#ace` and `#cafe` collide with them. As Swift's
/// `Character.isHexDigit`, full-width digits and letters count.
pub fn is_hex_color(tag: &str) -> bool {
    let body = if text::has_prefix(tag, "#") { &tag[1..] } else { tag };
    let count = text::count(body);
    (count == 6 || count == 8) && body.graphemes(true).all(is_hex_digit)
}

fn is_hex_digit(grapheme: &str) -> bool {
    let mut chars = grapheme.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_ascii_hexdigit() || matches!(c, '\u{FF10}'..='\u{FF19}' | '\u{FF21}'..='\u{FF26}' | '\u{FF41}'..='\u{FF46}'),
        _ => false,
    }
}

/// `content` with fenced code blocks, HTML and inline code spans blanked out,
/// line endings kept (so tags can't appear to start inside them).
fn masking_code(content: &str) -> String {
    let mut fences = Vec::new();
    let mut fence_start = None;
    for line in text::lines(content) {
        let trimmed = text::trim_ws(&content[line.content.clone()]);
        if !(text::has_prefix(trimmed, "```") || text::has_prefix(trimmed, "~~~")) {
            continue;
        }
        match fence_start.take() {
            None => fence_start = Some(line.full.start),
            Some(start) => fences.push(start..line.content.end),
        }
    }
    if let Some(start) = fence_start {
        fences.push(start..content.len());
    }

    let mut masked = String::with_capacity(content.len());
    let mut copied = 0;
    for fence in fences {
        masked.push_str(&content[copied..fence.start]);
        masked.push_str(&blank(&content[fence.clone()]));
        copied = fence.end;
    }
    masked.push_str(&content[copied..]);

    for pattern in MASKS.iter() {
        if let std::borrow::Cow::Owned(replaced) = pattern.replace_all(&masked, |c: &regex::Captures| blank(&c[0])) {
            masked = replaced;
        }
    }
    masked
}

/// One space per `Character`, `\n` and `\r` kept. A `\r\n` is one `Character`
/// that is neither, so it becomes one space, as in the Swift code.
fn blank(s: &str) -> String {
    s.graphemes(true).map(|g| if g == "\n" || g == "\r" { g } else { " " }).collect()
}

/// A node of the tag tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TagNode {
    /// Full path, e.g. `work/project`.
    pub id: String,
    /// Display name, e.g. `project`.
    pub name: String,
    /// Depth in the tree.
    pub level: usize,
    /// Direct note count for this exact tag.
    pub note_count: usize,
    /// Cumulative count including children.
    pub total_count: usize,
    pub children: Vec<TagNode>,
}

/// Builds the tag tree from tag paths, one per note that has the tag. A path
/// may contain `/` for nesting. Siblings are sorted by name.
pub fn build_tree<S: AsRef<str>>(tag_paths: &[S]) -> Vec<TagNode> {
    let mut roots = Vec::new();
    for path in tag_paths {
        insert(&text::split_on(path.as_ref(), "/", false), &mut roots, "");
    }
    roots
}

fn insert(parts: &[&str], siblings: &mut Vec<TagNode>, parent_path: &str) {
    let Some(first) = parts.first() else { return };
    let index = siblings.iter().position(|node| text::canon_eq(&node.name, first)).unwrap_or_else(|| {
        let id = if parent_path.is_empty() { first.to_string() } else { format!("{parent_path}/{first}") };
        let level = text::split_on(parent_path, "/", false).len();
        siblings.push(TagNode { id, name: first.to_string(), level, note_count: 0, total_count: 0, children: Vec::new() });
        siblings.len() - 1
    });
    let node = &mut siblings[index];
    if parts.len() == 1 {
        node.note_count += 1;
    } else {
        let path = node.id.clone();
        insert(&parts[1..], &mut node.children, &path);
    }
    node.total_count += 1;
    siblings.sort_by_cached_key(|node| text::canonical(&node.name));
}

/// All nodes in the tree, each parent before its descendants, in a single list.
pub fn flatten(nodes: &[TagNode]) -> Vec<TagNode> {
    let mut flat = Vec::new();
    for node in nodes {
        flat.push(node.clone());
        flat.extend(flatten(&node.children));
    }
    flat
}

/// Tags whose full path contains `query` (case-insensitive), sorted by path,
/// so a nested match is found even when its parent doesn't match. An empty or
/// whitespace query returns an empty list; the caller shows the tree.
pub fn matches(tree: &[TagNode], query: &str) -> Vec<TagNode> {
    let trimmed = text::trim_ws(query);
    if trimmed.is_empty() {
        return Vec::new();
    }
    let query = Folded::new(trimmed);
    let mut found: Vec<TagNode> = flatten(tree).into_iter().filter(|node| Folded::new(&node.id).contains(&query.text)).collect();
    found.sort_by_cached_key(|node| text::canonical(&node.id));
    found
}

/// A string as `localizedCaseInsensitiveContains` compares it: full case
/// folding (`ß` is `ss`, `ς` is `σ`) and canonical equivalence.
struct Folded {
    text: String,
    /// Byte offsets between the source's scalars. A match can't start or end
    /// inside the fold of one: `ﬁ` doesn't hold `f`, nor `각` hold `가`.
    edges: HashSet<usize>,
}

impl Folded {
    fn new(s: &str) -> Folded {
        let mut text = String::with_capacity(s.len());
        let mut edges = HashSet::from([0]);
        for c in s.nfc() {
            text.extend(fold(c).nfd());
            edges.insert(text.len());
        }
        Folded { text, edges }
    }

    /// Whether `needle` is here, not followed by a mark that belongs on its
    /// last character: `cafe` isn't in `café`, nor `👍` in `👍🏽`. Foundation
    /// looks at nothing before a match, and doesn't count ZWJ as a mark.
    fn contains(&self, needle: &str) -> bool {
        let mut from = 0;
        while let Some(found) = self.text[from..].find(needle) {
            let start = from + found;
            let end = start + needle.len();
            if self.edges.contains(&start) && self.edges.contains(&end) && self.ends_a_character(end) {
                return true;
            }
            from = start + self.text[start..].chars().next().map_or(1, char::len_utf8);
        }
        false
    }

    fn ends_a_character(&self, end: usize) -> bool {
        // Legacy clusters: Foundation doesn't count spacing marks either.
        self.text[end..].starts_with('\u{200D}')
            || GraphemeCursor::new(end, self.text.len(), false).is_boundary(&self.text, 0).unwrap_or(true)
    }
}

/// Upper then lower case gives the full fold, but Foundation folds only the
/// BMP, keeps `ı` apart from `i`, and folds `ẞ` to `ss`.
fn fold(c: char) -> String {
    match c {
        'ı' => c.to_string(),
        'ẞ' => "ss".to_string(),
        c if u32::from(c) > 0xFFFF => c.to_string(),
        c => c.to_uppercase().flat_map(char::to_lowercase).collect(),
    }
}

#[cfg(test)]
mod tests {
    //! Ported from `Tests/TagSearchTests.swift`.
    use super::*;

    fn tree() -> Vec<TagNode> {
        build_tree(&["AI", "AI/编程", "AI/编程/Swift", "AI图像", "AI新闻", "Anthropic", "Bubble日报", "Claude"])
    }

    fn ids(tree: &[TagNode], query: &str) -> Vec<String> {
        matches(tree, query).into_iter().map(|node| node.id).collect()
    }

    #[test]
    fn tag_search() {
        let tree = tree();

        // Empty or whitespace query: no flat results (the caller shows the tree).
        assert_eq!(ids(&tree, ""), Vec::<String>::new(), "empty query returns nothing");
        assert_eq!(ids(&tree, "   "), Vec::<String>::new(), "whitespace query returns nothing");

        // Substring match across the whole path, results sorted by full path.
        assert_eq!(ids(&tree, "AI"), ["AI", "AI/编程", "AI/编程/Swift", "AI图像", "AI新闻"], "matches all paths containing AI, sorted");

        // A nested tag matches by its parent segment even when querying the parent.
        assert_eq!(ids(&tree, "编程"), ["AI/编程", "AI/编程/Swift"], "nested matches found via path segment");

        // Deep leaf matches by its own name.
        assert_eq!(ids(&tree, "Swift"), ["AI/编程/Swift"], "deep leaf matches by name");

        // Case-insensitive.
        assert_eq!(ids(&tree, "claude"), ["Claude"], "case-insensitive match");
        assert_eq!(ids(&tree, "anthropic"), ["Anthropic"], "case-insensitive match 2");

        // No match.
        assert_eq!(ids(&tree, "zzz"), Vec::<String>::new(), "no match returns empty");

        // Query is trimmed before matching.
        assert_eq!(ids(&tree, "  Claude  "), ["Claude"], "query is trimmed");
    }
}

#[cfg(test)]
mod extra_tests {
    //! Cases the Swift suite lacks; `TagExtractor` has no Swift test at all.
    //! Expected values were checked against the Swift code.
    use super::*;

    #[test]
    fn frontmatter_shapes() {
        assert_eq!(frontmatter_tags("---\ntags: [规则, \"llm-wiki\", '#甲', #乙]\n---\n"), ["规则", "llm-wiki", "甲", "乙"]);
        assert_eq!(frontmatter_tags("---\ntags:\n  - 规则\n\n  - \"引用\"\n  -无空格\ntitle: x\n  - 不算\n---"), ["规则", "引用", "无空格"]);
        assert_eq!(frontmatter_tags("---\ntags: a, b ,,c\n---"), ["a", "b", "c"]);
        assert_eq!(frontmatter_tags("---\ntags: 单个\n---"), ["单个"]);
        assert_eq!(frontmatter_tags("---\ntags : x\n---"), ["x"], "space before the colon");
        assert_eq!(frontmatter_tags("---\ntags:\tx\n---"), ["x"]);
        assert_eq!(frontmatter_tags("---\n  tags: x\n---"), Vec::<String>::new(), "only a top-level key");
        assert_eq!(frontmatter_tags("---\nTags: x\n---"), Vec::<String>::new(), "case-sensitive");
        assert_eq!(frontmatter_tags("---\nmytags: x\n---"), Vec::<String>::new());
        assert_eq!(frontmatter_tags("---\ntags: x, x, \"x\"\n---"), ["x"], "no duplicates");
        assert_eq!(frontmatter_tags("---\ntags: \n---"), Vec::<String>::new());
        assert_eq!(frontmatter_tags("---\ntags: []\n---"), Vec::<String>::new());
        assert_eq!(frontmatter_tags("---\ntags: x\ntags: y\n---"), ["x"], "the first key wins");
        assert_eq!(frontmatter_tags("---\ntags: '\"a\"'\n---"), ["\"a\""], "the outer quotes go, `\"` is tried first");
        assert_eq!(frontmatter_tags("---\ntags: \"'a'\"\n---"), ["a"], "`\"` first, then `'`");
    }

    #[test]
    fn frontmatter_at_file_boundaries() {
        assert_eq!(frontmatter_tags(""), Vec::<String>::new());
        assert_eq!(frontmatter_tags("---"), Vec::<String>::new());
        assert_eq!(frontmatter_tags("---\ntags: [a]"), Vec::<String>::new(), "no closing fence");
        assert_eq!(frontmatter_tags("---\ntags: [a]\n"), Vec::<String>::new(), "no closing fence, final newline");
        assert_eq!(frontmatter_tags("---\ntags: [a]\n---"), ["a"], "closing fence ends the file");
        assert_eq!(frontmatter_tags("---\ntags: [a]\n..."), ["a"]);
        assert_eq!(frontmatter_tags("  ---  \ntags: [a]\n  ---  "), ["a"], "unlike `Frontmatter::parse`, an indented fence opens");
        assert_eq!(frontmatter_tags("\u{FEFF}---\ntags: [a]\n---\n"), Vec::<String>::new(), "a BOM hides the block");
        let long = format!("---\n{}tags: [a]\n---", "k: v\n".repeat(500));
        assert_eq!(frontmatter_tags(&long), ["a"], "no line cap here, unlike `Frontmatter::parse`");
    }

    #[test]
    fn frontmatter_in_crlf_and_other_endings() {
        assert_eq!(frontmatter_tags("---\r\ntags: [甲, 乙]\r\n---\r\n"), ["甲", "乙"]);
        assert_eq!(frontmatter_tags("---\r\ntags:\r\n  - 甲\r\n  - 乙\r\n---\r\n正文"), ["甲", "乙"]);
        assert_eq!(frontmatter_tags("---\rtags: a\r---\r"), ["a"]);
        assert_eq!(frontmatter_tags("---\u{2028}tags: a\u{2029}---\u{85}"), ["a"]);
    }

    #[test]
    fn frontmatter_with_emoji_and_marks() {
        assert_eq!(frontmatter_tags("---\ntags: [😀, 👨\u{200D}👩\u{200D}👧, 🇨🇳]\n---"), ["😀", "👨\u{200D}👩\u{200D}👧", "🇨🇳"]);
        let keycap = "#\u{FE0F}\u{20E3}";
        assert_eq!(frontmatter_tags(&format!("---\ntags: {keycap}话题\n---")), [format!("{keycap}话题")], "a keycap keeps its #");
        assert_eq!(frontmatter_tags("---\ntags: [a,\u{301}b]\n---"), ["a,\u{301}b"], "a marked comma doesn't split");
        assert_eq!(frontmatter_tags("---\ntags: [caf\u{E9}, cafe\u{301}]\n---"), ["caf\u{E9}"], "the same tag in another normalization");
    }

    #[test]
    fn inline_hashtags() {
        assert_eq!(extract_raw("#work and #work/project-a\n#life"), ["#work", "#work/project-a", "#life"]);
        assert_eq!(extract("#work #work"), ["work"]);
        assert_eq!(extract("#a中文 #中文 x#y (#z) 中文 #b"), ["a中文", "b"], "must start with an ASCII letter after whitespace");
        assert_eq!(extract("#FFFFFF #C7C7C7AA #ace #cafe #abcdefg"), ["ace", "cafe", "abcdefg"]);
        assert_eq!(extract("\u{3000}#a\u{2028}#b\u{A0}#c\r\n#d\r#e"), ["a", "b", "c", "d", "e"]);
        assert_eq!(extract("\u{200B}#a \u{FEFF}#b"), Vec::<String>::new(), "U+200B and U+FEFF aren't `\\s`");
        assert_eq!(extract("#a😀"), ["a"], "an emoji isn't a word character");
        assert_eq!(extract("#e\u{301}x"), ["e\u{301}x"], "a mark is");
    }

    #[test]
    fn what_is_masked() {
        assert_eq!(extract("```\n#a\n```\n#b"), ["b"]);
        assert_eq!(extract("```\r\n#a\r\n```\r\n#b\r\n"), ["b"]);
        assert_eq!(extract("~~~\n#a\n```\n#b"), ["b"], "any fence closes any fence");
        assert_eq!(extract("```\n#a"), Vec::<String>::new(), "open to the end");
        assert_eq!(extract("`#a` #b"), ["b"]);
        assert_eq!(extract("<style>\n#a { color: red }\n</style> #b"), ["b"]);
        assert_eq!(extract("<STYLE>\r\n#a\r\n</Style> #b"), ["b"]);
        assert_eq!(extract("<script>\n#a\n</script> #b"), ["b"]);
        assert_eq!(extract("<!-- #a --> #b"), ["b"]);
        assert_eq!(extract("<svg fill=\"url(#a)\" x=' #b'> #c"), ["c"]);
        assert_eq!(extract("1 < 2 #a > 3"), ["a"], "`<` needs a letter after it");
    }

    #[test]
    fn hex_colors() {
        assert!(is_hex_color("#FFFFFF"));
        assert!(is_hex_color("abcdef12"));
        assert!(is_hex_color("#\u{FF11}\u{FF12}\u{FF13}\u{FF41}\u{FF42}\u{FF43}"), "Swift's isHexDigit takes full-width");
        assert!(!is_hex_color("#abc"));
        assert!(!is_hex_color("#abcde\u{301}f"));
        assert!(!is_hex_color("#gggggg"));
        assert!(!is_hex_color("#١٢٣٤٥٦"), "other scripts' digits aren't hex");
    }

    #[test]
    fn tree_counts_and_levels() {
        let tree = build_tree(&["甲/乙", "甲/乙", "甲", "甲/丙/丁", "a//b/", "/"]);
        assert_eq!(tree.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["a", "甲"]);
        let a = &tree[0];
        assert_eq!((a.note_count, a.total_count, a.level), (0, 1, 0));
        assert_eq!(a.children[0].id, "a/b");
        let jia = &tree[1];
        assert_eq!((jia.note_count, jia.total_count), (1, 4));
        let names: Vec<&str> = jia.children.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["丙", "乙"], "by scalar");
        assert_eq!((jia.children[1].note_count, jia.children[1].total_count, jia.children[1].level), (2, 2, 1));
        assert_eq!((jia.children[0].children[0].id.as_str(), jia.children[0].children[0].level), ("甲/丙/丁", 2));
        assert_eq!(build_tree::<&str>(&[]), []);
        let flat: Vec<String> = flatten(&tree).into_iter().map(|n| n.id).collect();
        assert_eq!(flat, ["a", "a/b", "甲", "甲/丙", "甲/丙/丁", "甲/乙"], "parents before their children");
    }

    #[test]
    fn tree_sorts_like_swift_strings() {
        let tree = build_tree(&["😀", "\u{FF21}", "Z", "中", "caf\u{E9}", "cafe\u{301}", "cafg"]);
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Z", "cafg", "caf\u{E9}", "中", "\u{FF21}", "😀"], "the same name in another form is the same node");
        assert_eq!(tree[2].note_count, 2);
    }

    #[test]
    fn search_folds_as_foundation_does() {
        let hit = |id: &str, query: &str| !matches(&build_tree(&[id]), query).is_empty();
        assert!(hit("STRASSE", "straße"));
        assert!(hit("straße", "STRASSE"));
        assert!(hit("ΣΑΣ", "σας"));
        assert!(hit("cafe\u{301}", "caf\u{E9}"));
        assert!(hit("caf\u{E9}", "cafe\u{301}"));
        assert!(hit("ﬀ", "ff"));
        assert!(hit("\u{212A}", "k"));
        assert!(hit("가", "\u{1100}\u{1161}"));
        assert!(hit("编程", "编"));
        assert!(!hit("cafe\u{301}", "cafe"), "a marked letter is another letter");
        assert!(!hit("caf\u{E9}", "cafe"));
        assert!(!hit("ＡＢＣ", "abc"), "width counts");
        assert!(!hit("İ", "i"));
        assert!(!hit("ı", "I"));
        assert!(!hit("👍🏽", "👍"), "a skin tone is part of the emoji");
        assert!(!hit("a\u{FEFF}b", "ab"));
        assert!(hit("😀话题", "😀"));
        assert!(hit("👨\u{200D}👩", "👨"), "ZWJ is no mark to Foundation");
        assert!(hit("👨\u{200D}👩", "👩"), "and nothing before a match is looked at");
        assert!(hit("🇨🇳", "🇳"));
        assert!(hit("क\u{93E}", "क"), "a spacing mark doesn't count either");
        assert!(hit("ẞ", "ss"));
        assert!(hit("sß", "ss"));
        assert!(hit("ß", "ẞ"));
        assert!(!hit("ﬃ", "ff"), "a match can't end inside one letter's fold");
        assert!(!hit("ﬁ", "f"));
        assert!(!hit("ǰ", "J"));
        assert!(!hit("각", "가"), "nor inside a syllable");
        assert!(!hit("각", "\u{1100}"));
        assert!(!hit("가", "\u{1161}"));
        assert!(!hit("a\u{FE0F}", "a"));
        assert!(!hit("#\u{FE0F}\u{20E3}", "#"));
        assert!(!hit("𐐀", "𐐨"), "no fold past the BMP");
        assert_eq!(matches(&build_tree(&["甲"]), "\u{3000}甲\u{200B}").len(), 1, "the query is trimmed of Foundation's whitespace");
    }
}
