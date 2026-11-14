//! Fuzzy matching and note search. Port of `Services/FuzzySearch.swift`.
//!
//! `FuzzyMatch::offsets` count grapheme clusters, the `Character`s of the Swift
//! code; ranges handed to a UI are byte ranges on grapheme boundaries. Matching
//! compares clusters as Swift does, by canonical equivalence: a query typed in
//! NFC finds a file name stored in NFD.

use std::ops::Range;
use std::sync::Arc;

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};
use unicode_segmentation::UnicodeSegmentation;

use crate::note::Note;

use self::foundation::{
    character_is_lowercase, character_is_uppercase, drop_first_grapheme, has_prefix, is_foundation_newline, line_range,
    lowercased, trim_whitespaces, trim_whitespaces_and_newlines,
};

/// A byte range of the string it was made for, on grapheme boundaries.
pub type FuzzyRange = Range<usize>;

/// How many results `hits` keeps unless told otherwise.
pub const DEFAULT_LIMIT: usize = 100;

// Single-string fuzzy matching.

/// A fuzzy match of a query inside one string: the query's characters appear in
/// order (a subsequence), case-insensitively, anywhere in the haystack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    /// Higher is better. Early, word-boundary and consecutive matches score
    /// above scattered late ones.
    pub score: i64,
    /// Grapheme offsets into the haystack of the matched characters, ascending.
    pub offsets: Vec<usize>,
}

/// `s` as matching compares it: lowercased, then in NFC, so that `==` on its
/// clusters is `Character`'s canonical equivalence.
fn comparable(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    let lowered = lowercased(s);
    if is_nfc_quick(lowered.chars()) == IsNormalized::Yes { lowered } else { lowered.nfc().collect() }
}

fn clusters(s: &str) -> Vec<&str> {
    if s.is_ascii() && !s.contains('\r') {
        (0..s.len()).map(|i| &s[i..i + 1]).collect()
    } else {
        s.graphemes(true).collect()
    }
}

impl FuzzyMatch {
    /// Matches `query` against `haystack` as a case-insensitive subsequence, or
    /// returns `None` when some query character is missing.
    ///
    /// Greedy in both directions: each query character takes the earliest
    /// (forward) or latest (backward) haystack character that can still complete
    /// the query, and the better-scoring alignment wins. Forward greedy alone
    /// can split an obvious run (" talked" + "Kyoto" for "kyoto"); the backward
    /// pass anchors late matches tightly.
    pub fn find(query: &str, haystack: &str) -> Option<FuzzyMatch> {
        let query = comparable(query);
        Self::find_clusters(&clusters(&query), haystack)
    }

    fn find_clusters(q: &[&str], haystack: &str) -> Option<FuzzyMatch> {
        if q.is_empty() {
            return None;
        }
        let lowered = comparable(haystack);
        let h = clusters(&lowered);
        if q.len() > h.len() {
            return None;
        }
        let forward = Self::greedy(q, &h)?;
        match Self::reverse_greedy(q, &h) {
            Some(backward) if backward.score > forward.score => Some(backward),
            _ => Some(forward),
        }
    }

    /// Forward greedy: earliest completion. Biased toward matches that start
    /// early, which is what "fuzzy" search usually wants.
    fn greedy(q: &[&str], h: &[&str]) -> Option<FuzzyMatch> {
        let mut offsets = Vec::with_capacity(q.len());
        let mut hi = 0;
        for qc in q {
            hi += h[hi..].iter().position(|c| c == qc)?;
            offsets.push(hi);
            hi += 1;
        }
        Some(FuzzyMatch { score: score(&offsets, h), offsets })
    }

    /// Backward greedy: latest completion. Finds the tight alignment when the
    /// match sits late in the string.
    fn reverse_greedy(q: &[&str], h: &[&str]) -> Option<FuzzyMatch> {
        let mut offsets = Vec::with_capacity(q.len());
        let mut end = h.len();
        for qc in q.iter().rev() {
            end = h[..end].iter().rposition(|c| c == qc)?;
            offsets.push(end);
        }
        offsets.reverse();
        Some(FuzzyMatch { score: score(&offsets, h), offsets })
    }

    /// A literal (contiguous) match of `length` characters at `start`, scored
    /// like a fuzzy one.
    pub fn contiguous(start: usize, length: usize, haystack: &str) -> FuzzyMatch {
        let offsets: Vec<usize> = (start..start + length).collect();
        let lowered = comparable(haystack);
        FuzzyMatch { score: score(&offsets, &clusters(&lowered)), offsets }
    }

    /// The matched offsets as sliceable byte ranges of `haystack`, one per run
    /// of consecutive characters.
    pub fn ranges(&self, haystack: &str) -> Vec<FuzzyRange> {
        let starts: Vec<usize> = haystack.grapheme_indices(true).map(|(i, _)| i).collect();
        let boundary = |i: usize| starts.get(i).copied().unwrap_or(haystack.len());
        runs(&self.offsets).map(|run| boundary(run.start)..boundary(run.end)).filter(|r| !r.is_empty()).collect()
    }
}

/// Offsets, ascending, as half-open runs of consecutive values.
fn runs(offsets: &[usize]) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut i = 0;
    std::iter::from_fn(move || {
        let first = *offsets.get(i)?;
        let mut j = i;
        while j + 1 < offsets.len() && offsets[j + 1] == offsets[j] + 1 {
            j += 1;
        }
        i = j + 1;
        Some(first..offsets[j] + 1)
    })
}

/// Sublime-style scoring: a point per matched character, a bonus for starting
/// the string or a word, a bigger bonus for consecutive runs, and a capped
/// penalty for the gap skipped since the previous match.
fn score(offsets: &[usize], h: &[&str]) -> i64 {
    let mut score = 0;
    for (j, &i) in offsets.iter().enumerate() {
        score += 1;
        if i == 0 {
            score += 8; // starts the string
        } else if let (Some(previous), Some(current)) = (h.get(i - 1), h.get(i))
            && is_word_boundary(previous, current)
        {
            score += 6; // starts a word
        }
        if j > 0 {
            if offsets[j - 1] + 1 == i {
                score += 5; // consecutive run
            }
            score -= (i - offsets[j - 1] - 1).min(6) as i64; // gap penalty, capped
        }
    }
    score
}

/// Separators, plus case transitions. `h` is lowercased before it gets here, so
/// the case half only fires for uppercase letters with no lowercase form.
fn is_word_boundary(previous: &str, current: &str) -> bool {
    matches!(previous, " " | "_" | "-" | "." | "/" | "(" | "[" | "{" | "#" | "、" | "，")
        || (character_is_lowercase(previous) && character_is_uppercase(current))
}

// Note-level search.

/// Which field of a note produced a hit, which decides what the result row shows
/// as its second line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoteMatchField {
    FileName,
    Title,
    Folder,
    Tag,
    Content,
    /// Synthesized for the popover's empty-query "recent notes" list.
    Recent,
}

/// A short excerpt of a note body, windowed around the match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub text: String,
    /// Byte ranges of `text` to highlight.
    pub ranges: Vec<FuzzyRange>,
}

/// One search result: the note, where it matched, and what to highlight.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteSearchHit<'a> {
    pub note: &'a Note,
    pub score: i64,
    pub field: NoteMatchField,
    /// Ranges to highlight in the row's first line, `note.filename()`.
    pub title_ranges: Vec<FuzzyRange>,
    /// The row's second line: the matched title or tag, or the folder path.
    pub subtitle: String,
    /// Ranges to highlight in `subtitle`.
    pub subtitle_ranges: Vec<FuzzyRange>,
    /// Body excerpt, when the match is in the note's content.
    pub snippet: Option<Snippet>,
    /// The match's byte range in `note.content`, when the hit is in the body,
    /// so opening the note can jump to where it is.
    pub content_range: Option<Range<usize>>,
}

impl<'a> NoteSearchHit<'a> {
    /// A hit for the empty-query "recent notes" list: no highlights.
    pub fn recent(note: &'a Note) -> Self {
        NoteSearchHit {
            note,
            score: 0,
            field: NoteMatchField::Recent,
            title_ranges: Vec::new(),
            subtitle: folder_display_name(note),
            subtitle_ranges: Vec::new(),
            snippet: None,
            content_range: None,
        }
    }
}

/// Folder path for display: "Notes" at the library root.
pub fn folder_display_name(note: &Note) -> String {
    if note.relative_path == "/" { "Notes".to_string() } else { drop_first_grapheme(&note.relative_path).to_string() }
}

/// A body after `fold`, and how to find its offsets in the original.
struct FoldedBody {
    text: String,
    /// The characters whose folded form isn't one character of the same length,
    /// in order. Everywhere else an offset carries over by a constant shift.
    changes: Vec<LengthChange>,
}

struct LengthChange {
    folded: Range<usize>,
    original: Range<usize>,
}

impl FoldedBody {
    fn new(body: &str) -> Self {
        if body.is_ascii() {
            return FoldedBody { text: body.to_ascii_lowercase(), changes: Vec::new() };
        }
        let mut text = String::with_capacity(body.len());
        let mut changes = Vec::new();
        for (position, c) in body.char_indices() {
            if c.is_ascii() || is_caseless_cjk(c) {
                text.push(c.to_ascii_lowercase());
                continue;
            }
            let start = text.len();
            fold_char(c, &mut text);
            // ß to ss keeps the length but not the character boundaries.
            let added = &text[start..];
            if added.len() != c.len_utf8() || added.chars().nth(1).is_some() {
                changes.push(LengthChange { folded: start..text.len(), original: position..position + c.len_utf8() });
            }
        }
        FoldedBody { text, changes }
    }

