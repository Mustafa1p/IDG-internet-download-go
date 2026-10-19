IDG v1.4.0 introduces the compact desktop redesign and a status-only extension popup.

- Smaller **960×600** default window, with **800×500** minimum size.
- Dark teal library inspired by the supplied reference: sidebar Add download, live speed/count summaries, status and file-type categories, search/sorting, progress rows, and download details.
- Speeds remain in **Mbps**. Existing YouTube formats/qualities, in-app updates, tray behavior, and Windows startup remain available.
- Extension popup shows only **IDG is connected**, **IDG is ready**, or **IDG is not connected**. Removed capture switch, manual link form, recent downloads, popup YouTube button, and footer links.
- Browser capture stays automatic; use YouTube's page overlay or the browser context menu to open the desktop chooser.

Use **Settings → Check for updates now** in IDG v1.3.1, or install **IDG-Setup-v1.4.0.exe** after quitting the old app through the tray menu. Portable users should extract the entire **IDG-portable-v1.4.0.zip**. Existing v1.3.x unpacked extensions can update/reload through IDG; **IDG-extension-v1.4.0.zip** is available for a fresh installation.

Validated with Rust/extension regression tests and visual checks at both desktop sizes. SHA256SUMS.txt contains package checksums.
