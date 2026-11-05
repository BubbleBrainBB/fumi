//! An image, video or PDF opened from the sidebar, shown on the page in place of
//! the editor. A port of `FilePreviewView`.
//!
//! Images are GPUI's. Video and PDF are the same native views the Swift app uses,
//! `AVPlayerView` and `PDFView`, placed over GPUI's view: this page doesn't scroll
//! with them, and that keeps the player's own controls and full screen. GPUI can't
//! draw over a native view, so while search or Settings float above, the native
//! view hides and a still of it takes its place.

use fumi_ui::fonts::family_font;
use crate::mac;
use cocoa::base::{id, nil};
use cocoa::foundation::{NSRect, NSSize};
use fumi_core::attachment_import;
use fumi_core::note::{FileKind, LibraryFile};
use fumi_core::tabs::Content;
use fumi_ui::native::{self, MenuRow, class_named, load_framework, ns};
use fumi_ui::theme::t;
use fumi_workspace::Workspace;
use gpui::prelude::*;
use gpui::*;
use objc::{class, msg_send, sel, sel_impl};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const IMAGE_PADDING: f32 = 40.;
pub const VIDEO_RADIUS: f64 = 8.;
pub const PAGE_RADIUS: f64 = 12.;
pub const FALLBACK_ASPECT: f32 = 16. / 9.;
/// The largest side a still image is decoded to, in pixels.
const MAX_IMAGE_SIDE: f64 = 8192.;

/// `Folder / Sub · 1.2 MB`.
pub fn subtitle(relative_path: &str, size: Option<&str>) -> String {
    let folder = relative_path.split('/').filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" / ");
    [folder.as_str(), size.unwrap_or("")].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ")
}

/// What a video shows as, once its track's transform is applied.
pub fn oriented_aspect(w: f64, h: f64, a: f64, b: f64, c: f64, d: f64) -> Option<f32> {
    let (shown_w, shown_h) = (a * w + c * h, b * w + d * h);
    (shown_w != 0. && shown_h != 0.).then(|| (shown_w / shown_h).abs() as f32)
}

/// The largest rectangle of `aspect` in `area`, centered: left, top, width, height.
pub fn fit_rect(area: (f32, f32, f32, f32), aspect: f32) -> (f32, f32, f32, f32) {
    let (x, y, w, h) = area;
    if w <= 0. || h <= 0. || aspect <= 0. {
        return (x, y, 0., 0.);
    }
    let (fw, fh) = if w / h > aspect { (h * aspect, h) } else { (w, w / aspect) };
    (x + (w - fw) / 2., y + (h - fh) / 2., fw, fh)
}

/// Whether GPUI should decode the file itself, for animation.
pub fn animates(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        let e = e.to_string_lossy().to_lowercase();
        e == "gif" || e == "webp"
    })
}

/// The file's markdown link, from the library's root.
pub fn markdown_link(library: &Path, file: &Path) -> String {
    attachment_import::markdown_for_path(&attachment_import::relative_path(library, file))
}

