//! The Xiaohongshu Cards extension: a note laid out as image cards by the bundled
//! `cards.js`, exported as 1080-pixel-wide PNGs. What's here has no UI: each
//! note's choices, the input the layout takes, and the names and lists around it.
//! A port of the logic half of `XhsCards`.

use fumi_core::xhs_deck::{XhsDeck, asset_url};
use fumi_ui::settings::{self, keys};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use unicode_segmentation::UnicodeSegmentation;

pub const CARDS_HTML: &str = include_str!("../resources/Xiaohongshu/cards.html");
pub const CARDS_CSS: &str = include_str!("../resources/Xiaohongshu/cards.css");
pub const CARDS_JS: &str = include_str!("../resources/Xiaohongshu/cards.js");

/// Exported cards are this many pixels wide.
pub const EXPORT_WIDTH: f64 = 1080.;
pub const TITLE_LIMIT: usize = 20;
pub const CAPTION_LIMIT: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    Memo,
    Grid,
    Post,
    PostDark,
}

impl Template {
    pub const ALL: [Template; 4] = [Template::Memo, Template::Grid, Template::Post, Template::PostDark];

    pub fn raw(self) -> &'static str {
        match self {
            Template::Memo => "memo",
            Template::Grid => "grid",
            Template::Post => "post",
            Template::PostDark => "postDark",
        }
    }

    pub fn from_raw(raw: &str) -> Option<Template> {
        Template::ALL.into_iter().find(|t| t.raw() == raw)
    }

    pub fn name(self) -> &'static str {
        match self {
            Template::Memo => "Memo",
            Template::Grid => "Grid",
            Template::Post => "Post",
            Template::PostDark => "Post Dark",
        }
    }

    /// The paper in the bar's glyph.
    pub fn paper(self) -> u32 {
        match self {
            Template::Memo | Template::Post => 0xFFFFFF,
            Template::Grid => 0xFBFBF8,
            Template::PostDark => 0x1C1C1E,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ratio {
    Portrait,
    Square,
}

impl Ratio {
    pub const ALL: [Ratio; 2] = [Ratio::Portrait, Ratio::Square];

    pub fn raw(self) -> &'static str {
        match self {
            Ratio::Portrait => "3:4",
            Ratio::Square => "1:1",
        }
    }

    pub fn from_raw(raw: &str) -> Option<Ratio> {
        Ratio::ALL.into_iter().find(|r| r.raw() == raw)
    }

    pub fn menu_title(self) -> &'static str {
        match self {
            Ratio::Portrait => "3:4 Portrait",
            Ratio::Square => "1:1 Square",
        }
    }
}

// Each note's own choices -------------------------------------------------------

/// What the preview changed for one note: its template and size, the post's title
/// and caption when edited, and the blocks left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteChoices {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default)]
    pub left_out: Vec<String>,
}

fn choices_key(note_id: &str) -> String {
    format!("fumi.xhs.note.{note_id}")
}

pub fn choices(note_id: &str) -> NoteChoices {
    settings::get_data(&choices_key(note_id)).and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
}

pub fn set_choices(choices: &NoteChoices, note_id: &str) {
    if *choices == NoteChoices::default() {
        settings::remove(&choices_key(note_id));
    } else if let Ok(data) = serde_json::to_vec(choices) {
        settings::set_data(&choices_key(note_id), &data);
    }
}

pub fn default_template() -> Template {
    Template::from_raw(&settings::get_string(keys::XHS_TEMPLATE).unwrap_or_default()).unwrap_or(Template::Memo)
}

pub fn default_ratio() -> Ratio {
    Ratio::from_raw(&settings::get_string(keys::XHS_RATIO).unwrap_or_default()).unwrap_or(Ratio::Portrait)
}

pub fn template_of(choices: &NoteChoices) -> Template {
    choices.template.as_deref().and_then(Template::from_raw).unwrap_or_else(default_template)
}

pub fn ratio_of(choices: &NoteChoices) -> Ratio {
    choices.ratio.as_deref().and_then(Ratio::from_raw).unwrap_or_else(default_ratio)
}

/// Choosing stays with this note; choosing the Settings default clears the override.
pub fn choose_template(choices: &mut NoteChoices, option: Template, default: Template) {
    choices.template = (option != default).then(|| option.raw().to_string());
}

