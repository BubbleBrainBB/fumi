//! Settings as one sheet of paper over the dimmed window: Appearance, Writing,
//! Library and Publish read top to bottom. A port of `SettingsView.swift`.
//! Every change goes to `NSUserDefaults` under the Swift app's key names, then to
//! the closure the root gave, which applies it to the window.

use crate::CloseSettings;
use crate::anim::{Fades, Transition};
use fumi_ui::fonts::family_font;
use crate::controls::{Probes, chevron_down, disclosure, hairline, mix, tabular_digits, text_width, tip, truncate_middle};
use crate::font_picker::FontPicker;
use crate::mac;
use crate::scroller::{ScrollTarget, Scroller};
use cocoa::base::id;
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use fumi_ui::native::{self, MenuRow};
use fumi_ui::settings as s;
use fumi_ui::settings::keys;
use fumi_ui::text_field::{FieldEvent, TextField};
use fumi_ui::theme::{self, AppTheme, AppearanceMode, INK_ACCENT, t};
use fumi_workspace::Workspace;
use gpui::prelude::*;
use gpui::*;
use objc::{class, msg_send, sel, sel_impl};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SHEET_WIDTH: f32 = 560.;
pub const SHEET_RADIUS: f32 = 14.;
const FADE_SECS: f32 = 0.16;
const APPEAR_SCALE: f32 = 0.97;
const FOLD_SECS: f32 = 0.2;
const ROW_MIN_HEIGHT: f32 = 36.;
const SIDE_PADDING: f32 = 28.;
const SPACING_BEFORE_CONTROL: f32 = 44.;

/// As tall as its content, never closer than 40 to the window's edges, at least 260.
pub fn sheet_max_height(window_height: f32) -> f32 {
    (window_height - 80.).max(260.)
}

/// What changed, so the root can apply no more than it must. All of it is in
/// `NSUserDefaults` by the time this is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsChange {
    /// Theme, mode or accent: recompute tokens and redraw the window.
    Appearance,
    /// Typeface, size, line spacing or length, spelling, punctuation, slash commands.
    Writing,
    /// Publishing tools and their options: redraw an open preview.
    Publish,
}

pub type OnChange = Rc<dyn Fn(SettingsChange, &mut Window, &mut App)>;

// The options, as Swift names and stores them.

pub const ACCENTS: [(&str, &str); 5] =
    [("Blue", "#1D6FC2"), ("Ink", INK_ACCENT), ("Green", "#448361"), ("Purple", "#9065B0"), ("Red", "#C4402F")];
pub const LINE_SPACINGS: [(&str, f64); 3] = [("Compact", 1.4), ("Cozy", 1.7), ("Relaxed", 2.0)];
pub const LINE_LENGTHS: [(&str, &str); 3] = [("Narrow", "narrow"), ("Medium", "medium"), ("Wide", "wide")];
pub const WECHAT_THEMES: [(&str, &str); 4] = [
    ("Black Red", "black-red"),
    ("Blueprint", "blueprint"),
    ("Songti", "songti-supplement"),
    ("Motion", "motion-journal"),
];
pub const XHS_TEMPLATES: [(&str, &str); 4] = [("Memo", "memo"), ("Grid", "grid"), ("Post", "post"), ("Post Dark", "postDark")];
pub const XHS_RATIOS: [&str; 2] = ["3:4", "1:1"];
pub const SIZE_RANGE: (f64, f64) = (12., 24.);

pub const SHORTCUTS: [(&str, &[(&str, &str)]); 4] = [
    (
        "Notes",
        &[
            ("New Note", "⌘N"),
            ("New Window", "⇧⌘N"),
            ("Open Folder…", "⌘O"),
            ("Search Notes", "⇧⌘F"),
            ("Toggle Sidebar", "⌥⌘S"),
            ("Back", "⌘["),
            ("Forward", "⌘]"),
            ("Settings", "⌘,"),
        ],
    ),
    (
        "Tabs",
        &[
            ("New Tab", "⌘T"),
            ("Close Tab", "⌘W"),
            ("Reopen Closed Tab", "⇧⌘T"),
            ("Show Next Tab", "⌃⇥"),
            ("Show Previous Tab", "⌃⇧⇥"),
            ("Pin / Unpin Tab", "⌥⌘P"),
        ],
    ),
    ("Format", &[("Bold", "⌘B"), ("Italic", "⌘I"), ("Insert Link", "⌘K"), ("Toggle To-do", "⌘L")]),
    ("Publish", &[("Show Preview", "⌃⌘P"), ("Copy for WeChat / Add Cards to Photos", "⌥⌘C")]),
];

const SHORTCUT_LABEL_HEIGHT: f32 = 8. + 15. + 2.;
const SHORTCUT_ROW_HEIGHT: f32 = 28.;

/// The unfolded shortcut list's height.
pub fn shortcuts_height() -> f32 {
    SHORTCUTS.iter().map(|(_, items)| SHORTCUT_LABEL_HEIGHT + items.len() as f32 * SHORTCUT_ROW_HEIGHT).sum()
}

/// Which preset a stored line height shows as: the nearest.
pub fn nearest_line_spacing(value: f64) -> usize {
    let mut best = 0;
    for (i, (_, v)) in LINE_SPACINGS.iter().enumerate() {
        if (v - value).abs() < (LINE_SPACINGS[best].1 - value).abs() {
            best = i;
        }
    }
    best
}

pub fn mode_label(mode: AppearanceMode) -> &'static str {
    match mode {
        AppearanceMode::Auto => "Match System",
        other => other.title(),
    }
}

pub fn stepped(value: f64, delta: f64) -> f64 {
    (value + delta).clamp(SIZE_RANGE.0, SIZE_RANGE.1)
}

/// `~` for the home folder at the start of a path.
pub fn abbreviate_home(path: &str, home: &str) -> String {
    if !home.is_empty() {
        if path == home {
            return "~".into();
        }
        if let Some(rest) = path.strip_prefix(home) {
            if rest.starts_with('/') {
                return format!("~{rest}");
            }
        }
    }
    path.to_string()
}

