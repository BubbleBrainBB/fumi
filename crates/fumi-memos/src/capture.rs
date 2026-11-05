//! Write a memo from anywhere: a global hotkey (⌃⌥N) or the menu bar icon shows a
//! small floating panel near the top of the screen. It never brings Fumi's windows
//! forward; ⌘↩ saves to today's file in the last opened library; esc, or a click
//! anywhere else, puts it away and keeps the draft. `QuickCapture.swift`.

use crate::field;
use crate::logic;
use crate::typo::{self, line_box};
use block::ConcreteBlock;
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use fumi_core::memo_format;
use fumi_ui::icons::{self, Glyph};
use fumi_ui::native::{self, ns, weight};
use fumi_ui::settings;
use fumi_ui::text_field::{FieldEvent, TextField};
use fumi_ui::theme::Tokens;
use fumi_workspace::now;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::prelude::*;
use gpui::*;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

actions!(memos, [
    /// File › New Memo: opens the capture panel.
    NewMemo,
    SaveCapture,
    CloseCapture
]);

pub const PANEL_SIZE: (f64, f64) = (600., 420.);
const FLOATING_LEVEL: i64 = 3;
const CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
const TRANSIENT: u64 = 1 << 3;
const FULL_SCREEN_AUXILIARY: u64 = 1 << 8;
const LEFT_MOUSE_DOWN_MASK: u64 = 1 << 1;
const RIGHT_MOUSE_DOWN_MASK: u64 = 1 << 3;
const LEFT_MOUSE_UP_MASK: u64 = 1 << 2;
const RIGHT_MOUSE_UP_MASK: u64 = 1 << 4;
const RIGHT_MOUSE_UP_TYPE: u64 = 4;
const SQUARE_LENGTH: f64 = -2.;

pub fn bind_keys(cx: &mut App) {
    let c = Some("QuickCapture");
    cx.bind_keys([
        KeyBinding::new("cmd-enter", SaveCapture, c),
        KeyBinding::new("escape", CloseCapture, c),
    ]);
}

// Messages from AppKit and Carbon callbacks, which have no `App` to hand.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Message {
    Toggle,
    Show,
    Hide,
    OpenApp,
    Quit,
}

static MESSAGES: OnceLock<UnboundedSender<Message>> = OnceLock::new();

fn send(message: Message) {
    if let Some(tx) = MESSAGES.get() {
        let _ = tx.unbounded_send(message);
    }
}

// The hotkey, through Carbon: it works without Accessibility access.

#[repr(C)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

#[repr(C)]
struct EventTypeSpec {
    class: u32,
    kind: u32,
}

const EVENT_CLASS_KEYBOARD: u32 = 0x6B65_7962; // 'keyb'
const EVENT_HOT_KEY_PRESSED: u32 = 5;
const KEY_N: u32 = 0x2D;
const CONTROL_KEY: u32 = 0x1000;
const OPTION_KEY: u32 = 0x0800;
const SIGNATURE: u32 = 0x4E54_4C59; // 'NTLY', as the Swift app registers it.

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
        count: u64,
        list: *const EventTypeSpec,
        data: *mut c_void,
        out: *mut *mut c_void,
    ) -> i32;
    fn RegisterEventHotKey(
        key: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: *mut c_void,
        options: u32,
        out: *mut *mut c_void,
    ) -> i32;
}

extern "C" fn hot_key_pressed(_: *mut c_void, _: *mut c_void, _: *mut c_void) -> i32 {
    send(Message::Toggle);
    0
}

fn install_hot_key() {
    let spec = EventTypeSpec { class: EVENT_CLASS_KEYBOARD, kind: EVENT_HOT_KEY_PRESSED };
    unsafe {
        let target = GetApplicationEventTarget();
        InstallEventHandler(target, hot_key_pressed, 1, &spec, std::ptr::null_mut(), std::ptr::null_mut());
        let mut hot_key = std::ptr::null_mut();
        let id = EventHotKeyId { signature: SIGNATURE, id: 1 };
        let status = RegisterEventHotKey(KEY_N, CONTROL_KEY | OPTION_KEY, id, target, 0, &mut hot_key);
        if status != 0 {
            eprintln!("couldn't register ⌃⌥N: {status}");
        }
    }
}

