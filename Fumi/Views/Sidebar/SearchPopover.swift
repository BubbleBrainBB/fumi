import SwiftUI

/// Spotlight-style quick search: a floating panel over the window, opened by
/// the sidebar's search row, the rail's search button or ⇧⌘F.
///
/// The query is matched fuzzily (subsequence, scored) against every note's
/// file name, title, folder path, tags and body; a `#` prefix searches tags.
/// Results are keyboard-navigable: ↑↓ moves, ⏎ opens the note, esc dismisses.
struct SearchPopover: View {
    @Environment(AppModel.self) private var appModel
    @Environment(FileNoteStore.self) private var store

    @State private var query = ""
    @State private var selection = 0
    @State private var hoverIndex: Int?
    @FocusState private var fieldFocused: Bool
    /// Note bodies prepared for search, built once when the panel opens.
    @State private var corpus: SearchCorpus?
    /// The latest finished search. Searches run off the main thread, so for
    /// a moment after a keystroke this still holds the previous query's hits.
    @State private var searchResult: (query: String, hits: [NoteSearchHit]) = ("", [])
    @State private var recentHits: [NoteSearchHit] = []

    private var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Recent notes while the query is empty, ranked hits otherwise. Stored,
    /// not computed: the view reads this for every row, and recomputing the
    /// search on each read froze the app.
    private var hits: [NoteSearchHit] {
        trimmedQuery.isEmpty ? recentHits : searchResult.hits
    }

    /// Whether `hits` belongs to the current query (not a stale one).
    private var isSettled: Bool { searchResult.query == trimmedQuery }

    /// The row ⏎ would open: the hovered one, else the keyboard selection.
    private var activeIndex: Int {
        let index = hoverIndex ?? selection
        return hits.indices.contains(index) ? index : 0
    }

    private var sectionTitle: String {
        if trimmedQuery.isEmpty { return "Recently Updated" }
        if hits.isEmpty { return isSettled ? "No matching notes" : "Searching…" }
        return "\(hits.count) result\(hits.count == 1 ? "" : "s")"
    }

    /// Panel hugs its content for short lists, caps at 480 for long ones.
    /// Rows are two lines (~44pt); header 54, label 26, footer 31.
    private var panelMaxHeight: CGFloat {
        guard !trimmedQuery.isEmpty, !hits.isEmpty else { return 330 }
        return min(480, 112 + CGFloat(hits.count) * 44)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Rectangle().fill(Color.borderColor).frame(height: 1)
            results
            Rectangle().fill(Color.borderColor).frame(height: 1)
            footer
        }
        .frame(width: 600)
        .frame(maxHeight: panelMaxHeight)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Color.surface)
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(Color.controlBorder, lineWidth: 1)
                )
                .shadow(color: .black.opacity(0.22), radius: 32, y: 12)
        )
        .onAppear {
            fieldFocused = true
            recentHits = FuzzySearch.recent(store.notes, limit: 6).map(NoteSearchHit.recent)
        }
        .task(id: trimmedQuery) { await search(trimmedQuery) }
        .onChange(of: store.notes.count) { _, _ in corpus = nil }
        .onChange(of: query) { _, _ in selection = 0 }
        .onKeyPress(.upArrow) { moveSelection(-1); return .handled }
        .onKeyPress(.downArrow) { moveSelection(1); return .handled }
        .onKeyPress(.escape) { appModel.dismissSearch(); return .handled }
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 15, weight: .medium))
                .foregroundColor(.iconMuted)
            TextField("Search notes", text: $query)
                .textFieldStyle(.plain)
                .font(.fumi(.emphasis))
                .foregroundColor(.primaryText)
                .focused($fieldFocused)
                .onSubmit(openSelected)
                .onExitCommand { appModel.dismissSearch() }
            if query.isEmpty {
                ShortcutHint(text: "esc")
            } else {
                Button { query = "" } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 13))
                        .foregroundColor(.iconMuted)
                }
                .buttonStyle(.plain)
                .help("Clear (esc)")
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
    }

    // MARK: Results

    @ViewBuilder private var results: some View {
        if trimmedQuery.isEmpty == false, hits.isEmpty, isSettled {
            SearchEmptyState(query: trimmedQuery)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 1) {
                        SidebarSectionLabel(title: sectionTitle)
                        ForEach(hits.indices, id: \.self) { index in
                            SearchResultRow(
                                hit: hits[index],
                                isSelected: index == activeIndex
                            ) {
                                appModel.openNote(hits[index].note.id, inNewTab: NSEvent.modifierFlags.contains(.command))
                            } onHover: { hovering in
                                hoverIndex = hovering ? index : nil
                            }
                            .id(index)
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                }
                .scrollContentBackground(.hidden)
                .onChange(of: selection) { _, newValue in
                    withAnimation(.easeOut(duration: 0.12)) {
                        proxy.scrollTo(newValue, anchor: .center)
                    }
                }
            }
        }
    }

    private var footer: some View {
        HStack(spacing: 16) {
            Spacer()
            LabeledKey(key: "↑↓", label: "Navigate")
            LabeledKey(key: "↩", label: "Open")
            LabeledKey(key: "esc", label: "Dismiss")
            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 7)
    }

    // MARK: Searching

    /// Runs the search for `query` in the background, after a short pause so
    /// fast typing doesn't queue a search per keystroke. A newer query
    /// cancels this one (`.task(id:)`).
    private func search(_ query: String) async {
        guard !query.isEmpty else { return }
        if corpus != nil {
            try? await Task.sleep(for: .milliseconds(60))
            guard !Task.isCancelled else { return }
        }
        let notes = store.notes
        let existing = corpus
        let (built, found) = await Task.detached(priority: .userInitiated) {
            let corpus = existing ?? SearchCorpus(notes)
            return (corpus, FuzzySearch.hits(in: corpus, query: query))
        }.value
        guard !Task.isCancelled else { return }
        if corpus == nil { corpus = built }
        searchResult = (query, found)
    }

    // MARK: Keyboard

    private func moveSelection(_ delta: Int) {
        guard !hits.isEmpty else { return }
        let next = selection + delta
        if next < 0 { selection = hits.count - 1 }
        else if next >= hits.count { selection = 0 }
        else { selection = next }
    }

    private func openSelected() {
        // ⏎ typed faster than the background search: finish it here (a
        // search over the prepared corpus takes ~10 ms).
        if !trimmedQuery.isEmpty, !isSettled, let corpus {
            searchResult = (trimmedQuery, FuzzySearch.hits(in: corpus, query: trimmedQuery))
        }
        guard hits.indices.contains(activeIndex) else { return }
        // ⌘⏎ opens it in a new tab.
        appModel.openNote(hits[activeIndex].note.id, inNewTab: NSEvent.modifierFlags.contains(.command))
    }
}

