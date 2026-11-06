//! What the Memos page works out without a window: layout math, grouping,
//! labels, and the calendar's hover card.

use fumi_core::memo_format::{self, Memo};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

pub const COLUMN_WIDTH: f32 = 460.;
pub const MARGIN_WIDTH: f32 = 200.;
pub const MARGIN_GAP: f32 = 48.;
pub const CELL: f32 = 28.;
pub const ROW_HEIGHT: f32 = 30.;
pub const CARD_WIDTH: f32 = 250.;
pub const CARD_MEMOS: usize = 5;
pub const TAGS_SHOWN: usize = 12;

/// The calendar and tags sit in a margin when there's room.
pub fn shows_margin(page_width: f32) -> bool {
    page_width >= COLUMN_WIDTH + MARGIN_GAP + MARGIN_WIDTH + 80.
}

fn content_width(margin: bool) -> f32 {
    if margin { COLUMN_WIDTH + MARGIN_GAP + MARGIN_WIDTH } else { COLUMN_WIDTH }
}

/// The column's left edge: what's shown is centered as one.
pub fn column_x(page_width: f32) -> f32 {
    (page_width - content_width(shows_margin(page_width))) / 2.
}

pub fn margin_x(page_width: f32) -> f32 {
    column_x(page_width) + COLUMN_WIDTH + MARGIN_GAP
}

// Filters and grouping.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub day: Option<String>,
    pub tag: Option<String>,
}

impl Filter {
    pub fn admits(&self, memo: &Memo) -> bool {
        self.day.as_ref().is_none_or(|day| &memo.day == day)
            && self.tag.as_ref().is_none_or(|tag| memo.tags().contains(tag))
    }

    pub fn is_active(&self) -> bool {
        self.day.is_some() || self.tag.is_some()
    }

    pub fn toggle_tag(&mut self, tag: &str) {
        self.tag = if self.tag.as_deref() == Some(tag) { None } else { Some(tag.to_string()) };
    }

    /// A day with memos, or the selected one, toggles; an empty one does nothing.
    pub fn click_day(&mut self, day: &str, count: usize) {
        let selected = self.day.as_deref() == Some(day);
        if count > 0 || selected {
            self.day = if selected { None } else { Some(day.to_string()) };
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayGroup {
    pub day: String,
    pub memos: Vec<Memo>,
}

/// Runs of the same day, keeping the memos' order.
pub fn grouped_by_day(memos: impl IntoIterator<Item = Memo>) -> Vec<DayGroup> {
    let mut groups: Vec<DayGroup> = Vec::new();
    for memo in memos {
        match groups.last_mut() {
            Some(last) if last.day == memo.day => last.memos.push(memo),
            _ => groups.push(DayGroup { day: memo.day.clone(), memos: vec![memo] }),
        }
    }
    groups
}

/// How many memos each day has.
pub fn counts_by_day(memos: &[Memo]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for memo in memos {
        *counts.entry(memo.day.clone()).or_insert(0) += 1;
    }
    counts
}

/// The most used tags: count descending, then name; once per memo.
pub fn top_tags(memos: &[Memo]) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for memo in memos {
        let tags: HashSet<String> = memo.tags().into_iter().collect();
        for tag in tags {
            *counts.entry(tag).or_insert(0) += 1;
        }
    }
    let mut tags: Vec<(String, usize)> = counts.into_iter().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    tags.truncate(TAGS_SHOWN);
    tags
}

/// The smallest splice taking `old` to `new`: the range replaced, and how many
/// take its place. `None` when they're equal.
pub fn splice_between<T: PartialEq>(old: &[T], new: &[T]) -> Option<(Range<usize>, usize)> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    if prefix == old.len() && prefix == new.len() {
        return None;
    }
    let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
    Some((prefix..old.len() - suffix, new.len() - suffix - prefix))
}

// Text helpers.

/// Trims as `whitespacesAndNewlines` does.
pub fn is_blank(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

/// The draft with a `#` added, after a space if it needs one.
pub fn with_tag_marker(draft: &str) -> String {
    let joined = draft.is_empty() || draft.ends_with(' ') || draft.ends_with('\n');
    format!("{draft}{}", if joined { "#" } else { " #" })
}

/// The first nonblank line, trimmed.
pub fn first_line(text: &str) -> &str {
    text.split(['\n', '\r']).map(|line| line.trim_matches([' ', '\t'])).find(|line| !line.is_empty()).unwrap_or("")
}

pub fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

/// Under the calendar.
pub fn month_footer(memos: usize, days: usize) -> String {
    if memos == 0 {
        "Nothing yet this month".to_string()
    } else {
        format!("{} · {}", plural(memos, "memo"), plural(days, "day"))
    }
}

// Dates, in English as the app's copy is.

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
    "November", "December",
];
const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// Monday = 0.
    pub weekday: usize,
}

