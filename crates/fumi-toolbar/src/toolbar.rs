//! The band over the page: the title or the tab strip at the leading end, the
//! note's icons at the trailing end. Port of `NoteToolbar` and `NoteTitleHeader`.
//! Drawn entirely in GPUI: `macOS 26`'s look on every system.

use crate::actions::{extension_wechat, extension_xhs};
use crate::labels::{self, Moment};
use crate::layout::{self, BAND_HEIGHT, Tween};
use crate::mac;
use crate::menu_rows::{self, FileCommand, MenuSpec, NoteCommand, TabCommand};
use crate::menus::MenuState;
use crate::shell;
use crate::tip::tip;
use cocoa::base::id;
use fumi_core::app_model::PublishTarget;
use fumi_core::attachment_import;
use fumi_core::tabs::{Content, NoteTab};
use fumi_ui::icons::{Glyph, glyph, symbol};
use fumi_ui::theme::{t, text_size};
use fumi_ui::{native, settings};
use fumi_workspace::{Workspace, libraries, local_utc_offset, now as clock_now};
use gpui::prelude::*;
use gpui::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// The open note's text as typed, for the Info popover's counts. Root provides it,
/// since this crate doesn't know the editor.
pub type LiveText = Rc<dyn Fn(&App) -> Option<String>>;

/// What the toolbar asks root to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolbarEvent {
    /// The active tab's preview opened or closed: see `model.is_preview_open()`.
    PreviewToggled,
    /// Export this note as a PDF, as it stands on screen.
    ExportPdf(String),
    CopyForWeChat,
    AddXhsCards,
    ExportAllNotes,
}

impl EventEmitter<ToolbarEvent> for Toolbar {}

/// Where the preview publishes to: with both extensions on, each note keeps the
/// one it was last switched to.
pub fn publish_target(note_id: Option<&str>) -> PublishTarget {
    const KEY: &str = "fumi.publish.targets";
    choose_target(extension_wechat(), extension_xhs(), |key| settings::get_dict_string(KEY, key), note_id)
}

fn choose_target(
    wechat: bool,
    xhs: bool,
    saved: impl Fn(&str) -> Option<String>,
    note_id: Option<&str>,
) -> PublishTarget {
    match (wechat, xhs) {
        (true, false) => PublishTarget::Wechat,
        (false, true) => PublishTarget::Xiaohongshu,
        _ => {
            let chosen = note_id.and_then(&saved).or_else(|| saved(""));
            if chosen.as_deref() == Some("xiaohongshu") { PublishTarget::Xiaohongshu } else { PublishTarget::Wechat }
        }
    }
}

/// What the page under the band is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageKind {
    Note(String),
    /// A blank page not yet on disk.
    Draft,
    /// No note and no draft: a library with no notes, or one still loading.
    Empty,
    Memos,
    File(String),
}

/// As `EditorContainerView` decides it.
pub fn page_kind(content: &Content, note_found: bool, file_found: bool, loaded: bool, has_notes: bool) -> PageKind {
    match content {
        Content::Note(id) if note_found => PageKind::Note(id.clone()),
        Content::Note(_) => PageKind::Empty,
        Content::Draft if loaded && has_notes => PageKind::Draft,
        Content::Draft => PageKind::Empty,
        Content::Memos => PageKind::Memos,
        Content::File(id) if file_found => PageKind::File(id.clone()),
        Content::File(_) => PageKind::Empty,
    }
}

pub(crate) fn moment(secs: f64) -> Moment {
    Moment { secs, utc_offset: local_utc_offset(secs as i64) }
}

pub(crate) fn edited(updated_at: f64) -> String {
    labels::edited_label(moment(updated_at), moment(clock_now().secs), &mac::short_time(updated_at))
}

/// The face of the page's body, for titles.
pub(crate) fn body_font() -> Font {
    fumi_ui::fonts::family_font(&settings::editor_font())
}

struct InfoCard {
    words: usize,
    chars: usize,
    created: String,
    updated: String,
    tags: Vec<String>,
}

