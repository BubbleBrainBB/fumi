//! With no folder open: the app's icon, its name, Open Folder… and the folders
//! opened before; when the last folder can't be found, a panel that says so and
//! offers to find it. Port of `FolderOpenView`.

use crate::actions::enter_library;
use crate::labels;
use crate::layout::{Tween, ease_out};
use crate::mac;
use fumi_ui::icons::{Glyph, glyph, symbol};
use fumi_ui::theme::{self, t, text_size};
use fumi_ui::{native, settings};
use fumi_workspace::{Workspace, WorkspaceEvent, now as clock_now};
use gpui::prelude::*;
use gpui::*;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

actions!(fumi_welcome, [OpenDefault]);

pub(crate) const CONTEXT: &str = "FumiWelcome";

/// The folder made by New Notebook, in Documents.
const PRODUCT_NAME: &str = "Fumi";

struct Recent {
    path: PathBuf,
    name: String,
    location: String,
}

fn existing_recents() -> Vec<Recent> {
    settings::recent_workspaces()
        .into_iter()
        .filter(|path| Path::new(path).exists())
        .map(|path| {
            let path = PathBuf::from(path);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let location = path.parent().map(|p| mac::display_name(&p.to_string_lossy())).unwrap_or_default();
            Recent { path, name, location }
        })
        .collect()
}

