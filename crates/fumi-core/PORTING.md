# Porting Swift logic to fumi-core

Swift sources are in `~/bubble/notes-app/Fumi` and tests in `~/bubble/notes-app/Tests`. They are read-only.

- **Behavior is the spec.** Port every function the app uses, and every assertion in the matching Swift test as a Rust test, with the same inputs and expected values. Don't drop, weaken or merge cases. Keep a comment naming the Swift test file at the top of the test module.
- **A note's bytes are sacred.** Anything that rewrites a note must leave text it doesn't mean to change byte-identical, including CRLF, BOM, U+2028, unusual whitespace and no final newline.
- **Text offsets.** Swift often uses `NSString`/UTF-16 offsets; here offsets are UTF-8 byte offsets unless documented. Convert test expectations carefully and never slice inside a character. Where Swift iterates grapheme clusters (`Character`, `.count`), use `unicode-segmentation`; a plain `chars()` is not equivalent for emoji and combining marks. CJK must have its own test where the logic touches it.
- **Regexes.** ICU (`NSRegularExpression`) and Rust's `regex` differ: no lookaround or backreferences, and `\w`, `\b`, `\s` are Unicode-aware in both but not identical. Hand-written scanning is fine.
- **Foundation.** `trimmingCharacters(in: .whitespaces)` is not `str::trim`: `.whitespaces` excludes newlines. `.whitespacesAndNewlines` and `.newlines` have their own sets. `lastPathComponent`, `deletingPathExtension` and friends have edge cases; check them. `lowercased()` and `localizedCaseInsensitive*` differ from ASCII folding.
- **Not here.** No GPUI, AppKit, `objc`, or observers such as `@Observable`; state is plain data and callers react. Keep UI colors and fonts out.
- **Files.** Write only under a `tempfile` directory in tests. Never touch a real notes library.
- **When it can't match.** If Swift behavior can't or shouldn't be reproduced (including a bug you find in the Swift code), keep the closest correct behavior, and list it in your final report with file and line. Don't silently change it.
- **Dependencies.** Prefer what's in `Cargo.toml`. Add one only if it clearly earns its place, and name it in your report. Check its license is compatible with MIT (no GPL or AGPL).
- **Dates.** Keep them as seconds since the Unix epoch. No date crate: put small calendar helpers in the module that needs them, and take "now" and the UTC offset as arguments so tests are deterministic.
- **Cargo.** Run `cargo test -p fumi-core --offline`; three people are porting at once, so run only your module's tests while working (`cargo test -p fumi-core --offline <module>::`). crates.io is flaky from this machine through the proxy.
