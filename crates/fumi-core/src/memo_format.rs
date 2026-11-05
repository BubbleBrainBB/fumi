//! Memos: short, untitled entries, one Markdown file per day
//! (`Memos/2026-10-16.md`), each under a `## 09:31` heading, so the day's file
//! reads as a plain journal in any Markdown app. Port of
//! `Services/MemoFormat.swift`.
//!
//! Lines are split on a bare `\n` only, as Swift's `split(separator: "\n")`
//! does: `\r\n` is one `Character` there, so a CRLF is not a line break here
//! and a heading ended by one is not a memo. A leading BOM is set aside:
//! Swift's file reading strips it before these functions run, and rewrites here
//! must keep it.
//!
//! Dates are seconds since the Unix epoch, and the caller gives the UTC offset
//! in force, in seconds.

use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::fuzzy_search::foundation::{
    has_prefix, has_suffix, strip_regex_final_terminator, trim_whitespaces_and_newlines,
};

const BOM: &str = "\u{FEFF}";
const SECONDS_PER_DAY: i64 = 86_400;

/// The folder, inside the notes library, that holds the day files.
pub const FOLDER_NAME: &str = "Memos";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Memo {
    /// The day's file name without extension, "2026-10-16".
    pub day: String,
    /// Position among the memos of that day's file.
    pub index: usize,
    /// "09:31".
    pub time: String,
    pub text: String,
}

impl Memo {
    pub fn id(&self) -> String {
        format!("{}#{}", self.day, self.index)
    }

    /// `#tags` in the text, without the `#`.
    pub fn tags(&self) -> Vec<String> {
        tags(&self.text)
    }
}

// Days and times.

// Days since the epoch of a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn local_seconds(date: f64, utc_offset: i32) -> i64 {
    date.floor() as i64 + i64::from(utc_offset)
}

fn local_day(date: f64, utc_offset: i32) -> i64 {
    local_seconds(date, utc_offset).div_euclid(SECONDS_PER_DAY)
}

fn start_of_local_day(day: i64, utc_offset: i32) -> f64 {
    (day * SECONDS_PER_DAY - i64::from(utc_offset)) as f64
}

/// "2026-10-16", in the time zone at `utc_offset`.
pub fn day_of(date: f64, utc_offset: i32) -> String {
    let (year, month, day) = civil_from_days(local_day(date, utc_offset));
    format!("{year:04}-{month:02}-{day:02}")
}

/// "09:31", in the time zone at `utc_offset`.
pub fn time_of(date: f64, utc_offset: i32) -> String {
    let seconds = local_seconds(date, utc_offset).rem_euclid(SECONDS_PER_DAY);
    format!("{:02}:{:02}", seconds / 3600, seconds / 60 % 60)
}

/// The start of a named day, as `DateFormatter("yyyy-MM-dd")` reads it: a day
/// past its month's end but within 31 rolls over ("2026-02-30" is March 2).
/// `None` for anything else, including digits other than ASCII.
pub fn date_of_day(day: &str, utc_offset: i32) -> Option<f64> {
    let mut parts = day.split('-');
    let (year, month, date) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let number = |part: &str, widest: usize| -> Option<i64> {
        if part.is_empty() || part.len() > widest || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let (year, month, date) = (number(year, 4)?, number(month, 2)?, number(date, 2)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&date) {
        return None;
    }
    Some(start_of_local_day(days_from_civil(year, month, 1) + date - 1, utc_offset))
}

static DAY_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("valid day pattern"));
static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^##[ \t]+(\d{1,2}:\d{2})[ \t]*$").expect("valid heading pattern"));
static TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#(\p{L}[\p{L}\p{N}_/-]*)").expect("valid tag pattern"));

/// Whether a file name (without extension) names a day file.
pub fn is_day(name: &str) -> bool {
    DAY_NAME.is_match(strip_regex_final_terminator(name))
}

// Parsing.

/// The time in a memo heading line (`## 09:31`), or `None`.
fn heading_time(line: &str) -> Option<&str> {
    if !line.starts_with("##") {
        return None;
    }
    let captures = HEADING.captures(strip_regex_final_terminator(line))?;
    Some(captures.get(1)?.as_str())
}

/// Line spans of each memo: its heading line and the body lines after it.
struct Block<'a> {
    time: &'a str,
    heading: usize,
    body_start: usize,
    /// One past the last line.
    end: usize,
}