/// A day file's name read as a date. As Swift's formatter reads it, "2026-02-30"
/// rolls over to March 2.
pub fn civil(day: &str) -> Option<Civil> {
    let secs = memo_format::date_of_day(day, 0)?;
    let normal = memo_format::day_of(secs, 0);
    let mut parts = normal.split('-');
    let (year, month, date) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    let days = (secs / 86_400.).floor() as i64;
    Some(Civil { year, month, day: date, weekday: (days + 3).rem_euclid(7) as usize })
}

pub fn month_name(month: u32) -> &'static str {
    MONTHS[(month as usize).clamp(1, 12) - 1]
}

fn short_month(month: u32) -> &'static str {
    &month_name(month)[..3]
}

/// "Friday, October 9", with the year when it isn't this one.
pub fn long_day_label(day: &str, this_year: i32) -> String {
    let Some(date) = civil(day) else { return day.to_string() };
    let label = format!("{}, {} {}", WEEKDAYS[date.weekday], month_name(date.month), date.day);
    if date.year == this_year { label } else { format!("{label}, {}", date.year) }
}

/// Under a day's number: `Fri`, `Fri · Sep`, or `Fri · Sep 25`.
pub fn day_subtitle(day: &str, this_year: i32, this_month: u32) -> String {
    let Some(date) = civil(day) else { return String::new() };
    let weekday = &WEEKDAYS[date.weekday][..3];
    if date.year != this_year {
        format!("{weekday} · {} {:02}", short_month(date.month), date.year.rem_euclid(100))
    } else if date.month != this_month {
        format!("{weekday} · {}", short_month(date.month))
    } else {
        weekday.to_string()
    }
}

/// The big number of a day group; the name itself if it isn't a date.
pub fn day_number(day: &str) -> String {
    civil(day).map_or_else(|| day.to_string(), |date| date.day.to_string())
}

pub fn shift_month(year: i32, month: u32, by: i32) -> (i32, u32) {
    let total = year * 12 + month as i32 - 1 + by;
    (total.div_euclid(12), total.rem_euclid(12) as u32 + 1)
}

/// The month as weeks of seven, Monday first: each cell its day file's name.
pub fn month_cells(year: i32, month: u32) -> Vec<Vec<Option<String>>> {
    let first = format!("{year:04}-{month:02}-01");
    let Some(secs) = memo_format::date_of_day(&first, 0) else { return Vec::new() };
    memo_format::heatmap::month(secs, 0)
        .into_iter()
        .map(|week| week.into_iter().map(|cell| cell.map(|date| memo_format::day_of(date, 0))).collect())
        .collect()
}

/// A day's accent wash by level, a step stronger on a dark page.
pub fn wash(level: u8, dark: bool) -> f32 {
    let levels = if dark { [0., 0.16, 0.26, 0.38, 0.52] } else { [0., 0.09, 0.16, 0.24, 0.32] };
    levels[(level as usize).min(4)]
}

/// The hover card's corner, from its day cell's corner: its right edge with the
/// calendar's, 2 under the row.
pub fn card_offset(column: usize) -> (f32, f32) {
    (CELL * 7. - CARD_WIDTH - column as f32 * CELL, ROW_HEIGHT + 2.)
}

// The calendar's hover card.

/// What the hover asks for next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoverEffect {
    Nothing,
    /// Drop any timer and hide the card without a fade.
    HideNow,
    /// Follow the pointer to this day at once.
    Follow,
    /// Show `day` after 350 ms.
    ShowLater(String),
    /// Hide after 80 ms.
    HideLater,
}

/// The card waits a moment before it shows, then follows the pointer from day to
/// day without waiting again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardHover {
    pub hovered: Option<String>,
    pub card: Option<String>,
}

