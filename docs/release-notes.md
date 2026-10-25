IDG v1.5.0 preview adds an optional original-service music backend.

- Add Settings > Music with backend installation, Spotify Connect pairing, account management and configured-account status.
- Route Spotify, Bandcamp, Apple Music, Deezer, Qobuz and Tidal links to a separate, revision-pinned OnTheSpot worker. Audio is sourced from the requested service; no YouTube substitutions.
- Support audio output formats and collection folders. Verify file paths, audio streams, output containers and available source duration before marking success. Cancel the worker process tree and preserve existing destination files.
- Encrypt the persistent account profile for the current Windows user and keep temporary sessions out of IDG history.
- Keep the music runtime optional, per-user and separate from existing file/video downloads. It requires 64-bit Windows and Python 3.12 (64-bit).

Verification: an original Bandcamp track downloaded through IDG and passed audio verification (4,870,996 bytes). Native/browser regression tests and worker fixtures cover completion, invalid files, duration mismatch, credentials and cancellation. Spotify pairing did not receive an account before timeout; authenticated Spotify, Apple Music, Deezer, Qobuz and Tidal downloads are not yet verified. This is a preview and is not offered automatically to stable installations.

Install IDG-Setup-v1.5.0.exe (x64) or IDG-Setup-v1.5.0-x86.exe (x86), update the matching extension, then open Settings > Music. Accounts must be connected locally. Music currently requires Unlimited speed and does not support pause/resume. Source subscriptions, access and regional limits still apply. See docs/music-backend.md for setup and current limits.
