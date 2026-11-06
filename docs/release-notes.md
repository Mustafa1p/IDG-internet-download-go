IDG v1.6.6 - reliable concurrent downloads and scoped authentication

- Recover stalled and interrupted range requests from disk-acknowledged offsets, keeping short buffers and partial files. Stop promptly on permanent authorization, range-validation and disk errors.
- Share server capacity fairly between files. Pausing a resumable download frees connections for other files; one-use form exports keep their accepted stream without replaying the form.
- Keep captured credentials on their source origin across redirects. Reuse connections across authenticated files without sharing their headers. Preserve form redirect methods and updater integrity checks.
- Replace the redundant metadata round trip with one validated GET probe and queue bounded smaller ranges for available workers.
- Expand real-socket and Windows regression coverage, retaining complete benchmark evidence and failures. Throughput varies by server and workload; no universal speed claim is made.

Includes the v1.6.5 fixes for the format/quality dialog, closed-tab background errors, YouTube homepage preview churn, and bounded video URL refresh after transient CDN failures.

Install the matching desktop app and extension, then reload existing website tabs. This package's extension ID is blmjnmockbofpnipcniflmalfmiaolcj. The reported background/*.svg URLs reference omiebabagdcfedkhfmjlndcpocnafcpb, a different extension ID; this package contains no such SVG assets.

Persistent website refusals remain visible errors after bounded retries. Windows x64/x86 installers, portable packages, extension ZIP and SHA-256 checksums are included.
