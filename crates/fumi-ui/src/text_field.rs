//! A plain text field for the app's small inputs: search, rename, tags, the memo
//! composer. Handles input methods the way the editor does. One line, or wrapped
//! lines growing with their text when `multiline` is set.

use crate::theme::t;
use gpui::prelude::*;
use gpui::*;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

actions!(
    text_field,
    [
        Backspace, Delete, Left, Right, SelectLeft, SelectRight, WordLeft, WordRight, SelectWordLeft,
        SelectWordRight, Home, End, SelectHome, SelectEnd, Up, Down, SelectUp, SelectDown, SelectAll,
        Copy, Cut, Paste, Undo, Redo, Submit, NewLine, Cancel,
    ]
);

pub fn bind_keys(cx: &mut App) {
    let c = Some("TextField");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, c),
        KeyBinding::new("delete", Delete, c),
        KeyBinding::new("left", Left, c),
        KeyBinding::new("right", Right, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("alt-left", WordLeft, c),
        KeyBinding::new("alt-right", WordRight, c),
        KeyBinding::new("alt-shift-left", SelectWordLeft, c),
        KeyBinding::new("alt-shift-right", SelectWordRight, c),
        KeyBinding::new("cmd-left", Home, c),
        KeyBinding::new("cmd-right", End, c),
        KeyBinding::new("home", Home, c),
        KeyBinding::new("end", End, c),
        KeyBinding::new("cmd-shift-left", SelectHome, c),
        KeyBinding::new("cmd-shift-right", SelectEnd, c),
        KeyBinding::new("up", Up, c),
        KeyBinding::new("down", Down, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new("cmd-a", SelectAll, c),
        KeyBinding::new("cmd-c", Copy, c),
        KeyBinding::new("cmd-x", Cut, c),
        KeyBinding::new("cmd-v", Paste, c),
        KeyBinding::new("cmd-z", Undo, c),
        KeyBinding::new("cmd-shift-z", Redo, c),
        KeyBinding::new("enter", Submit, c),
        KeyBinding::new("cmd-enter", Submit, c),
        KeyBinding::new("shift-enter", NewLine, c),
        KeyBinding::new("escape", Cancel, c),
    ]);
}

/// What the field tells its owner. Up and Down only reach here on one line, or
/// at the first or last line of several, so a list can take them.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldEvent {
    Changed,
    Submit,
    Cancel,
    MoveUp,
    MoveDown,
    Focused,
    Blurred,
}

impl EventEmitter<FieldEvent> for TextField {}

pub struct TextField {
    pub focus: FocusHandle,
    text: String,
    anchor: usize,
    head: usize,
    marked: Option<Range<usize>>,
    pub placeholder: SharedString,
    pub multiline: bool,
    /// Enter adds a line and ⌘Enter submits, instead of Enter submitting.
    pub enter_adds_line: bool,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub text_color: Option<Hsla>,
    pub placeholder_color: Option<Hsla>,
    undo: Vec<(String, usize)>,
    redo: Vec<(String, usize)>,
    layout: Option<ShapedLayout>,
    bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    caret_on: bool,
    blink_epoch: u64,
    _subs: Vec<Subscription>,
}

struct ShapedLayout {
    lines: Vec<(WrappedLine, usize)>,
    line_height: Pixels,
}

