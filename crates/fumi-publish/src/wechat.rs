//! Lays out a note as a WeChat Official Account article with the bundled
//! renderer, run in JavaScriptCore. The HTML is what the skill's `render.mjs`
//! writes as article.html: every style inline, ready to paste. A port of
//! `WeChatRenderer` and `WeChatCopy`.
//!
//! The renderer lives on a thread of its own: JavaScriptCore runs one call at a
//! time, and a note with local images reads and encodes them.

use crate::jsc::Js;
use futures::channel::oneshot;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Sender, channel};
use std::sync::{LazyLock, Mutex};

const RENDERER: &str = include_str!("../resources/WeChat/wechat-render.js");

/// What the bundle reads as `/__skill/<path>`.
fn skill_asset(relative: &str) -> Option<&'static str> {
    Some(match relative {
        "assets/theme.json" => include_str!("../resources/WeChat/assets/theme.json"),
        "assets/themes/blueprint.json" => include_str!("../resources/WeChat/assets/themes/blueprint.json"),
        "assets/themes/motion-journal.json" => include_str!("../resources/WeChat/assets/themes/motion-journal.json"),
        "assets/themes/songti-supplement.json" => include_str!("../resources/WeChat/assets/themes/songti-supplement.json"),
        "assets/themes/folio.json" => include_str!("../resources/WeChat/assets/themes/folio.json"),
        "assets/themes/celadon.json" => include_str!("../resources/WeChat/assets/themes/celadon.json"),
        "assets/themes/zine.json" => include_str!("../resources/WeChat/assets/themes/zine.json"),
        _ => return None,
    })
}

/// The skill's fixed themes, by the name `render.mjs` takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Theme {
    BlackRed,
    Blueprint,
    Songti,
    MotionJournal,
    Folio,
    Celadon,
    Zine,
}

impl Theme {
    pub const ALL: [Theme; 7] =
        [Theme::BlackRed, Theme::Blueprint, Theme::Songti, Theme::MotionJournal, Theme::Folio, Theme::Celadon, Theme::Zine];

