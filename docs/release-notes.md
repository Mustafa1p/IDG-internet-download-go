YouTube downloads now work through the video page instead of trying to download a temporary blob URL.

### What changed

- **YouTube support:** watch pages, Shorts, and youtu.be links route to the bundled yt-dlp/FFmpeg/Deno toolchain. Video and audio are merged into MP4. Radio and playlist parameters are removed so only the selected video is downloaded.
- **Repaired browser connection:** opening the full IDG app registers its native bridge for Chrome and Edge. Settings includes a repair action. The extension reports missing registration or an outdated app with clear upgrade instructions.
- **YouTube controls:** use the button over the player, the extension popup, or paste the page link into IDG. Temporary authenticated cookies are cleaned up after the media job.
- **Complete packages:** installer and portable ZIP include pinned, checksum-verified media tools and their upstream licenses/source references.
- **Release badge:** the README shows the current release version without relying on a cached GitHub release badge.

### Downloads and upgrade

- **IDG-Setup-v1.2.0.exe** — complete Windows x64 installer, including browser bridge and media tools.
- **IDG-portable-v1.2.0.zip** — extract the entire archive and keep its tools folder beside IDG.exe.
- **IDG-extension-v1.2.0.zip** — IDG Integration Module for Chrome and Edge.
- **SHA256SUMS.txt** — download checksums.

Quit the previous IDG through its tray menu, install/extract the complete new package, then reload or replace the browser extension. Open IDG once to register the bridge. The app and extension must both be updated for YouTube support.

The prior automatic connections, Mbps display, editable filenames, system tray, and Windows startup features remain included. Regular files support pause/resume; YouTube jobs support cancellation through Remove. Video availability depends on YouTube and the current authenticated session.

### Validation

A live download of the reported YouTube video completed successfully, and FFprobe confirmed both audio and video streams in the MP4. Automated tests cover YouTube URL canonicalization, playlist removal, blob-player and SPA handoff, old-app upgrade errors, progress parsing, and the existing direct-file download behavior. GitHub Actions builds and packages the full media toolchain from the tested source.
