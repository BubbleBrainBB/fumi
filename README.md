# Fumi

Fumi (文, "writing") was called Notely until 0.3.0; the code and repository still use that name.

A calm Markdown notes app for macOS, written in SwiftUI and AppKit. Your notes are plain `.md` files in a folder on your Mac — open the same folder in Obsidian or any editor.

## Install

Download the DMG from [Releases](https://github.com/BubbleBrainBB/fumi/releases), open it and drag Fumi to Applications. Builds are for Apple silicon. Fumi doesn't update itself yet, so new versions are downloaded the same way.

## Features

- **Live preview editing.** Markdown renders as you type; the syntax of the line you're editing is revealed, Obsidian style — headings, lists, quotes, callouts, code, tables, math, images, local videos and YAML frontmatter. Type `/` to insert a block.
- **Plain files.** A notes library is just a folder; subfolders show as a tree in the sidebar. Works with an existing Obsidian vault as it is, including `[[wiki links]]`, and edits made in other apps are merged in.
- **Libraries and tabs.** Keep several libraries in the sidebar and open several pages in one window as tabs.
- **Memos.** Quick notes, kept as one Markdown file per day, with a month calendar and tags.
- **Quick search.** ⇧⌘F searches file names, titles, folders, tags (`#tag`) and note text.
- **Publishing.** Preview a note beside the page as a WeChat article or as Xiaohongshu cards, ready to copy or export. Notes also export as PDF.
- **Typography.** One line height for all body text, spacing between Chinese and Latin text, and a composite font that pairs a Latin family with Songti or PingFang.
- **Settings** for theme (with dark mode and Liquid Glass on macOS 26), accent color, font, size, line spacing and text width, with a live preview.

## Requirements

- macOS 14 or later
- Xcode 26 or later (the macOS 26 SDK) to build

## Build and run

```bash
xcodebuild -project Notely.xcodeproj -scheme Notely -configuration Debug -derivedDataPath build/dd build
open build/dd/Build/Products/Debug/Fumi.app
```

Or open `Notely.xcodeproj` in Xcode and run the Notely scheme.

## Release

```bash
Tools/release.sh
```

Builds Release, signs it with Developer ID and the hardened runtime, and writes a signed DMG to `build/release`. Add `--notarize` to send the DMG to Apple and staple the ticket; the top of the script says how to store the credentials.

## Tests

There is no Xcode test target. Each file in `Tests/` is a standalone runner; the `swiftc` command that builds it is in the comment at its top. Run from the repository root, for example:

```bash
swiftc Notely/Views/Editor/CursorMapping.swift Tests/CursorMappingTests.swift -o /tmp/cursormap-tests && /tmp/cursormap-tests
```

## Project structure

```
.
├── Notely/
│   ├── App/          # Window shell, app model, window behavior
│   ├── Models/       # Notes and settings
│   ├── Services/     # File store, search, frontmatter, titles, tags
│   ├── Utils/        # Colors, fonts, icons
│   ├── Views/
│   │   ├── Editor/   # Live-preview engine, text view, attachments
│   │   ├── Sidebar/  # Notes tree, search panel
│   │   ├── Settings/
│   │   └── FolderOpenView.swift   # Welcome page (first launch)
│   └── Resources/    # Asset catalog, app icon
├── Notely.xcodeproj
├── Tests/            # Standalone test runners
└── Tools/            # Release script, app icon generator, WeChat renderer build
```

## License

[MIT](LICENSE)
