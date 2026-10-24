IDG v1.4.5 fixes display sizing, header layout and Settings, with optional speed limits and less download overhead.

- Restore resizing and maximize, preserve usable window sizes, and respect native Windows display scaling instead of forcing a locked window on every launch.
- Reserve space for window controls and constrain the header search field so controls remain visible at compact sizes and on larger displays.
- Replace the scrolling Settings page with Downloads, App, Updates and Browser tabs. Keep Save and Reset visible, including at the minimum 800 × 500 logical size.
- Add a per-download speed limit in KiB/s, defaulting to Unlimited. The cap applies to new file and media downloads. File segments share one cap; limited media uses one fragment connection. Existing settings remain compatible.
- Batch small file writes and keep bounded speed samples to reduce per-chunk disk-lock and progress-calculation overhead. Use the elapsed startup interval for the speed display.
- Preserve token POST downloads, browser fallback, pause/resume, cancellation and exact file bytes.

Validation: Rust tests and Clippy on x64/x86; 42 real-app interaction checks; display render checks at 1080p, 2K and 100–200% egui scales, plus every Settings tab at minimum size; shared-limit timing, cancellation and exact-byte transfer checks; extension regressions and packaging checks. Render-scale checks do not change Windows display settings. Actual throughput depends on the source server, network and storage; no constant maximum speed is guaranteed.

Install the matching **IDG-Setup-v1.4.5.exe** (x64) or **IDG-Setup-v1.4.5-x86.exe** (x86). In Settings → Downloads, keep Unlimited selected for unrestricted transfers or choose Limit and Save changes before adding a new download.
