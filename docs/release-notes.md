IDG v1.6.3 - download probe and token form fixes

- Fix SafeFileHosting form detection when the website changes or removes its form ID, preserving the token-bearing POST.
- Prevent bare download.php URLs from being handed off as GET requests. Existing failed entries need a fresh download from the original website.
- Add a streaming GET fallback for servers that reject HEAD and range probes with HTTP 405 or other unsupported-probe responses. The accepted stream is reused, avoiding another request.
- Retain browser-page checks, one-use POST handling and verified parallel range downloads.

Install the matching desktop app and browser extension, then reload the source website before downloading again. A pasted SafeFileHosting endpoint URL alone does not contain the required file token.

Windows x64/x86 installers, portable packages and the matching browser extension are included.
