IDG v1.6.0 - larger desktop and safer downloads

- Prism-inspired native desktop: charcoal surfaces, status tabs, readable filenames, state-colored progress, live speed overview and selected-download details.
- Larger 1280 x 800 default window; Windows scaling, multi-monitor fixes and compact layouts.
- Exact Content-Range and server-validator checks prevent mixing bytes from changed files.
- Reject browser robot checks and unexpected HTML instead of saving them as binary files. Intentional HTML/JSON downloads remain supported.
- Reserve duplicate destinations atomically with numbered filenames. Flush and sync output before completion.
- Include pending Spotify browser track/library actions, metadata and media-overlay fixes. Preserve single-use form and mail-export behavior.

Install the matching 1.6.0 desktop installer and Chrome/Edge extension, then reload website tabs.
Windows x64/x86 installers and portable packages are included, with SHA-256 checksums.

Experimental music remains opt-in: Spotify pairing/browser actions do not establish working
full-track audio export. Spotify audio-key access can still fail; other subscription services
are unverified. Bandcamp was verified in the preceding preview. No YouTube substitutions.
The optional music runtime requires 64-bit Windows and Python 3.12. See docs/music-backend.md.

See docs/prism-comparison.md for source comparison and real-app before/after screenshots.
Prism's torrents, FTP, scheduling, localization, dynamic segmentation and persistent crash
resume are not implemented by this release. No speed/memory superiority is claimed.
