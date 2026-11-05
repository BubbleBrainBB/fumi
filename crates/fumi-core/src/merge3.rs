//! Three-way merge of a note, line by line: what the editor has (`mine`) and
//! what is now on disk (`theirs`), both edited from the same `base`. A
//! Markdown paragraph is one line, so this merges paragraph by paragraph.
//! Port of `Services/Merge3.swift`.
//!
//! Lines are split the way Swift's `split(separator: "\n")` splits a `String`:
//! at every LF that isn't the second half of a CRLF, because Swift treats CRLF
//! as one `Character`. A note written entirely with CRLF is therefore one
//! "line" here, as it is in the Swift app. Lines compare by their bytes.

/// The outcome of [`merge`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult {
    /// The merged text. Where both sides changed the same lines differently,
    /// `mine` is kept.
    pub text: String,
    /// How many such places there were.
    pub conflicts: usize,
}

/// Merges `mine` and `theirs`, both edited from `base`.
pub fn merge(base: &str, mine: &str, theirs: &str) -> MergeResult {
    if mine == theirs || theirs == base {
        return MergeResult { text: mine.to_string(), conflicts: 0 };
    }
    if mine == base {
        return MergeResult { text: theirs.to_string(), conflicts: 0 };
    }

    let b = lines(base);
    let m = lines(mine);
    let t = lines(theirs);
    let to_mine = matches(&b, &m);
    let to_theirs = matches(&b, &t);

    // diff3: base lines kept by both sides are fixed points; between two of
    // them each side either left the base alone or changed it.
    let mut out: Vec<&str> = Vec::new();
    let mut conflicts = 0;
    let (mut next_b, mut next_m, mut next_t) = (0, 0, 0);
    for i in 0..=b.len() {
        let fixed = if i == b.len() {
            (m.len(), t.len())
        } else if let (Some(mi), Some(ti)) = (to_mine[i], to_theirs[i]) {
            (mi, ti)
        } else {
            continue;
        };
        let base_chunk = &b[next_b..i];
        let mine_chunk = &m[next_m..fixed.0];
        let theirs_chunk = &t[next_t..fixed.1];
        if mine_chunk == base_chunk {
            out.extend_from_slice(theirs_chunk);
        } else if theirs_chunk == base_chunk || theirs_chunk == mine_chunk {
            out.extend_from_slice(mine_chunk);
        } else {
            out.extend_from_slice(mine_chunk);
            conflicts += 1;
        }
        if i < b.len() {
            out.push(b[i]);
        }
        (next_b, next_m, next_t) = (i + 1, fixed.0 + 1, fixed.1 + 1);
    }
    MergeResult { text: out.join("\n"), conflicts }
}

/// Where `location` (a byte offset in `old`) lands in `new`: lines both share
/// keep their offset within the line; a location inside lines that changed
/// lands at the same distance into what replaced them, at most its end. Keeps
/// the caret on the text it was on when a merge is put into the editor.
///
/// Returns a byte offset on a character boundary of `new`, at most its length.
/// "Same distance" counts UTF-16 units, as the Swift code does, so the caret
/// lands on the same character as it does there.
pub fn map_location(location: usize, old: &str, new: &str) -> usize {
    let location = floor_boundary(old, location);
    let o = lines(old);
    let n = lines(new);
    let pairs = matches(&o, &n);
    let (mut old_start, mut new_start) = (0, 0);
    let (mut next_o, mut next_n) = (0, 0);
    for i in 0..=o.len() {
        let j = if i == o.len() {
            n.len()
        } else if let Some(j) = pairs[i] {
            j
        } else {
            continue;
        };
        // The changed run before this kept line, then the line itself. A
        // location at the start of the kept line stays with that line.
        let old_run = byte_length(&o[next_o..i]);
        let new_run = byte_length(&n[next_n..j]);
        if location < old_start + old_run {
            // At most the end of the replacement's last line, not past its break.
            let distance = utf16_length(&old[old_start..location]);
            let limit = utf16_run_length(&n[next_n..j]).saturating_sub(1);
            return advance_utf16(new, new_start, distance.min(limit));
        }
        old_start += old_run;
        new_start += new_run;
        if i == o.len() {
            break;
        }
        let line_length = o[i].len() + 1;
        if location < old_start + line_length {
            return (new_start + (location - old_start)).min(new.len());
        }
        old_start += line_length;
        new_start += line_length;
        (next_o, next_n) = (i + 1, j + 1);
    }
    new.len()
}

