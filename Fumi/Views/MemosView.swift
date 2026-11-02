import AppKit
import SwiftUI

/// The Memos page (design boards ㊷ A and ㊸ 2): write on a line at the top
/// of the page, the memos below it, newest first, grouped by day; in the
/// right margin a month calendar — each day's number on an accent wash that
/// deepens with how many memos it has — and the tags. Memos are stored one
/// Markdown file per day in the library's Memos folder.
struct MemosView: View {
    @Environment(FileNoteStore.self) private var store
    @Environment(AppModel.self) private var appModel
    @AppStorage(AppSettings.editorFontKey) private var editorFont = "System"

    @State private var selectedDay: String?
    @State private var selectedTag: String?
    @State private var month = Date()
    @State private var pageWidth: CGFloat = 1000

    static let columnWidth: CGFloat = 460
    static let marginWidth: CGFloat = 200
    static let marginGap: CGFloat = 48

    /// The calendar sits in the margin when there's room; otherwise above
    /// the memos.
    private var showsMargin: Bool {
        pageWidth >= Self.columnWidth + Self.marginGap + Self.marginWidth + 80
    }

    /// Re-read so a font change in Settings restyles the page.
    private var textFont: Font {
        _ = editorFont
        return Font(MemoTypography.font(size: TextSize.emphasis.rawValue))
    }

    var body: some View {
        let memos = store.memos
        let counts = Dictionary(grouping: memos, by: \.day).mapValues(\.count)
        let shown = memos.filter { memo in
            (selectedDay == nil || memo.day == selectedDay) && (selectedTag == nil || memo.tags.contains(selectedTag!))
        }

        ScrollView {
            HStack(alignment: .top, spacing: Self.marginGap) {
                stream(memos: memos, shown: shown, counts: counts)
                    .frame(width: Self.columnWidth)
                if showsMargin {
                    Color.clear.frame(width: Self.marginWidth, height: 1)
                }
            }
            .padding(.top, 34)
            .padding(.bottom, 48)
            .frame(maxWidth: .infinity)
            .background(QuietScrollerInstaller())
        }
        .overlay(alignment: .top) { PageEdgeBlur() }
        // The margin stays put while the memos scroll by.
        .overlay(alignment: .top) {
            if showsMargin {
                HStack(alignment: .top, spacing: Self.marginGap) {
                    Color.clear.frame(width: Self.columnWidth, height: 1)
                        .allowsHitTesting(false)
                    MemoMargin(memos: memos, counts: counts, month: $month,
                               selectedDay: $selectedDay, selectedTag: $selectedTag)
                        .frame(width: Self.marginWidth, alignment: .leading)
                }
                .padding(.top, 36)
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { pageWidth = $0 }
        .scrollContentBackground(.hidden)
        .background(Color.pageContentBg)
        .navigationTitle("Memos")
        .navigationSubtitle("")
        // No title header of its own; the tab strip when there are tabs.
        .modifier(MemosTabStrip())
    }

    private func stream(memos: [Memo], shown: [Memo], counts: [String: Int]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            MemoComposer { store.addMemo($0) }

            if !showsMargin {
                HStack {
                    Spacer(minLength: 0)
                    MemoMonthCalendar(memos: memos, counts: counts, month: $month, selectedDay: $selectedDay)
                        .frame(width: Self.marginWidth, alignment: .leading)
                }
                .padding(.top, 22)
                .zIndex(1)
            }

            filterLine

            if memos.isEmpty {
                Text("Memos are saved in the Memos folder, one file per day.")
                    .font(.fumi(.caption))
                    .foregroundColor(.faintText)
                    .padding(.top, 28)
            }

            let groups = Self.groupedByDay(shown)
            ForEach(groups.indices, id: \.self) { index in
                MemoDayGroup(day: groups[index].day, isFirst: index == 0) {
                    ForEach(groups[index].memos) { memo in
                        MemoRow(memo: memo, font: textFont, onTag: { tag in selectedTag = tag })
                    }
                }
            }
            .padding(.top, 12)
        }
    }

    /// What the memos are narrowed to, each with an × that clears it.
    @ViewBuilder private var filterLine: some View {
        if selectedDay != nil || selectedTag != nil {
            HStack(spacing: 16) {
                if let day = selectedDay {
                    FilterLabel(label: Self.longDayLabel(day)) { selectedDay = nil }
                }
                if let tag = selectedTag {
                    FilterLabel(label: "#" + tag) { selectedTag = nil }
                }
            }
            .padding(.top, 18)
        }
    }

    // MARK: Grouping and labels

    static func groupedByDay(_ memos: [Memo]) -> [(day: String, memos: [Memo])] {
        var groups: [(day: String, memos: [Memo])] = []
        for memo in memos {
            if groups.last?.day == memo.day {
                groups[groups.count - 1].memos.append(memo)
            } else {
                groups.append((memo.day, [memo]))
            }
        }
        return groups
    }

    /// "Friday, October 9", with the year when it isn't this one.
    static func longDayLabel(_ day: String) -> String {
        guard let date = MemoFormat.date(ofDay: day) else { return day }
        let calendar = Calendar.current
        let sameYear = calendar.component(.year, from: date) == calendar.component(.year, from: Date())
        return date.formatted(sameYear ? .dateTime.weekday(.wide).month(.wide).day()
                                       : .dateTime.weekday(.wide).month(.wide).day().year())
    }
}

// MARK: - Composer

/// The writing line: no box, just the placeholder on the paper and a
/// hairline under it; grows with its text. Return starts a new line; Save
/// (or ⌘↩) adds the memo to today's file.
struct MemoComposer: View {
    var placeholder = "Write it down…"
    let onSubmit: (String) -> Void
    @State private var draft = ""
    @State private var focusRequest = 1