impl TextField {
    pub fn new(placeholder: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subs = vec![
            cx.on_focus(&focus, window, |this, _, cx| {
                this.restart_blink(cx);
                cx.emit(FieldEvent::Focused);
            }),
            cx.on_blur(&focus, window, |_, _, cx| cx.emit(FieldEvent::Blurred)),
        ];
        TextField {
            focus,
            text: String::new(),
            anchor: 0,
            head: 0,
            marked: None,
            placeholder: placeholder.into(),
            multiline: false,
            enter_adds_line: false,
            font_size: px(13.),
            line_height: px(18.),
            text_color: None,
            placeholder_color: None,
            undo: Vec::new(),
            redo: Vec::new(),
            layout: None,
            bounds: None,
            selecting: false,
            caret_on: true,
            blink_epoch: 0,
            _subs: subs,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.text = text.into();
        self.anchor = self.text.len();
        self.head = self.text.len();
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        cx.notify();
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.anchor = 0;
        self.head = self.text.len();
        cx.notify();
    }

    pub fn is_composing(&self) -> bool {
        self.marked.is_some()
    }

    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.blink_epoch += 1;
        self.caret_on = true;
        let epoch = self.blink_epoch;
        cx.spawn(async move |this: WeakEntity<TextField>, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(530)).await;
                let go_on = this
                    .update(cx, |f, cx| {
                        if f.blink_epoch != epoch {
                            return false;
                        }
                        f.caret_on = !f.caret_on;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !go_on {
                    break;
                }
            }
        })
        .detach();
    }

    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    fn replace(&mut self, range: Range<usize>, new: &str, cx: &mut Context<Self>) {
        self.undo.push((self.text.clone(), self.head));
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.text.replace_range(range.clone(), new);
        self.head = range.start + new.len();
        self.anchor = self.head;
        self.marked = None;
        self.caret_on = true;
        cx.emit(FieldEvent::Changed);
        cx.notify();
    }

    fn prev(&self, at: usize) -> usize {
        self.text[..at].grapheme_indices(true).next_back().map_or(0, |(i, _)| i)
    }

    fn next(&self, at: usize) -> usize {
        self.text[at..].grapheme_indices(true).nth(1).map_or(self.text.len(), |(i, _)| at + i)
    }

    fn word_prev(&self, at: usize) -> usize {
        self.text[..at]
            .split_word_bound_indices()
            .rev()
            .find(|(_, w)| w.chars().any(char::is_alphanumeric))
            .map_or(0, |(i, _)| i)
    }

    fn word_next(&self, at: usize) -> usize {
        self.text[at..]
            .split_word_bound_indices()
            .find(|(_, w)| w.chars().any(char::is_alphanumeric))
            .map_or(self.text.len(), |(i, w)| at + i + w.len())
    }

    fn line_start(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |p| p + 1)
    }

    fn line_end(&self, at: usize) -> usize {
        self.text[at..].find('\n').map_or(self.text.len(), |p| at + p)
    }

    fn set_head(&mut self, at: usize, extend: bool, cx: &mut Context<Self>) {
        self.head = at;
        if !extend {
            self.anchor = at;
        }
        self.caret_on = true;
        cx.notify();
    }

    fn vertical(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let on_edge = if !self.multiline {
            true
        } else if down {
            self.line_end(self.head) == self.text.len()
        } else {
            self.line_start(self.head) == 0
        };
        if on_edge {
            cx.emit(if down { FieldEvent::MoveDown } else { FieldEvent::MoveUp });
            return;
        }
        let start = self.line_start(self.head);
        let col = self.head - start;
        let target = if down {
            let next = self.line_end(self.head) + 1;
            let end = self.line_end(next);
            floor_boundary(&self.text, (next + col).min(end))
        } else {
            let prev = self.line_start(start - 1);
            floor_boundary(&self.text, (prev + col).min(start - 1))
        };
        self.set_head(target, extend, cx);
    }

    fn index_at(&self, p: Point<Pixels>) -> usize {
        let (Some(layout), Some(bounds)) = (&self.layout, self.bounds) else { return self.head };
        let mut y = bounds.top();
        let lh = layout.line_height;
        for (line, start) in &layout.lines {
            let h = lh * (line.wrap_boundaries().len() + 1) as f32;
            if p.y < y + h {
                let rel = point(p.x - bounds.left(), p.y - y);
                return start + line.closest_index_for_position(rel, lh).unwrap_or_else(|i| i);
            }
            y += h;
        }
        self.text.len()
    }

    fn on_key_submit(&mut self, cx: &mut Context<Self>, cmd: bool) {
        if self.multiline && self.enter_adds_line && !cmd {
            let r = self.selection();
            self.replace(r, "\n", cx);
        } else {
            cx.emit(FieldEvent::Submit);
        }
    }
}

