IDG v1.3.1 includes a final updater correction and adds updates inside IDG, YouTube format choices, and Windows integration fixes.

- **In-app updates:** automatically download and verify new releases; install and restart from IDG, with optional automatic installation after all downloads finish. Checksums and size checks reject damaged downloads. Failed installation reopens IDG and requires an explicit retry, preventing automatic restart loops.
- **Extension updates:** detect and update the existing unpacked IDG Integration Module, remember a custom folder, and reload when no handoff is in progress. Store-managed extensions use the browser's update mechanism.
- **YouTube choices:** MP4, MKV, or WebM video with available quality discovery and maximum resolution selection; MP3, M4A, Opus, WAV, or FLAC audio. Source quality limits output quality.
- **Windows icons:** embed the IDG icon in executables/installer and match taskbar/shortcut identity.
- **Windows startup:** start in the tray by default; preserve changes made in Settings.

### Install this version once

Quit the old IDG using its tray menu and install **IDG-Setup-v1.3.1.exe**. Portable users should extract the entire **IDG-portable-v1.3.1.zip**. Open Settings to choose automatic update options; future updates can install from inside IDG.

After upgrading from extension v1.2.x, reload IDG Integration Module once in `chrome://extensions` or `edge://extensions`, then refresh open YouTube pages. If it is not detected, use Settings → Choose existing extension folder. **IDG-extension-v1.3.1.zip** is also available for a fresh extension installation.

### Validation

Rust/extension regression tests cover checksum rejection, interrupted updates, installer arguments, extension folder identity, preference preservation, safe reloads, and download handoff behavior. Live tests with the supplied YouTube URL verify video/audio streams, a 480p quality limit, and MP3 conversion. Windows build and installer are checked in CI. SHA256SUMS.txt contains package checksums.