/// The first letter on the avatar's quiet circle, uppercased.
pub fn avatar_initial(name: &str, full_name: &str) -> String {
    let source = if name.is_empty() { full_name } else { name };
    source.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

/// Copies a picture into Fumi's own folder, so moving or deleting the original
/// doesn't take it off the cards. Returns the copy, named `xhs-avatar-<time>.<ext>`.
pub fn install_avatar(source: &Path, folder: &Path, seconds: u64) -> std::io::Result<PathBuf> {
    let ext = source.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let target = folder.join(format!("xhs-avatar-{seconds}.{ext}"));
    std::fs::create_dir_all(folder)?;
    std::fs::copy(source, &target)?;
    Ok(target)
}

/// The migration from the old multi-theme build: its accents went with its themes.
/// Run once at launch, before the first `theme::apply_from_settings`.
pub fn migrate_old_settings() {
    if s::has(keys::OLD_APP_THEME) {
        s::remove(keys::OLD_APP_THEME);
        s::set_string(keys::ACCENT, theme::DEFAULT_ACCENT);
    }
}

/// Every stored value the sheet shows. Read when it opens and after each change,
/// not per frame.
#[derive(Clone, Debug)]
struct Values {
    theme: AppTheme,
    mode: AppearanceMode,
    accent: String,
    font: String,
    size: f64,
    line_height: f64,
    width: String,
    spell: bool,
    smart: bool,
    slash: bool,
    wechat: bool,
    wechat_theme: String,
    xhs: bool,
    template: String,
    ratio: String,
    page_numbers: bool,
    name: String,
    handle: String,
    avatar: String,
    end_card: bool,
    end_line: String,
}

impl Values {
    fn read() -> Self {
        let text = |key: &str, default: &str| s::get_string(key).unwrap_or_else(|| default.to_string());
        Values {
            theme: s::color_theme(),
            mode: s::appearance_mode(),
            accent: s::accent_hex(),
            font: s::editor_font(),
            size: s::font_size(),
            line_height: s::line_height(),
            width: s::editor_width(),
            spell: s::spell_check(),
            smart: s::smart_punctuation(),
            slash: s::slash_commands(),
            wechat: s::get_bool(keys::WECHAT_EXTENSION, false),
            wechat_theme: text(keys::WECHAT_THEME, "black-red"),
            xhs: s::get_bool(keys::XHS_EXTENSION, false),
            template: text(keys::XHS_TEMPLATE, "memo"),
            ratio: text(keys::XHS_RATIO, "3:4"),
            page_numbers: s::get_bool(keys::XHS_PAGE_NUMBERS, true),
            name: text(keys::XHS_NAME, ""),
            handle: text(keys::XHS_HANDLE, ""),
            avatar: text(keys::XHS_AVATAR, ""),
            end_card: s::get_bool(keys::XHS_END_CARD, false),
            end_line: text(keys::XHS_END_LINE, ""),
        }
    }
}

// The theme swatch -------------------------------------------------------------

thread_local! {
    static SWATCHES: RefCell<HashMap<(&'static str, bool), (usize, Option<Arc<RenderImage>>)>> = RefCell::new(HashMap::new());
}

fn ns_color(hex: u32, alpha: f64) -> id {
    let channel = |shift: u32| ((hex >> shift) & 0xFF) as f64 / 255.;
    unsafe { msg_send![class!(NSColor), colorWithSRGBRed: channel(16) green: channel(8) blue: channel(0) alpha: alpha] }
}

/// A theme as a tiny desk: its ground with a page on the right. `ThemeSwatchImage`.
fn make_swatch(theme_: AppTheme, dark: bool) -> id {
    let p = theme::palette(theme_, dark);
    let glass = theme_ == AppTheme::Glass;
    unsafe {
        let size = NSSize::new(18., 13.);
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithSize: size];
        let _: () = msg_send![image, lockFocus];
        let rect = NSRect::new(NSPoint::new(0., 0.), size);
        let desk: id = msg_send![class!(NSBezierPath), bezierPathWithRoundedRect: rect xRadius: 3f64 yRadius: 3f64];
        if glass {
            let (a, b) = if dark { (0x2B3038, 0x302C35) } else { (0xE3EAF1, 0xF5F2F7) };
            let gradient: id = msg_send![class!(NSGradient), alloc];
            let gradient: id = msg_send![gradient, initWithStartingColor: ns_color(a, 1.) endingColor: ns_color(b, 1.)];
            let _: () = msg_send![gradient, drawInBezierPath: desk angle: -45f64];
            let _: () = msg_send![gradient, release];
        } else {
            let _: () = msg_send![ns_color(p.ground, 1.), setFill];
            let _: () = msg_send![desk, fill];
        }
        let page_rect = NSRect::new(NSPoint::new(6., 2.), NSSize::new(10., 9.));
        let page: id = msg_send![class!(NSBezierPath), bezierPathWithRoundedRect: page_rect xRadius: 1f64 yRadius: 1f64];
        let _: () = msg_send![ns_color(p.page, if glass { 0.75 } else { 1. }), setFill];
        let _: () = msg_send![page, fill];
        let edge = if dark { ns_color(0xFFFFFF, 0.16) } else { ns_color(0x000000, 0.14) };
        let _: () = msg_send![edge, setStroke];
        let inset = NSRect::new(NSPoint::new(0.25, 0.25), NSSize::new(17.5, 12.5));
        let outline: id = msg_send![class!(NSBezierPath), bezierPathWithRoundedRect: inset xRadius: 2.75f64 yRadius: 2.75f64];
        let _: () = msg_send![outline, setLineWidth: 0.5f64];
        let _: () = msg_send![outline, stroke];
        if p.page == p.ground {
            let _: () = msg_send![page, setLineWidth: 0.5f64];
            let _: () = msg_send![page, stroke];
        }
        let _: () = msg_send![image, unlockFocus];
        image
    }
}

fn swatch(theme_: AppTheme, dark: bool) -> (id, Option<Arc<RenderImage>>) {
    SWATCHES.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.entry((theme_.raw(), dark)).or_insert_with(|| {
            let image = make_swatch(theme_, dark);
            (image as usize, native::draw_to_render_image(image, NSSize::new(18., 13.), 2.))
        });
        (entry.0 as id, entry.1.clone())
    })
}