// MARK: - Result row

private struct SearchResultRow: View {
    let hit: NoteSearchHit
    let isSelected: Bool
    let action: () -> Void
    let onHover: (Bool) -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            HStack(alignment: .top, spacing: 10) {
                Group {
                    if hit.note.pinned {
                        Image(systemName: "pin.fill").font(.system(size: 12))
                    } else {
                        ShiyeIcon(.note, size: 15)
                    }
                }
                    .foregroundColor(isSelected ? .accent : .faintIcon)
                    .frame(width: 16)
                    .padding(.top, 2)
                VStack(alignment: .leading, spacing: 2) {
                    highlight(hit.note.filename, ranges: hit.titleRanges)
                        .font(.fumi(.body, weight: isSelected ? .semibold : .regular))
                        .foregroundColor(.primaryText)
                        .lineLimit(1)
                    secondLine
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                if isSelected {
                    Image(systemName: "return")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundColor(.tertiaryText)
                        .padding(.top, 3)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(isSelected ? Color.accentSelected : (isHovering ? Color.cardHover : Color.clear))
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            isHovering = hovering
            onHover(hovering)
        }
    }

    /// Second line: what matched — the body excerpt, the title, the tag, or
    /// the folder path.
    @ViewBuilder private var secondLine: some View {
        HStack(spacing: 5) {
            switch hit.field {
            case .content:
                if !hit.subtitle.isEmpty {
                    Text(hit.subtitle)
                        .foregroundColor(.tertiaryText)
                    Text("·")
                        .foregroundColor(.faintText)
                }
                if let snippet = hit.snippet {
                    highlight(snippet.text, ranges: snippet.ranges)
                        .foregroundColor(.secondaryText)
                }
            case .tag:
                ShiyeIcon(.tag, size: 11)
                    .foregroundColor(.faintIcon)
                highlight(hit.subtitle, ranges: hit.subtitleRanges)
                    .foregroundColor(.secondaryText)
            case .title:
                highlight(hit.subtitle, ranges: hit.subtitleRanges)
                    .foregroundColor(.secondaryText)
            case .fileName, .folder, .recent:
                highlight(hit.subtitle, ranges: hit.subtitleRanges)
                    .foregroundColor(.tertiaryText)
            }
        }
        .font(.fumi(.caption))
        .truncationMode(.tail)
    }
}

// MARK: - Empty state

private struct SearchEmptyState: View {
    let query: String

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 20, weight: .medium))
                .foregroundColor(.faintIcon)
                .frame(width: 44, height: 44)
                .background(Circle().fill(Color.quietSurface))
            Text("No results for “\(query)”")
                .font(.fumi(.body, weight: .medium))
                .foregroundColor(.secondaryText)
            Text("Try different keywords, or search tags with a # prefix.")
                .font(.fumi(.caption))
                .foregroundColor(.tertiaryText)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 34)
    }
}

// MARK: - Footer key hints

private struct LabeledKey: View {
    let key: String
    let label: String

    var body: some View {
        HStack(spacing: 5) {
            ShortcutHint(text: key)
            Text(label)
                .font(.fumi(.caption))
                .foregroundColor(.tertiaryText)
        }
    }
}

// MARK: - Highlighting

/// Renders `text` with the fuzzy-matched runs tinted in the accent color.
private func highlight(_ text: String, ranges: [FuzzyRange]) -> Text {
    guard !ranges.isEmpty else { return Text(text) }
    var chunks: [Text] = []
    var cursor = text.startIndex
    for range in ranges {
        if cursor < range.start {
            chunks.append(Text(String(text[cursor..<range.start])))
        }
        chunks.append(
            Text(String(text[range.start..<range.end]))
                .foregroundColor(.accent)
                .bold()
        )
        cursor = range.end
    }
    if cursor < text.endIndex {
        chunks.append(Text(String(text[cursor...])))
    }
    return chunks.reduce(Text("")) { $0 + $1 }
}