pub struct Toolbar {
    pub workspace: Entity<Workspace>,
    live_text: Option<LiveText>,
    /// The band's place in the window, read as it's laid out.
    pub(crate) band: Rc<Cell<Bounds<Pixels>>>,
    /// The page's body face, read once a frame.
    pub(crate) title_font: Font,
    pub(crate) hovered_tab: Option<String>,
    pub(crate) hovered_close: Option<String>,
    pub(crate) motion: layout::TabMotion,
    pub(crate) scroll: ScrollHandle,
    pub(crate) scroll_tween: Option<Tween>,
    pub(crate) last_active: Option<String>,
    pub(crate) drag_last: Option<String>,
    pub(crate) tab_titles: HashMap<Content, (String, Option<fumi_core::note::FileKind>)>,
    pub(crate) titles_revision: u64,
    moving_window: bool,
    /// What `NSWindow.subtitle` says now.
    window_subtitle: String,
    info: Option<InfoCard>,
    info_focus: FocusHandle,
    info_return_focus: Option<FocusHandle>,
    info_tab: Option<String>,
    file_size: Option<(String, f64, Option<String>)>,
    menu_state: Option<MenuState>,
    _subs: Vec<Subscription>,
}

impl Toolbar {
    pub fn new(workspace: Entity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        shell::attach(window, &workspace, cx);
        let subs = vec![
            cx.observe_in(&workspace, window, |this, _, window, cx| {
                this.sync_menus(window, cx);
                cx.notify();
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                // The bar speaks for the focused window: take it over.
                this.menu_state = None;
                this.sync_menus(window, cx);
                cx.notify();
            }),
        ];
        let size = window.bounds().size;
        Toolbar {
            workspace,
            live_text: None,
            band: Rc::new(Cell::new(Bounds::new(point(px(0.), px(0.)), size))),
            title_font: body_font(),
            hovered_tab: None,
            hovered_close: None,
            motion: layout::TabMotion::default(),
            scroll: ScrollHandle::new(),
            scroll_tween: None,
            last_active: None,
            drag_last: None,
            tab_titles: HashMap::new(),
            titles_revision: 0,
            moving_window: false,
            window_subtitle: String::new(),
            info: None,
            info_focus: cx.focus_handle(),
            info_return_focus: None,
            info_tab: None,
            file_size: None,
            menu_state: None,
            _subs: subs,
        }
    }

    /// The bar's words follow this window while it's the focused one.
    fn sync_menus(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !window.is_window_active() {
            return;
        }
        let state = Self::menu_state_of(self.workspace.read(cx));
        if self.menu_state.as_ref() != Some(&state) {
            self.menu_state = Some(state.clone());
            cx.defer(move |cx| shell::sync_menus(state, cx));
        }
    }

    /// Where Info reads the note's live text. Without it: the text last saved.
    pub fn set_live_text(&mut self, live_text: LiveText) {
        self.live_text = Some(live_text);
    }

