//! Keeps a moved note's links on the same files. Pasted images and videos are
//! linked relative to the note's folder (`../attachments/…` from a subfolder),
//! so a note that changes folder can lose them. Whether a file is there is
//! asked of `exists`. Port of `Services/LinkRewrite.swift`.
//!
//! A link is read the way the editor's `ImageLoader` reads it: relative to the
//! note's folder first, then to the library. Only links that found a file
//! before the move and would now find another one (or none) are rewritten.
//! URLs, absolute and `~` paths, broken links, bare-name embeds and anything
//! in code stay as typed, and every byte outside a rewritten destination
//! stays as it is.

use crate::frontmatter::text;
use crate::inline_link;
use crate::library_move;
use regex::Regex;
use std::ops::Range;
use std::sync::LazyLock;
use unicode_segmentation::UnicodeSegmentation;

/// Obsidian embed: `![[file.ext]]`, `![[folder/file|300]]`.
static EMBED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[\[([^\]\n]+)\]\]").expect("embed pattern"));
/// A URL scheme: `https:`, `mailto:`, `file:`.
static SCHEME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").expect("scheme pattern"));
static INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^`\n]+`").expect("inline code pattern"));

/// `content` of the note at `note_path` after the item at `moved_from` (the
/// note itself or a folder holding it) moved to `moved_to`. `exists` sees the
/// library after the move. Returns `content` unchanged when no link needs to
/// change, or when the move doesn't touch the note.
pub fn rewrite(content: &str, note_path: &str, moved_from: &str, moved_to: &str, library: &str, exists: impl Fn(&str) -> bool) -> String {
    let Some(new_note) = library_move::remap(note_path, moved_from, moved_to) else {
        return content.to_string();
    };
    if text::canon_eq(&new_note, note_path) || !(text::contains(content, "](") || text::contains(content, "![[")) {
        return content.to_string();
    }
    let context = Context {
        old_folder: text::deleting_last_path_component(note_path),
        new_folder: text::deleting_last_path_component(&new_note),
        library,
        moved_from,
        moved_to,
        exists,
    };

    let skipped = code_ranges(content);
    let in_code = |start: usize| skipped.iter().any(|r| r.contains(&start));
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for captures in inline_link::any_pattern().captures_iter(content) {
        let whole = captures.get(0).expect("group 0");
        if in_code(whole.start()) {
            continue;
        }
        let destination = inline_link::destination_range(&captures);
        let is_image = whole.as_str().starts_with('!');
        if let Some(new) = rewritten_target(&content[destination.clone()], is_image, inline_link::is_angled(&captures), &context) {
            edits.push((destination, new));
        }
    }
    for captures in EMBED.captures_iter(content) {
        let whole = captures.get(0).expect("group 0");
        if in_code(whole.start()) {
            continue;
        }
        let inner = captures.get(1).expect("group 1");
        if let Some(new) = rewritten_embed(inner.as_str(), &context) {
            edits.push((inner.range(), new));
        }
    }
    apply(content, edits)
}

/// Splices `edits` in. Two can only overlap for a file literally named like an
/// embed, `[a](![[b/c.png]])`; the Swift code would garble the text there, so
/// the later one is left out.
fn apply(content: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(range, _)| range.start);
    let mut result = String::with_capacity(content.len());
    let mut copied = 0;
    for (range, new) in edits {
        if range.start < copied {
            continue;
        }
        result.push_str(&content[copied..range.start]);
        result.push_str(&new);
        copied = range.end;
    }
    result.push_str(&content[copied..]);
    result
}

/// `file`'s path from `directory`, with `../` where needed. Both absolute.
pub fn relative_path(directory: &str, file: &str) -> String {
    let from = components(directory);
    let to = components(file);
    let mut shared = 0;
    while shared < from.len().min(to.len()) && text::canon_eq(from[shared], to[shared]) {
        shared += 1;
    }
    let mut parts = vec![".."; from.len() - shared];
    parts.extend_from_slice(&to[shared..]);
    parts.join("/")
}