// The menu bar icon.

extern "C" fn status_clicked(this: &Object, _: Sel, sender: id) {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let event: id = msg_send![app, currentEvent];
        let kind: u64 = if event == nil { 0 } else { msg_send![event, type] };
        if kind != RIGHT_MOUSE_UP_TYPE {
            send(Message::Toggle);
            return;
        }
        let target = this as *const Object as id;
        let menu: id = msg_send![class!(NSMenu), new];
        let _: () = msg_send![menu, setAutoenablesItems: false];
        let add = |title: &str, action: Sel| {
            let item: id = msg_send![class!(NSMenuItem), alloc];
            let item: id = msg_send![item, initWithTitle: ns(title) action: action keyEquivalent: ns("")];
            let _: () = msg_send![item, setTarget: target];
            let _: () = msg_send![menu, addItem: item];
            let _: () = msg_send![item, release];
        };
        add("New Memo", sel!(fumiNewMemo:));
        let separator: id = msg_send![class!(NSMenuItem), separatorItem];
        let _: () = msg_send![menu, addItem: separator];
        add("Open Fumi", sel!(fumiOpenApp:));
        add("Quit Fumi", sel!(fumiQuit:));
        let item = STATUS_ITEM.with(std::cell::Cell::get);
        let _: () = msg_send![item, setMenu: menu];
        let _: () = msg_send![sender, performClick: nil];
        let _: () = msg_send![item, setMenu: nil];
        let _: () = msg_send![menu, release];
    }
}

extern "C" fn new_memo(_: &Object, _: Sel, _: id) {
    send(Message::Show);
}

extern "C" fn open_app(_: &Object, _: Sel, _: id) {
    send(Message::OpenApp);
}

extern "C" fn quit(_: &Object, _: Sel, _: id) {
    send(Message::Quit);
}

thread_local! {
    static STATUS_ITEM: std::cell::Cell<id> = const { std::cell::Cell::new(nil) };
}

fn status_target() -> id {
    let class_name = "FumiCaptureTarget";
    let class = Class::get(class_name).unwrap_or_else(|| {
        let mut decl = ClassDecl::new(class_name, class!(NSObject)).expect("class is new");
        unsafe {
            decl.add_method(sel!(fumiStatusClicked:), status_clicked as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(fumiNewMemo:), new_memo as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(fumiOpenApp:), open_app as extern "C" fn(&Object, Sel, id));
            decl.add_method(sel!(fumiQuit:), quit as extern "C" fn(&Object, Sel, id));
        }
        decl.register()
    });
    unsafe { msg_send![class, new] }
}

/// Returns the status button, whose clicks are the icon's own business.
fn install_status_item() -> id {
    unsafe {
        let bar: id = msg_send![class!(NSStatusBar), systemStatusBar];
        let item: id = msg_send![bar, statusItemWithLength: SQUARE_LENGTH];
        let _: () = msg_send![item, retain];
        STATUS_ITEM.with(|cell| cell.set(item));
        let button: id = msg_send![item, button];
        if button == nil {
            return nil;
        }
        let image = icons::template_image(Glyph::Memo, 18.);
        let _: () = msg_send![button, setImage: image];
        let _: () = msg_send![image, release];
        let _: () = msg_send![button, setToolTip: ns("New Memo (⌃⌥N)")];
        let target = status_target();
        let _: () = msg_send![button, setTarget: target];
        let _: () = msg_send![button, setAction: sel!(fumiStatusClicked:)];
        let _: i64 = msg_send![button, sendActionOn: LEFT_MOUSE_UP_MASK | RIGHT_MOUSE_UP_MASK];
        button
    }
}

// The panel.

/// Everything about capture that outlives one showing.
pub struct QuickCapture {
    panel: Option<(WindowHandle<CaptureView>, usize)>,
    status_button: usize,
    monitors: Vec<usize>,
}

