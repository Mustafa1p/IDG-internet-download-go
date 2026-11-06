IDG v1.6.5 - stable download dialog and smoother YouTube integration

- Fix the download dialog collapsing after changing video format or quality. Give it a stable scrolling height and separate, clearly labeled format and quality rows with a blue section heading.
- Handle tabs closing during background badge updates without unhandled promise errors.
- Stop adding download controls to YouTube homepage hover previews. Coalesce scroll positioning, skip unchanged overlay styles, and ignore unrelated feed mutations.
- Retry video extraction once after transient CDN/network errors (including HTTP 403/410), retaining partial files and selected quality. Add bounded retry backoff and fail visibly rather than skip unavailable video fragments.
- Preserve the original checksum-verified x64/x86 FFmpeg archives in repository release storage after upstream removed the pinned nightly build.
- Add regressions for closed tabs, homepage churn/navigation, expired video URLs, persistent 403 failures, and real format/quality clicks at 100%, 125%, 150% and 200% UI scales.

Install both the desktop app and matching extension, then reload existing website tabs. This package's extension ID is blmjnmockbofpnipcniflmalfmiaolcj. The reported background/*.svg URLs reference omiebabagdcfedkhfmjlndcpocnafcpb, a different extension ID; this package contains no such SVG assets. If those messages persist after reloading, check the extension with that ID for an older or conflicting installation.

Authenticated, restricted, or expired source links can still be refused by the website. A persistent refusal remains a visible error after bounded retries. Windows x64/x86 installers, portable packages, extension ZIP and SHA-256 checksums are included.