pub fn choose_ratio(choices: &mut NoteChoices, option: Ratio, default: Ratio) {
    choices.ratio = (option != default).then(|| option.raw().to_string());
}

pub fn leave_out(choices: &mut NoteChoices, ids: &[String]) {
    let mut all: Vec<String> = choices.left_out.iter().chain(ids).cloned().collect::<HashSet<_>>().into_iter().collect();
    all.sort();
    choices.left_out = all;
}

// Layout input -----------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Author {
    pub name: String,
    pub handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EndCard {
    pub on: bool,
    pub line: String,
}

/// What cards.js lays out.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    pub deck: XhsDeck,
    pub template: String,
    pub ratio: String,
    pub page_numbers: bool,
    pub author: Author,
    pub end_card: EndCard,
    pub accent: String,
    pub mode: String,
}

impl Input {
    pub fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

pub fn handle_of(raw: &str) -> String {
    let handle = crate::wechat::trim(raw);
    if !handle.is_empty() && !handle.starts_with('@') { format!("@{handle}") } else { handle.to_string() }
}

fn full_user_name() -> String {
    use cocoa::base::{id, nil};
    use objc::{msg_send, sel, sel_impl};
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {
        fn NSFullUserName() -> id;
    }
    unsafe {
        let name = NSFullUserName();
        if name == nil {
            return String::new();
        }
        let utf8: *const std::os::raw::c_char = msg_send![name, UTF8String];
        if utf8.is_null() { String::new() } else { std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned() }
    }
}

pub fn author() -> Author {
    let name = settings::get_string(keys::XHS_NAME).unwrap_or_default();
    let name = crate::wechat::trim(&name);
    let avatar = settings::get_string(keys::XHS_AVATAR).filter(|p| Path::new(p).exists()).map(|p| asset_url(&p));
    Author {
        name: if name.is_empty() { full_user_name() } else { name.to_string() },
        handle: handle_of(&settings::get_string(keys::XHS_HANDLE).unwrap_or_default()),
        avatar,
    }
}

pub fn deck_for(markdown: &str, note_path: &str, choices: &NoteChoices) -> XhsDeck {
    let file_name = Path::new(note_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    XhsDeck::make(markdown, note_path, &file_name, &choices.left_out.iter().cloned().collect())
}

pub fn input_for(deck: &XhsDeck, choices: &NoteChoices) -> Input {
    Input {
        deck: deck.clone(),
        template: template_of(choices).raw().to_string(),
        ratio: ratio_of(choices).raw().to_string(),
        page_numbers: settings::get_bool(keys::XHS_PAGE_NUMBERS, true),
        author: author(),
        end_card: EndCard {
            on: settings::get_bool(keys::XHS_END_CARD, false),
            line: settings::get_string(keys::XHS_END_LINE).unwrap_or_default(),
        },
        accent: settings::accent_hex(),
        mode: "preview".to_string(),
    }
}

/// Read so that a change in Settings lays the cards out again.
pub fn settings_signature() -> String {
    let get = |key| settings::get_string(key).unwrap_or_default();
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        get(keys::XHS_TEMPLATE),
        get(keys::XHS_RATIO),
        settings::get_bool(keys::XHS_PAGE_NUMBERS, true),
        get(keys::XHS_NAME),
        get(keys::XHS_HANDLE),
        get(keys::XHS_AVATAR),
        settings::get_bool(keys::XHS_END_CARD, false),
        get(keys::XHS_END_LINE),
        settings::accent_hex(),
    )
}

/// The post's title: the one edited in the preview, else the note's own.
pub fn title_of(deck: &XhsDeck, choices: &NoteChoices) -> String {
    choices.title.clone().unwrap_or_else(|| deck.title.clone())
}

pub fn caption_of(deck: &XhsDeck, choices: &NoteChoices) -> String {
    choices.caption.clone().unwrap_or_else(|| deck.caption.clone())
}

/// Characters, as the preview counts them for the title and caption.
pub fn length(text: &str) -> usize {
    text.graphemes(true).count()
}

/// `20 / 20` only when over; otherwise the bare count.
pub fn count_label(count: usize, limit: usize) -> String {
    if count > limit { format!("{count} / {limit}") } else { count.to_string() }
}

pub fn count_help(count: usize, limit: usize) -> String {
    if count > limit { format!("Xiaohongshu takes {limit} characters at most") } else { "Characters".to_string() }
}

/// `01.png`, `02.png`, …
pub fn card_file_name(index: usize) -> String {
    format!("{:02}.png", index + 1)
}

pub fn safe_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if "/:\\?%*|\"<>".contains(c) { ' ' } else { c }).collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() { "Cards".to_string() } else { cleaned.graphemes(true).take(80).collect() }
}