impl Global for QuickCapture {}

impl QuickCapture {
    /// Registers ⌃⌥N, the menu bar icon, and the `NewMemo` action. Once, at launch.
    pub fn install(cx: &mut App) {
        if cx.has_global::<QuickCapture>() {
            return;
        }
        let (tx, mut rx) = unbounded();
        let _ = MESSAGES.set(tx);
        install_hot_key();
        let status_button = install_status_item() as usize;
        cx.set_global(QuickCapture { panel: None, status_button, monitors: Vec::new() });
        cx.on_action(|_: &NewMemo, cx| Self::show(cx));
        cx.spawn(async move |cx| {
            while let Some(message) = rx.next().await {
                cx.update(|cx| match message {
                    Message::Toggle => Self::toggle(cx),
                    Message::Show => Self::show(cx),
                    Message::Hide => Self::hide(cx),
                    Message::OpenApp => Self::open_app(cx),
                    Message::Quit => cx.quit(),
                });
            }
        })
        .detach();
    }

    pub fn toggle(cx: &mut App) {
        if Self::is_visible(cx) { Self::hide(cx) } else { Self::show(cx) }
    }

    fn is_visible(cx: &App) -> bool {
        let Some((_, panel)) = cx.global::<QuickCapture>().panel else { return false };
        unsafe { msg_send![panel as id, isVisible] }
    }

    pub fn show(cx: &mut App) {
        if !cx.has_global::<QuickCapture>() {
            return;
        }
        let existing = cx.global::<QuickCapture>().panel;
        let (handle, panel) = match existing {
            Some(panel) => panel,
            None => {
                let Some(panel) = open_panel(cx) else { return };
                cx.global_mut::<QuickCapture>().panel = Some(panel);
                panel
            }
        };
        let panel_id = panel as id;
        position(panel_id);
        unsafe {
            let _: () = msg_send![panel_id, makeKeyAndOrderFront: nil];
        }
        let _ = handle.update(cx, |view, window, cx| view.shown(window, cx));
        // Opened from a menu, the menu's close hands focus back to the main window
        // right after; take it again.
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_millis(1)).await;
            cx.update(|cx| {
                if Self::is_visible(cx) {
                    unsafe {
                        let _: () = msg_send![panel as id, makeKeyWindow];
                    }
                }
            });
        })
        .detach();
        watch_clicks_outside(panel, cx);
    }

    pub fn hide(cx: &mut App) {
        let state = cx.global_mut::<QuickCapture>();
        if let Some((_, panel)) = state.panel {
            unsafe {
                let _: () = msg_send![panel as id, orderOut: nil];
            }
        }
        for monitor in state.monitors.drain(..) {
            unsafe {
                let _: () = msg_send![class!(NSEvent), removeMonitor: monitor as id];
                let _: () = msg_send![monitor as id, release];
            }
        }
    }

    fn open_app(cx: &mut App) {
        cx.activate(true);
        let panel = cx.global::<QuickCapture>().panel.map(|(handle, _)| handle.window_id());
        let first = cx.windows().into_iter().find(|window| Some(window.window_id()) != panel);
        if let Some(window) = first {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
    }
}

/// A click anywhere but the panel puts it away, and the draft stays. A click on
/// the menu bar icon is left to the icon, so that it toggles.
fn watch_clicks_outside(panel: usize, cx: &mut App) {
    let status_button = cx.global::<QuickCapture>().status_button;
    if !cx.global::<QuickCapture>().monitors.is_empty() {
        return;
    }
    let mask = LEFT_MOUSE_DOWN_MASK | RIGHT_MOUSE_DOWN_MASK;
    unsafe {
        let local = ConcreteBlock::new(move |event: id| -> id {
            let window: id = msg_send![event, window];
            let status_window: id =
                if status_button == 0 { nil } else { msg_send![status_button as id, window] };
            if window as usize != panel && window != status_window {
                send(Message::Hide);
            }
            event
        })
        .copy();
        let global = ConcreteBlock::new(|_: id| send(Message::Hide)).copy();
        let local: id = msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: mask handler: &*local];
        let global: id = msg_send![class!(NSEvent), addGlobalMonitorForEventsMatchingMask: mask handler: &*global];
        let state = cx.global_mut::<QuickCapture>();
        for monitor in [local, global] {
            if monitor != nil {
                let _: () = msg_send![monitor, retain];
                state.monitors.push(monitor as usize);
            }
        }
    }
}