    private var isEmpty: Bool { draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            MemoTextEditor(text: $draft, placeholder: placeholder, minHeight: 26,
                           focusRequest: focusRequest, onSubmit: submit)
                .padding(.bottom, 12)
                .overlay(alignment: .bottom) {
                    Rectangle().fill(Color.primaryText.opacity(0.09)).frame(height: 1)
                }
            HStack(spacing: 14) {
                QuietTextButton(title: "# Tag") {
                    draft += (draft.isEmpty || draft.hasSuffix(" ") || draft.hasSuffix("\n")) ? "#" : " #"
                    focusRequest += 1
                }
                .help("Add a tag")
                Spacer(minLength: 0)
                Button("Save", action: submit)
                    .buttonStyle(MemoSaveButtonStyle())
                    .disabled(isEmpty)
                    .keyboardShortcut(.return, modifiers: .command)
                    .help("Save (⌘↩)")
            }
            .padding(.top, 8)
        }
    }

    private func submit() {
        guard !isEmpty else { return }
        onSubmit(draft)
        draft = ""
    }
}

/// Small gray text that darkens on hover.
private struct QuietTextButton: View {
    let title: String
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.fumi(.caption))
                .foregroundColor(isHovering ? .secondaryText : .faintText)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
    }
}

/// The composer's Save: faint while there's nothing to save, then the
/// accent, on an accent wash under the pointer that deepens when pressed.
private struct MemoSaveButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Label(configuration: configuration)
    }

    private struct Label: View {
        let configuration: ButtonStyleConfiguration
        @Environment(\.isEnabled) private var isEnabled
        @State private var isHovering = false

        var body: some View {
            let wash = !isEnabled ? 0 : configuration.isPressed ? 0.16 : isHovering ? 0.10 : 0
            configuration.label
                .font(.fumi(.caption))
                .foregroundColor(isEnabled ? .accent : .faintText)
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous).fill(Color.accent.opacity(wash))
                )
                .contentShape(Rectangle())
                // The word lines up with the hairline's end; the wash reaches past it.
                .padding(.trailing, -10)
                .onHover { isHovering = $0 }
                .animation(.easeOut(duration: 0.12), value: wash)
        }
    }
}

// MARK: - Margin: calendar and tags

private struct MemoMargin: View {
    let memos: [Memo]
    let counts: [String: Int]
    @Binding var month: Date
    @Binding var selectedDay: String?
    @Binding var selectedTag: String?

