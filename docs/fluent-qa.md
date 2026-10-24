# Fluent redesign: functionality inventory and regression evidence

This records the pre-redesign baseline and backend checks for the native Rust
`eframe` / `egui` application. UI screenshot evidence and release validation are
recorded separately; a listed feature is not evidence that every UI interaction
was exercised.

## Preserved behavior inventory

| Area | Existing behavior to preserve |
| --- | --- |
| Create download | Manual URL, clipboard paste, inferred or edited filename, safe filename handling, destination folder picker, HTTP/HTTPS validation |
| Browser confirmation | Browser request queues a confirmation dialog; Start preserves captured headers; Cancel removes the pending request |
| Transfer engine | HEAD/range probing, server filename and redirect resolution, automatic or manual connections, range fallback, preallocation, retries and flush before completion |
| Authentication | Browser cookies, referer, user agent; restricted export headers for supported Hostinger mail exports |
| Generated exports | A single authenticated GET, exact EML/ZIP output, clear login/API errors, no unsafe replay of an interrupted generated export |
| Pause/resume | Pause flags suspend existing HTTP workers; resume continues those workers; external media downloads do not offer pause |
| Remove/cancel | Cancellation signals the worker and removes the queue item; downloaded files and partial files are not deleted |
| Completed items | Open completed file or parent folder; progress, status, size, speed, completion time and details remain available |
| Queue and history | Queue/history is session-only in the baseline; it is not serialized on restart. Existing files remain on disk |
| Search/filter/sort | Filename/URL search, status filters, media/document/archive/other categories, newest/oldest ordering |
| Media | YouTube canonicalization and inspection, quality selection, MP4/MKV/WebM and audio conversions through bundled tools; tool errors remain visible |
| Preferences | Download folder, automatic/manual parallel connections, startup, close-to-tray, app update and extension update choices; save/reset/cancel |
| Browser bridge | Local API on loopback port 17843, native messaging, browser handoff fallback, health/capabilities, extension heartbeat and synchronization |
| Desktop | Single-instance foregrounding, background startup, tray Show/Quit, close-to-tray, remembered usable geometry, repair of minimized 0-by-0 snapshots |
| Updates | Startup/hourly release check, What's new, release notes, download progress, checksum/size verification, safe installer arguments, deferral during active transfers |

Baseline limitations are not silently changed by this redesign: unfinished
transfers do not resume across process restarts, and history is not persisted.
The baseline browser API captures its default directory at startup; changing the
setting may require restart for browser-originated downloads. Manual downloads
use the current setting. These are not new persistence or networking features.

## Automated checks actually run

Before integrating the redesign:

- `cargo test --all-targets`: 28 passed, 0 failed, 3 ignored live-network tests.
- `node --test tests/extension.test.cjs`: 23 passed, 0 failed.
- `./tests/signing.test.ps1`: 10 signing boundary scenarios passed using mock
  signatures; this does not certify or sign the actual release binaries.

Existing local HTTP fixtures verified a 10,003-byte authenticated download byte
for byte, exact EML/ZIP export output, unknown-length streaming to EOF, server
filenames/redirects, and a 64 MiB deterministic payload using 32 ranges and the
single-connection fallback when a server ignores ranges. The updater fixture
also checks exact installer size and SHA-256 before installation.

After the test-only engine regression module was added,
`cargo test --all-targets` passed with 30 passed, 0 failed and 3 ignored
live-network tests. The new local throttled range server verified that a paused
transfer stops increasing its written-byte count, resumes to completion, and
produces the exact 262,163 expected bytes. Cancelling an initially paused worker
exited without hanging, reported Cancelled rather than Completed, and kept the
partial output file. These are real loopback HTTP and temporary-file tests; no
user download was used.

## UI and release verification boundaries

The debug-only actual-app interaction suite was run successfully after native
preview integration: **33 checks passed**, process exit code 0. Its report is
`target/fluent-qa/interactive.json`. It injects egui pointer/key input against
rectangles recorded by the real rendered controls, then observes application
state. It checks all nine category destinations, Add download open/close,
Settings navigation, grid/list switching, select all/deselect, bulk pause and
resume, removal confirmation/cancellation with all five preview fixtures
retained, sort settings, search focus/text/clear, and native maximize/restore.
The minimize check verifies `IsIconic` on the preview application's own attached
HWND before restoring it and verifying the restored viewport. These native
checks found and led to a guard for transient zero-sized minimized layouts.
The fixtures do not start real transfers or write user downloads.

The available native Computer Use connection failed because its native pipe
was unavailable. No external Windows UI automation fallback was used. This
suite validates rendered egui interaction and observed native window states;
it is not a substitute for manual assistive-technology, OS display-DPI, or
signed-in browser end-to-end testing. Sort checks verify the selected sort
setting; screenshot/table checks cover the presentation separately.

The actual application was built and rendered through `python tests/fluent-preview.py`.
All **19 cases passed** with exact expected framebuffer dimensions and verified
egui points/pixels-per-point metadata. Captures covered the empty dashboard at
800×500, 1280×720, 1366×768, 1600×900 and 1920×1080; 125%, 150% and 200% scaling
at 800×500; the 1540×930 reference layout; populated list, selection/details,
grid, minimum list, Settings at two sizes, Add download at two sizes, update
popup and removal confirmation. The 100% cases complete the four-scale matrix.

Screenshots were reviewed and iterated against the user's reference. Fixes from
that review include the clipped Close button, uneven card alignment, missing
menu-arrow glyph, small-window overflow, readable folder paths, balanced table
columns, and transient minimize sizing. Machine-readable evidence is committed
in `docs/fluent-render-matrix.json` and `docs/fluent-interaction-results.json`.
The populated screenshots use explicitly defined QA rows and do not claim to
show real user downloads or measured network speeds.

