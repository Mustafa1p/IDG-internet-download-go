# Original-service music backend (1.5.0 preview)

The preview adds IDG Music Accounts, IDG-branded Spotify Connect pairing, a Music tab in Settings and an internal music worker built on OnTheSpot service libraries for Spotify, Bandcamp, Apple Music, Deezer, Qobuz and Tidal. The Rust desktop remains responsible for queue state, cancellation, output validation and final file placement. Music audio comes from the selected service; no YouTube search or substitution is used.

## Setup

1. Use 64-bit Windows and install Python 3.12 (64-bit) from https://www.python.org/downloads/windows/ if it is not already installed. A 32-bit IDG process can launch the 64-bit worker on 64-bit Windows; the optional music runtime is not supported on 32-bit Windows.
2. Open **Settings > Music > Install music backend**. Setup downloads checksum-verified OnTheSpot and librespot source archives at the revisions in `music/upstream.json`, plus the dependency versions in `music/requirements-lock.txt`. It installs into the current user's Local AppData under `IDG/music` without changing the system Python environment. Source copies and notices for those two projects are kept with the optional runtime.
3. For Spotify, click **Connect Spotify**, then select **IDG** in Spotify Desktop's **Connect to a device** menu. The pairing attempt times out after about three minutes; retry if no session was received. Spotify Desktop and IDG must be able to discover each other on the network.
4. Use **Manage music accounts** for Apple Music, Deezer, Qobuz and Tidal. This opens IDG Music Accounts. No separate downloader application is opened. The service libraries stay internal, with their source and licensing credits preserved. Close it to save and return to IDG. The upstream usage guide describes account and subscription requirements: https://github.com/ots-downloader/onthespot/blob/7f7e04d883dc2d12499e6a0f9a80a09116cf69f7/docs/USAGE.md
5. Paste a track, album or playlist URL into **New download**, choose an audio output format and a save location, then start. Albums/playlists save as a new folder; existing files or folders are not overwritten. Recognized music links can also be handed off through the browser extension.

Bandcamp public audio does not require a paid account. Paid or authenticated services retain their own access, quality and regional restrictions. The service bitrate is used by default. Choosing FLAC or a higher bitrate does not improve a lower-quality source.

## Spotify playback and unchanged page URLs

Spotify can keep a Liked Songs, playlist or track page URL unchanged while switching the playing song. IDG's row buttons read the clicked row's current track link at click time, including rows reused by Spotify's virtualized list. The current-player action follows the main player's link/URI and MediaSession metadata. It observes text and attribute changes and checks playback periodically, so navigation is not required. After a song change, it rejects stale address-bar URI hints and temporarily stale player links instead of downloading the previous track. A stale sidebar cannot override the primary player.

## Download all from Spotify Your Library

Playlist entries and Liked Songs in Spotify's sidebar have a Download all control. It queues the selected collection into IDG, not the currently playing track. Artist entries are excluded. Liked Songs is read from the Spotify account connected to IDG; that account should match the browser account. Library links discard playback URI and sharing parameters.

Collection resolution reads every page before starting downloads, using Spotify's current [playlist-items endpoint](https://developer.spotify.com/documentation/web-api/reference/get-playlists-items) and [saved-tracks endpoint](https://developer.spotify.com/documentation/web-api/reference/get-users-saved-tracks). It preserves order and repeated entries, limits jobs to 10,000 items, and fails on incomplete pages, denied access, unavailable/local-only tracks or a changed collection rather than silently completing a partial list. Spotify can restrict playlist access to owners/collaborators. Rate limits stop the request with retry guidance. A known audio-key failure stops the batch instead of requesting keys repeatedly for every song.

The library controls, full-collection routing and pagination are tested locally. **A complete Spotify playlist audio export is not verified: the connected session still fails to obtain audio access.**

## Credentials and completion

The persistent music account profile is encrypted with Windows DPAPI for the current Windows user. Temporary upstream profiles and session files are scoped to per-job directories. They are removed on normal exit; worker leases allow later actions to clean up profiles left by a terminated worker. Account secrets are not passed in command-line arguments, browser download cookies or IDG history. Refreshed service credentials are merged back into the encrypted profile without erasing other accounts or overwriting a newer interactive login. Refresh shows configured accounts; it does not assert that every service session is currently valid.

The worker resolves the collection before starting transfers, reports a structured manifest and must finish successfully. IDG requires every reported file to be nonempty and inside its staging folder. FFprobe must confirm an audio stream, usable duration and the selected output container. When the service provides the original duration, a substantially shorter preview or truncated result is rejected. A successful CLI exit alone does not complete a queue item. Cancellation terminates the worker process tree.