    private var tags: [(name: String, count: Int)] {
        var counts: [String: Int] = [:]
        for memo in memos {
            for tag in Set(memo.tags) { counts[tag, default: 0] += 1 }
        }
        return counts.sorted { $0.value != $1.value ? $0.value > $1.value : $0.key < $1.key }
            .prefix(12)
            .map { ($0.key, $0.value) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            MemoMonthCalendar(memos: memos, counts: counts, month: $month, selectedDay: $selectedDay)
                .zIndex(1)
            let tags = tags
            if !tags.isEmpty {
                Rectangle().fill(Color.primaryText.opacity(0.09)).frame(height: 1)
                    .padding(.top, 18)
                    .padding(.bottom, 14)
                Text("Tags")
                    .font(.fumi(.body, design: .serif).italic())
                    .foregroundColor(.faintText)
                    .padding(.bottom, 8)
                VStack(alignment: .leading, spacing: 7) {
                    ForEach(tags, id: \.name) { tag in
                        TagLine(name: tag.name, count: tag.count, isSelected: selectedTag == tag.name) {
                            selectedTag = selectedTag == tag.name ? nil : tag.name
                        }
                    }
                }
            }
        }
    }
}

private struct TagLine: View {
    let name: String
    let count: Int
    let isSelected: Bool
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Text("#" + name)
                    .font(.fumi(.caption, weight: isSelected ? .semibold : .regular))
                    .foregroundColor(Color.accent.opacity(isHovering && !isSelected ? 0.75 : 1))
                    .underline(isSelected, color: .accent)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
                Text("\(count)")
                    .font(.fumi(.caption))
                    .monospacedDigit()
                    .foregroundColor(.faintIcon)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .help(isSelected ? "Show all memos" : "Show memos tagged #\(name)")
    }
}

/// A month, Monday first. A day's number sits on an accent wash that
/// deepens in four steps with how many memos it has; today has an accent
/// ring. Hover a day for its memos, click it to see only that day.
struct MemoMonthCalendar: View {
    let memos: [Memo]
    let counts: [String: Int]
    @Binding var month: Date
    @Binding var selectedDay: String?
    @Environment(\.colorScheme) private var colorScheme

    @State private var hoveredDay: String?
    @State private var cardDay: String?
    @State private var cardTask: Task<Void, Never>?

    static let cell: CGFloat = 28
    static let rowHeight: CGFloat = 30
    static let cardWidth: CGFloat = 250
    private static let weekdays = ["M", "T", "W", "T", "F", "S", "S"]

    /// The wash for levels 0…4; a step stronger on a dark page.
    private var wash: [Double] {
        colorScheme == .dark ? [0, 0.16, 0.26, 0.38, 0.52] : [0, 0.09, 0.16, 0.24, 0.32]
    }

    var body: some View {
        let weeks = MemoHeatmap.month(containing: month)
        let days = weeks.joined().compactMap { $0 }.map(MemoFormat.day(of:))
        let monthCount = days.reduce(0) { $0 + (counts[$1] ?? 0) }
        let activeDays = days.filter { (counts[$0] ?? 0) > 0 }.count

        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline) {
                (Text(month.formatted(.dateTime.month(.wide)))
                    .font(.fumi(.emphasis, weight: .semibold, design: .serif))
                    .foregroundColor(.primaryText)
                 + Text(" " + month.formatted(.dateTime.year()))
                    .font(.fumi(.emphasis, design: .serif))
                    .foregroundColor(.faintText))
                Spacer(minLength: 0)
                HStack(spacing: 2) {
                    MonthArrow(symbol: "chevron.left", label: "Previous Month") { shift(-1) }
                    MonthArrow(symbol: "chevron.right", label: "Next Month") { shift(1) }
                }
            }
            .padding(.bottom, 10)

            HStack(spacing: 0) {
                ForEach(Self.weekdays.indices, id: \.self) { index in
                    Text(Self.weekdays[index])
                        .font(.fumi(.caption))
                        .foregroundColor(.faintText)
                        .frame(width: Self.cell)
                }
            }
            .padding(.bottom, 2)

