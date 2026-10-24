IDG v1.4.3 fixes the main window reopening very small at a screen corner when starting a browser download or restoring IDG.

- Preserve the last usable window size and position instead of saving minimized Windows coordinates and a zero-sized client area.
- Automatically recover window settings already affected by minimized-window autosaves.
- Restore hidden or minimized windows without undoing maximization or issuing duplicate minimize/restore commands.
- Include the updated Settings layout from the main branch.

Update through **IDG → Settings → Application updates**, or quit IDG from its tray menu and install **IDG-Setup-v1.4.3.exe**. Existing window settings recover automatically; no manual reset is needed.

The Windows x64 installer, complete portable app, extension ZIP, and SHA-256 checksums are the release assets. Keep the bundled tools and native bridge with the portable app.

Validation includes regression tests reproducing the corrupted minimized-window record, Rust and extension tests, formatting, Clippy, signing-script tests, and a Windows release build. Interactive tray testing was not run locally because an installed IDG instance is running.
