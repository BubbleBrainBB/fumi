# Fumi parity checklist

Acceptance checklist for the Rust + GPUI rebuild in this repo, against the Swift app in `~/bubble/notes-app/Fumi`. Every item traces to the Swift source named at its end; `File.swift:line` is the file's base name under `Fumi/`. UI-free logic already ported to `crates/fumi-core` is not itemized here. Sizes are points unless marked.

## Design tokens

### Theme palette (light / dark)

`ThemePalette.swift`. Default theme White, default mode Light. `card` and `selected` are defined but never drawn (Z3).

| Token | Use | White | Paper | Glass | Linen | Mist |
|---|---|---|---|---|---|---|
| `ground` | window ground, sidebar sits on it | #F6F6F7 / #161618 | #EFEEEA / #1C1B19 | #F5F5F7 / #262628 | #F1EEE7 / #1D1A16 | #ECEFF3 / #13171C |
| `groundSelected` | selected sidebar row | #EAEAEC / #2A2A2D | #E2E1DC / #2F2E2B | #FFFFFF / #3A3A3C | #E6E1D6 / #332E27 | #DCE1E8 / #29313B |
| `page` | page, popovers, cards | #FFFFFF / #1F1F21 | #FFFFFF / #262523 | #FFFFFF / #1C1C1E | #FFFEFC / #27231E | #FFFFFF / #1B2027 |
| `card` | settings group (unused, see Z3) | #F5F5F7 / #262628 | #F6F5F2 / #2F2E2B | #F2F2F4 / #2C2C2E | #F7F4EE / #302B25 | #F3F5F8 / #232931 |
| `sidebar` | memo edit field | #F6F6F7 / #161618 | #F7F7F5 / #1C1B19 | #F5F5F7 / #262628 | #F1EEE7 / #1D1A16 | #ECEFF3 / #13171C |
| `selected` | selected row on the page (unused, see Z3) | #EFEFF1 / #2C2C2E | #EBEBE9 / #353431 | #E8E8ED / #3A3A3C | #ECE8DF / #38322A | #E3E7EC / #2A313A |
| `border` | pane dividers | #EAEAEC / #2C2C2E | #EBEBE9 / #34332F | #E5E5EA / #2E2E30 | #ECE8DF / #37312A | #E6EAEF / #2A313A |
| `controlBorder` | hairline on controls | #E0E0E3 / #38383A | #E3E3E0 / #3D3C38 | #DCDCE0 / #3A3A3C | #E4DFD4 / #3D372F | #DEE3EA / #303843 |
| `text` | body ink | #1D1D1F / #F5F5F7 | #37352F / #E9E7E2 | #1D1D1F / #F5F5F7 | #3A342B / #ECE5D8 | #27303A / #E4E9EF |
| `text2` | secondary text | #3C3C43 / #D1D1D6 | #55534E / #C3C0B9 | #3C3C43 / #D1D1D6 | #5C574D / #C8BFAF | #4A5562 / #B8C2CE |
| `text3` | tertiary text | #6E6E73 / #98989F | #6B6A66 / #9A978F | #6E6E73 / #98989F | #716A5E / #9E9586 | #616C7A / #8995A3 |
| `faintText` | quietest text | #98989D / #7C7C80 | #9B9A97 / #7A7771 | #98989D / #7C7C80 | #A39C8F / #7F7769 | #97A1AD / #6C7886 |
| `icon` | default icon stroke | #86868B / #98989F | #8F8E8A / #8F8C85 | #7C7C82 / #98989F | #958E80 / #978E7F | #8792A0 / #808C9A |
| `faintIcon` | quietest icon, checkbox border | #AEAEB2 / #636366 | #B4B3AF / #6E6C66 | #AEAEB2 / #636366 | #BDB5A6 / #6E665A | #B3BBC6 / #566170 |
| `strongInk` | bold with the Ink accent | #000000 / #FFFFFF | #111110 / #FFFFFF | #000000 / #FFFFFF | #15120E / #FFFBF2 | #0E1318 / #FFFFFF |
| `marker` | revealed Markdown syntax (Glass: see below) | #C7C7CC / #545458 | #C7C6C2 / #5E5C57 | #C7C7CC / #545458 | #CBC4B6 / #655D51 | #C3CAD3 / #4E5866 |
| `rule` | horizontal rule | #EAEAEC / #2E2E30 | #EDEDEB / #383733 | #E5E5EA / #333335 | #EDE9E0 / #3A342C | #E8ECF1 / #2C333C |
| `codeBlock` | code block fill, table header | #F5F5F7 / #242426 | #F7F7F5 / #2D2C29 | #F5F5F7 / #242426 | #F7F4EE / #2E2A24 | #F4F6F9 / #222831 |
| `inlineCode` | inline code and hashtag pill fill | #F0F0F2 / #2C2C2E | #F1F1EF / #34332F | #EFEFF2 / #2C2C2E | #F2EEE6 / #332E27 | #EEF1F5 / #262D36 |
| `linkUnderline` | link underline, nested quote bar | #D1D1D6 / #545458 | #D3D3CF / #5E5C57 | #D1D1D6 / #545458 | #D6CFC1 / #655D51 | #CDD4DD / #4E5866 |
| `tableBorder` | table lines | #E5E5EA / #38383A | #E9E9E7 / #3D3C38 | #E5E5EA / #38383A | #E9E4DA / #3D372F | #E3E8EE / #303843 |
| `highlight` | `==highlight==` fill | #FBF3DB / #3F3820 | #FBF3DB / #4A4020 | #FBF3DB / #3F3820 | #FAF0D2 / #4A3E22 | #FBF3DB / #3E3820 |

### Derived colors

`ColorTokens.swift`. `text` here means the palette token. Alpha is written as a percentage.

| Token | Light | Dark | Line |
|---|---|---|---|
| `accent` | `fumi.accentColor`, default #1D6FC2; Ink #37352F resolves to `text` | same | `ColorTokens.swift:72` |
| `accentHover` | accent 10% | same | `ColorTokens.swift:171` |
| `accentSelected` | accent 6% | same | `ColorTokens.swift:172` |
| `cardHover` | `text` 5% | same | `ColorTokens.swift:173` |
| `searchFieldBg` | `text` 4% | same | `ColorTokens.swift:174` |
| `quietSurface` | `text` 3% | same | `ColorTokens.swift:175` |
| `controlFill` (unused) | #767680 13% | same | `ColorTokens.swift:177` |
| `pageEdge` | black 6% | white 7% | `ColorTokens.swift:180` |
| `scrim` | black 16% | black 45% | `ColorTokens.swift:195` |
| `scrimShade(s)` | white-level `1 - 0.16 x s`, a multiplier | white-level `1 - 0.45 x s` | `ColorTokens.swift:191` |
| `groundFill` | `ground`; Glass: `ground` 12% | same | `ColorTokens.swift:201` |
| `pageSheet` | `page`; Glass: `page` 50% | same | `ColorTokens.swift:207` |
| `pageContentBg` | `page`; Glass: clear | same | `ColorTokens.swift:213` |
| `selectedRowShadow` | clear; Glass: black 10% | same | `ColorTokens.swift:219` |
| `marker` in Glass | `text` 26% | same | `CodeBlockLayoutManager.swift:31` |

### Other fixed colors

| Use | Light | Dark | Line |
|---|---|---|---|
| Inline code text | #C4402F | #E5735F | `CodeBlockLayoutManager.swift:39` |
| Code keyword | #9065B0 | #B794D6 | `CodeBlockLayoutManager.swift:46` |
| Code string | #448361 | #6FB08A | `CodeBlockLayoutManager.swift:47` |
| Code number | #9A6B12 | #D1A04A | `CodeBlockLayoutManager.swift:48` |
| Code comment | `faintText` | same | `WysiwygEngine.swift:1168` |

Callout, frontmatter pill and publish colors are listed with their items (A10, A17, G).

### Type scale

`ColorTokens.swift:226`. All interface text uses the system font unless noted: `Font.fumi(size, weight, design)`, default weight regular.

| Name | Size | Use |
|---|---|---|
| `caption` | 12 | secondary lines |
| `body` | 13 | body and controls |
| `emphasis` | 15 | emphasis |
| `title` | 20 | page titles |

### Modes and themes

- Themes: `white` White, `paper` Paper, `glass` Glass, `linen` Linen, `mist` Mist. `ThemePalette.swift:6`
- Modes: `light` Light, `dark` Dark, `auto` shown as Match System. `ThemePalette.swift:23`, `SettingsView.swift:459`
- Themed colors resolve at draw time, by the theme and the appearance they are drawn in. `ColorTokens.swift:131`

## A. Editor rendering

All sizes below assume the default body size 16 and line-height 1.7 (body line 27 pt) unless a formula is given. Token names refer to the Design tokens table.

### A1. Page and column

- [ ] The note's text sits in a centered column capped at 600 (`narrow`), 680 (`medium`, default) or 860 (`wide`) points. `WysiwygTextView.swift:369`
- [ ] The column keeps a side inset of at least 40 on each side; the inset is `max(40, (available - column) / 2)`. `WysiwygTextView.swift:383`, `WysiwygTextView.swift:389`
- [ ] Collapsing or expanding the sidebar never rewraps the text when the pane is at or above the cap; the inset recomputes on any pane width change. `WysiwygTextView.swift:487`
- [ ] The text column has 30 pt of space above the first line and below the last line. `WysiwygTextView.swift:419`
- [ ] The scroll view spans the full page width, so the scroller hugs the page's right edge rather than the text column. `WysiwygTextView.swift:279`
- [ ] Line fragment padding is 0: text starts exactly at the column's left edge. `WysiwygTextView.swift:174`
- [ ] Text wraps within the column; there is no horizontal scroller. `WysiwygTextView.swift:163`
- [ ] Page background inside the editor is `pageContentBg` (clear in Glass, else `page`). `EditorView.swift:120`, `ColorTokens.swift:213`
- [ ] Text scrolling past the page's top frosts and fades: a 30 pt band at the top combines a blur (mask opaque at 0-30%, 50% at 65%, clear at 100%) with a `pageContentBg` wash at 80% (0), 30% (40%), 0% (75%); it takes no clicks. `PageEdgeBlur.swift:10`, `PageEdgeBlur.swift:15`, `PageEdgeBlur.swift:44`
- [ ] Scroller: no track, always the overlay kind whatever "Show scroll bars" is set to, auto-hiding, 7 pt slot, 4 pt wide rounded thumb (radius 2) in `faintIcon` at 60%. `QuietScroller.swift:14`, `QuietScroller.swift:27`, `QuietScroller.swift:32`
- [ ] Editor scroller thumb keeps its true length with a 24 pt minimum, inset 2 at each end. `QuietScroller.swift:18`, `QuietScroller.swift:19`
- [ ] Light/dark switch re-renders text colors and rebuilds tables, math and frontmatter images in the new appearance. `WysiwygTextView.swift:785`

### A2. Caret and selection

- [ ] The caret is drawn only as tall as the font (ascender to descender), standing on the line's baseline, not the full 27 pt line. `WysiwygTextView.swift:570`, `WysiwygTextView.swift:577`
- [ ] Next to collapsed syntax the caret keeps body-font height: a typing font under 80% of body size is replaced by the body size for caret measuring. `WysiwygTextView.swift:576`
- [ ] Caret color is the accent, and follows live accent changes. `WysiwygTextView.swift:299`, `WysiwygTextView.swift:415`
- [ ] Selection background is the accent at 18% alpha. `WysiwygTextView.swift:300`
- [ ] Text typed right after collapsed syntax takes the body style, never the 0.1 pt collapsed font. `WysiwygTextView.swift:1010`

### A3. Syntax hiding and caret reveal