    pub fn raw(self) -> &'static str {
        match self {
            Theme::BlackRed => "black-red",
            Theme::Blueprint => "blueprint",
            Theme::Songti => "songti-supplement",
            Theme::MotionJournal => "motion-journal",
            Theme::Folio => "folio",
            Theme::Celadon => "celadon",
            Theme::Zine => "zine",
        }
    }

    /// Anything else reads as Black Red, as `WeChatCopy.theme` has it.
    pub fn from_raw(raw: &str) -> Theme {
        Theme::ALL.into_iter().find(|t| t.raw() == raw).unwrap_or(Theme::BlackRed)
    }

    pub fn name(self) -> &'static str {
        match self {
            Theme::BlackRed => "Black Red",
            Theme::Blueprint => "Blueprint",
            Theme::Songti => "Songti",
            Theme::MotionJournal => "Motion",
            Theme::Folio => "Folio",
            Theme::Celadon => "Celadon",
            Theme::Zine => "Zine",
        }
    }

    /// The theme's own accent: its bold text and heading marks.
    pub fn accent(self) -> u32 {
        match self {
            Theme::BlackRed => 0x8F1D22,
            Theme::Blueprint => 0x2455D6,
            Theme::Songti => 0xBD3038,
            Theme::MotionJournal => 0x435924,
            Theme::Folio => 0xA86B1F,
            Theme::Celadon => 0x3E7C72,
            Theme::Zine => 0xFF4FA0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Article {
    /// The article body, all styles inline.
    pub html: String,
    /// From the note's leading `# H1` (left out of the body), its front matter
    /// title, or else its file name.
    pub title: String,
    /// What the renderer couldn't carry over cleanly, in its words.
    pub warnings: Vec<String>,
}

// The renderer's thread --------------------------------------------------------

struct Job {
    markdown: String,
    note_path: String,
    theme: Theme,
    reply: oneshot::Sender<Result<Article, String>>,
}

static WORKER: LazyLock<Mutex<Sender<Job>>> = LazyLock::new(|| {
    let (send, receive) = channel::<Job>();
    let _ = std::thread::Builder::new()
        .name("fumi.wechat-renderer".into())
        .stack_size(16 << 20)
        .spawn(move || {
            // The first call loads the renderer, which takes a moment.
            let renderer = load();
            for job in receive {
                let result = match &renderer {
                    Some(js) => run(js, &job.markdown, &job.note_path, job.theme),
                    None => Err(MISSING.to_string()),
                };
                let _ = job.reply.send(result);
            }
        });
    Mutex::new(send)
});

const MISSING: &str = "The WeChat renderer is missing from this build.";

fn load() -> Option<Js> {
    let js = Js::new()?;
    js.install_host(skill_asset);
    js.evaluate(RENDERER).ok()?;
    js.has_function("FumiWeChat", "render").then_some(js)
}

fn run(js: &Js, markdown: &str, note_path: &str, theme: Theme) -> Result<Article, String> {
    let options = serde_json::json!({ "inputPath": note_path, "theme": theme.raw() }).to_string();
    let json = js.call("FumiWeChat", "render", &separating_embeds(markdown), &options)?;
    parse_article(&json)
}

fn parse_article(json: &str) -> Result<Article, String> {
    const NOTHING: &str = "The WeChat renderer returned nothing.";
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| NOTHING.to_string())?;
    let html = value["html"].as_str().ok_or_else(|| NOTHING.to_string())?.to_string();
    let title = value["title"].as_str().unwrap_or("").to_string();
    let warnings = value["warnings"]
        .as_array()
        .map(|list| list.iter().filter_map(|w| w.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    Ok(Article { html, title, warnings })
}

/// Renders `markdown`; local images resolve against `note_path`'s folder.
pub async fn render(markdown: String, note_path: String, theme: Theme) -> Result<Article, String> {
    let (reply, answer) = oneshot::channel();
    let job = Job { markdown, note_path, theme, reply };
    if WORKER.lock().map_err(|_| MISSING.to_string())?.send(job).is_err() {
        return Err(MISSING.to_string());
    }
    answer.await.unwrap_or_else(|_| Err(MISSING.to_string()))
}

// Embeds -----------------------------------------------------------------------

static WIKI_LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`[^`\n]*`|(!?)\[\[([^\[\]\n]+)\]\]").unwrap());
static EMBED_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^!\[[^\]\n]*\]\([^)\n]*\)$").unwrap());

const IMAGE_EXTENSIONS: [&str; 9] = ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "heic", "avif"];
const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "mov", "m4v", "webm", "avi", "mkv"];

/// `CharacterSet.whitespaces`: the space separators and tab, not line ends.
pub(crate) fn trim(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, '\t' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'))
}

/// `markdown` with Obsidian's `![[…]]` and `[[…]]` turned into what the renderer
/// reads: an embedded image or video becomes `![](path)`, found by `resolve` (the
/// editor's lookup) or left as its name; any other embed or link keeps only its
/// words (the alias after `|`, else the note's name). Code spans and fenced code
/// stay as they are.
pub fn resolving_wiki_links(markdown: &str, resolve: &mut dyn FnMut(&str) -> Option<String>) -> String {
    if !markdown.contains("[[") {
        return markdown.to_string();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut fence: Option<String> = None;
    for line in markdown.split('\n') {
        let trimmed = trim(line);
        if let Some(open) = &fence {
            if trimmed.starts_with(open.as_str()) {
                fence = None;
            }
            lines.push(line.to_string());
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(trimmed[..3].to_string());
            lines.push(line.to_string());
            continue;
        }
        let mut result = String::with_capacity(line.len());
        let mut last = 0;
        for caps in WIKI_LINK.captures_iter(line) {
            let Some(inside) = caps.get(2) else { continue };
            let whole = caps.get(0).expect("the match itself");
            result.push_str(&line[last..whole.start()]);
            last = whole.end();
            let is_embed = caps.get(1).is_some_and(|m| !m.is_empty());
            let parts: Vec<&str> = inside.as_str().splitn(2, '|').filter(|p| !p.is_empty()).map(trim).collect();
            let target = parts.first().copied().unwrap_or("");
            let kind = Path::new(target).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if is_embed && (IMAGE_EXTENSIONS.contains(&kind.as_str()) || VIDEO_EXTENSIONS.contains(&kind.as_str())) {
                let path = resolve(target).unwrap_or_else(|| target.to_string());
                result.push_str(&format!("![](<{path}>)"));
            } else if parts.len() > 1 && !parts[1].is_empty() {
                result.push_str(parts[1]);
            } else {
                // `Note#Heading` reads as "Note › Heading", `#Heading` as the heading.
                let pieces: Vec<&str> = target.split('#').filter(|p| !p.is_empty()).map(trim).collect();
                result.push_str(&pieces.join(" › "));
            }
        }
        result.push_str(&line[last..]);
        lines.push(result);
    }
    lines.join("\n")
}