// The sheet --------------------------------------------------------------------

pub struct SettingsSheet {
    workspace: Entity<Workspace>,
    on_change: OnChange,
    /// The sheet's content as last laid out, for its appearing.
    sheet_height: Rc<Cell<f32>>,
    focus: FocusHandle,
    prev_focus: Option<FocusHandle>,
    shown: Transition,
    v: Values,
    hover: Fades<&'static str>,
    switches: Fades<&'static str>,
    folds: Fades<&'static str>,
    menu_open: Option<&'static str>,
    probes: Probes,
    scroll: ScrollHandle,
    scroller: Scroller,
    picker: Entity<FontPicker>,
    name_field: Entity<TextField>,
    handle_field: Entity<TextField>,
    end_line_field: Entity<TextField>,
    login_on: bool,
    login_error: Option<String>,
    avatar: Option<(String, Option<Arc<RenderImage>>)>,
    switch_size: Option<(f32, f32)>,
    full_name: String,
    home: String,
    version: String,
    _subs: Vec<Subscription>,
}

impl SettingsSheet {
    pub fn new(workspace: Entity<Workspace>, on_change: OnChange, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let v = Values::read();
        let full_name = mac::full_user_name();
        let make_field = |placeholder: String, text: &str, window: &mut Window, cx: &mut Context<Self>| {
            let text = text.to_string();
            cx.new(|cx| {
                let mut f = TextField::new(placeholder, window, cx);
                f.set_text(text, cx);
                f
            })
        };
        let name_field = make_field(full_name.clone(), &v.name, window, cx);
        let handle_field = make_field("@name".into(), &v.handle, window, cx);
        let end_line_field = make_field("One line to close with".into(), &v.end_line, window, cx);
        let this = cx.weak_entity();
        let picker = {
            let on_select: crate::font_picker::OnSelect = Rc::new(move |name, window, cx| {
                let _ = this.update(cx, |sheet, cx| {
                    sheet.change(SettingsChange::Writing, window, cx, |_| s::set_string(keys::EDITOR_FONT, &name));
                });
            });
            let font = v.font.clone();
            cx.new(|cx| FontPicker::new(font, on_select, window, cx))
        };
        let mut subs = vec![cx.observe_in(&workspace, window, |this, _, window, cx| this.workspace_changed(window, cx))];
        for (field, key) in [
            (&name_field, keys::XHS_NAME),
            (&handle_field, keys::XHS_HANDLE),
            (&end_line_field, keys::XHS_END_LINE),
        ] {
            subs.push(cx.subscribe_in(field, window, move |this, field, event: &FieldEvent, window, cx| match event {
                FieldEvent::Changed => {
                    let text = field.read(cx).text().to_string();
                    this.change(SettingsChange::Publish, window, cx, |_| s::set_string(key, &text));
                }
                FieldEvent::Cancel => this.close(cx),
                _ => {}
            }));
        }
        let scroll = ScrollHandle::new();
        SettingsSheet {
            workspace,
            on_change,
            sheet_height: Rc::new(Cell::new(0.)),
            focus: cx.focus_handle(),
            prev_focus: None,
            shown: Transition::new(false, FADE_SECS),
            v,
            hover: Fades::new(0.12),
            switches: Fades::new(0.12),
            folds: Fades::new(FOLD_SECS),
            menu_open: None,
            probes: Probes::default(),
            scroller: Scroller::new(ScrollTarget::Handle(scroll.clone())),
            scroll,
            picker,
            name_field,
            handle_field,
            end_line_field,
            login_on: false,
            login_error: None,
            avatar: None,
            switch_size: None,
            full_name,
            home: std::env::var("HOME").unwrap_or_default(),
            version: mac::bundle_short_version(),
            _subs: subs,
        }
    }