/// Centered on the screen with the pointer, its card a fifth of the way down.
fn position(panel: id) {
    unsafe {
        let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        let screens: id = msg_send![class!(NSScreen), screens];
        let count: u64 = msg_send![screens, count];
        let mut screen: id = msg_send![class!(NSScreen), mainScreen];
        for i in 0..count {
            let candidate: id = msg_send![screens, objectAtIndex: i];
            let frame: NSRect = msg_send![candidate, frame];
            let inside = mouse.x >= frame.origin.x
                && mouse.x < frame.origin.x + frame.size.width
                && mouse.y >= frame.origin.y
                && mouse.y < frame.origin.y + frame.size.height;
            if inside {
                screen = candidate;
                break;
            }
        }
        if screen == nil {
            return;
        }
        let area: NSRect = msg_send![screen, visibleFrame];
        let (x, y) = logic::panel_origin((area.origin.x, area.origin.y, area.size.width, area.size.height), PANEL_SIZE);
        let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(PANEL_SIZE.0, PANEL_SIZE.1));
        let _: () = msg_send![panel, setFrame: frame display: true];
    }
}

fn open_panel(cx: &mut App) -> Option<(WindowHandle<CaptureView>, usize)> {
    let bounds = Bounds::new(point(px(0.), px(0.)), size(px(PANEL_SIZE.0 as f32), px(PANEL_SIZE.1 as f32)));
    let mut ns_panel = 0usize;
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: false,
            kind: WindowKind::PopUp,
            is_movable: false,
            app_owns_titlebar_drag: true,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        |window, cx| {
            let panel = native::ns_window_of(window);
            unsafe {
                let _: () = msg_send![panel, setFloatingPanel: true];
                let _: () = msg_send![panel, setLevel: FLOATING_LEVEL];
                let _: () = msg_send![panel, setCollectionBehavior: CAN_JOIN_ALL_SPACES | FULL_SCREEN_AUXILIARY | TRANSIENT];
                let _: () = msg_send![panel, setHasShadow: false]; // the card draws its own
                let _: () = msg_send![panel, setHidesOnDeactivate: false];
            }
            ns_panel = panel as usize;
            cx.new(|cx| CaptureView::new(window, cx))
        },
    );
    match result {
        Ok(handle) => Some((handle, ns_panel)),
        Err(error) => {
            eprintln!("couldn't open the capture panel: {error}");
            None
        }
    }
}

/// The library capture saves to: the last opened one, or the dev override.
fn capture_library() -> String {
    std::env::var("FUMI_WORKSPACE").unwrap_or_else(|_| settings::workspace_path())
}

/// Saves a memo: `Err` holds the footer's message.
pub fn save_memo(text: &str, library: &str, secs: f64, utc_offset: i32) -> Result<(), String> {
    if library.is_empty() {
        return Err("Open a notebook in Fumi first.".to_string());
    }
    memo_format::append_to_file(text, secs, utc_offset, Path::new(library))
        .map(|_| ())
        .map_err(|error| format!("Couldn't save: {error}"))
}

pub struct CaptureView {
    field: Entity<TextField>,
    message: Option<String>,
    _events: Subscription,
}

/// The card draws in the light palette whatever Fumi's mode, its text included
/// (Swift forces only SwiftUI's half; see PARITY Z6).
fn light_tokens() -> Tokens {
    Tokens::resolve(settings::color_theme(), false, &settings::accent_hex())
}

