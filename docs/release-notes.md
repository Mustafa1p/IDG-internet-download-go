IDG v1.4.2 adds support for authenticated Hostinger Mail email exports.

- Export individual messages as `.eml`, or download Hostinger's ZIP containing multiple `.eml` messages.
- Support Inbox and Sent-folder export links with repeated message selections.
- Preserve browser session cookies and supported authentication headers for exports started from your signed-in Mail tab.
- Use a single streaming request for generated exports, avoiding HEAD and byte-range probes.
- Allow active downloads to run beyond the previous 60-second total timeout.
- Show actionable errors for missing or expired sessions, and flush file writes before reporting completion.

Update through **IDG → Settings → Check for updates now**, or quit IDG from the tray and install **IDG-Setup-v1.4.2.exe**. Update or reload the IDG extension, then start a fresh export from your signed-in Hostinger Mail tab. Pasting an authenticated export URL alone may fail. Interrupted generated exports must be started again from Mail.

Installer, portable app, and extension ZIPs are included, with SHA-256 checksums in SHA256SUMS.txt. The extension adds the webRequest permission to capture the export request's supported authentication headers.

Validated locally with 24 Rust tests, 23 extension tests, formatting, Clippy, and a Windows release build. Export regression tests use a local authenticated server and a real ZIP containing EML messages; a live signed-in Hostinger export has not been verified.
