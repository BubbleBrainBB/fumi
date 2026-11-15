//! The "a new version is available" dialog and the download/install state
//! machine. Drawn as a centred, occluding panel like the sidebar's dialogs so
//! it floats above the page and the publish web view.

use crate::{CheckResult, Release, install, net, update};
use fumi_ui::{button, theme::t};
use gpui::prelude::*;
use gpui::*;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Phases of the dialog. `Idle` draws nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Found(Box<Release>),
    Downloading(Box<Release>),
    Verifying(Box<Release>),
    /// Install staged; the app is about to quit and relaunch.
    Installing,
    Error(String),
}

/// What the host (RootView) can ask the prompt to do.
pub enum UpdateAction {
    /// The menu's "Check for Updates…" — check even if auto-check is off.
    CheckNow,
    /// Dismiss without installing.
    Later,
    /// Skip this version from now on.
    Skip,
    /// Download (if needed), verify and install.
    Install,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptEvent {
    /// The user chose to install; the host should flush notes to disk. The
    /// updater has staged the installer and now requests a quit.
    RequestQuit,
}

type OnEvent = Rc<dyn Fn(PromptEvent, &mut App)>;

pub struct UpdatePrompt {
    phase: Phase,
    focus: FocusHandle,
    on_event: OnEvent,
    /// Cached app path for the installer.
    app_path: Option<PathBuf>,
}

impl UpdatePrompt {
    pub fn new(on_event: OnEvent, cx: &mut Context<Self>) -> Self {
        UpdatePrompt {
            phase: Phase::Idle,
            focus: cx.focus_handle(),
            on_event,
            app_path: None,
        }
    }

    pub fn set_app_path(&mut self, path: PathBuf) {
        self.app_path = Some(path);
    }

    /// Show the dialog for an already-known release. Used by the menu path's
    /// verification harness and available to any future entry point.
    pub fn show(&mut self, release: Box<Release>, cx: &mut Context<Self>) {
        self.phase = Phase::Found(release);
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// Begin a check. The network work runs on the background executor; the
    /// result is applied back on this entity.
    pub fn request_check(&mut self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx: &mut AsyncApp| {
            let result = cx.background_spawn(async move { crate::check_now() }).await;
            let _ = weak.update(cx, |this, cx| this.apply_result(result, cx));
        })
        .detach();
    }

    /// Apply a finished check: show the dialog and maybe auto-download.
    fn apply_result(&mut self, result: CheckResult, cx: &mut Context<Self>) {
        match result {
            CheckResult::Available(release) => {
                self.phase = Phase::Found(release);
                cx.notify();
                if update::auto_download() {
                    let release = self.release_of_phase();
                    self.start_download(release, cx);
                }
            }
            CheckResult::UpToDate | CheckResult::Failed(_) => {}
        }
    }

    /// The release carried by whichever active phase has one.
    fn release_of_phase(&self) -> Box<Release> {
        match &self.phase {
            Phase::Found(r) | Phase::Downloading(r) | Phase::Verifying(r) => r.clone(),
            _ => Box::new(Release {
                version: crate::Version::parse("0.0.0").unwrap(),
                build: 0,
                date: String::new(),
                notes: String::new(),
                asset: crate::Asset { url: String::new(), sha256: String::new() },
                min_os: None,
            }),
        }
    }

    pub fn handle(&mut self, action: UpdateAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            UpdateAction::CheckNow => self.request_check(cx),
            UpdateAction::Later => self.close(cx),
            UpdateAction::Skip => {
                if let Phase::Found(release) | Phase::Downloading(release) = &self.phase {
                    update::set_skipped_version(Some(&release.version.to_string()));
                }
                self.close(cx);
            }
            UpdateAction::Install => {
                if let Phase::Found(_) = &self.phase {
                    let release = self.release_of_phase();
                    self.start_download(release, cx);
                }
            }
        }
        let _ = window;
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.phase = Phase::Idle;
        cx.notify();
    }

