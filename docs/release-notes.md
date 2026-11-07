IDG v1.6.8 - safer stalled-download recovery

- Recover several stalled download ranges using idle connections, resuming from bytes acknowledged by the disk writer.
- Preserve valid responses while their first headers are pending, during brief healthy gaps and when only a small tail remains. This avoids an unnecessary replacement request that some servers reject with HTTP 403.
- Reuse the initial validated response for eligible downloads to reduce startup requests. Keep source compatibility fallbacks, credentials, capacity queues, validators and pause/cancel safeguards.
- Preserve single-use generated exports without replaying their request. Add real-server recovery cases and browser checks for YouTube feed churn and closed-tab races.

Includes the previous responsive format/quality dialog, colored interface, YouTube homepage, media-link refresh and updater fixes. Experimental automatic-helper work remains outside the desktop downloader. Throughput varies by source and workload; no universal speed or all-hardware claim is made.

Install the matching desktop app and extension, then reload website tabs. This package uses extension ID blmjnmockbofpnipcniflmalfmiaolcj and contains no background/*.svg assets. The reported omiebabagdcfedkhfmjlndcpocnafcpb ID should be checked for an old or conflicting installation.

Windows x64/x86 installers, portable packages, extension ZIP and SHA-256 checksums are included. Actual packaged GUI/tray/cold-start verification remains incomplete because Windows desktop automation was unavailable. Persistent source refusals remain visible errors.
