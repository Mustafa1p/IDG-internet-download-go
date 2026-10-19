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
