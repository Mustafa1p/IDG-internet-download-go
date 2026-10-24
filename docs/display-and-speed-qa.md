# IDG 1.4.5 display and speed checks

The window keeps the same logical layout across DPI scales, starts at 960 × 640 points, and supports resize/maximize. Usable saved geometry is preserved. Settings uses four tabs with no page scroll container; Save and Reset remain visible at 800 × 500 points.

`python tests/display-settings-preview.py` renders the real debug app in isolated preview mode. It checks all four Settings tabs at minimum size, limited/manual controls, 100/125/150/200% rendering scales, 1920 × 1080, 2560 × 1440, and a 2K raster at 150%. Assertions check screenshot metadata, control bounds, header separation and Settings footer separation. These are egui render-scale checks; they do not change Windows display settings or replace testing physical monitor changes.

`python tests/fluent-interaction.py` exercises 42 real app interactions, including Settings tab switches, preserving a speed-limit draft across tabs, returning to Unlimited, search, filters, selection, pause/resume, and native minimize/restore and maximize/restore. Preview mode never applies settings to the user's account or starts fixture downloads.

The per-download speed limit defaults to Unlimited and applies to newly queued downloads. All HTTP segments share one pacing schedule. Media uses the configured rate with one fragment connection. Tests check shared timing across segments, exact output bytes, cancellation while limited, persistence, legacy settings, browser inheritance and media command arguments. Unlimited transfers bypass the pacing wait.

The download path batches small writes up to 256 KiB or 50 ms of incoming data, rather than taking the shared file lock for every packet. Progress uses at most about 41 time buckets rather than rescanning every chunk in a four-second window. Existing exact-byte, pause/resume, cancellation, POST-token and range-fallback regressions remain required. Actual speed depends on the source, network and disk; these changes do not promise constant peak throughput.

Validation commands: `cargo fmt --check`, x64/x86 `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked --bins`, the two UI scripts above, extension regressions, real Edge form smoke checks, signing regressions, installer-close smoke and release packaging/checksums.