            ForEach(weeks.indices, id: \.self) { row in
                HStack(spacing: 0) {
                    ForEach(0..<7, id: \.self) { column in
                        if let date = weeks[row][column] {
                            dayCell(date, column: column)
                        } else {
                            Color.clear.frame(width: Self.cell, height: Self.rowHeight)
                        }
                    }
                }
                .zIndex(weeks[row].contains { $0.map(MemoFormat.day(of:)) == cardDay } ? 1 : 0)
            }

            Text(monthCount == 0 ? "Nothing yet this month"
                 : "\(monthCount) memo\(monthCount == 1 ? "" : "s") · \(activeDays) day\(activeDays == 1 ? "" : "s")")
                .font(.fumi(.caption))
                .foregroundColor(.faintText)
                .padding(.top, 10)
        }
        .frame(width: Self.cell * 7, alignment: .leading)
    }

    private func dayCell(_ date: Date, column: Int) -> some View {
        let day = MemoFormat.day(of: date)
        let count = counts[day] ?? 0
        let level = MemoHeatmap.level(for: count)
        let isToday = Calendar.current.isDateInToday(date)
        let isSelected = selectedDay == day
        let shape = RoundedRectangle(cornerRadius: 7, style: .continuous)
        let numberColor: Color = isSelected ? .white : (isToday ? .accent : (count > 0 ? .primaryText : .faintIcon))
        return Button {
            guard count > 0 || isSelected else { return }
            selectedDay = isSelected ? nil : day
            cardDay = nil
        } label: {
            Text("\(Calendar.current.component(.day, from: date))")
                .font(.fumi(.caption, weight: isSelected || isToday ? .semibold : (count > 0 ? .medium : .regular)))
                .monospacedDigit()
                .foregroundColor(numberColor)
                .frame(width: 24, height: 24)
                .background(shape.fill(isSelected ? Color.accent : Color.accent.opacity(wash[level])))
                .overlay(shape.strokeBorder(Color.accent, lineWidth: isToday && !isSelected ? 1.5 : 0))
                .frame(width: Self.cell, height: Self.rowHeight)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { inside in hover(day, count: count, inside: inside) }
        .overlay(alignment: .top) {
            if cardDay == day {
                MemoDayCard(day: day, memos: memos.filter { $0.day == day })
                    // Right edge with the calendar's, just under the day.
                    .offset(x: Self.cell * 7 - Self.cardWidth / 2 - (CGFloat(column) * Self.cell + Self.cell / 2),
                            y: Self.rowHeight + 2)
                    .transition(.opacity)
                    .allowsHitTesting(false)
            }
        }
        .accessibilityLabel("\(date.formatted(.dateTime.month(.wide).day())), \(count) memo\(count == 1 ? "" : "s")")
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }

    /// The card waits a moment before it shows, then follows the pointer
    /// from day to day without waiting again.
    private func hover(_ day: String, count: Int, inside: Bool) {
        cardTask?.cancel()
        if inside {
            hoveredDay = day
            guard count > 0 else {
                cardDay = nil
                return
            }
            if cardDay != nil {
                cardDay = day
                return
            }
            cardTask = Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(350))
                guard !Task.isCancelled, hoveredDay == day else { return }
                withAnimation(.easeOut(duration: 0.12)) { cardDay = day }
            }
        } else if hoveredDay == day {
            hoveredDay = nil
            cardTask = Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(80))
                guard !Task.isCancelled, hoveredDay == nil else { return }
                withAnimation(.easeOut(duration: 0.12)) { cardDay = nil }
            }
        }
    }

    private func shift(_ months: Int) {
        if let next = Calendar.current.date(byAdding: .month, value: months, to: month) { month = next }
    }
}

private struct MonthArrow: View {
    let symbol: String
    let label: String
    let action: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
                .foregroundColor(isHovering ? .secondaryText : .faintText)
                .frame(width: 20, height: 20)
                .background(Circle().fill(isHovering ? Color.cardHover : .clear))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .onHover { isHovering = $0 }
        .accessibilityLabel(label)
    }
}

/// A day's memos at a glance: the date, how many, and each memo's time and
/// first line.
private struct MemoDayCard: View {
    let day: String
    let memos: [Memo]

