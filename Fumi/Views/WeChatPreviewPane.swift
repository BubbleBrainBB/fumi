import AppKit
import SwiftUI
import WebKit

/// The WeChat Article extension (design boards ㊹–㊻ B, 62 A): beside the
/// page, the open note laid out for the WeChat Official Account editor at
/// phone width, redrawn as you type. One bar on top: the theme and Copy.
/// The article sits on the sheet itself, its title at the head.
struct WeChatPreviewPane: View {
    @Environment(AppModel.self) private var appModel
    @Environment(\.colorScheme) private var colorScheme
    @AppStorage(AppSettings.wechatThemeKey) private var themeName = WeChatRenderer.Theme.blackRed.rawValue
    @State private var article: WeChatRenderer.Article?
    @State private var failure: String?
    @State private var copied = false
    /// The notices the reader hid; new ones show again.
    @State private var hiddenNotices: [String]?

    private var theme: WeChatRenderer.Theme { WeChatRenderer.Theme(rawValue: themeName) ?? .blackRed }

    private var notices: [String] {
        if let failure { return [failure] }
        return article?.warnings.map(WeChatCopy.english) ?? []
    }

    private struct Request: Equatable {
        let note: AppModel.NoteText?
        let theme: WeChatRenderer.Theme
    }

    var body: some View {
        let notices = notices
        let isHidden = !notices.isEmpty && hiddenNotices == notices
        VStack(spacing: 0) {
            PublishToolbar {
                if isHidden { NoticeDot { hiddenNotices = nil } }
                PopUpButton(help: "Theme", menu: themeMenu) {
                    Circle().fill(Color(nsColor: theme.accent)).frame(width: 8, height: 8)
                    Text(theme.name)
                }
                Button(action: copy) {
                    HStack(spacing: 5) {
                        if copied { Image(systemName: "checkmark").font(.system(size: 10, weight: .semibold)) }
                        Text(copied ? "Copied" : "Copy")
                    }
                }
                .buttonStyle(PaneButtonStyle())
                .disabled(article == nil || failure != nil)
                .help("Copy for WeChat (⌥⌘C)")
            }
            if !notices.isEmpty && !isHidden {
                if failure != nil {
                    PaneNotice(header: "This note can’t be laid out", items: notices)
                } else {
                    PaneNotice(header: PaneNotice<EmptyView>.header(notices.count, before: "pasting"), items: notices) {
                        hiddenNotices = notices
                    }
                }
            }
            ArticleWebView(html: article?.html ?? "", title: article?.title ?? "", isDark: colorScheme == .dark)
                .opacity(failure == nil ? 1 : 0.35)
        }
        .task(id: Request(note: appModel.previewNote, theme: theme)) { await render() }
    }

    private func themeMenu() -> NSMenu {
        let menu = NSMenu()
        for option in WeChatRenderer.Theme.allCases {
            let item = ClosureMenuItem(option.name) { themeName = option.rawValue }
            item.image = option.swatch
            item.state = option == theme ? .on : .off
            menu.addItem(item)
        }
        return menu
    }

    private func render() async {
        guard let note = appModel.previewNote else { return }
        do {
            let result = try await WeChatCopy.render(note, theme: theme)
            guard !Task.isCancelled else { return }
            article = result
            failure = nil
        } catch {
            guard !Task.isCancelled else { return }
            failure = WeChatCopy.english(error.localizedDescription)
        }
    }

    private func copy() {
        guard let article, let note = appModel.previewNote else { return }
        WeChatCopy.copy(article, markdown: note.markdown)
        copied = true
        Task {
            try? await Task.sleep(for: .seconds(1.2))
            copied = false
        }
    }
}

// MARK: - Copying

/// Rendering and the clipboard, shared by the preview's Copy and the
/// Copy for WeChat command.
enum WeChatCopy {
    static var theme: WeChatRenderer.Theme {
        WeChatRenderer.Theme(rawValue: UserDefaults.standard.string(forKey: AppSettings.wechatThemeKey) ?? "") ?? .blackRed
    }

    /// Off the main thread: a note with local images reads and encodes them.
    static func render(_ note: AppModel.NoteText, theme: WeChatRenderer.Theme) async throws -> WeChatRenderer.Article {
        // `![[…]]` files are found the way the editor finds them.
        let markdown = await MainActor.run {
            WeChatRenderer.resolvingWikiLinks(note.markdown) { ImageLoader.shared.resolveEmbed($0)?.path }
        }
        // Off the main thread from the start: the first call also loads the renderer.
        return try await Task.detached(priority: .userInitiated) {
            guard let renderer = WeChatRenderer.shared else {
                throw WeChatRenderer.RenderError(message: "The WeChat renderer is missing from this build.")
            }
            return try renderer.render(markdown: markdown, notePath: note.path, theme: theme)
        }.value
    }

