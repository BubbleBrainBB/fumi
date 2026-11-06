# Fumi

Fumi (文, "writing") is a calm Markdown notes app for macOS, written in Rust on [GPUI](https://www.gpui.rs), through Longbridge's [gpui-fast](https://github.com/longbridge/gpui-fast) fork. Your notes are plain `.md` files in a folder on your Mac — open the same folder in Obsidian or any editor.

Up to 0.3.2 Fumi was a SwiftUI and AppKit app; that code is in this repository's history and the `v0.3.x` tags. 0.4.0 is the same app rebuilt in Rust, with the same features and look, and much faster on large notes.

## Install

Download the DMG from [Releases](https://github.com/BubbleBrainBB/fumi/releases), open it and drag Fumi to Applications. Builds are for Apple silicon. Fumi doesn't update itself yet, so new versions are downloaded the same way.

## Features

- **Live preview editing.** Markdown renders as you type; the syntax you're editing is revealed, Obsidian style — headings, lists, quotes, callouts, code, tables, math, images, local videos and YAML frontmatter. Type `/` to insert a block.
- **Plain files.** A notes library is just a folder; subfolders show as a tree in the sidebar. Works with an existing Obsidian vault as it is, including `[[wiki links]]`, and edits made in other apps are merged in.
- **Libraries and tabs.** Keep several libraries in the sidebar and open several pages in one window as tabs.
- **Memos.** Quick notes, kept as one Markdown file per day, with a month calendar and tags.
- **Quick search.** ⇧⌘F searches file names, titles, folders, tags (`#tag`) and note text.
- **Publishing.** Preview a note beside the page as a WeChat article or as Xiaohongshu cards, ready to copy or export. Notes also export as PDF.
- **Typography.** One line height for all body text, spacing between Chinese and Latin text, and a composite font that pairs a Latin family with Songti or PingFang.
- **Settings** for theme (with dark mode and Liquid Glass on macOS 26), accent color, font, size, line spacing and text width.

## Requirements

- macOS 14 or later, Apple silicon
- Rust 1.95 (pinned in `rust-toolchain.toml`) and the Xcode command line tools to build

## Build and run

```bash
cargo build --release
scripts/bundle.sh
open target/FumiGPUI.app
```

`scripts/bundle.sh` makes a test bundle with its own id (`com.fumi.gpui-prototype`), so it keeps settings apart from an installed Fumi. `FUMI_WORKSPACE=<folder>` opens a library without touching the one in Settings.

## GPUI

Fumi builds on gpui-fast 0.1.13 rather than the `gpui-pre` snapshot: `Cargo.toml` aliases `gpui` and `gpui_platform` to `gpui-fast` and `gpui-fast-platform`. What that changes:

- **Window composition.** The publish pane's web views live in a native composition surface, so Search, Settings, the Info card and menus, all deferred draws, paint above the live page. A view drawn over a web view has to be deferred to land above it.
- **Retained views.** A view whose entities, globals and list or scroll state didn't change is drawn from the last frame. Render state kept anywhere else (a `thread_local`, an `Rc<RefCell>`, the time) needs a `cx.notify()` when it changes, or the view keeps showing the old frame. `GPUI_VIEW_RETENTION=0` turns retention off, to tell whether a stale frame comes from it.
- **Scroll layers are off.** `main` sets `GPUI_SCROLL_LAYERS=0` before GPUI starts (an explicit value in the environment wins): with them on, typing in the editor's list was about 1 ms slower per frame and hitched 20–80 ms once per session. `docs/spikes/gpui-fast.md` has the measurements.

Two patches in `vendor/`, both in `[patch.crates-io]`:

- `gpui-fast-macos` with three `Option::map_or_default` calls replaced, as that API isn't stable on Rust 1.95. Drop it on Rust 1.99.
- `gpui-fast-compat/gpui-pre`, a stand-in for `gpui-pre` that re-exports gpui-fast, so `ratex-gpui` doesn't build a second GPUI.

## Release

```bash
scripts/release.sh
```

Builds Release with the shipping id `com.notes.app`, signs it with Developer ID and the hardened runtime, and writes a signed DMG to `target/dist`. Add `--notarize` to send the DMG to Apple and staple the ticket.

## Tests

```bash
cargo test --workspace
```

Benchmarks: `scripts/matrix.sh` drives the editor alone; `BENCH_SCENARIO=type BENCH_LIB=<library> BENCH_DOC=<note>` drives the whole app (`src/autopilot.rs`). `results/compare-0.3.2-vs-0.4.0` has a run against the Swift app.

## Project structure

```
.
├── src/                 # The app: window, page host, benchmark driver
├── crates/
│   ├── fumi-core/       # Logic without UI: notes, sync and merge, search, memos, Markdown
│   ├── fumi-editor/     # The live-preview editor and its own text layout
│   ├── fumi-ui/         # Theme, icons, text field, native menus and other AppKit glue
│   ├── fumi-workspace/  # A window's library and navigation
│   ├── fumi-sidebar/    # Sidebar, note tree, libraries
│   ├── fumi-toolbar/    # Toolbar, tabs, menu bar, windows
│   ├── fumi-memos/      # Memos and quick capture
│   ├── fumi-settings/   # Settings, search, welcome page, file preview
│   └── fumi-publish/    # WeChat, Xiaohongshu, PDF
├── resources/           # App icon
├── scripts/             # Bundle, release, benchmarks
├── tools/               # WeChat renderer build, app icon generator
├── vendor/              # Patched gpui-fast macOS crate, gpui-pre stand-in
├── PARITY.md            # The checklist the rebuild was held to, against the Swift app
├── docs/parity-audit/   # Where the rebuild was checked against it
└── docs/spikes/         # gpui-fast evaluation (history)
```

## License

[MIT](LICENSE)
