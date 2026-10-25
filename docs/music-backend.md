# Original-service music backend (1.5.0 preview)

The preview adds a Music tab in Settings and a separate OnTheSpot worker for Spotify, Bandcamp, Apple Music, Deezer, Qobuz and Tidal. The Rust desktop remains responsible for queue state, cancellation, output validation and final file placement. Music audio comes from the selected service; no YouTube search or substitution is used.

## Setup

1. Use 64-bit Windows and install Python 3.12 (64-bit) from https://www.python.org/downloads/windows/ if it is not already installed. A 32-bit IDG process can launch the 64-bit worker on 64-bit Windows; the optional music runtime is not supported on 32-bit Windows.
2. Open **Settings > Music > Install music backend**. Setup downloads checksum-verified OnTheSpot and librespot source archives at the revisions in `music/upstream.json`, plus the dependency versions in `music/requirements-lock.txt`. It installs into the current user's Local AppData under `IDG/music` without changing the system Python environment. Source copies and notices for those two projects are kept with the optional runtime.
3. For Spotify, click **Connect Spotify**, then select **OnTheSpot** in Spotify Desktop's **Connect to a device** menu. The pairing attempt times out after about three minutes; retry if no session was received. Spotify Desktop and IDG must be able to discover each other on the network.
4. Use **Manage music accounts** for Apple Music, Deezer, Qobuz and Tidal. This opens the OnTheSpot account window using an isolated IDG profile. Close it to save and return to IDG. The upstream usage guide describes account and subscription requirements: https://github.com/ots-downloader/onthespot/blob/7f7e04d883dc2d12499e6a0f9a80a09116cf69f7/docs/USAGE.md
5. Paste a track, album or playlist URL into **New download**, choose an audio output format and a save location, then start. Albums/playlists save as a new folder; existing files or folders are not overwritten. Recognized music links can also be handed off through the browser extension.

Bandcamp public audio does not require a paid account. Paid or authenticated services retain their own access, quality and regional restrictions. The service bitrate is used by default. Choosing FLAC or a higher bitrate does not improve a lower-quality source.

## Credentials and completion

The persistent music account profile is encrypted with Windows DPAPI for the current Windows user. Temporary upstream profiles and session files are scoped to per-job directories. They are removed on normal exit; worker leases allow later actions to clean up profiles left by a terminated worker. Account secrets are not passed in command-line arguments, browser download cookies or IDG history. Refreshed service credentials are merged back into the encrypted profile without erasing other accounts or overwriting a newer interactive login. Refresh shows configured accounts; it does not assert that every service session is currently valid.

The worker resolves the collection before starting transfers, reports a structured manifest and must finish successfully. IDG requires every reported file to be nonempty and inside its staging folder. FFprobe must confirm an audio stream, usable duration and the selected output container. When the service provides the original duration, a substantially shorter preview or truncated result is rejected. A successful CLI exit alone does not complete a queue item. Cancellation terminates the worker process tree.

## Verification status

- Live original-service Bandcamp download of https://benprunty.bandcamp.com/track/lanius-battle through IDG: **4,870,996 bytes**, audio verified before completion.
- Spotify: public metadata and link routing checked. A pairing attempt did not receive an account before timeout. **Authenticated Spotify audio is not yet verified.**
- Apple Music, Deezer, Qobuz and Tidal: adapters and account setup are integrated through the pinned upstream worker; **authenticated downloads are not yet verified**.
- Automated checks cover URL/source routing, missing backend/account messages, encrypted credential persistence across a new Python process, stale-profile cleanup, partial-failure queue states, invalid/empty/outside output paths, missing manifests, wrong output containers, duration mismatch and cancellation.

## Current limits

Music downloads require the **Unlimited** speed setting and do not support pause/resume. One music worker runs at a time. The initial preview has a 30-minute worker limit and a 10,000-item collection bound. Playlist access or an unavailable track can fail the job; it is not silently marked complete. The optional runtime can be reinstalled while keeping the encrypted account profile. No automatic cloud uploading is performed.

## Licensing

IDG's Rust application is MIT licensed. The separately executed files in `music/` are GPL-2.0-only and include `music/COPYING`. The optional runtime is obtained separately from its upstream projects/PyPI and retains upstream licensing and source notices. No implementation from the unlicensed Spotify-playlist reference repository was copied.