    /// The article as rich text for the WeChat editor, with the Markdown as
    /// the plain-text fallback.
    static func copy(_ article: WeChatRenderer.Article, markdown: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(article.html, forType: .html)
        pasteboard.setString(markdown, forType: .string)
    }

    static func copyText(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    /// ⌥⌘C: copies the open note in the chosen theme without the preview.
    /// False when it can't be laid out (the preview then shows why).
    @MainActor
    static func copyOpenNote(_ appModel: AppModel) async -> Bool {
        guard let note = appModel.typedNote ?? appModel.previewNote, note.noteId == appModel.selectedNoteId,
              let article = try? await render(note, theme: theme) else { return false }
        copy(article, markdown: note.markdown)
        return true
    }

    /// Copies, or if the note can't be laid out, beeps and opens the
    /// preview, which says why.
    @MainActor
    static func copyOrShowWhy(_ appModel: AppModel) async {
        guard !(await copyOpenNote(appModel)) else { return }
        NSSound.beep()
        if !appModel.isPublishPreviewOpen { appModel.togglePublishPreview() }
    }

    /// The renderer's notes are in Chinese (the skill's own words); the app
    /// speaks English.
    static func english(_ message: String) -> String {
        for (pattern, template) in translations {
            let range = NSRange(location: 0, length: (message as NSString).length)
            if let match = pattern.firstMatch(in: message, range: range) {
                return pattern.replacementString(for: match, in: message, offset: 0, template: template)
            }
        }
        return message
    }

    private static let translations: [(NSRegularExpression, String)] = [
        (#"^图片文件不存在：(.*)$"#, "Image not found: $1"),
        (#"^本地图片仅支持 PNG/JPEG/GIF/WebP：(.*)$"#, "Only PNG, JPEG, GIF and WebP images can go in the article: $1"),
        (#"^不支持的图片地址：(.*)$"#, "This image address can't go in the article: $1"),
        (#"^图片路径编码无效：(.*)$"#, "This image path isn't valid: $1"),
        (#"^YAML frontmatter 没有结束分隔线"#, "The note's front matter has no closing ---."),
        (#"^视频 (\d+) 的本地文件未找到：(.*)$"#, "Video $1 isn't on this Mac: $2"),
        (#"^有 (\d+) 处视频待在公众号后台插入"#, "$1 video placeholder(s): add each video in the WeChat editor, then remove its card."),
        (#"^含 (\d+) 个 SVG 动画"#, "$1 animated diagram(s): they play here; check them in WeChat before publishing."),
        (#"^存在 HTTP 图片"#, "Some images use http://. Prefer https:// and check them after pasting."),
        (#"^含 Base64 图片"#, "Embedded images go with the article. Check that WeChat keeps them after pasting."),
        (#"^本地图片已嵌入正文"#, "Images on this Mac go with the article. Check that WeChat keeps them after pasting."),
        (#"^含非 HTTP\(S\) 链接"#, "Some links aren't web addresses; they won't open in WeChat."),
        (#"^Obsidian Wiki 链接或嵌入未转换"#, "[[Links]] and ![[embeds]] stay as plain text."),
        (#"^脚注语法未转换"#, "Footnotes stay as plain text."),
        (#"^数学公式未排版"#, "Math stays as plain text."),
        (#"^存在未支持的 HTML"#, "HTML in the note shows as text; it isn't applied."),
        (#"^frontmatter 标题与开头 H1 不同"#, "The front matter title differs from the first heading; the heading is the title."),
    ].map { (try! NSRegularExpression(pattern: $0.0), $0.1) }
}

// MARK: - The article

/// The article at phone width on a web view: what will be pasted, as it
/// will look, under its title. On a light sheet it sits on the sheet; on a
/// dark one, on a white phone page, as WeChat shows it. Updates swap the
/// article in place, so the scroll stays put.
private struct ArticleWebView: NSViewRepresentable {
    let html: String
    let title: String
    let isDark: Bool

    private static let page = """
    <!doctype html><html><head><meta charset="utf-8"><meta name="referrer" content="no-referrer">
    <style>
    html,body{margin:0;background:transparent}
    body{padding:22px 0 48px;font-family:-apple-system,BlinkMacSystemFont,sans-serif}
    body.dark{padding-top:14px}
    #column{width:375px;margin:0 auto}
    body.dark #column{background:#fff;border-radius:10px;padding-top:20px;overflow:hidden}
    #head{position:relative;margin:0 20px;padding-bottom:18px}
    #head.empty{display:none}
    #head::before{content:"";position:absolute;inset:-6px -10px 10px -10px;border-radius:8px;
      background:rgba(0,0,0,.035);opacity:0}
    #head:hover::before{opacity:1}
    #title{position:relative;margin:0;font-size:20px;line-height:1.45;font-weight:600;color:#1D1D1F;letter-spacing:.2px}
    #copy{position:absolute;right:-4px;bottom:-4px;display:none;align-items:center;gap:5px;height:22px;padding:0 8px;
      border-radius:6px;background:#fff;border:1px solid #D9D9DC;box-shadow:0 1px 3px rgba(0,0,0,.08);
      font:12px -apple-system,BlinkMacSystemFont,sans-serif;color:#1D1D1F;cursor:default}
    #head:hover #copy{display:flex}
    #copy:active{background:#F2F2F4}
    body:not(.dark) #article>*:first-child{background:transparent!important}
    #article>*:first-child{padding-top:0!important}
    </style></head><body><div id="column">
    <div id="head" class="empty"><h1 id="title"></h1><div id="copy"><svg width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="5" y="5" width="8.5" height="8.5" rx="1.8"/><path d="M10.5 3.2V3a1.5 1.5 0 0 0-1.5-1.5H3A1.5 1.5 0 0 0 1.5 3v6A1.5 1.5 0 0 0 3 10.5h.2"/></svg><span>Copy title</span></div></div>
    <div id="article"></div></div>
    <script>
    document.getElementById('copy').addEventListener('click', () => {
      window.webkit.messageHandlers.wechat.postMessage('copyTitle');
      const label = document.querySelector('#copy span');
      label.textContent = 'Copied';
      setTimeout(() => { label.textContent = 'Copy title'; }, 1200);
    });
    </script></body></html>
    """

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.userContentController.add(context.coordinator, name: "wechat")
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.setValue(false, forKey: "drawsBackground")
        view.navigationDelegate = context.coordinator
        context.coordinator.webView = view
        view.loadHTMLString(Self.page, baseURL: nil)
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {
        context.coordinator.show(Shown(html: html, title: title, isDark: isDark))
    }

    static func dismantleNSView(_ view: WKWebView, coordinator: Coordinator) {
        view.configuration.userContentController.removeScriptMessageHandler(forName: "wechat")
    }

    struct Shown: Equatable {
        let html: String
        let title: String
        let isDark: Bool
    }

    final class Coordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
        weak var webView: WKWebView?
        private var isLoaded = false
        private var pending: Shown?
        private var shown: Shown?

        func show(_ next: Shown) {
            guard next != shown else { return }
            guard isLoaded, let webView else { pending = next; return }
            let previous = shown
            shown = next
            var script = ""
            if next.html != previous?.html {
                script += "document.getElementById('article').innerHTML = \(Self.literal(next.html));"
            }
            if next.title != previous?.title {
                script += """
                document.getElementById('title').textContent = \(Self.literal(next.title));
                document.getElementById('head').classList.toggle('empty', \(next.title.isEmpty));
                """
            }
            if next.isDark != previous?.isDark {
                script += "document.body.classList.toggle('dark', \(next.isDark));"
            }
            webView.evaluateJavaScript(script)
        }

        private static func literal(_ text: String) -> String {
            let array = (try? JSONSerialization.data(withJSONObject: [text], options: []))
                .flatMap { String(data: $0, encoding: .utf8) } ?? "[\"\"]"
            return "\(array)[0]"
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            isLoaded = true
            if let pending { self.pending = nil; show(pending) }
        }

        /// WeChat takes the title in a field of its own, so it's copied apart.
        func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
            guard message.body as? String == "copyTitle", let title = shown?.title else { return }
            WeChatCopy.copyText(title)
        }

        /// Links open in the browser, never inside the preview.
        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                     decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
            if action.navigationType == .linkActivated, let url = action.request.url {
                NSWorkspace.shared.open(url)
                decisionHandler(.cancel)
            } else {
                decisionHandler(.allow)
            }
        }
    }
}
