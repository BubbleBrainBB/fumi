//! No folder open: the app icon, the name, one line, Open Folder… and the folders
//! opened before; or, when the last folder can't be found, a page that says so.
//! And the page of a library with no notes yet. Ports of `FolderOpenView` and
//! `EmptyLibraryPage`. The hand-over to the framed window is the root's: it gets
//! the chosen folder through `on_open`.

use crate::WelcomeDefault;
use crate::anim::{Fades, Transition};
use crate::controls::folder_glyph;
use fumi_ui::fonts::family_font;
use crate::mac;
use fumi_ui::icons::{Glyph, glyph, symbol};
use fumi_ui::native;
use fumi_ui::settings as s;
use fumi_ui::theme::t;
use fumi_workspace::{Workspace, now};
use gpui::prelude::*;
use gpui::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const PRODUCT_NAME: &str = "Fumi";
const VOLUME_POLL: Duration = Duration::from_secs(1);

/// The disk's name when `path` lives on a volume that isn't mounted.
pub fn unmounted_volume(path: &Path, exists: impl Fn(&Path) -> bool) -> Option<String> {
    let parts: Vec<_> = path.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    if parts.len() <= 2 || parts[1] != "Volumes" {
        return None;
    }
    let volume = Path::new("/Volumes").join(&parts[2]);
    (!exists(&volume)).then(|| parts[2].clone())
}

/// "Documents › My Notes", "iCloud Drive › My Notes", "T7 › Notes › My Notes".
pub fn breadcrumb(path: &Path, home: &Path) -> String {
    let split = |p: &Path| -> Vec<String> { p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect() };
    let mut parts = split(path);
    let home = split(home);
    if !home.is_empty() && parts.starts_with(&home) {
        parts.drain(..home.len());
        let icloud = ["Library", "Mobile Documents", "com~apple~CloudDocs"];
        if parts.len() >= 3 && parts[..3].iter().zip(icloud).all(|(a, b)| a == b) {
            parts.splice(..3, ["iCloud Drive".to_string()]);
        }
    } else if parts.len() > 2 && parts[1] == "Volumes" {
        parts.drain(..2);
    } else if !parts.is_empty() {
        parts.remove(0);
    }
    let from = parts.len().saturating_sub(3);
    parts[from..].join(" › ")
}

pub fn missing_title(name: &str, volume: Option<&str>) -> String {
    match volume {
        None => format!("Can’t find “{name}”"),
        Some(_) => format!("“{name}” is on a disk that isn’t connected"),
    }
}

pub fn missing_line(volume: Option<&str>) -> String {
    match volume {
        None => "It may have been moved, renamed or deleted.".to_string(),
        Some(disk) => format!("It opens once “{disk}” is connected."),
    }
}

/// Makes or reuses `<documents>/Fumi`. Errors are the page's own words.
pub fn notebook_folder(documents: &Path) -> Result<PathBuf, String> {
    let folder = documents.join(PRODUCT_NAME);
    if folder.exists() {
        if !folder.is_dir() {
            return Err(format!("A file named {PRODUCT_NAME} is already in Documents."));
        }
    } else if let Err(e) = std::fs::create_dir_all(&folder) {
        return Err(format!("Couldn't create the folder: {e}"));
    }
    Ok(folder)
}

/// What the page shows for the active tab when it holds no note: blank while the
/// first scan runs, the empty-library page when the library has no notes, a draft
/// otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyPage {
    Blank,
    NoNotes,
    Draft,
}

pub fn empty_page(has_loaded: bool, note_count: usize) -> EmptyPage {
    match (has_loaded, note_count) {
        (false, _) => EmptyPage::Blank,
        (true, 0) => EmptyPage::NoNotes,
        _ => EmptyPage::Draft,
    }
}

struct Recent {
    path: PathBuf,
    name: SharedString,
    location: SharedString,
}

fn recent_folders() -> Vec<Recent> {
    s::recent_workspaces()
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .map(|path| Recent {
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into(),
            location: path.parent().map(mac::display_name_at_path).unwrap_or_default().into(),
            path,
        })
        .collect()
}