fn blocks<'a>(lines: &[&'a str]) -> Vec<Block<'a>> {
    let mut heads: Vec<(usize, &'a str)> = Vec::new();
    let mut in_fence = false;
    for (i, line) in lines.iter().enumerate() {
        if has_prefix(line, "```") || has_prefix(line, "~~~") {
            in_fence = !in_fence;
        }
        if !in_fence && let Some(time) = heading_time(line) {
            heads.push((i, time));
        }
    }
    (0..heads.len())
        .map(|n| Block {
            time: heads[n].1,
            heading: heads[n].0,
            body_start: heads[n].0 + 1,
            end: heads.get(n + 1).map_or(lines.len(), |next| next.0),
        })
        .collect()
}

/// Splits on a `\n` that is not the end of a `\r\n`.
fn split_lines(content: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, _) in content.match_indices('\n') {
        if !content[..i].ends_with('\r') {
            lines.push(&content[start..i]);
            start = i + 1;
        }
    }
    lines.push(&content[start..]);
    lines
}

fn without_bom(content: &str) -> (&str, &str) {
    match content.strip_prefix(BOM) {
        Some(rest) => (BOM, rest),
        None => ("", content),
    }
}

/// The memos in a day file, in file order. Text before the first memo heading
/// (a daily note's own content) is not a memo.
pub fn parse(content: &str, day: &str) -> Vec<Memo> {
    let (_, content) = without_bom(content);
    let lines = split_lines(content);
    blocks(&lines)
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let joined = lines[block.body_start..block.end].join("\n");
            let text = trim_whitespaces_and_newlines(&joined);
            (!text.is_empty()).then(|| Memo {
                day: day.to_string(),
                index,
                time: block.time.to_string(),
                text: text.to_string(),
            })
        })
        .collect()
}

// Writing. Each of these changes only what it means to: the entry it adds,
// edits or removes and the blank lines where it joins the rest. Every other
// byte, line endings and final newline included, stays as it was.

/// The end of `content` up to and including the line break after its last text,
/// ignoring the blank lines that follow. `None` if there is only `\n` and `\r`.
fn through_last_line(content: &str) -> Option<&str> {
    let text_end = content.trim_end_matches(['\n', '\r']).len();
    if text_end == 0 {
        return None;
    }
    let breaks = &content[text_end..];
    let kept = if breaks.starts_with("\r\n") {
        2
    } else if breaks.is_empty() {
        0
    } else {
        1
    };
    Some(&content[..text_end + kept])
}

/// `content` with a new memo added at the end.
pub fn appending(text: &str, time: &str, content: &str) -> String {
    let (bom, content) = without_bom(content);
    let entry = format!("## {time}\n{}\n", trim_whitespaces_and_newlines(text));
    match through_last_line(content) {
        None => format!("{bom}{entry}"),
        Some(existing) if existing.ends_with('\n') => format!("{bom}{existing}\n{entry}"),
        // No line break yet, or a lone `\r`, which the next `\n` would join.
        Some(existing) => format!("{bom}{existing}\n\n{entry}"),
    }
}

/// `content` with memo number `index` given new text.
pub fn replacing(index: usize, text: &str, content: &str) -> String {
    let (bom, body) = without_bom(content);
    let mut lines = split_lines(body);
    let all = blocks(&lines);
    let Some(block) = all.get(index) else {
        return content.to_string();
    };
    let (body_start, end) = (block.body_start, block.end);
    let mut new_body = split_lines(trim_whitespaces_and_newlines(text));
    // Keep the gap before the next memo, or the file's final newline.
    if end < lines.len() || has_suffix(body, "\n") {
        new_body.push("");
    }
    lines.splice(body_start..end, new_body);
    format!("{bom}{}", lines.join("\n"))
}

/// `content` without memo number `index`.
pub fn removing(index: usize, content: &str) -> String {
    let (bom, body) = without_bom(content);
    let mut lines = split_lines(body);
    let all = blocks(&lines);
    let Some(block) = all.get(index) else {
        return content.to_string();
    };
    let (heading, end) = (block.heading, block.end);
    if end < lines.len() {
        lines.drain(heading..end);
        return format!("{bom}{}", lines.join("\n"));
    }
    // The last memo: what comes before it ends with its own last line.
    let before: usize = lines[..heading].iter().map(|line| line.len() + 1).sum();
    match through_last_line(&body[..before]) {
        Some(kept) => format!("{bom}{kept}"),
        None => bom.to_string(),
    }
}