Development preview rendering scales can exercise egui layout at 100%, 125%,
150% and 200%. They are not equivalent to changing actual Windows display DPI;
native monitor-DPI transitions remain unverified. The interaction suite
separately verified native minimize/restore and maximize/restore, keyboard
shortcuts, and clean closure of the owned preview process. The custom Close
button uses the same viewport-close path and retains close-to-tray semantics;
Windows Snap Layouts' hover flyout is not implemented by the custom title bar.

The installed user's running IDG instance and downloads are not stopped or
modified for regression tests. Cold-start `native-smoke.py` and `tray-smoke.py`
require IDG closed and therefore are not run against that instance. The browser
extension suite is a mocked protocol regression suite, not a live signed-in
Chrome/Edge end-to-end session. Live YouTube and GitHub network tests remain
ignored unless explicitly run and reported.


## Reproduce on an interactive Windows desktop

```powershell
cargo build --bin IDG
python tests/fluent-preview.py
python tests/fluent-interaction.py
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --bins
node --test tests/extension.test.cjs
./tests/signing.test.ps1
cargo build --locked --release --bins
python tests/update-smoke.py
```

The graphical tests are development-only and isolate preferences, the instance
lock, API server, browser registration, startup registration and updater work
from the installed running app. The fixture mode does not start downloads.
Production download code in `src/download` is unchanged; the additions to
`engine.rs` are guarded by `#[cfg(test)]`.

## Visual differences and remaining checks

The layout, blue palette, sidebar, cards, toolbar, table, illustration and footer
follow the reference. Materials are local gradients and vector meshes rather
than actual background-dependent Mica/Acrylic. Fonts fall back to bundled egui
fonts when Segoe is unavailable. Rounded native corners use DWM on Windows 11
and gracefully fall back on Windows 10. The artwork is a bundled code-native
vector interpretation, so it is not a pixel-identical copy of the reference's
3D folder. Compact layouts intentionally reduce detail while preserving actions.

Real OS 100/125/150/200% display-setting changes, cross-monitor DPI transitions,
manual screen-reader testing, a live authenticated browser session and real
YouTube downloads are not claimed. Existing fixture/protocol and native state
coverage passed as described above. Release packages remain explicitly unsigned.

## Locked-window and x86 package refresh (v1.4.3)

The production window now overrides persisted sizes with the 1540×930 logical-pixel reference size, capped to the monitor. Native resize and maximize are disabled. Minimize, move, close/tray and normal restore remain available. Small/large render previews continue to exercise responsive fallback; native maximize tests run only in the isolated development self-test.

The Open folder button reserves additional width. Empty-state artwork is smaller and the artwork/text/action group is centered in the table body. The title-bar search and controls reserve enough trailing space.

Windows x86 is built with i686-pc-windows-msvc and includes 32-bit native bridge, yt-dlp, FFmpeg/FFprobe and Node.js v22.23.4. Every bundled EXE and DLL must match the selected PE machine type. Both update handover smoke tests and x86 tool version smoke checks pass on the Windows x64 test host; a physical 32-bit Windows installation is not available for testing.

## Compact window and workflow fixes (v1.4.3 refresh)

The current production size is **960 x 640 logical pixels**, replacing the earlier 1540 x 930 setting. Small details reserve room for Open folder/Open file; per-row Open folder is also visible.

The actual-app interaction harness now includes browser handoff while Settings is open and while search text would hide the download (35 checks). The server regression asserts that a real browser handoff queues the correct UI selection event. Saved history tests verify restart behavior, atomic file replacement/removal, partial-file preservation and exclusion of cookies/authorization headers. Interrupted workers are marked failed with an explanation; no cross-restart partial resume is claimed.

The isolated installer close smoke test compiles a WinForms fixture that hides/cancels Close, starts it from a temporary installation directory, then upgrades it with Inno Setup's force-close mode. The locked process exits and its binary is replaced successfully; the real installed IDG and user registry are untouched.

Same-version update regressions cover a newer build, the currently installed build, an older build, an older version, and a missing installer. `build.rs` watches Git HEAD/refs and embeds the source revision and commit timestamp; release publication writes the matching `IDG-BUILD` marker. An older binary without this logic requires one manual update.

The current renderer matrix has 21 cases, including the 960 x 640 list and same-version update prompt. Both Rust targets pass 32 non-network tests; 4 live/publication tests are intentionally opt-in. The extension suite passes 23 checks, signing mocks pass 10 scenarios, and the interaction harness passes 35 checks. Package creation rejects an executable that lacks the current Git source identity.

## Statistics icons and sidebar polish (v1.4.3 refresh)

The compact layout now shows four 56-point cards with colored speed/download/completed/clock icons. The sidebar uses the bounded panel height directly instead of a scroll container; its navigation and footer fit at the supported preview sizes without a navigation scrollbar. Folder paths stay on one clipped line with the full path available on hover. The empty-state Add download button/dropdown and minimal-height fallback action are removed; sidebar/header/keyboard download actions remain. Settings shows the repository hyperlink beside Developed by Mustafa Ali. The 21-case renderer matrix and 35 interaction checks pass.

The category-label overlap fix measures count text to size each badge, reserves a separate label area with a 10-point gap, and keeps the compact icon/text padding within the sidebar width. Long labels are limited to one line with the full label available on hover; badge height stays within its row. The actual 960 x 640 preview shows Videos & audio fully separated from its count. The renderer matrix and 35 interaction checks pass.