    /// The original byte where the character at folded offset `at` starts.
    fn original_start(&self, at: usize) -> usize {
        match self.changes.partition_point(|change| change.folded.start <= at).checked_sub(1) {
            None => at,
            Some(i) => {
                let change = &self.changes[i];
                if at < change.folded.end {
                    change.original.start
                } else {
                    change.original.end + (at - change.folded.end)
                }
            }
        }
    }

    /// The original byte where the character ending at folded offset `at` ends.
    fn original_end(&self, at: usize) -> usize {
        match self.changes.partition_point(|change| change.folded.start < at).checked_sub(1) {
            None => at,
            Some(i) => {
                let change = &self.changes[i];
                if at <= change.folded.end {
                    change.original.end
                } else {
                    change.original.end + (at - change.folded.end)
                }
            }
        }
    }
}

/// Nothing folds here, and most of a Chinese vault is here.
fn is_caseless_cjk(c: char) -> bool {
    ('\u{3000}'..'\u{A640}').contains(&c)
}

fn fold_char(c: char, out: &mut String) {
    if c.is_ascii() {
        out.push(c.to_ascii_lowercase());
    } else if is_caseless_cjk(c) {
        out.push(c);
    } else if let Ok(i) = FOLD_EXCEPTIONS.binary_search_by_key(&(c as u32), |entry| entry.0) {
        out.push_str(FOLD_EXCEPTIONS[i].1);
    } else {
        out.extend(c.to_lowercase());
    }
}

/// Case and width folding for body search (Ａ→a, ＡＩ→ai, ß→ss), as
/// `folding(options: [.caseInsensitive, .widthInsensitive], locale: nil)`.
pub fn fold(text: &str) -> String {
    FoldedBody::new(text).text
}

/// Note bodies prepared once for fast body search: reuse one corpus for every
/// keystroke while the search panel is open.
pub struct SearchCorpus {
    notes: Arc<[Note]>,
    folded: Vec<FoldedBody>,
}

impl SearchCorpus {
    pub fn new(notes: impl Into<Arc<[Note]>>) -> Self {
        let notes = notes.into();
        let folded = notes.iter().map(|note| FoldedBody::new(&note.content)).collect();
        SearchCorpus { notes, folded }
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// `hits` over this corpus.
    pub fn hits(&self, query: &str, limit: usize) -> Vec<NoteSearchHit<'_>> {
        search(&self.notes, query, limit, |index, folded_query| {
            content_match(&self.notes[index].content, &self.folded[index], folded_query)
        })
    }
}

/// Ranks `notes` for `query`, best first.
///
/// A query starting with `#` searches tags instead: a tag matches when the rest
/// of the query is a subsequence of its full path (`#aisc` finds `AI/Swift`).
/// Otherwise the query is matched against the file name, the extracted title,
/// the folder path and the body, in that order of weight, and each note keeps
/// only its best field.
pub fn hits<'a>(notes: &'a [Note], query: &str, limit: usize) -> Vec<NoteSearchHit<'a>> {
    search(notes, query, limit, |index, folded_query| {
        content_match(&notes[index].content, &FoldedBody::new(&notes[index].content), folded_query)
    })
}

fn search<'a>(
    notes: &'a [Note],
    query: &str,
    limit: usize,
    body: impl Fn(usize, &str) -> Option<(String, FuzzyMatch, Range<usize>)>,
) -> Vec<NoteSearchHit<'a>> {
    let q = trim_whitespaces_and_newlines(query);
    if q.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    if has_prefix(q, "#") {
        let tag_query = trim_whitespaces(drop_first_grapheme(q));
        if tag_query.is_empty() {
            return Vec::new();
        }
        let lowered = comparable(tag_query);
        let q_clusters = clusters(&lowered);
        for note in notes {
            if let Some((tag, m)) = best_tag_match(&note.tags, &q_clusters) {
                hits.push(NoteSearchHit {
                    note,
                    score: m.score,
                    field: NoteMatchField::Tag,
                    title_ranges: Vec::new(),
                    subtitle: tag.clone(),
                    subtitle_ranges: m.ranges(tag),
                    snippet: None,
                    content_range: None,
                });
            }
        }
    } else {
        let folded_query = fold(q);
        let lowered = comparable(q);
        let q_clusters = clusters(&lowered);
        for (index, note) in notes.iter().enumerate() {
            if let Some(hit) = best_hit(note, &q_clusters, || body(index, &folded_query)) {
                hits.push(hit);
            }
        }
    }
    // Rank first, then cap: capping first dropped the best matches whenever more
    // than `limit` notes matched.
    sort_hits(&mut hits);
    hits.truncate(limit);
    hits
}

/// Notes for the popover's empty state: pinned first, then recently updated.
pub fn recent(notes: &[Note], limit: usize) -> Vec<&Note> {
    let mut sorted: Vec<&Note> = notes.iter().collect();
    sorted.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.updated_at.total_cmp(&a.updated_at)));
    sorted.truncate(limit);
    sorted
}

/// Score dominates; pinned and recency only break ties.
fn sort_hits(hits: &mut [NoteSearchHit<'_>]) {
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.note.pinned.cmp(&a.note.pinned))
            .then(b.note.updated_at.total_cmp(&a.note.updated_at))
    });
}

/// `Int(weight * Double(score))`, which truncates toward zero.
fn weighted(weight: f64, score: i64) -> i64 {
    (weight * score as f64) as i64
}

/// Field weights: the file name is what users aim at, the title and tags are
/// nearly as good, the folder path narrows by location, and a body match is the
/// weakest signal.
fn best_hit<'a>(
    note: &'a Note,
    q: &[&str],
    body: impl FnOnce() -> Option<(String, FuzzyMatch, Range<usize>)>,
) -> Option<NoteSearchHit<'a>> {
    let hit = |score, field, title_ranges, subtitle, subtitle_ranges, snippet, content_range| NoteSearchHit {
        note,
        score,
        field,
        title_ranges,
        subtitle,
        subtitle_ranges,
        snippet,
        content_range,
    };
    let filename = note.filename();
    if let Some(m) = FuzzyMatch::find_clusters(q, &filename) {
        return Some(hit(
            weighted(1.00, m.score),
            NoteMatchField::FileName,
            m.ranges(&filename),
            folder_display_name(note),
            Vec::new(),
            None,
            None,
        ));
    }
    if !note.title.is_empty()
        && let Some(m) = FuzzyMatch::find_clusters(q, &note.title)
    {
        let ranges = m.ranges(&note.title);
        return Some(hit(weighted(0.95, m.score), NoteMatchField::Title, Vec::new(), note.title.clone(), ranges, None, None));
    }
    if note.relative_path != "/" {
        let folder = folder_display_name(note);
        if let Some(m) = FuzzyMatch::find_clusters(q, &folder) {
            let ranges = m.ranges(&folder);
            return Some(hit(weighted(0.75, m.score), NoteMatchField::Folder, Vec::new(), folder, ranges, None, None));
        }
    }
    let (line, m, at) = body()?;
    Some(hit(
        weighted(0.55, m.score),
        NoteMatchField::Content,
        Vec::new(),
        folder_display_name(note),
        Vec::new(),
        Some(snippet(&line, &m)),
        Some(at),
    ))
}

/// The best-scoring tag, or `None` when no tag matches. The first wins a tie.
fn best_tag_match<'a>(tags: &'a [String], q: &[&str]) -> Option<(&'a String, FuzzyMatch)> {
    let mut best: Option<(&String, FuzzyMatch)> = None;
    for tag in tags {
        if let Some(m) = FuzzyMatch::find_clusters(q, tag)
            && best.as_ref().is_none_or(|(_, top)| m.score > top.score)
        {
            best = Some((tag, m));
        }
    }
    best
}

/// `at` moved back to the start of the grapheme holding it.
fn grapheme_floor(text: &str, at: usize) -> usize {
    let mut cursor = unicode_segmentation::GraphemeCursor::new(at, text.len(), true);
    match cursor.is_boundary(text, 0) {
        Ok(false) => cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(at),
        _ => at,
    }
}

/// `at` moved on to the end of the grapheme holding it.
fn grapheme_ceil(text: &str, at: usize) -> usize {
    let mut cursor = unicode_segmentation::GraphemeCursor::new(at, text.len(), true);
    match cursor.is_boundary(text, 0) {
        Ok(false) => cursor.next_boundary(text, 0).ok().flatten().unwrap_or(at),
        _ => at,
    }
}

