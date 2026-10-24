IDG v1.4.4 fixes token-protected POST downloads and missed PHP download detection.

- Detect files served by `waiting-resource.php` and other extensionless/PHP endpoints using the browser-resolved filename as well as the URL and MIME type.
- Recheck downloads when the browser resolves their metadata after creation. Forward the final redirected URL, cookies and referrer to IDG.
- Preserve the final Adsume/SafeFileHosting token-bearing POST before the website submits it. Stream its response once, without HEAD probes, token replays or parallel POST requests. Support equivalent hidden download forms on other sites while keeping uploads and login forms in the browser. Keep tokens in memory only.
- Restore native browser form submission when IDG is disabled, unavailable or too old to support POST transfers. Do not recapture that fallback as a tokenless GET.
- Handle trusted clicks on dynamically inserted `button_dm` controls with direct HTTP/HTTPS links or simple location navigation scripts.
- Let countdowns, forms and token-generating scripts run normally; detect the resulting browser download once it starts. Waiting HTML pages are not treated as file links.
- Prevent duplicate handoffs when browser download events arrive together. If IDG rejects a handoff, keep the browser download available.
- Include the desktop, installer, update detection and history improvements from the latest 1.4.3 source builds.

Install **IDG-Setup-v1.4.4.exe** (x64) or **IDG-Setup-v1.4.4-x86.exe** (x86). Update the IDG Integration Module to **1.4.4** and reload existing website tabs. Portable users must keep the native bridge and bundled media tools with IDG.

Validation: 39 extension regression tests; Rust tests and Clippy for Windows x64 and x86; a real Edge MV3 fixture verifying the MAIN/isolated-world automatic form handoff; exact-byte token POST transfer and expired-token rejection tests; signing and installer checks. Inspected the supplied iGetIntoPC → waiting-resource.php → Adsume POST HTML flow. The production archive itself was not downloaded. Final token transfers use a single connection because replaying a one-use token is unsafe. Chrome/Edge 111 or newer is required.
