IDG v1.6.7 - authenticated downloads and connection recovery

- Fix password-only URL authentication: an empty username no longer causes data ranges to lose credentials after the initial probe. Preserve explicit authorization overrides and source-origin credential boundaries.
- Queue downloads when a server repeatedly closes excess TCP connections without HTTP 429/503, rather than exhausting probes while another file occupies the server's connections.
- Keep quick retries for isolated connection failures and test recovered capacity with at most one GET/HEAD trial at a time. Single-use generated GET exports and forms are neither replayed nor used to probe capacity, including after form redirects to GET.
- Fix a queue wake-up case involving a waiting form. Preserve cancellation, validated byte offsets, origin isolation, metadata deadlines and bounded failure when no healthy peers remain.
- Add real-socket coverage for reset/EOF caps, recovered capacity, malformed HTTP, queued cancellation and form safety. Benchmark reports retain independent CPU-cycle coverage and complete trial evidence.

Includes the previous dialog, closed-tab, YouTube homepage, interrupted-transfer and updater fixes. Throughput varied across the tested workloads and runs; this update makes no universal speed or all-hardware claim.

Install the matching desktop app and extension, then reload website tabs. This package uses extension ID blmjnmockbofpnipcniflmalfmiaolcj and contains no background/*.svg assets. The reported omiebabagdcfedkhfmjlndcpocnafcpb ID should be checked for an old or conflicting installation.

Persistent source refusals remain visible errors. Windows x64/x86 installers, portable packages, extension ZIP and SHA-256 checksums are included.