/// The body line holding the query, cut to a window around it, and the hit's
/// byte range in the whole body, or `None`.
///
/// Bodies are matched literally (ignoring case and width), not as a subsequence:
/// across a whole note nearly any short query is a subsequence of some line,
/// "zqx" matched 67 of 499 notes.
fn content_match(body: &str, folded: &FoldedBody, folded_query: &str) -> Option<(String, FuzzyMatch, Range<usize>)> {
    if folded_query.is_empty() {
        return None;
    }
    let found = folded.text.find(folded_query)?;
    let hit = folded.original_start(found)..folded.original_end(found + folded_query.len());

    // The hit's line, cut to a window around it: a paragraph can run to thousands
    // of characters and the snippet shows a few dozen. The margins exceed the
    // snippet's own, so its ellipses still mark cuts. They count UTF-16 units, as
    // the Swift code does.
    let mut line = line_range(body, hit.clone());
    while line.end > hit.end {
        match body[line.start..line.end].chars().next_back() {
            Some(last) if is_foundation_newline(last) => line.end -= last.len_utf8(),
            _ => break,
        }
    }
    let mut start = hit.start;
    let mut units = 0;
    for c in body[line.start..hit.start].chars().rev() {
        if units >= 120 {
            break;
        }
        units += c.len_utf16();
        start -= c.len_utf8();
    }
    let mut end = hit.end;
    let mut units = 0;
    for c in body[hit.end..line.end].chars() {
        if units >= 200 {
            break;
        }
        units += c.len_utf16();
        end += c.len_utf8();
    }
    let start = grapheme_floor(body, start);
    let end = grapheme_ceil(body, end);
    let text = &body[start..end];

    // The characters the hit touches. A hit can begin inside a cluster (an emoji
    // inside a family, say); the whole cluster is what can be highlighted.
    let (hit_start, hit_end) = (hit.start - start, hit.end - start);
    let mut first = 0;
    let mut last = 0;
    for (i, (position, cluster)) in text.grapheme_indices(true).enumerate() {
        if position <= hit_start {
            first = i;
        }
        if position < hit_end {
            last = i;
        }
        if position + cluster.len() >= hit_end {
            break;
        }
    }
    Some((text.to_string(), FuzzyMatch::contiguous(first, last - first + 1, text), hit))
}

// Snippets.

/// Wraps a matched body line into a short excerpt: leading Markdown noise (`#`,
/// `>`, `- `) is dropped, and long lines are windowed around the match with
/// ellipses at the cut.
pub fn snippet(line: &str, m: &FuzzyMatch) -> Snippet {
    let chars: Vec<&str> = line.graphemes(true).collect();
    // Drop leading Markdown markers, but never cut into the match itself.
    let skip = markdown_skip(&chars).min(m.offsets.first().copied().unwrap_or(chars.len())).min(chars.len());
    let skipped: usize = chars[..skip].iter().map(|c| c.len()).sum();

    let shifted: Vec<usize> = m.offsets.iter().map(|offset| offset.saturating_sub(skip)).collect();
    let text = &line[skipped..];
    let body: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    let boundary = |i: usize| body.get(i).map_or(text.len(), |(position, _)| *position);
    let is_blank = |i: usize| body[i].1.chars().next().is_some_and(char::is_whitespace);

    let mut start = shifted.first().copied().unwrap_or(0).saturating_sub(24).min(body.len());
    let mut end = (shifted.last().copied().unwrap_or(0) + 48).min(body.len()).max(start);
    // Ellipses mark a real cut, not whitespace trimmed at the window's edge, so
    // decide before trimming.
    let cut_at_start = start > 0;
    let cut_at_end = end < body.len();
    if start == 0 {
        while start < end && is_blank(start) {
            start += 1;
        }
    }
    if end == body.len() {
        while end > start && is_blank(end - 1) {
            end -= 1;
        }
    }

    let prefix = if cut_at_start { "…" } else { "" };
    let suffix = if cut_at_end { "…" } else { "" };
    let window = boundary(start);
    let out = format!("{prefix}{}{suffix}", &text[window..boundary(end)]);
    let ranges = runs(&shifted)
        .filter_map(|run| {
            let (from, to) = (run.start.max(start), run.end.min(end));
            (from < to).then(|| {
                prefix.len() + boundary(from) - window..prefix.len() + boundary(to) - window
            })
        })
        .collect();
    Snippet { text: out, ranges }
}

/// Leading Markdown markers to drop from a body line for display.
fn markdown_skip(chars: &[&str]) -> usize {
    let hashes = chars.iter().take_while(|c| **c == "#").count();
    if hashes > 0 {
        return if chars.get(hashes) == Some(&" ") { hashes + 1 } else { hashes };
    }
    if chars.first() == Some(&">") {
        return if chars.get(1) == Some(&" ") { 2 } else { 1 };
    }
    if matches!(chars.first(), Some(&("-" | "*" | "+"))) && chars.get(1) == Some(&" ") {
        return 2;
    }
    0
}

/// Foundation's and ICU's text rules where they differ from `str`'s, shared by
/// the memo, slash and deck modules.
pub(crate) mod foundation {
    use std::ops::Range;
    use std::sync::LazyLock;

    use regex::Regex;
    use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

    /// `CharacterSet.whitespaces`: U+200B is in it, and no line break is.
    pub fn is_foundation_whitespace(c: char) -> bool {
        matches!(c, '\t' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200B}' | '\u{202F}' | '\u{205F}' | '\u{3000}')
    }