/// `markdown` with a blank line before and after each line that holds only an
/// image or video. The editor shows such a line as a block of its own even with
/// text right under it; Markdown would run the two into one paragraph, and the
/// renderer turns a video into its placeholder card only when it stands alone.
/// Indented lines (inside a list) and fenced code stay as they are.
pub fn separating_embeds(markdown: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut fence: Option<String> = None;
    let mut after_embed = false;
    for line in markdown.split('\n') {
        let trimmed = trim(line);
        if after_embed && !trimmed.is_empty() {
            lines.push("");
        }
        after_embed = false;
        if let Some(open) = &fence {
            if trimmed.starts_with(open.as_str()) {
                fence = None;
            }
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(trimmed[..3].to_string());
        } else if !line.starts_with(' ') && !line.starts_with('\t') && EMBED_LINE.is_match(trimmed) {
            if lines.last().is_some_and(|last| !trim(last).is_empty()) {
                lines.push("");
            }
            after_embed = true;
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Finds what `![[name]]` names, the way the editor does: by path first, then by
/// file name anywhere in the library.
pub fn resolve_embed(
    name: &str,
    base: Option<&Path>,
    library: Option<&Path>,
    index: Option<&HashMap<String, std::path::PathBuf>>,
) -> Option<String> {
    use fumi_editor::media::{embed_name, resolve_local_path};
    let name = embed_name(name).unwrap_or(name);
    if let Some(path) = resolve_local_path(name, base, library, &|p: &Path| p.exists()).filter(|p| p.exists()) {
        return Some(path.to_string_lossy().into_owned());
    }
    let file = Path::new(name).file_name()?.to_string_lossy().to_lowercase();
    Some(index?.get(&file)?.to_string_lossy().into_owned())
}

// The app speaks English -------------------------------------------------------

static TRANSLATIONS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"^图片文件不存在：(.*)$", "Image not found: ${1}"),
        (r"^本地图片仅支持 PNG/JPEG/GIF/WebP：(.*)$", "Only PNG, JPEG, GIF and WebP images can go in the article: ${1}"),
        (r"^不支持的图片地址：(.*)$", "This image address can't go in the article: ${1}"),
        (r"^图片路径编码无效：(.*)$", "This image path isn't valid: ${1}"),
        (r"^YAML frontmatter 没有结束分隔线", "The note's front matter has no closing ---."),
        (r"^视频 (\d+) 的本地文件未找到：(.*)$", "Video ${1} isn't on this Mac: ${2}"),
        (r"^有 (\d+) 处视频待在公众号后台插入", "${1} video placeholder(s): add each video in the WeChat editor, then remove its card."),
        (r"^含 (\d+) 个 SVG 动画", "${1} animated diagram(s): they play here; check them in WeChat before publishing."),
        (r"^存在 HTTP 图片", "Some images use http://. Prefer https:// and check them after pasting."),
        (r"^含 Base64 图片", "Embedded images go with the article. Check that WeChat keeps them after pasting."),
        (r"^本地图片已嵌入正文", "Images on this Mac go with the article. Check that WeChat keeps them after pasting."),
        (r"^含非 HTTP\(S\) 链接", "Some links aren't web addresses; they won't open in WeChat."),
        (r"^Obsidian Wiki 链接或嵌入未转换", "[[Links]] and ![[embeds]] stay as plain text."),
        (r"^脚注语法未转换", "Footnotes stay as plain text."),
        (r"^数学公式未排版", "Math stays as plain text."),
        (r"^存在未支持的 HTML", "HTML in the note shows as text; it isn't applied."),
        (r"^frontmatter 标题与开头 H1 不同", "The front matter title differs from the first heading; the heading is the title."),
    ]
    .into_iter()
    .map(|(pattern, template)| (Regex::new(pattern).unwrap(), template))
    .collect()
});