## Verification status

- Live original-service Bandcamp download of https://benprunty.bandcamp.com/track/lanius-battle through IDG: **4,870,996 bytes**, audio verified before completion.
- Spotify: account pairing and encrypted session storage now verified locally. The supplied Liked Songs link resolves its `uri=spotify:track:...` parameter to track `3PsejBjTRczNY88SlcWUyg`, without forwarding the collection `uid`. The playback session returned metadata with a duration of 174000 ms and audio variants. The live audio test then failed because Spotify did not provide an audio key to this backend. **Spotify audio export is currently blocked and is not verified working.** The Web API metadata endpoint also returned HTTP 429; single-track metadata now comes from the authenticated playback session, and rate-limit errors are reported with retry guidance.
- Apple Music, Deezer, Qobuz and Tidal: adapters and account setup are integrated through the pinned upstream worker; **authenticated downloads are not yet verified**.
- Automated checks cover URL/source routing, missing backend/account messages, encrypted credential persistence across a new Python process, stale-profile cleanup, partial-failure queue states, invalid/empty/outside output paths, missing manifests, wrong output containers, duration mismatch and cancellation.

## Current limits

Music downloads require the **Unlimited** speed setting and do not support pause/resume. One music worker runs at a time. The initial preview has a 30-minute worker limit and a 10,000-item collection bound. Playlist access or an unavailable track can fail the job; it is not silently marked complete. The optional runtime can be reinstalled while keeping the encrypted account profile. No automatic cloud uploading is performed.

## Licensing

IDG's Rust application is MIT licensed. The separately executed files in `music/` are GPL-2.0-only and include `music/COPYING`. The optional runtime is obtained separately from its upstream projects/PyPI and retains upstream licensing and source notices. No implementation from the unlicensed Spotify-playlist reference repository was copied.

## IDG interface in 1.5.2

Spotify Connect advertises the device as **IDG**, including its brand and model fields. IDG implements the pairing lifecycle directly with the internal librespot library. Account setup uses IDG's own dialog rather than importing the upstream GUI. Credentials are masked, connection work runs in the background, cancellation and failed sign-ins retain previous accounts, and Tidal browser authorization has a bounded wait. Library names remain in dependency credits and corresponding source notices.

Verification includes a live local Spotify Connect getInfo response advertising IDG, pairing timeout without account loss, and account UI credential masking, asynchronous failure, safe close and encrypted persistence. These checks do not establish an authenticated Spotify audio download.


Local Spotify page integration also reads public track/playlist identifiers from
bounded React props on the row or player when Spotify omits links. It does not
inspect account stores, session objects, credentials or access tokens. Ambiguous
identifiers are rejected. The row is resolved again when its button is clicked,
so virtualized rows and playback changes do not rely on the browser URL.

After reloading the unpacked extension, refresh existing Spotify tabs too. The
extension popup reports whether its current page script and Spotify reader are
loaded, and offers a page refresh plus copyable diagnostics containing only
extension/build identifiers, enabled state and control counts. These controls and
fixture tests do not establish successful audio export: the live Spotify session
still fails to obtain an audio key, as described above.


## Local track naming correction (not released)

Single-track Spotify jobs now report catalog title and artist names before the
worker starts audio transfer. The desktop changes an automatic filename to
`Artist One, Artist Two - Track title.ext`, keeping dots and Unicode in titles,
sanitizing path characters and reserving filename space for both artist and title.
Explicitly edited filenames are preserved. Metadata must match the requested
service and canonical URL, and existing files are never overwritten. The queue
retains the resolved name if audio access subsequently fails.

A live check of track `3PsejBjTRczNY88SlcWUyg` resolved **Bohème** by **ONZY, 7ari**,
174000 ms. The Rust worker received and applied **ONZY, 7ari - Bohème.mp3**.
The same live check still failed at Spotify audio access. A separate request at
normal source quality received explicit audio-key refusal code `1` on both
attempts, rather than merely timing out. The connected session identifies itself
as free; this does not establish that changing subscription would resolve the
refusal. Successful Spotify audio export remains unverified and blocked.

The filename handoff is also checked with real generated audio through Python,
Rust, FFprobe verification and final file placement, plus metadata-before-failure,
custom filenames, path sanitization and collision protection. Generated-audio
fixtures establish naming behavior, not Spotify audio availability.