// Tags.

/// `#tags` in `text`: names, and byte ranges including the `#`.
pub fn tag_ranges(text: &str) -> Vec<(String, Range<usize>)> {
    if !text.contains('#') {
        return Vec::new();
    }
    TAG.captures_iter(text)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            let opens = text[..whole.start()]
                .chars()
                .next_back()
                .is_none_or(|c| c.is_whitespace() || c == '(' || c == '[');
            opens.then(|| (captures[1].to_string(), whole.range()))
        })
        .collect()
}

pub fn tags(text: &str) -> Vec<String> {
    tag_ranges(text).into_iter().map(|(name, _)| name).collect()
}

/// The heatmap's grid: weeks as columns (Monday first), days as rows. Cells are
/// the start of their day at the UTC offset given, or `None`.
pub mod heatmap {
    use super::{civil_from_days, days_from_civil, local_day, start_of_local_day};

    /// Shade for a day with `count` memos: 0 (none) to 4 (most).
    pub fn level(count: i64) -> u8 {
        match count {
            ..1 => 0,
            1 => 1,
            2 => 2,
            3..=4 => 3,
            _ => 4,
        }
    }

    // Monday = 0. Day 0, 1970-01-01, was a Thursday.
    fn weekday(day: i64) -> i64 {
        (day + 3).rem_euclid(7)
    }

    /// `weeks` columns of seven days, the last one holding `today`. Days after
    /// today are `None`.
    pub fn grid(weeks: usize, today: f64, utc_offset: i32) -> Vec<Vec<Option<f64>>> {
        let start = local_day(today, utc_offset);
        let monday = start - weekday(start);
        let first = monday - (weeks as i64 - 1) * 7;
        (0..weeks as i64)
            .map(|week| {
                (0..7)
                    .map(|day| {
                        let date = first + week * 7 + day;
                        (date <= start).then(|| start_of_local_day(date, utc_offset))
                    })
                    .collect()
            })
            .collect()
    }

    /// The month holding `date` as weeks of seven days, Monday first; days
    /// outside the month are `None`.
    pub fn month(date: f64, utc_offset: i32) -> Vec<Vec<Option<f64>>> {
        let (year, month, _) = civil_from_days(local_day(date, utc_offset));
        let first = days_from_civil(year, month, 1);
        let next = if month == 12 { days_from_civil(year + 1, 1, 1) } else { days_from_civil(year, month + 1, 1) };
        let mut cells: Vec<Option<f64>> = vec![None; weekday(first) as usize];
        cells.extend((first..next).map(|day| Some(start_of_local_day(day, utc_offset))));
        while cells.len() % 7 != 0 {
            cells.push(None);
        }
        cells.chunks(7).map(<[_]>::to_vec).collect()
    }
}

/// The folder holding the day files of a library.
pub fn folder(workspace: &Path) -> PathBuf {
    workspace.join(FOLDER_NAME)
}

/// The day file of `day` in a library.
pub fn file_for_day(day: &str, workspace: &Path) -> PathBuf {
    folder(workspace).join(format!("{day}.md"))
}