/// The renderer's notes are in Chinese (the skill's own words); the app speaks
/// English. A message with no mapping shows as is.
pub fn english(message: &str) -> String {
    for (pattern, template) in TRANSLATIONS.iter() {
        if let Some(caps) = pattern.captures(message) {
            let mut out = String::new();
            caps.expand(template, &mut out);
            return out;
        }
    }
    message.to_string()
}

// The page ---------------------------------------------------------------------

/// The article at phone width: what will be pasted, as it will look, under its
/// title. On a light sheet it sits on the sheet; on a dark one, on a white phone
/// page, as WeChat shows it.
pub const ARTICLE_PAGE: &str = r##"<!doctype html><html><head><meta charset="utf-8"><meta name="referrer" content="no-referrer">
<style>
html,body{margin:0;background:transparent}
body{padding:22px 0 48px;font-family:-apple-system,BlinkMacSystemFont,sans-serif}
body.dark{padding-top:14px}
#column{width:375px;margin:0 auto}
body.dark #column{background:#fff;border-radius:10px;padding-top:20px;overflow:hidden}
#head{position:relative;margin:0 20px;padding-bottom:18px}
#head.empty{display:none}
#head::before{content:"";position:absolute;inset:-6px -10px 10px -10px;border-radius:8px;
  background:rgba(0,0,0,.035);opacity:0}
#head:hover::before{opacity:1}
#title{position:relative;margin:0;font-size:20px;line-height:1.45;font-weight:600;color:#1D1D1F;letter-spacing:.2px}
#copy{position:absolute;right:-4px;bottom:-4px;display:none;align-items:center;gap:5px;height:22px;padding:0 8px;
  border-radius:6px;background:#fff;border:1px solid #D9D9DC;box-shadow:0 1px 3px rgba(0,0,0,.08);
  font:12px -apple-system,BlinkMacSystemFont,sans-serif;color:#1D1D1F;cursor:default}
#head:hover #copy{display:flex}
#copy:active{background:#F2F2F4}
body:not(.dark) #article>*:first-child{background:transparent!important}
#article>*:first-child{padding-top:0!important}
</style></head><body><div id="column">
<div id="head" class="empty"><h1 id="title"></h1><div id="copy"><svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="5" y="5" width="8.5" height="8.5" rx="1.8"/><path d="M10.5 3.2V3a1.5 1.5 0 0 0-1.5-1.5H3A1.5 1.5 0 0 0 1.5 3v6A1.5 1.5 0 0 0 3 10.5h.2"/></svg><span>Copy title</span></div></div>
<div id="article"></div></div>
<script>
document.getElementById('copy').addEventListener('click', () => {
  window.webkit.messageHandlers.wechat.postMessage('copyTitle');
  const label = document.querySelector('#copy span');
  label.textContent = 'Copied';
  setTimeout(() => { label.textContent = 'Copy title'; }, 1200);
});
</script></body></html>"##;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub html: String,
    pub title: String,
    pub is_dark: bool,
}

fn literal(text: &str) -> String {
    format!("{}[0]", serde_json::to_string(&[text]).unwrap_or_else(|_| "[\"\"]".to_string()))
}