    fn workspace_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let w = self.workspace.read(cx);
        let wanted = w.model.show_settings && w.store.is_open();
        if wanted == self.shown.target() {
            cx.notify();
            return;
        }
        self.shown.set(wanted, Instant::now());
        if wanted {
            self.prepare(window, cx);
        } else {
            self.picker.update(cx, |p, cx| p.close(window, cx));
            // Back where it was, unless what closed the sheet already moved it.
            let handle = self.prev_focus.take();
            if self.focus.contains_focused(window, cx) {
                if let Some(handle) = handle {
                    window.focus(&handle, cx);
                }
            }
        }
        cx.notify();
    }

    fn prepare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prev_focus = window.focused(cx);
        window.focus(&self.focus, cx);
        self.refresh(cx);
        self.login_on = mac::login_item_enabled();
        self.login_error = None;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.folds.snap("shortcuts", false);
        for (key, on) in [
            ("wechat", self.v.wechat),
            ("xhs", self.v.xhs),
            ("end-card", self.v.end_card),
            ("sw-spell", self.v.spell),
            ("sw-smart", self.v.smart),
            ("sw-slash", self.v.slash),
            ("sw-login", self.login_on),
            ("sw-wechat", self.v.wechat),
            ("sw-xhs", self.v.xhs),
            ("sw-pages", self.v.page_numbers),
            ("sw-end", self.v.end_card),
        ] {
            if key.starts_with("sw-") {
                self.switches.snap(key, on);
            } else {
                self.folds.snap(key, on);
            }
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.v = Values::read();
        let font = self.v.font.clone();
        self.picker.update(cx, |p, cx| p.set_selection(font, cx));
    }

    /// Writes through, re-reads, and tells the root.
    fn change(
        &mut self,
        change: SettingsChange,
        window: &mut Window,
        cx: &mut Context<Self>,
        write: impl FnOnce(&mut Self),
    ) {
        write(self);
        self.refresh(cx);
        (self.on_change.clone())(change, window, cx);
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.workspace.update(cx, |w, cx| {
            w.model.show_settings = false;
            cx.notify();
        });
    }

    /// Opens a pop-up below its control from a task of its own: AppKit's menu loop
    /// must not run under an `App` borrow.
    fn present_menu(
        &mut self,
        key: &'static str,
        rows: Vec<MenuRow>,
        apply: impl FnOnce(&mut Self, usize, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.probes.get(key) else { return };
        let at = point(bounds.origin.x, bounds.bottom() + px(3.));
        let ns_window = native::ns_window_of(window) as usize;
        self.menu_open = Some(key);
        cx.notify();
        cx.spawn_in(window, async move |this: WeakEntity<Self>, cx| {
            // A frame for the control's open look.
            cx.background_executor().timer(Duration::from_millis(16)).await;
            let chosen = native::popup_menu_on(ns_window as id, &rows, at);
            let _ = this.update_in(cx, |this, window, cx| {
                this.menu_open = None;
                if let Some(index) = chosen {
                    apply(this, index, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Controls ----------------------------------------------------------------

    fn control_fill(&self, key: &'static str, now: Instant) -> Hsla {
        let v = if self.menu_open == Some(key) { 1. } else { self.hover.value(&key, now) };
        fumi_ui::button::control_fill(v)
    }

    fn pop_up(
        &self,
        key: &'static str,
        title: String,
        icon: Option<Arc<RenderImage>>,
        now: Instant,
        cx: &mut Context<Self>,
        open: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let c = t();
        div()
            .id(key)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .h(px(24.))
            .min_w(px(120.))
            .px(px(10.))
            .rounded(px(6.))
            .bg(self.control_fill(key, now))
            .cursor_pointer()
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                this.hover.set(key, *hovering, Instant::now());
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, window, cx| open(this, window, cx)))
            .child(self.probes.probe(key))
            .when_some(icon, |d, image| d.child(img(image).w(px(18.)).h(px(13.)).flex_none()))
            .child(div().text_size(px(13.)).line_height(px(16.)).text_color(c.text).whitespace_nowrap().child(title))
            .child(div().flex_1())
            .child(chevron_down(c.text3))
    }

    /// A pop-up over plain strings, stored under `key`.
    fn string_pop_up(
        &self,
        id: &'static str,
        options: Vec<(&'static str, &'static str)>,
        current: &str,
        change: SettingsChange,
        key: &'static str,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = options.iter().find(|(_, v)| *v == current).map(|(l, _)| l.to_string()).unwrap_or_default();
        let current = current.to_string();
        self.pop_up(id, label, None, now, cx, move |this, window, cx| {
            let rows = options
                .iter()
                .map(|(l, v)| MenuRow::Choice { title: l.to_string(), checked: *v == current, image: None })
                .collect();
            let options = options.clone();
            this.present_menu(
                id,
                rows,
                move |this, i, window, cx| {
                    if let Some((_, value)) = options.get(i) {
                        this.change(change, window, cx, |_| s::set_string(key, value));
                    }
                },
                window,
                cx,
            );
        })
    }

    fn switch(
        &mut self,
        key: &'static str,
        on: bool,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
        toggle: impl Fn(&mut Self, bool, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let c = t();
        let (w, h) = *self.switch_size.get_or_insert_with(mac::small_switch_size);
        self.switches.set(key, on, now);
        let v = self.switches.value(&key, now);
        if self.switches.is_animating(now) {
            window.request_animation_frame();
        }
        let off = if c.dark { hsla(0., 0., 1., 0.14) } else { hsla(0., 0., 0., 0.09) };
        let inset = 1.5;
        let knob = h - inset * 2.;
        div()
            .id(key)
            .relative()
            .flex_none()
            .w(px(w))
            .h(px(h))
            .rounded(px(h / 2.))
            .bg(mix(off, c.accent, v))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| toggle(this, !on, window, cx)))
            .child(
                div()
                    .absolute()
                    .top(px(inset))
                    .left(px(inset + (w - knob - inset * 2.) * v))
                    .w(px(knob))
                    .h(px(knob))
                    .rounded_full()
                    .bg(hsla(0., 0., 1., 1.))
                    .shadow(vec![BoxShadow {
                        color: hsla(0., 0., 0., 0.28),
                        offset: point(px(0.), px(0.5)),
                        blur_radius: px(1.),
                        spread_radius: px(0.),
                        inset: false,
                    }]),
            )
    }

    /// A boolean stored under `key`, as a switch.
    fn bool_switch(
        &mut self,
        id: &'static str,
        on: bool,
        change: SettingsChange,
        key: &'static str,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.switch(id, on, now, window, cx, move |this, value, window, cx| {
            this.change(change, window, cx, |_| s::set_bool(key, value));
        })
    }

    fn fill_button(
        &self,
        id: &'static str,
        title: &'static str,
        now: Instant,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let c = t();
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .h(px(24.))
            .px(px(10.))
            .rounded(px(6.))
            .bg(self.control_fill(id, now))
            .cursor_pointer()
            .text_size(px(13.))
            .line_height(px(16.))
            .text_color(c.text)
            .whitespace_nowrap()
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                this.hover.set(id, *hovering, Instant::now());
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, window, cx| action(this, window, cx)))
            .child(title)
    }

    fn text_field(&self, field: &Entity<TextField>, width: f32) -> impl IntoElement + use<> {
        let c = t();
        div()
            .flex()
            .flex_none()
            .items_center()
            .w(px(width))
            .h(px(24.))
            .px(px(8.))
            .rounded(px(6.))
            .bg(c.inline_bg)
            .text_color(c.text)
            .child(field.clone())
    }

    fn step_button(
        &self,
        id: &'static str,
        symbol: &'static str,
        enabled: bool,
        delta: f64,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let c = t();
        let hover = if enabled { self.hover.value(&id, now) } else { 0. };
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(26.))
            .h(px(24.))
            .rounded(px(6.))
            .bg(fumi_ui::button::quiet_fill(hover, false))
            .opacity(if enabled { 1. } else { 0.35 })
            .when(enabled, |d| {
                d.cursor_pointer().on_click(cx.listener(move |this, _, window, cx| {
                    let to = stepped(this.v.size, delta);
                    this.change(SettingsChange::Writing, window, cx, |_| s::set_f64(keys::FONT_SIZE, to));
                }))
            })
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                this.hover.set(id, *hovering, Instant::now());
                cx.notify();
            }))
            .child(fumi_ui::icons::symbol(symbol, 10., native::weight::SEMIBOLD, c.text2))
    }

    // Groups ------------------------------------------------------------------

    fn appearance(&mut self, now: Instant, window: &Window, cx: &mut Context<Self>) -> Div {
        let c = t();
        let dark = c.dark;
        let current = self.v.theme;
        let (_, icon) = swatch(current, dark);
        let theme_popup = self.pop_up("theme", current.title().to_string(), icon, now, cx, move |this, window, cx| {
            let rows = AppTheme::ALL
                .iter()
                .map(|th| MenuRow::Choice { title: th.title().to_string(), checked: *th == current, image: Some(swatch(*th, dark).0) })
                .collect();
            this.present_menu(
                "theme",
                rows,
                |this, i, window, cx| {
                    let value = AppTheme::ALL[i];
                    this.change(SettingsChange::Appearance, window, cx, |_| s::set_string(keys::COLOR_THEME, value.raw()));
                },
                window,
                cx,
            );
        });
        let mode = self.v.mode;
        let mode_popup = self.pop_up("mode", mode_label(mode).to_string(), None, now, cx, move |this, window, cx| {
            let rows = AppearanceMode::ALL
                .iter()
                .map(|m| MenuRow::Choice { title: mode_label(*m).to_string(), checked: *m == mode, image: None })
                .collect();
            this.present_menu(
                "mode",
                rows,
                |this, i, window, cx| {
                    let value = AppearanceMode::ALL[i];
                    this.change(SettingsChange::Appearance, window, cx, |_| s::set_string(keys::APPEARANCE_MODE, value.raw()));
                },
                window,
                cx,
            );
        });
        let dots = ACCENTS.iter().enumerate().map(|(i, (name, hex))| {
            let is_ink = *hex == INK_ACCENT;
            let fill = if is_ink { c.text } else { theme::hex(theme::parse_hex(hex).unwrap_or(0x1D6FC2)) };
            let selected = self.v.accent.eq_ignore_ascii_case(hex);
            let hex = *hex;
            div()
                .id(("accent", i))
                .relative()
                .w(px(21.))
                .h(px(21.))
                .mx(px(-3.5))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .tooltip(tip(*name))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.change(SettingsChange::Appearance, window, cx, |_| s::set_string(keys::ACCENT, hex));
                }))
                .child(div().w(px(14.)).h(px(14.)).rounded_full().bg(fill))
                .when(selected, |d| {
                    d.child(div().absolute().inset_0().rounded_full().border(px(1.5)).border_color(fill))
                })
        });
        let accent = div().flex().flex_none().items_center().gap(px(11.)).children(dots);
        group("Appearance", true, window)
            .child(row("Theme", None, theme_popup))
            .child(row("Mode", None, mode_popup))
            .child(row("Accent", None, accent))
    }

    fn writing(&mut self, now: Instant, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = t();
        let size = self.v.size;
        let stepper = div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(24.))
            .rounded(px(6.))
            .bg(c.inline_bg)
            .child(self.step_button("size-minus", "minus", size > SIZE_RANGE.0, -1., now, cx))
            .child(
                div()
                    .min_w(px(24.))
                    .flex()
                    .justify_center()
                    .text_size(px(13.))
                    .line_height(px(16.))
                    .text_color(c.text)
                    .font(Font { features: tabular_digits(), ..family_font("System") })
                    .child(format!("{}", size as i64)),
            )
            .child(self.step_button("size-plus", "plus", size < SIZE_RANGE.1, 1., now, cx));
        let spacing = nearest_line_spacing(self.v.line_height);
        let spacing_popup = self.pop_up("line-spacing", LINE_SPACINGS[spacing].0.to_string(), None, now, cx, move |this, window, cx| {
            let rows = LINE_SPACINGS
                .iter()
                .enumerate()
                .map(|(i, (l, _))| MenuRow::Choice { title: l.to_string(), checked: i == spacing, image: None })
                .collect();
            this.present_menu(
                "line-spacing",
                rows,
                |this, i, window, cx| {
                    this.change(SettingsChange::Writing, window, cx, |_| s::set_f64(keys::LINE_HEIGHT, LINE_SPACINGS[i].1));
                },
                window,
                cx,
            );
        });
        let length = self.string_pop_up(
            "line-length",
            LINE_LENGTHS.to_vec(),
            &self.v.width.clone(),
            SettingsChange::Writing,
            keys::EDITOR_WIDTH,
            now,
            cx,
        );
        let (spell, smart, slash) = (self.v.spell, self.v.smart, self.v.slash);
        let spell = self.bool_switch("sw-spell", spell, SettingsChange::Writing, keys::SPELL_CHECK, now, window, cx);
        let smart = self.bool_switch("sw-smart", smart, SettingsChange::Writing, keys::SMART_PUNCTUATION, now, window, cx);
        let slash = self.bool_switch("sw-slash", slash, SettingsChange::Writing, keys::SLASH_COMMANDS, now, window, cx);
        group("Writing", false, window)
            .child(row("Typeface", None, self.picker.clone()))
            .child(row("Size", None, stepper))
            .child(row("Line spacing", None, spacing_popup))
            .child(row("Line length", None, length))
            .child(row("Check spelling", None, spell))
            .child(row("Smart quotes and dashes", None, smart))
            .child(row("Slash commands", None, slash))
    }

    fn library(&mut self, now: Instant, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let path = self.workspace.read(cx).workspace_path();
        let title = path
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "No folder open".into());
        let show = path.is_some().then(|| {
            let reveal = path.clone();
            self.fill_button("show-in-finder", "Show in Finder", now, cx, move |_, _, _| {
                if let Some(p) = &reveal {
                    native::reveal_in_finder(p);
                }
            })
        });
        let change = self.fill_button("change-folder", "Change…", now, cx, |_, window, cx| {
            cx.spawn_in(window, async move |this: WeakEntity<Self>, cx| {
                let Some(url) = native::choose_notes_folder(None) else { return };
                let _ = this.update(cx, |this, cx| {
                    this.workspace.update(cx, |w, cx| {
                        w.open_workspace(&url, cx);
                        w.model.open_note(None, false);
                        w.navigated(cx);
                    });
                });
            })
            .detach();
        });
        let controls = div().flex().flex_none().items_center().gap(px(8.)).children(show).child(change);
        let room = SHEET_WIDTH - SIDE_PADDING * 2. - SPACING_BEFORE_CONTROL;
        let buttons = text_width("Change…", 13., window)
            + 20.
            + if path.is_some() { text_width("Show in Finder", 13., window) + 28. } else { 0. };
        let detail = path.as_ref().map(|p| {
            let abbreviated = abbreviate_home(&p.to_string_lossy(), &self.home);
            truncate_middle(&abbreviated, (room - buttons).max(40.), |t| text_width(t, 12., window))
        });
        let (login_on, login_error) = (self.login_on, self.login_error.clone());
        let login_detail = login_error.map(|e| {
            let room = room - self.switch_size.map_or(32., |s| s.0);
            truncate_middle(&e, room.max(40.), |t| text_width(t, 12., window))
        });
        let login = self.switch("sw-login", login_on, now, window, cx, |this, value, _, cx| {
            if value != this.login_on {
                this.login_error = mac::set_login_item(value).err().map(|e| format!("Couldn't change this: {e}"));
                this.login_on = mac::login_item_enabled();
            }
            cx.notify();
        });
        group("Library", false, window)
            .child(row(title, detail, controls))
            .child(row("Open at login", login_detail, login))
    }

    fn avatar_image(&mut self) -> Option<Arc<RenderImage>> {
        let path = self.v.avatar.clone();
        if path.is_empty() {
            self.avatar = None;
            return None;
        }
        if self.avatar.as_ref().is_none_or(|(p, _)| *p != path) {
            let image = mac::load_image_file(Path::new(&path), 96.).map(|(image, _)| image);
            self.avatar = Some((path, image));
        }
        self.avatar.as_ref().and_then(|(_, image)| image.clone())
    }

    fn remove_avatar(&mut self) {
        if !self.v.avatar.is_empty() {
            let _ = std::fs::remove_file(&self.v.avatar);
        }
        s::set_string(keys::XHS_AVATAR, "");
    }

    fn choose_avatar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this: WeakEntity<Self>, cx| {
            let Some(source) = mac::choose_avatar_file() else { return };
            let Some(folder) = mac::support_folder() else { return };
            let seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let installed = install_avatar(&source, &folder, seconds);
            let _ = this.update_in(cx, |this, window, cx| match installed {
                Ok(target) => {
                    this.change(SettingsChange::Publish, window, cx, |this| {
                        this.remove_avatar();
                        s::set_string(keys::XHS_AVATAR, &target.to_string_lossy());
                    });
                }
                Err(_) => mac::beep(),
            });
        })
        .detach();
    }

    fn publish(&mut self, now: Instant, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (wechat, xhs) = (self.v.wechat, self.v.xhs);
        self.folds.set("wechat", wechat, now);
        self.folds.set("xhs", xhs, now);
        self.folds.set("end-card", self.v.end_card, now);
        let wechat_on = self.bool_switch("sw-wechat", wechat, SettingsChange::Publish, keys::WECHAT_EXTENSION, now, window, cx);
        let xhs_on = self.bool_switch("sw-xhs", xhs, SettingsChange::Publish, keys::XHS_EXTENSION, now, window, cx);
        let wechat_p = self.folds.value(&"wechat", now);
        let xhs_p = self.folds.value(&"xhs", now);
        let end_p = self.folds.value(&"end-card", now);

        let wechat_row = (wechat_p > 0.).then(|| {
            let popup = self.string_pop_up(
                "wechat-theme",
                WECHAT_THEMES.to_vec(),
                &self.v.wechat_theme.clone(),
                SettingsChange::Publish,
                keys::WECHAT_THEME,
                now,
                cx,
            );
            fold(ROW_MIN_HEIGHT, wechat_p).child(row("Theme", None, popup))
        });
        let xhs_rows = (xhs_p > 0.).then(|| {
            let template = self.string_pop_up(
                "xhs-template",
                XHS_TEMPLATES.to_vec(),
                &self.v.template.clone(),
                SettingsChange::Publish,
                keys::XHS_TEMPLATE,
                now,
                cx,
            );
            let ratio = self.string_pop_up(
                "xhs-ratio",
                XHS_RATIOS.iter().map(|r| (*r, *r)).collect(),
                &self.v.ratio.clone(),
                SettingsChange::Publish,
                keys::XHS_RATIO,
                now,
                cx,
            );
            let picture = self.avatar_image();
            let has_avatar = !self.v.avatar.is_empty();
            let initial = avatar_initial(&self.v.name, &self.full_name);
            let remove = has_avatar.then(|| {
                self.fill_button("avatar-remove", "Remove", now, cx, |this, window, cx| {
                    this.change(SettingsChange::Publish, window, cx, |this| this.remove_avatar());
                })
            });
            let choose = self.fill_button("avatar-choose", "Choose…", now, cx, |this, window, cx| this.choose_avatar(window, cx));
            let avatar = div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.))
                .child(avatar_dot(picture, initial))
                .children(remove)
                .child(choose);
            let name = self.text_field(&self.name_field, 180.);
            let handle = self.text_field(&self.handle_field, 180.);
            let (pages, end_card) = (self.v.page_numbers, self.v.end_card);
            let pages = self.bool_switch("sw-pages", pages, SettingsChange::Publish, keys::XHS_PAGE_NUMBERS, now, window, cx);
            let end = self.bool_switch("sw-end", end_card, SettingsChange::Publish, keys::XHS_END_CARD, now, window, cx);
            let end_line = (end_p > 0.).then(|| {
                let field = self.text_field(&self.end_line_field, 220.);
                fold(ROW_MIN_HEIGHT, end_p).child(row("End card line", None, field))
            });
            fold(ROW_MIN_HEIGHT * (7. + end_p), xhs_p)
                .child(row("Template", None, template))
                .child(row("Size", None, ratio))
                .child(row("Avatar", None, avatar))
                .child(row("Name", None, name))
                .child(row("Handle", None, handle))
                .child(row("Page numbers", None, pages))
                .child(row("End card", None, end))
                .children(end_line)
        });
        group("Publish", false, window)
            .child(row("WeChat Article", None, wechat_on))
            .children(wechat_row)
            .child(row("Xiaohongshu Cards", None, xhs_on))
            .children(xhs_rows)
    }

    fn shortcuts(&mut self, now: Instant, window: &Window, cx: &mut Context<Self>) -> Div {
        let c = t();
        let p = self.folds.value(&"shortcuts", now);
        let list = (p > 0.).then(|| {
            let groups = SHORTCUTS.iter().map(|(title, items)| {
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .child(
                        div()
                            .flex_none()
                            .pt(px(8.))
                            .pb(px(2.))
                            .text_size(px(12.))
                            .line_height(px(15.))
                            .text_color(c.faint_text)
                            .child(*title),
                    )
                    .children(items.iter().map(|(name, keys)| {
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_between()
                            .h(px(SHORTCUT_ROW_HEIGHT))
                            .text_size(px(13.))
                            .child(div().text_color(c.text).child(*name))
                            .child(div().text_color(c.faint_text).child(*keys))
                    }))
            });
            fold(shortcuts_height(), p).pl(px(16.)).children(groups)
        });
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .py(px(12.))
            .child(div().absolute().top_0().left_0().right_0().child(hairline(window)))
            .child(
                div()
                    .id("shortcuts-toggle")
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.))
                    .h(px(32.))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let on = !this.folds.is_on(&"shortcuts");
                        this.folds.set("shortcuts", on, Instant::now());
                        cx.notify();
                    }))
                    .child(disclosure(p, c.text3))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c.text)
                            .child("Keyboard shortcuts"),
                    ),
            )
            .children(list)
    }
}

