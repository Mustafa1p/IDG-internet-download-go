IDG v1.4.7 fixes X/Twitter video downloads.

- Recognize X and Twitter status links in the desktop app and extension, including /video/1 selection and links with sharing parameters.
- Resolve blob video players to their X post, including posts on a timeline, instead of reporting No direct link.
- Route X posts through the bundled yt-dlp/FFmpeg tools with the video format and quality dialog; preserve available browser session cookies for the handoff.
- Check X support before forwarding to older desktop versions and show an update instruction when needed.
- Correct native-host media readiness on x86 installations that bundle Node instead of Deno.
- Keep ordinary context-menu links pointing to the clicked file even when the current page contains a supported video.

Validation: Rust regression tests, extension regression tests, a real Edge MV3 timeline/blob-player handoff, and a complete download of the supplied X post. FFprobe confirmed both audio and video in the 84,782,568-byte MP4.

Install the matching IDG-Setup-v1.4.7.exe (x64) or IDG-Setup-v1.4.7-x86.exe (x86), update the extension using IDG-extension-v1.4.7.zip, and reload existing X tabs. You can also paste an X post URL into New download. Access to private or unavailable posts still depends on the source website and your browser session.

IDG is free and has no license-key activation system. A reported recurring license prompt has not been identified and is not claimed as fixed by this release.
