//! Words the toolbar shows, with no window in sight: dates, counts, titles and
//! tips, each a pure function of what's passed in.

use fumi_core::note::{FileKind, LibraryFile, Note};
use fumi_core::tabs::Content;
use unicode_segmentation::UnicodeSegmentation;

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

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

/// A moment as local calendar fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Local {
    year: i64,
    month: i64,
    day: i64,
    /// Seconds into the local day.
    second: i64,
}

fn local(secs: f64, utc_offset: i32) -> Local {
    let at = secs.floor() as i64 + i64::from(utc_offset);
    let (year, month, day) = civil_from_days(at.div_euclid(86_400));
    Local { year, month, day, second: at.rem_euclid(86_400) }
}

/// What a calendar is asked for, per moment: its local offset. Keeps these
/// functions free of the system clock, and right across a DST change.
#[derive(Clone, Copy, Debug)]
pub struct Moment {
    pub secs: f64,
    pub utc_offset: i32,
}

/// "Oct 8", or "Oct 8, 2025" outside `now`'s year.
fn short_day(date: Moment, now: Moment) -> String {
    let d = local(date.secs, date.utc_offset);
    let name = MONTHS[(d.month - 1) as usize];
    if d.year == local(now.secs, now.utc_offset).year {
        format!("{name} {}", d.day)
    } else {
        format!("{name} {}, {}", d.day, d.year)
    }
}

/// "Edited 14:20" today, "Edited yesterday 09:12", "Edited Oct 8" this year, with
/// the year otherwise. `time` is the moment as the system writes a short time.
pub fn edited_label(date: Moment, now: Moment, time: &str) -> String {
    let d = local(date.secs, date.utc_offset);
    let n = local(now.secs, now.utc_offset);
    let day = days_from_civil(d.year, d.month, d.day);
    let today = days_from_civil(n.year, n.month, n.day);
    if day == today {
        format!("Edited {time}")
    } else if day == today - 1 {
        format!("Edited yesterday {time}")
    } else {
        format!("Edited {}", short_day(date, now))
    }
}

/// "Oct 8, 2026".
pub fn created_label(date: Moment) -> String {
    let d = local(date.secs, date.utc_offset);
    format!("{} {}, {}", MONTHS[(d.month - 1) as usize], d.day, d.year)
}

fn plural(n: i64, unit: &str, past: bool) -> String {
    let s = if n == 1 { "" } else { "s" };
    if past { format!("{n} {unit}{s} ago") } else { format!("in {n} {unit}{s}") }
}

/// As Foundation's `.relative(presentation: .named)`: the largest calendar unit
/// that isn't zero, the single steps by name.
pub fn relative_named(date: Moment, now: Moment) -> String {
    let past = date.secs <= now.secs;
    let (a, b) = if past { (date, now) } else { (now, date) };
    let (la, lb) = (local(a.secs, a.utc_offset), local(b.secs, b.utc_offset));
    let mut months = (lb.year - la.year) * 12 + lb.month - la.month;
    if (lb.day, lb.second) < (la.day, la.second) {
        months -= 1;
    }
    let (years, rest) = (months.div_euclid(12), months.rem_euclid(12));
    if years > 0 {
        return if years == 1 {
            (if past { "last year" } else { "next year" }).to_string()
        } else {
            plural(years, "year", past)
        };
    }
    if rest > 0 {
        return if rest == 1 {
            (if past { "last month" } else { "next month" }).to_string()
        } else {
            plural(rest, "month", past)
        };
    }
    // Under a month: plain elapsed local time.
    let elapsed = (days_from_civil(lb.year, lb.month, lb.day) - days_from_civil(la.year, la.month, la.day)) * 86_400
        + lb.second
        - la.second;
    let days = elapsed.div_euclid(86_400);
    let weeks = days / 7;
    if weeks > 0 {
        return if weeks == 1 {
            (if past { "last week" } else { "next week" }).to_string()
        } else {
            plural(weeks, "week", past)
        };
    }
    if days > 0 {
        return if days == 1 {
            (if past { "yesterday" } else { "tomorrow" }).to_string()
        } else {
            plural(days, "day", past)
        };
    }
    let hours = elapsed / 3600;
    if hours > 0 {
        return plural(hours, "hour", past);
    }
    let minutes = elapsed / 60;
    if minutes > 0 {
        return plural(minutes, "minute", past);
    }
    if elapsed > 0 {
        return plural(elapsed, "second", past);
    }
    "now".to_string()
}

/// Whitespace-separated tokens.
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Graphemes of the raw text, as Swift's `String.count`.
pub fn char_count(text: &str) -> usize {
    text.graphemes(true).count()
}

/// "3 min", at least 1.
pub fn reading_time(words: usize) -> String {
    format!("{} min", (words / 200).max(1))
}

/// A relative path's folders, "Clippings / AI".
pub fn folder_label(relative_path: &str) -> String {
    relative_path.split('/').filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" / ")
}