/// A group: its name in bold, 32 tall, over its rows; a device-pixel hairline above
/// all but the first.
fn group(title: &'static str, is_first: bool, window: &Window) -> Div {
    let c = t();
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_none()
        .py(px(12.))
        .when(!is_first, |d| d.child(div().absolute().top_0().left_0().right_0().child(hairline(window))))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .h(px(32.))
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c.text)
                .child(title),
        )
}

/// One setting: its name left, with what it points at under it, the control right.
fn row(title: impl Into<SharedString>, detail: Option<String>, control: impl IntoElement) -> Div {
    let c = t();
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(16.))
        .min_h(px(ROW_MIN_HEIGHT))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .gap(px(1.))
                .child(
                    div()
                        .text_size(px(13.))
                        .line_height(px(16.))
                        .text_color(c.text)
                        .truncate()
                        .child(title.into()),
                )
                .when_some(detail, |d, detail| {
                    d.child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(15.))
                            .text_color(c.faint_text)
                            .whitespace_nowrap()
                            .child(detail),
                    )
                }),
        )
        .child(control)
}

/// Rows that open and close under a switch: indented 16, fading as they unfold.
fn fold(full_height: f32, p: f32) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .pl(px(16.))
        .h(px(full_height * p))
        .overflow_hidden()
        .opacity(p)
}

