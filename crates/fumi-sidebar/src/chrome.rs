//! Small things drawn all over the sidebar: tooltips, drag labels, row icons,
//! the library tile, text measuring.

use crate::logic::{self, INDENT_GAP, INDENT_ICON};
use fumi_core::library_list::LibraryEntry;
use fumi_core::note::FileKind;
use fumi_ui::icons::{self, Glyph};
use fumi_ui::theme::{self, CalloutKind, t};
use gpui::prelude::*;
use gpui::*;

pub const SYSTEM_FONT: &str = ".SystemUIFont";

/// The system font at `weight`, measuring `text` on one line.
pub fn text_width(text: &str, size: f32, weight: FontWeight, window: &Window) -> f32 {
    let mut measured = font(SYSTEM_FONT);
    measured.weight = weight;
    let run = TextRun { len: text.len(), font: measured, color: black(), background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(SharedString::from(text.to_string()), px(size), &[run], None);
    f32::from(line.width)
}

/// A help tag, in the place of `.help()`.
pub struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div()
            .flex()
            .flex_col()
            .font_family(SYSTEM_FONT)
            .text_size(px(11.))
            .text_color(c.text2)
            .bg(c.page)
            .border_1()
            .border_color(c.control_border)
            .rounded(px(5.))
            .px(px(7.))
            .py(px(3.))
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.15),
                offset: point(px(0.), px(2.)),
                blur_radius: px(6.),
                spread_radius: px(0.),
                inset: false,
            }])
            .children(self.0.split('\n').map(|line| div().whitespace_nowrap().child(line.to_string())))
    }
}

pub fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone())).into()
}

/// What a row's drag label shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowIcon {
    Folder,
    Note,
    File(FileKind),
}

impl RowIcon {
    pub fn glyph(self) -> Glyph {
        match self {
            RowIcon::Folder => Glyph::FolderClosed,
            RowIcon::Note => Glyph::Note,
            RowIcon::File(FileKind::Image) => Glyph::Image,
            RowIcon::File(FileKind::Video) => Glyph::Video,
            RowIcon::File(FileKind::Pdf) => Glyph::File,
        }
    }

    pub fn size(self) -> f32 {
        if self == RowIcon::Folder { 16. } else { 15. }
    }
}

/// A 16 pt slot holding an icon, so names line up across rows.
pub fn icon_slot(icon: AnyElement) -> Div {
    div().flex_none().w(px(INDENT_ICON)).h(px(INDENT_ICON)).flex().items_center().justify_center().child(icon)
}

/// What follows the pointer while a tree row is dragged: its icon and name on a
/// small raised label, at most 220 wide.
pub struct RowDragLabel {
    pub icon: RowIcon,
    pub title: SharedString,
    pub width: f32,
    /// Keeps the label under the pointer, wherever on the row the drag began.
    pub shift: f32,
}

impl RowDragLabel {
    pub fn new(icon: RowIcon, title: SharedString, grab_x: f32, window: &Window) -> Self {
        let natural = 10. + INDENT_ICON + INDENT_GAP + text_width(&title, 13., FontWeight::MEDIUM, window).ceil() + 10. + 2.;
        RowDragLabel { icon, title, width: natural.min(220.), shift: (grab_x - 24.).max(0.) }
    }
}

impl Render for RowDragLabel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div().pl(px(self.shift)).font_family(SYSTEM_FONT).child(
            div()
                .w(px(self.width))
                .h(px(28.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(INDENT_GAP))
                .bg(c.page)
                .border_1()
                .border_color(c.border)
                .rounded(px(7.))
                .child(icon_slot(icons::glyph(self.icon.glyph(), self.icon.size(), c.text2)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_middle()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.text)
                        .child(self.title.clone()),
                ),
        )
    }
}

/// A library row dragged to reorder: tile and name.
pub struct LibraryDragLabel {
    pub entry: LibraryEntry,
    pub missing: bool,
}

impl Render for LibraryDragLabel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .p(px(6.))
            .font_family(SYSTEM_FONT)
            .child(library_tile(&self.entry, false, self.missing, 28.))
            .child(
                div()
                    .whitespace_nowrap()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.text)
                    .child(self.entry.display_name()),
            )
    }
}

const TINTS: [CalloutKind; 6] =
    [CalloutKind::Note, CalloutKind::Tip, CalloutKind::Important, CalloutKind::Warning, CalloutKind::Danger, CalloutKind::Plain];

/// A library's icon: the gray line glyph, a step darker for the open library and
/// faint for a missing one; or a chosen character on one of the callouts' tints.
pub fn library_tile(entry: &LibraryEntry, current: bool, missing: bool, size: f32) -> AnyElement {
    let c = t();
    match entry.icon.as_deref().filter(|icon| !icon.is_empty()) {
        Some(icon) => {
            let kind = TINTS[entry.tint()];
            let emoji = icon.chars().next().is_some_and(logic::is_emoji_presentation);
            let scale = size / 28.;
            div()
                .flex_none()
                .w(px(size))
                .h(px(size))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7. * scale))
                .bg(if missing { c.inline_bg } else { kind.background(c.dark, c.glass) })
                .text_color(if missing { c.faint_text } else { kind.color(c.dark) })
                .text_size(px(if emoji { 16. } else { 13. } * scale))
                .font_weight(FontWeight::SEMIBOLD)
                .opacity(if missing { 0.6 } else { 1. })
                .child(icon.to_string())
                .into_any_element()
        }
        None => {
            let color = if missing {
                c.faint_icon
            } else if current {
                c.text2
            } else {
                c.icon
            };
            div()
                .flex_none()
                .w(px(size))
                .h(px(size))
                .flex()
                .items_center()
                .justify_center()
                .child(icons::glyph(Glyph::Library, 16., color))
                .into_any_element()
        }
    }
}

/// The shadow under a selected row: nothing but on Glass.
pub fn selected_shadow(c: &theme::Tokens) -> Vec<BoxShadow> {
    if !c.glass {
        return Vec::new();
    }
    vec![BoxShadow { color: c.selected_row_shadow, offset: point(px(0.), px(0.5)), blur_radius: px(1.5), spread_radius: px(0.), inset: false }]
}

/// `colorMultiply(scrimShade)`: what the toolbar's items take on behind Settings
/// and search, quantized so a glyph's few shades are rasterized once.
pub fn multiply_shade(color: Hsla, shade: f32) -> Hsla {
    Hsla { l: color.l * ((shade * 32.).round() / 32.), ..color }
}
