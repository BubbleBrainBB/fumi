//! The publish preview beside the page: the WeChat article or the Xiaohongshu
//! cards. One bar on top, where the platform names switch between them; each note
//! keeps its choice. A port of `PublishPreviewPane`, `WeChatPreviewPane`,
//! `XhsPreviewPane` and `PublishParts`.

use crate::cards;
use crate::export;
use crate::mac;
use crate::web::{self, Rect, WebConfig, WebEvent, WebHost};
use crate::wechat::{self, Article, Shown, Theme, notice_header};
use crate::xhs::{self, NoteChoices, Ratio, Template};
use cocoa::base::{id, nil};
use futures::StreamExt;
use fumi_core::app_model::PublishTarget;
use fumi_core::xhs_deck::XhsDeck;
use fumi_toolbar::actions::{extension_wechat, extension_xhs};
use fumi_toolbar::layout::Tween;
use fumi_toolbar::toolbar::publish_target;
use fumi_ui::icons;
use fumi_ui::native::{self, MenuRow, weight};
use fumi_ui::settings::{self, keys};
use fumi_ui::text_field::{FieldEvent, TextField};
use fumi_ui::theme::{hex, mix, t, text_size};
use fumi_workspace::Workspace;
use gpui::prelude::*;
use gpui::*;
use objc::{msg_send, sel, sel_impl};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The preview sheet's width.
pub const WIDTH: f32 = 420.;
pub const BAR_HEIGHT: f32 = 44.;
const CARDS_MIN_HEIGHT: f32 = 160.;
const COPIED_FOR: Duration = Duration::from_millis(1200);
const STATUS_FOR: Duration = Duration::from_secs(4);
const OPEN_MS: f32 = 220.;
const PUBLISH_TARGETS: &str = "fumi.publish.targets";

/// The active note's text as typed, unsaved or not.
pub type LiveText = Rc<dyn Fn(&App) -> Option<String>>;

/// The open note as the preview, Copy for WeChat and the Xiaohongshu export see it.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteText {
    pub note_id: String,
    pub path: String,
    pub markdown: String,
}

fn amber(dark: bool) -> Hsla {
    hex(if dark { 0xE2B04A } else { 0xD99A1E })
}

fn over_limit(dark: bool) -> Hsla {
    hex(if dark { 0xE89A4A } else { 0xC8701A })
}

/// The side the bar shows as selected, and whether the names are a switch.
pub fn platform_names(wechat: bool, xhs: bool, selected: PublishTarget) -> (Vec<PublishTarget>, bool) {
    let is_switch = wechat && xhs;
    let names = [PublishTarget::Wechat, PublishTarget::Xiaohongshu].into_iter().filter(|t| is_switch || *t == selected).collect();
    (names, is_switch)
}

fn platform_title(target: PublishTarget) -> &'static str {
    match target {
        PublishTarget::Wechat => "WeChat",
        PublishTarget::Xiaohongshu => "Xiaohongshu",
    }
}

/// The notices to show now: none while hidden, until they change.
pub fn shown_notices(all: &[String], hidden: Option<&Vec<String>>) -> Vec<String> {
    if hidden.is_some_and(|h| h.as_slice() == all) { Vec::new() } else { all.to_vec() }
}

pub fn notice_is_hidden(all: &[String], hidden: Option<&Vec<String>>) -> bool {
    !all.is_empty() && hidden.is_some_and(|h| h.as_slice() == all)
}

/// Ease in-out over the open and close.
pub fn eased(p: f32) -> f32 {
    let p = p.clamp(0., 1.);
    if p < 0.5 { 2. * p * p } else { 1. - (-2. * p + 2.).powi(2) / 2. }
}

/// How the sheet sits in the detail column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetLayout {
    /// Too narrow for both: it lies over the page's right side.
    pub overlaps: bool,
    /// What the page gives up on its right.
    pub reserved: f32,
    /// How far past the column's right edge the sheet still is.
    pub offset: f32,
    pub opacity: f32,
    pub shadow_alpha: f32,
    pub shadow_blur: f32,
}

pub const SHEET_GAP: f32 = 8.;

/// With room, a second sheet right of the page, 8 pt apart; otherwise over it.
/// `detail_min` is the page's minimum width.
pub fn sheet_layout(detail_width: f32, detail_min: f32, appear: f32) -> SheetLayout {
    let overlaps = detail_width < detail_min + WIDTH + SHEET_GAP;
    let travel = if overlaps { WIDTH } else { WIDTH + SHEET_GAP };
    SheetLayout {
        overlaps,
        reserved: if overlaps { 0. } else { (WIDTH + SHEET_GAP) * appear },
        offset: travel * (1. - appear),
        opacity: appear,
        shadow_alpha: if overlaps { 0.12 } else { 0.05 },
        shadow_blur: if overlaps { 16. } else { 1.5 },
    }
}

/// The detail column's minimum for the page: the capped text column, its side
/// insets, the page's own insets. `AppShellView.detailMinWidth`.
pub fn detail_min_width() -> f32 {
    fumi_editor::editor::column_cap(&settings::editor_width()) + 40. * 2. + 8. * 2.
}

/// What the field holds, unless it's back to the note's own.
pub fn edited_value(typed: &str, own: &str) -> Option<String> {
    (typed != own).then(|| typed.to_string())
}

// Tips -------------------------------------------------------------------------

struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div().pl(px(8.)).pt(px(10.)).child(
            div()
                .max_w(px(360.))
                .px(px(6.))
                .py(px(3.))
                .bg(c.page)
                .border_1()
                .border_color(c.control_border)
                .rounded(px(5.))
                .shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.14),
                    offset: point(px(0.), px(2.)),
                    blur_radius: px(6.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .text_size(px(11.))
                .text_color(c.text2)
                .child(self.0.clone()),
        )
    }
}

fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone())).into()
}

// The pane ----------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Menu {
    Theme,
    Template,
    Export,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Destination {
    Photos,
    AirDrop,
    Folder,
}

#[derive(Clone, Debug, PartialEq)]
struct Status {
    message: String,
    id: u64,
    leaving: bool,
}

type Anchor = Rc<Cell<Bounds<Pixels>>>;

fn anchor() -> Anchor {
    Rc::new(Cell::new(Bounds::default()))
}

/// Remembers where an element ended up, to open a menu under it.
fn tracked(cell: &Anchor) -> impl IntoElement {
    let cell = cell.clone();
    canvas(move |bounds, _, _| cell.set(bounds), |_, _, _, _| {}).absolute().size_full()
}

const PLATFORM_FADE: Duration = Duration::from_millis(150);

fn target_index(target: PublishTarget) -> usize {
    match target {
        PublishTarget::Wechat => 0,
        PublishTarget::Xiaohongshu => 1,
    }
}

pub struct PublishPane {
    workspace: Entity<Workspace>,
    live: LiveText,
    focus: FocusHandle,

    note: Option<NoteText>,

    // Shell
    shown: bool,
    tab: String,
    appear: f32,
    animation: u64,
    detail_width: Rc<Cell<f32>>,
    covered: bool,
    hide_native: bool,
    still: Option<Arc<RenderImage>>,
    menu_open: Option<Menu>,
    theme_anchor: Anchor,
    template_anchor: Anchor,
    export_anchor: Anchor,
    cards_slot: Rc<Cell<Bounds<Pixels>>>,
    radius: f32,

    /// How selected each platform name looks, by `target_index`.
    platform_fade: Option<[Tween; 2]>,