    /// `CharacterSet.newlines`. Also what ICU calls a line terminator.
    pub fn is_foundation_newline(c: char) -> bool {
        matches!(c, '\n' | '\u{B}' | '\u{C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
    }

    /// `trimmingCharacters(in: .whitespaces)`.
    pub fn trim_whitespaces(s: &str) -> &str {
        s.trim_matches(is_foundation_whitespace)
    }

    /// `trimmingCharacters(in: .whitespacesAndNewlines)`.
    pub fn trim_whitespaces_and_newlines(s: &str) -> &str {
        s.trim_matches(|c| is_foundation_whitespace(c) || is_foundation_newline(c))
    }

    fn is_grapheme_boundary(s: &str, at: usize) -> bool {
        GraphemeCursor::new(at, s.len(), true).is_boundary(s, 0).unwrap_or(true)
    }

    /// `String.hasPrefix`: the prefix must end on a `Character` boundary, so
    /// `"#\u{FE0F}\u{20E3}"` (a keycap emoji) doesn't start with `#`.
    pub fn has_prefix(s: &str, prefix: &str) -> bool {
        s.starts_with(prefix) && is_grapheme_boundary(s, prefix.len())
    }

    /// `String.hasSuffix`: `"a\r\n"` doesn't end with `"\n"`.
    pub fn has_suffix(s: &str, suffix: &str) -> bool {
        s.ends_with(suffix) && is_grapheme_boundary(s, s.len() - suffix.len())
    }

    /// `dropFirst()`: everything after the first `Character`.
    pub fn drop_first_grapheme(s: &str) -> &str {
        s.graphemes(true).next().map_or("", |first| &s[first.len()..])
    }

    /// `.count`.
    pub fn grapheme_count(s: &str) -> usize {
        s.graphemes(true).count()
    }

    /// `String.lowercased()`, which maps scalar by scalar: unlike
    /// `str::to_lowercase`, a final sigma stays `σ`.
    pub fn lowercased(s: &str) -> String {
        if s.is_ascii() {
            return s.to_ascii_lowercase();
        }
        s.chars().flat_map(char::to_lowercase).collect()
    }

    fn cased_mappings(g: &str) -> (String, String) {
        (g.chars().flat_map(char::to_uppercase).collect(), lowercased(g))
    }

    fn is_cased(g: &str, upper: &str, lower: &str) -> bool {
        let mut chars = g.chars();
        let single = chars.next().filter(|_| chars.next().is_none());
        single.is_some_and(|c| c.is_lowercase() || c.is_uppercase()) || upper != lower
    }

    /// `Character.isLowercase`.
    pub fn character_is_lowercase(g: &str) -> bool {
        if g.len() == 1 {
            return g.as_bytes()[0].is_ascii_lowercase();
        }
        let (upper, lower) = cased_mappings(g);
        is_cased(g, &upper, &lower) && lower == g
    }

    /// `Character.isUppercase`.
    pub fn character_is_uppercase(g: &str) -> bool {
        if g.len() == 1 {
            return g.as_bytes()[0].is_ascii_uppercase();
        }
        let (upper, lower) = cased_mappings(g);
        is_cased(g, &upper, &lower) && upper == g
    }

    /// What `NSString.lineRange(for:)` returns. Lines end at `\n`, `\r`, `\r\n`,
    /// U+0085, U+2028 and U+2029, the line break included; not at VT or FF.
    pub fn line_range(text: &str, range: Range<usize>) -> Range<usize> {
        fn breaks_line(c: char) -> bool {
            matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
        }
        let bytes = text.as_bytes();
        let mut from = range.start.min(text.len());
        // Inside a CRLF is on the CR's line.
        if from > 0 && from < text.len() && bytes[from] == b'\n' && bytes[from - 1] == b'\r' {
            from -= 1;
        }
        let start = text[..from].char_indices().rev().find(|(_, c)| breaks_line(*c)).map_or(0, |(i, c)| i + c.len_utf8());
        let to = range.end.max(from).min(text.len());
        let end_scan = if range.end > range.start && text[..to].chars().next_back().is_some_and(breaks_line) {
            // The range already ends with a line break.
            if bytes[to - 1] == b'\r' && bytes.get(to) == Some(&b'\n') { to + 1 } else { to }
        } else {
            match text[to..].char_indices().find(|(_, c)| breaks_line(*c)) {
                None => text.len(),
                Some((i, '\r')) if bytes.get(to + i + 1) == Some(&b'\n') => to + i + 2,
                Some((i, c)) => to + i + c.len_utf8(),
            }
        };
        start..end_scan
    }

    /// ICU's `$`, without multiline: at the end, or before one line terminator
    /// (or CRLF) that ends the input. Whether `rest` is where it matches.
    pub fn is_regex_end(rest: &str) -> bool {
        strip_regex_final_terminator(rest).is_empty()
    }

    pub fn is_regex_line_terminator(c: char) -> bool {
        is_foundation_newline(c)
    }

    /// `s` without what ICU's `$` may match before, so that a `regex` pattern
    /// ending in `$` answers as an ICU one does.
    pub fn strip_regex_final_terminator(s: &str) -> &str {
        if let Some(stripped) = s.strip_suffix("\r\n") {
            return stripped;
        }
        match s.chars().next_back() {
            Some(last) if is_foundation_newline(last) => &s[..s.len() - last.len_utf8()],
            _ => s,
        }
    }

    static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\w$").expect("valid word pattern"));

    /// ICU's and `regex`'s `\w`.
    pub fn is_word_char(c: char) -> bool {
        if c.is_ascii() {
            return c.is_ascii_alphanumeric() || c == '_';
        }
        WORD.is_match(c.encode_utf8(&mut [0; 4]))
    }

    /// `at`, moved back to a character boundary and into `s`.
    pub fn floor_char_boundary(s: &str, at: usize) -> usize {
        let mut at = at.min(s.len());
        while !s.is_char_boundary(at) {
            at -= 1;
        }
        at
    }
}

/// Characters `fold` treats differently from `char::to_lowercase`, sorted, as
/// dumped from `String.folding(options: [.caseInsensitive, .widthInsensitive])`
/// over all of Unicode: full case folding and the halfwidth and fullwidth forms.
#[rustfmt::skip]
static FOLD_EXCEPTIONS: [(u32, &str); 340] = [
    (0xB5, "\u{3BC}"), (0xDF, "\u{73}\u{73}"), (0x149, "\u{2BC}\u{6E}"), (0x17F, "\u{73}"),
    (0x1F0, "\u{6A}\u{30C}"), (0x345, "\u{3B9}"), (0x390, "\u{3B9}\u{308}\u{301}"), (0x3B0, "\u{3C5}\u{308}\u{301}"),
    (0x3C2, "\u{3C3}"), (0x3D0, "\u{3B2}"), (0x3D1, "\u{3B8}"), (0x3D5, "\u{3C6}"),
    (0x3D6, "\u{3C0}"), (0x3F0, "\u{3BA}"), (0x3F1, "\u{3C1}"), (0x3F5, "\u{3B5}"),
    (0x587, "\u{565}\u{582}"), (0x1E96, "\u{68}\u{331}"), (0x1E97, "\u{74}\u{308}"), (0x1E98, "\u{77}\u{30A}"),
    (0x1E99, "\u{79}\u{30A}"), (0x1E9A, "\u{61}\u{2BE}"), (0x1E9B, "\u{1E61}"), (0x1E9E, "\u{73}\u{73}"),
    (0x1F50, "\u{3C5}\u{313}"), (0x1F52, "\u{3C5}\u{313}\u{300}"), (0x1F54, "\u{3C5}\u{313}\u{301}"), (0x1F56, "\u{3C5}\u{313}\u{342}"),
    (0x1F80, "\u{1F00}\u{3B9}"), (0x1F81, "\u{1F01}\u{3B9}"), (0x1F82, "\u{1F02}\u{3B9}"), (0x1F83, "\u{1F03}\u{3B9}"),
    (0x1F84, "\u{1F04}\u{3B9}"), (0x1F85, "\u{1F05}\u{3B9}"), (0x1F86, "\u{1F06}\u{3B9}"), (0x1F87, "\u{1F07}\u{3B9}"),
    (0x1F88, "\u{1F00}\u{3B9}"), (0x1F89, "\u{1F01}\u{3B9}"), (0x1F8A, "\u{1F02}\u{3B9}"), (0x1F8B, "\u{1F03}\u{3B9}"),
    (0x1F8C, "\u{1F04}\u{3B9}"), (0x1F8D, "\u{1F05}\u{3B9}"), (0x1F8E, "\u{1F06}\u{3B9}"), (0x1F8F, "\u{1F07}\u{3B9}"),
    (0x1F90, "\u{1F20}\u{3B9}"), (0x1F91, "\u{1F21}\u{3B9}"), (0x1F92, "\u{1F22}\u{3B9}"), (0x1F93, "\u{1F23}\u{3B9}"),
    (0x1F94, "\u{1F24}\u{3B9}"), (0x1F95, "\u{1F25}\u{3B9}"), (0x1F96, "\u{1F26}\u{3B9}"), (0x1F97, "\u{1F27}\u{3B9}"),
    (0x1F98, "\u{1F20}\u{3B9}"), (0x1F99, "\u{1F21}\u{3B9}"), (0x1F9A, "\u{1F22}\u{3B9}"), (0x1F9B, "\u{1F23}\u{3B9}"),
    (0x1F9C, "\u{1F24}\u{3B9}"), (0x1F9D, "\u{1F25}\u{3B9}"), (0x1F9E, "\u{1F26}\u{3B9}"), (0x1F9F, "\u{1F27}\u{3B9}"),
    (0x1FA0, "\u{1F60}\u{3B9}"), (0x1FA1, "\u{1F61}\u{3B9}"), (0x1FA2, "\u{1F62}\u{3B9}"), (0x1FA3, "\u{1F63}\u{3B9}"),
    (0x1FA4, "\u{1F64}\u{3B9}"), (0x1FA5, "\u{1F65}\u{3B9}"), (0x1FA6, "\u{1F66}\u{3B9}"), (0x1FA7, "\u{1F67}\u{3B9}"),
    (0x1FA8, "\u{1F60}\u{3B9}"), (0x1FA9, "\u{1F61}\u{3B9}"), (0x1FAA, "\u{1F62}\u{3B9}"), (0x1FAB, "\u{1F63}\u{3B9}"),
    (0x1FAC, "\u{1F64}\u{3B9}"), (0x1FAD, "\u{1F65}\u{3B9}"), (0x1FAE, "\u{1F66}\u{3B9}"), (0x1FAF, "\u{1F67}\u{3B9}"),
    (0x1FB2, "\u{1F70}\u{3B9}"), (0x1FB3, "\u{3B1}\u{3B9}"), (0x1FB4, "\u{3AC}\u{3B9}"), (0x1FB6, "\u{3B1}\u{342}"),
    (0x1FB7, "\u{3B1}\u{342}\u{3B9}"), (0x1FBC, "\u{3B1}\u{3B9}"), (0x1FBE, "\u{3B9}"), (0x1FC2, "\u{1F74}\u{3B9}"),
    (0x1FC3, "\u{3B7}\u{3B9}"), (0x1FC4, "\u{3AE}\u{3B9}"), (0x1FC6, "\u{3B7}\u{342}"), (0x1FC7, "\u{3B7}\u{342}\u{3B9}"),
    (0x1FCC, "\u{3B7}\u{3B9}"), (0x1FD2, "\u{3B9}\u{308}\u{300}"), (0x1FD3, "\u{3B9}\u{308}\u{301}"), (0x1FD6, "\u{3B9}\u{342}"),
    (0x1FD7, "\u{3B9}\u{308}\u{342}"), (0x1FE2, "\u{3C5}\u{308}\u{300}"), (0x1FE3, "\u{3C5}\u{308}\u{301}"), (0x1FE4, "\u{3C1}\u{313}"),
    (0x1FE6, "\u{3C5}\u{342}"), (0x1FE7, "\u{3C5}\u{308}\u{342}"), (0x1FF2, "\u{1F7C}\u{3B9}"), (0x1FF3, "\u{3C9}\u{3B9}"),
    (0x1FF4, "\u{3CE}\u{3B9}"), (0x1FF6, "\u{3C9}\u{342}"), (0x1FF7, "\u{3C9}\u{342}\u{3B9}"), (0x1FFC, "\u{3C9}\u{3B9}"),
    (0xFB00, "\u{66}\u{66}"), (0xFB01, "\u{66}\u{69}"), (0xFB02, "\u{66}\u{6C}"), (0xFB03, "\u{66}\u{66}\u{69}"),
    (0xFB04, "\u{66}\u{66}\u{6C}"), (0xFB05, "\u{73}\u{74}"), (0xFB06, "\u{73}\u{74}"), (0xFB13, "\u{574}\u{576}"),
    (0xFB14, "\u{574}\u{565}"), (0xFB15, "\u{574}\u{56B}"), (0xFB16, "\u{57E}\u{576}"), (0xFB17, "\u{574}\u{56D}"),
    (0xFF01, "\u{21}"), (0xFF02, "\u{22}"), (0xFF03, "\u{23}"), (0xFF04, "\u{24}"),
    (0xFF05, "\u{25}"), (0xFF06, "\u{26}"), (0xFF07, "\u{27}"), (0xFF08, "\u{28}"),
    (0xFF09, "\u{29}"), (0xFF0A, "\u{2A}"), (0xFF0B, "\u{2B}"), (0xFF0C, "\u{2C}"),
    (0xFF0D, "\u{2D}"), (0xFF0E, "\u{2E}"), (0xFF0F, "\u{2F}"), (0xFF10, "\u{30}"),
    (0xFF11, "\u{31}"), (0xFF12, "\u{32}"), (0xFF13, "\u{33}"), (0xFF14, "\u{34}"),
    (0xFF15, "\u{35}"), (0xFF16, "\u{36}"), (0xFF17, "\u{37}"), (0xFF18, "\u{38}"),
    (0xFF19, "\u{39}"), (0xFF1A, "\u{3A}"), (0xFF1B, "\u{3B}"), (0xFF1C, "\u{3C}"),
    (0xFF1D, "\u{3D}"), (0xFF1E, "\u{3E}"), (0xFF1F, "\u{3F}"), (0xFF20, "\u{40}"),
    (0xFF21, "\u{61}"), (0xFF22, "\u{62}"), (0xFF23, "\u{63}"), (0xFF24, "\u{64}"),
    (0xFF25, "\u{65}"), (0xFF26, "\u{66}"), (0xFF27, "\u{67}"), (0xFF28, "\u{68}"),
    (0xFF29, "\u{69}"), (0xFF2A, "\u{6A}"), (0xFF2B, "\u{6B}"), (0xFF2C, "\u{6C}"),
    (0xFF2D, "\u{6D}"), (0xFF2E, "\u{6E}"), (0xFF2F, "\u{6F}"), (0xFF30, "\u{70}"),
    (0xFF31, "\u{71}"), (0xFF32, "\u{72}"), (0xFF33, "\u{73}"), (0xFF34, "\u{74}"),
    (0xFF35, "\u{75}"), (0xFF36, "\u{76}"), (0xFF37, "\u{77}"), (0xFF38, "\u{78}"),
    (0xFF39, "\u{79}"), (0xFF3A, "\u{7A}"), (0xFF3B, "\u{5B}"), (0xFF3C, "\u{5C}"),
    (0xFF3D, "\u{5D}"), (0xFF3E, "\u{5E}"), (0xFF3F, "\u{5F}"), (0xFF40, "\u{60}"),
    (0xFF41, "\u{61}"), (0xFF42, "\u{62}"), (0xFF43, "\u{63}"), (0xFF44, "\u{64}"),
    (0xFF45, "\u{65}"), (0xFF46, "\u{66}"), (0xFF47, "\u{67}"), (0xFF48, "\u{68}"),
    (0xFF49, "\u{69}"), (0xFF4A, "\u{6A}"), (0xFF4B, "\u{6B}"), (0xFF4C, "\u{6C}"),
    (0xFF4D, "\u{6D}"), (0xFF4E, "\u{6E}"), (0xFF4F, "\u{6F}"), (0xFF50, "\u{70}"),
    (0xFF51, "\u{71}"), (0xFF52, "\u{72}"), (0xFF53, "\u{73}"), (0xFF54, "\u{74}"),
    (0xFF55, "\u{75}"), (0xFF56, "\u{76}"), (0xFF57, "\u{77}"), (0xFF58, "\u{78}"),
    (0xFF59, "\u{79}"), (0xFF5A, "\u{7A}"), (0xFF5B, "\u{7B}"), (0xFF5C, "\u{7C}"),
    (0xFF5D, "\u{7D}"), (0xFF5E, "\u{7E}"), (0xFF5F, "\u{2985}"), (0xFF60, "\u{2986}"),
    (0xFF61, "\u{3002}"), (0xFF62, "\u{300C}"), (0xFF63, "\u{300D}"), (0xFF64, "\u{3001}"),
    (0xFF65, "\u{30FB}"), (0xFF66, "\u{30F2}"), (0xFF67, "\u{30A1}"), (0xFF68, "\u{30A3}"),
    (0xFF69, "\u{30A5}"), (0xFF6A, "\u{30A7}"), (0xFF6B, "\u{30A9}"), (0xFF6C, "\u{30E3}"),
    (0xFF6D, "\u{30E5}"), (0xFF6E, "\u{30E7}"), (0xFF6F, "\u{30C3}"), (0xFF70, "\u{30FC}"),
    (0xFF71, "\u{30A2}"), (0xFF72, "\u{30A4}"), (0xFF73, "\u{30A6}"), (0xFF74, "\u{30A8}"),
    (0xFF75, "\u{30AA}"), (0xFF76, "\u{30AB}"), (0xFF77, "\u{30AD}"), (0xFF78, "\u{30AF}"),
    (0xFF79, "\u{30B1}"), (0xFF7A, "\u{30B3}"), (0xFF7B, "\u{30B5}"), (0xFF7C, "\u{30B7}"),
    (0xFF7D, "\u{30B9}"), (0xFF7E, "\u{30BB}"), (0xFF7F, "\u{30BD}"), (0xFF80, "\u{30BF}"),
    (0xFF81, "\u{30C1}"), (0xFF82, "\u{30C4}"), (0xFF83, "\u{30C6}"), (0xFF84, "\u{30C8}"),
    (0xFF85, "\u{30CA}"), (0xFF86, "\u{30CB}"), (0xFF87, "\u{30CC}"), (0xFF88, "\u{30CD}"),
    (0xFF89, "\u{30CE}"), (0xFF8A, "\u{30CF}"), (0xFF8B, "\u{30D2}"), (0xFF8C, "\u{30D5}"),
    (0xFF8D, "\u{30D8}"), (0xFF8E, "\u{30DB}"), (0xFF8F, "\u{30DE}"), (0xFF90, "\u{30DF}"),
    (0xFF91, "\u{30E0}"), (0xFF92, "\u{30E1}"), (0xFF93, "\u{30E2}"), (0xFF94, "\u{30E4}"),
    (0xFF95, "\u{30E6}"), (0xFF96, "\u{30E8}"), (0xFF97, "\u{30E9}"), (0xFF98, "\u{30EA}"),
    (0xFF99, "\u{30EB}"), (0xFF9A, "\u{30EC}"), (0xFF9B, "\u{30ED}"), (0xFF9C, "\u{30EF}"),
    (0xFF9D, "\u{30F3}"), (0xFF9E, "\u{3099}"), (0xFF9F, "\u{309A}"), (0xFFA0, "\u{1160}"),
    (0xFFA1, "\u{1100}"), (0xFFA2, "\u{1101}"), (0xFFA3, "\u{11AA}"), (0xFFA4, "\u{1102}"),
    (0xFFA5, "\u{11AC}"), (0xFFA6, "\u{11AD}"), (0xFFA7, "\u{1103}"), (0xFFA8, "\u{1104}"),
    (0xFFA9, "\u{1105}"), (0xFFAA, "\u{11B0}"), (0xFFAB, "\u{11B1}"), (0xFFAC, "\u{11B2}"),
    (0xFFAD, "\u{11B3}"), (0xFFAE, "\u{11B4}"), (0xFFAF, "\u{11B5}"), (0xFFB0, "\u{111A}"),
    (0xFFB1, "\u{1106}"), (0xFFB2, "\u{1107}"), (0xFFB3, "\u{1108}"), (0xFFB4, "\u{1121}"),
    (0xFFB5, "\u{1109}"), (0xFFB6, "\u{110A}"), (0xFFB7, "\u{110B}"), (0xFFB8, "\u{110C}"),
    (0xFFB9, "\u{110D}"), (0xFFBA, "\u{110E}"), (0xFFBB, "\u{110F}"), (0xFFBC, "\u{1110}"),
    (0xFFBD, "\u{1111}"), (0xFFBE, "\u{1112}"), (0xFFC2, "\u{1161}"), (0xFFC3, "\u{1162}"),
    (0xFFC4, "\u{1163}"), (0xFFC5, "\u{1164}"), (0xFFC6, "\u{1165}"), (0xFFC7, "\u{1166}"),
    (0xFFCA, "\u{1167}"), (0xFFCB, "\u{1168}"), (0xFFCC, "\u{1169}"), (0xFFCD, "\u{116A}"),
    (0xFFCE, "\u{116B}"), (0xFFCF, "\u{116C}"), (0xFFD2, "\u{116D}"), (0xFFD3, "\u{116E}"),
    (0xFFD4, "\u{116F}"), (0xFFD5, "\u{1170}"), (0xFFD6, "\u{1171}"), (0xFFD7, "\u{1172}"),
    (0xFFDA, "\u{1173}"), (0xFFDB, "\u{1174}"), (0xFFDC, "\u{1175}"), (0xFFE0, "\u{A2}"),
    (0xFFE1, "\u{A3}"), (0xFFE2, "\u{AC}"), (0xFFE4, "\u{A6}"), (0xFFE5, "\u{A5}"),
    (0xFFE6, "\u{20A9}"), (0xFFE8, "\u{2502}"), (0xFFE9, "\u{2190}"), (0xFFEA, "\u{2191}"),
    (0xFFEB, "\u{2192}"), (0xFFEC, "\u{2193}"), (0xFFED, "\u{25A0}"), (0xFFEE, "\u{25CB}"),
];

#[cfg(test)]
mod tests {
    // Ported from Tests/FuzzySearchTests.swift.
    use super::*;
    use std::path::PathBuf;

