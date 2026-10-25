IDG v1.5.1 preview keeps music account setup inside IDG's own interface.

- Spotify Desktop now discovers **IDG** in Connect to a device. IDG handles pairing directly through its internal Connect library.
- Replace the upstream account window with **IDG Music Accounts**, with masked credential fields, background connection progress, disconnect controls, and safe cancellation. No separate downloader application is opened.
- Preserve saved accounts after failed or cancelled sign-ins. Bound Tidal authorization and service network requests; allow pairing cleanup before the desktop timeout.
- Keep the existing original-service adapters and dependency credits internally. No YouTube audio substitution.

Verification: Windows desktop tests, account persistence/output regression tests, live local Spotify Connect device identity, and IDG account-window interaction checks. Bandcamp audio was verified in the preceding preview. Authenticated Spotify, Apple Music, Deezer, Qobuz and Tidal audio downloads remain unverified.

Install the matching v1.5.1 desktop installer and extension. Open Settings > Music, install the optional backend if necessary, then choose Connect Spotify and select IDG in Spotify Desktop's device picker. Existing accounts and the installed music runtime are retained. The runtime requires 64-bit Windows and Python 3.12 (64-bit). This preview is not offered automatically to stable installations.
