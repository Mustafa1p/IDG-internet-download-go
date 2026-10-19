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