    // WeChat
    article: Option<Article>,
    article_id: u64,
    failure: Option<String>,
    copied: bool,
    copied_id: u64,
    hidden_notices: Option<Vec<String>>,
    render_id: u64,
    rendered_for: Option<(NoteText, Theme)>,
    embed_index: Option<(u64, Arc<HashMap<String, PathBuf>>)>,
    article_web: Option<WebHost>,
    article_loaded: bool,
    article_shown: Option<Shown>,
    article_pushed: Option<(u64, bool)>,

    // Xiaohongshu
    choices: NoteChoices,
    choices_note: Option<String>,
    deck: Option<XhsDeck>,
    deck_for: Option<NoteText>,
    layout_warnings: Vec<String>,
    card_count: usize,
    cards_height: f32,
    status: Option<Status>,
    status_id: u64,
    is_working: bool,
    hidden_warnings: Option<Vec<String>>,
    signature: String,
    cards_web: Option<WebHost>,
    queue: xhs::LayoutQueue<xhs::Input>,
    title_field: Entity<TextField>,
    caption_field: Entity<TextField>,
    scroll: ScrollHandle,

    _subs: Vec<Subscription>,
}

impl PublishPane {
    pub fn new(workspace: Entity<Workspace>, live: LiveText, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title_field = cx.new(|cx| {
            let mut field = TextField::new("", window, cx);
            field.multiline = true;
            field.font_size = px(text_size::EMPHASIS);
            field.line_height = px(20.);
            field
        });
        let caption_field = cx.new(|cx| {
            let mut field = TextField::new("", window, cx);
            field.multiline = true;
            field.enter_adds_line = true;
            field.font_size = px(text_size::BODY);
            field.line_height = px(20.);
            field
        });
        let subs = vec![
            cx.observe_in(&workspace, window, |this, _, window, cx| this.sync(window, cx)),
            cx.subscribe(&title_field, |this, field, e: &FieldEvent, cx| {
                if *e == FieldEvent::Changed {
                    let typed = field.read(cx).text().to_string();
                    let own = this.deck.as_ref().map(|d| d.title.clone()).unwrap_or_default();
                    this.choices.title = edited_value(&typed, &own);
                    this.save_choices();
                    cx.notify();
                }
            }),
            cx.subscribe(&caption_field, |this, field, e: &FieldEvent, cx| {
                if *e == FieldEvent::Changed {
                    let typed = field.read(cx).text().to_string();
                    let own = this.deck.as_ref().map(|d| d.caption.clone()).unwrap_or_default();
                    this.choices.caption = edited_value(&typed, &own);
                    this.save_choices();
                    cx.notify();
                }
            }),
        ];
        let mut this = PublishPane {
            platform_fade: None,
            workspace,
            live,
            focus: cx.focus_handle(),
            note: None,
            shown: false,
            tab: String::new(),
            appear: 0.,
            animation: 0,
            detail_width: Rc::new(Cell::new(f32::MAX)),
            covered: false,
            hide_native: false,
            still: None,
            menu_open: None,
            theme_anchor: anchor(),
            template_anchor: anchor(),
            export_anchor: anchor(),
            cards_slot: anchor(),
            radius: 12.,
            article: None,
            article_id: 0,
            failure: None,
            copied: false,
            copied_id: 0,
            hidden_notices: None,
            render_id: 0,
            rendered_for: None,
            embed_index: None,
            article_web: None,
            article_loaded: false,
            article_shown: None,
            article_pushed: None,
            choices: NoteChoices::default(),
            choices_note: None,
            deck: None,
            deck_for: None,
            layout_warnings: Vec::new(),
            card_count: 0,
            cards_height: 0.,
            status: None,
            status_id: 0,
            is_working: false,
            hidden_warnings: None,
            signature: xhs::settings_signature(),
            cards_web: None,
            queue: xhs::LayoutQueue::new(),
            title_field,
            caption_field,
            scroll: ScrollHandle::new(),
            _subs: subs,
        };
        this.sync(window, cx);
        this
    }

