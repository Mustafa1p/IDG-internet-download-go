IDG now selects parallel connections automatically and keeps browser downloads available from the Windows system tray.

### What changed

- **Automatic parallel connections:** up to 32 connections for larger files, with fewer connections for small files. IDG verifies that the server honors byte ranges before splitting a download.
- **Manual control in Settings:** choose Automatic or enter a connection count from 1 to 32. Your choice persists across restarts and applies to new desktop and browser downloads. The download confirmation window has no connection slider.
- **IDG Integration Module:** repaired Chrome/Edge detection, native app launch, cookie/referrer forwarding, and editable download confirmation. Failed handoffs resume the original browser download.
- **System tray:** closing the window keeps downloads running; click the tray icon to reopen or choose Quit IDG to exit. Windows startup is available in Settings.
- **Refreshed interface:** searchable downloads, calmer selection, clearer connection status, and transfer speeds in Mbps.

### Downloads

- **IDG-Setup-v1.1.0.exe** — recommended Windows installer; registers the native bridge for Chrome and Edge.
- **IDG-portable-v1.1.0.zip** — app, native bridge, extension, and registration script. Keep the included executables together.
- **IDG-extension-v1.1.0.zip** — IDG Integration Module for Chrome and Edge.
- **IDG-v1.1.0.exe** — standalone desktop app; automatic launch from the extension requires the native bridge and registration.
- **SHA256SUMS.txt** — SHA-256 checksums for all four downloads.

### Upgrade and setup

1. Exit the old IDG through its tray menu, then install or extract this release.
2. Remove the old browser extension and load the new `extension` folder through `chrome://extensions` or `edge://extensions` with Developer mode enabled.
3. For portable use, run `installer/register-native-host.ps1` from PowerShell to register the included bridge.
4. Download a direct file link. Choose its filename and save location in IDG, then start the download.

The browser API now uses `127.0.0.1:17843`; update the app and extension together. Direct HTTP/HTTPS downloads are supported; blob URLs and protected media remain in the browser. Actual throughput depends on server and network capacity.

### Validation

Rust and extension regression tests cover automatic/manual connection planning, exact output from 32 parallel ranges, fallback for servers that ignore ranges, saved settings, authenticated downloads, and failed browser handoffs. The Windows lifecycle smoke test also verifies background downloads, tray reopening, duplicate-launch handling, browser confirmation, and quitting. The installer and all assets are built from the tested commit by GitHub Actions.
