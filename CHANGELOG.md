## 1.6.0 - 2026-10-30

- Prism-inspired transfer layout, status filters, readable rows, live speed overview and file inspector.
- Larger 1280 x 800 default and pending Windows per-monitor DPI fixes.
- Exact range/server-validator checks, browser-page rejection and durable completion.
- Atomic numbered duplicate filenames preserve existing files and simultaneous downloads.
- Include pending Spotify browser actions, metadata and overlay fixes; subscription audio remains experimental.
- Matching Windows x64/x86 installers, portable apps, browser extension and checksums.

## 1.4.3 — 2026-10-24

- Rebuild the existing Rust/egui interface around the supplied Windows 11 Fluent reference: blue sidebar, integrated search/title bar, separate live statistic cards, table and vector folder empty state.
- Add working list/grid views, date sorting, checkbox multi-selection, bulk transfer actions and remove-from-list confirmation that preserves files.
- Refresh Settings, download/media dialogs, details and update prompts; retain all engine, browser/native-host, startup/tray and updater behavior.
- Support custom native minimize/maximize/restore/close, draggable chrome and resize edges, with reduced-motion preference and responsive layouts.
- Add real desktop render checks across five window sizes and four rendering scales, debug-only interaction checks, and controlled pause/resume integrity coverage.

- Announce newer releases in a centered popup with release fixes, download progress, Update now, installation, and Later controls.
- Check for updates at startup and hourly even while IDG is hidden in the tray; announce each new version once per session.
- Publish unsigned Windows releases without Azure credentials by default; retain opt-in signing through `IDG_SIGN_RELEASE=true` and label unsigned downloads in release notes.

- Keep the last usable window size and position when IDG is minimized or hidden in the tray, preventing tiny windows at the screen edge on restart.
- Recover window settings already affected by minimized-window autosaves and preserve maximization when browser downloads or tray actions reopen IDG.

## 1.4.2 — 2026-10-22

- Support authenticated Hostinger Mail exports from Inbox and Sent, preserving repeated message-selection parameters.
- Download single-message EML files and server-generated ZIP archives through one streaming GET, without HEAD or range probes.
- Forward the export request's browser session and allowed authentication headers; recognize small EML downloads.
- Replace the fixed total download timeout with a read timeout so active exports can finish.
- Explain missing or expired mail sessions and reject sign-in pages or JSON API errors before saving an export.
- Flush buffered file writes before marking downloads complete.
- Add regression tests using an authenticated export server and a ZIP fixture containing EML messages.

## 1.4.1 — 2026-10-19

- Slim the page's Download with IDG overlay to 28px with smaller typography and padding.
- Add an accessible close button and Escape dismissal; keep the overlay hidden for that media until the page reloads.
- Show the overlay for different videos and prevent dismissed or navigated overlays from returning after an in-flight request finishes.
- Dismiss error notices with the overlay and preserve YouTube SPA routing.

## 1.4.0 — 2026-10-19

- Redesign the desktop library with a compact dark teal interface inspired by the supplied reference.
- Open at 960×600, support 800×500, and add live summaries, file-type categories, search/sorting, inline progress/actions, and selected-download details.
- Keep Mbps speed reporting and existing YouTube/update/startup functionality.
- Reduce the extension popup to connection status only; retain automatic browser handoffs, page overlays, and context-menu actions.
- Migrate the removed capture switch once so a stale disabled setting does not silently prevent capture.

## 1.3.1 — 2026-10-19

- Stop automatic update retries after an installer failure until the user explicitly retries, avoiding repeated download/restart cycles.
- Include all in-app updates, extension synchronization, YouTube format choices, Windows icon, and startup changes from 1.3.0.

## 1.3.0 — 2026-10-19