    private static let shown = 5

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(MemosView.longDayLabel(day))
                    .font(.fumi(.body, weight: .semibold, design: .serif))
                    .foregroundColor(.primaryText)
                    .lineLimit(1)
                Spacer(minLength: 8)
                Text("\(memos.count) memo\(memos.count == 1 ? "" : "s")")
                    .font(.fumi(.caption))
                    .foregroundColor(.faintText)
                    .fixedSize()
            }
            VStack(alignment: .leading, spacing: 5) {
                ForEach(memos.prefix(Self.shown)) { memo in
                    HStack(alignment: .firstTextBaseline, spacing: 10) {
                        Text(memo.time)
                            .font(Font(MemoTypography.italic(size: TextSize.caption.rawValue)))
                            .monospacedDigit()
                            .foregroundColor(.faintText)
                            .frame(width: 36, alignment: .leading)
                        Text(Self.firstLine(of: memo.text))
                            .font(Font(MemoTypography.font(size: TextSize.caption.rawValue)))
                            .foregroundColor(.primaryText)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                if memos.count > Self.shown {
                    Text("\(memos.count - Self.shown) more")
                        .font(.fumi(.caption))
                        .foregroundColor(.faintText)
                        .padding(.leading, 46)
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(width: MemoMonthCalendar.cardWidth, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(Color.editorBg)
                .shadow(color: .black.opacity(0.14), radius: 20, y: 10)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .strokeBorder(Color.primaryText.opacity(0.08), lineWidth: 1)
        )
    }

    static func firstLine(of text: String) -> String {
        text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty } ?? ""
    }
}

/// One day of memos, diary style (design boards ㉜ B, ㊷ A): the date hangs
/// in a column on the left — the day's number, then the weekday (and the month,
/// outside the current one) — with that day's memos beside it.
private struct MemoDayGroup<Content: View>: View {
    let day: String
    let isFirst: Bool
    @ViewBuilder let content: () -> Content

    static var dateColumn: CGFloat { 40 }

    private var date: Date? { MemoFormat.date(ofDay: day) }

    private var subtitle: String {
        guard let date else { return "" }
        let weekday = date.formatted(.dateTime.weekday(.abbreviated))
        let calendar = Calendar.current
        let now = Date()
        if calendar.component(.year, from: date) != calendar.component(.year, from: now) {
            return weekday + " · " + date.formatted(.dateTime.month(.abbreviated).year(.twoDigits))
        }
        if calendar.component(.month, from: date) != calendar.component(.month, from: now) {
            return weekday + " · " + date.formatted(.dateTime.month(.abbreviated))
        }
        return weekday
    }

    var body: some View {
        HStack(alignment: .top, spacing: 20) {
            VStack(alignment: .trailing, spacing: 4) {
                Text(date.map { String(Calendar.current.component(.day, from: $0)) } ?? day)
                    .font(Font(MemoTypography.font(size: TextSize.title.rawValue)))
                    .foregroundColor(.primaryText)
                Text(subtitle)
                    .font(Font(MemoTypography.italic(size: TextSize.caption.rawValue)))
                    .foregroundColor(.faintText)
                    .fixedSize()
            }
            .frame(width: Self.dateColumn, alignment: .trailing)
            .padding(.top, 2)
            VStack(alignment: .leading, spacing: 12) {
                content()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.top, 16)
        .padding(.bottom, 4)
        .overlay(alignment: .top) {
            if !isFirst { Rectangle().fill(Color.primaryText.opacity(0.07)).frame(height: 1) }
        }
    }
}

/// A filter as plain words with an × after them; a click clears it.
private struct FilterLabel: View {
    let label: String
    let onClear: () -> Void
    @State private var isHovering = false

    var body: some View {
        Button(action: onClear) {
            HStack(spacing: 6) {
                Text(label)
                    .font(.fumi(.caption))
                    .foregroundColor(isHovering ? .primaryText : .secondaryText)
                Image(systemName: "xmark")
                    .font(.system(size: 8.5, weight: .semibold))
                    .foregroundColor(isHovering ? .secondaryText : .faintText)
            }
        }
        .buttonStyle(FilterLabelStyle(isHovering: isHovering))
        .onHover { isHovering = $0 }
        .animation(.easeOut(duration: 0.12), value: isHovering)
        .help("Show all memos")
    }

    /// A gray wash under the pointer, deeper while pressed; the label stays
    /// in line with the memos, the wash reaching past it.
    private struct FilterLabelStyle: ButtonStyle {
        let isHovering: Bool

        func makeBody(configuration: Configuration) -> some View {
            configuration.label
                .padding(.horizontal, 8)
                .frame(height: 24)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(Color.primaryText.opacity(configuration.isPressed ? 0.08 : isHovering ? 0.05 : 0))
                )
                .contentShape(Rectangle())
                .padding(.horizontal, -8)
        }
    }
}

// MARK: - Memo row

/// One memo: its time, then the text. Hover for edit / copy / open the day's
/// file / delete.
private struct MemoRow: View {
    let memo: Memo
    let font: Font
    let onTag: (String) -> Void
    @Environment(FileNoteStore.self) private var store
    @Environment(AppModel.self) private var appModel
    @State private var isHovering = false
    @State private var isEditing = false
    @State private var editText = ""
    @State private var confirmingDelete = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(memo.time)
                    .font(Font(MemoTypography.italic(size: TextSize.caption.rawValue)))
                    .monospacedDigit()
                    .foregroundColor(.faintText)
                Spacer(minLength: 0)
                menu.opacity(isHovering && !isEditing ? 1 : 0)
            }
            if isEditing {
                editor
            } else {
                MemoBody(text: memo.text, font: font) { line in
                    store.updateMemo(memo, text: MemoMarkdown.togglingTask(line: line, in: memo.text))
                }
                    .tint(.accent)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .environment(\.openURL, OpenURLAction { url in
                        guard url.scheme == "fumi-tag" else { return .systemAction }
                        onTag(url.host(percentEncoded: false) ?? "")
                        return .handled
                    })
            }
        }
        .padding(.bottom, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .onHover { isHovering = $0 }
        .confirmationDialog("Delete this memo?", isPresented: $confirmingDelete) {
            Button("Delete", role: .destructive) { store.deleteMemo(memo) }
        }
    }

    private var menu: some View {
        Menu {
            Button("Edit") {
                editText = memo.text
                isEditing = true
            }
            Button("Copy") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(memo.text, forType: .string)
            }
            Button("Show in File") {
                if let note = store.memoNote(forDay: memo.day) { appModel.openNote(note.id) }
            }
            Divider()
            Button("Delete…", role: .destructive) { confirmingDelete = true }
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 12, weight: .semibold))
                .foregroundColor(.iconMuted)
                .frame(width: 24, height: 18)
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
    }

    private var editor: some View {
        VStack(alignment: .trailing, spacing: 8) {
            MemoTextEditor(text: $editText, minHeight: 30, focusRequest: 1,
                           onSubmit: saveEdit, onCancel: { isEditing = false })
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .background(RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color.sidebarBg))
            HStack(spacing: 8) {
                SettingsButton(title: "Cancel") { isEditing = false }
                    .keyboardShortcut(.cancelAction)
                SettingsButton(title: "Save", action: saveEdit)
                .keyboardShortcut(.return, modifiers: .command)
            }
        }
    }

    private func saveEdit() {
        store.updateMemo(memo, text: editText)
        isEditing = false
    }

    /// The memo with inline Markdown (bold, italic, code, links) and its
    /// `#tags` as clickable runs in the accent.
    static func attributed(_ text: String, size: CGFloat) -> AttributedString {
        let ns = text as NSString
        func inline(_ s: String) -> AttributedString {
            (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)))
                ?? AttributedString(s)
        }
        var result = AttributedString()
        var cursor = 0
        for tag in MemoFormat.tagRanges(in: text) {
            if tag.range.location > cursor {
                result += inline(ns.substring(with: NSRange(location: cursor, length: tag.range.location - cursor)))
            }
            var run = AttributedString(ns.substring(with: tag.range))
            run.foregroundColor = Color.accent
            if let encoded = tag.name.addingPercentEncoding(withAllowedCharacters: .urlHostAllowed) {
                run.link = URL(string: "fumi-tag://" + encoded)
            }
            result += run
            cursor = NSMaxRange(tag.range)
        }
        if cursor < ns.length {
            result += inline(ns.substring(from: cursor))
        }
        MemoTypography.styleRuns(&result, size: size)
        return result
    }
}

