//! The sidebar's first level: every library, one 44 pt row each, the open one
//! filled in; Open Folder… under them; Settings at the bottom. C10.

use crate::chrome::{LibraryDragLabel, icon_slot, library_tile, selected_shadow, tip};
use crate::logic::*;
use crate::view::{DialogKind, ListKind, SidebarView, Timed};
use fumi_core::library_list::{self, LibraryEntry};
use fumi_ui::icons::{self, Glyph};
use fumi_ui::native::{self, weight};
use fumi_ui::theme::t;
use fumi_workspace::libraries;
use gpui::prelude::*;
use gpui::*;
use std::path::Path;

/// A library row being dragged to a new place in the list.
#[derive(Clone, Debug)]
struct LibraryDrag {
    path: String,
}

impl SidebarView {
    pub(crate) fn render_libraries_level(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let entries: Vec<LibraryEntry> = libraries::with(|list| list.entries().to_vec());
        let home = std::env::var("HOME").unwrap_or_default();
        let e = self.library_positions.as_ref().map_or(1., |(_, timed)| timed.value());
        let mut rows: Vec<AnyElement> = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            // Sliding from where it was, after a reorder.
            let from = self.library_positions.as_ref().and_then(|(paths, _)| paths.iter().position(|path| *path == entry.path));
            let shift = from.map_or(0., |from| (from as f32 - index as f32) * LIBRARY_ROW_PITCH * (1. - e));
            rows.push(self.library_row(entry, &home, shift, cx));
        }
        let height = libraries_content_height(entries.len());
        let scroll = self.libraries_scroll.clone();
        let scroller = self.scroller(ListKind::Libraries, height, window, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(30.))
                    .mt(px(2.))
                    .ml(px(INSET))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c.text)
                    .child("Libraries"),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("libraries-list")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&scroll)
                            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(LIBRARY_ROW_SPACING))
                                    .pl(px(INSET))
                                    .pr(px(8.))
                                    .pt(px(4.))
                                    .pb(px(16.))
                                    .children(rows)
                                    .child(self.open_folder_row(cx)),
                            ),
                    )
                    .children(scroller),
            )
            .child(self.settings_row(cx))
    }

    fn library_row(&self, entry: &LibraryEntry, home: &str, shift: f32, cx: &mut Context<Self>) -> AnyElement {
        let c = t();
        let current = self.is_current_library(&entry.path, cx);
        let reason = library_list::missing_reason_on_disk(&entry.path);
        let missing = reason.is_some();
        let detail = reason.unwrap_or_else(|| library_list::location(&entry.path, home));
        let count = if current {
            let w = self.workspace.read(cx);
            w.store.has_loaded_notes().then(|| note_count_label(w.store.notes().len()))
        } else {
            None
        };
        let row_key: SharedString = format!("library:{}", entry.path).into();
        let button_key: SharedString = format!("library-window:{}", entry.path).into();
        let hovered = self.is_hovered(&row_key);
        let shows_new_window = hovered && !current && !missing;
        let name_color = if missing { c.faint_text } else { c.text };
        let detail_color = if missing { c.faint_text } else { c.text3 };

        let enter_path = entry.path.clone();
        let menu_entry = entry.clone();
        let drag_entry = entry.clone();
        let drop_target = entry.path.clone();
        let window_path = entry.path.clone();
        let button_hovered = self.is_hovered(&button_key);
        div()
            .id(row_key.clone())
            .relative()
            .flex_none()
            .top(px(shift))
            .h(px(LIBRARY_ROW_HEIGHT))
            // 8 with the target stroke, which takes no room in the Swift app.
            .pl(px(7.))
            .pr(px(7.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(7.))
            .border_1()
            .border_color(transparent_black())
            .cursor(CursorStyle::Arrow)
            .map(|row| {
                if current {
                    row.bg(c.ground_selected).shadow(selected_shadow(&c))
                } else {
                    row.when(hovered, |row| row.bg(c.card_hover))
                }
            })
            .child(library_tile(entry, current, missing, 28.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(if current { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                            .text_color(name_color)
                            .child(entry.display_name()),
                    )
                    .child(
                        // A long location gives way; the count stays whole.
                        div()
                            .flex()
                            .text_size(px(12.))
                            .text_color(detail_color)
                            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis_middle().child(detail))
                            .children(count.map(|count| div().flex_none().whitespace_nowrap().child(count))),
                    ),
            )
            .when(!missing, |row| {
                // The new-window button takes the chevron's place on hover.
                row.child(
                    div()
                        .flex_none()
                        .w(px(12.))
                        .ml(px(-4.))
                        .flex()
                        .justify_center()
                        .when(shows_new_window, |slot| slot.opacity(0.))
                        .child(icons::symbol("chevron.right", 9., weight::BOLD, c.faint_icon)),
                )
            })
            .when(shows_new_window, |row| {
                row.child(
                    div()
                        .id(button_key.clone())
                        .absolute()
                        .right(px(3.))
                        .top(px((LIBRARY_ROW_HEIGHT - 24.) / 2. - 1.))
                        .w(px(24.))
                        .h(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .when(button_hovered, |button| button.bg(c.card_hover))
                        .child(icons::symbol("arrow.up.forward.square", 12., weight::REGULAR, if button_hovered { c.text } else { c.icon }))
                        .tooltip(tip("Open in New Window"))
                        .on_hover(self.hover_listener(button_key.clone(), cx))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.open_in_window(&window_path, cx);
                        })),
                )
            })
            .when(!button_hovered, |row| row.tooltip(tip(library_tooltip(&entry.path, missing))))
            .on_hover(self.hover_listener(row_key, cx))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                // A click goes in; ⌘-click opens the library in a window of its own.
                if event.modifiers().platform && !this.is_current_library(&enter_path, cx) {
                    this.open_in_window(&enter_path, cx);
                } else {
                    this.enter_library(Path::new(&enter_path), cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.show_library_menu(menu_entry.clone(), event.position, window, cx);
                }),
            )
            .on_drag(LibraryDrag { path: entry.path.clone() }, move |_, _, _, cx| {
                let entry = drag_entry.clone();
                let missing = library_list::missing_reason_on_disk(&entry.path).is_some();
                cx.new(|_| LibraryDragLabel { entry, missing })
            })
            .drag_over::<LibraryDrag>(move |row, _, _, _| row.border_color(c.accent.opacity(0.5)))
            .on_drop(cx.listener(move |this, drag: &LibraryDrag, _, cx| this.reorder_library(&drag.path, &drop_target, cx)))
            .into_any_element()
    }

    fn reorder_library(&mut self, path: &str, target: &str, cx: &mut Context<Self>) {
        let before: Vec<String> = libraries::with(|list| list.entries().iter().map(|entry| entry.path.clone()).collect());
        if !before.iter().any(|known| known == path) {
            return;
        }
        libraries::with(|list| list.move_entry(path, target));
        self.library_positions = Some((before, Timed::new(0., 1., 0.15, Curve::EaseInOut)));
        cx.refresh_windows();
    }

    fn open_folder_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div()
            .id("open-folder-row")
            .flex_none()
            .mt(px(6.))
            .h(px(30.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(9.))
            .rounded(px(6.))
            .text_size(px(13.))
            .text_color(c.text3)
            .hover(move |row| row.bg(c.card_hover).text_color(c.text))
            .child(icon_slot(icons::glyph(Glyph::FolderClosed, 16., c.icon)))
            .child(div().flex_1().min_w_0().truncate().child("Open Folder…"))
            .child(div().flex_none().text_size(px(12.)).text_color(c.faint_text).child("⌘O"))
            .on_click(cx.listener(|this, _, window, cx| this.open_folder(window, cx)))
    }

    fn show_library_menu(&mut self, entry: LibraryEntry, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.is_current_library(&entry.path, cx);
        let missing = library_list::missing_reason_on_disk(&entry.path).is_some();
        self.run_menu(library_menu(current, missing), position, window, cx, move |this, action, window, cx| match action {
            LibraryAction::OpenInNewWindow => this.open_in_window(&entry.path, cx),
            LibraryAction::Reveal => native::reveal_in_finder(Path::new(&entry.path)),
            LibraryAction::Rename => this.open_dialog(
                DialogKind::RenameLibrary { path: entry.path.clone() },
                "Rename Library",
                "Only the name Fumi shows changes; the folder keeps its own. Leave it empty to use the folder’s name.",
                "Rename",
                Some(("Name", entry.display_name())),
                window,
                cx,
            ),
            LibraryAction::ChangeIcon => this.open_dialog(
                DialogKind::ChangeIcon { path: entry.path.clone() },
                "Change Icon",
                "One character or emoji. Leave it empty for the plain library icon.",
                "Change",
                Some(("A letter or emoji", entry.icon.clone().unwrap_or_default())),
                window,
                cx,
            ),
            LibraryAction::Remove => {
                libraries::with(|list| list.remove(&entry.path));
                cx.refresh_windows();
            }
        });
    }
}