    fn note_in(title: &str, folder: &str, tags: &[&str], body: &str, updated: f64, pinned: bool) -> Note {
        let dir = if folder == "/" { "" } else { folder };
        let path = format!("/ws{dir}/{title}.md");
        Note {
            id: path.clone(),
            path: PathBuf::from(path),
            title: title.to_string(),
            content: body.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            created_at: updated,
            updated_at: updated,
            pinned,
            relative_path: folder.to_string(),
        }
    }

    fn note(title: &str) -> Note {
        note_in(title, "/", &[], "", 0.0, false)
    }

    fn with_body(title: &str, body: &str) -> Note {
        note_in(title, "/", &[], body, 0.0, false)
    }

    /// Slices `haystack` down to the matched characters: asserts both that a
    /// match exists and that its offsets point at the right places.
    fn matched_text(query: &str, haystack: &str) -> Option<String> {
        let m = FuzzyMatch::find(query, haystack)?;
        let chars: Vec<&str> = haystack.graphemes(true).collect();
        Some(m.offsets.iter().map(|&i| chars[i]).collect())
    }

    fn slice(snippet: &Snippet, i: usize) -> &str {
        &snippet.text[snippet.ranges[i].clone()]
    }

    #[test]
    fn subsequence_matching() {
        assert_eq!(matched_text("kyo", "Kyoto Trip").as_deref(), Some("Kyo"), "substring is a subsequence");
        assert_eq!(matched_text("kt", "Kyoto Trip").as_deref(), Some("KT"), "scattered characters match in order");
        assert_eq!(matched_text("KYO", "kyoto trip").as_deref(), Some("kyo"), "case-insensitive on both sides");
        assert_eq!(matched_text("bj", "CubeJournal").as_deref(), Some("bJ"), "works across camelCase words");
        assert_eq!(matched_text("cj", "CubeJournal").as_deref(), Some("CJ"), "case-insensitive across camelCase");
        assert_eq!(matched_text("xyz", "Kyoto"), None, "non-subsequence does not match");
        assert_eq!(matched_text("kyotos", "Kyoto"), None, "query longer than haystack does not match");
        assert_eq!(matched_text("", "Kyoto"), None, "empty query does not match");
        assert_eq!(matched_text("京都", "Kyoto 京都 trip").as_deref(), Some("京都"), "matches CJK characters");

        // Offsets are ascending and point at the query's characters.
        assert_eq!(FuzzyMatch::find("kt", "Kyoto Trip").unwrap().offsets, [0, 6], "offsets of a scattered match");
    }