impl CardHover {
    pub fn hover(&mut self, day: &str, count: usize, inside: bool) -> HoverEffect {
        if inside {
            self.hovered = Some(day.to_string());
            if count == 0 {
                self.card = None;
                return HoverEffect::HideNow;
            }
            if self.card.is_some() {
                self.card = Some(day.to_string());
                return HoverEffect::Follow;
            }
            return HoverEffect::ShowLater(day.to_string());
        }
        if self.hovered.as_deref() == Some(day) {
            self.hovered = None;
            return HoverEffect::HideLater;
        }
        HoverEffect::Nothing
    }

    /// The 350 ms ran out. Whether the card now shows.
    pub fn show_due(&mut self, day: &str) -> bool {
        if self.hovered.as_deref() == Some(day) {
            self.card = Some(day.to_string());
            return true;
        }
        false
    }

    /// The 80 ms ran out. The card to fade away, if any.
    pub fn hide_due(&mut self) -> Option<String> {
        if self.hovered.is_none() { self.card.take() } else { None }
    }

    pub fn clicked(&mut self) {
        self.card = None;
    }
}

/// Where the capture panel goes, in AppKit's bottom-left coordinates: centered
/// on the area, its top a fifth of the way down.
pub fn panel_origin(area: (f64, f64, f64, f64), panel: (f64, f64)) -> (f64, f64) {
    let (x, y, w, h) = area;
    (x + w / 2. - panel.0 / 2., y + h - h * 0.2 - panel.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memo(day: &str, index: usize, text: &str) -> Memo {
        Memo { day: day.into(), index, time: "09:00".into(), text: text.into() }
    }

    #[test]
    fn the_margin_needs_788() {
        assert!(!shows_margin(787.));
        assert!(shows_margin(788.));
        assert_eq!(column_x(1000.), (1000. - 708.) / 2.);
        assert_eq!(margin_x(1000.), 146. + 508.);
        assert_eq!(column_x(600.), 70.);
    }

    #[test]
    fn filters_combine() {
        let m = memo("2026-10-09", 0, "hi #a");
        let mut f = Filter::default();
        assert!(f.admits(&m) && !f.is_active());
        f.tag = Some("a".into());
        assert!(f.admits(&m));
        f.day = Some("2026-10-08".into());
        assert!(!f.admits(&m));
    }

    #[test]
    fn clicking_a_day_toggles_only_with_memos() {
        let mut f = Filter::default();
        f.click_day("d", 0);
        assert_eq!(f.day, None);
        f.click_day("d", 2);
        assert_eq!(f.day.as_deref(), Some("d"));
        f.click_day("d", 2);
        assert_eq!(f.day, None);
        // A selected day left empty can still be cleared.
        f.day = Some("d".into());
        f.click_day("d", 0);
        assert_eq!(f.day, None);
    }

    #[test]
    fn groups_keep_runs_of_a_day() {
        let groups = grouped_by_day([memo("b", 1, "x"), memo("b", 0, "y"), memo("a", 0, "z")]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].memos.len(), 2);
        assert_eq!(groups[1].day, "a");
    }

    #[test]
    fn tags_rank_by_count_then_name_once_per_memo() {
        let memos = [memo("a", 0, "#x #x #b"), memo("a", 1, "#x #c"), memo("a", 2, "#b #d")];
        let tags = top_tags(&memos);
        assert_eq!(tags[0], ("b".to_string(), 2));
        assert_eq!(tags[1], ("x".to_string(), 2));
        assert_eq!(tags[2], ("c".to_string(), 1));
    }

    #[test]
    fn only_twelve_tags_show() {
        let text: String = (0..20).map(|n| format!("#t{} ", (b'a' + n) as char)).collect();
        assert_eq!(top_tags(&[memo("a", 0, &text)]).len(), 12);
    }

    #[test]
    fn splices_are_minimal() {
        assert_eq!(splice_between(&[1, 2, 3], &[1, 2, 3]), None);
        assert_eq!(splice_between(&[1, 2, 3], &[1, 9, 3]), Some((1..2, 1)));
        assert_eq!(splice_between(&[1, 2, 3], &[0, 1, 2, 3]), Some((0..0, 1)));
        assert_eq!(splice_between(&[1, 2, 3], &[1, 3]), Some((1..2, 0)));
        assert_eq!(splice_between::<i32>(&[], &[1]), Some((0..0, 1)));
        assert_eq!(splice_between(&[1, 1], &[1]), Some((1..2, 0)));
    }

    #[test]
    fn the_tag_button_adds_a_space_only_when_needed() {
        assert_eq!(with_tag_marker(""), "#");
        assert_eq!(with_tag_marker("a "), "a #");
        assert_eq!(with_tag_marker("a\n"), "a\n#");
        assert_eq!(with_tag_marker("a"), "a #");
    }

    #[test]
    fn labels() {
        assert_eq!(first_line("\n  \n  hello \nworld"), "hello");
        assert_eq!(month_footer(0, 0), "Nothing yet this month");
        assert_eq!(month_footer(1, 1), "1 memo · 1 day");
        assert_eq!(month_footer(5, 2), "5 memos · 2 days");
        assert!(is_blank(" \n\t") && !is_blank(" a"));
    }

    #[test]
    fn dates_read_as_swift_formats_them() {
        // October 9, 2026 is a Friday.
        assert_eq!(long_day_label("2026-10-09", 2026), "Friday, October 9");
        assert_eq!(long_day_label("2026-10-09", 2025), "Friday, October 9, 2026");
        assert_eq!(long_day_label("nope", 2026), "nope");
        assert_eq!(day_subtitle("2026-10-09", 2026, 10), "Fri");
        assert_eq!(day_subtitle("2026-09-25", 2026, 10), "Fri · Sep");
        assert_eq!(day_subtitle("2025-09-25", 2026, 10), "Thu · Sep 25");
        assert_eq!(day_number("2026-10-09"), "9");
        assert_eq!(day_number("nope"), "nope");
        assert_eq!(civil("2026-02-30").map(|c| (c.month, c.day)), Some((3, 2)));
    }

    #[test]
    fn months_step_over_years() {
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 6, 1), (2026, 7));
    }

    #[test]
    fn a_month_starts_on_monday() {
        // October 2026 starts on a Thursday and has 31 days.
        let weeks = month_cells(2026, 10);
        assert_eq!(weeks[0][..3], [None, None, None]);
        assert_eq!(weeks[0][3].as_deref(), Some("2026-10-01"));
        assert_eq!(weeks.iter().flatten().flatten().count(), 31);
        assert!(weeks.iter().all(|week| week.len() == 7));
    }

    #[test]
    fn washes_step_by_level_and_mode() {
        assert_eq!(wash(0, false), 0.);
        assert_eq!(wash(1, false), 0.09);
        assert_eq!(wash(4, false), 0.32);
        assert_eq!(wash(4, true), 0.52);
    }

    #[test]
    fn the_card_hangs_from_the_calendars_right_edge() {
        assert_eq!(card_offset(0), (-54., 32.));
        // Its right edge, measured from the calendar's left: the same for any day.
        for column in 0..7 {
            assert_eq!(column as f32 * CELL + card_offset(column).0 + CARD_WIDTH, 196.);
        }
    }

    #[test]
    fn the_card_waits_once_then_follows() {
        let mut h = CardHover::default();
        assert_eq!(h.hover("a", 2, true), HoverEffect::ShowLater("a".into()));
        assert!(h.show_due("a"));
        assert_eq!(h.card.as_deref(), Some("a"));
        // Either order of leaving a day and entering the next.
        assert_eq!(h.hover("a", 2, false), HoverEffect::HideLater);
        assert_eq!(h.hover("b", 1, true), HoverEffect::Follow);
        assert_eq!(h.card.as_deref(), Some("b"));
        assert_eq!(h.hover("c", 1, true), HoverEffect::Follow);
        assert_eq!(h.hover("b", 1, false), HoverEffect::Nothing);
        assert_eq!(h.card.as_deref(), Some("c"));
    }

    #[test]
    fn the_card_hides_after_leaving_and_not_on_empty_days_later() {
        let mut h = CardHover::default();
        h.hover("a", 2, true);
        h.show_due("a");
        h.hover("a", 2, false);
        assert_eq!(h.hide_due().as_deref(), Some("a"));
        assert_eq!(h.card, None);
        // An empty day hides it at once, and a stale timer does nothing.
        h.hover("a", 2, true);
        h.show_due("a");
        assert_eq!(h.hover("b", 0, true), HoverEffect::HideNow);
        assert_eq!(h.card, None);
        assert!(!h.show_due("a"));
        assert_eq!(h.hide_due(), None);
    }

    #[test]
    fn the_panel_sits_a_fifth_down() {
        // A 1000 x 800 area at (0, 40): top edge at 840, card frame 160 below it.
        assert_eq!(panel_origin((0., 40., 1000., 800.), (600., 420.)), (200., 260.));
    }
}
