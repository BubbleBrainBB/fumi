# fumi-gpui

A prototype of Fumi's Markdown editor on GPUI. See REPORT.md (in Chinese).

    cargo build --release && scripts/bundle.sh
    open -n target/FumiGPUI.app --args "$PWD/fixtures/visual.md"

Benchmarks: `scripts/matrix.sh`, `scripts/save_check.sh`, `python3 -I scripts/summary.py`.
Tests: `cargo test --release`.