    #[test]
    fn scoring() {
        // A prefix match outranks the same characters scattered later.
        let prefix = FuzzyMatch::find("kyo", "Kyoto").unwrap();
        let scattered = FuzzyMatch::find("kyo", "Kimono Kyoto trip").unwrap();
        assert!(prefix.score > scattered.score, "prefix match scores above scattered match");

        // Word-boundary matches beat mid-word ones.
        let boundary = FuzzyMatch::find("tr", "Kyoto Trip").unwrap();
        let mid_word = FuzzyMatch::find("tr", "Kyoto petrol").unwrap();
        assert!(boundary.score > mid_word.score, "word-boundary match scores above mid-word");

        // Consecutive runs beat the same characters with a gap.
        let run = FuzzyMatch::find("oto", "oto").unwrap();
        let gapped = FuzzyMatch::find("oto", "otxo").unwrap();
        assert!(run.score > gapped.score, "consecutive run scores above gapped run");

        // ranges merges consecutive offsets into single runs.
        let ranges = FuzzyMatch::find("kyo", "Kyoto").unwrap().ranges("Kyoto");
        assert_eq!(ranges.len(), 1, "consecutive match is one range");
        assert_eq!(&"Kyoto"[ranges[0].clone()], "Kyo", "range slices the matched run");
    }

    #[test]
    fn note_ranking() {
        let by_name = with_body("Kyoto guide", "nothing relevant here");
        let by_body = with_body("Random note", "we talked about Kyoto at length");
        let mut by_title = with_body("random-file", "irrelevant");
        by_title.title = "Kyoto in spring".to_string(); // clipped article: title differs from file name

        let notes = [by_body.clone(), by_title, by_name];
        let ranked = hits(&notes, "kyoto", DEFAULT_LIMIT);
        assert_eq!(
            ranked.iter().map(|h| h.note.title.as_str()).collect::<Vec<_>>(),
            ["Kyoto guide", "Kyoto in spring", "Random note"],
            "file name ranks first, then title, then body"
        );

        // The body hit carries a windowed snippet with highlight ranges.
        let body_hit = ranked.iter().find(|h| h.note.id == by_body.id).unwrap();
        assert_eq!(body_hit.field, NoteMatchField::Content, "body-only match is a content hit");
        let snippet = body_hit.snippet.as_ref().unwrap();
        assert!(snippet.text.contains("Kyoto"), "snippet contains the matched line");
        assert!(!snippet.ranges.is_empty(), "snippet has highlight ranges");
        assert_eq!(slice(snippet, 0), "Kyoto", "snippet highlight covers the match");

        // Folder path matches.
        let in_folder = [note_in("guide", "/Kyoto", &[], "", 0.0, false)];
        let folder_hits = hits(&in_folder, "kyoto", DEFAULT_LIMIT);
        let folder_hit = folder_hits.first().unwrap();
        assert_eq!(folder_hit.field, NoteMatchField::Folder, "folder path match");
        assert_eq!(folder_hit.subtitle, "Kyoto", "folder hit shows the folder as subtitle");

        // Root notes display "Notes" as their folder.
        assert!(hits(&[note("A")], "", DEFAULT_LIMIT).is_empty(), "empty query returns no hits");
        assert_eq!(folder_display_name(&note("A")), "Notes", "root folder display name");
        assert_eq!(folder_display_name(&note_in("A", "/产品思考", &[], "", 0.0, false)), "产品思考", "nested folder display name");
    }

    #[test]
    fn content_hits_carry_where_they_matched() {
        let body = "first line\nwe talked about Kyoto at length\nlast line";
        let notes = [with_body("Random note", body)];
        let found = hits(&notes, "kyoto", DEFAULT_LIMIT);
        assert_eq!(found[0].field, NoteMatchField::Content);
        let at = found[0].content_range.clone().expect("body hit says where it matched");
        assert_eq!(&body[at.clone()], "Kyoto", "the range points at the match in the content");

        // Width-folded matches map back to the original characters.
        let wide = "見出し\nＫｙｏｔｏに行った";
        let notes = [with_body("W", wide)];
        let at = hits(&notes, "kyoto", DEFAULT_LIMIT)[0].content_range.clone().unwrap();
        assert_eq!(&wide[at], "Ｋｙｏｔｏ");

        // Only body hits carry a range.
        let by_name = [with_body("Kyoto guide", "nothing here")];
        assert!(hits(&by_name, "kyoto", DEFAULT_LIMIT)[0].content_range.is_none());
        let tagged = [note_in("A", "/", &["ai"], "", 2.0, false)];
        assert!(hits(&tagged, "#ai", DEFAULT_LIMIT)[0].content_range.is_none());
    }

    #[test]
    fn tag_mode() {
        let tagged = [
            note_in("A", "/", &["ai"], "", 2.0, false),
            note_in("B", "/", &["AI/编程"], "", 1.0, false),
            note_in("C", "/", &["Anthropic"], "", 0.0, false),
        ];
        let tag_hits = hits(&tagged, "#ai", DEFAULT_LIMIT);
        assert_eq!(
            tag_hits.iter().map(|h| h.note.title.as_str()).collect::<Vec<_>>(),
            ["A", "B", "C"],
            "tag mode matches every tag containing the subsequence"
        );
        assert_eq!(tag_hits.first().map(|h| h.field), Some(NoteMatchField::Tag), "tag hit field");
        assert_eq!(tag_hits.first().map(|h| h.subtitle.as_str()), Some("ai"), "tag hit shows the matched tag");
        assert_eq!(
            hits(&tagged, "#编程", DEFAULT_LIMIT).iter().map(|h| h.note.title.as_str()).collect::<Vec<_>>(),
            ["B"],
            "tag mode matches CJK tag segments"
        );
        assert!(hits(&tagged, "#", DEFAULT_LIMIT).is_empty(), "bare # returns nothing");
        assert!(hits(&tagged, "#zzz", DEFAULT_LIMIT).is_empty(), "no tag match returns nothing");
    }

    #[test]
    fn ranking_tie_breakers_limit_and_recent() {
        // Two notes can't share a path in a folder, but ids differ by their stamp here.
        let mut older = note_in("same", "/", &[], "", 1_000.0, false);
        older.id = "older".to_string();
        let mut newer = note_in("same", "/", &[], "", 2_000.0, false);
        newer.id = "newer".to_string();
        let pair = [older.clone(), newer.clone()];
        assert_eq!(
            hits(&pair, "same", DEFAULT_LIMIT).iter().map(|h| h.note.id.as_str()).collect::<Vec<_>>(),
            [newer.id.as_str(), older.id.as_str()],
            "equal score: recently updated first"
        );
        let pinned = note_in("same", "/", &[], "", 1_000.0, true);
        let pair = [newer, pinned];
        assert_eq!(hits(&pair, "same", DEFAULT_LIMIT).first().map(|h| h.note.pinned), Some(true), "equal score: pinned first");

        let many: Vec<Note> = (0..10).map(|i| note(&format!("note {i}"))).collect();
        assert_eq!(hits(&many, "note", 3).len(), 3, "limit caps the result count");

        let list = [
            note_in("old", "/", &[], "", 1.0, false),
            note_in("pinned old", "/", &[], "", 2.0, true),
            note_in("new", "/", &[], "", 3.0, false),
        ];
        let recents = recent(&list, 2);
        assert_eq!(recents.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["pinned old", "new"], "recent: pinned first, then by date");
        assert_eq!(recent(&many, 3).len(), 3, "recent respects the limit");
    }