/// Adds a memo to the day file for `date`, creating the folder and file as
/// needed, and returns the file it wrote. A file that exists but can't be read
/// as UTF-8 is an error and is left alone.
pub fn append_to_file(text: &str, date: f64, utc_offset: i32, workspace: &Path) -> io::Result<PathBuf> {
    let path = file_for_day(&day_of(date, utc_offset), workspace);
    let parent = folder(workspace);
    fs::create_dir_all(&parent)?;
    let existing = match fs::read_to_string(&path) {
        Ok(existing) => existing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let updated = appending(text, &time_of(date, utc_offset), &existing);
    crate::note_store::write_atomic(&path, updated.as_bytes())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    // Ported from Tests/MemoFormatTests.swift. The Swift tests run in the
    // current time zone; these fix it at UTC+8.
    use super::*;

    const OFFSET: i32 = 8 * 3600;
    const DAY: &str = "## 09:31\n早上散步想到：随手记不该有标题，\n打开就写。 #idea\n\n## 14:02\n搜索正文改成精确匹配之后，结果干净了。 #fumi #search\n";
    const DAILY: &str = "# 2026-10-16\n今天的计划……\n\n## 08:00\n起床\n";

    fn texts(content: &str) -> Vec<String> {
        parse(content, "d").into_iter().map(|m| m.text).collect()
    }

    #[test]
    fn parsing() {
        let memos = parse(DAY, "2026-10-16");
        assert_eq!(memos.len(), 2, "two memos");
        assert_eq!(memos.iter().map(|m| m.time.as_str()).collect::<Vec<_>>(), ["09:31", "14:02"], "times in file order");
        assert_eq!(memos[0].text, "早上散步想到：随手记不该有标题，\n打开就写。 #idea", "multi-line body, trimmed");
        assert_eq!(memos[1].tags(), ["fumi", "search"], "tags");
        assert_eq!(memos[0].id(), "2026-10-16#0", "id is day + index");

        // A daily note's own text before the first memo is not a memo.
        assert_eq!(texts(DAILY), ["起床"], "preamble ignored");

        // Other headings stay inside a memo; a heading in a code block is not a memo.
        let parsed = parse("## 10:00\n## Not a time\n```\n## 11:00\n```\n", "d");
        assert_eq!(parsed.len(), 1, "only time headings outside code start memos");
        assert_eq!(parsed.first().map(|m| m.text.as_str()), Some("## Not a time\n```\n## 11:00\n```"), "body keeps other headings and code");

        // Empty memos are skipped.
        assert_eq!(texts("## 10:00\n\n## 11:00\nhi\n"), ["hi"], "empty memo skipped");
    }

    #[test]
    fn appending_replacing_removing() {
        assert_eq!(appending("  hello  ", "09:00", ""), "## 09:00\nhello\n", "first memo of the day");
        let two = appending("second", "10:00", "## 09:00\nfirst\n");
        assert_eq!(two, "## 09:00\nfirst\n\n## 10:00\nsecond\n", "blank line between memos");
        assert_eq!(texts(&two), ["first", "second"], "appended memo parses back");

        let edited = replacing(0, "changed\ntwo lines", &two);
        assert_eq!(edited, "## 09:00\nchanged\ntwo lines\n\n## 10:00\nsecond\n", "replace first keeps the gap");
        assert_eq!(replacing(1, "last", &two), "## 09:00\nfirst\n\n## 10:00\nlast\n", "replace last keeps the final newline");
        assert_eq!(replacing(5, "x", &two), two, "out-of-range replace is a no-op");

        assert_eq!(removing(0, &two), "## 10:00\nsecond\n", "remove first");
        assert_eq!(removing(1, &two), "## 09:00\nfirst\n", "remove last");
        assert_eq!(removing(0, "## 09:00\nonly\n"), "", "remove the only memo");
        assert_eq!(removing(0, DAILY), "# 2026-10-16\n今天的计划……\n", "remove keeps the preamble");
    }

    #[test]
    fn days() {
        assert!(is_day("2026-10-16"), "day file name");
        assert!(!is_day("Untitled"), "other file name");
        let date = date_of_day("2026-10-16", OFFSET).unwrap();
        assert_eq!(day_of(date, OFFSET), "2026-10-16", "day round trip");
    }

    #[test]
    fn tag_boundaries() {
        assert_eq!(tags("a #one, b#no #二 (#three)"), ["one", "二", "three"], "tag boundaries");
    }

    fn day_at(cell: Option<f64>) -> Option<String> {
        cell.map(|date| day_of(date, OFFSET))
    }

    #[test]
    fn heatmap_grid_and_month() {
        let levels: Vec<u8> = [0, 1, 2, 3, 4, 5, 9].into_iter().map(heatmap::level).collect();
        assert_eq!(levels, [0, 1, 2, 3, 3, 4, 4], "levels");
        let thursday = date_of_day("2026-10-15", OFFSET).unwrap(); // a Thursday
        let grid = heatmap::grid(3, thursday, OFFSET);
        assert_eq!(grid.len(), 3, "three weeks");
        assert!(grid.iter().all(|week| week.len() == 7), "seven days a week");
        assert_eq!(day_at(grid[2][3]).as_deref(), Some("2026-10-15"), "today is the last column's Thursday");
        assert!(grid[2][4].is_none(), "days after today are empty");
        assert_eq!(day_at(grid[0][0]).as_deref(), Some("2026-09-28"), "first cell is the Monday two weeks back");

        let october = heatmap::month(thursday, OFFSET);
        assert_eq!(october.len(), 5, "October 2026 spans five Monday-first weeks");
        assert!(october[0][2].is_none() && october[0][3].is_some(), "October 1 2026 is a Thursday");
        assert_eq!(day_at(october[0][3]).as_deref(), Some("2026-10-01"), "first day");
        assert_eq!(day_at(october[4][5]).as_deref(), Some("2026-10-31"), "last day on Saturday");
        assert!(october[4][6].is_none(), "padding after the last day");
        let february = heatmap::month(date_of_day("2027-02-10", OFFSET).unwrap(), OFFSET);
        assert_eq!(february.len(), 4, "February 2027 starts on Monday and fills four weeks");
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    // Other entries must survive every rewrite byte for byte. Each case is a
    // day file with something unusual in the entries left alone.
    const CJK_FIRST: &str = "## 09:31\n早上散步，😀👨\u{200D}👩\u{200D}👧 想到\u{2028}一件事。  \n\n";
    const CJK_LAST: &str = "## 14:02\n　全角空格开头\t\n第二行\u{0085}结束\u{2029}";

    #[test]
    fn replacing_leaves_other_cjk_entries_alone() {
        let content = format!("{CJK_FIRST}## 10:00\n中间\n\n{CJK_LAST}");
        let edited = replacing(1, "改过了", &content);
        assert_eq!(edited, format!("{CJK_FIRST}## 10:00\n改过了\n\n{CJK_LAST}"));
        let edited = replacing(0, "改过了", &content);
        assert_eq!(edited, format!("## 09:31\n改过了\n\n## 10:00\n中间\n\n{CJK_LAST}"));
        let edited = replacing(2, "改过了", &content);
        assert_eq!(edited, format!("{CJK_FIRST}## 10:00\n中间\n\n## 14:02\n改过了"), "no final newline stays that way");
    }

    #[test]
    fn removing_leaves_other_cjk_entries_alone() {
        let content = format!("{CJK_FIRST}## 10:00\n中间\n\n{CJK_LAST}");
        assert_eq!(removing(1, &content), format!("{CJK_FIRST}{CJK_LAST}"));
        assert_eq!(removing(0, &content), format!("## 10:00\n中间\n\n{CJK_LAST}"), "the last entry's U+2029 and missing newline stay");
        let last_removed = removing(2, &content);
        assert_eq!(last_removed, format!("{CJK_FIRST}## 10:00\n中间\n"));
        assert!(content.starts_with(&last_removed), "removing the last memo leaves a prefix of the file");
    }

    #[test]
    fn appending_leaves_every_entry_alone() {
        let content = format!("{CJK_FIRST}{CJK_LAST}");
        assert_eq!(appending("新的", "15:00", &content), format!("{content}\n\n## 15:00\n新的\n"), "U+2029 is text, not a blank line");
        assert_eq!(appending("x", "15:00", "\n\n## 09:00\na\n"), "\n\n## 09:00\na\n\n## 15:00\nx\n", "leading blank lines stay");
        assert_eq!(appending("x", "15:00", "## 09:00\na\n\n\n\n"), "## 09:00\na\n\n## 15:00\nx\n", "the gap is one blank line");
        assert_eq!(appending("x", "15:00", "\n\r\n\n"), "## 15:00\nx\n", "only line breaks counts as empty");
        assert_eq!(appending("x", "15:00", "a\r"), "a\r\n\n## 15:00\nx\n", "a lone CR doesn't swallow the heading");
        assert_eq!(texts_of("a\r\n\n## 15:00\nx\n"), ["x"]);
    }

    fn texts_of(content: &str) -> Vec<String> {
        parse(content, "d").into_iter().map(|m| m.text).collect()
    }

    #[test]
    fn crlf_inside_an_entry_survives_rewrites_of_others() {
        let first = "## 09:00\n第一行\r\n第二行\r\n\n";
        let last = "## 10:00\n第三行\r\n第四行\r\n";
        let content = format!("{first}{last}");
        let memos = parse(&content, "d");
        assert_eq!(memos.len(), 2);
        assert_eq!(memos[0].text, "第一行\r\n第二行", "CRLF inside a body is the body's");

        assert_eq!(replacing(1, "新", &content), format!("{first}## 10:00\n新"), "CRLF isn't a final \\n, as hasSuffix says");
        assert_eq!(replacing(0, "新", &content), format!("## 09:00\n新\n\n{last}"));
        assert_eq!(removing(0, &content), last);
        assert_eq!(removing(1, &content), "## 09:00\n第一行\r\n第二行\r\n");
        assert_eq!(appending("新", "11:00", &content), format!("{content}\n## 11:00\n新\n"), "the last CRLF stays and a bare LF makes the gap");
    }

    // Pins Swift: in a CRLF file, "## 09:00\r\n" is not a line of its own, so
    // there are no memos to edit, and nothing to damage. See the report.
    #[test]
    fn a_crlf_file_has_no_memos_and_keeps_its_bytes() {
        let content = "## 09:00\r\n第一条\r\n\r\n## 10:00\r\n第二条\r\n";
        assert!(parse(content, "d").is_empty());
        assert_eq!(replacing(0, "x", content), content);
        assert_eq!(removing(0, content), content);
        let appended = appending("新", "11:00", content);
        assert_eq!(appended, format!("{content}\n## 11:00\n新\n"), "every byte kept, new entry after");
        let memos = parse(&appended, "d");
        assert_eq!(memos.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["新"]);
        assert_eq!(removing(0, &appended), content, "and removing it gives the file back");
    }

    #[test]
    fn a_heading_ending_in_a_final_terminator_still_counts() {
        // ICU's `$` matches before a line break that ends the input.
        assert_eq!(parse("## 09:00\u{2028}\nx\n", "d")[0].time, "09:00");
        assert_eq!(parse("## 09:00\r\n\nx\n", "d")[0].text, "x");
        assert_eq!(parse("## 9:05 \t\nx", "d")[0].time, "9:05");
        assert_eq!(parse("##\t09:00\nx", "d")[0].time, "09:00");
        assert!(parse("## 09:00\u{2028}y\nx", "d").is_empty());
        assert!(parse("### 09:00\nx", "d").is_empty());
        assert!(parse(" ## 09:00\nx", "d").is_empty());
        assert!(parse("## 100:00\nx", "d").is_empty());
        assert_eq!(parse("## ０９:３１\nx", "d")[0].time, "０９:３１", "\\d takes any decimal digit");
    }

    #[test]
    fn index_counts_empty_memos() {
        let content = "## 10:00\n\n## 11:00\nhi\n";
        let memos = parse(content, "d");
        assert_eq!(memos[0].index, 1);
        assert_eq!(replacing(memos[0].index, "yo", content), "## 10:00\n\n## 11:00\nyo\n");
    }

    #[test]
    fn replacing_with_an_empty_text() {
        assert_eq!(replacing(0, "  \n ", "## 09:00\nfirst\n"), "## 09:00\n\n");
        assert_eq!(replacing(0, "x", "## 09:00"), "## 09:00\nx");
    }

    #[test]
    fn a_bom_is_kept_and_doesnt_hide_the_first_memo() {
        let content = "\u{FEFF}## 09:00\n一\n\n## 10:00\n二\n";
        assert_eq!(parse(content, "d").len(), 2);
        assert_eq!(replacing(1, "x", content), "\u{FEFF}## 09:00\n一\n\n## 10:00\nx\n");
        assert_eq!(removing(0, content), "\u{FEFF}## 10:00\n二\n");
        assert_eq!(removing(1, content), "\u{FEFF}## 09:00\n一\n");
        assert_eq!(appending("三", "11:00", content), format!("{content}\n## 11:00\n三\n"));
        assert_eq!(appending("三", "11:00", "\u{FEFF}"), "\u{FEFF}## 11:00\n三\n");
    }

    #[test]
    fn fences_hide_headings_and_a_combining_mark_is_no_fence() {
        assert_eq!(parse("~~~\n## 09:00\nx\n~~~\n## 10:00\ny", "d")[0].time, "10:00");
        assert_eq!(parse("```\u{301}\n## 09:00\nx", "d").len(), 1);
    }

    #[test]
    fn tag_ranges_are_bytes() {
        let text = "中文 #标签/子 emoji😀 #no😀 (#a_b-1) #1x\u{3000}#全角 \n#行首 [#方括号]";
        let found = tag_ranges(text);
        for (name, range) in &found {
            assert_eq!(&text[range.clone()], format!("#{name}"));
        }
        let names: Vec<_> = found.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["标签/子", "no", "a_b-1", "全角", "行首", "方括号"]);
        assert_eq!(tags("#a#b #c"), ["a", "c"]);
        assert_eq!(tags("#é #e\u{301}x"), ["é", "e"]);
    }

    #[test]
    fn day_names() {
        assert!(is_day("٢٠٢٦-١٠-١٦"), "\\d takes any decimal digit");
        assert!(is_day("2026-10-16\n"), "ICU's $ allows a final line break");
        assert!(!is_day("2026-10-16\nx"));
        assert!(!is_day("2026-1-16"));
    }

    #[test]
    fn date_formatter_rules() {
        let at = |day: &str| date_of_day(day, 0).map(|date| day_of(date, 0));
        assert_eq!(at("2026-02-30").as_deref(), Some("2026-03-02"), "rolls over as DateFormatter does");
        assert_eq!(at("2026-02-31").as_deref(), Some("2026-03-03"));
        assert_eq!(at("2026-04-31").as_deref(), Some("2026-05-01"));
        assert_eq!(at("2024-02-30").as_deref(), Some("2024-03-01"));
        assert_eq!(at("2026-02-32"), None);
        assert_eq!(at("2026-13-01"), None);
        assert_eq!(at("2026-00-10"), None);
        assert_eq!(at("2026-10-00"), None);
        assert_eq!(at("2026-1-5").as_deref(), Some("2026-01-05"));
        assert_eq!(at("2026-10-16x"), None);
        assert_eq!(date_of_day("2026-10-16", 0), Some(1792108800.0), "the epoch seconds Swift gives");
    }

    #[test]
    fn times_and_days_in_other_zones() {
        let date = 1792108800.0 + 3661.5; // 2026-10-16 01:01:01.5 UTC
        assert_eq!(day_of(date, 0), "2026-10-16");
        assert_eq!(time_of(date, 0), "01:01");
        assert_eq!(day_of(date, -2 * 3600), "2026-10-15");
        assert_eq!(time_of(date, -2 * 3600), "23:01");
        assert_eq!(time_of(date, 8 * 3600 + 1800), "09:31");
        assert_eq!(day_of(-1.0, 0), "1969-12-31");
        assert_eq!(time_of(-1.0, 0), "23:59");
    }

    #[test]
    fn heatmap_in_a_negative_offset_and_at_year_end() {
        let offset = -5 * 3600;
        let today = date_of_day("2026-12-31", offset).unwrap() + 3600.0;
        let grid = heatmap::grid(1, today, offset);
        assert_eq!(grid[0][3], date_of_day("2026-12-31", offset), "a Thursday, at the start of its day");
        assert_eq!(grid[0][4], None);
        let december = heatmap::month(today, offset);
        let last = december.last().unwrap().iter().flatten().last().unwrap();
        assert_eq!(day_of(*last, offset), "2026-12-31");
        assert!(heatmap::grid(0, today, offset).is_empty());
        assert_eq!(heatmap::level(-3), 0);
    }

    #[test]
    fn append_to_file_creates_and_keeps_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let date = 1792108800.0 + 3600.0;
        let path = append_to_file("第一条", date, 0, dir.path()).unwrap();
        assert_eq!(path, dir.path().join("Memos").join("2026-10-16.md"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "## 01:00\n第一条\n");
        append_to_file("第二条", date + 60.0, 0, dir.path()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "## 01:00\n第一条\n\n## 01:01\n第二条\n");

        fs::write(&path, "\u{FEFF}## 01:00\r\n旧\r\n").unwrap();
        append_to_file("新", date, 0, dir.path()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "\u{FEFF}## 01:00\r\n旧\r\n\n## 01:00\n新\n");
        let names: Vec<_> = fs::read_dir(dir.path().join("Memos")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "no temporary file left behind");
    }

    // Swift's `try? String(contentsOf:) ?? ""` would replace this file.
    #[test]
    fn append_to_file_refuses_a_file_it_cant_read() {
        let dir = tempfile::tempdir().unwrap();
        let date = 1792108800.0;
        let path = file_for_day("2026-10-16", dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = [b'#', b' ', 0xFF, 0xFE, b'\n'];
        fs::write(&path, bytes).unwrap();
        assert!(append_to_file("x", date, 0, dir.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
