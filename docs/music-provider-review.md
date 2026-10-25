# Music provider integration review

Reviewed on 2026-10-25. This is a source review, not a claim that authenticated service downloads have been tested in IDG. IDG v1.4.8 remains the stable release; original-service support is implemented as the 1.5.0 preview.

## References inspected

- [Psycho-Fan/spotfiy-playlist-downloader](https://github.com/Psycho-Fan/spotfiy-playlist-downloader/tree/9bcb54d91b2e56e801fb58af04b93d1c009bf5e4)
- [ots-downloader/onthespot](https://github.com/ots-downloader/onthespot/tree/7f7e04d883dc2d12499e6a0f9a80a09116cf69f7)

## What the projects actually do

The playlist downloader scrapes public Spotify embed metadata and constructs a `ytsearch1:` query from the artist and title. Its MP3 audio comes from YouTube. A matching title does not establish that the recording, edit, explicit version, or duration matches the Spotify original. Its default workflow also uploads a generated ZIP to Gofile unless disabled. No repository license was declared at the inspected revision; no source was copied into IDG.

OnTheSpot implements different service backends. Its Spotify path uses an authenticated librespot session and a service audio stream. Apple Music and Deezer have distinct media-processing paths, and the project includes pywidevine among its dependencies. This is a larger account/session integration, not another yt-dlp URL matcher. The project declares GPL-2.0; copying or redistributing its implementation requires preserving and satisfying its license terms.

IDG's existing Spotify error describes the limitation of its current backend. It should not be interpreted as proof that every third-party application is technically unable to access a service stream.

## Accounts and scope

These are upstream requirements documented or implemented at the inspected revision, not independently verified account access.

| Service | Upstream setup | IDG verification status |
| --- | --- | --- |
| Spotify | Account pairing through Spotify Connect; upstream code distinguishes free and Premium quality | Public metadata checked; no authenticated audio download tested |
| Apple Music | Media user token and a paid account according to upstream usage documentation | Not tested |
| Bandcamp | Public account mode; upstream exposes public audio | Live original-service download verified in 1.5.0 preview |
| Deezer | Account ARL session cookie | Not tested |
| Qobuz | Account login or authentication token | Not tested |
| Tidal | Browser account authorization | Not tested |

## Independent check using the supplied Spotify track

The public embed for `3uCdelPvIjIA0q0V6KRWFY` returned track metadata: title `JASON`, duration 173333 milliseconds, and artist fields. The data is nested under `props.pageProps.state.data.entity`. This establishes that catalog import is feasible for this example. It does not verify a full audio download, a matching YouTube recording, or availability for another account/region.

The first project's parser assumes `trackList` whenever `pageProps.state` exists. The supplied single-track embed has a track entity without `trackList`, so that parser needs different handling for this example. This conclusion comes from comparing the response structure with the source, without executing the downloaded project.

## Two distinct product workflows

1. **Optional original-service backend:** keep an explicitly configured music backend separate from IDG's file/video engine. Accounts are connected in that backend, and the selected source service is shown to the user. Do not copy passwords or tokens into download history or command-line arguments. Review dependency pinning, redistribution, and licensing before bundling anything. Authenticate and test each service separately before advertising it as verified.
2. **Spotify catalog to reviewed YouTube match:** implement public metadata import independently, present candidate recordings with source, artist, title and duration, and let the user select a candidate. Download through IDG's existing YouTube engine after selection. Label the actual source as YouTube; do not describe the resulting file as Spotify audio. Do not adopt automatic cloud uploading.

## Backend completion contract

The upstream CLI accepts `--download`, but does not expose the structured progress and output-file protocol that IDG's current media helper uses. Its queue-waiting code also nests the active-download wait inside the empty-queue loop, so a wrapper must not treat an exit status of zero alone as proof that all expected downloads completed. This is a static observation, not a reproduced runtime finding.

An IDG integration should require explicit item identifiers, progress, cancellation, final output paths, and per-item success/failure. Validate output existence and nonzero size and probe the resulting audio before marking an item complete. Preserve partial-failure information for playlists. Missing accounts, expired sessions and unavailable tracks must produce actionable errors rather than a completed queue entry.

The initial source review did not install a runtime or collect credentials. The subsequent implementation installs an optional, pinned runtime and keeps its licensing and source notices. The user selected the original-service backend. See [implementation, setup and verification](music-backend.md) for the 1.5.0 preview; Bandcamp is verified live, while authenticated services still need account verification.