    #[test]
    fn snippets() {
        let long_line = format!("{}Kyoto{}", "filler text before the match ".repeat(4), " trailing filler after".repeat(6));
        let m = FuzzyMatch::find("kyoto", &long_line).unwrap();
        let s = snippet(&long_line, &m);
        assert!(s.text.contains('…'), "long line is windowed with ellipses");
        assert!(s.text.contains("Kyoto"), "window keeps the match");
        assert!(s.text.graphemes(true).count() < long_line.graphemes(true).count(), "window is shorter than the line");
        assert_eq!(slice(&s, 0), "Kyoto", "snippet highlight points at the match after windowing");

        let line = "  a quiet line about Kyoto  ";
        let short = snippet(line, &FuzzyMatch::find("kyoto", line).unwrap());
        assert_eq!(short.text, "a quiet line about Kyoto", "short snippet: whitespace trimmed, no ellipses");
        assert_eq!(slice(&short, 0), "Kyoto", "trimmed snippet highlight stays aligned");

        let line = "## Kyoto notes";
        let heading = snippet(line, &FuzzyMatch::find("kyoto", line).unwrap());
        assert_eq!(heading.text, "Kyoto notes", "snippet drops leading markdown heading marks");
        assert_eq!(slice(&heading, 0), "Kyoto", "heading-stripped snippet highlight stays aligned");

        let line = "> quoted Kyoto line";
        let quoted = snippet(line, &FuzzyMatch::find("kyoto", line).unwrap());
        assert_eq!(quoted.text, "quoted Kyoto line", "snippet drops blockquote markers");
    }
}

#[cfg(test)]
mod extra_tests {
    use super::foundation::*;
    use super::*;
    use std::path::PathBuf;
    use std::time::Instant;

    fn body_note(content: &str) -> Note {
        Note {
            id: "/ws/n.md".into(),
            path: PathBuf::from("/ws/n.md"),
            title: String::new(),
            content: content.to_string(),
            tags: Vec::new(),
            created_at: 0.0,
            updated_at: 0.0,
            pinned: false,
            relative_path: "/".into(),
        }
    }

    fn assert_on_graphemes(text: &str, ranges: &[FuzzyRange]) {
        let mut boundaries: Vec<usize> = text.grapheme_indices(true).map(|(i, _)| i).collect();
        boundaries.push(text.len());
        for range in ranges {
            assert!(boundaries.contains(&range.start) && boundaries.contains(&range.end), "{range:?} splits a character in {text:?}");
        }
    }

    fn body_snippet(content: &str, query: &str) -> Snippet {
        let notes = [body_note(content)];
        let found = hits(&notes, query, DEFAULT_LIMIT);
        assert_eq!(found.len(), 1, "{query:?} in {content:?}");
        assert_eq!(found[0].field, NoteMatchField::Content);
        let snippet = found[0].snippet.clone().unwrap();
        assert_on_graphemes(&snippet.text, &snippet.ranges);
        // A prepared corpus gives the same answer.
        let corpus = SearchCorpus::new(notes.to_vec());
        assert_eq!(corpus.hits(query, DEFAULT_LIMIT)[0].snippet.as_ref(), Some(&snippet));
        snippet
    }

    fn highlighted(snippet: &Snippet) -> Vec<&str> {
        snippet.ranges.iter().map(|r| &snippet.text[r.clone()]).collect()
    }

    #[test]
    fn offsets_count_characters_not_scalars_or_bytes() {
        let family = "👨\u{200D}👩\u{200D}👧";
        let hay = format!("{family} 京都 e\u{301}x");
        let m = FuzzyMatch::find("京都", &hay).unwrap();
        assert_eq!(m.offsets, [2, 3], "a family of three is one character");
        assert_eq!(m.ranges(&hay), [family.len() + 1..family.len() + 1 + 6]);
        let m = FuzzyMatch::find("x", &hay).unwrap();
        assert_eq!(m.offsets, [6]);
        let m = FuzzyMatch::find(family, &hay).unwrap();
        assert_eq!(m.offsets, [0]);
        assert_eq!(&hay[m.ranges(&hay)[0].clone()], family);
    }

    #[test]
    fn an_emoji_inside_a_cluster_is_not_that_cluster() {
        assert!(FuzzyMatch::find("👩", "👨\u{200D}👩\u{200D}👧").is_none(), "Character == compares whole clusters");
        assert!(FuzzyMatch::find("🇺", "🇺🇸").is_none());
        assert!(FuzzyMatch::find("e", "e\u{301}").is_none());
        assert!(FuzzyMatch::find("1", "1\u{FE0F}\u{20E3}").is_none());
        assert_eq!(FuzzyMatch::find("🇺🇸", "a🇺🇸").unwrap().offsets, [1]);
    }

    #[test]
    fn canonical_equivalents_match_as_character_equality_does() {
        assert_eq!(FuzzyMatch::find("\u{E9}", "cafe\u{301}").unwrap().offsets, [3], "NFC query, NFD haystack");
        assert_eq!(FuzzyMatch::find("e\u{301}", "caf\u{E9}").unwrap().offsets, [3], "NFD query, NFC haystack");
        assert_eq!(FuzzyMatch::find("が", "か\u{3099}き").unwrap().offsets, [0], "Japanese file name from HFS+");
        assert_eq!(FuzzyMatch::find("한", "\u{1112}\u{1161}\u{11AB}글").unwrap().offsets, [0], "Hangul jamo");
    }

    #[test]
    fn lowercasing_is_scalar_by_scalar() {
        assert!(FuzzyMatch::find("σ", "ΑΣ").is_some(), "no final sigma, as Swift's lowercased()");
        assert_eq!(lowercased("ΑΣ"), "ασ");
        assert_eq!(FuzzyMatch::find("İ", "xİ").unwrap().offsets, [1]);
    }

    #[test]
    fn scoring_matches_swift_values() {
        // Numbers from running the Swift code.
        assert_eq!(FuzzyMatch::find("kyoto", "we talked about Kyoto").unwrap().score, 31);
        assert_eq!(FuzzyMatch::find("kt", "Kyoto Trip").unwrap().score, 11);
        assert_eq!(FuzzyMatch::find("京都", "Kyoto 京都 trip").unwrap().score, 13);
        assert_eq!(FuzzyMatch::find("cj", "CubeJournal").unwrap().score, 7, "no camelCase bonus: h is lowercased");
    }

    #[test]
    fn cjk_separators_start_words() {
        let plain = FuzzyMatch::find("京", "东京").unwrap();
        let after_comma = FuzzyMatch::find("京", "东，京").unwrap();
        let after_mark = FuzzyMatch::find("京", "东、京").unwrap();
        assert_eq!(plain.score + 6, after_comma.score);
        assert_eq!(plain.score + 6, after_mark.score);
    }

    #[test]
    fn body_hits_highlight_whole_characters() {
        let family = "👨\u{200D}👩\u{200D}👧";
        // Swift crashes here: its offset for the hit points past the only character.
        let s = body_snippet(family, "👩");
        assert_eq!(highlighted(&s), [family]);
        let s = body_snippet("🇺🇸", "🇸");
        assert_eq!(highlighted(&s), ["🇺🇸"]);
        let s = body_snippet(&format!("前面 {family} 后面"), "👧");
        assert_eq!(highlighted(&s), [family]);
        let s = body_snippet("e\u{301}x", "e");
        assert_eq!(highlighted(&s), ["e\u{301}"], "a literal hit on the base letter takes the mark along");
        let s = body_snippet("cafe\u{301} au lait", "cafe");
        assert_eq!(highlighted(&s), ["cafe\u{301}"]);
    }

    #[test]
    fn body_search_finds_cjk_and_keeps_line_breaks_out() {
        let s = body_snippet("第一行\r\n第二行有京都的事\r\n第三行", "京都");
        assert_eq!(s.text, "第二行有京都的事");
        assert_eq!(highlighted(&s), ["京都"]);
        let s = body_snippet("甲\u{2028}乙京都丙\u{2029}丁", "京都");
        assert_eq!(s.text, "乙京都丙");
        let s = body_snippet("甲\u{85}京都", "京都");
        assert_eq!(s.text, "京都");
        let s = body_snippet("甲\u{C}京都\u{B}丙", "京都");
        assert_eq!(s.text, "甲\u{C}京都\u{B}丙", "VT and FF don't end a line");
    }

    #[test]
    fn folding_maps_offsets_back_when_lengths_change() {
        let s = body_snippet("ＡＩ 和 Straße 的事", "strasse");
        assert_eq!(highlighted(&s), ["Straße"]);
        let s = body_snippet("ＡＩ 和 Straße 的事", "ai");
        assert_eq!(highlighted(&s), ["ＡＩ"]);
        let s = body_snippet("ＡＩ 和 Straße 的事", "ＡＩ");
        assert_eq!(highlighted(&s), ["ＡＩ"]);
        let s = body_snippet("ＡＩ 和 Straße 的事", "的事");
        assert_eq!(highlighted(&s), ["的事"]);
        let s = body_snippet("ß", "s");
        assert_eq!(highlighted(&s), ["ß"], "half of a folded character takes the character");
        assert_eq!(fold("ＡＩ Straße ΑΣς ﬁ ｶﾞ ｱ"), "ai strasse ασσ fi \u{30AB}\u{3099} \u{30A2}");
    }