- [ ] Collapsed syntax is drawn at 0.1 pt in a clear color, so rendered text sits where it would without it; the file keeps the raw Markdown. `WysiwygEngine.swift:1475`
- [ ] Revealed syntax uses the `marker` token (in Glass: `text` at 26% alpha). `WysiwygEngine.swift:1484`, `CodeBlockLayoutManager.swift:31`
- [ ] Inline syntax (`**`, `*`, `~~`, `==`, `<u>`, `` ` ``, link brackets) reveals only when the caret is within or at the end of that span, not the whole line. `WysiwygEngine.swift:702`
- [ ] Block syntax (`#`, `>`, list prefix, fences, `:::`, rule) reveals when the caret is on that line. `WysiwygEngine.swift:391`
- [ ] The frontmatter opens as source when the caret is at offset > 0 and at or before the frontmatter's end. `WysiwygEngine.swift:364`
- [ ] A table or math span shows its source only while the caret is inside it or at its end. `WysiwygTextView.swift:969`, `WysiwygTextView.swift:976`
- [ ] An image or embed reveals its source while the caret touches it, with the image following on the next line. `WysiwygEngine.swift:561`, `WysiwygEngine.swift:647`
- [ ] Blank lines are one full body line wherever the caret is; they never collapse and nothing shifts as the caret moves. `WysiwygEngine.swift:476`

### A4. Body text

- [ ] Body: chosen family at the chosen size (default 16), `text` color, every line exactly `round(size x lineHeight)` tall (27 at the defaults), for CJK and Latin alike. `WysiwygEngine.swift:164`, `WysiwygEngine.swift:196`
- [ ] A paragraph's 1-3 leading spaces are hidden on every line including the caret's, so text does not shift; 4 or more are shown. `WysiwygEngine.swift:1006`
- [ ] Han/Latin boundaries get a display-only gap of `round(size x 0.22)` (4 pt at 16) via kerning; no spaces enter the file. Applies to Han ranges U+4E00-9FFF, U+3400-4DBF, U+F900-FAFF against ASCII digits and letters; skips code blocks, monospaced runs and collapsed syntax; resets at line ends. `WysiwygEngine.swift:1095`
- [ ] Composite fonts: System and SF Mono use the native cascade; a family covering Han is left alone. `CompositeFont.swift:94`, `CompositeFont.swift:105`
- [ ] Latin-only serif family pairs CJK with Songti SC, then STSong, then Songti TC; other Latin-only families pair with PingFang SC, then Hiragino Sans GB. `CompositeFont.swift:49`, `CompositeFont.swift:52`, `CompositeFont.swift:109`
- [ ] Serif detection is by family-name fragment: new york, times, georgia, charter, palatino, baskerville, didot, hoefler, iowan, garamond, caslon, minion, athelas, serif, song, ming, 宋, slab, literata, lora, merriweather, playfair, crimson, spectral, newsreader, gelasio, tinos, bitter, domine, alegreya, cormorant, vollkorn, arvo. `CompositeFont.swift:68`
- [ ] Paired CJK glyphs are set at 0.97 x the run's size, keep the run's weight and are never slanted. `CompositeFont.swift:61`, `CompositeFont.swift:215`
- [ ] CJK claim covers radicals, punctuation U+3000-303F, kana, bopomofo, hangul, ideographs (including ext B-I), compatibility, vertical and fullwidth forms. `CompositeFont.swift:133`
- [ ] General punctuation (U+2010-2027, 2030-205E, 2070-209F, 20A0-20CF) follows the previous character; opening quotes `‘ “` follow the next visible character instead, skipping collapsed syntax. `CompositeFont.swift:146`, `CompositeFont.swift:182`, `CompositeFont.swift:191`
- [ ] Composite re-setting skips collapsed, monospaced and attachment runs. `CompositeFont.swift:262`
- [ ] Font resolution: "System" or empty = system font; "SF Mono" = monospaced system; "New York" = system serif design; any other name = that family, falling back to system if missing. `WysiwygEngine.swift:118`

### A5. Headings

- [ ] Pattern: 0-3 leading spaces, 1-6 `#`, then spaces/tabs or line end. `WysiwygEngine.swift:1265`
- [ ] The first `#` heading before any other content is the title: `round(size x 1.875)` (30), semibold, 0 before, 10 after. `WysiwygEngine.swift:100`, `WysiwygEngine.swift:203`, `WysiwygEngine.swift:480`
- [ ] A later `#`: `round(size x 1.625)` (26), semibold, 24 before, 8 after. `WysiwygEngine.swift:205`
- [ ] `##`: `round(size x 1.3125)` (21), semibold, 16 before, 8 after. `WysiwygEngine.swift:206`
- [ ] `###`: `round(size x 1.125)` (18), semibold, 12 before, 4 after. `WysiwygEngine.swift:207`
- [ ] `####` and `#####`: body size, semibold, 8 before, 2 after. `WysiwygEngine.swift:208`
- [ ] `######`: body size, semibold, `text3` color, 8 before, 2 after. `WysiwygEngine.swift:210`
- [ ] Headings use `text` color (except `######`) and line height `max(round(size x 1.3), body line)`. `WysiwygEngine.swift:182`
- [ ] Heading marker with its trailing space is collapsed off the caret's line and in `marker` color on it. `WysiwygEngine.swift:487`
- [ ] Title status: blank lines and the frontmatter before a `#` do not count as content; anything else does, including an earlier heading, a code block, a callout, a quote or a paragraph. `WysiwygEngine.swift:415`, `WysiwygEngine.swift:445`, `WysiwygEngine.swift:476`, `WysiwygEngine.swift:494`

### A6. Inline formatting

- [ ] `**bold**`: chosen family at bold weight (not semibold), color = accent. `WysiwygEngine.swift:171`, `WysiwygEngine.swift:212`, `WysiwygEngine.swift:304`
- [ ] With the Ink accent (#37352F) bold uses the `strongInk` token instead. `CodeBlockLayoutManager.swift:18`
- [ ] `***bold italic***`: bold italic, accent color. `WysiwygEngine.swift:213`, `WysiwygEngine.swift:303`
- [ ] `*italic*`: italic trait; no space just inside the asterisks, not adjacent to another `*`. `WysiwygEngine.swift:214`, `WysiwygEngine.swift:305`
- [ ] `_underscore_` emphasis is not rendered in body text (only inside table cells). `WysiwygEngine.swift:303`, `MarkdownTable.swift:127`
- [ ] `~~strike~~`: `faintText` color, single strikethrough in `faintText`. `WysiwygEngine.swift:215`, `WysiwygEngine.swift:306`
- [ ] `==highlight==`: `highlight` token fill hugging the glyphs (ascender to descender, 1 pt vertical inset, no side padding, radius 3), not painting line spacing; no whitespace or `=` just inside. `WysiwygEngine.swift:231`, `WysiwygEngine.swift:307`, `CodeBlockLayoutManager.swift:147`, `CodeBlockLayoutManager.swift:397`
- [ ] `<u>underline</u>`: single underline in `text` color; `<u>` and `</u>` hide off the caret. `WysiwygEngine.swift:310`
- [ ] `` `inline code` ``: monospaced system at the small size (`max(12, round(size x 0.875))`, 14 at 16), color light #C4402F / dark #E5735F. `WysiwygEngine.swift:103`, `WysiwygEngine.swift:220`, `CodeBlockLayoutManager.swift:39`
- [ ] Inline code pill: `inlineCode` token fill, radius 4, 4 pt side padding, 2 pt vertical inset beyond ascender/descender, one pill per wrapped line; kerning of 4 added after the last character and on the character before it so it never touches neighbors. `WysiwygEngine.swift:716`, `WysiwygEngine.swift:1080`, `CodeBlockLayoutManager.swift:145`, `CodeBlockLayoutManager.swift:392`
- [ ] `#tag`: `#` then a letter then letters, digits, `_`, `/`, `-` (any script), only at line start or after whitespace, `(` or `[`. `WysiwygEngine.swift:326`
- [ ] Hashtag look: small size in the body family, `text2` color, the `#` itself in `faintText`, on a pill like inline code. `WysiwygEngine.swift:232`, `WysiwygEngine.swift:720`
- [ ] Inline rules never apply inside code blocks, unresolved image syntax or the collapsed frontmatter. `WysiwygEngine.swift:685`
- [ ] Bold inside a callout keeps the surrounding ink rather than the accent. `WysiwygEngine.swift:693`

### A7. Links

- [ ] `[text](url)`: `text` color, single underline in `linkUnderline`, brackets and destination collapsed off the caret's span. `WysiwygEngine.swift:224`, `WysiwygEngine.swift:290`
- [ ] A bare `http://` or `https://` address is underlined like a link, not preceded by a word character, `/`, `(` or `<`; stops at whitespace, `<>[]()（）"'“”，。、；：！？`; trailing `.,;:!?)` excluded. `WysiwygEngine.swift:1441`, `WysiwygEngine.swift:1446`, `WysiwygEngine.swift:1461`
- [ ] `[text](url)` is clickable only for http, https and mailto addresses. `WysiwygEngine.swift:1435`, `WysiwygEngine.swift:1464`
- [ ] `[[Note]]` to an existing note, with the shown text present in the source: brackets and everything but the alias/last path component collapse, text styled as a link. `WysiwygEngine.swift:1420`, `WysiwygEngine.swift:1425`
- [ ] `[[Missing]]` to a note that does not exist: `text3` color, dashed underline in `linkUnderline`, text as written. `WysiwygEngine.swift:1369`, `WysiwygEngine.swift:1455`
- [ ] `[[Note]]` whose shown text is not in the source (frontmatter title, or `Title › Heading`) is drawn as an inline image in the line's font and color: width `min(text + 1, column)`, height ascender-descender, baseline aligned, tail-truncated, 1 pt underline in `linkUnderline` at baseline + 3. `WysiwygEngine.swift:605`, `WikiLinkAttachment.swift:31`, `WikiLinkAttachment.swift:38`, `WikiLinkAttachment.swift:55`
- [ ] On the caret's span a `[[link]]` shows its brackets in `marker` color with inner text plain. `WysiwygEngine.swift:1415`
- [ ] `[[#Heading]]`-style links show no hashtag pill inside them. `WysiwygEngine.swift:1412`
- [ ] Links in code and the collapsed frontmatter are not styled or clickable. `WysiwygEngine.swift:1407`, `WysiwygEngine.swift:1442`
- [ ] Hovering a link or wiki link shows the pointing hand; leaving a link returns to the I-beam. `WysiwygTextView.swift:1471`
- [ ] Hovered text link: fill of `text` at 7%, radius 4, 3 pt side and 2 pt vertical overhang, one fill per wrapped line, underline darkens to `text` at 70%. `CodeBlockLayoutManager.swift:421`, `WysiwygTextView.swift:1498`
- [ ] Hovered attachment link: fill of `text` at 7%, radius 4, underline `text` at 70%. `WikiLinkAttachment.swift:46`, `WikiLinkAttachment.swift:53`
- [ ] Hit area for a link is the glyph box expanded 1 pt horizontally and 2 pt vertically. `WysiwygTextView.swift:1436`
- [ ] Hovering a web link shows its address at the page's lower left, as in a browser: 11 pt system, `text2`, `inlineCode` fill, radius 5, height 20, 12 pt from left and bottom, middle-truncated, at most page width - 24; gone when the pointer leaves. `WysiwygTextView.swift:1507`, `WysiwygTextView.swift:1519`
- [ ] A restyle clears any link hover. `WysiwygTextView.swift:935`

### A8. Code blocks

- [ ] A block needs a closing fence: same character (`` ` `` or `~`), at least as long as the opener, no info string, optionally indented, opener at least 3 characters; a lone or unmatched fence renders as an ordinary line and the rest of the note stays normal. `WysiwygEngine.swift:1297`, `WysiwygEngine.swift:1321`
- [ ] Text: monospaced system at the small size (14), `text` color, indent 16 left and right, line spacing 6. `WysiwygEngine.swift:93`, `WysiwygEngine.swift:236`
- [ ] Box: one rounded rect in `codeBlock` token across the full column width, radius 8, 10 pt above and below the text. `CodeBlockLayoutManager.swift:136`, `CodeBlockLayoutManager.swift:197`, `CodeBlockLayoutManager.swift:276`
- [ ] The info string's first word shows top-right in the box: 12 pt system, `text3`, 12 from the right, 7 from the top; none when empty. `CodeBlockLayoutManager.swift:213`, `WysiwygEngine.swift:411`
- [ ] Fence lines collapse off the caret and show in `marker` color on the caret's line. `WysiwygEngine.swift:427`
- [ ] Syntax colors: keywords light #9065B0 / dark #B794D6, strings #448361 / #6FB08A, numbers #9A6B12 / #D1A04A, comments `faintText`. `WysiwygEngine.swift:1156`, `CodeBlockLayoutManager.swift:46`
- [ ] Keyword list applies to every language: func let var if else for while return guard import class struct enum protocol extension def function const true false nil null None True False self this in case switch break continue try catch throw throws async await public private static new from as interface type extends implements package go fn pub mut impl use where yield lambda with and or not elif end do then fi echo export select insert update delete create table into values. `WysiwygEngine.swift:1148`
- [ ] Numbers `\b\d+(\.\d+)?\b`; strings single- or double-quoted on one line, with backslash escapes. `WysiwygEngine.swift:1149`, `WysiwygEngine.swift:1150`
- [ ] Comment syntax: `#` to end of line for python, py, sh, bash, zsh, shell, yaml, yml, ruby, rb, toml, r, perl, makefile, dockerfile (case-insensitive); `//` and `/* */` for all others. Paint order is keywords, numbers, strings, comments, last wins. `WysiwygEngine.swift:1151`, `WysiwygEngine.swift:1153`, `WysiwygEngine.swift:1164`
- [ ] Code blocks and callouts get 14 pt extra spacing on the neighboring lines above and below. `WysiwygEngine.swift:1550`

### A9. Blockquotes

- [ ] Consecutive `>` lines form a group; depth is the count of `>` in the prefix. `WysiwygEngine.swift:913`, `WysiwygEngine.swift:966`
- [ ] Text indent 17 x depth, body line height. `WysiwygEngine.swift:89`, `WysiwygEngine.swift:970`
- [ ] Bars: 3 pt wide at x = 17 x (level - 1), from 2 pt below line top to the text bottom; level 1 in `text`, deeper levels in `linkUnderline`. `CodeBlockLayoutManager.swift:303`
- [ ] A line starting `— `, `——` or `-- ` is an attribution: small size in the body family, `text3`. `WysiwygEngine.swift:985`
- [ ] On the caret's line the raw `> ` shows in `marker` at body font, hung before the text so the text does not move. `WysiwygEngine.swift:974`, `WysiwygEngine.swift:991`

### A10. Callouts

- [ ] `::: type title` ... `:::` fence form: opening line's `::: type` hidden when a title follows; with no title the type name itself is shown as the title text; closing line collapses, showing in `marker` on the caret. `WysiwygEngine.swift:442`, `WysiwygEngine.swift:457`, `WysiwygEngine.swift:1493`
- [ ] `> [!type] Title` form (Obsidian/GitHub): type word up to `]`, optional `+`/`-` fold sign ignored; subsequent `>` lines are the body. `WysiwygEngine.swift:498`, `WysiwygEngine.swift:938`, `WysiwygEngine.swift:942`
- [ ] With no title, the `[!type]` form draws the type capitalized (`note` to "Note", `IMPORTANT` to "Important") over the hidden word. `WysiwygEngine.swift:1541`, `Callout.swift:34`, `CodeBlockLayoutManager.swift:257`
- [ ] A callout's header syntax on the caret's line shows in the type color at 40% alpha. `WysiwygEngine.swift:1509`, `WysiwygEngine.swift:1533`
- [ ] A quote whose first character is pictographic renders as a gray box with no icon, text indent 14, no quote bar. `WysiwygEngine.swift:515`, `WysiwygEngine.swift:951`, `Callout.swift:19`
- [ ] Digits, `#` and `*` do not count as an emoji opener. `WysiwygEngine.swift:951`
- [ ] Callout text: indent 40, right inset 14, body line height; title small size system semibold; body in the body font. `WysiwygEngine.swift:92`, `WysiwygEngine.swift:245`, `WysiwygEngine.swift:899`
- [ ] Box: tinted rounded rect across the full column, radius 8, 10 pt above and below. `CodeBlockLayoutManager.swift:205`
- [ ] Icon: SF Symbol 13 pt semibold in the type color, centered in a 16 pt slot 14 from the box's left, vertically centered on the title's cap height. `CodeBlockLayoutManager.swift:231`, `CodeBlockLayoutManager.swift:250`
- [ ] Type mapping, blue `note`: note, info, abstract, summary, tldr, todo. `Callout.swift:23`
- [ ] Type mapping, green `tip`: tip, hint, success, check, done. `Callout.swift:24`
- [ ] Type mapping, purple `important`: important. `Callout.swift:25`
- [ ] Type mapping, amber `warning`: warning, attention, question, help, faq. `Callout.swift:26`
- [ ] Type mapping, red `danger`: caution, danger, error, failure, fail, missing, bug. `Callout.swift:27`
- [ ] Any other type is gray `plain`, titled with its own name. `Callout.swift:28`
- [ ] Title and icon colors (light/dark): note #2A6FB8/#6FA8E6, tip #448361/#6FB08A, important #9065B0/#B48FD6, warning #9A6B12/#D1A04A, danger #C4402F/#E5735F, plain #55534E/#C3C0B9. `Callout.swift:41`
- [ ] Box fills (light/dark): note #E8F1FB/#1E2A38, tip #E7F5EC/#1E2E25, important #F3EEF8/#2B2436, warning #FBF3DB/#3A3220, danger #FDECEE/#3A2224, plain #F1F1EF/#2E2D2A. `Callout.swift:57`
- [ ] Glass theme box fills: the type color at 15% (plain at 10%). `Callout.swift:53`
- [ ] Icons: note info.circle, tip lightbulb, important exclamationmark.bubble, warning exclamationmark.triangle, danger xmark.octagon, plain quote.opening. `Callout.swift:79`
- [ ] Icon overrides: success/check/done checkmark.circle, question/help/faq questionmark.circle, todo checklist, bug ladybug. `Callout.swift:69`

### A11. Lists

- [ ] Item pattern: indentation, then `-`, `*`, `+`, `1.` or `1)`, then spaces/tabs, then optional `[ ]`, `[x]`, `[X]`. `WysiwygEngine.swift:1002`
- [ ] Nesting level = `min(columns / 2, 6)` where a tab counts 4 columns and a space 1. `WysiwygEngine.swift:1019`
- [ ] Text indent 24 x (level + 1); body line height, 4 pt after each item. `CodeBlockLayoutManager.swift:119`, `WysiwygEngine.swift:1021`, `WysiwygEngine.swift:1037`
- [ ] Bullets, 7 pt into the 24 pt slot, `text` color, vertically centered on cap height, cycling by `level % 3`: filled disc d = `max(5, round(size x 0.33))`; ring of the same diameter with 1.2 pt stroke; filled square s = `max(4, round(size x 0.28))`. `CodeBlockLayoutManager.swift:326`
- [ ] Numbers: monospaced-digit system at body size, `text`, right-aligned 6 pt before the text, keeping the number typed. `CodeBlockLayoutManager.swift:343`
- [ ] Numbers on odd nesting levels draw as letters: 1 = `a.`, 26 = `z.`, 27 = `aa.`. `WysiwygEngine.swift:1030`, `WysiwygEngine.swift:1066`
- [ ] To-do box: 16 x 16, 1 pt from the slot's left, centered on cap height, radius 4. `CodeBlockLayoutManager.swift:123`
- [ ] Unchecked: 1.5 pt stroke in `faintIcon`, no fill. `CodeBlockLayoutManager.swift:370`, `CodeBlockLayoutManager.swift:43`
- [ ] Checked: accent fill with a white 2 pt round check mark. `CodeBlockLayoutManager.swift:357`
- [ ] Checked item text: `faintText` with strikethrough. `WysiwygEngine.swift:1058`
- [ ] On the caret's line the raw prefix replaces the marker, in `marker` at the body font, hung in the slot so the text stays put; leading indentation stays collapsed. `WysiwygEngine.swift:1039`
- [ ] An empty item stays one body line tall. `WysiwygEngine.swift:1035`

### A12. Horizontal rule

- [ ] A line that, spaces removed, is exactly `---`, `***` or `___` renders as a 1 pt line in `rule` across the column, vertically centered; `----` does not. `WysiwygEngine.swift:1273`, `CodeBlockLayoutManager.swift:430`
- [ ] Body line height with 4 pt before and after. `WysiwygEngine.swift:535`
- [ ] On the caret's line the raw characters show in `marker` and no line is drawn. `WysiwygEngine.swift:529`

### A13. Images

- [ ] `![alt](url)` and `![[file]]` render as block images; destination parsing as in fumi-core's `InlineLink`. `WysiwygEngine.swift:272`, `WysiwygEngine.swift:86`
- [ ] Display width `min(natural, column)`, keeping aspect; decode is downsampled so the longest side is at most 1200 px, and that pixel size is the point size. `ImageAttachment.swift:160`, `ImageAttachment.swift:364`, `ImageAttachment.swift:376`
- [ ] Static images are clipped to radius 8. `ImageAttachment.swift:36`, `ImageAttachment.swift:72`
- [ ] A nonblank alt text is a caption under the image: 12 pt system, `text3`, centered, tail-truncated, 26 pt reserved, text 7 pt below the image. `ImageAttachment.swift:37`, `ImageAttachment.swift:75`
- [ ] Spacing: line spacing 4, 8 before, 8 after. `WysiwygEngine.swift:645`
- [ ] With the caret touching: raw source in `faintText` on its own line, image next on a line reserved at full column width; line spacing 10, 8 before, 8 after. `WysiwygEngine.swift:632`, `WysiwygEngine.swift:649`, `ImageAttachment.swift:58`, `ImageAttachment.swift:67`
- [ ] Unresolved `![alt](url)`: alt in `text3`, the rest in `faintText`, kept visible, not collapsed. `WysiwygEngine.swift:286`, `WysiwygEngine.swift:674`
- [ ] Animated images (more than one frame) play on a loop using each frame's GIF delay; delays under 0.02 s or missing are 0.1 s. They redraw only when on screen, have no rounded clip and no caption. `ImageAttachment.swift:54`, `ImageAttachment.swift:144`, `ImageAttachment.swift:404`, `WysiwygTextView.swift:1039`
- [ ] Embed `![[...]]`: anything after `|` or `#` is dropped, so `![[a.png|300]]` shows at normal width; no caption. `ImageAttachment.swift:220`, `WysiwygEngine.swift:598`
- [ ] Embed resolution: relative to note or workspace first; then by case-insensitive file name anywhere in the library, ignoring hidden files, packages and `.md`, shallowest match wins. `ImageAttachment.swift:218`, `ImageAttachment.swift:231`
- [ ] Local path resolution order: `file://` URL; absolute; `~`; relative to the note's folder; relative to the workspace root. Surrounding `<...>` stripped and percent-decoded first. `ImageAttachment.swift:421`, `ImageAttachment.swift:451`
- [ ] Remote images load asynchronously and appear when ready without moving the reader; bursts coalesce to one restyle per runloop turn. `WysiwygEngine.swift:580`, `WysiwygTextView.swift:922`
- [ ] Remote request sends a Safari User-Agent and an image Accept header; hosts containing feishu, larksuite or larkoffice also get `Referer: https://www.feishu.cn/`. `ImageAttachment.swift:301`, `ImageAttachment.swift:306`
- [ ] Remote URL cleanup: `&amp;` to `&`, non-breaking spaces and all whitespace removed, unencodable URLs percent-encoded. `ImageAttachment.swift:429`, `ImageAttachment.swift:443`
- [ ] Remote failure retry: timeout, connection-lost and DNS errors after 15 s; HTTP 408/429/5xx after 30 s; anything else after 300 s; raw syntax shows meanwhile. `ImageAttachment.swift:320`, `ImageAttachment.swift:331`, `ImageAttachment.swift:490`
- [ ] Remote images persist in a disk cache (`FumiImageCache`, 32 MB memory, 512 MB disk, cache preferred), 30 s between bytes and 120 s total. `ImageAttachment.swift:184`, `ImageAttachment.swift:191`

### A14. Video cards

- [ ] Local `.mp4`, `.mov`, `.m4v`, `.webm` (case-insensitive) in `![](...)` or `![[...]]` render as a card; remote video URLs do not. `VideoAttachment.swift:6`, `WysiwygEngine.swift:572`, `WysiwygEngine.swift:594`
- [ ] Card width = column; height = column x aspect, aspect from the poster capped at 1.25, or 9/16 before one exists; 26 pt caption below. `VideoAttachment.swift:67`, `VideoAttachment.swift:74`
- [ ] Poster area: radius 8, background white-level 0.12, poster aspect-fit, black 12% overlay. `VideoAttachment.swift:81`
- [ ] Play button: centered 52 pt circle of black 45% with a white triangle. `VideoAttachment.swift:93`
- [ ] Duration pill at the poster's bottom-left, 10 pt inset: 12 pt medium monospaced-digit white on black 60%, radius 4, `m:ss` or `h:mm:ss` over an hour, hidden at 0. `VideoAttachment.swift:106`, `VideoAttachment.swift:143`
- [ ] File name caption: 12 pt system, `text3`, centered, middle-truncated, 7 pt under the poster. `VideoAttachment.swift:121`
- [ ] Poster frame is taken at `min(0.5 s, duration / 2)`, at most 1400 pt, with the track's orientation applied, and arrives asynchronously. `VideoAttachment.swift:34`, `VideoAttachment.swift:37`

### A15. Tables

- [ ] A table is a row containing `|` followed directly by a delimiter row, then zero or more rows containing `|`; fences suspend detection. `MarkdownTable.swift:30`, `MarkdownTable.swift:82`
- [ ] Rendered as a grid image: 1 pt `tableBorder` lines, rounded outline radius 8, cell padding 12 horizontal, 8 vertical. `MarkdownTable.swift:243`, `MarkdownTable.swift:308`
- [ ] Cell font: system at the small size (14); header semibold, `text2`, on a `codeBlock` fill; body `text`. `WysiwygEngine.swift:1225`, `MarkdownTable.swift:241`, `MarkdownTable.swift:249`, `MarkdownTable.swift:274`, `MarkdownTable.swift:314`
- [ ] Alignment from delimiter (`:-:` center, `-:` right, otherwise left). `MarkdownTable.swift:98`
- [ ] A left-aligned column whose body cells are all numeric (ignoring commas, `%$¥€+-`) right-aligns with monospaced digits; at least one body row needed. `MarkdownTable.swift:256`, `MarkdownTable.swift:263`
- [ ] Short rows are padded, long rows truncated to the header's column count. `MarkdownTable.swift:107`
- [ ] Column widths: natural when all fits; otherwise each keeps its longest unbreakable run (at least 30) and the rest is shared by how much more each wants; if even minimums do not fit they scale down, floor 30, and words may break. Natural floor 8. `MarkdownTable.swift:223`, `MarkdownTable.swift:284`
- [ ] Han, kana, hangul and CJK punctuation count as breakable when measuring the longest run. `MarkdownTable.swift:167`
- [ ] Cells wrap at word boundaries, and row height follows the tallest cell. `MarkdownTable.swift:136`, `MarkdownTable.swift:303`
- [ ] Cell inline: `**bold**` and `__bold__` in the bold color, `*italic*` and `_italic_`, `~~strike~~` (no color change), `` `code` `` in monospaced and #C4402F/#E5735F on an `inlineCode` pill (radius 4, 4 pt side, 1 pt vertical inset), `[links](...)` underlined in `linkUnderline`; markers removed. Text inside code is not Markdown. `MarkdownTable.swift:127`, `MarkdownTable.swift:143`, `MarkdownTable.swift:376`
- [ ] Spacing: 8 before, 10 after. `WysiwygEngine.swift:1239`
- [ ] With the caret inside: raw source in the small monospaced font so pipes line up. `WysiwygEngine.swift:1230`

### A16. Math

- [ ] `$$...$$` display math (may span lines) and `$...$` inline math; not in fenced code or tables; `\$` escapes. `LatexMath.swift:19`, `LatexMath.swift:36`
- [ ] Inline math: no whitespace just inside the delimiters, no `$` or newline inside, last character not a backslash. `LatexMath.swift:47`
- [ ] Inline formulas sit on the text baseline; 2 pt padding around; drawn in `text` at body size in the system font. `LatexMath.swift:242`, `LatexMath.swift:381`, `WysiwygEngine.swift:1183`
- [ ] Display formulas: size body + 2, centered, 10 before and 10 after. `LatexMath.swift:238`, `WysiwygEngine.swift:1196`
- [ ] Superscript/subscript at 70% size (min 7), `^` and `_` with a braced group or single base. `LatexMath.swift:115`, `LatexMath.swift:292`
- [ ] `\frac{a}{b}`: centered numerator and denominator over a bar `max(1, size x 0.06)` thick, 3 pt gaps, 6 pt wider than its content. `LatexMath.swift:325`
- [ ] `\sqrt{x}`: drawn radical with overline; optional `[n]` index is parsed and dropped. `LatexMath.swift:166`, `LatexMath.swift:345`
- [ ] Single ASCII letters render italic; numbers, operators and multi-character tokens upright. `LatexMath.swift:253`
- [ ] Symbol table covers Greek, operators, relations, arrows, sets, logic and delimiters; `\mathbb`, `\mathcal`, `\mathrm`, `\leftroot` render as nothing. `LatexMath.swift:188`
- [ ] Unknown commands render as their literal name; `\left` and `\right` draw their delimiter, `.` drawing nothing. `LatexMath.swift:171`, `LatexMath.swift:180`
- [ ] Spacing: `\,` `\;` `\ ` `\!` a space; `\\` a space; `\quad` `\qquad` two spaces; `\{ \} \$ \%` literal. `LatexMath.swift:149`, `LatexMath.swift:177`
- [ ] With the caret inside: raw source in the small monospaced font. `WysiwygEngine.swift:1188`

### A17. Frontmatter panel

- [ ] Frontmatter collapses to a properties panel at the note's top: 26 pt row per field, 96 pt key column, 13 pt system. `FrontmatterAttachment.swift:9`, `FrontmatterAttachment.swift:35`
- [ ] Keys in `text3`, tail-truncated at 88 pt; text values in `text`, tail-truncated. `FrontmatterAttachment.swift:38`, `FrontmatterAttachment.swift:48`
- [ ] List values draw as pills: height 20, radius 4, text + 12 wide, 4 pt gap, in five cycling colors; a pill that would overflow stops the row. `FrontmatterAttachment.swift:61`
- [ ] Pill colors bg/fg light: #FBF0D9/#7A5410, #E8F1FB/#1F4E80, #FDECEE/#8A2F36, #E7F5EC/#2F6B4A, #F3EEF8/#6B4A8A. Dark: #3A3220/#E8C77A, #1E2A38/#9CC4EE, #3A2224/#EBA0A6, #1E2E25/#8FCBA6, #2E2638/#C6A9E0. `FrontmatterAttachment.swift:24`
- [ ] A 1 pt `rule` line runs across the panel's bottom with 10 pt padding above it and 16 pt after it. `FrontmatterAttachment.swift:11`, `FrontmatterAttachment.swift:76`, `WysiwygEngine.swift:762`
- [ ] A frontmatter with no fields shows one row reading "Properties" in `text3`. `FrontmatterAttachment.swift:42`
- [ ] Opened as source: small monospaced, `text3`, line spacing 4, 16 pt after the last line. `WysiwygEngine.swift:398`

### A18. Draft placeholder

- [ ] An untouched draft shows faint "Title" in `faintIcon` at the title font, right past the hidden `# `. `WysiwygTextView.swift:743`, `WysiwygTextView.swift:752`, `EditorView.swift:100`
- [ ] Under it, 14 pt below the title line, in `faintIcon` at body font: "Start typing — the note is saved once you write something." `WysiwygTextView.swift:759`, `EditorView.swift:100`
- [ ] The placeholder disappears with the first keystroke; it shows only while the text equals the draft's empty heading. `WysiwygTextView.swift:743`

## B. Editor behavior

### B1. Keyboard shortcuts in the editor

- [ ] ⌘B wraps in `**`: with a selection, the wrapped text including markers stays selected; with none, inserts `****` with the caret in the middle. It always adds, never unwraps. `FumiApp.swift:218`, `WysiwygTextView.swift:1743`, `WysiwygTextView.swift:1751`
- [ ] ⌘I wraps in `*`, same rules. `FumiApp.swift:220`, `WysiwygTextView.swift:1747`
- [ ] ⌘K wraps in `[` and `](url)`, same rules: the selection then covers the whole link. `FumiApp.swift:222`, `WysiwygTextView.swift:1766`
- [ ] ⌘L flips the first `[ ]`, `[x]` or `[X]` on the caret's line between `[ ]` and `[x]`; a line without one is unchanged, nothing is created. `FumiApp.swift:225`, `WysiwygTextView.swift:1770`
- [ ] Format commands act on the focused window's text view only, once. `FumiApp.swift:215`
- [ ] Undo and redo work across typing, with `allowsUndo`. `WysiwygTextView.swift:410`
- [ ] Arrow keys, Home, End, Page Up and Page Down without Shift re-render so the syntax of the line landed on is revealed the same way a click reveals it. `WysiwygTextView.swift:1171`
- [ ] Moving with Shift or making a selection does not re-render and does not collapse the selection. `WysiwygTextView.swift:1174`, `WysiwygTextView.swift:1176`
- [ ] Right arrow from just before a revealed image, or Down arrow landing on either edge of one, jumps past it and its line break to the next line. `WysiwygTextView.swift:1189`
- [ ] Moving up to document offset 0 when the note has a collapsed frontmatter panel opens the panel for editing. `WysiwygTextView.swift:1209`

### B2. Enter, Tab, Backspace, auto-pairing

- [ ] Return, Tab and Backspace outside the slash menu are plain text-view behavior: keys go straight to `NSTextView`, with no list or quote continuation, no list indent or outdent, no marker removal. `WysiwygTextView.swift:1167`, `WysiwygTextView.swift:1170`
- [ ] Typed quotes, brackets and backticks are inserted as typed: no auto-pairing or wrap-selection on type. `WysiwygTextView.swift:1170`
- [ ] Return adds one full body line every time, on any line including blank ones. `WysiwygEngine.swift:473`
- [ ] Smart quotes and dashes follow Settings (`fumi.smartPunctuation`, default off); spell check follows `fumi.spellCheck`, default off. `WysiwygTextView.swift:149`
- [ ] Input methods compose normally; a merge from outside is refused while text is being composed. `WysiwygTextView.swift:1086`

### B3. Paste

- [ ] Paste checks for media first: file URLs that are images or videos are imported, each on its own line. `WysiwygTextView.swift:624`, `WysiwygTextView.swift:679`
- [ ] With no file URLs, image data on the pasteboard is saved, unless plain text is also there (Word and Pages put a picture of copied text). `WysiwygTextView.swift:661`
- [ ] Pasted image format preference: PNG, JPEG, GIF as supplied; TIFF converts to PNG. `WysiwygTextView.swift:663`, `WysiwygTextView.swift:670`
- [ ] Saved as `attachments/Pasted image yyyyMMddHHmmss.ext` in the library, with `attachments/` created as needed. `AttachmentImport.swift:27`, `AttachmentImport.swift:30`, `AttachmentImport.swift:92`
- [ ] A file already inside the library is linked in place, not copied; one from outside is copied to `attachments/`. `AttachmentImport.swift:17`
- [ ] Name clashes become `name 2.ext`, `name 3.ext`. `AttachmentImport.swift:36`
- [ ] Links are `![](path)` relative to the note's folder with `../` as needed; only `%`, space, `(`, `)` are encoded, so Chinese names stay readable. `AttachmentImport.swift:50`, `AttachmentImport.swift:61`
- [ ] Several files become one embed per line, with a leading newline if the insert point is not at line start and a trailing newline if text follows. `WysiwygTextView.swift:693`, `WysiwygTextView.swift:700`
- [ ] Non-media paste (other files, including PDFs, and text) falls to the system's paste with no HTML-to-Markdown conversion. `WysiwygTextView.swift:626`, `AttachmentImport.swift:9`
- [ ] Rich text pasted in keeps only its characters: the next restyle resets all styling from the Markdown. `WysiwygTextView.swift:409`, `WysiwygEngine.swift:354`
- [ ] With no library open, media handling is skipped and paste is the system's. `WysiwygTextView.swift:676`

### B4. Drag and drop into the editor

- [ ] Dragging image or video files over the page shows the copy cursor. `WysiwygTextView.swift:633`, `WysiwygTextView.swift:638`
- [ ] Dropping inserts at the character nearest the pointer, using the same import and link rules as paste, and focuses the editor. `WysiwygTextView.swift:643`, `WysiwygTextView.swift:702`
- [ ] Other drags use the system's behavior. `WysiwygTextView.swift:649`

### B5. Slash menu: when it opens and closes

- [ ] Typing `/`, or `、` (the same key under a Chinese input method), opens it at line start or after a space, tab or U+3000. `SlashCommand.swift:133`, `SlashCommand.swift:143`
- [ ] A lone slash replacing a selection does not open it; a slash committed by an input method does. `WysiwygTextView.swift:863`
- [ ] It does not open with Settings "Slash commands" off (`fumi.slashCommands`). `WysiwygTextView.swift:1617`
- [ ] It does not open on a line starting `|`, after an odd number of backticks on the line, or inside a fenced block, a `$$` block or the frontmatter. `SlashCommand.swift:149`, `SlashCommand.swift:151`, `SlashCommand.swift:157`
- [ ] Text between slash and caret filters it; composing pinyin has separators and spaces stripped first. `WysiwygTextView.swift:1637`, `WysiwygTextView.swift:1638`
- [ ] It closes when: the caret leaves the query, a selection is made, the slash is gone, whitespace is typed, the query exceeds 32 characters, or 3 characters are typed past the last match. `WysiwygTextView.swift:1632`, `WysiwygTextView.swift:1642`, `SlashMenu.swift:141`
- [ ] It closes on any click in the text, losing focus and leaving the window. `WysiwygTextView.swift:1219`, `WysiwygTextView.swift:719`, `WysiwygTextView.swift:731`
- [ ] It closes when an outside change replaces the text. `WysiwygTextView.swift:1087`

### B6. Slash menu: keys and pointer

- [ ] ↑ and ⌃P move up, ↓ and ⌃N move down, clamped without wrap. `WysiwygTextView.swift:1655`, `WysiwygTextView.swift:1675`, `SlashMenu.swift:155`
- [ ] ↵, keypad Enter and ⇥ insert the selected item; with no results they close the menu and pass the key on. `WysiwygTextView.swift:1661`
- [ ] Esc closes. `WysiwygTextView.swift:1668`
- [ ] Other keys type as usual and filter; menu keys are not intercepted during composition. `WysiwygTextView.swift:1168`
- [ ] Hovering a row selects it; clicking inserts it; the selected row scrolls into view. `SlashMenu.swift:225`, `SlashMenu.swift:226`, `SlashMenu.swift:233`
- [ ] Typing resets selection to the first result; clearing the query back to empty keeps the current item if present. `SlashMenu.swift:148`

### B7. Slash menu: filtering

- [ ] Ranking: title starts with query (3000), keyword typed in full (2000), fuzzy title match (1000 + score), keyword starts with query (500); ties keep menu order; apostrophes stripped from the query. `SlashCommand.swift:109`
- [ ] Empty query lists all in menu order with group labels; a filtered list has no labels. `SlashCommand.swift:111`, `SlashMenu.swift:20`
- [ ] Matched title characters are semibold. `SlashMenu.swift:298`

### B8. Slash menu: items, in order

- [ ] Basic / Text: tile serif `Aa`, no hint; keywords p plain paragraph 正文 zw; removes the line's block prefix. `SlashCommand.swift:44`
- [ ] Basic / Heading 1: tile `H1`, hint `#`; keywords h1 title 标题 bt; prefix `# `. `SlashCommand.swift:46`
- [ ] Basic / Heading 2: tile `H2`, hint `##`; keywords h2 标题 bt; prefix `## `. `SlashCommand.swift:48`
- [ ] Basic / Heading 3: tile `H3`, hint `###`; keywords h3 标题 bt; prefix `### `. `SlashCommand.swift:50`
- [ ] Basic / Bulleted List: symbol list.bullet, hint `-`; keywords ul bullet 列表 lb; prefix `- `. `SlashCommand.swift:52`
- [ ] Basic / Numbered List: tile `1.`, hint `1.`; keywords ol number 编号 bh; prefix `1. `. `SlashCommand.swift:54`
- [ ] Basic / To-do: symbol checkmark.square, hint `[ ]`; keywords todo task check 待办 db; prefix `- [ ] `. `SlashCommand.swift:56`
- [ ] Basic / Quote: symbol text.quote, hint `>`; keywords blockquote 引用 yy; prefix `> `. `SlashCommand.swift:58`
- [ ] Blocks / Callout: symbol info.square, hint `[!]`; keywords note tip warning 提示 ts; prefix `> [!note] `. `SlashCommand.swift:61`
- [ ] Blocks / Code Block: symbol chevron.left.forwardslash.chevron.right, hint ```` ``` ````; keywords code fence 代码 dm; inserts a fence pair with the caret on the empty middle line. `SlashCommand.swift:63`
- [ ] Blocks / Math Block: tile serif `∑`, hint `$$`; keywords latex formula 公式 gs; inserts a `$$` pair with the caret on the empty middle line. `SlashCommand.swift:67`
- [ ] Blocks / Table: symbol tablecells, hint `|`; keywords grid 表格 bg; inserts `| Column 1 | Column 2 |`, `| --- | --- |`, `|  |  |` with "Column 1" selected. `SlashCommand.swift:70`
- [ ] Blocks / Divider: symbol minus, hint `---`; keywords hr line rule 分割线 fgx; inserts `---` and a newline with the caret after. `SlashCommand.swift:74`
- [ ] Insert / Image…: symbol photo, no hint; keywords photo picture video 图片 tp; removes slash and query, then opens a sheet choosing several images or movies, button "Insert", imported as on a drop at the caret. `SlashCommand.swift:78`, `WysiwygTextView.swift:1726`
- [ ] Insert / Link: symbol link, hint `⌘K`; keywords url 链接 lj; inserts `[](url)` with the caret inside `[]`. `SlashCommand.swift:80`
- [ ] Format / Bold: symbol bold, hint `⌘B`; keywords b strong 粗体 ct; `**|**`. `SlashCommand.swift:83`
- [ ] Format / Italic: symbol italic, hint `⌘I`; keywords i em 斜体 xt; `*|*`. `SlashCommand.swift:85`
- [ ] Format / Strikethrough: symbol strikethrough, hint `~~`; keywords del strike 删除线 scx; `~~|~~`. `SlashCommand.swift:87`
- [ ] Format / Highlight: symbol highlighter, hint `==`; keywords mark 高亮 gl; `==|==`. `SlashCommand.swift:89`
- [ ] Format / Inline Code: tile `` ` ``, hint `` ` ``; keywords code 行内代码 hndm; `` `|` ``. `SlashCommand.swift:91`

### B9. Slash menu: applying an item

- [ ] Line items replace the line's existing marker (`#`, `- [ ]`, `-`, `1.`, `> [!x]`, `>`) keeping its indent and text, caret keeping its place in the content. `SlashCommand.swift:201`, `SlashCommand.swift:230`
- [ ] Block items on an otherwise empty line replace it; on a line with text they go below it after a blank line, so `---` under a paragraph does not make a heading. `SlashCommand.swift:209`
- [ ] Inline items wrap at the slash, removing slash and query. `SlashCommand.swift:219`
- [ ] On a line holding a rendered attachment, only the slash and query are replaced. `WysiwygTextView.swift:1704`
- [ ] After inserting, the view restyles (code or a table opens as source), applies the selection and scrolls it into view. `WysiwygTextView.swift:1714`

### B10. Slash menu: appearance

- [ ] Borderless panel, 248 wide, radius 10, padding 5, shadow; does not take focus, moves with the window, hides when the app deactivates. `SlashMenu.swift:26`, `SlashMenu.swift:49`, `SlashMenu.swift:55`, `SlashMenu.swift:62`, `SlashMenu.swift:205`
- [ ] Placed under the slash with a 6 pt gap, tile aligned to the slash (x = slash - 9); flips above when there is no room below; kept within the page inset 8; follows scrolling. `SlashMenu.swift:70`, `SlashMenu.swift:73`, `SlashMenu.swift:126`, `SlashMenu.swift:179`
- [ ] Rows 30 tall; group labels 24 tall, 11 pt medium, `text3`; nine rows then scroll (list max 294), scrollbar only when capped. `SlashMenu.swift:22`, `SlashMenu.swift:28`, `SlashMenu.swift:210`, `SlashMenu.swift:215`
- [ ] Row: 22 x 22 tile radius 5, title 13 pt `text`, spacing 9, padding 8, hint 11 pt monospaced `text3`. `SlashMenu.swift:270`, `SlashMenu.swift:276`, `SlashMenu.swift:307`, `SlashMenu.swift:323`
- [ ] Tile contents: letters 9.5 semibold; serif letters 11 medium; symbols 10.5 medium; `text2` color, `page` fill (60% in Glass), border `controlBorder`, or accent 35% when selected. `SlashMenu.swift:315`, `SlashMenu.swift:317`, `SlashMenu.swift:319`, `SlashMenu.swift:324`, `SlashMenu.swift:325`
- [ ] Selected row: radius 6, accent 12% in light and 24% in dark. `SlashMenu.swift:284`
- [ ] Footer, 29 tall under a 1 pt `border`: "↑↓ Move", "↵ Insert", "esc Close", 11 pt `text3`, spacing 12. `SlashMenu.swift:24`, `SlashMenu.swift:240`, `SlashMenu.swift:248`
- [ ] No match: "No results", 13 pt `text3`, one row, no footer. `SlashMenu.swift:192`, `SlashMenu.swift:38`
- [ ] Background: `page` with a 1 pt line of primary at 8% (12% dark); on Glass in macOS 26, regular glass. `SlashMenu.swift:255`, `SlashMenu.swift:258`

### B11. Link clicks

- [ ] A single unmodified click on a link opens it, unless the caret is on that link's line and the editor has focus: then it places the caret to edit. ⌘ always opens. `WysiwygTextView.swift:1220`, `WysiwygTextView.swift:1535`
- [ ] Dragging more than 3 pt from the press selects instead of opening. `WysiwygTextView.swift:1546`
- [ ] Shift-click and double-click are normal text actions on a link. `WysiwygTextView.swift:1220`
- [ ] A web link opens in the default browser. `WysiwygTextView.swift:1553`
- [ ] A wiki link to an existing note opens it: in this tab as a Back-able step, or in a pinned tab opens beside it. `WysiwygTextView.swift:1559`, `AppModel.swift:222`
- [ ] ⌘-click opens the note in a new tab behind this one; nothing happens if a tab already shows it. `AppModel.swift:224`, `WysiwygTextView.swift:1548`
- [ ] Links with `#heading` open at that heading, matched case-insensitively, 0.35 s after opening, in this tab only. `EditorView.swift:181`, `EditorView.swift:191`
- [ ] A missing note: click shows a popover under the link, "No note named “X” yet." (`X` = last path component), Cancel (esc) and Create Note (↵), 280 wide, padding 14, spacing 10, `text2` 13 pt, closes on outside click. `WysiwygTextView.swift:1570`, `WysiwygTextView.swift:1789`
- [ ] Create Note creates it beside the current note and opens it. `WysiwygTextView.swift:1583`, `EditorView.swift:197`
- [ ] Linked notes renamed or retitled refresh shown link text in open notes. `WysiwygTextView.swift:448`, `WysiwygTextView.swift:1035`

### B12. Task checkboxes

- [ ] Clicking in the checkbox rect expanded 4 pt flips `[ ]` and `[x]` on that line, without moving the caret, as an undoable edit. `WysiwygTextView.swift:1250`, `WysiwygTextView.swift:1264`, `WysiwygTextView.swift:1271`
- [ ] Clicking on an item's text only places the caret. `WysiwygTextView.swift:1264`
- [ ] When the checkbox's line is the caret's, the raw prefix shows and there is no drawn box to click. `WysiwygEngine.swift:1039`

### B13. Tables and math

- [ ] Placing the caret in a table (click or arrows) opens the whole table as raw Markdown; leaving renders the grid again. Editing is on source only: no cell UI, column handles or row buttons. `WysiwygTextView.swift:1233`, `WysiwygTextView.swift:967`
- [ ] Placing the caret in a math span opens it as raw source, and leaving renders it. `WysiwygTextView.swift:974`
- [ ] A click anywhere with no drag re-renders the lines concerned, line by line where safe, whole note otherwise. `WysiwygTextView.swift:1238`, `WysiwygTextView.swift:873`
- [ ] A restyle keeps the caret's line at the same height in the viewport, even if things above change height, and keeps the scroll position otherwise. `WysiwygTextView.swift:946`, `WysiwygTextView.swift:994`
- [ ] A caret at a table or image's trailing edge stays after it, not before. `CursorMapping.swift:59`

### B14. Images and video

- [ ] Clicking an image puts the caret at the end of its source and reveals the source above it, rather than selecting the image. `WysiwygTextView.swift:1316`
- [ ] Clicking a video poster opens an inline player over the poster, inline controls with a full-screen button, radius 8, and starts playing. `WysiwygTextView.swift:1284`, `WysiwygTextView.swift:1299`
- [ ] The scroll wheel over a playing video scrolls the note instead of scrubbing. `WysiwygTextView.swift:48`
- [ ] A player stays on its card as text re-lays out; it stops and is removed if the card is deleted or its source is revealed. `WysiwygTextView.swift:1396`
- [ ] Clicks on the 26 pt caption below the poster do not start playback. `WysiwygTextView.swift:1292`, `VideoAttachment.swift:73`

### B15. Frontmatter

- [ ] Clicking the panel opens it as source, caret at the end of the last property, without scrolling the page. `WysiwygTextView.swift:1347`, `WysiwygTextView.swift:1365`
- [ ] A caret click resolving to offset 0 with a panel also opens it. `WysiwygTextView.swift:1239`
- [ ] Moving the caret out of the frontmatter collapses it to the panel again. `WysiwygEngine.swift:751`

### B16. Outline rail

- [ ] The rail appears at the page's right edge only when the note has headings; it never covers centered text: 30 wide with a 14 inset. `OutlineRail.swift:37`, `OutlineRail.swift:38`, `OutlineRail.swift:49`
- [ ] Outline headings: `#{1,6}`, whitespace, then a non-space; lines between fence toggles are skipped; order in the document. `WysiwygTextView.swift:508`
- [ ] One tick per heading, 2 pt tall, capsule, `text`; width 7 (h1), 5 (h2), 3.5 (others); resting opacity 22%; vertically centered. `OutlineRail.swift:217`, `OutlineRail.swift:226`
- [ ] Tick pitch 12, rail height at most 520 and at most pane height - 40; beyond that ticks compress. `OutlineRail.swift:35`, `OutlineRail.swift:63`
- [ ] Under the pointer, ticks swell leftward like a dock: proximity `max(0, 1 - |i - pos| / 3.5)^2`; x-scale `1 + 3p`, anchored right; opacity `0.22 + 0.78p`; spring mass 0.5, stiffness 700, damping 45. `OutlineRail.swift:74`, `OutlineRail.swift:157`, `OutlineRail.swift:229`
- [ ] After 120 ms over the rail, a preview card appears left of it for the nearest heading and glides between headings. `OutlineRail.swift:42`, `OutlineRail.swift:168`
- [ ] Card: 280 wide, 8 gap from the rail, 12 margin from page edges, radius 10, `page` fill, 1 pt `border` at 65%, shadows black 12% r12 y6 and black 4% r2.5 y2. `OutlineRail.swift:39`, `OutlineRail.swift:40`, `OutlineRail.swift:41`, `OutlineRail.swift:134`, `OutlineRail.swift:145`
- [ ] Card content: heading in 12 pt medium `text`, one line tail-truncated; summary 12 pt `text3`, line spacing 3, up to 3 lines; padding 14 by 12, spacing 6. `OutlineRail.swift:107`, `OutlineRail.swift:113`, `OutlineRail.swift:120`
- [ ] Summary: section body between this heading and the next, excluding code, table rows and rules, stripping block prefixes, images, link syntax and emphasis, single-spaced, at most 240 characters. `OutlineSectionPreview.swift:9`, `EditorView.swift:45`
- [ ] Moving between headings: card content drops to 45% opacity and returns over 0.09 s; position eases over 0.18 s; opening fades over 0.16 s and slides 4 pt; easing curve (0.22, 1, 0.36, 1). `OutlineRail.swift:45`, `OutlineRail.swift:94`, `OutlineRail.swift:96`, `OutlineRail.swift:125`, `OutlineRail.swift:153`
- [ ] Leaving the rail closes the card after 120 ms unless the pointer is over the card. `OutlineRail.swift:149`, `OutlineRail.swift:178`
- [ ] Clicking a tick (nearest) or the card scrolls to the heading, with 12 pt margin above it, in the key window's editor only. `OutlineRail.swift:86`, `OutlineRail.swift:103`, `WysiwygTextView.swift:531`, `WysiwygTextView.swift:557`
- [ ] Reduce Motion: no spring, no slides, no content dip. `OutlineRail.swift:22`, `OutlineRail.swift:74`, `OutlineRail.swift:126`
- [ ] The rail updates after typing pauses for 300 ms. `EditorView.swift:171`

### B17. Saving, restoring and live data

- [ ] Typing saves 500 ms after the last keystroke, to the note it was typed in even if another has since opened. `EditorView.swift:229`, `EditorView.swift:233`, `EditorView.swift:241`
- [ ] Outline, word counts and publish preview catch up 300 ms after typing pauses; a keystroke redraws none of them. `EditorView.swift:165`, `EditorView.swift:171`
- [ ] Switching tabs and back restores selection, scroll position and focus, per note, within this launch. `WysiwygTextView.swift:138`, `WysiwygTextView.swift:207`, `WysiwygTextView.swift:236`
- [ ] A tab switched back to does not flash its top first: restore runs before anything is drawn. `WysiwygTextView.swift:69`, `WysiwygTextView.swift:75`
- [ ] A draft takes focus when it appears with the caret at the end; its first save puts it in the library in the tab it was typed in, with the caret carrying on. `WysiwygTextView.swift:732`, `EditorView.swift:102`, `EditorView.swift:251`, `EditorContainerView.swift:11`
- [ ] Clicking in the page focuses the editor, even as the first click on an inactive window. `WysiwygTextView.swift:80`, `WysiwygTextView.swift:84`, `WysiwygTextView.swift:1224`

## C. Window and shell

### C1. Window chrome and page

- [ ] One window type with a unified toolbar; the first window of a fresh install opens at 1200 x 800, later launches restore the last size. `FumiApp.swift:25`, `FumiApp.swift:26`, `FumiApp.swift:27`
- [ ] The window's title and subtitle are the page's: file name, and folder with edited label. `EditorView.swift:337`, `FilePreviewView.swift:20`, `MemosView.swift:72`
- [ ] Minimum window size: width 240 + detail minimum, height 600, where detail minimum = column width + 80 + 16 (1016 wide at the Medium width). `AppShellView.swift:20`, `AppShellView.swift:120`
- [ ] Traffic lights are the standard system ones, with no custom placement; the sidebar toggle sits on their line. `SidebarView.swift:34`
- [ ] No hard line under the title bar: titlebar separators are off for the window and every split item. `SidebarView.swift:1105`
- [ ] The toolbar has no background of its own: title and icons sit on the window ground. `AppShellView.swift:116`, `FolderOpenView.swift:35`
- [ ] The toolbar's "Icon and Text / Icon Only" menu is disabled (macOS 15+). `TabContextMenu.swift:34`
- [ ] The system's sidebar toggle is removed in favor of Fumi's own. `AppShellView.swift:70`
- [ ] The page floats on the ground: 8 pt inset left, right and bottom, none on top; radius 12 (continuous). `AppShellView.swift:13`, `AppShellView.swift:14`, `AppShellView.swift:108`
- [ ] Page edge: 1 pt stroke of `pageEdge` (black 6% in light, white 7% in dark), not intercepting clicks. `AppShellView.swift:97`, `ColorTokens.swift:180`
- [ ] Page shadow: black 5% radius 1.5 y 1 and black 4% radius 8 y 4; none in the White theme. `AppShellView.swift:51`, `AppShellView.swift:92`
- [ ] Window ground is `ground`, in Glass `ground` at 12% over the blur. `ColorTokens.swift:201`, `AppModel.swift:184`
- [ ] Page sheet is `page`, in Glass `page` at 50%. `ColorTokens.swift:207`
- [ ] Accent tints native controls everywhere instead of the system accent. `FumiApp.swift:46`
- [ ] Changing theme or accent reskins the whole window without losing the open folder, tabs or notes. `FumiApp.swift:51`, `ColorTokens.swift:39`
- [ ] Toolbar items dim with the window behind search and Settings, matching the scrim's darkening, animating 0.16 s. `AppShellView.swift:209`, `ColorTokens.swift:191`

### C2. Glass theme window

- [ ] Glass: what is behind the window shows through; macOS 26 uses native regular Liquid Glass, earlier systems the sidebar behind-window blur kept active when unfocused. `AppShellView.swift:146`, `AppShellView.swift:252`, `AppShellView.swift:257`
- [ ] Glass: window becomes non-opaque with a clear background, restored when the theme changes. `AppShellView.swift:276`, `AppShellView.swift:281`
- [ ] Glass: on macOS 15+ the window container paints no background. `AppShellView.swift:295`
- [ ] Glass: note toolbar icons sit in glass capsules on macOS 26, the title stays bare; other themes show flat icons. `EditorView.swift:454`, `EditorView.swift:460`
- [ ] Glass: selected sidebar rows are a raised white pill with a shadow of black 10%, radius 1.5, y 0.5. `ColorTokens.swift:219`, `SidebarView.swift:447`
- [ ] Glass: active tab uses `text` for its title, has a `pageEdge` outline and a shadow of black 7%, radius 5, y -1. `TabStrip.swift:349`, `TabStrip.swift:362`, `TabStrip.swift:367`
- [ ] Glass: Settings' scrim is 45% of normal strength, so the note reads through. `AppShellView.swift:200`

### C3. Welcome-to-window hand-over

- [ ] Opening a library from the welcome page starts as one plain white sheet (page radius 0, no inset, no stroke, `page` ground), then 0.12 s later animates into the framed window over 0.5 s, ease in-out. `AppModel.swift:181`, `AppShellView.swift:52`, `AppShellView.swift:108`, `AppShellView.swift:157`, `AppModel.swift:193`
- [ ] The switch between welcome page and shell fades over 0.2 s. `AppShellView.swift:162`
- [ ] Newly opened libraries get their saved or default sidebar width after the expand animation, 0.55 s after expanding. `SidebarView.swift:1086`

### C4. Note toolbar

- [ ] Note toolbar, left to right after the title: Preview (only with a publish extension on), New Note, Info, More. `EditorView.swift:347`, `EditorView.swift:367`, `EditorView.swift:380`, `EditorView.swift:397`
- [ ] Icons are Fumi's own line glyphs at 18 pt on a 24 grid, stroke 1.7 at 24 scale, round caps and joins. `ShiyeIcon.swift:38`, `ShiyeIcon.swift:173`
- [ ] Glyphs to match: sidebar, newNote, note, library, tag, memo, image, video, file, info, more, phone. `ShiyeIcon.swift:53`
- [ ] Title is the file name in the body family, 13 pt medium, `text2`, tail-truncated, at most 560 wide, 6 pt leading. `TabStrip.swift:38`, `TabStrip.swift:50`, `TabStrip.swift:62`, `EditorView.swift:323`
- [ ] Subtitle under the title, 12 pt `faintText`: `Folder / Sub · Edited ...`, or just the edited label at the library root. `EditorView.swift:328`, `TabStrip.swift:44`
- [ ] Edited label: "Edited 14:20" today, "Edited yesterday 09:12", "Edited Oct 8" this year, with the year otherwise. `EditorView.swift:433`
- [ ] A lone pinned page shows a 9 pt pin in `faintText` before its title, tooltip "Pinned". `TabStrip.swift:32`
- [ ] Right-clicking the title shows the tab menu for the page. `TabStrip.swift:52`
- [ ] Before macOS 26 the system title and subtitle show, with the tab strip when tabs > 1. `TabStrip.swift:56`, `EditorView.swift:493`
- [ ] Draft page: toolbar title "New Note" and only the New Note button, no Info, More or Preview. `EditorView.swift:70`, `EditorView.swift:347`, `EditorView.swift:380`
- [ ] Empty library page: toolbar title "Notes", New Note only. `EditorView.swift:302`, `EditorContainerView.swift:43`
- [ ] Preview button: phone glyph, accent while open; tooltip "WeChat Preview (⌃⌘P)", "Xiaohongshu Preview (⌃⌘P)" or "Publish Preview (⌃⌘P)" by which extensions are on, "Hide ..." while open. `EditorView.swift:313`, `EditorView.swift:356`, `EditorView.swift:363`
- [ ] New Note button: tooltip "New Note (⌘N)", a blank draft in a new tab. `EditorView.swift:369`, `EditorView.swift:377`
- [ ] Info button: tooltip "Note Info", opens a popover below it. `EditorView.swift:383`, `EditorView.swift:391`
- [ ] Info popover, 240 wide, padding 16: rows Words, Characters, Reading time, Created, Updated, 13 pt label in `text2` and medium value in `text`, spacing 8. `EditorView.swift:518`, `EditorView.swift:551`, `EditorView.swift:562`
- [ ] Reading time `max(1, words / 200)` min; Words = whitespace-separated tokens; Characters = grapheme count of the raw text; Created like "Oct 8, 2026"; Updated relative. `EditorView.swift:224`, `EditorView.swift:214`, `EditorView.swift:511`, `EditorView.swift:521`, `EditorView.swift:522`
- [ ] Info popover tags: a "TAGS" label in 12 pt semibold uppercase `text3` with 0.4 tracking, then wrapped `#tag` capsules, 12 pt medium accent on accent 8%, padding 8 by 3, spacing 6; hidden when none. `EditorView.swift:528`
- [ ] More menu: Pin/Unpin; Reveal in Finder; Export as PDF…; preview toggle (with an extension on); Copy for WeChat (WeChat on); Add Xiaohongshu Cards to Photos (Xiaohongshu on); separator; Delete (destructive), closes its tabs and deletes the note, no confirmation. No menu indicator, tooltip "More". `EditorView.swift:399`, `EditorView.swift:414`, `EditorView.swift:425`

### C5. Sidebar shell

- [ ] Sidebar opens at 240, drags from 200 to 420; dragging narrower collapses it. `AppShellView.swift:61`, `SidebarWidthPolicy.swift:6`, `SidebarWidthPolicy.swift:11`
- [ ] Release rules: under 200 goes to 200; up to 280 rounds, snapping to 240 within 8; between 280 and 300 goes to the nearer end; above 300 clamps to 420, rounds, snapping to 320 within 8. Animated 0.2 s. `SidebarWidthPolicy.swift:20`, `SidebarView.swift:1126`
- [ ] Haptic tick when the divider passes 240 or 320 during a drag. `SidebarWidthPolicy.swift:16`, `SidebarView.swift:1091`
- [ ] Double-click the divider, 5 pt either side: fit the longest row (clamped 200-420), or return to 240 if already fitted. `SidebarView.swift:1055`, `SidebarView.swift:1062`, `SidebarWidthPolicy.swift:45`
- [ ] Width is remembered per library under `fumi.sidebarWidth.<path>`; default 240; saved values below 200 count as unset. `SidebarView.swift:1133`, `SidebarView.swift:1142`
- [ ] Fit width formula per note row: 16 outer + 14 base + 24 x depth + 16 icon + 8 gap + name at 13 pt semibold + 10, + 16 if pinned. `SidebarView.swift:74`
- [ ] Toggle button in the sidebar's part of the toolbar, one item that stays beside the traffic lights when collapsed; flat, no glass circle; tooltip "Hide Sidebar (⌥⌘S)" or "Show Sidebar (⌥⌘S)". `SidebarView.swift:34`, `SidebarView.swift:937`, `SidebarView.swift:965`
- [ ] Toggle animates ease in-out 0.2 s; collapse by dragging stays in sync with the button. `AppModel.swift:196`, `SidebarView.swift:45`
- [ ] Sidebar has the window ground behind it, no divider line against the page. `SidebarView.swift:31`
- [ ] Two levels side by side; going in or back slides one sidebar width, ease in-out 0.25 s; the hidden level takes no clicks. `SidebarView.swift:89`, `SidebarView.swift:104`
- [ ] A window always opens inside its library, not at the Libraries level. `AppModel.swift:173`
- [ ] Going back: "‹ Libraries" row, ⌘↑ when no text view has focus, or a two-finger swipe right over the sidebar past 60 pt with horizontal dominance of 1.5x; once per swipe. `SidebarView.swift:112`, `SidebarView.swift:150`, `LibrariesLevel.swift:343`, `LibrariesLevel.swift:354`, `LibrariesLevel.swift:357`
- [ ] ⌘↑ in the editor is left to the text view (top of note). `LibrariesLevel.swift:344`
- [ ] Sidebar scroller is short: 45% of true length, 28-88 pt, travels the full slot. `QuietScroller.swift:19`, `SidebarView.swift:137`, `LibrariesLevel.swift:45`

### C6. Sidebar, library level (inside a library)

- [ ] Order: ‹ Libraries, library header, Search, Memos, a rule, the notes tree (scrolling), Settings pinned at bottom. `SidebarView.swift:109`, `SidebarView.swift:144`
- [ ] Top group: spacing 1, leading and trailing 8, top 2; rule 1 pt `border`, inset 16 on each side, 8 above and below. `SidebarView.swift:111`, `SidebarView.swift:120`, `SidebarView.swift:124`
- [ ] "‹ Libraries": chevron 10 pt semibold, label 12 pt, 26 tall, padding 8, radius 6; colors `icon` and `text3`, hover `text2` with `cardHover` fill; tooltip "Show Libraries (⌘↑)". `SidebarView.swift:301`, `SidebarView.swift:320`
- [ ] Library header, 32 tall: tile 16, name 13 pt semibold `text`, tail-truncated, leading 10; the display name, else the folder's name, else "Notes". `SidebarView.swift:215`, `SidebarView.swift:221`, `SidebarView.swift:289`
- [ ] Header `…` button, 24 x 22, radius 5, tooltip "Folder Options": Open Folder…, Open in New Window (a new window, not this library), Reveal in Finder, separator, Sort By picker (Date Modified, Date Created, Title), default Date Modified, persisted in `fumi.sortMode`. `SidebarView.swift:240`, `SidebarView.swift:250`, `AppSettings.swift:261`, `AppModel.swift:202`
- [ ] Header `+` button, plus 12 pt medium, 24 x 22, tooltip "New Note (⌘N)": creates a note on disk at once and opens it. `SidebarView.swift:271`
- [ ] Header buttons on hover: `text` icon, `cardHover` fill. `SidebarView.swift:255`, `SidebarView.swift:276`
- [ ] Search row: magnifier 13 pt `icon`, "Search" 13 pt `text3`, 30 tall, padding 10, radius 6; hover `cardHover` with 1 pt `controlBorder`; tooltip "Search Notes (⇧⌘F)"; click opens search. `SidebarView.swift:333`, `SidebarView.swift:358`
- [ ] Memos row: memo glyph 16, "Memos", 30 tall, padding 10, radius 6; selected when the Memos page shows; click opens Memos, ⌘-click in a new tab. `SidebarView.swift:116`, `SidebarView.swift:432`
- [ ] Top-level row states: unselected `icon` and `text2` regular; selected `text` semibold on `groundSelected`; hover `cardHover`. `SidebarView.swift:435`, `SidebarView.swift:438`, `SidebarView.swift:446`
- [ ] Settings row: slider.horizontal.3 symbol 13 pt, "Settings", at the bottom of both levels with padding 8; toggles Settings; selected while open. `SidebarView.swift:144`, `SidebarView.swift:927`

### C7. Note tree

- [ ] Each folder lists subfolders first (natural name order), then notes in the chosen sort with pinned first, then files by name. `SidebarView.swift:519`, `SidebarView.swift:533`, `SidebarView.swift:537`, `SidebarView.swift:188`, `NoteTree.swift:60`
- [ ] Sort: Date Modified newest first; Date Created newest first; Title natural ascending by file name. `SidebarView.swift:190`
- [ ] The Memos folder and everything in it stay out of the tree. `SidebarView.swift:21`
- [ ] Indent: base 14, each level 24, icon 16, icon gap 8; children line up under the parent's name. `SidebarView.swift:459`, `SidebarView.swift:576`
- [ ] Rows 28 tall, radius 6, spacing 1, padding right 10; list bottom 16. `SidebarView.swift:131`, `SidebarView.swift:136`, `SidebarView.swift:578`
- [ ] Folder row: no disclosure arrow; line-drawn folder glyph with stroke 1.15, closed in `icon`, open in `text2`; name 13 pt, `text2` regular when closed, `text` medium when open, `text` when hovered; `cardHover` fill on hover. `SidebarView.swift:564`, `SidebarView.swift:566`, `SidebarView.swift:571`, `SidebarView.swift:469`
- [ ] Clicking a folder toggles expand with ease in-out 0.15 s. `SidebarView.swift:521`
- [ ] Note row: note glyph 15, `faintIcon` (selected `text2`); file name 13 pt `text2` (selected semibold `text` on `groundSelected`), tail-truncated; 9 pt pin in `icon` at the row's end when pinned. `SidebarView.swift:631`, `SidebarView.swift:634`, `SidebarView.swift:640`
- [ ] Note row shows the file name, never the first line. `SidebarView.swift:622`
- [ ] Clicking a note opens it, in its existing tab if any, else a new tab; only a blank unpinned draft is filled in place; ⌘-click always keeps the draft. `SidebarView.swift:628`, `AppModel.swift:242`
- [ ] File row: image, video or file glyph 15, `faintIcon`; name middle-truncated, `text3` (selected semibold `text`). `SidebarView.swift:707`, `SidebarView.swift:723`
- [ ] The selected note's ancestor folders expand automatically whenever the selection changes. `SidebarView.swift:54`, `SidebarView.swift:177`
- [ ] Switching library resets expansion. `SidebarView.swift:58`
- [ ] Empty tree: "No notes yet", 12 pt `text3`, leading 14, 28 tall; shows nothing before the first scan lands. `SidebarView.swift:160`, `SidebarView.swift:164`
- [ ] Hidden: a selected row is not shown as selected while Settings is open. `SidebarView.swift:617`, `SidebarView.swift:703`

### C8. Note tree context menus

- [ ] Note: Pin/Unpin; Rename…; Move To ▸; Reveal in Finder; separator; Delete (destructive, immediate). `SidebarView.swift:663`
- [ ] Rename dialog: title "Rename Note", text field "File name" prefilled, message "Enter a new file name (without the .md extension).", Cancel and Rename; tabs follow the new path. `SidebarView.swift:677`, `SidebarView.swift:686`
- [ ] Folder: Move To ▸; Reveal in Finder. `SidebarView.swift:595`
- [ ] File: Copy Markdown Link; Open in Default App; Move To ▸; Reveal in Finder; separator; Move to Trash (destructive). `SidebarView.swift:746`
- [ ] Copy Markdown Link copies `![](path)` relative to the library root, as plain text. `SidebarView.swift:747`, `AttachmentImport.swift:61`
- [ ] Move To lists the library's name and every folder as "Travel › Japan", excluding the item's own folder and, for a folder, itself and its descendants; disabled if none. `SidebarView.swift:781`, `SidebarView.swift:795`, `SidebarView.swift:800`

### C9. Tree drag and drop

- [ ] Notes, files and folders drag as file URLs, with a label preview: 28 tall, at most 220 wide, radius 7, `page` fill, 1 pt `border`, icon and name 13 pt medium. `SidebarView.swift:588`, `SidebarView.swift:658`, `SidebarView.swift:742`, `SidebarView.swift:858`
- [ ] Drop targets: a folder row moves into it; a note or file row moves into that item's folder; the library header and the empty list go to the top level. `SidebarView.swift:141`, `SidebarView.swift:291`, `SidebarView.swift:594`, `SidebarView.swift:662`, `SidebarView.swift:745`
- [ ] The cursor shows a move, no copy badge. `SidebarView.swift:830`
- [ ] Drop highlight: accent 10% fill, 1 pt accent 50% stroke, radius 6, 0.12 s ease-out. `SidebarView.swift:907`, `ColorTokens.swift:171`
- [ ] Highlight goes straight between rows: changes off a row or onto the list behind wait 70 ms. `NoteTreeState.swift:32`, `NoteTreeState.swift:37`
- [ ] A closed folder held under a drag for 650 ms opens with ease in-out 0.18 s. `NoteTreeState.swift:26`, `NoteTreeState.swift:56`
- [ ] After a move the destination's ancestors open, tabs follow new paths, and a moved folder keeps its expansion and that of those inside. `NoteTreeState.swift:101`, `NoteTreeState.swift:114`
- [ ] Name clash alert: title `Couldn't Move “name”`, message `An item named “name” already exists in “folder”.`, OK. `NoteTreeState.swift:91`, `SidebarView.swift:63`
- [ ] Any other failure alert: `Couldn't Move “file”` with the reason. `NoteTreeState.swift:94`
- [ ] Moving a folder into itself beeps; moving to where it already is, or from outside the library, does nothing. `NoteTreeState.swift:95`, `NoteTreeState.swift:97`

### C10. Sidebar, Libraries level

- [ ] Heading "Libraries": 13 pt semibold `text`, 30 tall; then scrolling rows, then Settings at bottom. `LibrariesLevel.swift:16`, `LibrariesLevel.swift:49`
- [ ] Library row, 44 tall, radius 7, padding 8, spacing 10, rows spacing 2: tile 28; name 13 pt `text`, semibold when current, tail-truncated; second line 12 pt `text3`, the location; chevron.right 9 pt bold `faintIcon` at the end. `LibrariesLevel.swift:100`, `LibrariesLevel.swift:124`, `LibrariesLevel.swift:134`
- [ ] Second line of the current library appends " · 516 notes" (" · 1 note"), once loaded; a long location middle-truncates, the count stays whole. `LibrariesLevel.swift:66`, `LibrariesLevel.swift:109`
- [ ] Current library fill `groundSelected`, hover `cardHover`. `LibrariesLevel.swift:137`
- [ ] A missing library: name and detail in `faintText`, detail is why it is missing, no chevron, tile faded. `LibrariesLevel.swift:61`, `LibrariesLevel.swift:105`, `LibrariesLevel.swift:121`
- [ ] Hovering a non-current, non-missing row replaces the chevron with a new-window button: arrow.up.forward.square 12 pt, 24 x 24, radius 5, tooltip "Open in New Window". `LibrariesLevel.swift:96`, `LibrariesLevel.swift:147`, `LibrariesLevel.swift:243`
- [ ] Row tooltip: full path, plus "⌘-click to open in a new window" when not missing. `LibrariesLevel.swift:154`
- [ ] Click enters the library; ⌘-click on a non-current one opens it in its own window or brings forward the window showing it. `LibrariesLevel.swift:73`, `Libraries.swift:47`
- [ ] Entering a library open in another window brings that window forward instead. `Libraries.swift:26`
- [ ] Entering a missing library shows the missing-folder page. `Libraries.swift:20`
- [ ] Rows drag to reorder, showing a tile-and-name preview, with a 1 pt accent 50% target stroke; move animates 0.15 s. `LibrariesLevel.swift:142`, `LibrariesLevel.swift:156`, `LibrariesLevel.swift:165`
- [ ] Context menu: Open in New Window (disabled if current or missing); Reveal in Finder (disabled if missing); Rename…; Change Icon…; separator; Remove from List (disabled if current). `LibrariesLevel.swift:170`
- [ ] Rename dialog "Rename Library": field "Name"; "Only the name Fumi shows changes; the folder keeps its own. Leave it empty to use the folder’s name."; Cancel and Rename. `LibrariesLevel.swift:187`
- [ ] Icon dialog "Change Icon": field "A letter or emoji"; "One character or emoji. Leave it empty for the plain library icon."; Cancel and Change. `LibrariesLevel.swift:194`
- [ ] Tile with no icon: library line glyph 16 centered, `icon`, current `text2`, missing `faintIcon`. `LibrariesLevel.swift:230`
- [ ] Tile with an icon: character 13 pt or emoji 16 pt semibold (scaled by size / 28), on a callout tint by the entry's `tint` index among note, tip, important, warning, danger, plain, radius 7 scaled; missing: `faintText` on `inlineCode`, 60% opacity. `LibrariesLevel.swift:215`, `LibrariesLevel.swift:218`
- [ ] Open Folder… row: folder glyph, 13 pt `text3`, hover `text`, right "⌘O" in 12 pt `faintText`, 30 tall; 6 pt above. `LibrariesLevel.swift:39`, `LibrariesLevel.swift:266`
- [ ] Open Folder panel: title "Open Notes Folder", button "Open", folders only, single selection, can create folders; can start at a folder's nearest existing ancestor. `FolderOpenView.swift:134`
- [ ] Show Libraries first shows the sidebar if hidden. `Libraries.swift:32`

### C11. Tab strip

- [ ] With one page there is no strip, just the title; with more than one the strip replaces the title on every page type. `AppModel.swift:262`, `TabStrip.swift:26`
- [ ] A page is one of: blank draft, note, Memos, file preview. A window always has at least one tab; the last one closed leaves a blank draft. `NoteTabs.swift:6`, `AppModel.swift:309`
- [ ] Strip height 52; tab height 40, title in the upper 28 centering on the toolbar's middle line. `TabStrip.swift:78`, `TabStrip.swift:83`, `TabStrip.swift:84`
- [ ] Strip width: page width - 164, - 44 with a publish extension, - `max(0, 132 - page left)`, at least 240; read from where the page actually is. `AppModel.swift:31`, `TabStrip.swift:94`
- [ ] Tab width shares the room between 96 and 208; pinned tabs 46. Leading inset 14, trailing 10. `TabStrip.swift:86`, `TabStrip.swift:87`, `TabStrip.swift:88`, `TabStrip.swift:90`, `TabStrip.swift:96`
- [ ] Below 96 each the strip scrolls horizontally without a scrollbar, trailing edge fading from 94%, and a ⌄ button (28 x 28, chevron.down 11 pt semibold `text2`, tooltip "All Tabs") lists every tab with a check on the active; the active tab scrolls into view, 0.2 s. `TabStrip.swift:120`, `TabStrip.swift:122`, `TabStrip.swift:130`, `TabStrip.swift:175`, `TabStrip.swift:202`
- [ ] Active tab: page color, top radius 10, lower corners flaring out 10 on each side into the page. `TabStrip.swift:355`, `TabStrip.swift:368`, `TabStrip.swift:378`
- [ ] Inactive tab: title in `text3`; hover: `text` at 4%, top radius 10. `TabStrip.swift:350`, `TabStrip.swift:370`
- [ ] Separators between tabs: 1 x 14 in `text` at 16%, hidden next to an active or hovered tab. `TabStrip.swift:144`, `TabStrip.swift:154`
- [ ] Title: body family, 13 pt medium, tail-truncated, padding 14 left and 28 right; active `text2`. `TabStrip.swift:317`, `TabStrip.swift:349`
- [ ] Titles: file name without extension; file name with extension for files; "Memos"; a draft reads "Untitled" in italic. `TabStrip.swift:221`, `TabStrip.swift:318`
- [ ] Hover tip: name, newline, folder path and edited label for notes; name and edited label for files; title otherwise. `TabStrip.swift:232`
- [ ] Close button: on active or hovered, unpinned tabs; xmark 8.5 pt bold, 16 x 16, radius 4, 7 from the right; hover `text` on `text` at 10%; tooltip "Close Tab (⌘W)". `TabStrip.swift:278`, `TabStrip.swift:292`, `TabStrip.swift:332`
- [ ] Pinned tab: glyph 15 (active `text2`, else `icon`) and pin mark 8 pt semibold in `icon`, no title, no close button; leading 9; pinned tabs stay first, in pin order. `TabStrip.swift:304`, `AppModel.swift:359`
- [ ] Tab glyphs: note for note and draft; memo; image, video or file by kind. `TabStrip.swift:244`
- [ ] Clicking a tab selects it, dismissing search and Settings. `TabStrip.swift:281`, `AppModel.swift:279`
- [ ] Dragging a tab over another moves it there live, 0.16 s, within its pinned or unpinned side. `TabStrip.swift:166`, `TabStrip.swift:412`, `AppModel.swift:370`
- [ ] Strip changes animate ease-out 0.18 s. `TabStrip.swift:136`

### C12. Tab behavior and context menu

- [ ] Tab context menu, works inside the toolbar via right-click or control-click, smallest region under the pointer wins: Pin Tab or Unpin Tab. `TabContextMenu.swift:66`, `TabContextMenu.swift:75`, `TabContextMenu.swift:105`
- [ ] With more than one tab, then: separator; Close Tab (disabled if pinned); Close Other Tabs (disabled if all others are pinned); Close Tabs to the Right (disabled on the last). `TabContextMenu.swift:108`
- [ ] For notes and files, then: separator; Reveal in Sidebar (selects the tab, shows the sidebar); Reveal in Finder. `TabContextMenu.swift:121`
- [ ] Right-clicking the strip's empty part offers the active tab's menu. `TabStrip.swift:138`
- [ ] New Note, New Tab and ⌘N are one thing: a blank draft in a new tab; if the active tab is already blank, nothing new opens. `AppModel.swift:266`, `FumiApp.swift:95`
- [ ] A new tab goes right after the active tab, never among pinned ones. `AppModel.swift:273`
- [ ] Closing the active tab focuses the one on its right, else on its left. `AppModel.swift:313`
- [ ] ⌘W on a pinned active tab does not close it: the first unpinned tab comes forward, or a new draft opens. `AppModel.swift:324`
- [ ] Close Other Tabs and Close Tabs to the Right leave pinned tabs. `AppModel.swift:337`, `AppModel.swift:343`
- [ ] ⇧⌘T reopens the most recently closed tab, up to 30 kept, skipping those whose note or file is gone; pinned state restored; closed drafts are not kept. `AppModel.swift:305`, `AppModel.swift:307`, `AppModel.swift:350`
- [ ] ⌘1-⌘8 pick that tab, ⌘9 the last; ⌃Tab and ⇧⌘] next, ⌃⇧Tab and ⇧⌘[ previous, wrapping. `AppShellView.swift:226`, `AppShellView.swift:230`, `AppModel.swift:286`, `AppModel.swift:294`, `FumiApp.swift:133`
- [ ] Each tab keeps its own Back and Forward (⌘[ and ⌘]), up to 50 steps; a blank draft is not a place to return to; a new page clears Forward; repeats move to the end of Back. `NoteTabs.swift:57`, `NoteTabs.swift:65`, `AppModel.swift:61`
- [ ] Moves and renames update every tab and history entry inside the moved path. `AppModel.swift:380`
- [ ] Deleted notes and files close their tabs, or turn the only tab into a draft; a disk change that removes them does the same. `AppModel.swift:394`, `AppModel.swift:409`, `AppShellView.swift:171`
- [ ] Opening a note, memo or file dismisses search and Settings. `AppModel.swift:243`, `AppModel.swift:253`

### C13. Publish preview in the shell

- [ ] The preview opens per tab, sliding from the right with a fade, 0.22 s; only for notes, not Memos or files. `AppModel.swift:92`, `AppModel.swift:96`, `AppShellView.swift:27`, `AppShellView.swift:45`
- [ ] With room (detail width at least the page's minimum + preview width + 8) it is a second sheet to the page's right, 8 pt gap; shadow black 5%, radius 1.5, y 1. `AppShellView.swift:33`, `AppShellView.swift:42`, `AppShellView.swift:100`
- [ ] Too narrow: it lies over the page's right side; shadow black 12%, radius 16, y 1. `AppShellView.swift:42`, `AppShellView.swift:103`
- [ ] Preview sheet radius matches the page, 1 pt `pageEdge`. `AppShellView.swift:44`

### C14. Closing, reopening, multiple windows

- [ ] ⇧⌘N opens another window; each window has its own folder, tabs and state, edited in parallel. `FumiApp.swift:32`, `FumiApp.swift:106`
- [ ] A new window with no library requested starts at the welcome page. `WindowCloseBehavior.swift:76`, `AppShellView.swift:54`
- [ ] Menu commands act on the focused window. `FumiApp.swift:47`
- [ ] ⌘W closes the active tab; on the only tab, and for the red button, the window closes. `WindowCloseBehavior.swift:121`
- [ ] A closed window is really hidden: Dock click brings it back at once, without rescanning. `WindowCloseBehavior.swift:129`, `WindowCloseBehavior.swift:145`
- [ ] Clicking the Dock icon with no visible windows shows hidden windows, and activates the app; empty windows the system makes within the next 2 s are discarded. `WindowCloseBehavior.swift:27`, `WindowCloseBehavior.swift:140`, `WindowCloseBehavior.swift:151`
- [ ] Quit is immediate, with no confirmation; a window closing during quit really closes. `WindowCloseBehavior.swift:22`, `WindowCloseBehavior.swift:120`

### C15. Session restore

- [ ] macOS does not resurrect windows: `NSQuitAlwaysKeepsWindows` is false and windows are not restorable; launch opens one window. `FumiApp.swift:14`, `WindowCloseBehavior.swift:68`
- [ ] Empty extra windows in the first 2 s of launch are discarded. `WindowCloseBehavior.swift:57`
- [ ] The first window reopens the last library; if it is gone, the missing-folder page says so. `WindowCloseBehavior.swift:91`, `WindowCloseBehavior.swift:98`
- [ ] Each library keeps its own tabs between launches (tabs, pin state, active index) under `fumi.tabs`; reopening the folder brings them back. `NoteTabs.swift:32`, `AppShellView.swift:165`, `AppModel.swift:431`
- [ ] With no saved set, opening a library keeps tabs inside the new folder, drafts and Memos, and closes others. `AppModel.swift:440`
- [ ] Back/Forward history and reopen list do not survive switching library or relaunch. `AppModel.swift:436`
- [ ] Tabs changed while another folder is opening are not saved under the new one. `AppModel.swift:452`
- [ ] Tabs of notes gone by launch close once the scan lands. `AppShellView.swift:173`, `AppShellView.swift:181`

## D. Search, tags, folder view, file preview

### D1. Quick search panel

- [ ] Opens from ⇧⌘F or the sidebar's Search row as a floating panel 92 pt from the window's top; it shows only with a library open. `FumiApp.swift:191`, `SidebarView.swift:333`, `AppShellView.swift:54`, `AppShellView.swift:129`
- [ ] Behind it, a scrim over the window: black 16% in light, 45% in dark; clicking it dismisses. `AppShellView.swift:124`, `ColorTokens.swift:195`
- [ ] Appears and disappears with fade and scale from 0.96 at the top; 0.16 s ease-out. `AppShellView.swift:130`, `AppShellView.swift:151`
- [ ] Panel: 600 wide, radius 12, `page` fill, 1 pt `controlBorder`, shadow black 22%, radius 32, y 12. `SearchPopover.swift:65`, `SearchPopover.swift:68`
- [ ] Height hugs its content: 330 at most when the query is empty or there are no hits, else `min(480, 112 + 44 x hits)`. `SearchPopover.swift:52`
- [ ] Header: magnifier 15 pt medium `icon`, field "Search notes" at 15 pt `text`, focused on open; right side a boxless "esc" hint when empty, or a clear button (xmark.circle.fill 13 pt, tooltip "Clear (esc)") with text. Padding 14 by 11. `SearchPopover.swift:92`, `SearchPopover.swift:95`, `SearchPopover.swift:102`, `SearchPopover.swift:105`, `SearchPopover.swift:114`
- [ ] 1 pt `border` rules above and below the results. `SearchPopover.swift:60`, `SearchPopover.swift:62`
- [ ] Footer centered: keys ↑↓ "Navigate", ↩ "Open", esc "Dismiss", 12 pt `text3`, spacing 16, padding 14 by 7. `SearchPopover.swift:154`
- [ ] Section label, 12 pt semibold `text3`: "Recently Updated" with an empty query; "Searching…" until the search settles; "1 result" or "N results". `SearchPopover.swift:44`
- [ ] Empty query lists the 6 most recently updated notes. `SearchPopover.swift:78`
- [ ] The query is fuzzy matched against every note's file name, title, folder path, tags and body; `#` prefix searches tags. `SearchPopover.swift:6`, `SearchPopover.swift:180`
- [ ] Searches run off the main thread, debounced 60 ms once prepared, newest query cancels older. `SearchPopover.swift:170`
- [ ] Corpus rebuilds when the note count changes. `SearchPopover.swift:81`
- [ ] ↑↓ move and wrap around; typing resets to the first. `SearchPopover.swift:82`, `SearchPopover.swift:83`, `SearchPopover.swift:189`
- [ ] ↩ opens the active row, hovered row winning over the keyboard's; ⌘↩ opens in a new tab; a ↩ faster than the search finishes it first. `SearchPopover.swift:39`, `SearchPopover.swift:197`
- [ ] Click opens a row; ⌘-click in a new tab. `SearchPopover.swift:133`
- [ ] Esc dismisses. `SearchPopover.swift:85`, `SearchPopover.swift:101`
- [ ] The selected row scrolls to center, 0.12 s. `SearchPopover.swift:144`
- [ ] Result row: padding 12 by 7, spacing 10, radius 7; selected accent 6%, hover `cardHover`. `SearchPopover.swift:247`, `SearchPopover.swift:250`, `ColorTokens.swift:172`
- [ ] Result icon, 16 wide: pin.fill 12 pt for pinned, else note glyph 15; accent when selected, else `faintIcon`. `SearchPopover.swift:222`
- [ ] Result title: file name 13 pt `text`, semibold when selected; matched runs accent and bold. `SearchPopover.swift:232`, `SearchPopover.swift:339`
- [ ] Selected row shows a return symbol at the right, 10 pt medium `text3`. `SearchPopover.swift:240`
- [ ] Second line, 12 pt: body match shows folder in `text3`, "·" in `faintText`, snippet in `text2`, with matches highlighted. `SearchPopover.swift:267`
- [ ] Second line: tag match shows a tag glyph 11 in `faintIcon` then the tag in `text2`; title match in `text2`; file name, folder and recent matches in `text3`. `SearchPopover.swift:278`, `SearchPopover.swift:283`, `SearchPopover.swift:286`
- [ ] No results: 44 pt circle `quietSurface` with a 20 pt magnifier in `faintIcon`; "No results for “q”" 13 pt medium `text2`; "Try different keywords, or search tags with a # prefix." 12 pt `text3`; padding 34. `SearchPopover.swift:298`

### D2. Tags

- [ ] Tags show in the Info popover and as `#tag` pills in the text; there is no tag browser in the sidebar. `EditorView.swift:536`, `WysiwygEngine.swift:326`
- [ ] Tags are found through search: a `#` prefix searches them, and a tag hit shows the tag glyph. `SearchPopover.swift:7`, `SearchPopover.swift:278`, `SearchPopover.swift:311`
- [ ] Frontmatter list values draw as colored pills in the properties panel. `FrontmatterAttachment.swift:59`

### D3. Folder view

- [ ] A folder has no page of its own: its contents show only by expanding its row in the tree. `SidebarView.swift:562`, `AppShellView.swift:79`
- [ ] A library with no notes shows, 140 pt from the page's top, 440 wide: "No notes here yet" in the body family, 20 pt semibold `text`. `EditorContainerView.swift:58`, `EditorContainerView.swift:92`
- [ ] Under it: `“Folder” is a plain folder. Write here, or drop .md files into it — they show up on the left.`, 13 pt `text3`, line spacing 4, 8 above. `EditorContainerView.swift:60`
- [ ] Actions 24 pt below, spacing 22: new-note glyph 15, "Write the First Note" medium, "⌘N" in `faintText`, 13 pt `text`, underlined 1 pt, 3 below; creates a note on disk and opens it. `EditorContainerView.swift:67`
- [ ] "Reveal in Finder", 13 pt `text3`, hover `text`. `EditorContainerView.swift:83`
- [ ] While a library's first scan runs the page is blank, not "no notes". `EditorContainerView.swift:35`
- [ ] A library that has notes opens a blank draft when no note is selected. `EditorContainerView.swift:13`

### D4. File preview

- [ ] Images, videos and PDFs open from the tree on the page in place of the editor, per tab. `AppShellView.swift:81`, `SidebarView.swift:717`
- [ ] Toolbar: title (file name), subtitle `Folder / Sub · 1.2 MB`, New Note, More. `FilePreviewView.swift:24`, `FilePreviewView.swift:68`
- [ ] More menu: Copy Markdown Link; Open in Default App; Reveal in Finder; separator; Move to Trash (destructive; beeps on failure). `FilePreviewView.swift:41`, `FilePreviewView.swift:83`
- [ ] Moving to the Trash closes the file's tabs and rescans. `FilePreviewView.swift:86`
- [ ] Image: 40 pt padding, at its own size, shrunk to fit, never enlarged; GIFs animate; SVGs draw; not editable. `FilePreviewView.swift:17`, `FilePreviewView.swift:95`
- [ ] Video: 40 pt padding, inline controls with a full-screen button, shaped to its orientation-corrected aspect (16:9 fallback), radius 8; hidden until the aspect is known; stops when closed; does not autoplay. `FilePreviewView.swift:17`, `FilePreviewView.swift:119`, `FilePreviewView.swift:123`, `FilePreviewView.swift:140`, `FilePreviewView.swift:149`
- [ ] PDF: no padding, single-page continuous, auto-scaled, clear background. `FilePreviewView.swift:17`, `FilePreviewView.swift:160`

## E. Memos

### E1. Page layout

- [ ] Memos replaces the editor on the page, in a tab; centered 460 wide column; 34 top and 48 bottom padding; page edge blur at the top; slim quiet scroller. `AppShellView.swift:79`, `MemosView.swift:19`, `MemosView.swift:50`, `MemosView.swift:53`, `MemosView.swift:55`
- [ ] When the page is at least 460 + 48 + 200 + 80 = 788 wide, a 200 wide right margin holds the calendar and tags, 48 gap, 36 from the top, staying put while memos scroll. `MemosView.swift:20`, `MemosView.swift:25`, `MemosView.swift:57`
- [ ] Narrower: no tags margin; the calendar sits above the memos at the column's right, 22 below the composer. `MemosView.swift:82`
- [ ] Toolbar: system title "Memos" with one tab, tab strip with several; no action buttons. `MemosView.swift:72`, `MemosView.swift:893`
- [ ] With no memos at all: "Memos are saved in the Memos folder, one file per day." 12 pt `faintText`, 28 above. `MemosView.swift:94`

### E2. Composer

- [ ] No box: placeholder "Write it down…" in `faintText` on the paper, a 1 pt `text` at 9% hairline under it, 12 below the text; grows with its text, minimum 26. `MemosView.swift:158`, `MemosView.swift:167`, `MemosView.swift:171`, `MemoText.swift:97`
- [ ] Text at 15 pt in the editor's font, line spacing 6, `text`, CJK in the paired face; caret in the accent; takes focus when the page appears. `MemoText.swift:47`, `MemoText.swift:73`, `MemoText.swift:119`, `MemoText.swift:132`, `MemosView.swift:161`
- [ ] Plain text only; undo on; smart quotes and spell check follow Settings when created. `MemoText.swift:61`, `MemoText.swift:64`, `MemoText.swift:65`, `MemoText.swift:67`
- [ ] Return starts a new line; ⌘↩ or keypad ⌘Enter saves. `MemosView.swift:155`, `MemoText.swift:195`
- [ ] Live highlighting, syntax left in place but quieter: markers `marker`; strong bold in the bold color; emphasis italic, keeping an enclosing bold; strike with `faintText` line. `MemoText.swift:154`, `MemoText.swift:156`, `MemoText.swift:161`, `MemoText.swift:168`
- [ ] Live highlighting continued: code small monospaced #C4402F/#E5735F on `inlineCode` background; links and tags accent; quotes `text2`; code blocks small monospaced; done to-dos struck and `text3`. `MemoText.swift:170`, `MemoText.swift:173`, `MemoText.swift:175`, `MemoText.swift:177`, `MemoText.swift:179`
- [ ] Under it, 8 below, spacing 14: left "# Tag" button, 12 pt `faintText`, hover `text2`, tooltip "Add a tag": appends `#` (with a space before it if needed) and refocuses. `MemosView.swift:173`, `MemosView.swift:174`, `MemosView.swift:205`
- [ ] Right "Save" button, tooltip "Save (⌘↩)", 12 pt, 24 tall, radius 6, padding 10, disabled and `faintText` when the text is blank, accent otherwise; wash accent 10% hover, 16% pressed, 0.12 s; word aligns with the hairline's end. `MemosView.swift:180`, `MemosView.swift:228`, `MemosView.swift:239`
- [ ] Saving adds the memo to today's file in the library and clears the composer; memos newest first. `MemosView.swift:80`, `MemosView.swift:190`
- [ ] Composer text is lost when the page goes away (kept only in view state). `MemosView.swift:160`

### E3. Calendar

- [ ] A month, Monday first, 7 cells of 28 wide, rows 30 tall, weekday letters M T W T F S S at 12 pt `faintText`. `MemosView.swift:334`, `MemosView.swift:335`, `MemosView.swift:337`, `MemosView.swift:367`
- [ ] Header: month name serif 15 pt semibold `text`, " 2026" serif 15 pt `faintText`; previous and next arrows, chevron 10 pt semibold `faintText`, 20 x 20 circle, hover `text2` on `cardHover`, accessibility labels "Previous Month" and "Next Month". `MemosView.swift:352`, `MemosView.swift:360`, `MemosView.swift:479`
- [ ] Day: number in a 24 x 24 radius 7 square; accent wash by memo count level 0-4: light 0, 9, 16, 24, 32%; dark 0, 16, 26, 38, 52%. `MemosView.swift:340`, `MemosView.swift:401`, `MemosView.swift:415`
- [ ] Day number color: selected white on full accent; today accent, with a 1.5 pt accent ring; has memos `text`; none `faintIcon`. `MemosView.swift:405`, `MemosView.swift:416`, `MemosView.swift:417`
- [ ] Day number weight: selected or today semibold; memos medium; otherwise regular; monospaced digits. `MemosView.swift:412`
- [ ] Clicking a day with memos filters to it; again clears; an empty day does nothing. `MemosView.swift:407`
- [ ] Hovering a day with memos waits 350 ms then shows a card, following the pointer day to day without waiting; leaving hides it after 80 ms, fading 0.12 s. `MemosView.swift:439`
- [ ] Card: 250 wide, its right edge at the calendar's, 2 below the day; radius 11, `page` fill, 1 pt `text` at 8%, shadow black 14%, radius 20, y 10; padding 14 by 12; no pointer interaction. `MemosView.swift:336`, `MemosView.swift:427`, `MemosView.swift:430`, `MemosView.swift:540`
- [ ] Card content: long day label serif 13 pt semibold; "N memo(s)" 12 pt `faintText`; up to 5 memos, time italic 12 pt in `faintText` 36 wide, first nonblank line 12 pt tail-truncated; "N more" 12 pt `faintText` indented 46. `MemosView.swift:498`, `MemosView.swift:503`, `MemosView.swift:508`, `MemosView.swift:516`, `MemosView.swift:529`, `MemosView.swift:550`
- [ ] Footer: "Nothing yet this month", or "N memo(s) · M day(s)", 12 pt `faintText`, 10 above. `MemosView.swift:389`

### E4. Tags margin

- [ ] Under the calendar, a `text` at 9% hairline, then "Tags" in serif italic 13 pt `faintText`; only when any memo has tags. `MemosView.swift:270`, `MemosView.swift:274`
- [ ] Up to 12 tags by count, descending, then name; a tag counts once per memo. `MemosView.swift:255`
- [ ] Row, spacing 7: `#name` 12 pt accent, hover 75%, tail-truncated; count monospaced in `faintIcon` at the right. `MemosView.swift:278`, `MemosView.swift:300`, `MemosView.swift:307`
- [ ] Selected tag: semibold, accent underline; clicking toggles the filter; tooltip "Show memos tagged #x" or "Show all memos". `MemosView.swift:281`, `MemosView.swift:301`, `MemosView.swift:303`, `MemosView.swift:316`

### E5. Filter line

- [ ] Active filters show as words with an × above the list, spacing 16, 18 above: the long day label, and `#tag`. Day and tag filters combine. `MemosView.swift:38`, `MemosView.swift:114`
- [ ] Long day label: "Friday, October 9", plus the year when not this one. `MemosView.swift:143`
- [ ] Filter label: 12 pt `text2`, × 8.5 pt semibold `faintText`, hover darker; 24 tall, wash `text` at 5% hover and 8% pressed, radius 6; click clears; tooltip "Show all memos". `MemosView.swift:616`, `MemosView.swift:629`, `MemosView.swift:639`

### E6. Memo list

- [ ] Memos group by day, in a diary layout: a 40 wide date column right-aligned, 20 gap, 16 above and 4 below each group, a 1 pt `text` at 7% rule above all but the first, 12 pt spacing between memos in a day. `MemosView.swift:101`, `MemosView.swift:565`, `MemosView.swift:584`, `MemosView.swift:596`, `MemosView.swift:604`
- [ ] Date column: day number 20 pt in the editor's font, `text`; below, 4 apart, italic 12 pt `faintText`, `Fri`, or `Fri · Sep` in another month, or `Fri · Sep 25` in another year. `MemosView.swift:569`, `MemosView.swift:587`, `MemosView.swift:590`
- [ ] Row: time in italic 12 pt monospaced `faintText`; `…` menu appears at right on hover; 4 to the text; 14 below. `MemosView.swift:669`, `MemosView.swift:674`, `MemosView.swift:692`
- [ ] Row text is selectable. `MemosView.swift:683`

### E7. Memo rendering

- [ ] Text at 15 pt in the editor's font; blocks 8 apart: paragraphs, list items, quotes, code. `MemosView.swift:32`, `MemosView.swift:788`, `MemosView.swift:792`
- [ ] Paragraph: line spacing 6, wrapping. `MemosView.swift:807`
- [ ] Inline: bold (in the bold color), italic, code and links via inline Markdown; CJK in the paired face. `MemosView.swift:754`, `MemoText.swift:20`
- [ ] `#tags` are accent runs, clicking one filters by it. `MemosView.swift:764`, `MemosView.swift:685`
- [ ] Other links open with the system. `MemosView.swift:686`
- [ ] List item: 22 slot, indented 22 x level. `MemosView.swift:789`, `MemosView.swift:825`
- [ ] Bullets 5 x 5, 4 from the slot's left, by `level % 3`: disc, 1.2 pt ring, square. `MemosView.swift:853`
- [ ] Ordered: label in the text font, monospaced digits, 6 trailing. `MemosView.swift:863`
- [ ] To-do: 15 x 15 radius 4, unchecked 1.5 pt `faintIcon`; checked accent fill with a white checkmark 8.5 pt heavy. Clicking toggles it in the file; tooltip "Mark as done" or "Mark as not done". `MemosView.swift:869`, `MemosView.swift:680`, `MemosView.swift:885`
- [ ] Checked item text: `text3` with `faintText` strikethrough. `MemosView.swift:816`
- [ ] Quote: `text2`, 14 indent, 2 pt capsule bar `faintIcon`. `MemosView.swift:826`
- [ ] Code: monospaced 12 pt, line spacing 3, padding 12 by 10, full width, radius 8, `codeBlock` fill. `MemosView.swift:834`

### E8. Editing a memo

- [ ] `…` button: ellipsis 12 pt semibold `icon`, 24 x 18, no indicator: Edit; Copy (plain text of the memo); Show in File (opens the day's file as a note); separator; Delete… (destructive). `MemosView.swift:702`, `MemosView.swift:717`
- [ ] Delete asks a confirmation dialog "Delete this memo?" with Delete. `MemosView.swift:696`
- [ ] Edit swaps the text for an editor in place: min 30, padding 8 by 6, radius 8, `sidebar` fill; focused; ⌘↩ saves, esc cancels. `MemosView.swift:731`, `MemosView.swift:735`
- [ ] Edit buttons right-aligned, spacing 8: Cancel (esc), Save (⌘↩): 12 pt, 26 tall, radius 7, padding 12, white with 0.5 pt black 12% and a shadow, in dark white 12%, hover 16%, stroke white 8%. `MemosView.swift:737`, `MemosView.swift:739`, `SettingsView.swift:986`

### E9. Quick capture

- [ ] Menu bar icon: memo glyph 18 pt template, tooltip "New Memo (⌃⌥N)"; installed at launch. `QuickCapture.swift:126`, `QuickCapture.swift:127`, `WindowCloseBehavior.swift:19`
- [ ] Left click toggles the panel. `QuickCapture.swift:136`
- [ ] Right click menu: New Memo; separator; Open Fumi (activates, brings forward the first window); Quit Fumi. `QuickCapture.swift:139`, `QuickCapture.swift:151`
- [ ] Global hotkey ⌃⌥N toggles the panel from any app, with no Accessibility permission (Carbon hot key). `QuickCapture.swift:111`
- [ ] File › New Memo opens the panel too, and it retakes focus after the menu closes. `FumiApp.swift:101`, `QuickCapture.swift:36`
- [ ] The panel never brings Fumi's windows forward and does not activate the app; takes keyboard focus; floating; appears on all Spaces and over full screen. `QuickCapture.swift:81`, `QuickCapture.swift:84`, `QuickCapture.swift:85`, `QuickCapture.swift:159`
- [ ] Position: centered on the screen with the pointer, its card a fifth of the way down the visible area. `QuickCapture.swift:100`
- [ ] Panel 600 x 420, transparent, with a card 560 wide inset 20, top aligned, growing with its text. `QuickCapture.swift:19`, `QuickCapture.swift:222`, `QuickCapture.swift:225`
- [ ] Card: radius 14, `page` fill, 1 pt `pageEdge`, shadow black 22%, radius 26, y 12; padding 18 sides, 16 top, 12 bottom; spacing 14. `QuickCapture.swift:205`, `QuickCapture.swift:209`
- [ ] The card's SwiftUI environment forces `colorScheme` to light, whatever Fumi's mode (see Z6). `QuickCapture.swift:226`
- [ ] Editor: 15 pt, placeholder "Write it down…", minimum 44, same highlighting as the composer. `QuickCapture.swift:181`, `MemoText.swift:49`
- [ ] Footer: left "Memos · Today" 12 pt `faintText`, or a message in `text2`; right "Save" 12 pt `text2` and a 24 pt circle, accent (22% when blank), white arrow.up 11 pt bold. `QuickCapture.swift:184`, `QuickCapture.swift:192`, `QuickCapture.swift:193`
- [ ] ⌘↩ or Save saves if not blank, clears the draft, hides the panel; the Save button is disabled when blank. `QuickCapture.swift:182`, `QuickCapture.swift:201`, `QuickCapture.swift:202`, `QuickCapture.swift:70`
- [ ] Saves to today's file in the last opened library (`fumi.workspacePath`), not the focused window's. `QuickCapture.swift:63`, `QuickCapture.swift:69`
- [ ] No library yet: message "Open a notebook in Fumi first."; a failed save: "Couldn't save: <reason>". Message resets on each show. `QuickCapture.swift:65`, `QuickCapture.swift:73`, `QuickCapture.swift:33`
- [ ] Esc, or a left or right click anywhere else in any app, puts it away and keeps the draft. `QuickCapture.swift:50`, `QuickCapture.swift:166`, `QuickCapture.swift:218`
- [ ] Shown again: focus in the editor, caret at the end. `QuickCapture.swift:39`, `MemoText.swift:217`

## F. Settings

Theme palette values are in the Design tokens table at the top of this file.

### F1. The sheet

- [ ] Opens with ⌘,, or the sidebar's Settings row as a toggle; it shows only with a library open, so ⌘, on the welcome page displays nothing until one opens. `FumiApp.swift:177`, `SidebarView.swift:928`, `AppShellView.swift:54`, `AppShellView.swift:134`
- [ ] Not a window or tabs: one sheet over the dimmed window, 560 wide, radius 14, as tall as its content, kept 40 from the window's top and bottom, at least 260 tall; past that the content scrolls. `SettingsView.swift:22`, `SettingsView.swift:23`, `SettingsView.swift:32`
- [ ] Reads top to bottom: title "Settings" (20 pt semibold), Appearance, Writing, Library, Publish, Keyboard shortcuts, then "Fumi <version>" 12 pt `faintText`. `SettingsView.swift:178`, `SettingsView.swift:186`, `SettingsView.swift:190`, `SettingsView.swift:193`
- [ ] Padding: 24 top, 28 sides, 20 bottom. `SettingsView.swift:126`, `SettingsView.swift:148`
- [ ] The title stays fixed; scrolling content fades over its last 32 pt, with the slim scroller. `SettingsView.swift:125`, `SettingsView.swift:132`, `SettingsView.swift:134`
- [ ] Group: bold 13 pt name, 32 tall, over rows; 12 above and below; one device-pixel hairline of `text` at 9% above all but the first. `SettingsView.swift:213`, `SettingsView.swift:220`, `SettingsView.swift:232`
- [ ] Row: at least 36 tall; 13 pt title left with optional 12 pt `faintText` detail, middle-truncated; control right. `SettingsView.swift:244`, `SettingsView.swift:261`
- [ ] Scrim: same as search, at 100%, or 45% in Glass; click on it closes; click on an empty spot of the sheet does not. `AppShellView.swift:136`, `SettingsView.swift:43`
- [ ] Esc or ⌘. closes whichever control has focus. `SettingsView.swift:48`, `SettingsView.swift:49`
- [ ] Appears and disappears with fade and scale from 0.97; 0.16 s ease-out. `AppShellView.swift:142`, `AppShellView.swift:152`
- [ ] Sheet shadow: black 16% in light, 45% in dark; radius 40, y 30. `SettingsView.swift:44`
- [ ] Sheet surface, other themes: `page`, 1 pt black 5% (white 7% in dark). `SettingsView.swift:90`
- [ ] Sheet surface, Glass on macOS 26: `page` at 60% (45% dark) under regular Liquid Glass, 1 pt white 75% (12% dark). `SettingsView.swift:72`
- [ ] Sheet surface, Glass before 26: within-window popover blur under `page` at 70% (55% dark), 1 pt white 90% (14% dark). `SettingsView.swift:79`
- [ ] Controls: switch small, in the accent. `SettingsView.swift:272`
- [ ] Pop-ups: 24 tall, radius 6, at least 120 wide, `inlineCode` fill plus `text` at 5% on hover or while open; 13 pt value, chevron.down 9 pt semibold `text3`; a system menu with the current item checked. `SettingsView.swift:311`, `SettingsView.swift:323`, `SettingsView.swift:288`, `SettingsView.swift:338`
- [ ] Buttons and text fields in the sheet share the same fill; text fields 24 tall, padding 8, default width 180. `SettingsView.swift:368`, `SettingsView.swift:384`

### F2. Appearance

- [ ] Theme, key `fumi.colorTheme`, default `white`, options White, Paper, Glass, Linen, Mist: reskins everything. `SettingsView.swift:454`, `AppSettings.swift:105`, `AppSettings.swift:109`, `ThemePalette.swift:6`
- [ ] Theme swatch, 18 x 13, radius 3: ground color with a 10 wide page on the right; Glass ground is a -45 degree gradient, light #E3EAF1 to #F5F2F7, dark #2B3038 to #302C35, page at 75%; 0.5 pt outline black 14% (white 16% in dark). `SettingsView.swift:401`
- [ ] Mode, key `fumi.appearanceMode`, default `light` (not auto), options Light, Dark, Match System: sets the app's appearance. `SettingsView.swift:458`, `AppSettings.swift:106`, `AppSettings.swift:114`, `ColorTokens.swift:106`
- [ ] Accent, key `fumi.accentColor`, default #1D6FC2: five dots with tooltips, Blue #1D6FC2, Ink #37352F, Green #448361, Purple #9065B0, Red #C4402F. `SettingsView.swift:436`, `AppSettings.swift:14`, `AppSettings.swift:119`
- [ ] Accent affects caret, text selection, checked to-dos, bold text, memo tags and links, search highlights, switches and native controls, Xiaohongshu cards. `WysiwygTextView.swift:299`, `CodeBlockLayoutManager.swift:51`, `WysiwygTextView.swift:769`, `MemoText.swift:174`, `SearchPopover.swift:349`, `FumiApp.swift:46`, `XhsCards.swift:123`
- [ ] Ink resolves to `text`, so it stays visible in dark; bold then uses `strongInk`. `ColorTokens.swift:73`, `CodeBlockLayoutManager.swift:18`
- [ ] Dots: 14 pt circle, selected ring 21 pt, 1.5 pt, in its own color; spacing 11 between 21 pt frames at -3.5 side padding; Ink's dot follows `text`. `SettingsView.swift:463`, `SettingsView.swift:483`, `SettingsView.swift:490`, `SettingsView.swift:493`, `SettingsView.swift:500`
- [ ] A value from the old build's `fumi.appTheme` key is removed once at launch, resetting the accent to default. `ColorTokens.swift:59`

### F3. Writing

- [ ] Typeface, key `fumi.editorFont`, default "System": a pop-up showing the name in its own face; opens a 250 x 340 popover. `SettingsView.swift:522`, `SettingsView.swift:638`, `SettingsView.swift:670`, `AppSettings.swift:19`, `AppSettings.swift:149`
- [ ] Font popover: "Search fonts" field 12 pt, 28 tall, radius 7, `text` at 4% fill, padding 8, case-insensitive filter; System, New York, SF Mono first, a hairline, then installed families sorted, hiding `.` families; each row in its own face 13 pt. `SettingsView.swift:614`, `SettingsView.swift:616`, `SettingsView.swift:648`, `SettingsView.swift:657`, `SettingsView.swift:680`
- [ ] Font row: padding 9 by 6, radius 6; selected accent 8% with an accent ✓; hover `cardHover`; click selects and closes. Popover background is `page`. `SettingsView.swift:685`, `SettingsView.swift:694`, `SettingsView.swift:699`, `SettingsView.swift:673`
- [ ] Typeface affects the editor, memos, tab titles, the toolbar title and PDF, plus "No notes here yet". `EditorView.swift:24`, `MemoText.swift:10`, `TabStrip.swift:12`, `PDFExporter.swift:23`, `EditorContainerView.swift:58`
- [ ] Size, key `fumi.editorFontSize`, default 16, range 12-24, step 1: − N + stepper 24 tall, 26 wide ends, radius 6, value monospaced at least 24 wide, disabled ends at 35% opacity; scales the editor's body, headings, small text and PDF. `SettingsView.swift:525`, `SettingsView.swift:555`, `SettingsView.swift:585`, `SettingsView.swift:594`, `AppSettings.swift:7`, `AppSettings.swift:67`
- [ ] Memos do not follow Size: memo text stays 15 pt. `MemosView.swift:788`, `MemoText.swift:47`
- [ ] Line spacing, key `fumi.editorLineHeight`, default 1.7: Compact 1.4, Cozy 1.7, Relaxed 2.0; shows the preset nearest the stored value; sets the body line in editor and PDF. `SettingsView.swift:528`, `AppSettings.swift:8`, `AppSettings.swift:75`, `AppSettings.swift:241`, `AppSettings.swift:250`
- [ ] Line length, key `fumi.editorWidth`, default `medium`: Narrow 600, Medium 680, Wide 860; also changes the window's minimum width. `SettingsView.swift:532`, `AppSettings.swift:20`, `AppSettings.swift:155`, `AppShellView.swift:20`
- [ ] Check spelling, key `fumi.spellCheck`, default off: continuous spell check in the editor, read when created for memo editors. `SettingsView.swift:536`, `AppSettings.swift:21`, `AppSettings.swift:163`, `WysiwygTextView.swift:150`, `MemoText.swift:67`
- [ ] Smart quotes and dashes, key `fumi.smartPunctuation`, default off: substitution in the editor, read when created for memo editors. `SettingsView.swift:539`, `AppSettings.swift:23`, `AppSettings.swift:164`, `WysiwygTextView.swift:153`, `MemoText.swift:65`
- [ ] Slash commands, key `fumi.slashCommands`, default on: `/` opens the slash menu. `SettingsView.swift:542`, `AppSettings.swift:24`, `AppSettings.swift:182`, `WysiwygTextView.swift:1617`

### F4. Library

- [ ] Row titled with the open folder's name ("No folder open" if none), its `~`-abbreviated path as detail. `SettingsView.swift:714`, `SettingsView.swift:718`, `SettingsView.swift:724`
- [ ] Show in Finder (with a folder) and Change…: the open-folder panel, then opens it in this window, leaving a draft in the active tab. `SettingsView.swift:727`, `SettingsView.swift:731`
- [ ] Open at login, no stored key; reads and sets the system login item; a failure shows "Couldn't change this: <reason>" as the row's detail and the switch shows the system's true state. `SettingsView.swift:711`, `SettingsView.swift:738`, `SettingsView.swift:743`, `SettingsView.swift:753`, `SettingsView.swift:755`

### F5. Publish

- [ ] WeChat Article, key `fumi.extensions.wechat`, default off: shows the preview button, menu items and Copy for WeChat. `SettingsView.swift:769`, `AppSettings.swift:34`, `AppSettings.swift:38`
- [ ] When on, a nested Theme row, key `fumi.wechat.theme`, default `black-red`: Black Red, Blueprint, Songti, Motion; indented 16, fades in. `SettingsView.swift:772`, `AppSettings.swift:35`, `SettingsView.swift:765`, `WeChatRenderer.swift:15`, `WeChatRenderer.swift:24`
- [ ] Xiaohongshu Cards, key `fumi.extensions.xiaohongshu`, default off: shows the preview button, menu items and card export. `SettingsView.swift:780`, `AppSettings.swift:44`, `AppSettings.swift:55`
- [ ] Nested when on, indented 16, defaults for every note: Template, key `fumi.xhs.template`, default `memo`: Memo, Grid, Post, Post Dark. `SettingsView.swift:794`, `SettingsView.swift:805`, `AppSettings.swift:45`, `XhsCards.swift:17`
- [ ] Size, key `fumi.xhs.ratio`, default `3:4`: 3:4, 1:1. `SettingsView.swift:795`, `SettingsView.swift:809`, `AppSettings.swift:46`, `XhsCards.swift:26`
- [ ] Avatar, key `fumi.xhs.avatar`, default empty: 24 pt circle, the picture or the name's first letter uppercased, light #6B6457 on #E4DFD4, dark #C9C9C6 on #34363B; Remove when set; Choose…. `SettingsView.swift:799`, `SettingsView.swift:813`, `SettingsView.swift:873`, `AppSettings.swift:50`
- [ ] Choose…: panel for PNG, JPEG, HEIC, WebP, TIFF, message "Choose a picture for your avatar on Post cards."; the picture is copied to Fumi's own support folder as `xhs-avatar-<time>.<ext>`, removing the old; beeps on failure. Remove deletes the copy. `SettingsView.swift:846`, `SettingsView.swift:865`
- [ ] Name, key `fumi.xhs.name`, default empty, placeholder the macOS full name; empty uses the full name on cards. `SettingsView.swift:822`, `AppSettings.swift:48`, `XhsCards.swift:108`
- [ ] Handle, key `fumi.xhs.handle`, default empty, placeholder "@name"; `@` added on cards if missing. `SettingsView.swift:825`, `AppSettings.swift:49`, `XhsCards.swift:104`
- [ ] Page numbers, key `fumi.xhs.pageNumbers`, default on. `SettingsView.swift:796`, `SettingsView.swift:828`, `AppSettings.swift:47`, `XhsCards.swift:119`
- [ ] End card, key `fumi.xhs.endCard`, default off; when on, nested End card line, key `fumi.xhs.endLine`, default empty, placeholder "One line to close with", 220 wide. `SettingsView.swift:800`, `SettingsView.swift:801`, `SettingsView.swift:831`, `SettingsView.swift:834`, `AppSettings.swift:51`, `AppSettings.swift:52`
- [ ] Nested rows open and close with a 0.2 s ease-out. `SettingsView.swift:770`, `SettingsView.swift:781`, `SettingsView.swift:832`
- [ ] Changing any of these redraws an open preview. `XhsPreviewPane.swift:52`

### F6. Keyboard shortcuts

- [ ] Folded by default, a chevron rotating 90 degrees, "Keyboard shortcuts" bold, 0.2 s; read-only, no rebinding; keys are plain faint text. `SettingsView.swift:935`, `SettingsView.swift:942`
- [ ] Notes: New Note ⌘N, New Window ⇧⌘N, Open Folder… ⌘O, Search Notes ⇧⌘F, Toggle Sidebar ⌥⌘S, Back ⌘[, Forward ⌘], Settings ⌘,. `SettingsView.swift:903`
- [ ] Tabs: New Tab ⌘T, Close Tab ⌘W, Reopen Closed Tab ⇧⌘T, Show Next Tab ⌃⇥, Show Previous Tab ⌃⇧⇥, Pin / Unpin Tab ⌥⌘P. `SettingsView.swift:913`
- [ ] Format: Bold ⌘B, Italic ⌘I, Insert Link ⌘K, Toggle To-do ⌘L. `SettingsView.swift:921`
- [ ] Publish: Show Preview ⌃⌘P, Copy for WeChat / Add Cards to Photos ⌥⌘C. `SettingsView.swift:927`
- [ ] Layout: group label 12 pt `faintText`, 8 above; rows 28 tall, name 13 pt, keys 13 pt `faintText`; indented 16. `SettingsView.swift:956`, `SettingsView.swift:971`, `SettingsView.swift:976`

### F7. Other stored preferences (no row in the sheet)

- [ ] `fumi.sortMode`, default `updated-desc`: sidebar sort, set from Folder Options. `AppSettings.swift:80`, `AppModel.swift:204`
- [ ] `fumi.workspacePath`, default empty: the last library, reopened at launch and used by quick capture. `AppSettings.swift:15`, `WindowCloseBehavior.swift:93`, `QuickCapture.swift:63`
- [ ] `fumi.recentWorkspaces`: up to 5, most recent first, seeded from the last folder; welcome page's Recent list and the libraries seed. `AppSettings.swift:134`, `AppSettings.swift:143`, `Libraries.swift:7`
- [ ] `fumi.sidebarWidth.<path>`: sidebar width per library. `SidebarView.swift:1133`
- [ ] `fumi.tabs`: per library, tabs and active index. `NoteTabs.swift:33`
- [ ] `fumi.publish.targets`: per note, and `""` for last used, WeChat or Xiaohongshu when both are on; default WeChat. `AppModel.swift:107`, `AppModel.swift:116`
- [ ] `fumi.xhs.note.<id>`: per note, template, ratio, edited title and caption, and left-out blocks; removed when back to defaults. `XhsCards.swift:47`, `XhsCards.swift:55`

## G. Publishing and export

### G1. Preview pane

- [ ] Only with an extension on; toggled by the toolbar's phone button, More menu, or ⌃⌘P. `AppShellView.swift:27`, `FumiApp.swift:158`, `EditorView.swift:350`
- [ ] Pane 420 wide; placement in C13. `XhsPreviewPane.swift:11`
- [ ] One toolbar, 44 tall, padding 16 left and 12 right, with a 1 pt `text` at 6% rule under it. `PublishParts.swift:14`
- [ ] Platform names left, 13 pt semibold, spacing 14: with both extensions on, "WeChat" and "Xiaohongshu" are a switch: selected `text`, others `faintText`, hover `text2`, 0.15 s. `PublishParts.swift:36`, `PublishParts.swift:58`, `PublishParts.swift:63`
- [ ] With one extension on, only its name, not a button. `PublishParts.swift:38`, `PublishParts.swift:71`
- [ ] With both on, each note keeps the side last chosen for it; new notes take the last used. `AppModel.swift:111`, `AppModel.swift:121`
- [ ] Pop-up button: 12 pt `text2`, chevron.down 8 pt semibold `icon`, 24 tall, radius 6, `inlineCode` fill on hover or open; menu opens 4 below, left edges aligned. `PublishParts.swift:91`, `PublishParts.swift:239`
- [ ] Main button: 12 pt medium `text`, padding 11, 24 tall, radius 6, white (white 10% in dark), 1 pt `controlBorder`, a slight shadow in light; pressed `inlineCode`; disabled 45%. `PublishParts.swift:122`
- [ ] Notice under the bar: a 6 pt amber dot (light #D99A1E, dark #E2B04A), a 12 pt medium header "1 thing to check before ..." or "N things to check before ...", ending "pasting" for WeChat and "posting" for Xiaohongshu, "Hide" at the right; items 12 pt `text3`, indented 13; line spacing 2; padding 16, 9 above, 10 below. `PublishParts.swift:147`, `PublishParts.swift:153`, `PublishParts.swift:158`, `PublishParts.swift:162`, `PublishParts.swift:172`, `PublishParts.swift:179`, `WeChatPreviewPane.swift:55`, `XhsPreviewPane.swift:154`
- [ ] Hide folds the notice into an amber dot in the bar, 18 x 24, tooltip "Show what to check"; new warnings show again. `PublishParts.swift:190`, `WeChatPreviewPane.swift:33`, `XhsPreviewPane.swift:104`
- [ ] The preview redraws when a note opens and when typing pauses for 300 ms. `AppModel.swift:84`, `EditorView.swift:175`

### G2. WeChat preview

- [ ] Theme pop-up: an 8 pt dot in the theme's accent, then its name; tooltip "Theme"; menu items with a 20 x 14 page swatch and a check. `WeChatPreviewPane.swift:37`, `WeChatPreviewPane.swift:66`, `PublishParts.swift:257`
- [ ] Theme accents: Black Red #8F1D22, Blueprint #2455D6, Songti #BD3038, Motion #435924. `PublishParts.swift:250`
- [ ] Layout is the bundled renderer's (Resources/WeChat), four fixed themes keyed `black-red`, `blueprint`, `songti-supplement`, `motion-journal`; every style inline in the output. `WeChatRenderer.swift:5`, `WeChatRenderer.swift:15`
- [ ] Copy button: "Copy", tooltip "Copy for WeChat (⌥⌘C)"; shows a check and "Copied" for 1.2 s; disabled with no article or on failure. `WeChatPreviewPane.swift:41`, `WeChatPreviewPane.swift:48`, `WeChatPreviewPane.swift:95`
- [ ] Article at phone width, 375 wide, centered, in a web view: body padding 22 top, 48 bottom. On a dark sheet it sits on a white page, radius 10, 14 top. `WeChatPreviewPane.swift:206`, `WeChatPreviewPane.swift:207`, `WeChatPreviewPane.swift:208`, `WeChatPreviewPane.swift:209`
- [ ] Title at the head from the note's leading `# H1` (left out of the body), then the front matter title, then the file name: 20 px, line height 1.45, weight 600, #1D1D1F, 0.2 px tracking, margin 20; hidden if empty. `WeChatRenderer.swift:35`, `WeChatPreviewPane.swift:210`, `WeChatPreviewPane.swift:215`
- [ ] Hovering the title shows a faint fill (black 3.5%, radius 8) and "Copy title" at bottom right; it copies the title as plain text, shows "Copied" for 1.2 s. `WeChatPreviewPane.swift:212`, `WeChatPreviewPane.swift:216`, `WeChatPreviewPane.swift:227`, `WeChatPreviewPane.swift:301`
- [ ] Updates swap the article in place, so the scroll stays put. `WeChatPreviewPane.swift:276`
- [ ] Links in the preview open in the browser, never inside it. `WeChatPreviewPane.swift:310`
- [ ] Failure: header "This note can’t be laid out", the reason as an item, article dimmed to 35%. `WeChatPreviewPane.swift:53`, `WeChatPreviewPane.swift:61`
- [ ] Missing renderer message: "The WeChat renderer is missing from this build." `WeChatPreviewPane.swift:119`
- [ ] Renderer warnings are mapped to English, per pattern: image not found; only PNG, JPEG, GIF, WebP; unusable image address; invalid image path; front matter with no closing `---`. `WeChatPreviewPane.swift:171`
- [ ] Warning mapping continued: video not on this Mac; video placeholders to add in the WeChat editor; animated diagrams to check; `http://` images; embedded images; images on this Mac. `WeChatPreviewPane.swift:176`
- [ ] Warning mapping continued: non-web links; `[[Links]]` and `![[embeds]]` as plain text; footnotes as plain text; math as plain text; HTML as text; front matter title differs from the first heading. `WeChatPreviewPane.swift:182`
- [ ] A message with no mapping shows as is. `WeChatPreviewPane.swift:167`
- [ ] Before rendering: `![[image or video]]` becomes `![](path)` via the editor's lookup; other wiki links keep only their words (alias, or `Note › Heading`); code stays as it is. `WeChatRenderer.swift:107`, `WeChatPreviewPane.swift:114`
- [ ] Image extensions png jpg jpeg gif webp svg bmp heic avif and video extensions mp4 mov m4v webm avi mkv count as embeds here. `WeChatRenderer.swift:99`, `WeChatRenderer.swift:100`
- [ ] A line holding only an image or video gets blank lines around it, except when indented or in a fence. `WeChatRenderer.swift:160`

### G3. WeChat clipboard

- [ ] Copy puts the article HTML on the pasteboard as `.html`, and the note's Markdown as `.string`. `WeChatPreviewPane.swift:127`
- [ ] ⌥⌘C or Copy for WeChat copies the open note in the chosen theme without the preview, as typed even if unsaved. `FumiApp.swift:163`, `WeChatPreviewPane.swift:142`
- [ ] If it can't be laid out: beep and open the preview, which says why. `WeChatPreviewPane.swift:152`

### G4. Xiaohongshu preview

- [ ] Order: the cards, three across, in a web view as tall as they are (minimum 160); then Title; then Caption; the pane scrolls them together. `XhsPreviewPane.swift:62`, `XhsPreviewPane.swift:69`, `XhsPreviewPane.swift:70`
- [ ] Card layout, templates and pagination come from the bundled `cards.js`, in preview mode, re-laid out on new input or width; one layout at a time; laid out again when images arrive. `XhsCards.swift:5`, `XhsPreviewPane.swift:417`, `XhsPreviewPane.swift:475`, `XhsPreviewPane.swift:510`
- [ ] The cards use the app's accent. `XhsCards.swift:123`
- [ ] Template pop-up: a 9 x 12 paper glyph (Memo white with a #FFE070 top band, Post white, Grid #FBFBF8, Post Dark #1C1C1E), and "Memo · 3:4"; tooltip "Template and card size". `XhsPreviewPane.swift:105`, `XhsPreviewPane.swift:107`, `XhsPreviewPane.swift:333`
- [ ] Template menu: section "Template" (Memo, Grid, Post, Post Dark), separator, section "Card Size" (3:4 Portrait, 1:1 Square), current checked; choosing stays with this note; the Settings default clears the override. `XhsPreviewPane.swift:124`, `XhsPreviewPane.swift:129`
- [ ] Layout failure warning: "The cards couldn't be laid out: <detail>". `XhsPreviewPane.swift:489`
- [ ] Web view scroll goes to the pane; its own context menu is replaced; links never open. `XhsPreviewPane.swift:446`, `XhsPreviewPane.swift:449`, `XhsPreviewPane.swift:504`

### G5. Xiaohongshu post fields

- [ ] Title field: placeholder and default the deck's title; 15 pt semibold `text`; label "Title"; tooltip "The post's title. Changing it here doesn't change the note." `XhsPreviewPane.swift:180`, `XhsPreviewPane.swift:182`, `XhsPreviewPane.swift:189`
- [ ] Caption field: grows with its text, 13 pt, line spacing 4; label "Caption"; the pane's scroll carries it. `XhsPreviewPane.swift:191`, `XhsPreviewPane.swift:376`
- [ ] Each field: 12 pt `text3` label, count at right. Over limit (title 20, caption 1000) the count reads "N / limit" in light #C8701A, dark #E89A4A, tooltip "Xiaohongshu takes N characters at most"; else "Characters". `XhsPreviewPane.swift:180`, `XhsPreviewPane.swift:191`, `XhsPreviewPane.swift:363`, `XhsPreviewPane.swift:365`, `XhsPreviewPane.swift:366`
- [ ] "Reset" appears when edited, restoring the note's own. `XhsPreviewPane.swift:181`, `XhsPreviewPane.swift:192`, `XhsPreviewPane.swift:362`
- [ ] Edits stay with the note in preferences and never change the file. `XhsPreviewPane.swift:78`, `XhsPreviewPane.swift:175`
- [ ] Padding 22 sides, 18 above, 28 below, spacing 16. `XhsPreviewPane.swift:179`, `XhsPreviewPane.swift:199`

### G6. Xiaohongshu card menu

- [ ] Right-clicking a card: Copy Image (flash "Image copied."); Save Image… (panel, PNG, `NN.png`). `XhsPreviewPane.swift:293`, `XhsPreviewPane.swift:294`, `XhsPreviewPane.swift:298`
- [ ] If the card has blocks: separator, Leave Out of Cards. `XhsPreviewPane.swift:303`
- [ ] Left-out notice: "1 part left out of the cards." or "N parts ...", with a "Put Back" link. `XhsPreviewPane.swift:157`
- [ ] Copy Image puts PNG and TIFF on the pasteboard. `XhsCards.swift:216`

### G7. Xiaohongshu export

- [ ] Export button: "Export" with a chevron, "Exporting…" while working and disabled; tooltip "Export the cards (⌥⌘C adds them to Photos)". `XhsPreviewPane.swift:113`, `XhsPreviewPane.swift:118`, `XhsPreviewPane.swift:120`
- [ ] Export menu: "Add N Images to Photos" (⌥⌘C), AirDrop…, Save Images to Folder…, separator, Copy Caption, Copy Title. `XhsPreviewPane.swift:209`
- [ ] Add to Photos: requests add-only access, usage string "Fumi adds the Xiaohongshu cards you export to Photos, so they reach your phone."; adds in order with creation dates 1 s apart so they sort as they read; copies the caption; status "N images added to Photos. Caption copied." `XhsCards.swift:170`, `XhsPreviewPane.swift:256`, `Info.plist:26`
- [ ] Denied: "Fumi can't add to Photos. Allow it in System Settings › Privacy & Security › Photos." `XhsCards.swift:173`
- [ ] AirDrop: the system share; copies the caption; status "Caption copied."; unavailable "AirDrop isn't available on this Mac." `XhsCards.swift:186`, `XhsPreviewPane.swift:258`
- [ ] Save to folder: save panel "Save Cards", prompt "Save", message "The cards are saved as 01.png, 02.png, … in a folder with this name."; name safe from `/:\?%*|"<>`, up to 80 characters, "Cards" if empty; starts in `attachments/xhs`, else `attachments`, else the library; overwrites same names; reveals the folder; status "N images saved to Folder." `XhsCards.swift:195`, `XhsCards.swift:162`, `XhsPreviewPane.swift:267`, `XhsPreviewPane.swift:272`
- [ ] PNGs: exactly 1080 px wide, sRGB, height from the card's aspect, named `01.png`, `02.png`, rendered off screen, working at any display scale. `XhsCards.swift:33`, `XhsCards.swift:156`, `XhsCards.swift:372`, `XhsCards.swift:381`
- [ ] Copy Caption and Copy Title copy plain text, statuses "Caption copied." and "Title copied." `XhsPreviewPane.swift:221`, `XhsPreviewPane.swift:227`
- [ ] Status lines appear in the notice for 4 s, fading 0.15 in and 0.2 out, in `text`; errors show their description. `XhsPreviewPane.swift:276`, `XhsPreviewPane.swift:281`
- [ ] ⌥⌘C with the preview on Xiaohongshu runs Add to Photos for the open note; failure beeps and opens the preview with the reason, including "Open a note first." `FumiApp.swift:163`, `XhsCards.swift:226`, `XhsCards.swift:248`
- [ ] Other card errors: "The cards couldn't be laid out.", "The card couldn't be drawn.", "The card couldn't be saved as PNG." `XhsCards.swift:357`, `XhsCards.swift:385`, `XhsCards.swift:395`

### G8. PDF

- [ ] More › Export as PDF…: save panel "Export as PDF", prompt "Export", default name the file name with `/` and `:` as `-` ("Note" if empty) + `.pdf`, starting beside the note. `EditorView.swift:401`, `PDFExporter.swift:317`, `PDFExporter.swift:347`
- [ ] Exports the text as typed, even if unsaved. `EditorView.swift:219`
- [ ] A4 (595.28 x 841.89), margins 64 top, 60 sides, 60 bottom; body type follows the editor's font, size and line spacing. `PDFExporter.swift:21`, `PDFExporter.swift:22`, `PDFExporter.swift:23`
- [ ] Laid out with the editor's own engine and decorations, no caret, so no syntax shows, at the page's column width. `PDFExporter.swift:7`, `PDFExporter.swift:67`
- [ ] Always painted in the light palette, whatever the app's mode; uses the theme's light values and the current accent for bold. `PDFExporter.swift:65`, `PDFExporter.swift:150`
- [ ] A line never splits; one that does not fit goes to the next page whole; an empty note still gets one page. `PDFExporter.swift:106`, `PDFExporter.swift:125`
- [ ] Foot of each page, 22 below the text area: note name left, "N / total" right, 9 pt system `faintText`. `PDFExporter.swift:173`
- [ ] Copying text out of the PDF gives no Markdown marks: "Lists", not "## Lists". `PDFExporter.swift:201`
- [ ] Attachments taller than a page shrink to fit; images re-made sharply, others scaled. `PDFExporter.swift:232`, `PDFExporter.swift:251`
- [ ] Remote images are fetched first, each waited for up to 20 s. `PDFExporter.swift:266`, `PDFExporter.swift:294`
- [ ] PDF metadata: title the note's name, creator "Fumi". `PDFExporter.swift:84`
- [ ] Failure alert, warning style: "The PDF could not be exported", with "The note could not be laid out for printing." or the error; OK. `PDFExporter.swift:335`, `PDFExporter.swift:359`
- [ ] Nothing is opened or revealed after export. `PDFExporter.swift:339`

### G9. Export All Notes

- [ ] File › Export All Notes…: panel "Choose Export Folder", prompt "Export". `FumiApp.swift:150`, `SettingsView.swift:1022`
- [ ] One flat `.md` per note named from its title or file name, with `/\:*?"<>|` as `-`; "Untitled" if empty; contents as they are; same names overwrite each other; failures silent. `SettingsView.swift:1029`

### G10. Other clipboard

- [ ] File's Copy Markdown Link: plain `![](path)`. `FilePreviewView.swift:76`
- [ ] Memo's Copy: plain text. `MemosView.swift:707`

## H. App-level

### H1. Menus

- [ ] Only additions: the system's app, Edit, Window and Help menus stay as SwiftUI provides them; About is the system's. `FumiApp.swift:28`, `SettingsView.swift:10`
- [ ] App menu: Settings… ⌘,, replacing the default; disabled with no window. `FumiApp.swift:176`
- [ ] File: New Note ⌘N. `FumiApp.swift:95`
- [ ] File: New Tab ⌘T. `FumiApp.swift:97`
- [ ] File: New Memo, no menu shortcut (quick capture panel; ⌃⌥N is global). `FumiApp.swift:101`
- [ ] File: New Window ⇧⌘N. `FumiApp.swift:103`
- [ ] File: Open Folder… ⌘O: replaces the focused window's library, or opens a new window if none. `FumiApp.swift:108`, `FumiApp.swift:232`
- [ ] File: separator, Reopen Closed Tab ⇧⌘T, disabled with none. `FumiApp.swift:113`, `FumiApp.swift:117`
- [ ] File, import/export: Export All Notes…, no shortcut, disabled with no window. `FumiApp.swift:150`
- [ ] File, with an extension on: separator; Show Preview or Hide Preview ⌃⌘P, disabled with no note. `FumiApp.swift:156`, `FumiApp.swift:158`
- [ ] File, with an extension on: "Copy for WeChat" or "Add Xiaohongshu Cards to Photos" ⌥⌘C, following the preview's side; disabled with no note, or when WeChat is off and the side is not Xiaohongshu. `FumiApp.swift:86`, `FumiApp.swift:163`, `FumiApp.swift:172`
- [ ] View: Toggle Sidebar ⌥⌘S. `FumiApp.swift:183`
- [ ] View: Back ⌘[, disabled when the tab has none. `FumiApp.swift:185`
- [ ] View: Forward ⌘], disabled when the tab has none. `FumiApp.swift:188`
- [ ] View: Search Notes ⇧⌘F. `FumiApp.swift:191`
- [ ] View: separator; Show Libraries, no shortcut, disabled with no open library. `FumiApp.swift:196`, `FumiApp.swift:199`
- [ ] View: Go to Library ▸ first nine libraries in list order, ⌃⌘1-⌃⌘9. `FumiApp.swift:201`
- [ ] Window: separator; Show Next Tab ⌃⇥; Show Previous Tab ⌃⇧⇥; both disabled with one tab. `FumiApp.swift:132`, `FumiApp.swift:133`, `FumiApp.swift:136`
- [ ] Window: Pin Tab or Unpin Tab ⌥⌘P, disabled with one tab. `FumiApp.swift:139`
- [ ] Format: Bold ⌘B; Italic ⌘I; Insert Link ⌘K; Toggle Todo ⌘L. `FumiApp.swift:214`
- [ ] Without a menu item: ⌘1-⌘9, ⇧⌘], ⇧⌘[ for tabs. `AppShellView.swift:219`
- [ ] ⌘W: system Close, routed to the active tab, or the window on its last tab. `WindowCloseBehavior.swift:123`

### H2. Dock and menu bar

- [ ] Dock icon click with no visible window restores hidden windows. `WindowCloseBehavior.swift:27`
- [ ] Menu bar item, see E9; it installs at launch and has no Settings row to hide it. `WindowCloseBehavior.swift:19`, `QuickCapture.swift:124`
- [ ] App icon from the bundle's `AppIcon`, also shown 64 x 64 on the welcome page. `FolderOpenView.swift:43`

### H3. URL schemes and file associations

- [ ] The app registers no custom URL scheme or document type: `Info.plist` has neither, so `.md` files do not open with Fumi by association. `Info.plist:4`
- [ ] Internal only, not registered: `fumi-tag://<tag>` for memo tag clicks, `fumi-xhs://app/...` and `fumi-xhs://local/...` inside the card web view. `MemosView.swift:686`, `MemosView.swift:767`, `XhsCards.swift:266`

### H4. Launch and platform

- [ ] Display name Fumi; minimum macOS from the build setting (14.0 in `project.yml`); automatic and sudden termination off. `Info.plist:8`, `Info.plist:24`, `Info.plist:27`
- [ ] Launch applies the saved appearance mode. `FumiApp.swift:21`
- [ ] Liquid Glass paths apply on macOS 26; 15 and up clears the window background in Glass; earlier systems fall back as noted. `AppShellView.swift:252`, `AppShellView.swift:295`, `EditorView.swift:458`

### H5. First run: welcome page

- [ ] With no folder open: a page of `page` color to the window's top, no toolbar strip, empty title; fades in over 0.2 s. `AppShellView.swift:54`, `FolderOpenView.swift:27`, `FolderOpenView.swift:30`, `FolderOpenView.swift:32`, `FolderOpenView.swift:35`
- [ ] A group centered with 48 bottom padding, so a little above center: 64 x 64 icon; 16 below "Fumi" 20 pt semibold `text`; 4 below "Your notes, as plain files." 13 pt `text3`. `FolderOpenView.swift:45`, `FolderOpenView.swift:47`, `FolderOpenView.swift:51`, `FolderOpenView.swift:86`
- [ ] 28 below: "Open Folder…" button, folder glyph, 13 pt medium, key caps ⌘ O; 28 tall, radius 7, padding 10; `text` at 5% on hover, 9% pressed, 0.12 s; ↩ activates it. `FolderOpenView.swift:56`, `FolderOpenView.swift:62`, `FolderOpenView.swift:69`, `FolderOpenView.swift:70`, `FolderOpenView.swift:250`
- [ ] Key caps: 12 pt `text3`, at least 18 x 18, radius 4, `inlineCode` fill, spacing 2. `FolderOpenView.swift:222`
- [ ] 40 below, only if any: "Recent" 12 pt semibold `faintText`, then existing folders, up to 5, most recent first; 320 wide, rows 30 tall: folder glyph, name 13 pt, parent's Finder display name 12 pt `faintText` at the right. Click opens. `FolderOpenView.swift:73`, `FolderOpenView.swift:76`, `FolderOpenView.swift:179`, `FolderOpenView.swift:162`, `FolderOpenView.swift:204`
- [ ] No onboarding, sample notes or tour: first launch shows only this. `FolderOpenView.swift:72`

### H6. Missing library page

- [ ] When the library is gone at launch or on entering it: a panel 420 wide, left aligned. `FolderOpenView.swift:17`, `FolderOpenView.swift:359`
- [ ] 44 pt tile, radius 11, `inlineCode` fill: questionmark.folder, or externaldrive when on an unmounted volume, 19 pt light `icon`; 20 below. `FolderOpenView.swift:300`
- [ ] Title in New York 20 pt semibold: "Can’t find “X”" or "“X” is on a disk that isn’t connected". `FolderOpenView.swift:307`
- [ ] Breadcrumb 12 pt `faintText`: last three path parts by " › ", `~` trimmed, iCloud shown as "iCloud Drive", `/Volumes/` trimmed. `FolderOpenView.swift:282`, `FolderOpenView.swift:313`
- [ ] Line 13 pt `text3`: "It may have been moved, renamed or deleted." or "It opens once “Disk” is connected." `FolderOpenView.swift:317`
- [ ] Unmounted disk: a 6 pt #E0A93B dot with a 12 pt 18% halo, "Waiting for the disk…" 12 pt `faintText`. `FolderOpenView.swift:322`
- [ ] When a volume mounts and the folder exists, it opens by itself. `FolderOpenView.swift:360`
- [ ] Actions, 28 below, spacing 24: "Locate…" with an arrow that nudges 2 on hover, underlined, ↩, not shown for a disk; panel starts at the nearest existing ancestor. `FolderOpenView.swift:335`, `FolderOpenView.swift:342`, `FolderOpenView.swift:350`, `FolderOpenView.swift:98`, `FolderOpenView.swift:137`
- [ ] Quiet actions, 13 pt `text3`, hover `text`: "Open Another Folder…" and "New Notebook". `FolderOpenView.swift:354`, `FolderOpenView.swift:365`
- [ ] New Notebook makes or reuses `~/Documents/Fumi` and opens a first blank note. `FolderOpenView.swift:103`
- [ ] Its errors: "A file named Fumi is already in Documents." and "Couldn't create the folder: <reason>". `FolderOpenView.swift:112`, `FolderOpenView.swift:116`

### H7. Updates

- [ ] No update UI or check exists; version is shown only at Settings' foot and About. `SettingsView.swift:172`, `SettingsView.swift:193`

### H8. External edits

- [ ] An open file changed on disk with no local edits comes in place: the caret stays on its text, its line stays where it was in the window, as one undoable edit. No alert. `EditorView.swift:136`, `EditorView.swift:145`, `WysiwygTextView.swift:1085`
- [ ] Refused while composing: then the text is replaced outright. `EditorView.swift:146`, `WysiwygTextView.swift:1086`
- [ ] Local edits plus an outside change: saved at once, merging; the merge appears on screen. `EditorView.swift:154`, `EditorView.swift:257`
- [ ] The same paragraph changed both ways: the note keeps what was typed here, and a sheet says: "This note was also changed outside Fumi" / "Some paragraphs were changed both here and outside. The note keeps your version; the outside version was saved as “name” in the same folder." with OK and Open Copy; a sheet on the key window, or a modal. `EditorView.swift:270`, `EditorView.swift:272`, `EditorView.swift:275`, `EditorView.swift:280`
- [ ] Open Copy opens the copy in a new tab. `EditorView.swift:278`
- [ ] Notes or files removed outside close their tabs. `AppShellView.swift:171`

### H9. Errors and confirmations, in one place

- [ ] Destructive without a prompt: Delete note, from the tree or More menu; Move to Trash, for files. `SidebarView.swift:672`, `EditorView.swift:414`, `SidebarView.swift:757`, `FilePreviewView.swift:45`
- [ ] Destructive with a prompt: Delete memo only. `MemosView.swift:696`
- [ ] Beeps rather than alerts: a failed Move to Trash, moving a folder into itself, avatar copy failure, Copy for WeChat failure, Cards failure. `FilePreviewView.swift:89`, `NoteTreeState.swift:96`, `SettingsView.swift:861`, `WeChatPreviewPane.swift:154`, `XhsCards.swift:254`
- [ ] Modal alerts: Rename Note, Rename Library, Change Icon, Couldn't Move, conflict sheet, PDF failure. `SidebarView.swift:677`, `LibrariesLevel.swift:187`, `LibrariesLevel.swift:194`, `SidebarView.swift:63`, `EditorView.swift:271`, `PDFExporter.swift:357`
- [ ] All interface text is English, sized 12, 13, 15 or 20 by the `TextSize` scale, with exceptions at 11 and below for footers, hints and icons. `ColorTokens.swift:226`, `SlashMenu.swift:244`, `WysiwygTextView.swift:1509`, `PDFExporter.swift:178`

## Z. Found in code but apparently unused

Not requirements. Each was checked by a repo-wide search; nothing outside its definition reads it.

### Z1. Preference keys nothing reads

- [ ] `fumi.autoPair`, default true: no pairing code exists. `AppSettings.swift:22`, `AppSettings.swift:172`
- [ ] `fumi.markdownSyntax`, default `focus`, values always, focus, hidden: reveal behavior is fixed. `AppSettings.swift:25`, `AppSettings.swift:191`
- [ ] `fumi.tabSize`, default 4. `AppSettings.swift:26`, `AppSettings.swift:196`
- [ ] `fumi.wordWrap`, default true. `AppSettings.swift:27`, `AppSettings.swift:201`
- [ ] `fumi.typewriter`, default false. `AppSettings.swift:28`, `AppSettings.swift:206`
- [ ] `fumi.exportFormat`, default `markdown`. `AppSettings.swift:29`, `AppSettings.swift:214`
- [ ] `fumi.includeFrontmatter`, default true. `AppSettings.swift:30`, `AppSettings.swift:219`
- [ ] `fumi.preserveTags`, default true. `AppSettings.swift:31`, `AppSettings.swift:224`
- [ ] `fumi.imageHandling`, default `copy`. `AppSettings.swift:32`, `AppSettings.swift:230`
- [ ] `fumi.autoUpdates`, default true: there is no updater. `AppSettings.swift:13`, `AppSettings.swift:100`
- [ ] `fumi.launchAtLogin`: Open at login reads the system service instead. `AppSettings.swift:12`, `AppSettings.swift:95`
- [ ] `fumi.noteListWidth`, default 300: there is no note list column. `AppSettings.swift:11`, `AppSettings.swift:90`
- [ ] `fumi.sidebarWidth` (no suffix), default 220: per-library keys are used. `AppSettings.swift:10`, `AppSettings.swift:85`
- [ ] `fumi.theme`, default `system`, and the `ThemePreference` enum: superseded by `fumi.colorTheme` and `fumi.appearanceMode`. `AppSettings.swift:6`, `AppSettings.swift:63`, `AppSettings.swift:271`

### Z2. Types and functions with no callers

- [ ] `DataController` and SwiftData `NoteModel`: create, search, pin, trash, restore and 30-day trash cleanup. Never instantiated; Fumi has no in-app Trash. `DataController.swift:9`, `DataController.swift:117`
- [ ] `TagSearch`, flat tag filtering for a sidebar tag search that no longer exists. `TagSearch.swift:7`
- [ ] `NoteCardSummary`, a note list card's preview line. `NoteCardSummary.swift:12`
- [ ] Notification names `newNote`, `formatBold`, `formatItalic`, `formatLink`, `toggleTodo`. `FumiApp.swift:243`
- [ ] `ImageLoader.clearCache` and `diskCacheCapacity`. `ImageAttachment.swift:201`, `ImageAttachment.swift:485`
- [ ] `ThemePalette.luminance` and `contrast`, used only by tests. `ThemePalette.swift:182`, `ThemePalette.swift:191`
- [ ] `NoteFolder.itemCount`. `NoteTree.swift:20`
- [ ] `restyleCount` and `engineBuildCount`, test instrumentation. `WysiwygTextView.swift:356`
- [ ] `SidebarRow.isEmphasized`: never set to true. `SidebarView.swift:427`
- [ ] `ShortcutHint.boxed`: never set to true. `SidebarView.swift:364`
- [ ] `Diag` file logging to `~/fumi_debug.log`: compiled off (`enabled = false`). `WysiwygTextView.swift:8`

### Z3. Tokens and glyphs nothing draws

- [ ] `Color.card` and palette key `card`. `ColorTokens.swift:163`
- [ ] `Color.selectedBg`, `Palette.selectedBg` and palette key `selected`. `ColorTokens.swift:30`, `ColorTokens.swift:160`
- [ ] `Color.appBg`. `ColorTokens.swift:146`
- [ ] `Color.controlFill` (#767680 at 13%). `ColorTokens.swift:177`
- [ ] Glyphs `back` and `forward`: Back and Forward have no toolbar buttons, only menu items and shortcuts. `ShiyeIcon.swift:157`, `ShiyeIcon.swift:160`

### Z4. Unreachable or stale

- [ ] `case "quad".first!` (the letter q) in the LaTeX escape switch: that branch only sees non-letters. `LatexMath.swift:149`
- [ ] Comments promise what code does not: "the rail's search button", no such button exists; "double-clicking" a table, any caret placement opens it. `SearchPopover.swift:4`, `MarkdownTable.swift:203`

### Z5. Live, but diagnostic: decide whether to port

- [ ] Image load failures are always appended to `~/fumi_image_debug.log` as well as stderr. `ImageAttachment.swift:508`

### Z6. Ambiguous: needs a decision or a check on a running build

- [ ] ⌘B, ⌘I, ⌘K and ⌘L edit the text storage directly, without `shouldChangeText`, so they may bypass undo; the checkbox click does go through it. Keep or fix? `WysiwygTextView.swift:1755`, `WysiwygTextView.swift:1783`, `WysiwygTextView.swift:1271`
- [ ] Commands named Toggle (`toggleBold`, `toggleItalic`) only ever add markers. Keep or fix? `WysiwygTextView.swift:1743`
- [ ] New items are added after the default New Item group, so SwiftUI's own New Window may also claim ⌘N; only a running build shows it. `FumiApp.swift:92`
- [ ] Folder Options' Open in New Window opens a blank window, while the Libraries level's opens that library. Keep or fix? `SidebarView.swift:245`, `LibrariesLevel.swift:33`
- [ ] Whether the system Edit menu's Find items do anything in the editor is not decided by this source. `WysiwygTextView.swift:406`
- [ ] Quick capture forces SwiftUI's `colorScheme` to light, while the memo editor's AppKit colors follow the app's appearance; check for a mismatch in dark mode. `QuickCapture.swift:226`, `MemoText.swift:123`
- [ ] The single-line restyle check treats `----` as a rule, while rendering requires exactly three characters, so `----` draws as text. `WysiwygEngine.swift:858`, `WysiwygEngine.swift:1273`
- [ ] Xiaohongshu card layouts and WeChat themes live in `Resources/Xiaohongshu` and `Resources/WeChat` (JS and CSS), outside this inventory's reading list; G points at them rather than itemizing them. `XhsCards.swift:6`, `WeChatRenderer.swift:6`

### Z7. Asked for in the brief, not present in the source

No code for these was found, so they are not requirements. Searches: `insertNewline`, `insertTab`, `deleteBackward`, `doCommand`, `usesFindBar`, `usesFindPanel`, `performFindPanelAction`, `menu(for`, `CFBundleURLTypes`, `CFBundleDocumentTypes`, `Sparkle`, plus reading every listed file.

- Auto-pairing, smart list or quote continuation, and Tab or Backspace rules in the note editor (B2 states the resulting plain behavior).
- Wiki-link completion while typing `[[`.
- Find within a note: nothing in the listed files sets it up, and ⌘F is not bound; ⇧⌘F is library search.
- Code block copy buttons, and table cell editing UI.
- Image resize handles, and custom context menus on images, video or text in the editor. The `|300` in `![[a.png|300]]` is ignored.
- Custom URL schemes, file associations and in-app updates.
- A tag browser or a folder page.
