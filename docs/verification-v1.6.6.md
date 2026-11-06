# v1.6.6 verification

Source engine: 93e0fde; version metadata/build candidate: cfbe8f2. Release publication is still pending.

- Windows x64/x86: 79 app tests and two native-host tests pass per architecture; nine external/tool-dependent tests are explicitly ignored. Both architectures pass all-target clippy with warnings denied. Formatting passes.
- Extension unit regressions: 60 pass. Version regressions: two pass. Music protocol/profile regressions: 18 pass.
- Real Edge fixture checks pass for YouTube, X, Instagram and TikTok: natural clicks, hostile page CSS, scroll visibility, removed-button recovery, SPA navigation, visible handoff errors, 60 homepage feed mutations, homepage preview exclusion, and the closed-tab badge race. The package never requests background/*.svg.
- The isolated development build passes five dialog cases (800x500 at 100%, 125%, 150%, 200%; 1280x800 at 100%) and 54 desktop interaction assertions. MP3, MP4 and 480p selection keep the footer clickable. These checks use the development-only preview harness; they do not establish runtime verification of the packaged release window.
- Exact-byte regressions cover interrupted/stalled streams, short-buffer reconnects, permanent failures, validator/range mismatch, pause/resume, concurrent file capacity, cancellation while queued, same/cross-origin credential isolation, connection reuse, POST redirect methods, updater checksums/sizes and response-body timeouts.
- Fresh release CLI engine 93e0fde passes 24/24 local A-D trials. An earlier 27/27 HTTPS integrity run overlapped compilation: its speed timings are not used for a conclusion. A separate isolated comparison of the versioned binary passed35/35 Rust and18/18 Node/Python transfers. Rust medians were55.93 MiB/s for IDG versus17.70 original IDG,43.33 curl,37.97 aria2c and29.43 Prism. Curl still led the Node test. Raw evidence is retained under target/perf-20261106-*.

The normal-suite tests do not claim live success for restricted YouTube/social posts or paired music accounts. A persistent website 403 remains an error after bounded retries.

The Windows Computer Use helper returned "native pipe is unavailable" after retry/reset recovery. An existing development app process locked target/debug/IDG.exe; it was left running. The preview checks used a separate target directory. Normal release-window, tray and bridge cold-start checks remain incomplete in this session. No claim of all-computer compatibility or universal speed superiority is made.

Before publishing, require hosted checks to finish, validate the actual release packages/architectures/checksums/build identity and exercise the packaged native host. After publishing, download the published assets and verify their integrity and updater-visible version.