fn mounted_volumes() -> BTreeSet<String> {
    std::fs::read_dir("/Volumes")
        .map(|dir| dir.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default()
}

thread_local! {
    static ICON: Option<Arc<RenderImage>> = mac::app_icon(64.);
}

pub type OnOpen = Rc<dyn Fn(PathBuf, &mut Window, &mut App)>;

/// The page shown instead of the shell while no library is open.
pub struct WelcomePage {
    workspace: Entity<Workspace>,
    on_open: OnOpen,
    focus: FocusHandle,
    appeared: Transition,
    recents: Vec<Recent>,
    error: Option<String>,
    hover: Fades<SharedString>,
    pressed: Option<SharedString>,
    hovered_quiet: Option<&'static str>,
    locate_hover: Fades<&'static str>,
    volumes: BTreeSet<String>,
    _poll: Task<()>,
    _subs: Vec<Subscription>,
}

impl WelcomePage {
    /// `on_open` opens the library and runs the hand-over: `begin the frame intro`,
    /// then `w.open_workspace(path, cx)`, as Swift's `open(_:)`.
    pub fn new(workspace: Entity<Workspace>, on_open: OnOpen, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut appeared = Transition::new(false, 0.2);
        appeared.set(true, Instant::now());
        let poll = cx.spawn_in(window, async move |this: WeakEntity<Self>, cx| {
            loop {
                cx.background_executor().timer(VOLUME_POLL).await;
                let alive = this.update_in(cx, |this, window, cx| this.check_volumes(window, cx));
                if alive.is_err() {
                    break;
                }
            }
        });
        let subs = vec![cx.observe(&workspace, |this, _, cx| {
            if !this.workspace.read(cx).store.is_open() {
                this.recents = recent_folders();
            }
            cx.notify();
        })];
        WelcomePage {
            workspace,
            on_open,
            focus,
            appeared,
            recents: recent_folders(),
            error: None,
            hover: Fades::new(0.12),
            pressed: None,
            hovered_quiet: None,
            locate_hover: Fades::new(0.15),
            volumes: mounted_volumes(),
            _poll: poll,
            _subs: subs,
        }
    }

    /// So ↩ reaches the page.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    /// The root calls this when the page shows again: it fades in anew, as a new
    /// `FolderOpenView` does, and takes the keyboard.
    pub fn shown_again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.appeared = Transition::new(false, 0.2);
        self.appeared.set(true, Instant::now());
        self.focus(window, cx);
        cx.notify();
    }

    /// A newly mounted disk with the missing folder on it opens it.
    fn check_volumes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let w = self.workspace.read(cx);
        let Some(missing) = w.missing_workspace.clone() else { return };
        if w.store.is_open() {
            return;
        }
        let now_mounted = mounted_volumes();
        let mounted_new = now_mounted.iter().any(|v| !self.volumes.contains(v));
        self.volumes = now_mounted;
        if mounted_new && missing.exists() {
            self.open(missing, window, cx);
        }
    }

    fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        (self.on_open.clone())(path, window, cx);
        cx.notify();
    }

    fn open_panel(&mut self, start: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this: WeakEntity<Self>, cx| {
            let Some(path) = native::choose_notes_folder(start.as_deref()) else { return };
            let _ = this.update_in(cx, |this, window, cx| this.open(path, window, cx));
        })
        .detach();
    }

    /// Makes (or reuses) Documents/Fumi and opens a first, blank note.
    fn new_notebook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(home) = std::env::var_os("HOME") else { return };
        match notebook_folder(&Path::new(&home).join("Documents")) {
            Err(message) => {
                self.error = Some(message);
                cx.notify();
            }
            Ok(folder) => {
                self.open(folder, window, cx);
                self.workspace.update(cx, |w, cx| {
                    let id = w.store.create_note(None, None, now()).ok().flatten().map(|n| n.id);
                    w.model.open_note(id.as_deref(), false);
                    w.library_changed(cx);
                });
            }
        }
    }

    fn default_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.workspace.read(cx).missing_workspace.clone() {
            None => self.open_panel(None, window, cx),
            Some(missing) => {
                if unmounted_volume(&missing, Path::exists).is_none() {
                    self.open_panel(Some(missing), window, cx);
                }
            }
        }
    }

    /// No fill until the pointer is over it, a deeper one while pressed.
    fn hover_fill(
        &self,
        id: SharedString,
        moment: Instant,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let fill = fumi_ui::button::quiet_fill(self.hover.value(&id, moment), self.pressed.as_ref() == Some(&id));
        let (hover_id, press_id) = (id.clone(), id.clone());
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(7.))
            .bg(fill)
            .cursor_pointer()
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                this.hover.set(hover_id.clone(), *hovering, Instant::now());
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.pressed = Some(press_id.clone());
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.pressed = None;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.pressed = None;
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |this, _, window, cx| action(this, window, cx)))
    }

    fn render_welcome(&self, moment: Instant, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let c = t();
        let key_cap = |key: &'static str| {
            div()
                .flex()
                .items_center()
                .justify_center()
                .px(px(4.))
                .min_w(px(18.))
                .min_h(px(18.))
                .rounded(px(4.))
                .bg(c.inline_bg)
                .text_size(px(12.))
                .line_height(px(15.))
                .text_color(c.text3)
                .child(key)
        };
        let icon = ICON.with(Clone::clone);
        let recents = (!self.recents.is_empty()).then(|| {
            let rows: Vec<_> = self
                .recents
                .iter()
                .map(|r| {
                    let path = r.path.clone();
                    self.hover_fill(format!("recent-{}", r.path.display()).into(), moment, cx, move |this, window, cx| {
                        this.open(path.clone(), window, cx);
                    })
                    .gap(px(10.))
                    .px(px(10.))
                    .h(px(30.))
                    .child(folder_glyph(c.icon))
                    .child(div().text_size(px(13.)).text_color(c.text).truncate().min_w_0().child(r.name.clone()))
                    .child(div().flex_1().min_w(px(12.)))
                    .child(div().flex_none().text_size(px(12.)).text_color(c.faint_text).whitespace_nowrap().child(r.location.clone()))
                })
                .collect();
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .w(px(320.))
                .mt(px(40.))
                .child(
                    div()
                        .px(px(10.))
                        .pb(px(4.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c.faint_text)
                        .child("Recent"),
                )
                .children(rows)
        });
        div()
            .flex()
            .flex_col()
            .items_center()
            .pb(px(48.))
            .when_some(icon, |d, icon| d.child(img(icon).w(px(64.)).h(px(64.))))
            .child(
                div()
                    .mt(px(16.))
                    .text_size(px(20.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c.text)
                    .child(PRODUCT_NAME),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(13.))
                    .line_height(px(16.))
                    .text_color(c.text3)
                    .child("Your notes, as plain files."),
            )
            .child(
                div().mt(px(28.)).child(
                    self.hover_fill("open-folder".into(), moment, cx, |this, window, cx| this.open_panel(None, window, cx))
                        .gap(px(6.))
                        .px(px(10.))
                        .h(px(28.))
                        .child(folder_glyph(c.icon))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(c.text)
                                .child("Open Folder…"),
                        )
                        .child(div().flex().gap(px(2.)).ml(px(6.)).child(key_cap("⌘")).child(key_cap("O"))),
                ),
            )
            .children(recents)
            .when_some(self.error.clone(), |d, error| {
                d.child(div().mt(px(20.)).text_size(px(12.)).text_color(c.text3).child(error))
            })
    }

    fn quiet_button(
        &self,
        title: &'static str,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let c = t();
        div()
            .id(title)
            .pb(px(4.))
            .text_size(px(13.))
            .text_color(if self.hovered_quiet == Some(title) { c.text } else { c.text3 })
            .cursor_pointer()
            .whitespace_nowrap()
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                if *hovering {
                    this.hovered_quiet = Some(title);
                } else if this.hovered_quiet == Some(title) {
                    this.hovered_quiet = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, window, cx| action(this, window, cx)))
            .child(title)
    }

    fn render_missing(&self, missing: &Path, moment: Instant, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let c = t();
        let volume = unmounted_volume(missing, Path::exists);
        let name = missing.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let amber = fumi_ui::theme::hex(0xE0A93B);
        let start = missing.to_path_buf();
        let nudge = 2. * self.locate_hover.value(&"locate", moment);
        let locate = volume.is_none().then(|| {
            let start = start.clone();
            div()
                .id("locate")
                .relative()
                .flex()
                .items_center()
                .gap(px(6.))
                .pb(px(3.))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.text)
                .cursor_pointer()
                .whitespace_nowrap()
                .on_hover(cx.listener(|this, hovering: &bool, _, cx| {
                    this.locate_hover.set("locate", *hovering, Instant::now());
                    cx.notify();
                }))
                .on_click(cx.listener(move |this, _, window, cx| this.open_panel(Some(start.clone()), window, cx)))
                .child("Locate…")
                .child(div().ml(px(nudge)).mr(px(2. - nudge)).child(symbol("arrow.right", 11., native::weight::MEDIUM, c.text)))
                .child(div().absolute().left_0().right_0().bottom_0().h(px(1.)).bg(c.text))
        });
        let waiting = volume.is_some().then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .mt(px(14.))
                .child(
                    div()
                        .w(px(12.))
                        .h(px(12.))
                        .mx(px(-3.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(amber.opacity(0.18))
                        .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(amber)),
                )
                .child(div().text_size(px(12.)).text_color(c.faint_text).child("Waiting for the disk…"))
        });
        let other = self.quiet_button("Open Another Folder…", cx, |this, window, cx| this.open_panel(None, window, cx));
        let notebook = self.quiet_button("New Notebook", cx, |this, window, cx| this.new_notebook(window, cx));
        div()
            .flex()
            .flex_col()
            .items_start()
            .w(px(420.))
            .child(
                div()
                    .w(px(44.))
                    .h(px(44.))
                    .mb(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(11.))
                    .bg(c.inline_bg)
                    .child(symbol(
                        if volume.is_none() { "questionmark.folder" } else { "externaldrive" },
                        19.,
                        -0.4,
                        c.icon,
                    )),
            )
            .child(
                div()
                    .font(family_font("New York"))
                    .text_size(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c.text)
                    .child(missing_title(&name, volume.as_deref())),
            )
            .child(div().mt(px(6.)).text_size(px(12.)).text_color(c.faint_text).child(breadcrumb(missing, &home)))
            .child(div().mt(px(12.)).text_size(px(13.)).text_color(c.text3).child(missing_line(volume.as_deref())))
            .children(waiting)
            .child(div().flex().items_start().gap(px(24.)).mt(px(28.)).children(locate).child(other).child(notebook))
            .when_some(self.error.clone(), |d, error| {
                d.child(div().mt(px(20.)).text_size(px(12.)).text_color(c.text3).child(error))
            })
    }
}

