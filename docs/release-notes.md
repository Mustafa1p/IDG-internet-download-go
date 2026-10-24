IDG v1.4.3 fixes tiny restored windows and adds clear update notifications with release highlights.

- Preserve the last usable window size and position instead of saving minimized Windows coordinates and a zero-sized client area.
- Automatically recover window settings already affected by minimized-window autosaves.
- Restore hidden or minimized windows without undoing maximization or issuing duplicate minimize/restore commands.
- Show a centered “Please update” popup with the new version, fixes, update progress, and Update now / Later controls.
- Check at startup and hourly while IDG runs in the tray, and announce each new version once per session.
- Keep the update banner available after dismissing the popup; reopen the popup through “What’s new”.
- Include the updated Settings layout and allow unsigned releases without Azure configuration.

Update through **IDG → Settings → Application updates**, or quit IDG from its tray menu and install **IDG-Setup-v1.4.3.exe**. Existing window settings recover automatically; no manual reset is needed. This refreshed package keeps version 1.4.3. If you already installed the earlier 1.4.3 package, install this refreshed installer once to gain the new popup; equal version numbers do not trigger an update loop. Older installed builds keep their existing notification behavior until updated.

The Windows x64 installer, complete portable app, extension ZIP, and SHA-256 checksums are the release assets. Keep the bundled tools and native bridge with the portable app.

Validation includes regression tests for minimized-window settings, release-note parsing, installed-version filtering, and installer integrity, plus Rust and extension tests, formatting, Clippy, signing-script tests, a Windows release build, and a rendered update-popup preview. Interactive tray testing was not run locally because an installed IDG instance is running.
