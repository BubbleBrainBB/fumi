//! Opens this crate's screens in a window, over the library in FUMI_WORKSPACE:
//! ⇧⌘F search, ⌘, Settings, a first image, video or PDF on the page. With no
//! library it shows the welcome page.
use fumi_core::tabs::Content;
use fumi_settings::file_preview::FilePreview;
use fumi_settings::search::{self, SearchPanel};
use fumi_settings::sheet::{self, OnChange, SettingsSheet, migrate_old_settings};
use fumi_settings::welcome::{EmptyLibraryPage, EmptyPage, OnOpen, WelcomePage, empty_page};
use fumi_settings::{ShowSearch, ShowSettings};
use fumi_ui::{native, settings, theme};
use fumi_workspace::Workspace;
use gpui::*;
use std::rc::Rc;

fn apply_theme(window: &mut Window) {
    let dark = matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark);
    theme::apply_from_settings(dark);
    native::set_app_appearance(match settings::appearance_mode() {
        theme::AppearanceMode::Light => Some(false),
        theme::AppearanceMode::Dark => Some(true),
        theme::AppearanceMode::Auto => None,
    });
    window.refresh();
}

struct Demo {
    workspace: Entity<Workspace>,
    search: Entity<SearchPanel>,
    sheet: Entity<SettingsSheet>,
    welcome: Entity<WelcomePage>,
    empty: Entity<EmptyLibraryPage>,
    preview: Option<Entity<FilePreview>>,
    focus: FocusHandle,
    _sub: Subscription,
}

impl Demo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(Workspace::new);
        let on_change: OnChange = Rc::new(|_, window, _| apply_theme(window));
        let ws = workspace.clone();
        let on_open: OnOpen = Rc::new(move |path, _, cx| {
            ws.update(cx, |w, cx| {
                w.model.frame_revealed = false;
                w.open_workspace(&path, cx);
            });
        });
        let search = cx.new(|cx| SearchPanel::new(workspace.clone(), window, cx));
        let sheet = cx.new(|cx| SettingsSheet::new(workspace.clone(), on_change, window, cx));
        let welcome = cx.new(|cx| WelcomePage::new(workspace.clone(), on_open, window, cx));
        let empty = cx.new(|cx| EmptyLibraryPage::new(workspace.clone(), cx));
        let sub = cx.observe_in(&workspace, window, |this, _, window, cx| this.sync_preview(window, cx));
        let focus = cx.focus_handle();
        let mut this = Demo { workspace, search, sheet, welcome, empty, preview: None, focus, _sub: sub };
        this.sync_preview(window, cx);
        this
    }

    /// One preview for the active tab's file, as the root keeps it. Here the first
    /// file in the library opens by itself when nothing else is open.
    fn sync_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let w = self.workspace.read(cx);
        let id = match w.model.active_content() {
            Content::File(id) => Some(id.clone()),
            Content::Draft => w.store.files().first().map(|f| f.id.clone()),
            _ => None,
        };
        let file = id.and_then(|id| w.store.files().iter().find(|f| f.id == id).cloned());
        match file {
            None => self.preview = None,
            Some(file) => {
                if self.preview.as_ref().is_none_or(|p| p.read(cx).file_id != file.id) {
                    let workspace = self.workspace.clone();
                    self.preview = Some(cx.new(|cx| FilePreview::new(workspace, &file, window, cx)));
                }
            }
        }
        cx.notify();
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = theme::t();
        let w = self.workspace.read(cx);
        let root = div()
            .id("demo")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(c.page)
            .text_color(c.text)
            .text_size(px(13.))
            .on_action(cx.listener(|this, _: &ShowSearch, _, cx| search::present(&this.workspace, cx)))
            .on_action(cx.listener(|this, _: &ShowSettings, _, cx| sheet::present(&this.workspace, cx)));
        if !w.store.is_open() {
            return root.child(self.welcome.clone());
        }
        let page: AnyView = match (&self.preview, empty_page(w.store.has_loaded_notes(), w.store.notes().len())) {
            (Some(preview), _) => preview.clone().into(),
            (None, EmptyPage::NoNotes) => self.empty.clone().into(),
            _ => self.empty.clone().into(),
        };
        root.child(div().size_full().pt(px(52.)).child(page)).child(self.search.clone()).child(self.sheet.clone())
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        fumi_ui::text_field::bind_keys(cx);
        fumi_settings::bind_keys(cx);
        migrate_old_settings();
        theme::apply_from_settings(false);
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Demo".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(19.), px(18.))),
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Demo::new(window, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}