fn mounted_volumes() -> BTreeSet<String> {
    std::fs::read_dir("/Volumes")
        .map(|dir| dir.filter_map(|entry| entry.ok()).map(|entry| entry.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default()
}

/// New Notebook's folder: made if need be, an error if a file has the name.
pub fn prepare_notebook(documents: &Path) -> Result<PathBuf, String> {
    let folder = documents.join(PRODUCT_NAME);
    match std::fs::metadata(&folder) {
        Ok(meta) if meta.is_dir() => Ok(folder),
        Ok(_) => Err(format!("A file named {PRODUCT_NAME} is already in Documents.")),
        Err(_) => match std::fs::create_dir_all(&folder) {
            Ok(()) => Ok(folder),
            Err(error) => Err(format!("Couldn't create the folder: {error}")),
        },
    }
}

/// How a button fills under the pointer: `text` at 5% over it, 9% pressed, 0.12 s.
#[derive(Default)]
struct HoverFills {
    hovered: Option<SharedString>,
    pressed: Option<SharedString>,
    fills: HashMap<SharedString, Tween>,
}

impl HoverFills {
    fn alpha(&mut self, id: &SharedString, now: Instant) -> (f32, bool) {
        let target = if self.pressed.as_ref() == Some(id) {
            0.09
        } else if self.hovered.as_ref() == Some(id) {
            0.05
        } else {
            0.
        };
        let tween = self.fills.entry(id.clone()).or_insert_with(|| Tween::settled(0.));
        tween.go(target, Duration::from_millis(120), now);
        (tween.value(now), tween.is_moving(now))
    }
}

pub struct Welcome {
    workspace: Entity<Workspace>,
    focus: FocusHandle,
    recents: Vec<Recent>,
    error: Option<String>,
    fills: HoverFills,
    locate_hover: bool,
    locate_nudge: Tween,
    quiet_hover: Option<&'static str>,
    _subs: Vec<Subscription>,
}

impl Welcome {
    pub fn new(workspace: Entity<Workspace>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subs = vec![
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.subscribe(&workspace, |this, _, event: &WorkspaceEvent, cx| {
                if matches!(event, WorkspaceEvent::WorkspaceChanged) {
                    this.recents = existing_recents();
                    this.error = None;
                    cx.notify();
                }
            }),
        ];
        // A folder on a disk that isn't connected opens by itself once the disk
        // mounts: the volume list stands in for `didMountNotification`.
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut volumes = mounted_volumes();
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let now = mounted_volumes();
                if now != volumes {
                    let mounted = now.difference(&volumes).next().is_some();
                    volumes = now;
                    if mounted && this.update(cx, |this, cx| this.volume_mounted(cx)).is_err() {
                        break;
                    }
                }
                if this.upgrade().is_none() {
                    break;
                }
            }
        })
        .detach();
        Welcome {
            workspace,
            focus: cx.focus_handle(),
            recents: existing_recents(),
            error: None,
            fills: HoverFills::default(),
            locate_hover: false,
            locate_nudge: Tween::settled(0.),
            quiet_hover: None,
            _subs: subs,
        }
    }

    fn volume_mounted(&mut self, cx: &mut Context<Self>) {
        let missing = self.workspace.read(cx).store.missing_workspace.clone();
        if let Some(path) = missing
            && path.exists()
        {
            self.open(&path, cx);
        }
    }

    /// Opens a library from this page, handing over to the window: root shows the
    /// plain sheet, and 0.12 s later the page settles into its frame.
    fn open(&mut self, path: &Path, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        workspace.update(cx, |w, cx| {
            w.model.frame_revealed = false;
            cx.notify();
        });
        enter_library(&workspace, path, cx);
        if !workspace.read(cx).store.is_open() {
            workspace.update(cx, |w, cx| {
                w.model.frame_revealed = true;
                cx.notify();
            });
            return;
        }
        let weak = workspace.downgrade();
        cx.spawn(async move |_, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(120)).await;
            let _ = weak.update(cx, |w, cx| {
                w.model.frame_revealed = true;
                cx.notify();
            });
        })
        .detach();
    }

    /// The folder panel, then into what was chosen. In a task: it runs a modal loop.
    fn choose(&mut self, start: Option<PathBuf>, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let Some(path) = native::choose_notes_folder(start.as_deref()) else { return };
            let _ = this.update(cx, |this, cx| this.open(&path, cx));
        })
        .detach();
    }

    fn new_notebook(&mut self, cx: &mut Context<Self>) {
        let documents = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Documents");
        match prepare_notebook(&documents) {
            Ok(folder) => {
                self.open(&folder, cx);
                self.workspace.update(cx, |w, cx| {
                    if let Ok(Some(note)) = w.store.create_note(None, None, clock_now()) {
                        w.model.open_note(Some(&note.id), false);
                        w.library_changed(cx);
                    }
                    w.navigated(cx);
                });
            }
            Err(message) => {
                self.error = Some(message);
                cx.notify();
            }
        }
    }

    fn default_action(&mut self, _: &OpenDefault, _: &mut Window, cx: &mut Context<Self>) {
        let missing = self.workspace.read(cx).store.missing_workspace.clone();
        match missing {
            Some(path) => {
                let on_disk = labels::unmounted_volume(&path.to_string_lossy(), |p| Path::new(p).exists());
                if on_disk.is_none() {
                    self.choose(Some(path), cx);
                }
            }
            None => self.choose(None, cx),
        }
    }

    /// A row or button with the quiet hover fill.
    fn hover_button(&mut self, id: impl Into<SharedString>, now: Instant, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let c = t();
        let id: SharedString = id.into();
        let (alpha, moving) = self.fills.alpha(&id, now);
        if moving {
            window.request_animation_frame();
        }
        let (hover_id, press_id) = (id.clone(), id.clone());
        let mut button = div()
            .id(id)
            .rounded(px(7.))
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                if *hovering {
                    this.fills.hovered = Some(hover_id.clone());
                } else if this.fills.hovered.as_ref() == Some(&hover_id) {
                    this.fills.hovered = None;
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    this.fills.pressed = Some(press_id.clone());
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.fills.pressed = None;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.fills.pressed = None;
                    cx.notify();
                }),
            );
        if alpha > 0. {
            button = button.bg(c.text.opacity(alpha));
        }
        button
    }

    fn welcome_page(&mut self, now: Instant, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let key_cap = |key: &'static str| {
            div()
                .min_w(px(18.))
                .min_h(px(18.))
                .px(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .bg(c.inline_bg)
                .text_size(px(text_size::CAPTION))
                .text_color(c.text3)
                .child(key)
        };
        let open_button = self
            .hover_button("welcome-open", now, window, cx)
            .h(px(28.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .on_click(cx.listener(|this, _, _, cx| this.choose(None, cx)))
            .child(glyph(Glyph::FolderClosed, 16., c.icon))
            .child(div().text_size(px(text_size::BODY)).font_weight(FontWeight::MEDIUM).text_color(c.text).child("Open Folder…"))
            .child(div().ml(px(6.)).flex().gap(px(2.)).child(key_cap("⌘")).child(key_cap("O")));

        let rows: Vec<(PathBuf, String, String)> =
            self.recents.iter().take(5).map(|r| (r.path.clone(), r.name.clone(), r.location.clone())).collect();
        let recent_list = (!rows.is_empty()).then(|| {
            let mut list = div().mt(px(40.)).w(px(320.)).flex().flex_col().gap(px(2.)).child(
                div()
                    .px(px(10.))
                    .pb(px(4.))
                    .text_size(px(text_size::CAPTION))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c.faint_text)
                    .child("Recent"),
            );
            for (path, name, location) in rows {
                let id = format!("recent-{}", path.display());
                let button = self
                    .hover_button(id, now, window, cx)
                    .h(px(30.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .on_click(cx.listener(move |this, _, _, cx| this.open(&path, cx)))
                    .child(glyph(Glyph::FolderClosed, 16., c.icon))
                    .child(div().min_w_0().truncate().text_size(px(text_size::BODY)).text_color(c.text).child(name))
                    .child(div().flex_1().min_w(px(12.)))
                    .child(div().min_w_0().truncate().text_size(px(text_size::CAPTION)).text_color(c.faint_text).child(location));
                list = list.child(button);
            }
            list
        });

        let icon = mac::app_icon().map(|image| img(image).w(px(64.)).h(px(64.)));
        div()
            .pb(px(48.))
            .flex()
            .flex_col()
            .items_center()
            .children(icon)
            .child(
                div()
                    .mt(px(16.))
                    .text_size(px(text_size::TITLE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c.text)
                    .child(PRODUCT_NAME),
            )
            .child(div().mt(px(4.)).text_size(px(text_size::BODY)).text_color(c.text3).child("Your notes, as plain files."))
            .child(div().mt(px(28.)).child(open_button))
            .children(recent_list)
            .children(self.error.clone().map(|error| {
                div().mt(px(20.)).text_size(px(text_size::CAPTION)).text_color(c.text3).child(error)
            }))
    }

    fn quiet_button(&mut self, title: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        let c = t();
        div()
            .id(title)
            .pb(px(4.))
            .text_size(px(text_size::BODY))
            .text_color(if self.quiet_hover == Some(title) { c.text } else { c.text3 })
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                if *hovering {
                    this.quiet_hover = Some(title);
                } else if this.quiet_hover == Some(title) {
                    this.quiet_hover = None;
                }
                cx.notify();
            }))
            .child(title)
    }

    fn missing_panel(&mut self, path: PathBuf, now: Instant, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let text = path.to_string_lossy().into_owned();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let volume = labels::unmounted_volume(&text, |p| Path::new(p).exists());
        let home = std::env::var("HOME").unwrap_or_default();
        let amber = theme::hex(0xE0A93B);

        let locate = volume.is_none().then(|| {
            self.locate_nudge.go(if self.locate_hover { 2. } else { 0. }, Duration::from_millis(150), now);
            if self.locate_nudge.is_moving(now) {
                window.request_animation_frame();
            }
            let nudge = self.locate_nudge.value(now);
            let start = path.clone();
            div()
                .id("locate")
                .flex()
                .flex_col()
                .on_hover(cx.listener(|this, hovering: &bool, _, cx| {
                    this.locate_hover = *hovering;
                    cx.notify();
                }))
                .on_click(cx.listener(move |this, _, _, cx| this.choose(Some(start.clone()), cx)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .pb(px(3.))
                        .child(div().text_size(px(text_size::BODY)).font_weight(FontWeight::MEDIUM).text_color(c.text).child("Locate…"))
                        .child(div().relative().left(px(nudge)).child(symbol("arrow.right", 11., 0.23, c.text))),
                )
                .child(div().h(px(1.)).bg(c.text))
        });

        let open_other = self.quiet_button("Open Another Folder…", cx).on_click(cx.listener(|this, _, _, cx| this.choose(None, cx)));
        let new_notebook = self.quiet_button("New Notebook", cx).on_click(cx.listener(|this, _, _, cx| this.new_notebook(cx)));

        let waiting = volume.is_some().then(|| {
            div()
                .mt(px(14.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .w(px(12.))
                        .h(px(12.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(amber.opacity(0.18))
                        .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(amber)),
                )
                .child(div().text_size(px(text_size::CAPTION)).text_color(c.faint_text).child("Waiting for the disk…"))
        });

        let title_font = fumi_ui::fonts::family_font("New York");

        div()
            .w(px(420.))
            .flex()
            .flex_col()
            .items_start()
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
                    .child(symbol(if volume.is_none() { "questionmark.folder" } else { "externaldrive" }, 19., -0.4, c.icon)),
            )
            .child(
                div()
                    .font(title_font)
                    .text_size(px(text_size::TITLE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c.text)
                    .child(if volume.is_none() {
                        format!("Can’t find “{name}”")
                    } else {
                        format!("“{name}” is on a disk that isn’t connected")
                    }),
            )
            .child(div().mt(px(6.)).text_size(px(text_size::CAPTION)).text_color(c.faint_text).child(labels::breadcrumb(&text, &home)))
            .child(div().mt(px(12.)).text_size(px(text_size::BODY)).text_color(c.text3).child(match &volume {
                Some(disk) => format!("It opens once “{disk}” is connected."),
                None => "It may have been moved, renamed or deleted.".to_string(),
            }))
            .children(waiting)
            .child(
                div()
                    .mt(px(28.))
                    .flex()
                    .items_start()
                    .gap(px(24.))
                    .children(locate)
                    .child(open_other)
                    .child(new_notebook),
            )
            .children(self.error.clone().map(|error| {
                div().mt(px(20.)).text_size(px(text_size::CAPTION)).text_color(c.text3).child(error)
            }))
    }
}

impl Render for Welcome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let now = Instant::now();
        // ↩ belongs to this page while it's up.
        if !self.focus.contains_focused(window, cx) {
            window.focus(&self.focus, cx);
        }
        window.set_window_title("");
        let missing = self.workspace.read(cx).store.missing_workspace.clone();
        let content: AnyElement = match missing {
            Some(path) => self.missing_panel(path, now, window, cx).into_any_element(),
            None => self.welcome_page(now, window, cx).into_any_element(),
        };
        div()
            .id("welcome")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::default_action))
            .size_full()
            .bg(c.page)
            .flex()
            .items_center()
            .justify_center()
            .child(content)
            .with_animation(
                "welcome-fade",
                Animation::new(Duration::from_millis(200)).with_easing(ease_out),
                |el, delta| el.opacity(delta),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn a_notebook_is_made_or_reused() {
        let base = std::env::temp_dir().join(format!("fumi-notebook-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let folder = prepare_notebook(&base).unwrap();
        assert!(folder.is_dir());
        assert_eq!(prepare_notebook(&base).unwrap(), folder, "reused");
        std::fs::remove_dir_all(&folder).unwrap();
        std::fs::write(base.join(PRODUCT_NAME), b"").unwrap();
        assert_eq!(prepare_notebook(&base).unwrap_err(), "A file named Fumi is already in Documents.");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[core::prelude::v1::test]
    fn fills_follow_the_pointer() {
        let id: SharedString = "b".into();
        let mut fills = HoverFills::default();
        let t0 = Instant::now();
        assert_eq!(fills.alpha(&id, t0).0, 0.);
        fills.hovered = Some(id.clone());
        let (alpha, moving) = fills.alpha(&id, t0);
        assert!(moving && alpha == 0.);
        let later = t0 + Duration::from_secs(1);
        assert_eq!(fills.alpha(&id, later), (0.05, false));
        fills.pressed = Some(id.clone());
        fills.alpha(&id, later);
        assert_eq!(fills.alpha(&id, later + Duration::from_secs(1)), (0.09, false));
    }
}
