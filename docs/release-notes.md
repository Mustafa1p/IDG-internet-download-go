IDG v1.4.3 refreshes the Windows desktop interface to follow the supplied Windows 11 Fluent design reference, while retaining the Rust download engine and browser integration.

- Blue Fluent navigation, integrated search/filter controls, real window controls, and separate live speed/active/completed/queued cards.
- Functional list and grid views, seven table columns, multi-selection, date sorting, bulk pause/resume, and remove-from-list confirmation that keeps files on disk.
- A locally rendered folder empty state, consistent Settings cards, download/media dialogs, details and error/update messages.
- Compact layouts at 800×500, plus larger desktop layouts; Windows reduced-motion preference and Segoe typography.
- Keep the tiny-window restoration fix and newer-version update popup with release highlights, progress, Update now and Later.
- Bundle the UI route guide, QA report and screenshots with installed/portable packages so README paths remain usable offline.
- Preserve the existing HTTP, authenticated export, YouTube/media, native bridge, updater, startup and tray workflows.

**The version remains 1.4.3.** This is a refreshed package under the existing release. If you already installed an earlier 1.4.3 build, quit IDG from its tray menu and run the refreshed **IDG-Setup-v1.4.3.exe** once. Equal version numbers do not trigger an automatic update loop. User downloads and saved preferences remain intact; the existing queue/history is still session-only.

The release includes Windows x64 (`IDG-Setup-v1.4.3.exe`) and x86 (`IDG-Setup-v1.4.3-x86.exe`) installers, matching portable packages, browser extension, and SHA-256 checksums. Keep the native bridge and bundled media tools with the portable app.

Validation and limitations are documented in [the QA report](https://github.com/Mustafa1p/IDG-internet-download-go/blob/main/docs/fluent-qa.md), and every UI/integration route is mapped in [the navigation guide](https://github.com/Mustafa1p/IDG-internet-download-go/blob/main/docs/ui-navigation.md). Rendering-scale tests are not actual Windows display-DPI changes; a live signed-in browser/media session and Windows Snap-layout flyout are not certified by these checks.
