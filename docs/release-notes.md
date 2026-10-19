IDG v1.2.1 fixes filenames for GitHub archives and other redirected downloads.

- Uses the server’s complete filename instead of generic URL names such as `main` or `main.zip`.
- Correctly handles UTF-8 filenames, encoded URL paths, and quoted filenames.
- Keeps filenames you explicitly edit in the download dialog.
- Includes all v1.2.0 browser integration, YouTube, automatic connections, system tray, and startup improvements.

Install **IDG-Setup-v1.2.1.exe** after quitting the old IDG through its tray menu. For portable use, extract the entire **IDG-portable-v1.2.1.zip**. Update the Chrome/Edge extension using **IDG-extension-v1.2.1.zip**.

Verified with redirected archive fixtures, exact-byte download checks, filename parsing tests, and the supplied GitHub archive URL. SHA256SUMS.txt contains package checksums.