/// The toolbar's More menu, in the order of `MoreAction::ALL`.
pub fn more_menu_rows() -> Vec<MenuRow> {
    vec![
        MenuRow::item("Copy Markdown Link"),
        MenuRow::item("Open in Default App"),
        MenuRow::item("Reveal in Finder"),
        MenuRow::Separator,
        MenuRow::destructive("Move to Trash"),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoreAction {
    CopyMarkdownLink,
    OpenInDefaultApp,
    RevealInFinder,
    MoveToTrash,
}

impl MoreAction {
    pub const ALL: [MoreAction; 4] =
        [Self::CopyMarkdownLink, Self::OpenInDefaultApp, Self::RevealInFinder, Self::MoveToTrash];
}

fn file_url(path: &Path) -> id {
    unsafe { msg_send![class!(NSURL), fileURLWithPath: ns(&path.to_string_lossy())] }
}

/// The aspect of a video file's first track, or None. Off the main thread.
fn video_aspect(path: &Path) -> Option<f32> {
    load_framework("AVFoundation");
    mac::with_pool(|| unsafe {
        let asset: id = msg_send![class_named("AVURLAsset")?, URLAssetWithURL: file_url(path) options: nil];
        if asset == nil {
            return None;
        }
        let tracks: id = msg_send![asset, tracksWithMediaType: ns("vide")];
        let track: id = msg_send![tracks, firstObject];
        if track == nil {
            return None;
        }
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Transform {
            a: f64,
            b: f64,
            c: f64,
            d: f64,
            tx: f64,
            ty: f64,
        }
        let size: NSSize = msg_send![track, naturalSize];
        let m: Transform = msg_send![track, preferredTransform];
        let _ = (m.tx, m.ty);
        oriented_aspect(size.width, size.height, m.a, m.b, m.c, m.d)
    })
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

/// A still of the player where it is now.
fn video_still(player: id, path: &Path) -> Option<Arc<RenderImage>> {
    mac::with_pool(|| unsafe {
        let asset: id = msg_send![class_named("AVURLAsset")?, URLAssetWithURL: file_url(path) options: nil];
        let generator: id = msg_send![class_named("AVAssetImageGenerator")?, assetImageGeneratorWithAsset: asset];
        if generator == nil {
            return None;
        }
        let _: () = msg_send![generator, setAppliesPreferredTrackTransform: true];
        let at: CMTime = msg_send![player, currentTime];
        let cg: *mut std::ffi::c_void = msg_send![generator,
            copyCGImageAtTime: at
            actualTime: std::ptr::null_mut::<CMTime>()
            error: std::ptr::null_mut::<id>()];
        if cg.is_null() {
            return None;
        }
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithCGImage: cg size: NSSize::new(0., 0.)];
        let size: NSSize = msg_send![image, size];
        let scale = (1920. / size.width.max(size.height).max(1.)).min(1.);
        let out = native::draw_to_render_image(image, size, scale);
        let _: () = msg_send![image, release];
        unsafe extern "C" {
            fn CGImageRelease(image: *mut std::ffi::c_void);
        }
        CGImageRelease(cg);
        out
    })
}

/// A still of what any view shows.
fn view_still(view: id, scale: f64) -> Option<Arc<RenderImage>> {
    mac::with_pool(|| unsafe {
        let bounds: NSRect = msg_send![view, bounds];
        let rep: id = msg_send![view, bitmapImageRepForCachingDisplayInRect: bounds];
        if rep == nil {
            return None;
        }
        let _: () = msg_send![view, cacheDisplayInRect: bounds toBitmapImageRep: rep];
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithSize: bounds.size];
        let _: () = msg_send![image, addRepresentation: rep];
        let out = native::draw_to_render_image(image, bounds.size, scale);
        let _: () = msg_send![image, release];
        out
    })
}

/// A native view above GPUI's, following a layout slot.
struct NativeSlot {
    view: id,
    /// The `AVPlayer`, for a video.
    player: id,
    parent_height: f64,
    gpui_view: id,
    ns_window: id,
    last: Option<Bounds<Pixels>>,
}

impl NativeSlot {
    fn install(view: id, player: id, radius: f64, window: &Window) -> Self {
        let gpui_view = mac::gpui_view_of(window);
        let ns_window = native::ns_window_of(window);
        unsafe {
            let parent: id = msg_send![gpui_view, superview];
            let _: () = msg_send![view, setWantsLayer: true];
            let layer: id = msg_send![view, layer];
            let _: () = msg_send![layer, setCornerRadius: radius];
            let _: () = msg_send![layer, setMasksToBounds: true];
            let _: () = msg_send![view, setHidden: true];
            // NSWindowAbove.
            let _: () = msg_send![parent, addSubview: view positioned: 1i64 relativeTo: gpui_view];
            let frame: NSRect = msg_send![parent, frame];
            NativeSlot { view, player, parent_height: frame.size.height, gpui_view, ns_window, last: None }
        }
    }

    fn place(&mut self, bounds: Bounds<Pixels>, visible: bool) {
        unsafe {
            let parent: id = msg_send![self.gpui_view, superview];
            let frame: NSRect = msg_send![parent, frame];
            self.parent_height = frame.size.height;
        }
        if self.last != Some(bounds) {
            self.last = Some(bounds);
            native::set_frame(self.view, native::ns_frame(bounds, self.parent_height));
        }
        native::set_hidden(self.view, !visible);
    }

    fn hide(&mut self) {
        native::set_hidden(self.view, true);
        mac::make_first_responder(self.ns_window, self.gpui_view);
    }
}

impl Drop for NativeSlot {
    fn drop(&mut self) {
        unsafe {
            if self.player != nil {
                let _: () = msg_send![self.player, pause];
                let _: () = msg_send![self.view, setPlayer: nil];
                let _: () = msg_send![self.player, release];
            }
            let _: () = msg_send![self.view, removeFromSuperview];
            let _: () = msg_send![self.view, release];
        }
        mac::make_first_responder(self.ns_window, self.gpui_view);
    }
}

fn make_video(path: &Path, window: &Window) -> Option<NativeSlot> {
    load_framework("AVFoundation");
    load_framework("AVKit");
    unsafe {
        let player: id = msg_send![class_named("AVPlayer")?, playerWithURL: file_url(path)];
        let player: id = msg_send![player, retain];
        let view: id = msg_send![class_named("AVPlayerView")?, alloc];
        let view: id = msg_send![view, initWithFrame: mac::zero_rect()];
        let _: () = msg_send![view, setControlsStyle: 1i64]; // inline
        let _: () = msg_send![view, setVideoGravity: ns("AVLayerVideoGravityResizeAspect")];
        let _: () = msg_send![view, setShowsFullScreenToggleButton: true];
        let _: () = msg_send![view, setPlayer: player];
        Some(NativeSlot::install(view, player, VIDEO_RADIUS, window))
    }
}

fn make_pdf(path: &Path, window: &Window) -> Option<NativeSlot> {
    load_framework("PDFKit");
    unsafe {
        let view: id = msg_send![class_named("PDFView")?, alloc];
        let view: id = msg_send![view, initWithFrame: mac::zero_rect()];
        let _: () = msg_send![view, setAutoScales: true];
        let _: () = msg_send![view, setDisplayMode: 1i64]; // single page, continuous
        let clear: id = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![view, setBackgroundColor: clear];
        let doc: id = msg_send![class_named("PDFDocument")?, alloc];
        let doc: id = msg_send![doc, initWithURL: file_url(path)];
        if doc != nil {
            let _: () = msg_send![view, setDocument: doc];
            let _: () = msg_send![doc, release];
        }
        Some(NativeSlot::install(view, nil, PAGE_RADIUS, window))
    }
}

enum Body {
    /// Decoded through AppKit, like the image view it replaces.
    Still { image: Option<(Arc<RenderImage>, (f32, f32))> },
    /// Left to GPUI, which animates: its size in points if known.
    Animated { size: Option<(f32, f32)> },
    Video { aspect: Option<f32>, slot: Option<NativeSlot> },
    Pdf { slot: Option<NativeSlot> },
}

/// One file on the page. The root keeps one for the active tab's file and drops it
/// when the tab shows anything else: dropping stops a video and takes its native
/// view away.
pub struct FilePreview {
    workspace: Entity<Workspace>,
    pub file_id: String,
    path: PathBuf,
    kind: FileKind,
    relative_path: String,
    size_label: Option<String>,
    body: Body,
    still: Option<Arc<RenderImage>>,
    covered: bool,
    _load: Option<Task<()>>,
    _sub: Subscription,
}

impl FilePreview {
    pub fn new(workspace: Entity<Workspace>, file: &LibraryFile, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let path = file.path.clone();
        let kind = file.kind;
        let size_label = std::fs::metadata(&path).ok().map(|m| mac::byte_count_string(m.len() as i64));
        let sub = cx.observe_in(&workspace, window, |this, _, window, cx| this.workspace_changed(window, cx));
        let (body, load) = match kind {
            FileKind::Image if animates(&path) => {
                let p = path.clone();
                let load = cx.spawn(async move |this: WeakEntity<Self>, cx| {
                    let size = cx.background_spawn(async move { mac::image_file_size(&p) }).await;
                    let _ = this.update(cx, |this, cx| {
                        this.body = Body::Animated { size };
                        cx.notify();
                    });
                });
                (Body::Still { image: None }, Some(load))
            }
            FileKind::Image => {
                let p = path.clone();
                let load = cx.spawn(async move |this: WeakEntity<Self>, cx| {
                    let image = cx.background_spawn(async move { mac::load_image_file(&p, MAX_IMAGE_SIDE) }).await;
                    let _ = this.update(cx, |this, cx| {
                        this.body = Body::Still { image };
                        cx.notify();
                    });
                });
                (Body::Still { image: None }, Some(load))
            }
            FileKind::Video => {
                let p = path.clone();
                let load = cx.spawn(async move |this: WeakEntity<Self>, cx| {
                    let aspect = cx.background_spawn(async move { video_aspect(&p) }).await;
                    let _ = this.update(cx, |this, cx| {
                        if let Body::Video { aspect: slot_aspect, .. } = &mut this.body {
                            *slot_aspect = Some(aspect.unwrap_or(FALLBACK_ASPECT));
                        }
                        cx.notify();
                    });
                });
                (Body::Video { aspect: None, slot: make_video(&path, window) }, Some(load))
            }
            FileKind::Pdf => (Body::Pdf { slot: make_pdf(&path, window) }, None),
        };
        let mut this = FilePreview {
            workspace,
            file_id: file.id.clone(),
            path,
            kind,
            relative_path: file.relative_path.clone(),
            size_label,
            body,
            still: None,
            covered: false,
            _load: load,
            _sub: sub,
        };
        this.workspace_changed(window, cx);
        this
    }

    /// The toolbar's title.
    pub fn title(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// The toolbar's subtitle.
    pub fn subtitle(&self) -> String {
        subtitle(&self.relative_path, self.size_label.as_deref())
    }

    /// The toolbar's More button: shows the menu below `at` and runs the choice.
    pub fn show_more_menu(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let ns_window = native::ns_window_of(window) as usize;
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let chosen = native::popup_menu_on(ns_window as id, &more_menu_rows(), at);
            let _ = this.update(cx, |this, cx| {
                if let Some(action) = chosen.and_then(|i| MoreAction::ALL.get(i)) {
                    this.perform(*action, cx);
                }
            });
        })
        .detach();
    }

    pub fn perform(&mut self, action: MoreAction, cx: &mut Context<Self>) {
        match action {
            MoreAction::CopyMarkdownLink => {
                if let Some(library) = self.workspace.read(cx).workspace_path() {
                    cx.write_to_clipboard(ClipboardItem::new_string(markdown_link(&library, &self.path)));
                }
            }
            MoreAction::OpenInDefaultApp => cx.open_with_system(&self.path),
            MoreAction::RevealInFinder => native::reveal_in_finder(&self.path),
            MoreAction::MoveToTrash => {
                if native::trash(&self.path) {
                    let id = self.file_id.clone();
                    self.workspace.update(cx, |w, cx| {
                        w.model.remove_tabs(&Content::File(id));
                        w.navigated(cx);
                        w.reload(cx);
                    });
                } else {
                    mac::beep();
                }
            }
        }
    }

    fn slot(&mut self) -> Option<&mut NativeSlot> {
        match &mut self.body {
            Body::Video { slot, .. } | Body::Pdf { slot } => slot.as_mut(),
            _ => None,
        }
    }

    /// Search or Settings going up or coming down over a native view.
    fn workspace_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let model = &self.workspace.read(cx).model;
        let covered = model.search_presented || model.show_settings;
        if covered == self.covered {
            return;
        }
        self.covered = covered;
        let scale = window.scale_factor() as f64;
        let path = self.path.clone();
        let kind = self.kind;
        if let Some(slot) = self.slot() {
            if covered {
                let still = match kind {
                    FileKind::Video => video_still(slot.player, &path),
                    _ => view_still(slot.view, scale),
                };
                slot.hide();
                self.still = still;
            }
        }
        if !covered {
            self.still = None;
        }
        cx.notify();
    }

    fn render_native(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let aspect = match &self.body {
            Body::Video { aspect, .. } => Some(*aspect),
            _ => None,
        };
        let covered = self.covered;
        let still = self.still.clone();
        let radius = if matches!(self.body, Body::Video { .. }) { VIDEO_RADIUS } else { 0. };
        div()
            .relative()
            .size_full()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let shown = match aspect {
                            None => Some(bounds),
                            Some(None) => None,
                            Some(Some(a)) => {
                                let area = (f32::from(bounds.origin.x), f32::from(bounds.origin.y), f32::from(bounds.size.width), f32::from(bounds.size.height));
                                let (x, y, w, h) = fit_rect(area, a);
                                Some(Bounds::new(point(px(x), px(y)), size(px(w), px(h))))
                            }
                        };
                        entity.update(cx, |this, _| {
                            if let Some(slot) = this.slot() {
                                match shown {
                                    Some(b) => slot.place(b, !covered),
                                    None => native::set_hidden(slot.view, true),
                                }
                            }
                        });
                        shown
                    },
                    move |_, shown, window, _| {
                        if let (Some(b), Some(image)) = (shown, still.clone()) {
                            let _ = window.paint_image(b, b, Corners::all(px(radius as f32)), image, 0, false);
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
}

impl Render for FilePreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = t();
        let padding = if self.kind == FileKind::Pdf { 0. } else { IMAGE_PADDING };
        let content = match &self.body {
            Body::Still { image: Some((image, (w, h))) } => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(img(image.clone()).w(px(*w)).h(px(*h)).max_w_full().max_h_full().object_fit(ObjectFit::Contain))
                .into_any_element(),
            Body::Still { image: None } => div().into_any_element(),
            Body::Animated { size } => {
                let image = img(self.path.clone());
                let image = match size {
                    Some((w, h)) => image.w(px(*w)).h(px(*h)).max_w_full().max_h_full().object_fit(ObjectFit::Contain),
                    None => image.size_full().object_fit(ObjectFit::ScaleDown),
                };
                div().size_full().flex().items_center().justify_center().child(image).into_any_element()
            }
            Body::Video { .. } | Body::Pdf { .. } => self.render_native(cx),
        };
        div()
            .size_full()
            .p(px(padding))
            .bg(c.page_content)
            .font(family_font("System"))
            .child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn subtitles() {
        assert_eq!(subtitle("/attachments/2026", Some("1.2 MB")), "attachments / 2026 · 1.2 MB");
        assert_eq!(subtitle("/", Some("3 KB")), "3 KB");
        assert_eq!(subtitle("/a", None), "a");
        assert_eq!(subtitle("/", None), "");
    }

    #[core::prelude::v1::test]
    fn byte_counts_read_as_finder_does() {
        assert_eq!(mac::byte_count_string(1_200_000), "1.2 MB");
    }

    #[core::prelude::v1::test]
    fn aspect_follows_the_tracks_rotation() {
        // Landscape, upright.
        assert_eq!(oriented_aspect(1920., 1080., 1., 0., 0., 1.), Some(1920. / 1080.));
        // Filmed upright on a phone: rotated 90 degrees.
        let portrait = oriented_aspect(1920., 1080., 0., 1., -1., 0.).unwrap();
        assert!((portrait - 1080. / 1920.).abs() < 1e-5);
        assert_eq!(oriented_aspect(0., 1080., 1., 0., 0., 1.), None);
    }

    #[core::prelude::v1::test]
    fn video_fits_centered_in_its_area() {
        let (x, y, w, h) = fit_rect((40., 40., 800., 800.), 2.);
        assert_eq!((x, y, w, h), (40., 240., 800., 400.));
        let (x, y, w, h) = fit_rect((0., 0., 800., 400.), 1.);
        assert_eq!((x, y, w, h), (200., 0., 400., 400.));
        assert_eq!(fit_rect((1., 2., 0., 10.), 1.), (1., 2., 0., 0.));
    }

    #[core::prelude::v1::test]
    fn only_animated_formats_go_to_gpui() {
        assert!(animates(Path::new("/a/B.GIF")));
        assert!(animates(Path::new("/a/b.webp")));
        assert!(!animates(Path::new("/a/b.png")));
        assert!(!animates(Path::new("/a/b.heic")));
    }

    #[core::prelude::v1::test]
    fn more_menu_order_matches_its_actions() {
        let titles: Vec<_> = more_menu_rows()
            .into_iter()
            .filter_map(|r| match r {
                MenuRow::Item { title, .. } => Some(title),
                _ => None,
            })
            .collect();
        assert_eq!(titles, ["Copy Markdown Link", "Open in Default App", "Reveal in Finder", "Move to Trash"]);
        assert_eq!(MoreAction::ALL.len(), titles.len());
    }
}