/// `relative` from `base`, with `.` and `..` worked out.
pub fn join(base: &str, relative: &str) -> String {
    let mut parts = components(base);
    for part in components(relative) {
        match part {
            "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    format!("/{}", parts.join("/"))
}

struct Context<'a, F> {
    old_folder: &'a str,
    new_folder: &'a str,
    library: &'a str,
    moved_from: &'a str,
    moved_to: &'a str,
    exists: F,
}

impl<F: Fn(&str) -> bool> Context<'_, F> {
    /// Whether a file was at `path` before the move.
    fn existed_before(&self, path: &str) -> bool {
        if let Some(now) = library_move::remap(path, self.moved_from, self.moved_to) {
            return (self.exists)(&now);
        }
        // Moves never overwrite, so nothing was where the item landed.
        if library_move::is_same_or_inside(path, self.moved_to) {
            return false;
        }
        (self.exists)(path)
    }

    /// Where the file once at `path` is now.
    fn now(&self, path: &str) -> String {
        library_move::remap(path, self.moved_from, self.moved_to).unwrap_or_else(|| path.to_string())
    }

    /// The file `relative` points at now, if it moved, when it no longer
    /// reaches it from the note's new folder.
    fn new_target(&self, relative: &str) -> Option<String> {
        let in_library = join(self.library, relative);
        let before = [join(self.old_folder, relative), in_library.clone()].into_iter().find(|p| self.existed_before(p))?;
        let target = self.now(&before);
        let after = [join(self.new_folder, relative), in_library].into_iter().find(|p| (self.exists)(p));
        match after {
            Some(after) if text::canon_eq(&after, &target) => None,
            _ => Some(target),
        }
    }
}

/// The new destination of `[…](destination)` (inside the brackets when
/// written `<…>`), or `None` to leave it.
fn rewritten_target<F: Fn(&str) -> bool>(destination: &str, is_image: bool, angled: bool, context: &Context<F>) -> Option<String> {
    // A link's `#section` is not part of the file; an image's whole target is
    // a path, as `ImageLoader` reads it.
    let (raw, fragment) = match text::find_char(destination, "#") {
        Some(hash) if !is_image => destination.split_at(hash),
        _ => (destination, ""),
    };
    if !is_relative_local(raw) {
        return None;
    }
    let decoded = removing_percent_encoding(raw).unwrap_or_else(|| raw.to_string());
    let target = context.new_target(&decoded)?;
    let relative = relative_path(context.new_folder, &target);
    Some(format!("{}{fragment}", if angled { relative } else { encoded(&relative) }))
}

/// The new inside of `![[ … ]]`, or `None` to leave it. Only embeds written
/// with a folder change; a bare name is found wherever the file is.
fn rewritten_embed<F: Fn(&str) -> bool>(inner: &str, context: &Context<F>) -> Option<String> {
    let cut = inner.grapheme_indices(true).find(|(_, g)| *g == "|" || *g == "#").map_or(inner.len(), |(i, _)| i);
    let name = text::trim_ws(&inner[..cut]);
    if !text::contains(name, "/") || !is_relative_local(name) {
        return None;
    }
    let target = context.new_target(name)?;
    Some(format!("{}{}", relative_path(context.new_folder, &target), &inner[cut..]))
}

fn is_relative_local(path: &str) -> bool {
    !path.is_empty()
        && !text::has_prefix(path, "/")
        && !text::has_prefix(path, "~")
        && !text::has_prefix(path, "#")
        && !SCHEME.is_match(path)
}

