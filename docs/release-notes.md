IDG v1.4.8 adds Instagram and TikTok video downloads.

- Paste Instagram Reel/video-post URLs or TikTok video/share URLs into New download, or use the Download with IDG button and browser context menu.
- Resolve supported blob players to their media post, including Instagram and TikTok feed items. Preserve available browser cookies for authenticated handoffs.
- Exclude TikTok source variants marked as watermarked. If no matching source remains, the download fails instead of choosing a marked watermarked fallback. Availability depends on the source post and region; embedded creator logos are preserved.
- Handle the first video in Instagram carousel posts and show its available formats.
- Explain that Spotify music tracks are DRM-protected and must use Spotify's own offline-listening feature. Prevent saving Spotify pages as fake audio downloads. Spotify music export is not supported.
- Preserve existing YouTube and X video routing and ordinary file downloads.

Verification: complete public Instagram and TikTok downloads; FFprobe verified the Instagram source's silent video and TikTok audio/video; selected TikTok source was not marked watermarked, and three sampled frames had no TikTok overlay. A watermarked-only fixture was rejected. Rust/extension regressions, real Edge blob-player/feed handoff checks, and Spotify rejection without queue entries or files are covered.

Tested public examples:
- Instagram: https://www.instagram.com/reel/Chunk8-jurw/
- TikTok: https://www.tiktok.com/@patroxofficial/video/6742501081818877190

Install the matching IDG-Setup-v1.4.8.exe (x64) or IDG-Setup-v1.4.8-x86.exe (x86), update the extension with IDG-extension-v1.4.8.zip, and reload existing website tabs. Website restrictions, authentication and regional availability still apply.