/// A saved memo's Markdown (MemoMarkdown.blocks): paragraphs, lists whose
/// to-dos tick on a click, quotes and code blocks, drawn like the editor's.
private struct MemoBody: View {
    let text: String
    let font: Font
    /// A to-do's line in the memo was clicked.
    let onToggle: (Int) -> Void

    private static let size = TextSize.emphasis.rawValue
    private static let slot: CGFloat = 22

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(MemoMarkdown.blocks(text).enumerated()), id: \.offset) { _, block in
                view(for: block)
            }
        }
    }

    private func inline(_ text: String, color: Color = .primaryText) -> Text {
        Text(MemoRow.attributed(text, size: Self.size))
            .font(font)
            .foregroundColor(color)
    }

    @ViewBuilder private func view(for block: MemoMarkdown.Block) -> some View {
        switch block {
        case .paragraph(let text):
            inline(text)
                .lineSpacing(6)
                .fixedSize(horizontal: false, vertical: true)
        case .item(let level, let marker, let text, let line):
            HStack(alignment: .firstTextBaseline, spacing: 0) {
                markerView(marker, level: level, line: line)
                    .frame(minWidth: Self.slot, alignment: .leading)
                Group {
                    if case .task(true) = marker {
                        inline(text, color: .tertiaryText).strikethrough(color: .faintText)
                    } else {
                        inline(text)
                    }
                }
                .lineSpacing(6)
                .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.leading, CGFloat(level) * Self.slot)
        case .quote(let text):
            inline(text, color: .secondaryText)
                .lineSpacing(6)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.leading, 14)
                .overlay(alignment: .leading) {
                    Capsule().fill(Color.faintIcon).frame(width: 2)
                }
        case .code(_, let text):
            Text(text)
                .font(.fumi(.caption, design: .monospaced))
                .foregroundColor(.primaryText)
                .lineSpacing(3)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color.codeBlockBg))
        }
    }

    // Each marker sits on the text's baseline, centered on its x-height
    // like the editor's drawn ones (CodeBlockLayoutManager).
    @ViewBuilder private func markerView(_ marker: MemoMarkdown.Marker, level: Int, line: Int) -> some View {
        switch marker {
        case .bullet:
            Group {
                switch level % 3 {
                case 0: Circle().fill(Color.primaryText)
                case 1: Circle().strokeBorder(Color.primaryText, lineWidth: 1.2)
                default: Rectangle().fill(Color.primaryText).padding(0.5)
                }
            }
            .frame(width: 5, height: 5)
            .padding(.leading, 4)
            .alignmentGuide(.firstTextBaseline) { $0[.bottom] + 2.5 }
        case .ordered(let label):
            Text(label)
                .font(font)
                .monospacedDigit()
                .foregroundColor(.primaryText)
                .padding(.trailing, 6)
        case .task(let checked):
            Button { onToggle(line) } label: {
                ZStack {
                    if checked {
                        RoundedRectangle(cornerRadius: 4, style: .continuous).fill(Color.accent)
                        Image(systemName: "checkmark")
                            .font(.system(size: 8.5, weight: .heavy))
                            .foregroundColor(.white)
                    } else {
                        RoundedRectangle(cornerRadius: 4, style: .continuous)
                            .strokeBorder(Color.faintIcon, lineWidth: 1.5)
                    }
                }
                .frame(width: 15, height: 15)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(checked ? "Mark as not done" : "Mark as done")
            .alignmentGuide(.firstTextBaseline) { $0[.bottom] - 2 }
        }
    }
}

/// Memos keeps the system title; with more than one tab open the toolbar
/// shows the tab strip instead, as over notes.
private struct MemosTabStrip: ViewModifier {
    @Environment(AppModel.self) private var appModel

    func body(content: Content) -> some View {
        if appModel.showsTabStrip {
            content
                .modifier(RemovesSystemTitle())
                .toolbar { NoteTitleHeader() }
        } else {
            content
        }
    }
}