fn floor_boundary(s: &str, mut at: usize) -> usize {
    while !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

impl TextField {
    fn to_utf16(&self, byte: usize) -> usize {
        utf16_len(&self.text[..byte])
    }

    fn from_utf16(&self, units: usize) -> usize {
        let mut n = 0;
        for (i, c) in self.text.char_indices() {
            if n >= units {
                return i;
            }
            n += c.len_utf16();
        }
        self.text.len()
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let (a, b) = (self.from_utf16(range.start), self.from_utf16(range.end));
        *adjusted = Some(self.to_utf16(a)..self.to_utf16(b));
        Some(self.text[a..b].to_string())
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        let s = self.selection();
        Some(UTF16Selection { range: self.to_utf16(s.start)..self.to_utf16(s.end), reversed: self.head < self.anchor })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|m| self.to_utf16(m.start)..self.to_utf16(m.end))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        let range = range
            .map(|r| self.from_utf16(r.start)..self.from_utf16(r.end))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selection());
        if !self.multiline && text.contains('\n') {
            let single = text.replace('\n', " ");
            self.replace(range, &single, cx);
        } else {
            self.replace(range, text, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|r| self.from_utf16(r.start)..self.from_utf16(r.end))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.selection());
        self.text.replace_range(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        let base = self.to_utf16(range.start);
        match selected {
            Some(s) => {
                self.anchor = self.from_utf16(base + s.start);
                self.head = self.from_utf16(base + s.end);
            }
            None => {
                self.head = range.start + text.len();
                self.anchor = self.head;
            }
        }
        cx.emit(FieldEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(&mut self, range: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds?;
        let layout = self.layout.as_ref()?;
        let at = self.from_utf16(range.start);
        let p = caret_position(layout, at)?;
        Some(Bounds::new(bounds.origin + p, size(px(1.), layout.line_height)))
    }

    fn character_index_for_point(&mut self, p: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.to_utf16(self.index_at(p)))
    }
}

fn caret_position(layout: &ShapedLayout, at: usize) -> Option<Point<Pixels>> {
    let lh = layout.line_height;
    let mut y = px(0.);
    for (i, (line, start)) in layout.lines.iter().enumerate() {
        let end = start + line.len();
        let last = i + 1 == layout.lines.len();
        if at >= *start && (at <= end || last) {
            let p = line.position_for_index((at - start).min(line.len()), lh)?;
            return Some(point(p.x, y + p.y));
        }
        y += lh * (line.wrap_boundaries().len() + 1) as f32;
    }
    None
}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        macro_rules! go {
            ($el:ident, $a:ty, |$this:ident, $cx:ident| $body:expr) => {
                $el.on_action(cx.listener(|$this: &mut Self, _: &$a, _, $cx| $body))
            };
        }
        let el = div()
            .id("text-field")
            .key_context("TextField")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .w_full();
        let el = go!(el, Backspace, |this, cx| {
            let r = if this.anchor != this.head { this.selection() } else { this.prev(this.head)..this.head };
            if !r.is_empty() {
                this.replace(r, "", cx);
            }
        });
        let el = go!(el, Delete, |this, cx| {
            let r = if this.anchor != this.head { this.selection() } else { this.head..this.next(this.head) };
            if !r.is_empty() {
                this.replace(r, "", cx);
            }
        });
        let el = go!(el, Left, |this, cx| {
            let to = if this.anchor != this.head { this.selection().start } else { this.prev(this.head) };
            this.set_head(to, false, cx)
        });
        let el = go!(el, Right, |this, cx| {
            let to = if this.anchor != this.head { this.selection().end } else { this.next(this.head) };
            this.set_head(to, false, cx)
        });
        let el = go!(el, SelectLeft, |this, cx| this.set_head(this.prev(this.head), true, cx));
        let el = go!(el, SelectRight, |this, cx| this.set_head(this.next(this.head), true, cx));
        let el = go!(el, WordLeft, |this, cx| this.set_head(this.word_prev(this.head), false, cx));
        let el = go!(el, WordRight, |this, cx| this.set_head(this.word_next(this.head), false, cx));
        let el = go!(el, SelectWordLeft, |this, cx| this.set_head(this.word_prev(this.head), true, cx));
        let el = go!(el, SelectWordRight, |this, cx| this.set_head(this.word_next(this.head), true, cx));
        let el = go!(el, Home, |this, cx| this.set_head(this.line_start(this.head), false, cx));
        let el = go!(el, End, |this, cx| this.set_head(this.line_end(this.head), false, cx));
        let el = go!(el, SelectHome, |this, cx| this.set_head(this.line_start(this.head), true, cx));
        let el = go!(el, SelectEnd, |this, cx| this.set_head(this.line_end(this.head), true, cx));
        let el = go!(el, Up, |this, cx| this.vertical(false, false, cx));
        let el = go!(el, Down, |this, cx| this.vertical(true, false, cx));
        let el = go!(el, SelectUp, |this, cx| this.vertical(false, true, cx));
        let el = go!(el, SelectDown, |this, cx| this.vertical(true, true, cx));
        let el = go!(el, SelectAll, |this, cx| this.select_all(cx));
        let el = go!(el, Copy, |this, cx| {
            if this.anchor != this.head {
                cx.write_to_clipboard(ClipboardItem::new_string(this.text[this.selection()].to_string()));
            }
        });
        let el = go!(el, Cut, |this, cx| {
            if this.anchor != this.head {
                cx.write_to_clipboard(ClipboardItem::new_string(this.text[this.selection()].to_string()));
                let r = this.selection();
                this.replace(r, "", cx);
            }
        });
        let el = go!(el, Paste, |this, cx| {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                let text = if this.multiline { text } else { text.replace('\n', " ") };
                let r = this.selection();
                this.replace(r, &text, cx);
            }
        });
        let el = go!(el, Undo, |this, cx| {
            if let Some((text, head)) = this.undo.pop() {
                this.redo.push((std::mem::replace(&mut this.text, text), this.head));
                this.head = head.min(this.text.len());
                this.anchor = this.head;
                cx.emit(FieldEvent::Changed);
                cx.notify();
            }
        });
        let el = go!(el, Redo, |this, cx| {
            if let Some((text, head)) = this.redo.pop() {
                this.undo.push((std::mem::replace(&mut this.text, text), this.head));
                this.head = head.min(this.text.len());
                this.anchor = this.head;
                cx.emit(FieldEvent::Changed);
                cx.notify();
            }
        });
        let el = go!(el, Cancel, |_this, cx| cx.emit(FieldEvent::Cancel));
        let el = el
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                let cmd = window.modifiers().platform;
                this.on_key_submit(cx, cmd);
            }))
            .on_action(cx.listener(|this, _: &NewLine, _, cx| {
                if this.multiline {
                    let r = this.selection();
                    this.replace(r, "\n", cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    let at = this.index_at(e.position);
                    if e.click_count >= 2 {
                        this.anchor = this.word_prev(this.next(at));
                        this.head = this.word_next(this.anchor);
                    } else {
                        this.selecting = true;
                        this.set_head(at, e.modifiers.shift, cx);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.selecting && e.pressed_button == Some(MouseButton::Left) {
                    let at = this.index_at(e.position);
                    this.set_head(at, true, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.selecting = false));
        el.child(FieldText { field: cx.entity() })
    }
}

struct FieldText {
    field: Entity<TextField>,
}

impl IntoElement for FieldText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

struct FieldPrepaint {
    lines: Vec<(WrappedLine, usize)>,
    placeholder: bool,
}

impl Element for FieldText {
    type RequestLayoutState = ();
    type PrepaintState = FieldPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let field = self.field.clone();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let f = window.text_style().font();
        let id = window.request_measured_layout(style, move |known, avail, window, cx| {
            let width = known.width.unwrap_or(match avail.width {
                AvailableSpace::Definite(w) => w,
                _ => px(300.),
            });
            let (text, multiline, font_size, lh) = {
                let t = field.read(cx);
                (t.text.clone(), t.multiline, t.font_size, t.line_height)
            };
            if !multiline {
                return size(width, lh);
            }
            let run = TextRun {
                len: text.len(),
                font: f.clone(),
                color: black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let rows = window
                .text_system()
                .shape_text(SharedString::from(text.clone()), font_size, &[run], Some(width), None)
                .map(|lines| lines.iter().map(|l| l.wrap_boundaries().len() + 1).sum::<usize>())
                .unwrap_or(1);
            size(width, lh * rows.max(1) as f32)
        });
        let _ = cx;
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> FieldPrepaint {
        let c = t();
        let field = self.field.read(cx);
        let font = window.text_style().font();
        let placeholder = field.text.is_empty();
        let (text, color): (SharedString, Hsla) = if placeholder {
            (field.placeholder.clone(), field.placeholder_color.unwrap_or(c.faint_text))
        } else {
            (field.text.clone().into(), field.text_color.unwrap_or(c.text))
        };
        let wrap = field.multiline.then_some(bounds.size.width);
        let mut runs = vec![TextRun {
            len: text.len(),
            font: font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        // The composing text is underlined.
        if let (Some(m), false) = (&field.marked, placeholder) {
            let base = runs.remove(0);
            for (range, under) in [(0..m.start, false), (m.clone(), true), (m.end..text.len(), false)] {
                if !range.is_empty() {
                    let mut r = base.clone();
                    r.len = range.len();
                    if under {
                        r.underline = Some(UnderlineStyle { thickness: px(1.), color: Some(color), wavy: false });
                    }
                    runs.push(r);
                }
            }
        }
        let shaped = window
            .text_system()
            .shape_text(text.clone(), field.font_size, &runs, wrap, None)
            .map(|v| v.into_vec())
            .unwrap_or_default();
        let mut lines = Vec::with_capacity(shaped.len());
        let mut at = 0;
        for line in shaped {
            let len = line.len();
            lines.push((line, at));
            at += len + 1;
        }
        FieldPrepaint { lines, placeholder }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut FieldPrepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let c = t();
        let (focus, anchor, head, caret_on, lh) = {
            let f = self.field.read(cx);
            (f.focus.clone(), f.anchor, f.head, f.caret_on, f.line_height)
        };
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.field.clone()), cx);
        let focused = focus.is_focused(window);
        let layout = ShapedLayout {
            lines: std::mem::take(&mut prepaint.lines),
            line_height: lh,
        };
        // Selection, row by row.
        if focused && anchor != head && !prepaint.placeholder {
            let (a, e) = (anchor.min(head), anchor.max(head));
            let mut y = bounds.top();
            for (line, start) in &layout.lines {
                let rows = line.wrap_boundaries().len() + 1;
                let end = start + line.len();
                if e >= *start && a <= end {
                    let from = a.max(*start) - start;
                    let to = e.min(end) - start;
                    if let (Some(p0), Some(p1)) =
                        (line.position_for_index(from, lh), line.position_for_index(to, lh))
                    {
                        let mut row_y = p0.y;
                        while row_y <= p1.y {
                            let x0 = if row_y == p0.y { p0.x } else { px(0.) };
                            let x1 = if row_y == p1.y { p1.x } else { bounds.size.width };
                            window.paint_quad(fill(
                                Bounds::from_corners(
                                    point(bounds.left() + x0, y + row_y),
                                    point(bounds.left() + x1, y + row_y + lh),
                                ),
                                c.selection,
                            ));
                            row_y += lh;
                        }
                    }
                }
                y += lh * rows as f32;
            }
        }
        let mut y = bounds.top();
        for (line, _) in &layout.lines {
            let rows = line.wrap_boundaries().len() + 1;
            let _ = line.paint(point(bounds.left(), y), lh, TextAlign::Left, None, window, cx);
            y += lh * rows as f32;
        }
        if focused && caret_on && anchor == head {
            let p = if prepaint.placeholder { Some(point(px(0.), px(0.))) } else { caret_position(&layout, head) };
            if let Some(p) = p {
                window.paint_quad(fill(
                    Bounds::new(bounds.origin + p + point(px(0.), px(1.)), size(px(1.5), lh - px(2.))),
                    c.accent,
                ));
            }
        }
        self.field.update(cx, |f, _| {
            f.layout = Some(layout);
            f.bounds = Some(bounds);
        });
    }
}