/// What to run to go from `previous` to `next`. Updates swap the article in place,
/// so the scroll stays put.
pub fn update_script(previous: Option<&Shown>, next: &Shown) -> String {
    let mut script = String::new();
    if previous.is_none_or(|p| p.html != next.html) {
        script += &format!("document.getElementById('article').innerHTML = {};", literal(&next.html));
    }
    if previous.is_none_or(|p| p.title != next.title) {
        script += &format!(
            "document.getElementById('title').textContent = {};document.getElementById('head').classList.toggle('empty', {});",
            literal(&next.title),
            next.title.is_empty()
        );
    }
    if previous.is_none_or(|p| p.is_dark != next.is_dark) {
        script += &format!("document.body.classList.toggle('dark', {});", next.is_dark);
    }
    script
}

/// `1 thing to check before pasting`.
pub fn notice_header(count: usize, action: &str) -> String {
    if count == 1 {
        format!("1 thing to check before {action}")
    } else {
        format!("{count} things to check before {action}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocking(markdown: &str, path: &str, theme: Theme) -> Result<Article, String> {
        futures::executor::block_on(render(markdown.to_string(), path.to_string(), theme))
    }

    #[test]
    fn renders_as_the_swift_tests_expect() {
        let article = blocking("# Title\n\nHello **world**", "/tmp/note.md", Theme::BlackRed).unwrap();
        assert_eq!(article.title, "Title");
        assert!(!article.html.contains("<h1"), "the title is not in the body");
        assert!(article.html.contains("color:#8f1d22;font-weight:700;\">world</strong>"), "{}", article.html);
        assert!(!article.html.contains("class="), "no classes: WeChat drops them");
        assert_eq!(blocking("Body", "/tmp/My Note.md", Theme::Blueprint).unwrap().title, "My Note");
    }

    #[test]
    fn every_theme_loads_its_file() {
        for theme in Theme::ALL {
            let article = blocking("# T\n\n**bold**", "/tmp/n.md", theme).unwrap();
            assert!(article.html.contains("<strong"), "{theme:?}");
        }
        let hex = format!("{:06x}", Theme::Blueprint.accent());
        assert!(blocking("**b**", "/tmp/n.md", Theme::Blueprint).unwrap().html.to_lowercase().contains(&hex));
    }

    #[test]
    fn folio_celadon_and_zine_draw_their_own_marks() {
        let note = "# T\n\nLead.\n\n## One\n\n## Two\n\n---\n\n> [!END]\n> Bye.";
        for (theme, number, divider, end) in [
            (Theme::Folio, "— II —", "· · ·", "（完）"),
            (Theme::Celadon, ">2</span>", "◇ ◇ ◇", ">END</p>"),
            (Theme::Zine, ">02</span>", "★ ★ ★", ">THE END</p>"),
        ] {
            let html = blocking(note, "/tmp/n.md", theme).unwrap().html;
            assert!(html.contains(&format!("data-template=\"{}\"", theme.raw())), "{theme:?}");
            for mark in [number, divider, end] {
                assert!(html.contains(mark), "{theme:?} lacks {mark}");
            }
            assert!(!html.contains("<hr"), "{theme:?}");
        }
    }

    #[test]
    fn a_missing_image_is_the_renderers_own_error() {
        let error = blocking("![](/nonexistent/a.png)", "/tmp/note.md", Theme::BlackRed).unwrap_err();
        assert!(error.contains("图片文件不存在"), "{error}");
        assert!(english(&error).starts_with("Image not found: "), "{}", english(&error));
    }

    #[test]
    fn embeds_read_files_through_the_host() {
        let dir = std::env::temp_dir().join(format!("fumi-publish-wechat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A one-pixel PNG.
        let png: [u8; 67] = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
            0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0, 1, 0, 0, 5, 0, 1, 0x0D,
            0x0A, 0x2D, 0xB4, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(dir.join("p.png"), png).unwrap();
        let note = dir.join("n.md");
        let article = blocking("![](p.png)", &note.to_string_lossy(), Theme::BlackRed).unwrap();
        assert!(article.html.contains("data:image/png;base64,iVBOR"), "{}", article.html);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn separates_embeds_as_swift_does() {
        assert_eq!(separating_embeds("Intro:\n![](a.mp4)\nNext line"), "Intro:\n\n![](a.mp4)\n\nNext line");
        assert_eq!(separating_embeds("![](a.mp4)\n\nNext"), "![](a.mp4)\n\nNext");
        assert_eq!(separating_embeds("Text ![](a.png) inline\nmore"), "Text ![](a.png) inline\nmore");
        assert_eq!(separating_embeds("```\n![](a.mp4)\nx\n```"), "```\n![](a.mp4)\nx\n```");
        assert_eq!(separating_embeds("- item\n  ![](a.png)\n  more"), "- item\n  ![](a.png)\n  more");
    }

    #[test]
    fn a_lone_video_becomes_its_placeholder() {
        let video = blocking("![](a.mp4)\n还有一个是台灯", "/tmp/note.md", Theme::BlackRed).unwrap();
        assert!(video.html.contains("在此插入视频"));
        assert!(video.html.contains("还有一个是台灯"));
    }

    #[test]
    fn wiki_links_resolve_as_swift_does() {
        let found: HashMap<&str, &str> = [("10月14日.mp4", "/tmp/notes/attachments/10月14日.mp4"), ("a b.png", "/tmp/notes/a b.png")].into();
        let wiki = |markdown: &str| resolving_wiki_links(markdown, &mut |t| found.get(t).map(|p| p.to_string()));
        assert_eq!(wiki("效果是这样：\n\n![[10月14日.mp4]]"), "效果是这样：\n\n![](</tmp/notes/attachments/10月14日.mp4>)");
        assert_eq!(wiki("![[a b.png|300]]"), "![](</tmp/notes/a b.png>)");
        assert_eq!(wiki("![[missing.mp4]]"), "![](<missing.mp4>)");
        assert_eq!(wiki("见 [[另一篇笔记]] 和 [[笔记|这里]]。"), "见 另一篇笔记 和 这里。");
        assert_eq!(wiki("[[笔记#小节]]、[[#小节]]、![[笔记]]"), "笔记 › 小节、小节、笔记");
        assert_eq!(wiki("`[[不动]]`\n```\n[[也不动]]\n```"), "`[[不动]]`\n```\n[[也不动]]\n```");
    }

    #[test]
    fn messages_map_to_english() {
        assert_eq!(english("视频 2 的本地文件未找到：a.mp4"), "Video 2 isn't on this Mac: a.mp4");
        assert_eq!(english("有 3 处视频待在公众号后台插入，请"), "3 video placeholder(s): add each video in the WeChat editor, then remove its card.");
        assert_eq!(english("含 1 个 SVG 动画"), "1 animated diagram(s): they play here; check them in WeChat before publishing.");
        assert_eq!(english("存在 HTTP 图片，建议"), "Some images use http://. Prefer https:// and check them after pasting.");
        assert_eq!(english("含非 HTTP(S) 链接"), "Some links aren't web addresses; they won't open in WeChat.");
        assert_eq!(english("nothing we know"), "nothing we know");
    }

    #[test]
    fn updates_only_touch_what_changed() {
        let a = Shown { html: "<p>a</p>".into(), title: "T".into(), is_dark: false };
        let first = update_script(None, &a);
        assert!(first.contains("innerHTML") && first.contains("textContent") && first.contains("toggle('dark', false)"));
        assert_eq!(update_script(Some(&a), &a), "");
        let b = Shown { html: "<p>\"b\"</p>".into(), ..a.clone() };
        let script = update_script(Some(&a), &b);
        assert!(script.starts_with("document.getElementById('article').innerHTML = [\"<p>\\\"b\\\"</p>\"][0];"), "{script}");
        assert!(!script.contains("textContent"));
    }

    #[test]
    fn headers_count() {
        assert_eq!(notice_header(1, "pasting"), "1 thing to check before pasting");
        assert_eq!(notice_header(3, "posting"), "3 things to check before posting");
    }

    #[test]
    fn themes_round_trip() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_raw(theme.raw()), theme);
        }
        assert_eq!(Theme::from_raw("nope"), Theme::BlackRed);
    }
}