    /// Root passes the entity whose changes mean the text settled: the page host,
    /// which notifies 300 ms after typing pauses.
    pub fn watch<T: 'static>(&mut self, entity: &Entity<T>, window: &mut Window, cx: &mut Context<Self>) {
        let sub = cx.observe_in(entity, window, |this, _, window, cx| this.sync(window, cx));
        self._subs.push(sub);
    }

    /// Settings › Extensions changed something the cards or the bar show.
    pub fn settings_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync(window, cx);
        cx.notify();
    }

    /// Whether root should lay the sheet out: open, or still sliding away.
    pub fn is_visible(&self) -> bool {
        self.appear > 0.
    }

    /// Whether the active tab has the preview open, on a note.
    pub fn is_open(&self) -> bool {
        self.shown
    }

    fn should_show(&self, cx: &App) -> (bool, String, bool) {
        let w = self.workspace.read(cx);
        let model = &w.model;
        let shown = (extension_wechat() || extension_xhs())
            && w.store.is_open()
            && model.is_preview_open()
            && model.selected_note().is_some()
            && !model.shows_memos();
        (shown, model.active_id().to_string(), model.search_presented || model.show_settings || model.info_presented)
    }

    fn target(&self, cx: &App) -> PublishTarget {
        publish_target(self.workspace.read(cx).model.selected_note())
    }

    /// The workspace, the page or Settings changed: follow them.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (shown, tab, covered) = self.should_show(cx);
        let same_tab = tab == self.tab;
        self.tab = tab;
        if shown != self.shown {
            self.shown = shown;
            if same_tab {
                self.animate_open(cx);
            } else {
                self.appear = if shown { 1. } else { 0. };
                self.animation += 1;
            }
            cx.notify();
        } else if !same_tab {
            self.appear = if shown { 1. } else { 0. };
            self.animation += 1;
        }
        self.set_covered(covered, window, cx);
        if self.appear <= 0. && !self.shown {
            self.hide_web_views();
        }
        if self.shown {
            self.refresh(cx);
        }
    }

    fn hide_web_views(&mut self) {
        if let Some(web) = &mut self.article_web {
            web.hide();
        }
        if let Some(web) = &mut self.cards_web {
            web.hide();
        }
    }

    /// Sliding from the right with a fade, 0.22 s, ease in-out.
    fn animate_open(&mut self, cx: &mut Context<Self>) {
        self.animation += 1;
        let animation = self.animation;
        let from = self.appear;
        let to = if self.shown { 1. } else { 0. };
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let start = Instant::now();
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let p = (start.elapsed().as_secs_f32() * 1000. / OPEN_MS).min(1.);
                let alive = this.update(cx, |this, cx| {
                    if this.animation != animation {
                        return false;
                    }
                    this.appear = from + (to - from) * eased(p);
                    if p >= 1. && !this.shown {
                        this.hide_web_views();
                    }
                    cx.notify();
                    true
                });
                if p >= 1. || !alive.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Search, Settings and the Info card float above; GPUI can't draw over a web view, so it goes
    /// away while they're up and a still takes its place.
    fn set_covered(&mut self, covered: bool, window: &mut Window, cx: &mut Context<Self>) {
        // gpui-fast spike: with composition the overlays draw above the live page.
        let covered = covered && !web::composing();
        if covered == self.covered {
            return;
        }
        self.covered = covered;
        if !covered {
            self.hide_native = false;
            self.still = None;
            cx.notify();
            return;
        }
        let target = self.target(cx);
        let web = match target {
            PublishTarget::Wechat => self.article_web.as_ref(),
            PublishTarget::Xiaohongshu => self.cards_web.as_ref(),
        };
        let visible = web.filter(|w| !w.is_hidden()).map(|w| w.web as usize);
        let Some(web_view) = visible else {
            self.hide_native = true;
            self.hide_web_views();
            return;
        };
        let scale = window.scale_factor() as f64;
        let shot = web::snapshot(web_view as id, nil);
        let timer = cx.background_executor().timer(Duration::from_millis(150));
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let image = match futures::future::select(shot, Box::pin(timer)).await {
                futures::future::Either::Left((Ok(Ok(image)), _)) => Some(image as id),
                _ => None,
            };
            let still = image.and_then(|image| unsafe {
                let size: cocoa::foundation::NSSize = msg_send![image, size];
                let out = native::draw_to_render_image(image, size, scale);
                let _: () = msg_send![image, release];
                out
            });
            let _ = this.update(cx, |this, cx| {
                if this.covered {
                    this.still = still;
                    this.hide_native = true;
                    this.hide_web_views();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The open note, as typed.
    fn current_note(&self, cx: &App) -> Option<NoteText> {
        let w = self.workspace.read(cx);
        let id = w.model.selected_note()?;
        let note = w.note(id)?;
        let markdown = (self.live)(cx).unwrap_or_else(|| note.content.clone());
        Some(NoteText { note_id: id.to_string(), path: note.path.to_string_lossy().into_owned(), markdown })
    }

    /// Redraws what the note, the theme or the Settings ask for. Called when a note
    /// opens and when typing pauses, so never per keystroke.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.current_note(cx) else {
            self.note = None;
            return;
        };
        self.note = Some(note.clone());
        match self.target(cx) {
            PublishTarget::Wechat => {
                let key = (note, theme());
                if self.rendered_for.as_ref() != Some(&key) {
                    self.rendered_for = Some(key);
                    self.render_article(cx);
                }
            }
            PublishTarget::Xiaohongshu => {
                let signature = xhs::settings_signature();
                if self.deck_for.as_ref() != Some(&note) || signature != self.signature {
                    self.signature = signature;
                    self.rebuild(cx);
                }
            }
        }
    }

    // WeChat ---------------------------------------------------------------------

    /// `![[…]]` files are found the way the editor finds them, off the main thread;
    /// the library's index is kept until its notes change.
    fn article_task(&mut self, note: NoteText, theme: Theme, cx: &mut Context<Self>) -> Task<Result<Article, String>> {
        let revision = self.workspace.read(cx).revision;
        let library = self.workspace.read(cx).workspace_path();
        let cached = self.embed_index.as_ref().filter(|(r, _)| *r == revision).map(|(_, i)| i.clone());
        let needs_index = cached.is_none() && library.is_some() && note.markdown.contains("![[");
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let scan = library.clone();
            let index = match (cached, needs_index, scan) {
                (Some(index), _, _) => Some(index),
                (None, true, Some(root)) => {
                    let index = cx.background_spawn(async move { Arc::new(fumi_editor::media::build_embed_index(&root)) }).await;
                    let keep = index.clone();
                    let _ = this.update(cx, |this, _| this.embed_index = Some((revision, keep)));
                    Some(index)
                }
                _ => None,
            };
            let NoteText { markdown, path, .. } = note;
            let note_path = path.clone();
            let resolved = cx
                .background_spawn(async move {
                    let base = Path::new(&note_path).parent().map(Path::to_path_buf);
                    wechat::resolving_wiki_links(&markdown, &mut |name| {
                        wechat::resolve_embed(name, base.as_deref(), library.as_deref(), index.as_deref())
                    })
                })
                .await;
            wechat::render(resolved, path, theme).await
        })
    }

    fn render_article(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.note.clone() else { return };
        self.render_id += 1;
        let render_id = self.render_id;
        let task = self.article_task(note, theme(), cx);
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.render_id != render_id {
                    return;
                }
                match result {
                    Ok(article) => {
                        if this.article.as_ref() != Some(&article) {
                            this.article = Some(article);
                            this.article_id += 1;
                        }
                        this.failure = None;
                    }
                    Err(error) => this.failure = Some(wechat::english(&error)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn notices(&self) -> Vec<String> {
        if let Some(failure) = &self.failure {
            return vec![failure.clone()];
        }
        self.article.as_ref().map(|a| a.warnings.iter().map(|w| wechat::english(w)).collect()).unwrap_or_default()
    }

    fn ensure_article_web(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.article_web.is_some() {
            return;
        }
        let config = WebConfig { handler: "wechat", opens_links: true, assets: false, forwards_to: None };
        let Some((host, mut events)) = WebHost::new(config, window) else { return };
        unsafe {
            let base: id = nil;
            let _: id = msg_send![host.web, loadHTMLString: mac::ns(wechat::ARTICLE_PAGE) baseURL: base];
        }
        self.article_web = Some(host);
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = events.next().await {
                let alive = this.update_in(cx, |this, window, cx| this.article_event(event, window, cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn article_event(&mut self, event: WebEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            WebEvent::Finished => {
                self.article_loaded = true;
                self.article_pushed = None;
                cx.notify();
            }
            WebEvent::Message(serde_json::Value::String(message)) => match message.as_str() {
                // WeChat takes the title in a field of its own, so it's copied apart.
                "copyTitle" => {
                    if let Some(article) = &self.article {
                        mac::copy_text(&article.title);
                    }
                }
                "pointer" => self.pressed_in_page(window, cx),
                _ => {}
            },
            _ => {}
        }
    }

    /// The keyboard went with the press into a web view: nothing in GPUI keeps it.
    fn pressed_in_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }

    /// Hands the web view what it doesn't show yet. Only called on a change of
    /// article or mode.
    fn push_article(&mut self) {
        let Some(web) = &self.article_web else { return };
        if !self.article_loaded {
            return;
        }
        let dark = t().dark;
        let key = (self.article_id, dark);
        if self.article_pushed == Some(key) {
            return;
        }
        self.article_pushed = Some(key);
        let next = Shown {
            html: self.article.as_ref().map(|a| a.html.clone()).unwrap_or_default(),
            title: self.article.as_ref().map(|a| a.title.clone()).unwrap_or_default(),
            is_dark: dark,
        };
        web.eval(&wechat::update_script(self.article_shown.as_ref(), &next));
        self.article_shown = Some(next);
    }

    fn copy_clicked(&mut self, cx: &mut Context<Self>) {
        let (Some(article), Some(note)) = (&self.article, &self.note) else { return };
        mac::copy_article(&article.html, &note.markdown);
        self.copied = true;
        self.copied_id += 1;
        let id = self.copied_id;
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                if this.copied_id == id {
                    this.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// ⌥⌘C or Copy for WeChat: copies the open note in the chosen theme without the
    /// preview, as typed. If it can't be laid out, beeps and opens the preview,
    /// which says why.
    pub fn copy_for_wechat(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.current_note(cx) else {
            self.beep_and_open(cx);
            return;
        };
        let markdown = note.markdown.clone();
        let task = self.article_task(note, theme(), cx);
        cx.spawn(async move |this: WeakEntity<Self>, cx| match task.await {
            Ok(article) => mac::copy_article(&article.html, &markdown),
            Err(_) => {
                let _ = this.update(cx, |this, cx| this.beep_and_open(cx));
            }
        })
        .detach();
    }

    fn beep_and_open(&mut self, cx: &mut Context<Self>) {
        native::beep();
        self.workspace.update(cx, |w, cx| {
            if !w.model.is_preview_open() && w.model.selected_note().is_some() {
                w.model.toggle_preview();
                w.navigated(cx);
            }
        });
    }

    // Xiaohongshu -----------------------------------------------------------------

    fn save_choices(&self) {
        if let (Some(note), Some(id)) = (&self.note, &self.choices_note) {
            if note.note_id == *id {
                xhs::set_choices(&self.choices, id);
            }
        }
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.note.clone() else {
            self.deck = None;
            return;
        };
        if self.choices_note.as_deref() != Some(&note.note_id) {
            self.choices = xhs::choices(&note.note_id);
            self.choices_note = Some(note.note_id.clone());
            self.hidden_warnings = None;
        }
        let deck = xhs::deck_for(&note.markdown, &note.path, &self.choices);
        self.queue.show(xhs::input_for(&deck, &self.choices));
        self.deck = Some(deck);
        self.deck_for = Some(note);
        self.sync_fields(cx);
        self.run_layout(cx);
        cx.notify();
    }

    /// Shows the post's title and caption, unless that's what the fields hold.
    fn sync_fields(&mut self, cx: &mut Context<Self>) {
        let Some(deck) = &self.deck else { return };
        let (title, caption) = (xhs::title_of(deck, &self.choices), xhs::caption_of(deck, &self.choices));
        let placeholder = deck.title.clone();
        self.title_field.update(cx, |field, cx| {
            field.placeholder = placeholder.into();
            if field.text() != title && !field.is_composing() {
                field.set_text(title, cx);
            }
        });
        self.caption_field.update(cx, |field, cx| {
            if field.text() != caption && !field.is_composing() {
                field.set_text(caption, cx);
            }
        });
    }

    fn ensure_cards_web(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cards_web.is_some() {
            return;
        }
        let forwards_to = Some(mac::gpui_view_of(window));
        let config = WebConfig { handler: "xhs", opens_links: false, assets: true, forwards_to };
        let Some((host, mut events)) = WebHost::new(config, window) else { return };
        unsafe {
            let url: id = msg_send![objc::class!(NSURL), URLWithString: mac::ns(web::CARDS_PAGE_URL)];
            let request: id = msg_send![objc::class!(NSURLRequest), requestWithURL: url];
            let _: id = msg_send![host.web, loadRequest: request];
        }
        self.cards_web = Some(host);
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = events.next().await {
                let alive = this.update_in(cx, |this, window, cx| this.cards_event(event, window, cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn cards_event(&mut self, event: WebEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            WebEvent::Finished => {
                self.queue.loaded();
                self.run_layout(cx);
            }
            WebEvent::Message(value) => {
                if value.as_str() == Some("pointer") {
                    self.pressed_in_page(window, cx);
                    return;
                }
                match value["type"].as_str() {
                    // The cards laid out again once their images arrived.
                    Some("layout") => self.apply_layout(&value, cx),
                    Some("menu") => self.show_card_menu(&value, window, cx),
                    _ => {}
                }
            }
        }
    }

    fn run_layout(&mut self, cx: &mut Context<Self>) {
        let Some(web) = self.cards_web.as_ref().map(|w| w.web as usize) else { return };
        let Some(input) = self.queue.next() else { return };
        let answer = web::call_async(web as id, cards::LAYOUT_SCRIPT, &input.json());
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let result = answer.await.unwrap_or_else(|_| Err(String::new()));
            let _ = this.update(cx, |this, cx| {
                this.queue.finished();
                match result.map(|text| serde_json::from_str::<serde_json::Value>(&text).unwrap_or_default()) {
                    Ok(value) => this.apply_layout(&value, cx),
                    Err(detail) => {
                        this.card_count = 0;
                        this.layout_warnings = vec![format!("The cards couldn't be laid out: {detail}")];
                        cx.notify();
                    }
                }
                this.run_layout(cx);
            });
        })
        .detach();
    }

    fn apply_layout(&mut self, value: &serde_json::Value, cx: &mut Context<Self>) {
        self.card_count = value["count"].as_u64().unwrap_or(0) as usize;
        self.layout_warnings = value["warnings"]
            .as_array()
            .map(|list| list.iter().filter_map(|w| w.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let height = value["height"].as_f64().unwrap_or(0.) as f32;
        if height > 0. {
            self.cards_height = height;
        }
        cx.notify();
    }

    fn warnings(&self) -> Vec<String> {
        let mut all = self.deck.as_ref().map(|d| d.warnings.clone()).unwrap_or_default();
        all.extend(self.layout_warnings.iter().cloned());
        all
    }

    fn input(&self) -> Option<xhs::Input> {
        self.deck.as_ref().map(|deck| xhs::input_for(deck, &self.choices))
    }

    /// In the notice for 4 s, fading 0.15 in and 0.2 out.
    fn flash(&mut self, message: String, cx: &mut Context<Self>) {
        self.status_id += 1;
        let id = self.status_id;
        self.status = Some(Status { message, id, leaving: false });
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(STATUS_FOR).await;
            let still = this.update(cx, |this, cx| {
                let Some(status) = this.status.as_mut().filter(|s| s.id == id) else { return false };
                status.leaving = true;
                cx.notify();
                true
            });
            if still.unwrap_or(false) {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                let _ = this.update(cx, |this, cx| {
                    if this.status.as_ref().is_some_and(|s| s.id == id) {
                        this.status = None;
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn export(&mut self, destination: Destination, window: &mut Window, cx: &mut Context<Self>) {
        let Some(deck) = self.deck.clone() else { return };
        if self.is_working {
            return;
        }
        let input = xhs::input_for(&deck, &self.choices);
        let name = xhs::title_of(&deck, &self.choices);
        let caption = xhs::caption_of(&deck, &self.choices);
        let library = self.workspace.read(cx).workspace_path();
        cx.spawn_in(window, async move |this, cx| {
            // Ask where first, so the cards render only once there's a place.
            let mut folder = None;
            if destination == Destination::Folder {
                let start = library.as_deref().and_then(|l| xhs::save_start(l, |p| p.exists()));
                let safe = xhs::safe_name(&name);
                let chosen = mac::run_save_panel(mac::SavePanel {
                    title: Some("Save Cards"),
                    prompt: Some("Save"),
                    message: Some("The cards are saved as 01.png, 02.png, … in a folder with this name."),
                    name: &safe,
                    directory: start.as_deref(),
                    content_type: None,
                });
                let Some(chosen) = chosen else { return };
                folder = Some(chosen);
            }
            if this.update(cx, |this, cx| {
                this.is_working = true;
                cx.notify();
            }).is_err() {
                return;
            }
            let outcome = export_to(destination, &input, &name, &caption, folder.as_deref(), cx).await;
            let _ = this.update(cx, |this, cx| {
                this.is_working = false;
                let message = match outcome {
                    Ok(message) => message,
                    Err(error) => error,
                };
                if !message.is_empty() {
                    this.flash(message, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// ⌥⌘C with the preview on Xiaohongshu: the cards into Photos and the caption
    /// onto the clipboard. If that fails, beeps and opens the preview, which says why.
    pub fn add_xhs_cards(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.current_note(cx) else {
            self.beep_and_open(cx);
            self.flash("Open a note first.".to_string(), cx);
            return;
        };
        let choices = xhs::choices(&note.note_id);
        let deck = xhs::deck_for(&note.markdown, &note.path, &choices);
        let input = xhs::input_for(&deck, &choices);
        let name = xhs::title_of(&deck, &choices);
        let caption = xhs::caption_of(&deck, &choices);
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let outcome = export_to(Destination::Photos, &input, &name, &caption, None, cx).await;
            let _ = this.update(cx, |this, cx| match outcome {
                Ok(message) => this.flash(message, cx),
                Err(error) => {
                    this.beep_and_open(cx);
                    this.flash(error, cx);
                }
            });
        })
        .detach();
    }

    // Menus -------------------------------------------------------------------------

    /// Menu tracking runs a nested run loop, so it happens in a task of its own.
    fn popup(
        &mut self,
        which: Option<Menu>,
        rows: Vec<MenuRow>,
        at: Point<Pixels>,
        then: impl FnOnce(&mut Self, usize, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ns_window = native::ns_window_of(window) as usize;
        self.menu_open = which;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let chosen = native::popup_menu_on(ns_window as id, &rows, at);
            let _ = this.update_in(cx, |this, window, cx| {
                this.menu_open = None;
                if let Some(index) = chosen {
                    then(this, index, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn below(bounds: Bounds<Pixels>) -> Point<Pixels> {
        point(bounds.left(), bounds.bottom() + px(4.))
    }

    fn show_theme_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = theme();
        let swatches = mac::theme_swatches();
        let rows = Theme::ALL
            .iter()
            .zip(swatches)
            .map(|(option, image)| MenuRow::Choice { title: option.name().to_string(), checked: *option == current, image: Some(image) })
            .collect();
        let at = Self::below(self.theme_anchor.get());
        self.popup(
            Some(Menu::Theme),
            rows,
            at,
            |this, index, _, cx| {
                if let Some(option) = Theme::ALL.get(index) {
                    settings::set_string(keys::WECHAT_THEME, option.raw());
                    this.refresh(cx);
                }
            },
            window,
            cx,
        );
    }

    fn show_template_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (template, ratio) = (xhs::template_of(&self.choices), xhs::ratio_of(&self.choices));
        let mut rows = vec![MenuRow::Label("Template".to_string())];
        rows.extend(Template::ALL.map(|o| MenuRow::Choice { title: o.name().to_string(), checked: o == template, image: None }));
        rows.push(MenuRow::Separator);
        rows.push(MenuRow::Label("Card Size".to_string()));
        rows.extend(Ratio::ALL.map(|o| MenuRow::Choice { title: o.menu_title().to_string(), checked: o == ratio, image: None }));
        let at = Self::below(self.template_anchor.get());
        self.popup(
            Some(Menu::Template),
            rows,
            at,
            |this, index, _, cx| {
                if let Some(option) = Template::ALL.get(index) {
                    xhs::choose_template(&mut this.choices, *option, xhs::default_template());
                } else if let Some(option) = Ratio::ALL.get(index - Template::ALL.len()) {
                    xhs::choose_ratio(&mut this.choices, *option, xhs::default_ratio());
                }
                this.save_choices();
                this.rebuild(cx);
            },
            window,
            cx,
        );
    }

    fn show_export_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.card_count.max(1);
        let rows = vec![
            MenuRow::Item { title: format!("Add {count} Images to Photos"), enabled: true, destructive: false, key: Some("⌥c".to_string()) },
            MenuRow::item("AirDrop…"),
            MenuRow::item("Save Images to Folder…"),
            MenuRow::Separator,
            MenuRow::item("Copy Caption"),
            MenuRow::item("Copy Title"),
        ];
        let at = Self::below(self.export_anchor.get());
        self.popup(
            None,
            rows,
            at,
            |this, index, window, cx| match index {
                0 => this.export(Destination::Photos, window, cx),
                1 => this.export(Destination::AirDrop, window, cx),
                2 => this.export(Destination::Folder, window, cx),
                3 => {
                    if let Some(deck) = &this.deck {
                        mac::copy_text(&xhs::caption_of(deck, &this.choices));
                        this.flash("Caption copied.".to_string(), cx);
                    }
                }
                4 => {
                    if let Some(deck) = &this.deck {
                        mac::copy_text(&xhs::title_of(deck, &this.choices));
                        this.flash("Title copied.".to_string(), cx);
                    }
                }
                _ => {}
            },
            window,
            cx,
        );
    }

    /// Right-clicking a card: `x` and `y` are in the page.
    fn show_card_menu(&mut self, value: &serde_json::Value, window: &mut Window, cx: &mut Context<Self>) {
        let index = value["index"].as_u64().unwrap_or(0) as usize;
        let ids: Vec<String> =
            value["ids"].as_array().map(|l| l.iter().filter_map(|i| i.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let origin = self.cards_slot.get().origin;
        let at = origin + point(px(value["x"].as_f64().unwrap_or(0.) as f32), px(value["y"].as_f64().unwrap_or(0.) as f32));
        let mut rows = vec![MenuRow::item("Copy Image"), MenuRow::item("Save Image…")];
        if !ids.is_empty() {
            rows.push(MenuRow::Separator);
            rows.push(MenuRow::item("Leave Out of Cards"));
        }
        let web = self.cards_web.as_ref().map(|w| w.web as usize);
        self.popup(
            None,
            rows,
            at,
            move |this, chosen, window, cx| match chosen {
                0 => this.card_image(index, false, window, cx),
                1 => this.card_image(index, true, window, cx),
                2 => {
                    xhs::leave_out(&mut this.choices, &ids);
                    this.save_choices();
                    this.rebuild(cx);
                }
                _ => {}
            },
            window,
            cx,
        );
        // The menu has run by the time the next task does.
        if let Some(web) = web {
            cx.spawn(async move |_: WeakEntity<Self>, _| web::evaluate(web as id, "FumiXhs.menuClosed()")).detach();
        }
    }

    fn card_image(&mut self, index: usize, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.input() else { return };
        cx.spawn_in(window, async move |this, cx| {
            let rendered = cards::render(&input, Some(index), cx).await;
            let _ = this.update(cx, |this, cx| match rendered {
                Ok(images) => {
                    let Some(png) = images.into_iter().next() else { return };
                    if save {
                        cx.spawn(async move |this: WeakEntity<Self>, cx| {
                            let name = xhs::card_file_name(index);
                            let path = mac::run_save_panel(mac::SavePanel {
                                title: None,
                                prompt: None,
                                message: None,
                                name: &name,
                                directory: None,
                                content_type: Some("public.png"),
                            });
                            let Some(path) = path else { return };
                            if let Err(error) = std::fs::write(path, png) {
                                let _ = this.update(cx, |this, cx| this.flash(error.to_string(), cx));
                            }
                        })
                        .detach();
                    } else {
                        mac::copy_image(&png);
                        this.flash("Image copied.".to_string(), cx);
                    }
                }
                Err(error) => this.flash(error, cx),
            });
        })
        .detach();
    }

    // PDF and Markdown ---------------------------------------------------------------

    /// More › Export as PDF…: the text as typed, even if unsaved.
    pub fn export_pdf(&mut self, note_id: &str, cx: &mut Context<Self>) {
        let w = self.workspace.read(cx);
        let Some(note) = w.note(note_id).cloned() else { return };
        let library = w.workspace_path();
        let links = w.links.clone();
        let is_active = w.model.selected_note() == Some(note_id);
        let live = is_active.then(|| (self.live)(cx)).flatten().filter(|text| !text.is_empty());
        let markdown = live.unwrap_or_else(|| note.content.clone());
        cx.spawn(async move |_: WeakEntity<Self>, cx| {
            export::export_pdf(&note, markdown, library, links, cx).await;
        })
        .detach();
    }

    /// File › Export All Notes…
    pub fn export_all_notes(&mut self, cx: &mut Context<Self>) {
        let notes: Vec<(String, String, String)> = self
            .workspace
            .read(cx)
            .store
            .notes()
            .iter()
            .map(|n| (n.title.clone(), n.filename(), n.content.clone()))
            .collect();
        cx.spawn(async move |_: WeakEntity<Self>, cx| {
            let Some(folder) = mac::choose_export_folder() else { return };
            cx.background_spawn(async move { export::write_all_notes(&notes, &folder) }).await;
        })
        .detach();
    }

    // Drawing ---------------------------------------------------------------------

    fn set_target(&mut self, target: PublishTarget, cx: &mut Context<Self>) {
        let raw = match target {
            PublishTarget::Wechat => "wechat",
            PublishTarget::Xiaohongshu => "xiaohongshu",
        };
        if let Some(id) = self.workspace.read(cx).model.selected_note() {
            settings::set_dict_string(PUBLISH_TARGETS, id, raw);
        }
        settings::set_dict_string(PUBLISH_TARGETS, "", raw);
        self.refresh(cx);
        cx.notify();
    }

    /// Heads each platform name for what it should be now.
    fn sync_platform_fade(&mut self, selected: PublishTarget, window: &mut Window) {
        let now = Instant::now();
        let weight = |target| if target == selected { 1. } else { 0. };
        let targets = [PublishTarget::Wechat, PublishTarget::Xiaohongshu];
        let fades = self.platform_fade.get_or_insert_with(|| targets.map(|target| Tween::settled(weight(target))));
        for target in targets {
            let fade = &mut fades[target_index(target)];
            fade.go(weight(target), PLATFORM_FADE, now);
            if fade.is_moving(now) {
                window.request_animation_frame();
            }
        }
    }

    fn platform_row(&self, selected: PublishTarget, cx: &mut Context<Self>) -> Div {
        let c = t();
        let now = Instant::now();
        let (names, is_switch) = platform_names(extension_wechat(), extension_xhs(), selected);
        div().flex().items_center().gap(px(14.)).children(names.into_iter().map(|target| {
            let title = div().text_size(px(text_size::BODY)).font_weight(FontWeight::SEMIBOLD);
            if is_switch {
                let is_selected = target == selected;
                // Selection fades, 0.15 s; hover answers at once, as in Swift.
                let weight = self.platform_fade.map_or(if is_selected { 1. } else { 0. }, |fades| fades[target_index(target)].value(now));
                title
                    .id(platform_title(target))
                    .text_color(mix(c.faint_text, c.text, weight))
                    .when(!is_selected, |el| el.hover(|s| s.text_color(c.text2)))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_target(target, cx)))
                    .child(platform_title(target))
                    .into_any_element()
            } else {
                title.text_color(c.text).child(platform_title(target)).into_any_element()
            }
        }))
    }

    fn bar(&self, selected: PublishTarget, trailing: Vec<AnyElement>, cx: &mut Context<Self>) -> Div {
        let c = t();
        div()
            .flex_none()
            .h(px(BAR_HEIGHT))
            .pl(px(16.))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(c.text.opacity(0.06))
            .child(self.platform_row(selected, cx))
            .child(div().flex_1().min_w(px(8.)))
            .children(trailing)
    }

    fn popup_button(
        &self,
        name: &'static str,
        help: &'static str,
        which: Menu,
        cell: &Anchor,
        label: Vec<AnyElement>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = t();
        div()
            .id(name)
            .relative()
            .h(px(24.))
            .pl(px(8.))
            .pr(px(7.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .text_size(px(text_size::CAPTION))
            .text_color(c.text2)
            .when(self.menu_open == Some(which), |el| el.bg(c.inline_bg))
            .hover(|s| s.bg(c.inline_bg))
            .tooltip(tip(help))
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
            .child(tracked(cell))
            .children(label)
            .child(icons::symbol("chevron.down", 8., weight::SEMIBOLD, c.icon))
            .into_any_element()
    }

    /// The side's one button, Copy or Export: a plain bordered button, so the color
    /// on the sheet is the article's own.
    fn pane_button(name: &'static str, help: &'static str, enabled: bool, content: Vec<AnyElement>) -> Stateful<Div> {
        let c = t();
        div()
            .id(name)
            .relative()
            .h(px(24.))
            .px(px(11.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(6.))
            .bg(if c.dark { hsla(0., 0., 1., 0.1) } else { gpui::white() })
            .border_1()
            .border_color(c.control_border)
            .when(!c.dark, |el| {
                el.shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.05),
                    offset: point(px(0.), px(0.5)),
                    blur_radius: px(0.5),
                    spread_radius: px(0.),
                    inset: false,
                }])
            })
            .text_size(px(text_size::CAPTION))
            .font_weight(FontWeight::MEDIUM)
            .text_color(c.text)
            .tooltip(tip(help))
            .when(enabled, |el| el.active(|s| s.bg(c.inline_bg)))
            .when(!enabled, |el| el.opacity(0.45))
            .children(content)
    }

    fn quiet_link(name: &'static str, title: &'static str) -> Stateful<Div> {
        let c = t();
        div().id(name).text_size(px(text_size::CAPTION)).text_color(c.text3).hover(|s| s.text_color(c.text)).child(title)
    }

    /// A hidden notice, left in the bar as its dot; clicking shows it again.
    fn notice_dot(cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        div()
            .id("notice-dot")
            .w(px(18.))
            .h(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .tooltip(tip("Show what to check"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.hidden_notices = None;
                this.hidden_warnings = None;
                cx.notify();
            }))
            .child(div().size(px(6.)).rounded_full().bg(amber(c.dark)))
            .into_any_element()
    }

    /// What to look at before posting: an amber dot and one line, the details under
    /// it, Hide to fold it into a dot in the bar.
    fn notice(header: Option<String>, items: &[String], hide: Option<AnyElement>, extra: Vec<AnyElement>) -> Div {
        let c = t();
        div()
            .flex_none()
            .w_full()
            .px(px(16.))
            .pt(px(9.))
            .pb(px(10.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .text_size(px(text_size::CAPTION))
            .line_height(px(17.))
            .border_b_1()
            .border_color(c.text.opacity(0.06))
            .when_some(header.clone(), |el, header| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .child(div().flex_none().size(px(6.)).rounded_full().bg(amber(c.dark)))
                        .child(div().font_weight(FontWeight::MEDIUM).text_color(c.text).child(header))
                        .child(div().flex_1().min_w(px(8.)))
                        .children(hide),
                )
            })
            .children(items.iter().map(|item| {
                div().text_color(c.text3).when(header.is_some(), |el| el.pl(px(13.))).child(item.clone())
            }))
            .children(extra)
    }

    /// The web view's place in the layout. It follows on every frame; its still, while
    /// something floats above it, is drawn here.
    fn slot(&self, cards: bool, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let still = self.still.clone();
        let handles = if cards { self.cards_web.as_ref() } else { self.article_web.as_ref() }.map(|w| w.window_handles());
        canvas(
            move |bounds, window, cx| {
                let clip = window.content_mask().bounds;
                entity.update(cx, |this, cx| this.place_web(cards, bounds, clip, cx));
            },
            move |bounds, _, window, _| {
                if let Some(image) = still {
                    let _ = window.paint_image(bounds, bounds, Corners::default(), image, 0, false);
                }
                // AppKit leaves the keyboard with the web view when GPUI is clicked.
                if let Some((ns_window, view)) = handles {
                    window.on_mouse_event(move |_: &MouseDownEvent, phase, _, _| {
                        if phase == DispatchPhase::Capture {
                            mac::claim_first_responder(ns_window as id, view as id);
                        }
                    });
                }
            },
        )
        .size_full()
        .into_any_element()
    }

    fn place_web(&mut self, cards: bool, bounds: Bounds<Pixels>, clip: Bounds<Pixels>, cx: &mut Context<Self>) {
        let alpha = (self.appear as f64) * if !cards && self.failure.is_some() { 0.35 } else { 1. };
        let radius = (self.radius as f64 - 1.).max(0.);
        let hidden = self.hide_native || self.appear <= 0.;
        if cards {
            self.cards_slot.set(bounds);
            let Some(web) = &mut self.cards_web else { return };
            web.set_corner_radius(radius);
            if hidden {
                web.hide();
            } else if web.place(Rect::of(bounds), Rect::of(clip), alpha) {
                // A new width lays the cards out again.
                self.queue.relayout();
                self.run_layout(cx);
            }
        } else if let Some(web) = &mut self.article_web {
            web.set_corner_radius(radius);
            if hidden {
                web.hide();
            } else {
                web.place(Rect::of(bounds), Rect::of(clip), alpha);
            }
        }
    }

    fn wechat_side(&mut self, cx: &mut Context<Self>) -> Div {
        let c = t();
        let notices = self.notices();
        let is_hidden = self.failure.is_none() && notice_is_hidden(&notices, self.hidden_notices.as_ref());
        let current = theme();
        let mut trailing = Vec::new();
        if is_hidden {
            trailing.push(Self::notice_dot(cx));
        }
        let theme_cell = self.theme_anchor.clone();
        trailing.push(self.popup_button(
            "theme",
            "Theme",
            Menu::Theme,
            &theme_cell,
            vec![
                div().flex_none().size(px(8.)).rounded_full().bg(hex(current.accent())).into_any_element(),
                div().child(current.name()).into_any_element(),
            ],
            |this, window, cx| this.show_theme_menu(window, cx),
            cx,
        ));
        let enabled = self.article.is_some() && self.failure.is_none();
        let mut content = Vec::new();
        if self.copied {
            content.push(icons::symbol("checkmark", 10., weight::SEMIBOLD, c.text));
        }
        content.push(div().child(if self.copied { "Copied" } else { "Copy" }).into_any_element());
        trailing.push(
            Self::pane_button("copy", "Copy for WeChat (⌥⌘C)", enabled, content)
                .when(enabled, |el| el.on_click(cx.listener(|this, _, _, cx| this.copy_clicked(cx))))
                .into_any_element(),
        );
        let notice = if notices.is_empty() || is_hidden {
            None
        } else if self.failure.is_some() {
            Some(Self::notice(Some("This note can’t be laid out".to_string()), &notices, None, Vec::new()))
        } else {
            let keep = notices.clone();
            let hide = Self::quiet_link("hide", "Hide")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.hidden_notices = Some(keep.clone());
                    cx.notify();
                }))
                .into_any_element();
            Some(Self::notice(Some(notice_header(notices.len(), "pasting")), &notices, Some(hide), Vec::new()))
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.bar(PublishTarget::Wechat, trailing, cx))
            .children(notice)
            .child(div().flex_1().min_h_0().child(self.slot(false, cx)))
    }

    fn post_field(
        &self,
        label: &'static str,
        count: usize,
        limit: usize,
        reset: Option<AnyElement>,
        value: AnyElement,
    ) -> Div {
        let c = t();
        let over = count > limit;
        let features = FontFeatures(Arc::new(vec![("tnum".to_string(), 1)]));
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(text_size::CAPTION))
                    .text_color(c.text3)
                    .child(label)
                    .child(div().flex_1().min_w(px(8.)))
                    .children(reset)
                    .child(
                        div()
                            .id(label)
                            .font_features(features)
                            .text_color(if over { over_limit(c.dark) } else { c.text3 })
                            .tooltip(tip(xhs::count_help(count, limit)))
                            .child(xhs::count_label(count, limit)),
                    ),
            )
            .child(value)
    }

    fn post_fields(&self, deck: &XhsDeck, cx: &mut Context<Self>) -> Div {
        let c = t();
        let title = xhs::title_of(deck, &self.choices);
        let caption = xhs::caption_of(deck, &self.choices);
        let reset_title = self.choices.title.is_some().then(|| {
            Self::quiet_link("reset-title", "Reset")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.choices.title = None;
                    this.save_choices();
                    this.sync_fields(cx);
                    cx.notify();
                }))
                .into_any_element()
        });
        let reset_caption = self.choices.caption.is_some().then(|| {
            Self::quiet_link("reset-caption", "Reset")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.choices.caption = None;
                    this.save_choices();
                    this.sync_fields(cx);
                    cx.notify();
                }))
                .into_any_element()
        });
        let title_value = div()
            .id("post-title")
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c.text)
            .tooltip(tip("The post's title. Changing it here doesn't change the note."))
            .child(self.title_field.clone())
            .into_any_element();
        let caption_value = div().text_color(c.text).child(self.caption_field.clone()).into_any_element();
        div()
            .px(px(22.))
            .pt(px(18.))
            .pb(px(28.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(self.post_field("Title", xhs::length(&title), xhs::TITLE_LIMIT, reset_title, title_value))
            .child(self.post_field("Caption", xhs::length(&caption), xhs::CAPTION_LIMIT, reset_caption, caption_value))
    }

    fn template_glyph(template: Template) -> AnyElement {
        div()
            .flex_none()
            .w(px(9.))
            .h(px(12.))
            .rounded(px(2.))
            .overflow_hidden()
            .bg(hex(template.paper()))
            .border(px(0.5))
            .border_color(hsla(0., 0., 0., 0.18))
            .when(template == Template::Memo, |el| el.child(div().w_full().h(px(3.)).bg(hex(0xFFE070))))
            .into_any_element()
    }

    fn xhs_side(&mut self, cx: &mut Context<Self>) -> Div {
        let c = t();
        let warnings = self.warnings();
        let hidden = notice_is_hidden(&warnings, self.hidden_warnings.as_ref());
        let shown = shown_notices(&warnings, self.hidden_warnings.as_ref());
        let (template, ratio) = (xhs::template_of(&self.choices), xhs::ratio_of(&self.choices));

        let mut trailing = Vec::new();
        if hidden {
            trailing.push(Self::notice_dot(cx));
        }
        let cell = self.template_anchor.clone();
        trailing.push(self.popup_button(
            "template",
            "Template and card size",
            Menu::Template,
            &cell,
            vec![Self::template_glyph(template), div().child(xhs::template_label(template, ratio)).into_any_element()],
            |this, window, cx| this.show_template_menu(window, cx),
            cx,
        ));
        let enabled = self.deck.is_some() && !self.is_working;
        let export_cell = self.export_anchor.clone();
        trailing.push(
            Self::pane_button(
                "export",
                "Export the cards (⌥⌘C adds them to Photos)",
                enabled,
                vec![
                    tracked(&export_cell).into_any_element(),
                    div().child(if self.is_working { "Exporting…" } else { "Export" }).into_any_element(),
                    icons::symbol("chevron.down", 8., weight::SEMIBOLD, c.text),
                ],
            )
            .when(enabled, |el| el.on_click(cx.listener(|this, _, window, cx| this.show_export_menu(window, cx))))
            .into_any_element(),
        );

        let left_out = self.deck.as_ref().map_or(0, |d| d.left_out_count);
        let mut extra = Vec::new();
        if left_out > 0 {
            extra.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().text_color(c.text2).child(xhs::left_out_label(left_out)))
                    .child(Self::quiet_link("put-back", "Put Back").on_click(cx.listener(|this, _, _, cx| {
                        this.choices.left_out.clear();
                        this.save_choices();
                        this.rebuild(cx);
                    })))
                    .into_any_element(),
            );
        }
        if let Some(status) = &self.status {
            let text = div().text_color(c.text).child(status.message.clone());
            let (leaving, id) = (status.leaving, status.id);
            extra.push(if leaving {
                text.with_animation(("status-out", id), Animation::new(Duration::from_millis(200)), |el, p| el.opacity(1. - p))
                    .into_any_element()
            } else {
                text.with_animation(("status-in", id), Animation::new(Duration::from_millis(150)), |el, p| el.opacity(p))
                    .into_any_element()
            });
        }
        let notice = (!extra.is_empty() || !shown.is_empty()).then(|| {
            let header = (!shown.is_empty()).then(|| notice_header(shown.len(), "posting"));
            let hide = (!shown.is_empty()).then(|| {
                let keep = warnings.clone();
                Self::quiet_link("hide", "Hide")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.hidden_warnings = Some(keep.clone());
                        cx.notify();
                    }))
                    .into_any_element()
            });
            Self::notice(header, &shown, hide, extra)
        });

        let deck = self.deck.clone();
        let body = match deck {
            Some(deck) => div().flex_1().min_h_0().child(
                div()
                    .id("xhs-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(div().w_full().h(px(self.cards_height.max(CARDS_MIN_HEIGHT))).child(self.slot(true, cx)))
                    .child(self.post_fields(&deck, cx)),
            ),
            None => div().flex_1(),
        };
        div().size_full().flex().flex_col().child(self.bar(PublishTarget::Xiaohongshu, trailing, cx)).children(notice).child(body)
    }
}

/// The chosen theme.
pub fn theme() -> Theme {
    Theme::from_raw(&settings::get_string(keys::WECHAT_THEME).unwrap_or_default())
}

/// Makes the files and sends them; says what happened.
async fn export_to(
    destination: Destination,
    input: &xhs::Input,
    name: &str,
    caption: &str,
    folder: Option<&Path>,
    cx: &AsyncApp,
) -> Result<String, String> {
    let files = cards::render_files(input, name, cx).await?;
    match destination {
        Destination::Photos => {
            cards::add_to_photos(&files).await?;
            mac::copy_text(caption);
            Ok(format!("{} images added to Photos. Caption copied.", files.len()))
        }
        Destination::AirDrop => {
            if cards::air_drop(&files) {
                mac::copy_text(caption);
                Ok("Caption copied.".to_string())
            } else {
                Ok("AirDrop isn't available on this Mac.".to_string())
            }
        }
        Destination::Folder => {
            let Some(folder) = folder else { return Ok(String::new()) };
            cards::copy_to_folder(&files, folder)?;
            native::reveal_in_finder(folder);
            let shown = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            Ok(format!("{} images saved to {shown}.", files.len()))
        }
    }
}

impl Focusable for PublishPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PublishPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let target = self.target(cx);
        self.sync_platform_fade(target, window);
        match target {
            PublishTarget::Wechat => {
                self.ensure_article_web(window, cx);
                self.push_article();
                if let Some(web) = &mut self.cards_web {
                    web.hide();
                }
            }
            PublishTarget::Xiaohongshu => {
                self.ensure_cards_web(window, cx);
                if let Some(web) = &mut self.article_web {
                    web.hide();
                }
            }
        }
        let side = match target {
            PublishTarget::Wechat => self.wechat_side(cx),
            PublishTarget::Xiaohongshu => self.xhs_side(cx),
        };
        div().size_full().track_focus(&self.focus).key_context("PublishPane").child(side)
    }
}

/// Places the page, and the preview beside or over it, in the detail column: what
/// `AppShellView` does with `previewSheet`. `page` should be sized by the row it's
/// put in; `radius` is the page's corner radius.
pub fn beside_page(page: impl IntoElement, pane: &Entity<PublishPane>, radius: f32, cx: &mut App) -> Div {
    let c = t();
    let (width_cell, appear) = {
        pane.update(cx, |p, _| p.radius = radius);
        let p = pane.read(cx);
        (p.detail_width.clone(), p.appear)
    };
    let layout = sheet_layout(width_cell.get(), detail_min_width(), appear);
    let measured = pane.clone();
    let measure = canvas(
        move |bounds, window, _| {
            let width = f32::from(bounds.size.width);
            if (width - width_cell.get()).abs() > 0.5 {
                width_cell.set(width);
                // It may change which way the sheet sits; next frame draws that.
                window.on_next_frame(move |_, cx| measured.update(cx, |_, cx| cx.notify()));
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full();
    let row = div()
        .relative()
        .size_full()
        .flex()
        .child(measure)
        .child(div().flex_1().min_w_0().h_full().child(page))
        .child(div().flex_none().h_full().w(px(layout.reserved)));
    if appear <= 0. {
        return row;
    }
    row.child(
        div()
            // Clicks here are the pane's, not the page's under it.
            .occlude()
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-layout.offset))
            .w(px(WIDTH))
            .opacity(layout.opacity)
            .rounded(px(radius))
            .border_1()
            .border_color(c.page_edge)
            .bg(c.page_sheet)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., layout.shadow_alpha),
                offset: point(px(0.), px(1.)),
                blur_radius: px(layout.shadow_blur),
                spread_radius: px(0.),
                inset: false,
            }])
            .overflow_hidden()
            .child(pane.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn names_are_a_switch_only_with_both() {
        let (names, switch) = platform_names(true, true, PublishTarget::Wechat);
        assert_eq!(names, [PublishTarget::Wechat, PublishTarget::Xiaohongshu]);
        assert!(switch);
        let (names, switch) = platform_names(false, true, PublishTarget::Xiaohongshu);
        assert_eq!(names, [PublishTarget::Xiaohongshu]);
        assert!(!switch);
    }

    #[core::prelude::v1::test]
    fn hidden_notices_come_back_when_they_change() {
        let all = vec!["a".to_string()];
        let hidden = Some(all.clone());
        assert!(notice_is_hidden(&all, hidden.as_ref()));
        assert!(shown_notices(&all, hidden.as_ref()).is_empty());
        let more = vec!["a".to_string(), "b".to_string()];
        assert!(!notice_is_hidden(&more, hidden.as_ref()));
        assert_eq!(shown_notices(&more, hidden.as_ref()), more);
        assert!(!notice_is_hidden(&[], Some(&Vec::new())));
    }

    #[core::prelude::v1::test]
    fn the_sheet_sits_beside_with_room_and_over_without() {
        // 680 column + 80 + 16 = 776; 776 + 420 + 8 = 1204.
        let beside = sheet_layout(1204., 776., 1.);
        assert!(!beside.overlaps);
        assert_eq!(beside.reserved, 428.);
        assert_eq!((beside.offset, beside.shadow_alpha, beside.shadow_blur), (0., 0.05, 1.5));
        let over = sheet_layout(1203., 776., 1.);
        assert!(over.overlaps);
        assert_eq!(over.reserved, 0.);
        assert_eq!((over.shadow_alpha, over.shadow_blur), (0.12, 16.));
    }

    #[core::prelude::v1::test]
    fn the_sheet_slides_in_from_the_right_with_a_fade() {
        let start = sheet_layout(2000., 776., 0.);
        assert_eq!((start.reserved, start.offset, start.opacity), (0., 428., 0.));
        let half = sheet_layout(2000., 776., 0.5);
        assert_eq!((half.reserved, half.offset, half.opacity), (214., 214., 0.5));
        assert_eq!(eased(0.), 0.);
        assert_eq!(eased(1.), 1.);
        assert_eq!(eased(0.5), 0.5);
        assert!(eased(0.25) < 0.25);
    }

    #[core::prelude::v1::test]
    fn an_edit_back_to_the_notes_own_clears_it() {
        assert_eq!(edited_value("Own", "Own"), None);
        assert_eq!(edited_value("Mine", "Own").as_deref(), Some("Mine"));
    }
}