/// Where Save Images to Folder starts: `attachments/xhs`, else `attachments`, else
/// the library.
pub fn save_start(library: &Path, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    [library.join("attachments/xhs"), library.join("attachments"), library.to_path_buf()].into_iter().find(|p| exists(p))
}

pub fn left_out_label(count: usize) -> String {
    if count == 1 { "1 part left out of the cards.".to_string() } else { format!("{count} parts left out of the cards.") }
}

/// `Memo · 3:4`.
pub fn template_label(template: Template, ratio: Ratio) -> String {
    format!("{} · {}", template.name(), ratio.raw())
}

/// Serves the card page's own files and image files on this Mac: `fumi-xhs://app/…`
/// and `fumi-xhs://local/<path>`. The MIME type and what to send, or None.
pub enum Asset {
    Embedded(&'static str, &'static str),
    File(PathBuf, &'static str),
}

pub fn asset_for(host: &str, path: &str) -> Option<Asset> {
    let last = path.rsplit('/').next().unwrap_or(path);
    match host {
        "app" => match last {
            "cards.html" => Some(Asset::Embedded(CARDS_HTML, "text/html")),
            "cards.css" => Some(Asset::Embedded(CARDS_CSS, "text/css")),
            "cards.js" => Some(Asset::Embedded(CARDS_JS, "text/javascript")),
            _ => None,
        },
        "local" => {
            let extension = Path::new(path).extension()?.to_string_lossy().to_lowercase();
            let mime = match extension.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "heic" => "image/heic",
                "tif" | "tiff" => "image/tiff",
                "bmp" => "image/bmp",
                "svg" => "image/svg+xml",
                _ => return None,
            };
            Some(Asset::File(PathBuf::from(path), mime))
        }
        _ => None,
    }
}

// The one layout at a time -----------------------------------------------------

/// One layout at a time; the newest input waits for the one running. A new width
/// lays the shown input out again.
#[derive(Default)]
pub struct LayoutQueue<T> {
    loaded: bool,
    laying_out: bool,
    shown: Option<T>,
    pending: Option<T>,
}

impl<T: Clone + PartialEq> LayoutQueue<T> {
    pub fn new() -> Self {
        LayoutQueue { loaded: false, laying_out: false, shown: None, pending: None }
    }

    pub fn show(&mut self, input: T) {
        if self.shown.as_ref() != Some(&input) || self.pending.is_some() {
            self.pending = Some(input);
        }
    }

    pub fn relayout(&mut self) {
        if self.pending.is_none() {
            self.pending = self.shown.clone();
        }
    }

    pub fn loaded(&mut self) {
        self.loaded = true;
    }

    /// The next input to lay out now, if one is due.
    pub fn next(&mut self) -> Option<T> {
        if !self.loaded || self.laying_out {
            return None;
        }
        let input = self.pending.take()?;
        self.shown = Some(input.clone());
        self.laying_out = true;
        Some(input)
    }

