# v1.6.4 verification

Run from the repository root on Windows:

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --bins
node --test tests/extension.test.cjs
cargo build --locked --bins
python tests/media-dialog-smoke.py
python tests/fluent-interaction.py
node tests/media-overlay-browser-smoke.cjs
node tests/extension-browser-smoke.cjs
python tests/package-version.test.py
python tests/music-bridge.test.py
$env:IDG_MEDIA_TEST_URL='https://www.youtube.com/watch?v=KE0aEBW2syw'
cargo test --locked --bin IDG supplied_video_supports_selected_video_quality_and_mp3 -- --ignored --nocapture
```

The media dialog test clicks actual egui controls and checks state after MP3/MP4/480p changes and footer cancellation. It covers 800x500 at 1.0/1.25/1.5/2.0 scale and 1280x800 at 1.0. Reinstating the original zero-height sizing makes this test fail at the missing MP3 menu item. Reports are written under `target/fluent-qa`.

The real Edge media test checks hostile page CSS, natural overlay clicks, scrolling, removed controls, SPA navigation, visible handoff errors, homepage preview exclusion, 60 unrelated feed mutation batches without full-document rescans, and a closed-tab badge race.

Rust tests include a real subprocess fixture that fails with HTTP 403 on the first attempt, then verifies retained partial data and completion on re-extraction. Persistent 403 stops after two attempts. Existing local HTTP regressions cover interrupted streams, resume, exact bytes, authentication headers, rejected probes, one-use POST tokens and cancellation.

Live YouTube tests use bundled tools and probe actual MP4/MP3/WebM outputs for expected media streams and a 480p cap. Network success is dependent on source availability and is not a guarantee for restricted videos or all computers. Live social/account-dependent checks are separate ignored tests; they are not claimed by the normal suite.

The SVG paths reported by the user reference extension `omiebabagdcfedkhfmjlndcpocnafcpb`. This repository builds `blmjnmockbofpnipcniflmalfmiaolcj` and contains no `background/*.svg` assets. Its package must not request those resources; persistent errors from the other ID require checking that installed extension separately.