impl Render for WelcomePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let moment = Instant::now();
        if self.appeared.is_animating(moment) || self.hover.is_animating(moment) || self.locate_hover.is_animating(moment) {
            window.request_animation_frame();
        }
        let missing = self.workspace.read(cx).missing_workspace.clone();
        let content = match missing {
            Some(missing) => self.render_missing(&missing, moment, cx).into_any_element(),
            None => self.render_welcome(moment, cx).into_any_element(),
        };
        div()
            .id("welcome")
            .key_context("FumiWelcome")
            .track_focus(&self.focus)
            .size_full()
            .bg(c.page)
            .font(family_font("System"))
            .on_action(cx.listener(|this, _: &WelcomeDefault, window, cx| this.default_action(window, cx)))
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(self.appeared.value(moment))
                    .child(content),
            )
    }
}

/// A library with no notes in it yet: a line about the folder and a way to write
/// the first one. The root shows it for `EmptyPage::NoNotes`.
pub struct EmptyLibraryPage {
    workspace: Entity<Workspace>,
    hovering_reveal: bool,
    _sub: Subscription,
}

impl EmptyLibraryPage {
    pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe(&workspace, |_, _, cx| cx.notify());
        EmptyLibraryPage { workspace, hovering_reveal: false, _sub: sub }
    }
}