    #[test]
    fn folding_table_is_sorted() {
        assert!(FOLD_EXCEPTIONS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(FOLD_EXCEPTIONS.iter().all(|e| !(0x3000..0xA640).contains(&e.0)), "the CJK shortcut skips nothing");
        for c in '\u{3000}'..'\u{A640}' {
            assert_eq!(c.to_lowercase().collect::<String>(), c.to_string(), "{c}");
        }
    }

    #[test]
    fn the_window_counts_utf16_units() {
        // 150 characters before the hit: the line is cut 120 units back, which is
        // 120 CJK characters but 60 emoji.
        let cjk = format!("{}京都", "字".repeat(150));
        let notes = [body_note(&cjk)];
        let found = hits(&notes, "京都", 1);
        let s = found[0].snippet.as_ref().unwrap();
        assert_eq!(s.text.graphemes(true).count(), 1 + 24 + 2, "ellipsis, then the snippet's own 24");
        let (line, m, at) = content_match(&cjk, &FoldedBody::new(&cjk), "京都").unwrap();
        assert_eq!(line.graphemes(true).count(), 122);
        assert_eq!(m.offsets, [120, 121]);
        assert_eq!(at, 150 * 3..150 * 3 + 6, "the hit, in the whole body");

        let emoji = format!("{}京都", "😀".repeat(150));
        let (line, m, _) = content_match(&emoji, &FoldedBody::new(&emoji), "京都").unwrap();
        assert_eq!(line.graphemes(true).count(), 62);
        assert_eq!(m.offsets, [60, 61]);

        let tail = format!("京都{}", "字".repeat(300));
        let (line, _, at) = content_match(&tail, &FoldedBody::new(&tail), "京都").unwrap();
        assert_eq!(line.graphemes(true).count(), 202);
        assert_eq!(at, 0..6);
    }

    #[test]
    fn snippets_keep_clusters_whole() {
        let family = "👨\u{200D}👩\u{200D}👧";
        let line = format!("{}京都{}", family.repeat(40), family.repeat(60));
        let m = FuzzyMatch::find("京都", &line).unwrap();
        let s = snippet(&line, &m);
        assert_eq!(highlighted(&s), ["京都"]);
        assert_on_graphemes(&s.text, &s.ranges);
        assert!(s.text.starts_with('…') && s.text.ends_with('…'));
        assert_eq!(s.text.graphemes(true).count(), 1 + 24 + 48 + 1 + 1, "ellipsis, 24 before, match through 48 past its last character, ellipsis");
    }

    #[test]
    fn snippet_trims_only_at_a_real_edge() {
        let s = snippet("\u{3000} 全角空格 京都 \u{3000}", &FuzzyMatch::find("京都", "\u{3000} 全角空格 京都 \u{3000}").unwrap());
        assert_eq!(s.text, "全角空格 京都");
        assert_eq!(highlighted(&s), ["京都"]);
        let line = "# #tag";
        let s = snippet(line, &FuzzyMatch::find("#tag", line).unwrap());
        assert_eq!(s.text, "#tag");
        assert_eq!(highlighted(&s), ["#tag"]);
        let line = "## a";
        let s = snippet(line, &FuzzyMatch::find("#", line).unwrap());
        assert_eq!(s.text, "## a", "never cuts into the match");
    }

    #[test]
    fn a_match_on_trimmed_whitespace_does_not_panic() {
        // Swift's text.index(offsetBy:) would trap on the negative offset.
        let line = "  ab";
        let m = FuzzyMatch { score: 0, offsets: vec![0, 1, 2] };
        let s = snippet(line, &m);
        assert_eq!(s.text, "ab");
        assert_eq!(highlighted(&s), ["a"]);
    }

    #[test]
    fn keycap_hash_is_a_query_not_tag_mode() {
        let notes = [body_note("按 #\u{FE0F}\u{20E3} 键")];
        let found = hits(&notes, "#\u{FE0F}\u{20E3}", DEFAULT_LIMIT);
        assert_eq!(found[0].field, NoteMatchField::Content);
        let mut tagged = body_note("");
        tagged.tags = vec!["AI/编程".into(), "编程".into()];
        let notes = [tagged];
        let found = hits(&notes, " # 编程 ", DEFAULT_LIMIT);
        assert_eq!(found[0].subtitle, "编程", "the better score wins, and spaces trim");
        assert_eq!(found[0].subtitle_ranges, [0..6]);
    }

    #[test]
    fn title_and_subtitle_ranges_are_byte_ranges_of_what_they_highlight() {
        let mut n = body_note("");
        n.path = PathBuf::from("/ws/😀京都笔记.md");
        let notes = [n];
        let found = hits(&notes, "京笔", DEFAULT_LIMIT);
        assert_eq!(found[0].field, NoteMatchField::FileName);
        let name = found[0].note.filename();
        assert_eq!(found[0].title_ranges.iter().map(|r| &name[r.clone()]).collect::<Vec<_>>(), ["京", "笔"]);
    }

    #[test]
    fn weights_truncate_toward_zero() {
        assert_eq!(weighted(0.55, 36), 19);
        assert_eq!(weighted(0.95, 13), 12);
        assert_eq!(weighted(0.55, -3), -1);
    }

    #[test]
    fn query_trimming_uses_foundation_sets() {
        assert_eq!(trim_whitespaces_and_newlines("\u{200B} a\u{2028}\u{85}"), "a");
        assert_eq!(trim_whitespaces(" a\n\u{3000}"), "a\n");
        assert_eq!(trim_whitespaces("\u{200B}a"), "a");
        assert_eq!(trim_whitespaces("\u{2028}a"), "\u{2028}a");
    }

    #[test]
    fn line_ranges_match_nsstring() {
        // (text, location, start, length) from running NSString.lineRange.
        for (text, at, start, length) in [
            ("a\nb", 1, 0, 2),
            ("a\nb", 2, 2, 1),
            ("a\r\nb", 1, 0, 3),
            ("a\r\nb", 2, 0, 3),
            ("a\r\nb", 3, 3, 1),
            ("a\rb", 2, 2, 1),
            ("ab\n", 3, 3, 0),
            ("\n\n", 1, 1, 1),
            ("a\u{B}b", 2, 0, 3),
            ("a\u{C}b", 2, 0, 3),
        ] {
            assert_eq!(line_range(text, at..at), start..start + length, "{text:?} at {at}");
        }
        assert_eq!(line_range("a\u{85}b", 3..3), 3..4);
        assert_eq!(line_range("a\u{2028}b", 4..4), 4..5);
        assert_eq!(line_range("a\u{2029}b", 4..4), 4..5);
    }

    #[test]
    fn prefix_and_suffix_respect_characters() {
        assert!(!has_prefix("e\u{301}x", "e"));
        assert!(has_prefix("ex", "e"));
        assert!(!has_suffix("a\r\n", "\n"));
        assert!(has_suffix("a\n", "\n"));
        assert_eq!(drop_first_grapheme("👨\u{200D}👩x"), "x");
    }

    fn synthetic_notes(count: usize) -> Vec<Note> {
        const WORDS: [&str; 16] = [
            "今天", "讨论", "产品", "设计", "搜索", "笔记", "同步", "the", "quick", "notes", "Markdown", "editor",
            "性能", "缓存", "索引", "plan",
        ];
        // A small deterministic generator, so runs compare.
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move |bound: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % bound as u64) as usize
        };
        (0..count)
            .map(|i| {
                let title: Vec<&str> = (0..2 + next(4)).map(|_| WORDS[next(WORDS.len())]).collect();
                let title = format!("{} {i}", title.join(" "));
                let mut body = format!("# {title}\n\n");
                // Around 800 characters a note, the size of a 4M-character vault at 5000.
                while body.chars().count() < 800 {
                    for _ in 0..12 {
                        body.push_str(WORDS[next(WORDS.len())]);
                        body.push(' ');
                    }
                    body.push_str("。\n");
                }
                let folder = if i % 3 == 0 { "/".to_string() } else { format!("/{}/{}", WORDS[i % 16], WORDS[(i * 7) % 16]) };
                let dir = if folder == "/" { String::new() } else { folder.clone() };
                let path = format!("/ws{dir}/{title}.md");
                Note {
                    id: path.clone(),
                    path: PathBuf::from(path),
                    title,
                    content: body,
                    tags: vec![WORDS[i % 16].to_string(), format!("AI/{}", WORDS[(i * 3) % 16])],
                    created_at: i as f64,
                    updated_at: i as f64,
                    pinned: i % 50 == 0,
                    relative_path: folder,
                }
            })
            .collect()
    }

    /// Run with `cargo test -p fumi-core --offline --release fuzzy_search::extra_tests::search_benchmark -- --ignored --nocapture`.
    #[test]
    #[ignore = "benchmark: prints timings"]
    fn search_benchmark() {
        const QUERIES: [&str; 8] = ["kyoto", "产品", "索引 plan", "the quick", "zqx", "mdn", "#ai", "缓存性能"];
        for count in [1000, 5000] {
            let notes = synthetic_notes(count);
            let chars: usize = notes.iter().map(|n| n.content.chars().count()).sum();
            let started = Instant::now();
            let corpus = SearchCorpus::new(notes.clone());
            let build = started.elapsed();
            println!("{count} notes, {chars} body characters: corpus built in {build:.2?}");
            for query in QUERIES {
                let mut best = f64::MAX;
                let mut worst = 0f64;
                let mut found = 0;
                for _ in 0..7 {
                    let started = Instant::now();
                    found = std::hint::black_box(corpus.hits(query, DEFAULT_LIMIT)).len();
                    let ms = started.elapsed().as_secs_f64() * 1000.0;
                    best = best.min(ms);
                    worst = worst.max(ms);
                }
                let started = Instant::now();
                let direct = std::hint::black_box(hits(&notes, query, DEFAULT_LIMIT)).len();
                let unprepared = started.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(direct, found);
                println!("  {query:<12} {found:>4} hits  corpus {best:>7.2}-{worst:>7.2} ms  no corpus {unprepared:>7.2} ms");
            }
        }
    }
}