/// "Clippings / AI · Edited Oct 9", or just the edited label at the library root.
pub fn note_subtitle(relative_path: &str, edited: &str) -> String {
    let folder = folder_label(relative_path);
    if folder.is_empty() { edited.to_string() } else { format!("{folder} · {edited}") }
}

/// "attachments · 1.2 MB".
pub fn file_subtitle(relative_path: &str, size: Option<&str>) -> String {
    let folder = folder_label(relative_path);
    [folder.as_str(), size.unwrap_or("")].iter().filter(|part| !part.is_empty()).copied().collect::<Vec<_>>().join(" · ")
}

/// A path's last part.
fn last_part(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A tab's title: the file name without extension for notes, with it for files.
pub fn tab_title(content: &Content, note: Option<&Note>) -> String {
    match content {
        Content::Draft => "Untitled".to_string(),
        Content::Memos => "Memos".to_string(),
        Content::Note(id) => note.map(Note::filename).unwrap_or_else(|| {
            let name = last_part(id);
            match name.rfind('.') {
                Some(dot) if dot > 0 => name[..dot].to_string(),
                _ => name.to_string(),
            }
        }),
        Content::File(id) => last_part(id).to_string(),
    }
}

/// The hover tip: name, then folder and edited label for notes; name and edited
/// label for files; the title otherwise. `edited` is the label for that tab's date.
pub fn tab_help(content: &Content, note: Option<&Note>, file: Option<&LibraryFile>, edited: &str) -> String {
    if let Some(note) = note {
        return format!("{}\n{}", note.filename(), note_subtitle(&note.relative_path, edited));
    }
    if let Some(file) = file {
        return format!("{}\n{edited}", file.name());
    }
    tab_title(content, None)
}

/// The glyph a tab shows when pinned.
pub fn tab_glyph(content: &Content, kind: Option<FileKind>) -> fumi_ui::icons::Glyph {
    use fumi_ui::icons::Glyph;
    match content {
        Content::Draft | Content::Note(_) => Glyph::Note,
        Content::Memos => Glyph::Memo,
        Content::File(_) => match kind {
            Some(FileKind::Image) => Glyph::Image,
            Some(FileKind::Video) => Glyph::Video,
            _ => Glyph::File,
        },
    }
}

/// The preview's name for the extensions that are on.
pub fn preview_name(wechat: bool, xhs: bool) -> &'static str {
    match (wechat, xhs) {
        (true, false) => "WeChat Preview",
        (false, true) => "Xiaohongshu Preview",
        _ => "Publish Preview",
    }
}

/// The Preview button's tip.
pub fn preview_tip(wechat: bool, xhs: bool, open: bool) -> String {
    let name = preview_name(wechat, xhs);
    if open { format!("Hide {name} (⌃⌘P)") } else { format!("{name} (⌃⌘P)") }
}

/// "Documents › My Notes", "iCloud Drive › My Notes", "T7 › Notes › My Notes": the
/// last three parts of where a missing folder was.
pub fn breadcrumb(path: &str, home: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let home: Vec<&str> = home.split('/').filter(|part| !part.is_empty()).collect();
    let rest: Vec<String> = if !home.is_empty() && parts.starts_with(&home) {
        let rest = &parts[home.len()..];
        let icloud = ["Library", "Mobile Documents", "com~apple~CloudDocs"];
        if rest.starts_with(&icloud) {
            std::iter::once("iCloud Drive").chain(rest[3..].iter().copied()).map(str::to_string).collect()
        } else {
            rest.iter().map(|part| part.to_string()).collect()
        }
    } else if parts.len() > 1 && parts[0] == "Volumes" {
        parts[1..].iter().map(|part| part.to_string()).collect()
    } else {
        parts.iter().map(|part| part.to_string()).collect()
    };
    let start = rest.len().saturating_sub(3);
    rest[start..].join(" › ")
}

