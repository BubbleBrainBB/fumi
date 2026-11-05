//! Opens the publish preview beside a plain page, over the library in
//! FUMI_WORKSPACE, on its first note. ⌥⌘C copies for WeChat, ⌘E exports a PDF.
use fumi_core::tabs::Content;
use fumi_publish::{LiveText, PublishPane, beside_page};
use fumi_workspace::Workspace;
use gpui::*;
use std::rc::Rc;

actions!(demo, [CopyForWeChat, ExportPdf, ExportAll]);

struct Demo {
    workspace: Entity<Workspace>,
    pane: Entity<PublishPane>,
    focus: FocusHandle,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = fumi_ui::theme::t();
        let page = div().size_full().bg(c.page).rounded(px(12.)).border_1().border_color(c.page_edge);
        let note = self.workspace.read(cx).model.selected_note().map(str::to_string);
        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context("Demo")
            .bg(c.ground_fill)
            .p(px(8.))
            .pt(px(52.))
            .font(fumi_editor::display::ui_font())
            .text_color(c.text)
            .on_action(cx.listener(|this, _: &CopyForWeChat, _, cx| this.pane.update(cx, |p, cx| p.copy_for_wechat(cx))))
            .on_action(cx.listener(move |this, _: &ExportPdf, _, cx| {
                if let Some(note) = &note {
                    this.pane.update(cx, |p, cx| p.export_pdf(note, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &ExportAll, _, cx| this.pane.update(cx, |p, cx| p.export_all_notes(cx))))
            .child(beside_page(page, &self.pane, 12., cx))
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        fumi_ui::text_field::bind_keys(cx);
        fumi_publish::bind_keys(cx);
        fumi_ui::theme::apply_from_settings(false);
        fumi_ui::settings::set_bool(fumi_ui::settings::keys::WECHAT_EXTENSION, true);
        fumi_ui::settings::set_bool(fumi_ui::settings::keys::XHS_EXTENSION, true);
        cx.bind_keys([
            KeyBinding::new("alt-cmd-c", CopyForWeChat, None),
            KeyBinding::new("cmd-e", ExportPdf, None),
            KeyBinding::new("shift-cmd-e", ExportAll, None),
        ]);
        let bounds = Bounds::centered(None, size(px(1300.), px(860.)), cx);
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
            |window, cx| {
                let workspace = cx.new(Workspace::new);
                // The notes arrive off the main thread; the first to arrive opens.
                cx.observe(&workspace, |workspace, cx| {
                    workspace.update(cx, |w, cx| {
                        if matches!(w.model.active_content(), Content::Draft) {
                            if let Some(id) = w.store.notes().first().map(|n| n.id.clone()) {
                                w.model.open_note(Some(&id), false);
                                if !w.model.is_preview_open() {
                                    w.model.toggle_preview();
                                }
                                w.navigated(cx);
                            }
                        }
                    });
                })
                .detach();
                let live: LiveText = Rc::new(|_: &App| None);
                let pane = cx.new(|cx| PublishPane::new(workspace.clone(), live, window, cx));
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                cx.new(|cx| {
                    cx.observe(&pane, |_, _, cx| cx.notify()).detach();
                    cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
                    Demo { workspace, pane, focus }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
