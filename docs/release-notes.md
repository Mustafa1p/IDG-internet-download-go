IDG v1.4.3 refresh fixes the compact desktop, browser transfer visibility, update detection and installer closing, with cleaner completed downloads and grid cards.

- Hide the selected-download details panel after completion while keeping the finished file in the library.
- Fix grid cards laying out all content on one line: use vertical metadata, consistent card widths and spacing, and truncate long source hosts.
- Place each download's folder icon in a dedicated column at the far right, after Added, instead of underneath its status. Use folder icons in compact details and the toolbar, with Open folder tooltips and accessible labels.
- Hide the sidebar download-folder path and its path tooltip; keep the Download folder action. Refresh the README screenshots and remove obsolete interface images.
- Fix sidebar category labels overlapping count badges: reserve a separate label area, keep a clear gap and size badges for their counts.
- Restore colored statistics icons, fit sidebar navigation without a scrollbar, and remove the empty-state Add download split button.
- Add GitHub source code beside Developed by Mustafa Ali in Settings.
- Reduce the locked desktop to 960 x 640 logical pixels, capped to the monitor work area, with a compact sidebar and table.
- Detect later builds of the same version using the published source identity and timestamp. Show an update popup with release fixes; avoid repeated offers of the installed build or downgrades.
- Browser handoffs leave Settings, clear search/category filters, select the incoming transfer and show IDG, including when confirmation is disabled.
- Restore visible Open folder actions on rows and in compact details; allow opening the default folder without a selection.
- Save completed/failed/interrupted download history across restarts without cookies or authorization headers. Interrupted workers are not resumed across a restart; entries explain how to restart and preserve partial files.
- Prevent Windows Setup from waiting on close-to-tray using Restart Manager force-close fallback and explicit launch control. The installer asks to close applications; running transfers stop when closed.
- Keep both Windows x64 and x86 installers/portable packages, native bridge and media tools.

**The version remains 1.4.3.** Install this refresh once manually if your installed build lacks build-aware update detection. Later refreshed 1.4.3 source builds can then be detected automatically. Older releases did not persist download history, so their previous session lists cannot be reconstructed.

Windows setup: **IDG-Setup-v1.4.3.exe** (x64) or **IDG-Setup-v1.4.3-x86.exe** (x86). Keep the native bridge and bundled media tools with portable IDG.

Validation: both Rust targets, same-version update/downgrade tests, browser handoff UI checks, history replacement/restart tests, and an isolated Inno upgrade against a fixture that hides when closed. Actual signed-in browser/media sessions and a physical 32-bit Windows installation are not certified by these checks.
