//! The sidebar's second level, inside a library: ‹ Libraries, the library's name,
//! Search, Memos, the notes tree, Settings. C6 to C9.

use crate::chrome::{RowDragLabel, RowIcon, icon_slot, library_tile, selected_shadow, tip};
use crate::logic::{Menu, *};
use crate::view::{DialogKind, KNOB_FADE, KNOB_HOLD, KnobGrab, ListKind, SidebarEvent, SidebarView, TreeDrag};
use fumi_core::library_list::{self, last_path_component};
use fumi_core::library_move::{self, Refusal};
use fumi_core::note_tree;
use fumi_core::tabs::Content;
use fumi_core::{app_model::SortMode, attachment_import};
use fumi_ui::icons::{self, Glyph};
use fumi_ui::native::{self, weight};
use fumi_ui::settings;
use fumi_ui::theme::{Tokens, t};
use fumi_workspace::{libraries, now};
use gpui::prelude::*;
use gpui::*;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

impl SidebarView {
    /// Remembers whether an element is under the pointer, for what `.hover` can't
    /// restyle: an image's color.
    pub(crate) fn hover_listener(&self, key: impl Into<SharedString>, cx: &mut Context<Self>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        let key: SharedString = key.into();
        cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered {
                this.hovered.insert(key.clone());
            } else {
                this.hovered.remove(&key);
            }
            cx.notify();
        })
    }

    pub(crate) fn is_hovered(&self, key: &str) -> bool {
        self.hovered.contains(key)
    }

    pub(crate) fn render_library_level(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let top = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(1.))
            .pl(px(INSET))
            .pr(px(8.))
            .pt(px(2.))
            .child(self.back_row(cx))
            .child(self.library_header(cx))
            .child(self.search_row(cx))
            .child(self.memos_row(cx));
        let rule = div().flex_none().h(px(1.)).mx(px(INSET + 8.)).my(px(8.)).bg(c.border);
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(top)
            .child(rule)
            .child(self.notes_list(window, cx))
            .child(self.settings_row(cx))
            .child(self.swipe_catcher(cx))
    }

    // MARK: Top group

    fn back_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let hovered = self.is_hovered("back-row");
        div().flex().pl(px(2.)).child(
            div()
                .id("back-row")
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(6.))
                .text_size(px(12.))
                .text_color(if hovered { c.text2 } else { c.text3 })
                .when(hovered, |row| row.bg(c.card_hover))
                .child(icons::symbol("chevron.left", 10., weight::SEMIBOLD, if hovered { c.text2 } else { c.icon }))
                .child("Libraries")
                .tooltip(tip("Show Libraries (⌘↑)"))
                .on_hover(self.hover_listener("back-row", cx))
                .on_click(cx.listener(|this, _, _, cx| this.go_to_libraries(cx))),
        )
    }

    fn library_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let library = self.workspace.read(cx).workspace_path();
        let entry = library.as_ref().and_then(|path| libraries::with(|list| list.entry_for(&path.to_string_lossy()).cloned()));
        let name: SharedString = entry
            .as_ref()
            .map(|entry| entry.display_name())
            .or_else(|| library.as_ref().map(|path| last_path_component(&path.to_string_lossy()).to_string()))
            .unwrap_or_else(|| "Notes".to_string())
            .into();
        let tile = match &entry {
            Some(entry) => library_tile(entry, false, false, 16.),
            None => icons::glyph(Glyph::Library, 16., c.icon),
        };
        let (more_hovered, add_hovered) = (self.is_hovered("header-more"), self.is_hovered("header-add"));
        let more_bounds = self.more_button.clone();
        let glow = self.highlight("/");
        div()
            .id("library-header")
            .relative()
            .h(px(32.))
            .mb(px(4.))
            .pr(px(4.))
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pl(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .child(tile)
                    .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(c.text).child(name)),
            )
            .child(
                div()
                    .id("header-more")
                    .relative()
                    .flex_none()
                    .w(px(24.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .when(more_hovered, |button| button.bg(c.card_hover))
                    .child(icons::glyph(Glyph::More, 15., if more_hovered { c.text } else { c.icon }))
                    .child(canvas(move |bounds, _, _| more_bounds.set(bounds), |_, _, _, _| {}).absolute().size_full())
                    .tooltip(tip("Folder Options"))
                    .on_hover(self.hover_listener("header-more", cx))
                    .on_click(cx.listener(|this, _, window, cx| this.show_header_menu(window, cx))),
            )
            .child(
                div()
                    .id("header-add")
                    .flex_none()
                    .w(px(24.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .when(add_hovered, |button| button.bg(c.card_hover))
                    .child(icons::symbol("plus", 12., weight::MEDIUM, if add_hovered { c.text } else { c.icon }))
                    .tooltip(tip("New Note (⌘N)"))
                    .on_hover(self.hover_listener("header-add", cx))
                    .on_click(cx.listener(|this, _, _, cx| this.create_note(cx))),
            )
            .when(glow > 0., |header| header.child(drop_highlight(&c, glow)))
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<TreeDrag>, _, cx| this.drag_over_header(event.event.position, event.bounds, cx)))
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<ExternalPaths>, _, cx| this.drag_over_header(event.event.position, event.bounds, cx)))
            .on_drop(cx.listener(|this, drag: &TreeDrag, window, cx| this.dropped(vec![drag.path.clone()], Some("/"), window, cx)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| this.dropped(paths.paths().to_vec(), Some("/"), window, cx)))
    }

    fn search_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div()
            .id("search-row")
            .h(px(30.))
            // 10 with the hairline, which takes no room in the Swift app.
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(9.))
            .rounded(px(6.))
            .border_1()
            .border_color(transparent_black())
            .hover(move |row| row.bg(c.card_hover).border_color(c.control_border))
            .child(icon_slot(icons::symbol("magnifyingglass", 13., weight::REGULAR, c.icon)))
            .child(div().text_size(px(13.)).text_color(c.text3).child("Search"))
            .tooltip(tip("Search Notes (⇧⌘F)"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.workspace.update(cx, |w, cx| {
                    w.model.search_presented = true;
                    w.navigated(cx);
                });
            }))
    }

    fn memos_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.workspace.read(cx).model.shows_memos();
        top_row("memos-row", icons_for_top(Glyph::Memo, selected), "Memos", selected).on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
            let in_new_tab = event.modifiers().platform;
            this.workspace.update(cx, |w, cx| {
                w.model.open_memos(in_new_tab);
                w.navigated(cx);
            });
        }))
    }

    /// At the bottom of both levels; toggles Settings.
    pub(crate) fn settings_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let selected = self.workspace.read(cx).model.show_settings;
        let icon = icons::symbol("slider.horizontal.3", 13., weight::REGULAR, if selected { c.text } else { c.icon });
        div().flex_none().pl(px(INSET)).pr(px(8.)).py(px(8.)).child(top_row("settings-row", icon, "Settings", selected).on_click(cx.listener(|this, _, _, cx| {
            this.workspace.update(cx, |w, cx| {
                w.model.show_settings = !w.model.show_settings;
                w.navigated(cx);
            });
        })))
    }

    fn create_note(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            let created = w.store.create_note(None, None, now()).ok().flatten();
            w.model.open_note(created.as_ref().map(|note| note.id.as_str()), false);
            w.library_changed(cx);
            w.navigated(cx);
        });
    }

    fn show_header_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (has_library, sort) = {
            let w = self.workspace.read(cx);
            (w.workspace_path().is_some(), w.model.sort_mode)
        };
        let bounds = self.more_button.get();
        let position = point(bounds.origin.x, bounds.origin.y + bounds.size.height);
        self.run_menu(header_menu(has_library, sort), position, window, cx, |this, action, window, cx| match action {
            HeaderAction::OpenFolder => this.open_folder(window, cx),
            HeaderAction::NewWindow => cx.emit(SidebarEvent::OpenWindow { library: None }),
            HeaderAction::Reveal => {
                if let Some(path) = this.workspace.read(cx).workspace_path() {
                    native::reveal_in_finder(&path);
                }
            }
            HeaderAction::Sort(mode) => this.set_sort_mode(mode, cx),
        });
    }

    fn set_sort_mode(&mut self, mode: SortMode, cx: &mut Context<Self>) {
        settings::set_string(settings::keys::SORT_MODE, mode.raw());
        self.workspace.update(cx, |w, cx| {
            w.model.sort_mode = mode;
            w.navigated(cx);
        });
    }

    /// Shows a native menu, outside any GPUI borrow, and does what was chosen.
    pub(crate) fn run_menu<A: 'static>(
        &mut self,
        menu: Menu<A>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&mut Self, A, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let ns_window = native::ns_window_of(window);
        cx.spawn_in(window, async move |this, cx| {
            let chosen = native::popup_menu_on(ns_window, &menu.rows, position);
            if let Some(action) = chosen.and_then(|index| menu.take(index)) {
                let _ = this.update_in(cx, |this, window, cx| apply(this, action, window, cx));
            }
        })
        .detach();
    }

    // MARK: The tree

    fn notes_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let (loaded, selected_note, selected_file, settings_open, memos_open) = {
            let w = self.workspace.read(cx);
            (
                w.store.has_loaded_notes(),
                w.model.selected_note().map(str::to_string),
                w.model.selected_file().map(str::to_string),
                w.model.show_settings,
                w.model.shows_memos(),
            )
        };
        let selection = Selection {
            note: if settings_open { None } else { selected_note.clone() },
            file: if settings_open || memos_open || selected_note.is_some() { None } else { selected_file },
        };

        let bounds = self.tree_scroll.bounds();
        let viewport = f32::from(bounds.size.height);
        let scrolled = -f32::from(self.tree_scroll.offset().y);
        let row_count = self.rows.len();
        let empty = loaded && self.snapshot.as_ref().is_some_and(|tree| tree.note_count == 0);
        let mut rows: Vec<AnyElement> = Vec::new();
        let height;
        if empty {
            height = ROW_HEIGHT + LIST_BOTTOM;
            rows.push(
                div()
                    .absolute()
                    .left(px(INSET + INDENT_BASE))
                    .top_0()
                    .h(px(ROW_HEIGHT))
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .text_color(c.text3)
                    .child("No notes yet")
                    .into_any_element(),
            );
        } else if let Some(moving) = self.in_motion.as_ref() {
            let e = moving.timed.value();
            let to_height = content_height(row_count as f32);
            height = moving.from_height + (to_height - moving.from_height) * e;
            let (top, bottom) = (scrolled - ROW_PITCH, scrolled + viewport.max(1.) + ROW_PITCH);
            let list: Vec<_> = moving.rows.iter().map(|m| (m.row.clone(), m.at(e))).filter(|(_, (y, _))| *y >= top && *y <= bottom).collect();
            for (row, (y, alpha)) in list {
                rows.push(self.tree_row(&row, y, alpha, &selection, cx));
            }
        } else {
            height = content_height(row_count as f32);
            let range = visible_range(scrolled, if viewport > 0. { viewport } else { 2000. }, row_count);
            let list: Vec<(usize, Rc<TreeRow>)> = range.clone().map(|i| (i, self.rows[i].clone())).collect();
            for (index, row) in list {
                rows.push(self.tree_row(&row, index as f32 * ROW_PITCH, 1., &selection, cx));
            }
        }

        let tree_scroll = self.tree_scroll.clone();
        let scroller = self.scroller(ListKind::Tree, height, window, cx);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id("notes-tree")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&tree_scroll)
                    .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                    .on_drag_move(cx.listener(|this, event: &DragMoveEvent<TreeDrag>, _, cx| this.drag_over_list(event.event.position, event.bounds, cx)))
                    .on_drag_move(cx.listener(|this, event: &DragMoveEvent<ExternalPaths>, _, cx| this.drag_over_list(event.event.position, event.bounds, cx)))
                    .on_drop(cx.listener(|this, drag: &TreeDrag, window, cx| this.dropped(vec![drag.path.clone()], None, window, cx)))
                    .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| this.dropped(paths.paths().to_vec(), None, window, cx)))
                    .child(div().relative().w_full().h(px(height)).children(rows)),
            )
            .children(scroller)
    }

    fn tree_row(&mut self, row: &Rc<TreeRow>, y: f32, alpha: f32, selection: &Selection, cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        let selected = match row.kind {
            RowKind::Folder => false,
            RowKind::Note { .. } => selection.note.as_deref() == Some(row.id.as_ref()),
            RowKind::File(_) => selection.file.as_deref() == Some(row.id.as_ref()),
        };
        let mut element = div()
            .id(ElementId::from(row.id.clone()))
            .absolute()
            .left(px(INSET))
            .right(px(8.))
            .top(px(y))
            .h(px(ROW_HEIGHT))
            .pl(px(row_leading(row.depth)))
            .pr(px(10.))
            .flex()
            .items_center()
            .gap(px(INDENT_GAP))
            .rounded(px(6.))
            .cursor(CursorStyle::Arrow)
            .text_size(px(13.));
        if alpha < 1. {
            element = element.opacity(alpha);
        }
        element = if selected {
            element.bg(c.ground_selected).shadow(selected_shadow(&c))
        } else {
            element.hover(move |row| row.bg(c.card_hover))
        };

        let (icon, element) = match row.kind {
            RowKind::Folder => {
                let open = self.expanded.contains(row.id.as_ref());
                let glyph = if open { Glyph::FolderOpen } else { Glyph::FolderClosed };
                let glow = self.highlight(row.id.as_ref());
                let element = element
                    .text_color(if open { c.text } else { c.text2 })
                    .hover(move |row| row.bg(c.card_hover).text_color(c.text))
                    .child(icon_slot(icons::glyph(glyph, 16., if open { c.text2 } else { c.icon })))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(if open { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                            .child(row.name.clone()),
                    )
                    .when(glow > 0., |element| element.child(drop_highlight(&c, glow)));
                (RowIcon::Folder, element)
            }
            RowKind::Note { pinned } => {
                let element = element
                    .text_color(if selected { c.text } else { c.text2 })
                    .child(icon_slot(icons::glyph(Glyph::Note, 15., if selected { c.text2 } else { c.faint_icon })))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                            .child(row.name.clone()),
                    )
                    .when(pinned, |element| element.child(icons::symbol("pin.fill", 9., weight::REGULAR, c.icon)));
                (RowIcon::Note, element)
            }
            RowKind::File(kind) => {
                let icon = RowIcon::File(kind);
                let element = element
                    .text_color(if selected { c.text } else { c.text3 })
                    .child(icon_slot(icons::glyph(icon.glyph(), 15., if selected { c.text2 } else { c.faint_icon })))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_middle()
                            .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                            .child(row.name.clone()),
                    );
                (icon, element)
            }
        };

        let title = row.name.clone();
        let click_row = row.clone();
        let menu_row = row.clone();
        element
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| this.row_clicked(&click_row, event.modifiers().platform, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| this.show_row_menu(menu_row.clone(), event.position, window, cx)),
            )
            .on_drag(TreeDrag { path: row.path.clone(), is_folder: row.is_folder() }, move |_, grab, window, cx| {
                let label = RowDragLabel::new(icon, title.clone(), f32::from(grab.x), window);
                cx.new(|_| label)
            })
            .external_drag_payload(|drag: &TreeDrag, _, _| {
                Some(ExternalDragPayload::Files(FileDragPaths::new([(drag.path.clone(), drag.is_folder)])))
            })
            .into_any_element()
    }

    fn row_clicked(&mut self, row: &Rc<TreeRow>, in_new_tab: bool, cx: &mut Context<Self>) {
        let id = row.id.to_string();
        match row.kind {
            RowKind::Folder => self.toggle_folder(&id, cx),
            RowKind::Note { .. } => self.workspace.update(cx, |w, cx| {
                w.model.open_note(Some(&id), in_new_tab);
                w.navigated(cx);
            }),
            RowKind::File(_) => self.workspace.update(cx, |w, cx| {
                w.model.open_file(&id, in_new_tab);
                w.navigated(cx);
            }),
        }
    }

    // MARK: Row menus

    fn show_row_menu(&mut self, row: Rc<TreeRow>, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let folder_ids = self.snapshot.as_ref().map(|tree| tree.folder_ids.clone()).unwrap_or_default();
        let library_name = self.library_folder_name(cx);
        let menu = row_menu(&row, &folder_ids, &library_name);
        self.run_menu(menu, position, window, cx, move |this, action, window, cx| this.apply_row_action(&row, action, window, cx));
    }

    fn library_folder_name(&self, cx: &App) -> String {
        let library = self.workspace.read(cx).workspace_path().map(|path| path.to_string_lossy().into_owned());
        note_tree::folder_name("/", library.as_deref())
    }

    fn apply_row_action(&mut self, row: &TreeRow, action: RowAction, window: &mut Window, cx: &mut Context<Self>) {
        let id = row.id.to_string();
        match action {
            RowAction::TogglePin => self.workspace.update(cx, |w, cx| {
                w.store.toggle_pin(&id);
                w.library_changed(cx);
            }),
            RowAction::Rename => self.open_dialog(
                DialogKind::RenameNote { id },
                "Rename Note",
                "Enter a new file name (without the .md extension).",
                "Rename",
                Some(("File name", row.name.to_string())),
                window,
                cx,
            ),
            RowAction::MoveTo(folder) => self.move_items(vec![row.path.clone()], &folder, window, cx),
            RowAction::Reveal => native::reveal_in_finder(&row.path),
            RowAction::Delete => self.workspace.update(cx, |w, cx| {
                w.model.remove_tabs(&Content::Note(id.clone()));
                let _ = w.store.delete_note(&id);
                w.library_changed(cx);
                w.navigated(cx);
            }),
            RowAction::CopyLink => {
                if let Some(library) = self.workspace.read(cx).workspace_path() {
                    let path = attachment_import::relative_path(&library, &row.path);
                    cx.write_to_clipboard(ClipboardItem::new_string(attachment_import::markdown_for_path(&path)));
                }
            }
            RowAction::OpenInDefaultApp => cx.open_with_system(&row.path),
            RowAction::Trash => {
                if native::trash(&row.path) {
                    self.workspace.update(cx, |w, cx| {
                        w.model.remove_tabs(&Content::File(id.clone()));
                        w.navigated(cx);
                        w.reload(cx);
                    });
                }
            }
        }
    }

    // MARK: Moving

    /// Moves items into `folder`: the tabs and the open folders follow what moved,
    /// and the destination opens to show it. `NoteTreeState.move`.
    fn move_items(&mut self, items: Vec<PathBuf>, folder: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_state.reset();
        let Some(library) = self.workspace.read(cx).workspace_path() else { return };
        let library_text = library.to_string_lossy().into_owned();
        let folder_label = note_tree::folder_name(folder, Some(&library_text));
        let mut moved = Vec::new();
        let mut failure = None;
        let mut refused_into_itself = false;
        self.workspace.update(cx, |w, cx| {
            for item in &items {
                match w.store.move_item(item, folder, now()) {
                    Ok(new_path) => {
                        w.model.remap_tabs(&item.to_string_lossy(), &new_path.to_string_lossy());
                        moved.push((item.clone(), new_path));
                    }
                    Err(Refusal::IntoItself) => refused_into_itself = true,
                    Err(refusal) => {
                        let name = item.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                        failure = note_tree::failed_move(&refusal, &name, &folder_label).or(failure.take());
                    }
                }
            }
            if !moved.is_empty() {
                w.library_changed(cx);
                w.navigated(cx);
            }
        });
        if refused_into_itself {
            native::beep();
        }
        if !moved.is_empty() {
            // A moved folder stays open, and so do the folders inside it.
            for (old, new) in &moved {
                if let (Some(old_id), Some(new_id)) = (library_move::folder_id(old, &library), library_move::folder_id(new, &library)) {
                    self.expanded = note_tree::remap_expanded(std::mem::take(&mut self.expanded), &old_id, &new_id).into_iter().collect();
                }
            }
            self.expanded.extend(note_tree::ancestor_ids(folder));
            self.rows_changed(0.15, cx);
        }
        if let Some(failure) = failure {
            self.open_dialog(DialogKind::Notice, &failure.title, &failure.message, "OK", None, window, cx);
        }
    }

    // MARK: Drag and drop

    fn enter_zone(&mut self, zone: Option<Zone>, cx: &mut Context<Self>) {
        if self.drop_state.zone() == zone.as_ref() {
            return;
        }
        let (settle, spring) = self.drop_state.enter_zone(zone);
        if let Some(ticket) = settle {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(SETTLE_DELAY).await;
                let _ = this.update(cx, |this, cx| {
                    if this.drop_state.settled(ticket) {
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        if let Some(ticket) = spring {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(SPRING_DELAY).await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(folder) = this.drop_state.sprung(ticket) {
                        this.expand_with_motion([folder], 0.18, cx);
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn drag_over_header(&mut self, position: Point<Pixels>, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        if bounds.contains(&position) {
            self.enter_zone(Some(Zone::Header), cx);
        } else if self.drop_state.zone() == Some(&Zone::Header) {
            self.enter_zone(None, cx);
        }
    }

    fn drag_over_list(&mut self, position: Point<Pixels>, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        if bounds.contains(&position) {
            let x = f32::from(position.x - bounds.origin.x);
            let y = f32::from(position.y - bounds.origin.y) - f32::from(self.tree_scroll.offset().y);
            let zone = zone_at(&self.rows, &self.expanded, x, y, f32::from(bounds.size.width));
            self.enter_zone(Some(zone), cx);
        } else if matches!(self.drop_state.zone(), Some(Zone::Row { .. } | Zone::Behind)) {
            self.enter_zone(None, cx);
        }
    }

    /// A drop on the heading (`folder` given) or on the list, which goes by what
    /// the drag was last over.
    fn dropped(&mut self, items: Vec<PathBuf>, folder: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let target = folder.or_else(|| self.drop_state.zone().map(Zone::folder)).unwrap_or("/").to_string();
        self.move_items(items, &target, window, cx);
    }

    // MARK: Scroller

    /// A list's short overlay thumb, while it's moving.
    pub(crate) fn scroller(&mut self, list: ListKind, content: f32, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let c = t();
        let handle = match list {
            ListKind::Tree => self.tree_scroll.clone(),
            ListKind::Libraries => self.libraries_scroll.clone(),
        };
        let offset = -f32::from(handle.offset().y);
        let viewport = f32::from(handle.bounds().size.height);
        let held = self.knob_grab.as_ref().is_some_and(|grab| grab.list == list);
        let knob_state = match list {
            ListKind::Tree => &mut self.tree_knob,
            ListKind::Libraries => &mut self.libraries_knob,
        };
        if knob_state.offset != offset || held {
            knob_state.offset = offset;
            knob_state.moved_at = Some(Instant::now());
        }
        let moved_at = knob_state.moved_at?;
        let elapsed = moved_at.elapsed();
        if elapsed >= KNOB_HOLD + KNOB_FADE {
            knob_state.moved_at = None;
            return None;
        }
        if elapsed < KNOB_HOLD {
            if !knob_state.timer {
                knob_state.timer = true;
                let wait = KNOB_HOLD - elapsed;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    let _ = this.update(cx, |this, cx| {
                        match list {
                            ListKind::Tree => this.tree_knob.timer = false,
                            ListKind::Libraries => this.libraries_knob.timer = false,
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        let alpha = if elapsed < KNOB_HOLD { 1. } else { 1. - (elapsed - KNOB_HOLD).as_secs_f32() / KNOB_FADE.as_secs_f32() };
        if elapsed >= KNOB_HOLD {
            window.request_animation_frame();
        }
        let (y, height) = scroller_knob(viewport, content, offset)?;
        Some(
            div()
                .id(match list {
                    ListKind::Tree => "tree-knob",
                    ListKind::Libraries => "libraries-knob",
                })
                .absolute()
                .right_0()
                .top(px(y))
                .w(px(7.))
                .h(px(height))
                .flex()
                .justify_center()
                .occlude()
                .child(div().w(px(4.)).h_full().rounded(px(2.)).bg(c.faint_icon.opacity(0.6 * alpha)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let top = f32::from(handle.bounds().origin.y) + y;
                        this.knob_grab = Some(KnobGrab { list, grab: f32::from(event.position.y) - top, content });
                        cx.notify();
                    }),
                ),
        )
    }

    /// Whether `path` is the library this window shows.
    pub(crate) fn is_current_library(&self, path: &str, cx: &App) -> bool {
        self.workspace
            .read(cx)
            .workspace_path()
            .is_some_and(|current| library_list::standardized_path(&current.to_string_lossy()) == path)
    }
}

/// What the tree shows as selected.
struct Selection {
    note: Option<String>,
    file: Option<String>,
}

/// Memos and Settings: 30 tall, unselected `icon` and `text2`, selected `text`
/// semibold on `groundSelected`.
fn top_row(id: &'static str, icon: AnyElement, title: &'static str, selected: bool) -> Stateful<Div> {
    let c = t();
    div()
        .id(id)
        .h(px(30.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(9.))
        .rounded(px(6.))
        .text_size(px(13.))
        .map(|row| {
            if selected {
                row.bg(c.ground_selected).shadow(selected_shadow(&c)).text_color(c.text).font_weight(FontWeight::SEMIBOLD)
            } else {
                row.hover(move |row| row.bg(c.card_hover)).text_color(c.text2)
            }
        })
        .child(icon_slot(icon))
        .child(div().flex_1().min_w_0().truncate().child(title))
}

fn icons_for_top(glyph: Glyph, selected: bool) -> AnyElement {
    let c = t();
    icons::glyph(glyph, 16., if selected { c.text } else { c.icon })
}

/// The tint on a folder row or the library heading a drop would land in.
fn drop_highlight(c: &Tokens, amount: f32) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .rounded(px(6.))
        .bg(c.accent_hover.opacity(amount))
        .border_1()
        .border_color(c.accent.opacity(0.5 * amount))
}