/// The disk's name when `path` lives on a volume that isn't mounted.
pub fn unmounted_volume(path: &str, exists: impl Fn(&str) -> bool) -> Option<String> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() > 1 && parts[0] == "Volumes" && !exists(&format!("/Volumes/{}", parts[1])) {
        Some(parts[1].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UTC: i32 = 0;

    fn at(year: i64, month: i64, day: i64, hour: i64, minute: i64) -> Moment {
        let secs = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60;
        Moment { secs: secs as f64, utc_offset: UTC }
    }

    #[test]
    fn edited_labels() {
        let now = at(2026, 11, 5, 16, 0);
        assert_eq!(edited_label(at(2026, 11, 5, 14, 20), now, "14:20"), "Edited 14:20");
        assert_eq!(edited_label(at(2026, 11, 4, 9, 12), now, "09:12"), "Edited yesterday 09:12");
        assert_eq!(edited_label(at(2026, 10, 8, 9, 12), now, "x"), "Edited Oct 8");
        assert_eq!(edited_label(at(2025, 10, 8, 9, 12), now, "x"), "Edited Oct 8, 2025");
        // The new year's first day: yesterday is last year's.
        let january = at(2027, 1, 1, 8, 0);
        assert_eq!(edited_label(at(2026, 12, 31, 23, 0), january, "23:00"), "Edited yesterday 23:00");
    }

    #[test]
    fn local_time_decides_the_day() {
        // 23:30 UTC on the 4th is the 5th at UTC+8.
        let date = Moment { utc_offset: 8 * 3600, ..at(2026, 11, 4, 23, 30) };
        let now = Moment { utc_offset: 8 * 3600, ..at(2026, 11, 5, 2, 0) };
        assert_eq!(edited_label(date, now, "07:30"), "Edited 07:30");
    }

    #[test]
    fn created_labels() {
        assert_eq!(created_label(at(2026, 10, 8, 1, 0)), "Oct 8, 2026");
    }

    #[test]
    fn relative_labels() {
        let now = at(2026, 11, 5, 16, 0);
        let check = |date: Moment, want: &str| assert_eq!(relative_named(date, now), want);
        check(now, "now");
        check(Moment { secs: now.secs - 5., ..now }, "5 seconds ago");
        check(at(2026, 11, 5, 15, 59), "1 minute ago");
        check(at(2026, 11, 5, 13, 0), "3 hours ago");
        check(at(2026, 11, 4, 17, 0), "23 hours ago");
        check(at(2026, 11, 4, 16, 0), "yesterday");
        check(at(2026, 11, 2, 16, 0), "3 days ago");
        check(at(2026, 10, 29, 16, 0), "last week");
        check(at(2026, 10, 20, 16, 0), "2 weeks ago");
        check(at(2026, 10, 5, 16, 0), "last month");
        check(at(2026, 8, 5, 16, 0), "3 months ago");
        check(at(2025, 11, 5, 16, 0), "last year");
        check(at(2023, 11, 5, 16, 0), "3 years ago");
        check(at(2026, 11, 6, 16, 0), "tomorrow");
        check(at(2026, 11, 5, 18, 0), "in 2 hours");
    }

    #[test]
    fn counts() {
        assert_eq!(word_count("  one two\n\nthree\t"), 3);
        assert_eq!(word_count("  \n"), 0);
        assert_eq!(char_count("a👍🏽é"), 3);
        assert_eq!(reading_time(0), "1 min");
        assert_eq!(reading_time(399), "1 min");
        assert_eq!(reading_time(400), "2 min");
    }

    #[test]
    fn subtitles() {
        assert_eq!(note_subtitle("Clippings/AI", "Edited Oct 9"), "Clippings / AI · Edited Oct 9");
        assert_eq!(note_subtitle("", "Edited Oct 9"), "Edited Oct 9");
        assert_eq!(file_subtitle("attachments", Some("1.2 MB")), "attachments · 1.2 MB");
        assert_eq!(file_subtitle("", Some("1.2 MB")), "1.2 MB");
        assert_eq!(file_subtitle("a", None), "a");
    }

    #[test]
    fn titles() {
        assert_eq!(tab_title(&Content::Draft, None), "Untitled");
        assert_eq!(tab_title(&Content::Memos, None), "Memos");
        assert_eq!(tab_title(&Content::Note("/lib/a.b.md".into()), None), "a.b");
        assert_eq!(tab_title(&Content::File("/lib/cover.jpg".into()), None), "cover.jpg");
        assert_eq!(tab_help(&Content::Memos, None, None, ""), "Memos");
    }

    #[test]
    fn preview_words() {
        assert_eq!(preview_tip(true, false, false), "WeChat Preview (⌃⌘P)");
        assert_eq!(preview_tip(false, true, true), "Hide Xiaohongshu Preview (⌃⌘P)");
        assert_eq!(preview_tip(true, true, false), "Publish Preview (⌃⌘P)");
    }

    #[test]
    fn breadcrumbs() {
        let home = "/Users/me";
        assert_eq!(breadcrumb("/Users/me/Documents/My Notes", home), "Documents › My Notes");
        assert_eq!(
            breadcrumb("/Users/me/Library/Mobile Documents/com~apple~CloudDocs/My Notes", home),
            "iCloud Drive › My Notes"
        );
        assert_eq!(breadcrumb("/Volumes/T7/Notes/My Notes", home), "T7 › Notes › My Notes");
        assert_eq!(breadcrumb("/Users/me/a/b/c/d", home), "b › c › d");
        assert_eq!(breadcrumb("/opt/notes", home), "opt › notes");
    }

    #[test]
    fn volumes() {
        assert_eq!(unmounted_volume("/Volumes/T7/Notes", |_| false), Some("T7".to_string()));
        assert_eq!(unmounted_volume("/Volumes/T7/Notes", |_| true), None);
        assert_eq!(unmounted_volume("/Users/me/Notes", |_| false), None);
    }
}