/// `String.removingPercentEncoding`: `None` for a `%` without two hex digits,
/// or for bytes that aren't UTF-8.
fn removing_percent_encoding(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let high = char::from(*bytes.get(i + 1)?).to_digit(16)?;
            let low = char::from(*bytes.get(i + 2)?).to_digit(16)?;
            out.push((high * 16 + low) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Only what would break the link is encoded, as pasting writes it.
fn encoded(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    for grapheme in path.graphemes(true) {
        match grapheme {
            "%" => result.push_str("%25"),
            " " => result.push_str("%20"),
            "(" => result.push_str("%28"),
            ")" => result.push_str("%29"),
            "#" => result.push_str("%23"),
            other => result.push_str(other),
        }
    }
    result
}

/// Fenced code blocks and inline code spans, where links are only text.
fn code_ranges(content: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut fence: Option<(usize, &'static str)> = None;
    for line in text::lines(content) {
        // The ending stays on, as in `lineRange(for:)`: it isn't whitespace.
        let trimmed = text::trim_ws(&content[line.full.clone()]);
        match fence {
            Some((start, marker)) => {
                if text::has_prefix(trimmed, marker) {
                    ranges.push(start..line.full.end);
                    fence = None;
                }
            }
            None => {
                if text::has_prefix(trimmed, "```") {
                    fence = Some((line.full.start, "```"));
                } else if text::has_prefix(trimmed, "~~~") {
                    fence = Some((line.full.start, "~~~"));
                }
            }
        }
    }
    if let Some((start, _)) = fence {
        ranges.push(start..content.len());
    }
    ranges.extend(INLINE_CODE.find_iter(content).map(|m| m.range()));
    ranges
}

fn components(path: &str) -> Vec<&str> {
    text::split_on(path, "/", false)
}

#[cfg(test)]
mod tests {
    //! Ported from `Tests/LinkRewriteTests.swift`.
    use super::*;
    use std::collections::HashSet;

    /// The library before any move.
    pub(super) const FILES: &[&str] = &[
        "/ws/attachments/a.png",
        "/ws/attachments/b c.png",
        "/ws/attachments/clip.mov",
        "/ws/doc.pdf",
        "/ws/Notes.md",
        "/ws/notes/x.png",
        "/ws/Travel/Kyoto.md",
        "/ws/Travel/cover.jpg",
        "/ws/Travel/my pic.png",
        "/ws/Travel/a#1.png",
        "/ws/Travel/截图 (1).png",
        "/ws/attachments/视频生成节点-1 (1).mp4",
        "/ws/Travel/attachments/local.png",
        "/ws/Travel/notes/x.png",
    ];

    /// `content` of the note at `note` after `from` moved to `to`, with the
    /// library's files moved too.
    pub(super) fn rewrite_as(content: &str, note: &str, from: &str, to: &str) -> String {
        let after: HashSet<String> = FILES.iter().map(|f| library_move::remap(f, from, to).unwrap_or_else(|| f.to_string())).collect();
        rewrite(content, note, from, to, "/ws", |p| after.contains(p))
    }

    /// The default move: the note up to the root.
    pub(super) fn up(content: &str) -> String {
        rewrite_as(content, "/ws/Travel/Kyoto.md", "/ws/Travel/Kyoto.md", "/ws/Kyoto.md")
    }

    #[test]
    fn paths() {
        assert_eq!(relative_path("/ws/Travel", "/ws/attachments/a.png"), "../attachments/a.png", "up and over");
        assert_eq!(relative_path("/ws", "/ws/Travel/cover.jpg"), "Travel/cover.jpg", "down");
        assert_eq!(relative_path("/ws/Travel", "/ws/Travel/cover.jpg"), "cover.jpg", "same folder");
        assert_eq!(join("/ws/Travel", "../attachments/./a.png"), "/ws/attachments/a.png", "dots worked out");
    }

    #[test]
    fn a_note_moved_up_to_the_root() {
        assert_eq!(up("![](../attachments/a.png)"), "![](attachments/a.png)", "pasted image from a subfolder");
        assert_eq!(up("![](cover.jpg)"), "![](Travel/cover.jpg)", "image beside the note stays behind");
        assert_eq!(up("![](attachments/local.png)"), "![](Travel/attachments/local.png)", "note-relative attachments folder");
        assert_eq!(up("![](attachments/a.png)"), "![](attachments/a.png)", "library-relative link still resolves");
        assert_eq!(up("![](notes/x.png)"), "![](Travel/notes/x.png)", "same path at the new folder is another file");
        assert_eq!(up("![](../attachments/b%20c.png)"), "![](attachments/b%20c.png)", "encoded space kept encoded");
        assert_eq!(up("![](<../attachments/b c.png>)"), "![](<attachments/b c.png>)", "angle brackets kept");
        assert_eq!(up("![](my pic.png)"), "![](Travel/my%20pic.png)", "unencoded space");
        assert_eq!(up("![](a#1.png)"), "![](Travel/a%231.png)", "an image's # is part of its name");
        assert_eq!(up("![Cover](cover.jpg \"The cover\")"), "![Cover](Travel/cover.jpg \"The cover\")", "alt and title kept");
        assert_eq!(up("![](../attachments/clip.mov)"), "![](attachments/clip.mov)", "video");
        assert_eq!(up("![](截图 (1).png)"), "![](Travel/截图%20%281%29.png)", "parentheses in the name");
        assert_eq!(
            up("![](../attachments/视频生成节点-1 (1).mp4 \"Clip\")"),
            "![](attachments/视频生成节点-1%20%281%29.mp4 \"Clip\")",
            "video with parentheses and a title"
        );
        assert_eq!(up("![](<截图 (1).png>)"), "![](<Travel/截图 (1).png>)", "parentheses inside <…>");
        assert_eq!(up("![](cover.jpg) ![](截图 (1).png)"), "![](Travel/cover.jpg) ![](Travel/截图%20%281%29.png)", "two images on one line");
        assert_eq!(up("[Report](../doc.pdf)"), "[Report](doc.pdf)", "PDF link");
        assert_eq!(up("See [notes](../Notes.md#todo)."), "See [notes](Notes.md#todo).", "link keeps its #section");
        assert_eq!(up("[top](Kyoto.md#start)"), "[top](Kyoto.md#start)", "link to itself still resolves");
        assert_eq!(up("![[../attachments/a.png|300]]"), "![[attachments/a.png|300]]", "embed with a folder");
        assert_eq!(up("![[cover.jpg]]"), "![[cover.jpg]]", "bare-name embed is found by name");
    }

    #[test]
    fn left_as_written() {
        let untouched = [
            "![](https://example.com/a.png)",
            "[Site](https://example.com/page)",
            "[Mail](mailto:me@example.com)",
            "![](file:///Users/me/a.png)",
            "![](/Users/me/Pictures/a.png)",
            "![](~/Pictures/a.png)",
            "[Jump](#heading)",
            "![](missing.png)",
            "[[Notes]]",
            "No links here.",
        ];
        for text in untouched {
            assert_eq!(up(text), text, "untouched: {text}");
        }
        assert_eq!(up("Use `![](cover.jpg)` to embed."), "Use `![](cover.jpg)` to embed.", "inline code");
        let fenced = "```md\n![](cover.jpg)\n```\n![](cover.jpg)";
        assert_eq!(up(fenced), "```md\n![](cover.jpg)\n```\n![](Travel/cover.jpg)", "fenced code");
    }

    #[test]
    fn a_whole_document() {
        let doc = "# Kyoto\n\n![](../attachments/a.png)\n\nText with [a link](https://kyoto.jp) and ![](cover.jpg).\n";
        assert_eq!(
            up(doc),
            "# Kyoto\n\n![](attachments/a.png)\n\nText with [a link](https://kyoto.jp) and ![](Travel/cover.jpg).\n",
            "several links in one note"
        );
    }

    #[test]
    fn other_moves() {
        assert_eq!(
            rewrite_as("![](attachments/a.png) ![](doc.pdf)", "/ws/Kyoto.md", "/ws/Kyoto.md", "/ws/Archive/2025/Kyoto.md"),
            "![](attachments/a.png) ![](doc.pdf)",
            "moved deeper: library-relative still resolves"
        );
        assert_eq!(
            rewrite_as("![](../attachments/a.png)", "/ws/Travel/Kyoto.md", "/ws/Travel/Kyoto.md", "/ws/Archive/Kyoto.md"),
            "![](../attachments/a.png)",
            "same depth elsewhere still resolves"
        );
        assert_eq!(
            rewrite_as("![](cover.jpg) ![](../attachments/a.png)", "/ws/Travel/Kyoto.md", "/ws/Travel", "/ws/Archive/Travel"),
            "![](cover.jpg) ![](../../attachments/a.png)",
            "folder move: what moved along stays, what's outside follows"
        );
        assert_eq!(
            rewrite_as("![](cover.jpg)", "/ws/Notes.md", "/ws/Travel/Kyoto.md", "/ws/Kyoto.md"),
            "![](cover.jpg)",
            "a note the move doesn't touch"
        );
    }
}

#[cfg(test)]
mod extra_tests {
    //! Cases the Swift suite lacks. Expected values were checked against the
    //! Swift code unless a comment says the port differs on purpose.
    use super::tests::up;
    use super::*;

    /// Everything around `link` must come back byte for byte while `link`
    /// becomes `rewritten`, whatever surrounds it.
    fn assert_only_link_changes(before: &str, link: &str, rewritten: &str, after: &str) {
        let input = format!("{before}{link}{after}");
        let output = up(&input);
        assert_eq!(output.as_bytes(), format!("{before}{rewritten}{after}").as_bytes(), "{input:?}");
        assert_eq!(output.as_bytes()[..before.len()], *before.as_bytes());
        assert_eq!(output.as_bytes()[output.len() - after.len()..], *after.as_bytes());
    }

    #[test]
    fn everything_outside_the_link_stays_byte_identical() {
        let surroundings = [
            ("", ""),
            ("", "no final newline"),
            ("前\r\n\r\n", "\r\n后\r\n"),
            ("前\r\n", " 后 no final newline"),
            ("\u{FEFF}", "\n"),
            ("\u{FEFF}---\r\ntitle: 甲\r\n---\r\n", "\r\n"),
            ("---\ntags: [a]\n---\n", ""),
            ("行\u{2028}分隔\u{2029}段\u{85}", "\u{2028}"),
            ("\u{A0}\u{3000}\u{200B}\t  ", "  \t\u{3000}\u{A0}   \n\n\n"),
            ("😀🇨🇳👨\u{200D}👩\u{200D}👧 e\u{301} caf\u{E9} ", " 🎉\u{FE0F}"),
            ("\r", "\r"),
            ("\u{B}\u{C}\0", "\0\u{C}\u{B}"),
            ("#\u{FE0F}\u{20E3} [未动](https://x.y) ![未动](missing.png) ", " `![](cover.jpg)`"),
        ];
        let links = [
            ("![](../attachments/a.png)", "![](attachments/a.png)"),
            ("![图 😀](cover.jpg \"题 🎉\")", "![图 😀](Travel/cover.jpg \"题 🎉\")"),
            ("[文](../Notes.md#节 😀)", "[文](Notes.md#节 😀)"),
            ("![](<截图 (1).png>)", "![](<Travel/截图 (1).png>)"),
            ("![](截图 (1).png)", "![](Travel/截图%20%281%29.png)"),
            ("![[../attachments/a.png|300]]", "![[attachments/a.png|300]]"),
            ("![]( \t../attachments/a.png\t )", "![]( \tattachments/a.png\t )"),
        ];
        for (before, after) in surroundings {
            for (link, rewritten) in links {
                assert_only_link_changes(before, link, rewritten, after);
            }
        }
    }

    #[test]
    fn a_note_with_nothing_to_rewrite_comes_back_whole() {
        for content in [
            "",
            "\u{FEFF}",
            "\r\n\r\n",
            "无链接\r\n无尾换行",
            "\u{FEFF}---\r\na: 1\r\n---\r\n![](missing.png)\r\n",
            "![](https://x.y/a.png)\u{2028}[a](mailto:b@c.d)\u{2029}",
            "![](%E4%B8%AD.png) ![](100%.png)",
        ] {
            assert_eq!(up(content).as_bytes(), content.as_bytes(), "{content:?}");
        }
    }

    #[test]
    fn crlf_code_is_still_code() {
        let fenced = "```md\r\n![](cover.jpg)\r\n```\r\n![](cover.jpg)\r\n";
        assert_eq!(up(fenced), "```md\r\n![](cover.jpg)\r\n```\r\n![](Travel/cover.jpg)\r\n");
        let unclosed = "![](cover.jpg)\r\n~~~\r\n![](cover.jpg)\r\n";
        assert_eq!(up(unclosed), "![](Travel/cover.jpg)\r\n~~~\r\n![](cover.jpg)\r\n", "an open fence runs to the end");
        let other_marker = "```\n~~~\n![](cover.jpg)\n```\n![](cover.jpg)";
        assert_eq!(up(other_marker), "```\n~~~\n![](cover.jpg)\n```\n![](Travel/cover.jpg)", "only its own marker closes it");
        let indented = "  ```  \r\n![](cover.jpg)\r\n\u{3000}```\r\n![](cover.jpg)";
        assert_eq!(up(indented), "  ```  \r\n![](cover.jpg)\r\n\u{3000}```\r\n![](Travel/cover.jpg)");
        let u2028 = "```\u{2028}![](cover.jpg)\u{2028}```\u{2028}![](cover.jpg)";
        assert_eq!(up(u2028), "```\u{2028}![](cover.jpg)\u{2028}```\u{2028}![](Travel/cover.jpg)", "U+2028 ends a fence line");
    }

    #[test]
    fn code_counts_only_at_the_start_of_a_match() {
        assert_eq!(up("[`甲`](cover.jpg)"), "[`甲`](Travel/cover.jpg)", "code inside the link text");
        assert_eq!(up("`![](cover.jpg)` ![](cover.jpg)"), "`![](cover.jpg)` ![](Travel/cover.jpg)");
    }

    #[test]
    fn percent_encoding() {
        assert_eq!(up("![](../attachments/%E8%A7%86%E9%A2%91%E7%94%9F%E6%88%90%E8%8A%82%E7%82%B9-1%20(1).mp4)"), "![](attachments/视频生成节点-1%20%281%29.mp4)", "written back as pasting writes it");
        assert_eq!(removing_percent_encoding("%e4%b8%ad%20a+b").as_deref(), Some("中 a+b"));
        assert_eq!(removing_percent_encoding("100%25").as_deref(), Some("100%"));
        assert_eq!(removing_percent_encoding("%F0%9F%98%80").as_deref(), Some("😀"));
        assert_eq!(removing_percent_encoding("%00").as_deref(), Some("\0"));
        for invalid in ["%", "%2", "%zz", "a%", "%E4%B8", "%C0%80", "%ED%A0%80", "%中中"] {
            assert_eq!(removing_percent_encoding(invalid), None, "{invalid}");
        }
        assert_eq!(encoded("a b(c)#d%e😀中"), "a%20b%28c%29%23d%25e😀中");
        assert_eq!(encoded("(\u{301}"), "(\u{301}", "a marked parenthesis is another character");
    }

    #[test]
    fn links_with_a_fragment_keep_its_bytes() {
        assert_eq!(up("[a](../Notes.md#标题 😀%20x)"), "[a](Notes.md#标题 😀%20x)");
        assert_eq!(up("[a](#only-a-fragment)"), "[a](#only-a-fragment)");
    }

    #[test]
    fn embeds() {
        assert_eq!(up("![[../attachments/a.png#page=1|300]]"), "![[attachments/a.png#page=1|300]]");
        assert_eq!(up("![[ ../attachments/a.png ]]"), "![[attachments/a.png]]", "padding goes when it is rewritten");
        assert_eq!(up("![[https://x.y/a.png]]"), "![[https://x.y/a.png]]");
        assert_eq!(up("[[../attachments/a.png]]"), "[[../attachments/a.png]]", "a plain wiki link is not an embed");
    }

    #[test]
    fn overlapping_edits_do_not_panic() {
        let exists = |p: &str| p == "/ws/Travel/![[a/b.png]]" || p == "/ws/Travel/a/b.png";
        let out = rewrite("[x](![[a/b.png]]) 中", "/ws/Travel/N.md", "/ws/Travel/N.md", "/ws/N.md", "/ws", exists);
        assert_eq!(out, "[x](Travel/![[a/b.png]]) 中");
    }

    #[test]
    fn normalization_of_paths() {
        assert_eq!(relative_path("/ws/caf\u{E9}", "/ws/cafe\u{301}/a.png"), "a.png");
        let nfd_note = "/ws/cafe\u{301}/N.md";
        let exists = |p: &str| p == "/ws/x.png" || p == "/ws/Archive/caf\u{E9}/N.md";
        let out = rewrite("![](../x.png)", nfd_note, "/ws/caf\u{E9}", "/ws/Archive/caf\u{E9}", "/ws", exists);
        assert_eq!(out, "![](../../x.png)", "the note's path and the moved folder's differ only in form");
    }

    #[test]
    fn join_edges() {
        assert_eq!(join("/", "../../a"), "/a");
        assert_eq!(join("/ws", ""), "/ws");
        assert_eq!(join("/ws", "a//b/"), "/ws/a/b");
        assert_eq!(relative_path("/a/b/c", "/a"), "../..");
        assert_eq!(relative_path("/", "/a/b"), "a/b");
    }
}