/// The line about the folder.
pub fn plain_folder_line(name: &str) -> String {
    format!("“{name}” is a plain folder. Write here, or drop .md files into it — they show up on the left.")
}

impl Render for EmptyLibraryPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let path = self.workspace.read(cx).workspace_path();
        let name = path
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "This folder".into());
        let reveal = path.map(|p| {
            div()
                .id("reveal-library")
                .text_size(px(13.))
                .text_color(if self.hovering_reveal { c.text } else { c.text3 })
                .cursor_pointer()
                .on_hover(cx.listener(|this, hovering: &bool, _, cx| {
                    this.hovering_reveal = *hovering;
                    cx.notify();
                }))
                .on_click(move |_, _, _| native::reveal_in_finder(&p))
                .child("Reveal in Finder")
        });
        div().size_full().flex().justify_center().child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .w(px(440.))
                .pt(px(140.))
                .child(
                    div()
                        .font(family_font(&s::editor_font()))
                        .text_size(px(20.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c.text)
                        .child("No notes here yet"),
                )
                .child(
                    div()
                        .mt(px(8.))
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .text_color(c.text3)
                        .child(plain_folder_line(&name)),
                )
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(22.))
                        .mt(px(24.))
                        .font(family_font("System"))
                        .child(
                            div()
                                .id("write-first-note")
                                .relative()
                                .flex()
                                .items_center()
                                .gap(px(7.))
                                .pb(px(3.))
                                .text_size(px(13.))
                                .text_color(c.text)
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.workspace.update(cx, |w, cx| {
                                        let id = w.store.create_note(None, None, now()).ok().flatten().map(|n| n.id);
                                        w.model.open_note(id.as_deref(), false);
                                        w.library_changed(cx);
                                    });
                                }))
                                .child(glyph(Glyph::NewNote, 15., c.text))
                                .child(div().font_weight(FontWeight::MEDIUM).child("Write the First Note"))
                                .child(div().text_color(c.faint_text).child("⌘N"))
                                .child(div().absolute().left_0().right_0().bottom_0().h(px(1.)).bg(c.text)),
                        )
                        .children(reveal),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn volumes_count_as_unmounted_only_under_volumes() {
        let nothing = |_: &Path| false;
        let all = |_: &Path| true;
        assert_eq!(unmounted_volume(Path::new("/Volumes/T7/Notes"), nothing), Some("T7".into()));
        assert_eq!(unmounted_volume(Path::new("/Volumes/T7/Notes"), all), None);
        assert_eq!(unmounted_volume(Path::new("/Users/me/Notes"), nothing), None);
        assert_eq!(unmounted_volume(Path::new("/Volumes"), nothing), None);
    }

    #[core::prelude::v1::test]
    fn breadcrumbs() {
        let home = Path::new("/Users/me");
        assert_eq!(breadcrumb(Path::new("/Users/me/Documents/My Notes"), home), "Documents › My Notes");
        assert_eq!(
            breadcrumb(Path::new("/Users/me/Library/Mobile Documents/com~apple~CloudDocs/My Notes"), home),
            "iCloud Drive › My Notes"
        );
        assert_eq!(breadcrumb(Path::new("/Volumes/T7/Notes/My Notes"), home), "T7 › Notes › My Notes");
        assert_eq!(breadcrumb(Path::new("/Volumes/T7/a/b/c"), home), "a › b › c");
        assert_eq!(breadcrumb(Path::new("/opt/a/b/c/d"), home), "b › c › d");
    }

    #[core::prelude::v1::test]
    fn missing_copy() {
        assert_eq!(missing_title("X", None), "Can’t find “X”");
        assert_eq!(missing_title("X", Some("T7")), "“X” is on a disk that isn’t connected");
        assert_eq!(missing_line(None), "It may have been moved, renamed or deleted.");
        assert_eq!(missing_line(Some("T7")), "It opens once “T7” is connected.");
    }

    #[core::prelude::v1::test]
    fn new_notebook_makes_or_reuses_and_refuses_a_file() {
        let root = std::env::temp_dir().join(format!("fumi-notebook-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let docs = root.join("Documents");
        std::fs::create_dir_all(&docs).unwrap();
        let folder = notebook_folder(&docs).unwrap();
        assert_eq!(folder, docs.join("Fumi"));
        assert!(folder.is_dir());
        assert_eq!(notebook_folder(&docs).unwrap(), folder);
        std::fs::remove_dir(&folder).unwrap();
        std::fs::write(&folder, b"").unwrap();
        assert_eq!(notebook_folder(&docs).unwrap_err(), "A file named Fumi is already in Documents.");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[core::prelude::v1::test]
    fn blank_until_the_first_scan_then_no_notes_or_a_draft() {
        assert_eq!(empty_page(false, 0), EmptyPage::Blank);
        assert_eq!(empty_page(false, 3), EmptyPage::Blank);
        assert_eq!(empty_page(true, 0), EmptyPage::NoNotes);
        assert_eq!(empty_page(true, 3), EmptyPage::Draft);
    }

    #[core::prelude::v1::test]
    fn the_line_names_the_folder() {
        assert_eq!(
            plain_folder_line("Folder"),
            "“Folder” is a plain folder. Write here, or drop .md files into it — they show up on the left."
        );
    }
}