- Add verified in-app update downloads, installation/restart, optional automatic installation when idle, and periodic checks.
- Reject incomplete/checksum-mismatched update installers; defer browser handoffs during installation and keep queued/paused downloads from being interrupted.
- Detect and update IDG's existing unpacked Chrome/Edge extension; support a saved custom folder, version reporting, safe reloads, and an automatic-update setting.
- Add YouTube format/quality choices: MP4/MKV/WebM video with source resolution discovery and MP3/M4A/Opus/WAV/FLAC audio conversion.
- Embed Windows executable/installer icons, add a consistent taskbar AppUserModelID, and match shortcut identity.
- Enable tray startup on first portable run and preserve the user's later startup choice.
- Verify selected 480p MP4/WebM and MP3 conversion with the supplied live YouTube video.

## 1.2.1 — 2026-10-19

- Resolve redirected download filenames from server metadata instead of saving GitHub archives as main or main.zip.
- Share filename detection between the desktop dialog and browser API; decode URL and UTF-8 header filenames correctly.
- Preserve filenames explicitly edited by the user; test redirected downloads and exact output bytes.

## 1.2.0 — 2026-10-19

- Add YouTube watch-page, Shorts, and short-link downloads using bundled, checksum-verified yt-dlp, FFmpeg, and Deno.
- Download one selected video and merge video/audio into MP4; remove radio/playlist parameters.
- Route YouTube blob players through the page URL and add a download action in the extension popup.
- Automatically repair native-host registration when the full app starts; add a repair action in Settings.
- Show actionable connection errors and reject YouTube handoffs to older apps without media support.
- Correct the release badge and include media dependencies/licenses in installer and portable packages.

## 1.1.0 — 2026-10-19

- Select parallel connections automatically, scaling to 32 for larger files after verifying server byte-range support.
- Move manual connection counts to Settings and persist them for desktop and browser downloads.
- Remove the per-download connection slider and show the actual connection count after probing.
- Use one stream when file size is unknown or a server advertises ranges but ignores them.
- Publish reproducible locked builds, checked release assets, portable packages, and SHA-256 checksums.

- Keep IDG running in the Windows system tray when its window is closed; add Open IDG and Quit IDG tray actions.
- Add a current-user Windows startup setting and background startup mode; enable startup by default in the installer.
- Reopen the existing app on a second launch and restore hidden windows for browser download confirmation.

- Redesigned desktop library and browser popup with clearer actions, search, calmer selection, and connection feedback.
- Display transfer rates as decimal Mbps and make filenames editable before downloading.
- Add a real native messaging bridge and stable unpacked extension identity; register Chrome/Edge integration during installation.
- Pause browser downloads during handoff; resume them if IDG cannot accept the request.
- Open an IDG confirmation window for browser downloads and forward cookies, referrer, and user agent through probe and segment requests.
- Correct popup API fields, retain interception settings across worker restarts, and add regression tests.
- Move the browser API to port 17843 to avoid a verified port 1001 conflict; show bind failures in the desktop UI.
- Fix duplicate release workflow definitions and package the native bridge.

# Changelog

All notable changes to IDG are documented here.  
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).  
Versions follow [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

---

## [1.0.0] — 2026-10-19

### Added
- Multi-segment parallel HTTP download engine (`Range: bytes=X-Y`)
- Native Windows GUI — compact IDM-style layout (egui/eframe)
- Chrome / Edge browser extension (Manifest V3)
  - `webRequest.onHeadersReceived` download interceptor
  - Context-menu "Download with IDG" on links, images, and video
  - Extension popup with live download queue
- Native messaging host bridge (`com.rust_idm.host`)
- HEAD probe with `Accept-Ranges` detection and `Content-Disposition` filename
- Per-segment retry with exponential back-off (up to 5 attempts)
- Pre-allocated output files to prevent filesystem fragmentation
- Pause / Resume / Cancel with atomic flags
- 4-second rolling speed average and ETA per download
- Category sidebar: All · Downloading · Finished · Paused · Failed
- Persistent settings (default save dir + segment count)
- Inno Setup 6 installer script with native host registration
- GitHub Actions CI: build on push, release on tag

[Unreleased]: https://github.com/mustafa1p/IDG/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/mustafa1p/IDG/releases/tag/v1.0.0