/// The avatar as the Post template shows it: the picture, or the name's first
/// letter on a quiet circle.
fn avatar_dot(picture: Option<Arc<RenderImage>>, initial: String) -> impl IntoElement {
    let c = t();
    let (ink, ground) = if c.dark { (0xC9C9C6, 0x34363B) } else { (0x6B6457, 0xE4DFD4) };
    let dot = div().w(px(24.)).h(px(24.)).flex_none().rounded_full().overflow_hidden();
    match picture {
        Some(image) => dot.child(img(image).size_full().rounded_full().object_fit(ObjectFit::Cover)),
        None => dot
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::hex(ground))
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::hex(ink))
            .child(initial),
    }
}

impl Render for SettingsSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        if self.shown.is_gone(now) {
            return div().absolute().size_0().into_any_element();
        }
        let c = t();
        let p = self.shown.value(now);
        let strength = self.workspace.read(cx).model.scrim_strength(c.glass);
        let strength = if strength > 0. { strength } else if c.glass { 0.45 } else { 1. };

        let appearance = self.appearance(now, window, cx);
        let writing = self.writing(now, window, cx);
        let library = self.library(now, window, cx);
        let publish = self.publish(now, window, cx);
        let shortcuts = self.shortcuts(now, window, cx);
        let version = format!("Fumi {}", self.version);
        let thumb = self.scroller.thumb(|this: &mut Self| &mut this.scroller, now, window, cx);
        if self.shown.is_animating(now) || self.hover.is_animating(now) || self.folds.is_animating(now) {
            window.request_animation_frame();
        }

        let dark = c.dark;
        // There's no blur to set the text on in Glass, so the page stays nearly solid.
        let (surface, edge) = if c.glass {
            let page = c.page.opacity(if dark { 0.88 } else { 0.92 });
            let edge = match (fumi_ui::native::has_liquid_glass(), dark) {
                (true, true) => 0.12,
                (true, false) => 0.75,
                (false, true) => 0.14,
                (false, false) => 0.9,
            };
            (page, hsla(0., 0., 1., edge))
        } else {
            (c.page, if dark { hsla(0., 0., 1., 0.07) } else { hsla(0., 0., 0., 0.05) })
        };
        let scrollable = self.scroll.max_offset().y > px(0.);
        let content = div()
            .flex()
            .flex_col()
            .w_full()
            .px(px(SIDE_PADDING))
            .pb(px(20.))
            .child(appearance)
            .child(writing)
            .child(library)
            .child(publish)
            .child(shortcuts)
            .child(
                div()
                    .flex_none()
                    .pt(px(4.))
                    .text_size(px(12.))
                    .line_height(px(15.))
                    .text_color(c.faint_text)
                    .child(version),
            );
        let body = div()
            .relative()
            .flex()
            .flex_col()
            .flex_shrink(1.)
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .id("settings-scroll")
                    .flex_shrink(1.)
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(content),
            )
            .when(scrollable, |d| {
                d.child(
                    div().absolute().left_0().right_0().bottom_0().h(px(32.)).bg(linear_gradient(
                        0.,
                        linear_color_stop(c.page, 0.),
                        linear_color_stop(c.page.opacity(0.), 1.),
                    )),
                )
            })
            .children(thumb);
        let inner = div()
            .relative()
            .w(px(SHEET_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .max_h(px(sheet_max_height(f32::from(window.viewport_size().height)) - 2.))
            .child(
                div()
                    .flex_none()
                    .pt(px(24.))
                    .px(px(SIDE_PADDING))
                    .pb(px(4.))
                    .text_size(px(20.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c.text)
                    .child("Settings"),
            )
            .child(body);
        // The box grows from 97% around content that stays still. Its height is
        // the content's, known once it's been laid out.
        let scale = APPEAR_SCALE + (1. - APPEAR_SCALE) * p;
        let inner_height = self.sheet_height.clone();
        let inner = inner.child(
            canvas(move |bounds, _, _| inner_height.set(f32::from(bounds.size.height)), |_, _, _, _| {}).absolute().size_full(),
        );
        let sheet = div()
            .id("settings-sheet")
            .occlude()
            .w(px(SHEET_WIDTH * scale))
            .when(p < 1. && self.sheet_height.get() > 0., |d| d.h(px(self.sheet_height.get() * scale)))
            .flex()
            .justify_center()
            .items_center()
            .overflow_hidden()
            .rounded(px(SHEET_RADIUS))
            .bg(surface)
            .border_1()
            .border_color(edge)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., if dark { 0.45 } else { 0.16 }),
                offset: point(px(0.), px(30.)),
                blur_radius: px(40.),
                spread_radius: px(0.),
                inset: false,
            }])
            .on_click(|_, _, _| {})
            .child(inner);
        div()
            .id("settings-scrim")
            .key_context("FumiSettings")
            .track_focus(&self.focus)
            .absolute()
            .inset_0()
            .occlude()
            .opacity(p)
            .bg(c.scrim.opacity(strength))
            .flex()
            .items_center()
            .justify_center()
            .font(family_font("System"))
            .on_action(cx.listener(|this, _: &CloseSettings, window, cx| {
                if this.picker.read(cx).is_open() {
                    this.picker.update(cx, |p, cx| p.close(window, cx));
                } else {
                    this.close(cx);
                }
            }))
            .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
            .child(sheet)
            .into_any_element()
    }
}

