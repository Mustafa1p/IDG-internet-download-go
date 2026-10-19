IDG v1.4.1 makes the browser's media download overlay slimmer and dismissible.

- Slim **28px** Download with IDG bar with smaller text, padding, and rounded corners.
- New **×** close button with an accessible label; keyboard users can dismiss with **Escape** while focused.
- Dismissal hides the overlay and any error notice for that video until a page reload, without changing playback or sending a download.
- New videos can show their own overlay. Late handoff responses cannot bring a dismissed overlay back.
- The extension popup stays connection-status only. Desktop UI, YouTube choices, updater, and startup features are included.

Update through **IDG → Settings → Check for updates now**, or install **IDG-Setup-v1.4.1.exe** after quitting the old app from the tray. Existing v1.3.x/v1.4.x unpacked extensions can update through IDG. Refresh already-open video pages to load the new overlay. Portable and extension ZIPs are also available; SHA256SUMS.txt lists package checksums.

Verified with extension regressions for close/Escape, mutation rescans, YouTube navigation, and closing while a handoff is in flight, plus desktop build tests.