    pub(crate) fn nav(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Workspace)) {
        self.workspace.update(cx, |w, cx| {
            f(w);
            w.navigated(cx);
        });
    }

    /// Shows a native menu from a task, see `native::popup_menu`, and runs what's chosen.
    pub(crate) fn popup<C: 'static>(
        &self,
        menu: MenuSpec<C>,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
        run: impl FnOnce(&mut Self, C, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let ns_window = native::ns_window_of(window) as usize;
        let handle = window.window_handle();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let chosen = native::popup_menu_on(ns_window as id, &menu.rows, position);
            let Some(command) = chosen.and_then(|index| menu.take(index)) else { return };
            let _ = cx.update_window(handle, |_, window, cx| {
                let _ = this.update(cx, |this, cx| run(this, command, window, cx));
            });
        })
        .detach();
    }

    fn file_of(&self, tab: &NoteTab, cx: &App) -> Option<PathBuf> {
        let w = self.workspace.read(cx);
        match &tab.content {
            Content::Note(id) => w.note(id).map(|note| note.path.clone()),
            Content::File(id) => w.store.files().iter().find(|file| file.id == *id).map(|file| file.path.clone()),
            _ => None,
        }
    }

    /// The right-click menu for a tab.
    pub(crate) fn open_tab_menu(
        &mut self,
        tab: NoteTab,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let file = self.file_of(&tab, cx);
        let tabs = self.workspace.read(cx).model.tabs().to_vec();
        let menu = menu_rows::tab_menu(&tab, &tabs, file.as_deref());
        self.popup(menu, position, window, cx, |this, command, _, cx| this.run_tab_command(command, cx));
    }

    fn run_tab_command(&mut self, command: TabCommand, cx: &mut Context<Self>) {
        match command {
            TabCommand::TogglePin(id) => self.nav(cx, |w| w.model.toggle_pin(&id)),
            TabCommand::Close(id) => self.nav(cx, |w| w.model.close_tab(&id)),
            TabCommand::CloseOthers(id) => self.nav(cx, |w| w.model.close_other_tabs(&id)),
            TabCommand::CloseRight(id) => self.nav(cx, |w| w.model.close_tabs_to_the_right(&id)),
            TabCommand::RevealInSidebar(id) => self.nav(cx, |w| {
                w.model.select_tab(&id);
                w.model.sidebar_visible = true;
            }),
            TabCommand::RevealInFinder(path) => native::reveal_in_finder(&path),
        }
    }

    fn open_note_menu(&mut self, id: String, position: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let w = self.workspace.read(cx);
        let Some(note) = w.note(&id) else { return };
        let menu = menu_rows::note_menu(note.pinned, extension_wechat(), extension_xhs(), w.model.is_preview_open());
        self.popup(menu, position, window, cx, move |this, command, _, cx| this.run_note_command(&id, command, cx));
    }

    fn run_note_command(&mut self, id: &str, command: NoteCommand, cx: &mut Context<Self>) {
        match command {
            NoteCommand::TogglePin => self.workspace.update(cx, |w, cx| {
                w.store.toggle_pin(id);
                w.library_changed(cx);
            }),
            NoteCommand::RevealInFinder => {
                if let Some(note) = self.workspace.read(cx).note(id) {
                    native::reveal_in_finder(&note.path);
                }
            }
            NoteCommand::ExportPdf => cx.emit(ToolbarEvent::ExportPdf(id.to_string())),
            NoteCommand::TogglePreview => self.toggle_preview(cx),
            NoteCommand::CopyForWeChat => cx.emit(ToolbarEvent::CopyForWeChat),
            NoteCommand::AddXhsCards => cx.emit(ToolbarEvent::AddXhsCards),
            NoteCommand::Delete => self.workspace.update(cx, |w, cx| {
                w.model.remove_tabs(&Content::Note(id.to_string()));
                let _ = w.store.delete_note(id);
                w.library_changed(cx);
            }),
        }
    }

    fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        self.nav(cx, |w| w.model.toggle_preview());
        cx.emit(ToolbarEvent::PreviewToggled);
    }

    fn open_file_menu(&mut self, id: String, position: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        self.popup(menu_rows::file_menu(), position, window, cx, move |this, command, _, cx| {
            this.run_file_command(&id, command, cx)
        });
    }

    fn run_file_command(&mut self, id: &str, command: FileCommand, cx: &mut Context<Self>) {
        let w = self.workspace.read(cx);
        let Some(path) = w.store.files().iter().find(|file| file.id == id).map(|file| file.path.clone()) else {
            return;
        };
        match command {
            FileCommand::CopyMarkdownLink => {
                if let Some(library) = w.workspace_path() {
                    let relative = attachment_import::relative_path(&library, &path);
                    cx.write_to_clipboard(ClipboardItem::new_string(attachment_import::markdown_for_path(&relative)));
                }
            }
            FileCommand::OpenInDefaultApp => cx.open_with_system(&path),
            FileCommand::RevealInFinder => native::reveal_in_finder(&path),
            FileCommand::MoveToTrash => {
                if native::trash(&path) {
                    let id = id.to_string();
                    self.workspace.update(cx, |w, cx| {
                        w.model.remove_tabs(&Content::File(id));
                        w.reload(cx);
                        w.navigated(cx);
                    });
                } else {
                    native::beep();
                }
            }
        }
    }

    fn toggle_info(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.info.is_some() {
            self.close_info(window, cx);
            return;
        }
        let w = self.workspace.read(cx);
        let Some(note) = w.note(id) else { return };
        // The numbers are read once, as the card opens: nothing can type under it.
        let text = self.live_text.as_ref().and_then(|live| live(cx)).unwrap_or_else(|| note.content.clone());
        self.info = Some(InfoCard {
            words: labels::word_count(&text),
            chars: labels::char_count(&text),
            created: labels::created_label(moment(note.created_at)),
            updated: labels::relative_named(moment(note.updated_at), moment(clock_now().secs)),
            tags: note.tags.clone(),
        });
        self.info_tab = Some(w.model.active_id().to_string());
        self.info_return_focus = window.focused(cx);
        window.focus(&self.info_focus, cx);
        self.set_info_presented(true, cx);
        cx.notify();
    }

    fn close_info(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.info = None;
        self.info_tab = None;
        if let Some(focus) = self.info_return_focus.take() {
            window.focus(&focus, cx);
        }
        self.set_info_presented(false, cx);
        cx.notify();
    }

    /// GPUI can't draw over the preview sheet's web view, so the sheet trades it
    /// for a still while the card is up.
    fn set_info_presented(&self, presented: bool, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            if w.model.info_presented != presented {
                w.model.info_presented = presented;
                cx.notify();
            }
        });
    }

    /// The trailing icon's frame in the window, `index` 0 the last.
    fn icon_cell(&self, index: usize) -> Bounds<Pixels> {
        let band = self.band.get();
        let x = layout::icon_cell_x(f32::from(band.right()), index);
        Bounds::new(point(px(x), band.top()), size(px(layout::ICON_CELL), px(BAND_HEIGHT)))
    }

    fn band_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Clicking the band doesn't take the keyboard from the page.
        window.prevent_default();
        cx.stop_propagation();
        if event.click_count == 2 {
            window.titlebar_double_click();
        } else {
            self.moving_window = true;
        }
    }

    fn icon_button(
        &self,
        id: &'static str,
        icon: Glyph,
        ink: Hsla,
        help: String,
    ) -> Stateful<Div> {
        let c = t();
        let hover = if c.glass { c.page.opacity(0.75) } else { c.text.opacity(0.05) };
        let press = c.text.opacity(0.09);
        let mut button = div()
            .id(id)
            .w(px(36.))
            .h(px(32.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(16.))
            .hover(move |s| s.bg(hover))
            .active(move |s| s.bg(press))
            .tooltip(tip(help))
            .child(glyph(icon, 18., ink));
        if c.glass {
            // Stands in for macOS 26's glass capsule: a translucent fill and a hairline.
            button = button.bg(c.page_sheet).border_1().border_color(c.page_edge);
        }
        button
    }

    fn title_block(
        &self,
        tab: &NoteTab,
        title: String,
        subtitle: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let c = t();
        let right_click = {
            let tab = tab.clone();
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_tab_menu(tab.clone(), event.position, window, cx);
            })
        };
        let control_click = {
            let tab = tab.clone();
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if event.modifiers.control {
                    cx.stop_propagation();
                    window.prevent_default();
                    this.open_tab_menu(tab.clone(), event.position, window, cx);
                }
            })
        };
        div()
            .id("note-title")
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(1.))
            .on_mouse_down(MouseButton::Right, right_click)
            .on_mouse_down(MouseButton::Left, control_click)
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap(px(5.))
                    .when(tab.is_pinned, |row| {
                        row.child(
                            div()
                                .id("lone-pin")
                                .flex_none()
                                .tooltip(tip("Pinned"))
                                .child(symbol("pin.fill", 9., 0.3, c.faint_text)),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .font(self.title_font.clone())
                            .text_size(px(text_size::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c.text2)
                            .truncate()
                            .child(title),
                    ),
            )
            .when(!subtitle.is_empty(), |block| {
                block.child(
                    div()
                        .text_size(px(text_size::CAPTION))
                        .text_color(c.faint_text)
                        .truncate()
                        .child(subtitle),
                )
            })
    }

    fn info_popover(&self, card: &InfoCard, cx: &mut Context<Self>) -> impl IntoElement {
        const WIDTH: f32 = 240.;
        const ARROW: f32 = 8.;
        let c = t();
        let index = 1;
        let cell = self.icon_cell(index);
        let center = f32::from(cell.center().x);
        // The band runs to the window's right edge.
        let window_width = f32::from(self.band.get().right());
        let (left, arrow_x) = layout::popover_x(center, WIDTH, window_width, 8.);
        let top = f32::from(cell.top()) + BAND_HEIGHT / 2. + 16. + 2.;
        let stat = |label: &'static str, value: String| {
            div()
                .flex()
                .justify_between()
                .gap(px(8.))
                .text_size(px(text_size::BODY))
                .child(div().flex_none().text_color(c.text2).child(label))
                .child(div().min_w_0().truncate().font_weight(FontWeight::MEDIUM).text_color(c.text).child(value))
        };
        let close_out = cx.listener(move |this, event: &MouseDownEvent, window, cx| {
            // A click on the button itself is its own toggle.
            if !this.icon_cell(index).contains(&event.position) {
                this.close_info(window, cx);
            }
        });
        let on_key = cx.listener(|this, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                this.close_info(window, cx);
                cx.stop_propagation();
            }
        });
        let arrow = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let mut path = PathBuilder::fill();
                let (x, y) = (bounds.origin.x, bounds.origin.y);
                path.move_to(point(x, y + px(ARROW)));
                path.line_to(point(x + px(ARROW), y));
                path.line_to(point(x + px(ARROW * 2.), y + px(ARROW)));
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, c.page);
                }
            },
        )
        .absolute()
        .left(px(arrow_x - ARROW))
        .top(px(-ARROW))
        .w(px(ARROW * 2.))
        .h(px(ARROW));

        let mut body = div()
            .id("note-info")
            .track_focus(&self.info_focus)
            .relative()
            .w(px(WIDTH))
            .bg(c.page)
            .rounded(px(10.))
            .shadow(vec![
                BoxShadow { color: hsla(0., 0., 0., 0.16), offset: point(px(0.), px(8.)), blur_radius: px(24.), spread_radius: px(0.), inset: false },
                BoxShadow { color: hsla(0., 0., 0., 0.10), offset: point(px(0.), px(0.)), blur_radius: px(1.), spread_radius: px(0.5), inset: false },
            ])
            .occlude()
            .on_mouse_down_out(close_out)
            .on_key_down(on_key)
            .child(arrow)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .px(px(16.))
                    .pt(px(16.))
                    .pb(px(14.))
                    .child(stat("Words", card.words.to_string()))
                    .child(stat("Characters", card.chars.to_string()))
                    .child(stat("Reading time", labels::reading_time(card.words)))
                    .child(stat("Created", card.created.clone()))
                    .child(stat("Updated", card.updated.clone())),
            );
        if !card.tags.is_empty() {
            let accent = c.accent;
            body = body
                .child(div().mx(px(20.)).h(px(1.)).bg(c.text.opacity(0.06)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .px(px(16.))
                        .py(px(14.))
                        .child(
                            div()
                                .text_size(px(text_size::CAPTION))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(c.text3)
                                .child(tracked("TAGS", 0.4)),
                        )
                        .child(div().flex().flex_wrap().gap(px(6.)).children(card.tags.iter().map(|tag| {
                            div()
                                .px(px(8.))
                                .py(px(3.))
                                .rounded_full()
                                .bg(accent.opacity(0.08))
                                .text_size(px(text_size::CAPTION))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(accent)
                                .child(format!("#{tag}"))
                        }))),
                );
        }
        deferred(anchored().position(point(px(left), px(top + ARROW))).child(body)).with_priority(1)
    }

    fn menu_state_of(w: &Workspace) -> MenuState {
        let wechat = extension_wechat();
        let xhs = extension_xhs();
        MenuState {
            any_extension: wechat || xhs,
            preview_open: w.model.is_preview_open(),
            exports_cards: xhs && publish_target(w.model.selected_note()) == PublishTarget::Xiaohongshu,
            tab_pinned: w.model.active_tab().is_pinned,
            libraries: libraries::with(|list| list.entries().iter().take(9).map(|e| e.display_name()).collect()),
        }
    }
}