/// ⌘,. The sheet shows only with a library open, so on the welcome page this
/// displays nothing until one opens.
pub fn present(workspace: &Entity<Workspace>, cx: &mut App) {
    workspace.update(cx, |w, cx| {
        w.model.show_settings = true;
        cx.notify();
    });
}

/// The sidebar's Settings row.
pub fn toggle(workspace: &Entity<Workspace>, cx: &mut App) {
    workspace.update(cx, |w, cx| {
        w.model.show_settings = !w.model.show_settings;
        cx.notify();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn sheet_keeps_40_from_the_window_edges_and_260_tall() {
        assert_eq!(sheet_max_height(800.), 720.);
        assert_eq!(sheet_max_height(300.), 260.);
    }

    #[core::prelude::v1::test]
    fn nearest_preset_wins() {
        assert_eq!(nearest_line_spacing(1.7), 1);
        assert_eq!(nearest_line_spacing(1.0), 0);
        assert_eq!(nearest_line_spacing(1.5), 0);
        assert_eq!(nearest_line_spacing(1.9), 2);
        assert_eq!(nearest_line_spacing(3.0), 2);
    }

    #[core::prelude::v1::test]
    fn mode_labels() {
        assert_eq!(mode_label(AppearanceMode::Auto), "Match System");
        assert_eq!(mode_label(AppearanceMode::Dark), "Dark");
    }

    #[core::prelude::v1::test]
    fn size_steps_stay_in_range() {
        assert_eq!(stepped(16., 1.), 17.);
        assert_eq!(stepped(24., 1.), 24.);
        assert_eq!(stepped(12., -1.), 12.);
    }

    #[core::prelude::v1::test]
    fn home_abbreviates_only_on_a_folder_boundary() {
        assert_eq!(abbreviate_home("/Users/me/Notes", "/Users/me"), "~/Notes");
        assert_eq!(abbreviate_home("/Users/me", "/Users/me"), "~");
        assert_eq!(abbreviate_home("/Users/meg/Notes", "/Users/me"), "/Users/meg/Notes");
        assert_eq!(abbreviate_home("/Volumes/T7", "/Users/me"), "/Volumes/T7");
    }

    #[core::prelude::v1::test]
    fn avatar_initial_prefers_the_name() {
        assert_eq!(avatar_initial("wang", "Full Name"), "W");
        assert_eq!(avatar_initial("", "full name"), "F");
        assert_eq!(avatar_initial("", ""), "");
        assert_eq!(avatar_initial("文", "x"), "文");
    }

    #[core::prelude::v1::test]
    fn avatar_copy_is_named_by_time_with_a_lowercase_extension() {
        let root = std::env::temp_dir().join(format!("fumi-avatar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("Me.JPG");
        std::fs::write(&source, b"x").unwrap();
        let folder = root.join("support/Fumi");
        let target = install_avatar(&source, &folder, 123).unwrap();
        assert_eq!(target, folder.join("xhs-avatar-123.jpg"));
        assert_eq!(std::fs::read(&target).unwrap(), b"x");
        assert!(source.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[core::prelude::v1::test]
    fn avatar_copy_fails_for_a_missing_source() {
        let root = std::env::temp_dir().join(format!("fumi-avatar-miss-{}", std::process::id()));
        assert!(install_avatar(&root.join("gone.png"), &root.join("f"), 1).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[core::prelude::v1::test]
    fn unfolded_shortcuts_add_up() {
        // Four labels of 25 and twenty rows of 28.
        assert_eq!(shortcuts_height(), 4. * 25. + 20. * 28.);
    }

    #[core::prelude::v1::test]
    fn accent_and_option_values_match_swift() {
        assert_eq!(ACCENTS.map(|a| a.0), ["Blue", "Ink", "Green", "Purple", "Red"]);
        assert_eq!(WECHAT_THEMES[2].1, "songti-supplement");
        assert_eq!(XHS_TEMPLATES[3].1, "postDark");
    }
}