impl CaptureView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = field::memo_field("Write it down…", 44., Some(light_tokens), window, cx);
        let events = cx.subscribe(&field, |this, _, event: &FieldEvent, cx| match event {
            FieldEvent::Changed => cx.notify(),
            FieldEvent::Submit => this.save(cx),
            FieldEvent::Cancel => QuickCapture::hide(cx),
            _ => {}
        });
        CaptureView { field, message: None, _events: events }
    }

    /// Focus in the editor, the caret at the end, the message gone.
    fn shown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.message = None;
        let c = light_tokens();
        let focus = self.field.update(cx, |field, cx| {
            field.caret_color = Some(c.accent);
            field.selection_color = Some(c.selection);
            field.move_to_end(cx);
            field.focus.clone()
        });
        window.focus(&focus, cx);
        window.refresh();
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let text = self.field.read(cx).text().to_string();
        if logic::is_blank(&text) {
            return;
        }
        let at = now();
        match save_memo(&text, &capture_library(), at.secs, at.utc_offset) {
            Ok(()) => {
                self.field.update(cx, |field, cx| field.set_text("", cx));
                QuickCapture::hide(cx);
            }
            Err(message) => self.message = Some(message),
        }
        cx.notify();
    }
}

impl Render for CaptureView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = light_tokens();
        let blank = logic::is_blank(self.field.read(cx).text());
        let footer_text = self.message.clone().unwrap_or_else(|| "Memos · Today".to_string());
        let footer_color = if self.message.is_some() { c.text2 } else { c.faint_text };
        let save = div()
            .id("capture-save")
            .flex()
            .items_center()
            .gap(px(8.))
            .when(!blank, |el| {
                el.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.save(cx)))
            })
            .child(
                div()
                    .font(typo::ui(FontWeight::NORMAL))
                    .text_size(px(12.))
                    .line_height(line_box(12.))
                    .text_color(c.text2)
                    .child("Save"),
            )
            .child(
                div()
                    .w(px(24.))
                    .h(px(24.))
                    .rounded_full()
                    .bg(if blank { c.accent.opacity(0.22) } else { c.accent })
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::symbol("arrow.up", 11., weight::BOLD, white())),
            );
        let card = div()
            .w(px(560.))
            .px(px(18.))
            .pt(px(16.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .rounded(px(14.))
            .bg(c.page)
            .border_1()
            .border_color(c.page_edge)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.22),
                offset: point(px(0.), px(12.)),
                blur_radius: px(52.),
                spread_radius: px(0.),
                inset: false,
            }])
            .child(self.field.clone())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font(typo::ui(FontWeight::NORMAL))
                            .text_size(px(12.))
                            .line_height(line_box(12.))
                            .text_color(footer_color)
                            .child(footer_text),
                    )
                    .child(save),
            );
        div()
            .key_context("QuickCapture")
            .on_action(cx.listener(|this, _: &SaveCapture, _, cx| this.save(cx)))
            .on_action(cx.listener(|_, _: &CloseCapture, _, cx| QuickCapture::hide(cx)))
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .pl(px(20.))
            .pt(px(20.))
            .child(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn no_library_asks_for_one() {
        assert_eq!(save_memo("hi", "", 0., 0), Err("Open a notebook in Fumi first.".to_string()));
    }

    #[core::prelude::v1::test]
    fn a_failed_save_says_why() {
        let error = save_memo("hi", "/dev/null/nope", 0., 0).unwrap_err();
        assert!(error.starts_with("Couldn't save: "), "{error}");
    }

    #[core::prelude::v1::test]
    fn saving_appends_to_todays_file() {
        let dir = std::env::temp_dir().join(format!("fumi-capture-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let library = dir.to_string_lossy().into_owned();
        // 2026-10-17 09:31 UTC.
        save_memo("first", &library, 1_792_229_460., 0).unwrap();
        let day = memo_format::day_of(1_792_229_460., 0);
        let file = memo_format::file_for_day(&day, &dir);
        let content = std::fs::read_to_string(file).unwrap();
        assert!(content.contains("first"));
        assert_eq!(memo_format::parse(&content, &day).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