/// Letters spaced apart. GPUI has no tracking, so each glyph takes its own box:
/// only ever used on the short label it's meant for.
fn tracked(text: &'static str, tracking: f32) -> impl IntoElement {
    div().flex().gap(px(tracking)).children(text.chars().map(|ch| div().child(ch.to_string())))
}

impl Render for Toolbar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let (wechat, xhs) = (extension_wechat(), extension_xhs());
        let any_extension = wechat || xhs;
        self.title_font = body_font();

        let (tab, kind, strip, title, subtitle, preview_open) = {
            let w = self.workspace.read(cx);
            let tab = w.model.active_tab().clone();
            let (note_found, file) = match &tab.content {
                Content::Note(id) => (w.note(id).is_some(), None),
                Content::File(id) => (false, w.store.files().iter().find(|file| file.id == *id)),
                _ => (false, None),
            };
            let kind = page_kind(&tab.content, note_found, file.is_some(), w.store.has_loaded_notes(), !w.store.notes().is_empty());
            let (title, subtitle) = match &kind {
                PageKind::Note(id) => match w.note(id) {
                    Some(note) => (note.filename(), labels::note_subtitle(&note.relative_path, &edited(note.updated_at))),
                    None => (String::new(), String::new()),
                },
                PageKind::Draft => ("New Note".to_string(), String::new()),
                PageKind::Empty => ("Notes".to_string(), String::new()),
                PageKind::Memos => ("Memos".to_string(), String::new()),
                PageKind::File(_) => match file {
                    Some(file) => {
                        let stale = self
                            .file_size
                            .as_ref()
                            .is_none_or(|(id, updated, _)| *id != file.id || *updated != file.updated_at);
                        if stale {
                            let size = std::fs::metadata(&file.path).ok().map(|meta| mac::file_size_label(meta.len()));
                            self.file_size = Some((file.id.clone(), file.updated_at, size));
                        }
                        let size = self.file_size.as_ref().and_then(|(_, _, size)| size.as_deref());
                        (file.name(), labels::file_subtitle(&file.relative_path, size))
                    }
                    None => (String::new(), String::new()),
                },
            };
            (
                tab,
                kind,
                w.model.shows_tab_strip(),
                title,
                subtitle,
                w.model.is_preview_open(),
            )
        };

        window.set_window_title(&title);
        if self.window_subtitle != subtitle {
            native::set_window_subtitle(native::ns_window_of(window), &subtitle);
            self.window_subtitle = subtitle.clone();
        }
        // Settings change the extensions without a word to the workspace.
        self.sync_menus(window, cx);

        // A popover belongs to the page it opened over.
        if self.info.is_some() && self.info_tab.as_deref() != Some(tab.id.as_str()) {
            self.close_info(window, cx);
        }

        let band = self.band.get();
        let band_left = f32::from(band.left());
        let strip_width = layout::strip_width(band_left, f32::from(band.size.width), any_extension);
        let lead = layout::leading_x(band_left + layout::PAGE_INSET) - band_left;

        let leading: AnyElement = if strip {
            self.render_strip(strip_width, window, cx)
        } else {
            self.motion.reset();
            self.last_active = None;
            self.hovered_tab = None;
            self.hovered_close = None;
            let block = (!title.is_empty()).then(|| self.title_block(&tab, title, subtitle, cx));
            div()
                .h_full()
                .flex()
                .items_center()
                .pl(px(6.))
                .max_w(px(566.).min(px(strip_width)))
                .children(block)
                .into_any_element()
        };

        let ink = c.text;
        let mut icons: Vec<AnyElement> = Vec::new();
        let cell = |button: AnyElement| {
            div().w(px(layout::ICON_CELL)).h_full().flex().items_center().justify_center().child(button).into_any_element()
        };
        let stop = |_: &MouseDownEvent, window: &mut Window, cx: &mut App| {
            window.prevent_default();
            cx.stop_propagation();
        };
        if matches!(kind, PageKind::Note(_)) && any_extension {
            let preview_ink = if preview_open { c.accent } else { ink };
            let button = self
                .icon_button("toolbar-preview", Glyph::Phone, preview_ink, labels::preview_tip(wechat, xhs, preview_open))
                .on_mouse_down(MouseButton::Left, stop)
                .on_click(cx.listener(|this, _, _, cx| this.toggle_preview(cx)));
            icons.push(cell(button.into_any_element()));
        }
        if !matches!(kind, PageKind::Memos) {
            let button = self
                .icon_button("toolbar-new-note", Glyph::NewNote, ink, "New Note (⌘N)".to_string())
                .on_mouse_down(MouseButton::Left, stop)
                .on_click(cx.listener(|this, _, _, cx| this.nav(cx, |w| w.model.new_tab())));
            icons.push(cell(button.into_any_element()));
        }
        if let PageKind::Note(id) = &kind {
            let info_id = id.clone();
            let button = self
                .icon_button("toolbar-info", Glyph::Info, ink, "Note Info".to_string())
                .on_mouse_down(MouseButton::Left, stop)
                .on_click(cx.listener(move |this, _, window, cx| this.toggle_info(&info_id, window, cx)));
            icons.push(cell(button.into_any_element()));
            let more_id = id.clone();
            let button = self
                .icon_button("toolbar-more", Glyph::More, ink, "More".to_string())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let cell = this.icon_cell(0);
                        let position = point(cell.left(), cell.top() + px(BAND_HEIGHT / 2. + 18.));
                        this.open_note_menu(more_id.clone(), position, window, cx);
                    }),
                );
            icons.push(cell(button.into_any_element()));
        } else if let PageKind::File(id) = &kind {
            let more_id = id.clone();
            let button = self
                .icon_button("toolbar-more", Glyph::More, ink, "More".to_string())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        let cell = this.icon_cell(0);
                        let position = point(cell.left(), cell.top() + px(BAND_HEIGHT / 2. + 18.));
                        this.open_file_menu(more_id.clone(), position, window, cx);
                    }),
                );
            icons.push(cell(button.into_any_element()));
        }

        let band_cell = self.band.clone();
        let view = cx.entity_id();
        let measure = canvas(
            move |bounds, window, _| {
                if band_cell.get() != bounds {
                    band_cell.set(bounds);
                    window.on_next_frame(move |_, cx| cx.notify(view));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let popover = self.info.as_ref().map(|card| self.info_popover(card, cx).into_any_element());

        div()
            .id("toolbar")
            .relative()
            .w_full()
            .h(px(BAND_HEIGHT))
            .flex_none()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::band_mouse_down))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, _| {
                if this.moving_window && event.pressed_button == Some(MouseButton::Left) {
                    this.moving_window = false;
                    window.start_window_move();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.moving_window = false))
            .child(measure)
            .child(div().absolute().left(px(lead)).top_0().h_full().child(leading))
            .child(
                div()
                    .absolute()
                    .right(px(layout::PAGE_INSET + layout::ICON_TRAILING))
                    .top_0()
                    .h_full()
                    .flex()
                    .children(icons),
            )
            .children(popover)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn pages_as_the_editor_container_decides_them() {
        let note = Content::Note("/a.md".into());
        assert_eq!(page_kind(&note, true, false, true, true), PageKind::Note("/a.md".into()));
        assert_eq!(page_kind(&note, false, false, true, true), PageKind::Empty, "gone: nothing to show");
        assert_eq!(page_kind(&Content::Draft, false, false, true, true), PageKind::Draft);
        assert_eq!(page_kind(&Content::Draft, false, false, true, false), PageKind::Empty, "an empty library");
        assert_eq!(page_kind(&Content::Draft, false, false, false, false), PageKind::Empty, "still loading");
        assert_eq!(page_kind(&Content::Memos, false, false, false, false), PageKind::Memos);
        let file = Content::File("/a.png".into());
        assert_eq!(page_kind(&file, false, true, true, true), PageKind::File("/a.png".into()));
        assert_eq!(page_kind(&file, false, false, true, true), PageKind::Empty);
    }

    #[core::prelude::v1::test]
    fn publish_side() {
        let none = |_: &str| None;
        assert_eq!(choose_target(true, false, none, Some("/a.md")), PublishTarget::Wechat);
        assert_eq!(choose_target(false, true, none, None), PublishTarget::Xiaohongshu);
        assert_eq!(choose_target(true, true, none, None), PublishTarget::Wechat);
        let saved = |key: &str| match key {
            "/a.md" => Some("xiaohongshu".to_string()),
            "" => Some("wechat".to_string()),
            _ => None,
        };
        assert_eq!(choose_target(true, true, saved, Some("/a.md")), PublishTarget::Xiaohongshu);
        assert_eq!(choose_target(true, true, saved, Some("/b.md")), PublishTarget::Wechat);
    }

}