    fn start_download(&mut self, release: Box<Release>, cx: &mut Context<Self>) {
        self.phase = Phase::Downloading(release.clone());
        cx.notify();
        let weak = cx.entity().downgrade();
        let url = release.asset.url.clone();
        let expected = release.asset.sha256.clone();
        cx.spawn(async move |_, cx: &mut AsyncApp| {
            let download = cx
                .background_spawn(async move {
                    let dir = install::work_dir();
                    std::fs::create_dir_all(&dir).map_err(|e| net::NetError::Io(e.to_string()))?;
                    let file = dir.join("update.dmg");
                    let _ = std::fs::remove_file(&file);
                    net::download(&url, &file)?;
                    let digest = net::sha256_file(&file)?;
                    if !net::digest_matches(&digest, &expected) {
                        let _ = std::fs::remove_file(&file);
                        return Err(net::NetError::Io("SHA-256 mismatch; download discarded".into()));
                    }
                    Ok::<PathBuf, net::NetError>(file)
                })
                .await;
            let _ = weak.update(cx, |this, cx| match download {
                Ok(file) => {
                    this.phase = Phase::Verifying(release.clone());
                    cx.notify();
                    this.finish_install(file, cx);
                }
                Err(e) => {
                    this.phase = Phase::Error(e.to_string());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Stage the detached installer and ask the host to flush + quit.
    fn finish_install(&mut self, dmg: PathBuf, cx: &mut Context<Self>) {
        let app = self.app_path.clone().or_else(|| {
            let exe = std::env::current_exe().ok()?;
            let mut p = exe.as_path();
            while p.file_name().and_then(|n| n.to_str()) != Some("Fumi.app") {
                p = p.parent()?;
            }
            Some(p.to_path_buf())
        });
        let Some(app) = app else {
            self.phase = Phase::Error("couldn't locate the running app bundle".into());
            cx.notify();
            return;
        };
        match install::stage(&dmg, &app) {
            Ok(_) => {
                self.phase = Phase::Installing;
                let on_event = self.on_event.clone();
                (on_event)(PromptEvent::RequestQuit, cx);
            }
            Err(e) => self.phase = Phase::Error(e),
        }
        cx.notify();
    }

    fn render_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if matches!(self.phase, Phase::Idle) {
            return div().into_any_element();
        }
        let c = t();
        let size = window.viewport_size();
        let (title, body, buttons): (String, AnyElement, AnyElement) = match &self.phase {
            Phase::Found(release) => {
                let version = release.version.to_string();
                let title = format!("A new version of Fumi is available — {version}");
                let notes = if release.notes.trim().is_empty() {
                    "No release notes were provided.".to_string()
                } else {
                    release.notes.clone()
                };
                let body = div()
                    .text_size(px(12.))
                    .line_height(px(17.))
                    .text_color(c.text2)
                    .child(notes)
                    .into_any_element();
                let buttons = div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button::bordered("update-skip")
                            .flex_1().text_size(px(13.)).child("Skip This Version")
                            .on_click(cx.listener(|this, _, window, cx| this.handle(UpdateAction::Skip, window, cx))),
                    )
                    .child(
                        button::bordered("update-later")
                            .flex_1().text_size(px(13.)).child("Later")
                            .on_click(cx.listener(|this, _, window, cx| this.handle(UpdateAction::Later, window, cx))),
                    )
                    .child(
                        button::prominent("update-install")
                            .flex_1().text_size(px(13.)).child("Download & Install")
                            .on_click(cx.listener(|this, _, window, cx| this.handle(UpdateAction::Install, window, cx))),
                    )
                    .into_any_element();
                (title, body, buttons)
            }
            Phase::Downloading(_) => {
                let body = div().text_size(px(12.)).text_color(c.text2).child("Fetching the signed disk image.").into_any_element();
                let buttons = div().flex().gap(px(8.)).child(
                    button::bordered("update-cancel").flex_1().text_size(px(13.)).child("Cancel")
                        .on_click(cx.listener(|this, _, window, cx| this.handle(UpdateAction::Later, window, cx))),
                ).into_any_element();
                ("Downloading update…".to_string(), body, buttons)
            }
            Phase::Verifying(_) => {
                let body = div().text_size(px(12.)).text_color(c.text2).child("Checking the signature and preparing the update.").into_any_element();
                ("Verifying and installing…".to_string(), body, div().into_any_element())
            }
            Phase::Installing => {
                let body = div().text_size(px(12.)).text_color(c.text2).child("Fumi will quit and relaunch to finish installing.").into_any_element();
                ("Update ready".to_string(), body, div().into_any_element())
            }
            Phase::Error(msg) => {
                let body = div().text_size(px(12.)).line_height(px(17.)).text_color(c.text2).child(msg.clone()).into_any_element();
                let buttons = div().flex().gap(px(8.)).child(
                    button::prominent("update-error-ok").flex_1().text_size(px(13.)).child("OK")
                        .on_click(cx.listener(|this, _, window, cx| this.handle(UpdateAction::Later, window, cx))),
                ).into_any_element();
                ("Couldn't update".to_string(), body, buttons)
            }
            Phase::Idle => unreachable!(),
        };

        let panel = div()
            .id("update-dialog")
            .w(px(380.))
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .rounded(px(12.))
            .bg(c.page)
            .border_1()
            .border_color(c.page_edge)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.25),
                offset: point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
                inset: false,
            }])
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    this.handle(UpdateAction::Later, window, cx);
                }
            }))
            .child(
                div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(c.text)
                    .child(SharedString::from(title)),
            )
            .child(body)
            .child(buttons);

        deferred(
            anchored().position(point(px(0.), px(0.))).child(
                div().id("update-backdrop").w(size.width).h(size.height).occlude()
                    .flex().items_center().justify_center().child(panel),
            ),
        )
        .with_priority(4)
        .into_any_element()
    }
}

impl Render for UpdatePrompt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_dialog(window, cx)
    }
}

/// How often a background check runs when auto-check is on.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