/// `text` split at each LF that isn't part of a CRLF, empty lines kept.
fn lines(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b'\n' && (i == 0 || bytes[i - 1] != b'\r') {
            out.push(&text[start..i]);
            start = i + 1;
        }
    }
    out.push(&text[start..]);
    out
}

/// Byte length of `lines` with the line break after each.
fn byte_length(lines: &[&str]) -> usize {
    lines.iter().map(|line| line.len() + 1).sum()
}

fn utf16_length(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// UTF-16 length of `lines` with the line break after each.
fn utf16_run_length(lines: &[&str]) -> usize {
    lines.iter().map(|line| utf16_length(line) + 1).sum()
}

/// The byte offset `units` UTF-16 units after `start`, never inside a
/// character and never past the end of `text`.
fn advance_utf16(text: &str, start: usize, units: usize) -> usize {
    if start >= text.len() {
        return text.len();
    }
    let mut offset = start;
    let mut left = units;
    for ch in text[start..].chars() {
        let width = ch.len_utf16();
        if width > left {
            break;
        }
        left -= width;
        offset += ch.len_utf8();
    }
    offset
}

fn floor_boundary(text: &str, mut offset: usize) -> usize {
    if offset >= text.len() {
        return offset;
    }
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// For each line of `a`, the line of `b` it stays as, or `None` when it was
/// removed or changed. Increasing, from a shortest diff.
fn matches(a: &[&str], b: &[&str]) -> Vec<Option<usize>> {
    let (removed, inserted) = difference(a, b);
    let mut result = vec![None; a.len()];
    let mut j = 0;
    for i in 0..a.len() {
        if removed[i] {
            continue;
        }
        while inserted[j] {
            j += 1;
        }
        result[i] = Some(j);
        j += 1;
    }
    result
}

/// Which lines of `a` a shortest edit script removes and which lines of `b` it
/// inserts. When several shortest scripts exist, picks the one Swift's
/// `BidirectionalCollection.difference(from:)` picks, so merges come out the
/// same: this is the linear-space Myers search of the Swift 6.4 standard library
/// (macOS 27), with its order of search, checked against it on random input.
fn difference(a: &[&str], b: &[&str]) -> (Vec<bool>, Vec<bool>) {
    let mut removed = vec![false; a.len()];
    let mut inserted = vec![false; b.len()];
    let mut pending = vec![Rect { left: 0, top: 0, right: a.len() as isize, bottom: b.len() as isize }];
    while let Some(rect) = pending.pop() {
        let rect = rect.shrunk(a, b);
        if rect.left == rect.right {
            inserted[rect.top as usize..rect.bottom as usize].fill(true);
        } else if rect.top == rect.bottom {
            removed[rect.left as usize..rect.right as usize].fill(true);
        } else {
            let step = middle_step(rect, a, b);
            let (marks, index) = match step.change {
                Change::Remove(index) => (&mut removed, index),
                Change::Insert(index) => (&mut inserted, index),
            };
            marks[index as usize] = true;
            pending.push(step.before);
            pending.push(step.after);
        }
    }
    (removed, inserted)
}

/// Part of the edit graph: lines `left..right` of `a` against `top..bottom` of `b`.
#[derive(Clone, Copy)]
struct Rect {
    left: isize,
    top: isize,
    right: isize,
    bottom: isize,
}

impl Rect {
    /// Without the lines the two sides share at the start, then at the end.
    fn shrunk(mut self, a: &[&str], b: &[&str]) -> Self {
        while self.left < self.right && self.top < self.bottom && a[self.left as usize] == b[self.top as usize] {
            self.left += 1;
            self.top += 1;
        }
        while self.left < self.right && self.top < self.bottom && a[self.right as usize - 1] == b[self.bottom as usize - 1] {
            self.right -= 1;
            self.bottom -= 1;
        }
        self
    }
}

enum Change {
    Remove(isize),
    Insert(isize),
}

/// One edit on a shortest path through a rect, and what's left on either side.
struct Step {
    change: Change,
    before: Rect,
    after: Rect,
}

/// Searches from both corners of a rect with lines on both sides until the two
/// searches meet. The forward search keeps the furthest x of each diagonal, the
/// backward search the smallest y.
fn middle_step(rect: Rect, a: &[&str], b: &[&str]) -> Step {
    let Rect { left, top, right, bottom } = rect;
    let delta = (right - left) - (bottom - top);
    let odd = delta % 2 != 0;
    // Two searches of depth d can span 2d changes, and the sides at most this many.
    let limit = (right - left) + (bottom - top) + 2;
    let at = |k: isize| (k + limit) as usize;
    let mut forward = vec![left; 2 * limit as usize + 1];
    let mut backward = vec![bottom; 2 * limit as usize + 1];

    let mut d = 0;
    loop {
        let mut k = d;
        while k >= -d {
            let (mut x, mut y, change, before_x, before_y);
            if k == -d || (k != d && forward[at(k - 1)] < forward[at(k + 1)]) {
                x = forward[at(k + 1)];
                y = x - left + top - k;
                change = Change::Insert(y - 1);
                (before_x, before_y) = (x, y - 1);
            } else {
                x = forward[at(k - 1)] + 1;
                y = x - left + top - k;
                change = Change::Remove(x - 1);
                (before_x, before_y) = (x - 1, y);
            }
            while x < right && y < bottom && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            forward[at(k)] = x;
            let other = k - delta;
            if odd && -(d - 1) <= other && other <= d - 1 && y >= backward[at(other)] {
                return Step {
                    change,
                    before: Rect { left, top, right: before_x, bottom: before_y },
                    after: Rect { left: x, top: y, right, bottom },
                };
            }
            k -= 2;
        }

        let mut k = d;
        while k >= -d {
            let (mut x, mut y, change, after_x, after_y);
            if k == -d || (k != d && backward[at(k + 1)] < backward[at(k - 1)]) {
                y = backward[at(k + 1)];
                x = y - top + left + k + delta;
                change = Change::Remove(x);
                (after_x, after_y) = (x + 1, y);
            } else {
                y = backward[at(k - 1)] - 1;
                x = y - top + left + k + delta;
                change = Change::Insert(y);
                (after_x, after_y) = (x, y + 1);
            }
            while x > left && y > top && a[x as usize - 1] == b[y as usize - 1] {
                x -= 1;
                y -= 1;
            }
            backward[at(k)] = y;
            let other = k + delta;
            if !odd && -d <= other && other <= d && forward[at(other)] >= x {
                return Step {
                    change,
                    before: Rect { left, top, right: x, bottom: y },
                    after: Rect { left: after_x, top: after_y, right, bottom },
                };
            }
            k -= 2;
        }
        d += 1;
    }
}

#[cfg(test)]
mod tests {
    // Ported from Tests/Merge3Tests.swift.
    use super::*;

    fn result(text: &str, conflicts: usize) -> MergeResult {
        MergeResult { text: text.to_string(), conflicts }
    }

    #[test]
    fn trivial_cases() {
        assert_eq!(merge("a", "a2", "a"), result("a2", 0), "only mine changed → mine");
        assert_eq!(merge("a", "a", "a3"), result("a3", 0), "only theirs changed → theirs");
        assert_eq!(merge("a", "same", "same"), result("same", 0), "both made the same change → it");
    }

    #[test]
    fn merges_different_paragraphs() {
        let base = "# W43\n\nDone\n\n## Next\n\n- find";
        // The agent edits the top while the user types at the bottom.
        let mine = "# W43\n\nDone\n\n## Next\n\n- find and replace";
        let theirs = "# W43\n\nDone, three templates\n\n## Next\n\n- find";
        assert_eq!(
            merge(base, mine, theirs),
            result("# W43\n\nDone, three templates\n\n## Next\n\n- find and replace", 0),
            "edits in different paragraphs → both kept"
        );

        // The agent adds a section; the user edits another line.
        let added = "# W43\n\nDone\n\n## Risks\n\nNo history yet\n\n## Next\n\n- find";
        assert_eq!(
            merge(base, mine, added),
            result("# W43\n\nDone\n\n## Risks\n\nNo history yet\n\n## Next\n\n- find and replace", 0),
            "a section added outside + a line edited here → both kept"
        );

        // The agent removes lines the user didn't touch.
        let removed = "# W43\n\n## Next\n\n- find";
        assert_eq!(
            merge(base, mine, removed),
            result("# W43\n\n## Next\n\n- find and replace", 0),
            "lines removed outside stay removed"
        );

        // A trailing newline on both sides survives.
        assert_eq!(merge("a\n\nb\n", "a!\n\nb\n", "a\n\nb!\n"), result("a!\n\nb!\n", 0), "trailing newline kept");

        // Chinese text, paragraph by paragraph.
        assert_eq!(
            merge("本周完成\n\n下周计划", "本周完成了三件事\n\n下周计划", "本周完成\n\n下周计划：查找替换"),
            result("本周完成了三件事\n\n下周计划：查找替换", 0),
            "CJK paragraphs merge"
        );
    }

    #[test]
    fn conflicts() {
        assert_eq!(
            merge("a\n\nb", "a mine\n\nb", "a theirs\n\nb"),
            result("a mine\n\nb", 1),
            "same paragraph changed both ways → mine, 1 conflict"
        );

        assert_eq!(
            merge("a\n\nb\n\nc", "a1\n\nb\n\nc1", "a2\n\nb\n\nc2"),
            result("a1\n\nb\n\nc1", 2),
            "two conflicting paragraphs counted apart"
        );

        // Mine and theirs both insert different text at the same place.
        assert_eq!(
            merge("a\n\nb", "a\n\nmine\n\nb", "a\n\ntheirs\n\nb"),
            result("a\n\nmine\n\nb", 1),
            "different inserts at one place → conflict"
        );

        // A conflict next to a clean change: the clean one still comes through.
        assert_eq!(
            merge("a\n\nb\n\nc", "a1\n\nb\n\nc", "a2\n\nb\n\nc2"),
            result("a1\n\nb\n\nc2", 1),
            "conflict doesn't block other changes"
        );
    }

    #[test]
    fn map_location_cases() {
        let old = "first\nsecond\nthird";
        // A line inserted above: the caret moves down with its line.
        let inserted = "zero\nfirst\nsecond\nthird";
        assert_eq!(map_location(8, old, inserted), 13, "caret follows its line past an insert above");
        assert_eq!(map_location(6, old, inserted), 11, "caret at a line start stays with that line");

        // A line changed below: the caret doesn't move.
        let changed_below = "first\nsecond\nTHIRD!!";
        assert_eq!(map_location(8, old, changed_below), 8, "change below leaves the caret");

        // A line removed above.
        let removed_above = "second\nthird";
        assert_eq!(map_location(8, old, removed_above), 2, "caret follows its line past a removal above");

        // The caret inside a changed line lands inside what replaced it.
        let changed_here = "first\nsec\nthird";
        assert_eq!(map_location(10, old, changed_here), 9, "inside a shorter replacement → at its end");
        assert_eq!(map_location(7, old, changed_here), 7, "inside a replacement → same distance in");

        // The end of the text.
        assert_eq!(map_location(18, old, inserted), 23, "end stays at the end");
        assert_eq!(map_location(0, "", "new"), 0, "empty old text");
    }

    // ── Beyond the Swift test ──

    #[test]
    fn crlf_is_one_line_as_in_swift() {
        // Swift's `split(separator: "\n")` doesn't split at CRLF, so a CRLF
        // note is one line: edits to different paragraphs conflict, mine wins.
        assert_eq!(lines("a\r\nb\nc"), vec!["a\r\nb", "c"]);
        assert_eq!(
            merge("a\r\n\r\nb", "a!\r\n\r\nb", "a\r\n\r\nb!"),
            result("a!\r\n\r\nb", 1),
            "whole CRLF text conflicts, bytes of mine kept"
        );
    }

    #[test]
    fn merged_bytes_are_kept_exactly() {
        // Lines the merge doesn't touch keep their unusual bytes.
        let base = "x\u{2028}y  \n\tz\u{3000}\n\nlast";
        let mine = "x\u{2028}y  \n\tz\u{3000}\n\nlast!";
        let theirs = "x\u{2028}y  !\n\tz\u{3000}\n\nlast";
        assert_eq!(merge(base, mine, theirs), result("x\u{2028}y  !\n\tz\u{3000}\n\nlast!", 0));
    }

    #[test]
    fn map_location_with_cjk_and_emoji() {
        // A kept CJK line: bytes after a line inserted above.
        assert_eq!(map_location("本周\n".len() + 3, "本周\n下周", "新\n本周\n下周"), "新\n本周\n下".len());
        // Inside a replaced line: the same character count in UTF-16 units.
        assert_eq!(map_location(6, "本周完成\nb", "下周计划\nb"), 6);
        // A replacement never lands inside a character.
        assert_eq!(map_location(1, "ab\nz", "😀\nz"), 0, "one UTF-16 unit into an emoji stays before it");
        assert_eq!(map_location(2, "ab\nz", "😀\nz"), 4);
        // A location that isn't on a boundary doesn't panic.
        assert_eq!(map_location(1, "本\nb", "本\nb"), 0);
    }

    #[test]
    fn ties_go_the_way_swift_breaks_them() {
        // Each expected value printed by Swift's `difference(from:)` on macOS 27.
        let cases: [(&[&str], &[&str], &[usize], &[usize]); 12] = [
            (&["", "a", "", "e", "p", "c", "x", "r"], &["b", "", "A", "e", "p", "c", "x", "r"], &[0, 1], &[0, 2]),
            (&["a", "b", "c", "a", "b", "b", "a"], &["c", "b", "a", "b", "a", "c"], &[0, 1, 3], &[2, 5]),
            (&["a", "b"], &["b", "a"], &[0], &[1]),
            (&["a", "b", "c"], &["c", "b", "a"], &[0, 1], &[1, 2]),
            (&["x", "a", "x"], &["a", "x", "a"], &[0], &[2]),
            (&["", "", ""], &[""], &[1, 2], &[]),
            (&[""], &["", "", ""], &[], &[1, 2]),
            (&["", "c", "c"], &["c", "a", ""], &[0, 1], &[1, 2]),
            (&["b", "c"], &["c", "", "b"], &[0], &[1, 2]),
            (&["a", "c", "c", "a", ""], &["c"], &[0, 1, 3, 4], &[]),
            (&["c"], &["b", "c", "c", "b"], &[], &[0, 2, 3]),
            (&["a", "", ""], &["", "a"], &[0, 2], &[1]),
        ];
        for (a, b, removed, inserted) in cases {
            let (got_removed, got_inserted) = difference(a, b);
            let indices = |marks: Vec<bool>| marks.iter().enumerate().filter(|(_, mark)| **mark).map(|(i, _)| i).collect::<Vec<_>>();
            assert_eq!(indices(got_removed), removed, "{a:?} -> {b:?}");
            assert_eq!(indices(got_inserted), inserted, "{a:?} -> {b:?}");
        }
        // Where another shortest diff would give another merge. Printed by Merge3.swift.
        assert_eq!(
            merge("c\nc\n\r\n\n😀 z\na\n\nq  \n😀 z\n\r\n中文", "c\n\r\n\n\na\n\nq  \n😀 z\n\r\n中文", "中文\nc\n\r\n\n😀 z\na\nq  \n😀 z\n\r\n中文"),
            result("中文\nc\n\r\n\n\na\nq  \n😀 z\n\r\n中文", 0)
        );
        assert_eq!(
            merge(
                "😀 z\n\nq  \nq  \n中文\n😀 z\na\r\nq  \na\n\r\n中文\n😀 z\nq  ",
                "😀 z\n\nq  \n中文\ne\u{301}\n\r\n中文\n😀 z\n中文\nq  ",
                "😀 z\n\nq  \n中文\n中文\n😀 z\na\r\nq  \na\n\n😀 z\nq  "
            ),
            result("😀 z\n\nq  \n中文\ne\u{301}\n\r\n中文\n😀 z\n中文\nq  ", 2)
        );
    }

    #[test]
    fn map_location_never_passes_the_end() {
        // Swift returns 2 here, past the end of the one-byte text.
        assert_eq!(map_location(2, "a\nb", "a"), 1);
    }
}