    pub fn finished(&mut self) {
        self.laying_out = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_names() {
        assert_eq!(safe_name("a/b:c?d"), "a b c d");
        assert_eq!(safe_name("  /:  "), "Cards");
        assert_eq!(safe_name(""), "Cards");
        assert_eq!(safe_name(&"字".repeat(100)).chars().count(), 80);
    }

    #[test]
    fn counts() {
        assert_eq!(count_label(20, 20), "20");
        assert_eq!(count_label(21, 20), "21 / 20");
        assert_eq!(count_help(21, 20), "Xiaohongshu takes 20 characters at most");
        assert_eq!(count_help(2, 20), "Characters");
        assert_eq!(length("e\u{301}字👍🏽"), 3);
    }

    #[test]
    fn file_names_are_two_digits() {
        assert_eq!(card_file_name(0), "01.png");
        assert_eq!(card_file_name(11), "12.png");
    }

    #[test]
    fn handles_get_their_at() {
        assert_eq!(handle_of(" fumi "), "@fumi");
        assert_eq!(handle_of("@fumi"), "@fumi");
        assert_eq!(handle_of("  "), "");
    }

    #[test]
    fn labels() {
        assert_eq!(left_out_label(1), "1 part left out of the cards.");
        assert_eq!(left_out_label(3), "3 parts left out of the cards.");
        assert_eq!(template_label(Template::PostDark, Ratio::Square), "Post Dark · 1:1");
    }

    #[test]
    fn choosing_the_default_clears_the_override() {
        let mut c = NoteChoices::default();
        choose_template(&mut c, Template::Grid, Template::Memo);
        assert_eq!(c.template.as_deref(), Some("grid"));
        choose_template(&mut c, Template::Memo, Template::Memo);
        assert_eq!(c.template, None);
        choose_ratio(&mut c, Ratio::Square, Ratio::Portrait);
        assert_eq!(c.ratio.as_deref(), Some("1:1"));
        leave_out(&mut c, &["b".to_string(), "a".to_string()]);
        leave_out(&mut c, &["a".to_string()]);
        assert_eq!(c.left_out, ["a", "b"]);
    }

    #[test]
    fn choices_keep_swifts_json_keys() {
        let c = NoteChoices { template: Some("post".into()), left_out: vec!["x".into()], ..Default::default() };
        assert_eq!(serde_json::to_string(&c).unwrap(), r#"{"template":"post","leftOut":["x"]}"#);
        let back: NoteChoices = serde_json::from_str(r#"{"title":"T","leftOut":[]}"#).unwrap();
        assert_eq!(back.title.as_deref(), Some("T"));
    }

    #[test]
    fn input_keeps_swifts_json_keys() {
        let deck = deck_for("# Title\n\nHello", "/tmp/note.md", &NoteChoices::default());
        let input = Input {
            deck,
            template: "memo".into(),
            ratio: "3:4".into(),
            page_numbers: true,
            author: Author { name: "A".into(), handle: "".into(), avatar: None },
            end_card: EndCard { on: false, line: "".into() },
            accent: "#000000".into(),
            mode: "preview".into(),
        };
        let value: serde_json::Value = serde_json::from_str(&input.json()).unwrap();
        assert_eq!(value["pageNumbers"], true);
        assert_eq!(value["endCard"]["on"], false);
        assert_eq!(value["mode"], "preview");
        assert!(value["author"].get("avatar").is_none());
        assert_eq!(value["deck"]["title"], "Title");
        assert!(value["deck"]["blocks"].is_array());
    }

    #[test]
    fn folder_starts_at_the_nearest_that_exists() {
        let library = Path::new("/lib");
        assert_eq!(save_start(library, |p| p == Path::new("/lib/attachments")), Some(PathBuf::from("/lib/attachments")));
        assert_eq!(save_start(library, |_| true), Some(PathBuf::from("/lib/attachments/xhs")));
        assert_eq!(save_start(library, |p| p == library), Some(PathBuf::from("/lib")));
    }

    #[test]
    fn assets_are_served_by_host_and_type() {
        assert!(matches!(asset_for("app", "/cards.html"), Some(Asset::Embedded(_, "text/html"))));
        assert!(matches!(asset_for("app", "/cards.js"), Some(Asset::Embedded(_, "text/javascript"))));
        assert!(asset_for("app", "/other.txt").is_none());
        assert!(matches!(asset_for("local", "/a/b.JPG"), Some(Asset::File(_, "image/jpeg"))));
        assert!(asset_for("local", "/a/b.txt").is_none());
        assert!(asset_for("else", "/a.png").is_none());
    }

    #[test]
    fn one_layout_at_a_time_and_the_newest_waits() {
        let mut q = LayoutQueue::new();
        q.show(1);
        assert_eq!(q.next(), None, "not loaded yet");
        q.loaded();
        assert_eq!(q.next(), Some(1));
        q.show(2);
        q.show(3);
        assert_eq!(q.next(), None, "one is running");
        q.finished();
        assert_eq!(q.next(), Some(3));
        q.finished();
        q.show(3);
        assert_eq!(q.next(), None, "already shown");
        q.relayout();
        assert_eq!(q.next(), Some(3));
    }
}
