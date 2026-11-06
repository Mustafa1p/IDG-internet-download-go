# Active performance work — 6 November 2026

The goal is to make IDG (the download manager in the referenced benchmark image) better and faster than the compared tools. This goal remains active. Three exploratory wins do not prove superiority across workloads or all computers. No performance release has been published.

## Current production candidate

- Reusable HTTP/1 connections for range downloads. Pooled HTTP/2 remains available through diagnostic CLI flags but is not the default: the pooled candidates had unacceptable tail delays and timeouts.
- A bounded queue of smaller ranges: at most eight work units per configured active connection, with a 512 KiB target minimum. The existing connection gate still bounds concurrent requests. Single-stream/non-range/one-use transfers are not split.
- Bounded retries for transient range-probe failures. A controlled first-503 failure previously downgraded 16 workers to one; it now recovers. The one-byte response is consumed and legal empty HTTP/2 frames are accepted.
- Cancellation interrupts stalled probes, headers and bodies. The regression failed before the fix and passes now.
- A 15-second network idle watchdog for verified resumable ranges. Validated short buffers are acknowledged before reconnecting. User pauses, rate limiting and disk writes do not count as network inactivity. Non-replayable streams keep the longer timeout and are not replayed.
- Completion requires the actual acknowledged byte count to match a known source length. HEAD/GET size changes previously produced falsely completed padded files; shorter and larger changed responses now fail visibly. Unknown-length successful streams receive their actual final size.
- Shared authenticated/default client settings; cookies and validators remain attached during reconnection.
- Permanent authorization, representation and destination errors stop without useless retries. The engine owns its workers in a JoinSet, aborts and drains siblings on failure, and keeps internal cleanup distinct from user cancellation.
- A new single-GET metadata candidate removes the separate HEAD round trip. A complete one-byte partial response validates both size and range support; ignored ranges reuse the full response, rejected/malformed ranges fall back to a plain full stream, and exhausted inconclusive probes can fall back without replaying an accepted body. The probe deadline does not become a timeout on a reused full stream. Mail exports and token POSTs retain their dedicated one-request paths.
- Concurrent native file transfers now share FIFO admission by HTTP origin, in addition to each file's connection gate. Explicit HTTP 429/503 refusals reduce shared capacity; existing accepted bodies retain their slots, and queued/cancelled/granted-but-unobserved reservations clean up through ownership. Response leases cover reused full streams and one-use exports. Registry entries are weak and contain origins only, with no headers, credentials, path/query strings or persistent learned idle history.
- Pausing a verified resumable range now flushes its validated buffer, closes its request and releases both file/server admission slots. Resume requests begin at the acknowledged cursor with the existing validator and credentials. Intentional pauses do not spend retry attempts. Accepted one-use POST/generated streams remain open while paused and are not replayed.

## Verification completed

Current candidate: formatting, warning-free clippy and full Rust bin suites on x64 and x86. Each architecture passed 62 app/engine tests and two native-host tests; nine explicit external/tool-dependent tests remain ignored.

The later single-GET candidate adds three tests, bringing each architecture to 65 app/engine and two native-host tests. New socket checks validate one GET with no HEAD, malformed/overlong range fallback, empty-probe retry, non-success response rejection, reused full streams outliving the probe deadline without replay, and incomplete full streams refusing completion. Both architectures pass warning-free clippy. Detailed logs: `target/perf-single-probe-x64-tests.log`, `target/perf-single-probe-x86-tests.log` and corresponding clippy logs.

The shared-capacity candidate passes 70 app/engine and two native-host tests on each architecture, with warning-free x64/x86 clippy. Nine explicit external/tool-dependent tests remain ignored. Logs: `target/perf-shared-host-x64-tests.log`, `target/perf-shared-host-x86-tests.log` and corresponding clippy logs.

Its new socket regression failed before the fix: a first file held all four allowed server slots, while a second exhausted its short probe retries and failed with HTTP 503 in roughly 150 ms. The first file then completed, proving that waiting rather than failing was possible. Reproduction: `target/perf-concurrent-slots-reproduction.log`. After the fix, both files queue and save exact bytes. The strengthened test covers direct requests with independent authentication headers, a second URL redirecting from another origin to the busy server, and cancellation of a third queued file without any request/output file. Verification: `target/perf-concurrent-slots-verification.log`. Unit checks cover FIFO admission, reduced limits while existing bodies remain admitted, queued cancellation, unobserved-grant cancellation and origin isolation. A separate socket test holds capacity for 400 ms with a 50 ms probe deadline, then verifies one successful range probe without consuming network retry time while waiting.

The subsequent pause regression also failed before the fix: a paused resumable file kept all learned server slots and a second file could not progress until resume (`target/perf-paused-capacity-reproduction.log`). After the fix, the second file completes while the first stays paused, and the first then resumes with exact file bytes (`target/perf-paused-capacity-verification.log`). A separate socket test verifies both HTTP/1 and HTTP/2, confirmed disk-acknowledged prefixes, retained Cookie/If-Range headers, zero failure retries across pause/resume, and one-use POST streams staying open with exactly one POST. The old pre-acknowledgement pause fixture now correctly expects a restart from offset zero; the strict acknowledged-prefix restart is tested independently. Full pause candidate checks pass 72 app/engine plus two native-host tests per architecture, with clean x64/x86 clippy; nine external/tool-dependent checks remain ignored. Logs: `target/perf-pause-{x64,x86}-{tests,clippy}.log` and `target/perf-paused-segment-tests.log`.

The HEAD-size mismatch regression now verifies exact fresh GET bytes and final size: HEAD is no longer queried, so an inconsistent HEAD length cannot pad the file. This deliberately supersedes the earlier behavior of rejecting HEAD/GET size differences; the acknowledged-byte completion guard still applies to the actual GET representation, with a separate truncated-stream regression and existing changed-validator/range checks.

New real-socket regressions cover cancellation in all network phases; range-probe recovery; exact HTTP/1 and HTTP/2 bytes; stalled-header/body reconnection with retained cookies/If-Range; short-buffer resume at the acknowledged offset; pauses; speed limits; bounded persistent failures; changed full-response lengths; and fast workers claiming queued work while the first slow range is pending. A malformed-range/stalled-siblings regression failed before the new terminal-error/JoinSet handling and now returns within about 0.01 seconds. Read-only destinations and HTTP 403 make one request; HTTP 408 still retries successfully over both HTTP/1 and HTTP/2.

The optimized CLI also passed a black-box size-change test: known 4096-byte source versus 2048/8192-byte GET bodies fails; an unchanged 4096-byte file completes exactly. Reproduction and verification are retained in `target/perf-size-change-reproduction.json` and `target/perf-size-change-verification.json`.

## Retained experiments

All numbers below are successful-transfer medians. Failures are retained and disqualify a candidate; never present a successful-only median as an overall reliability result.

| Experiment | Candidate | Reference / result |
| --- | --- | --- |
| Fresh HTTP/1 baseline, E, n=3 | 14.67 MiB/s, 3/3 | Prism 24.77, aria2c 33.76, curl 53.89 |
| Pooled HTTP/2, E, n=7 | 26.61 MiB/s, 6/7 | One 90-second timeout; baseline 15.83, 7/7 |
| Pooled HTTP/2 with idle recovery, E, n=7 | 35.91 MiB/s, 5/7 | Two 90-second timeouts; aria2c 40.19, 7/7 |
| Independent HTTP/2, E, n=3 | 15.95 MiB/s, 3/3 | aria2c 36.04, Prism 27.37; not selected |
| HTTP/1 queue up to 4 work units/connection, E, n=3 | 28.48 MiB/s, 3/3 | Static same-policy reference 10.48; aria2c 37.27; Prism 24.79 |
| HTTP/1 queue up to 8 work units/connection, E, n=3 | 39.41 MiB/s, 3/3 | Previous queue 30.66; aria2c 34.07; Prism 26.49 |
| Actual default policy, E, n=7, commit 726f23d | 45.99 MiB/s, 7/7 | Original IDG 12.86; curl 42.31; aria2c 39.95; Prism 25.04; all 7/7 |
| Terminal-error refinement, E, n=7, commit 022d640 | 47.20 MiB/s, 7/7 | Original IDG 15.84; curl 50.44; aria2c 42.32; Prism 25.67; all 7/7 |

The n=7 E result led this same-machine batch. The later terminal-error/JoinSet refinement preserves the scheduler and transport policy, but must be measured as its own build; do not silently combine source identities. Broader verification remains required. Internet conditions varied substantially, including curl stalls/timeouts. No significance claim is made.

Raw files:

- `target/perf-20261106-baseline-e.json`
- `target/perf-20261106-interleaved-e.json` and `...-summary.json` (terminal session 72900)
- `target/perf-20261106-local-bd.json` (terminal session 97523): B approximately tied; one old-candidate D probe downgrade timed out, subsequently reproduced and fixed.
- `target/perf-20261106-recovery-e.json`: explicitly aborted after the empty-frame probe-reader regression; retained, not counted as a valid comparison.
- `target/perf-20261106-recovery-v2-e.json` and `...-summary.json` (terminal session 16212)
- `target/perf-20261106-independent-e.json` (terminal session 47982)
- `target/perf-20261106-queued-http1-e.json` (terminal session 68056)
- `target/perf-20261106-queued8-http1-e.json` (terminal session 81905)
- `target/perf-20261106-validation-e.json` and `...-summary.json` (terminal session 28087), all 35 trials recorded for commit 726f23d.

Each run has a fresh destination, exact-size/SHA-256 verification, rotated tool order, retained logs and executable hashes. The harness now captures complete candidate source/patch snapshots. Do not overwrite prior results. Preserve all failed trials.

## Diagnostics and reproducibility

The current `022d640` pipeline is terminal: all 52 A–D trials and 35 E trials are retained. IDG passed 12/12 local and 7/7 HTTPS transfers; original IDG and Prism also passed all their trials. Curl and aria2c failed their deliberately interrupted D trials. The complete reports are `target/perf-20261106-validation-local-v2-report` and `target/perf-20261106-validation-final-e-report` (raw data, logs, source snapshots and inspected charts).

| Current build workload | IDG | Original IDG | Prism | aria2c | curl |
| --- | ---: | ---: | ---: | ---: | ---: |
| A, per-connection limit, n=3 (curl n=1) | 62.89 | 62.96 | 62.74 | 62.98 | 4.00 |
| B, localhost, n=3 (curl n=1) | 1593.21 | 1623.33 | 1228.05 | 364.73 | 832.34 |
| C, four-connection cap, n=3 (curl n=1) | 7.98 | 7.98 | 7.99 | 7.98 | 2.00 |
| D, random drops, n=3 (curl n=1) | 42.20 | 38.03 | 35.53 | 0/3 verified | 0/1 verified |
| E, official Rust HTTPS, n=7 | 47.20 | 15.84 | 25.67 | 42.32 | 50.44 |

Rates are successful medians in MiB/s. This batch supports a large improvement on E versus original IDG, but curl leads E and original IDG is about 1.9% ahead on B. A/C are effectively server-limit ties. The first E candidate transfer took 8.784 seconds, retained in the seven samples; its trace showed a gradual throughput ramp, rather than a silent idle tail. Do not substitute the prior batch where curl was slower or discard this slow sample. The full objective is not achieved.

The broader-host run `target/perf-20261106-independent-hosts-fg.json` is terminal (session 5861, all 30/30 files verified). F (Node) medians: IDG 43.06, original IDG 42.91, curl 44.64, aria2c 43.36, Prism 35.55 MiB/s. G (Python): IDG 18.58, original IDG 19.53, curl 19.02, aria2c 18.88, Prism 16.80 MiB/s. Resource counters succeeded for every attempt; IDG's maximum client working set was 24.93/22.37 MiB on F/G versus Prism's 34.83/33.23 MiB, while curl remained lower at about 13 MiB. IDG's client CPU median on F was higher than original IDG (0.234 versus 0.141 seconds). Preserve these limits rather than claiming across-the-board superiority. Complete report: `target/perf-20261106-independent-hosts-fg-report`.

The single-GET change has a separate controlled comparison in `target/perf-20261106-get-probe-latency.json`. Both release executables transferred exact 4 MiB files on a local server with 80 ms per-request latency, three interleaved trials each. Median completion fell from 0.359759 to 0.283138 seconds (about 21.3%); requests fell from ten to nine. All six hashes passed. This isolates the removed metadata round trip, not a general internet speed claim. Reproduce with `scripts/benchmark_probe_latency.py` and the preserved `target/perf-idg-022d640.exe` (SHA-256 `0a0137e11aa5ea5485a3030c3fcb6f28c7096d1d276bf283dccbf20b2890285a`). Real E/F/G verification of the new source remains required.

Single-GET source `00d30a8` completed that broader verification in terminal session 42285: all 105 E/F/G trials and all 12 A–D candidate rechecks passed exact size/hash verification. E/F/G candidate medians were 45.86/41.59/20.81 MiB/s, versus queued-HEAD build 022d640 at 44.66/44.72/20.10 and curl at 53.84/42.81/19.17. Local candidate medians were A 62.90, B 1604.19, C 7.98 and D 42.25 MiB/s. Results are mixed; do not turn the controlled latency improvement into a universal internet claim. F's median was lower but its mean completion improved from 0.8511 to 0.8202 seconds; E's mean was roughly tied. Samples overlap. Setup medians E/F/G fell from 227/173/381 ms to 213/130/310 ms. Reports: `target/perf-20261106-single-get-efg-report-v2` and `target/perf-20261106-single-get-local-report`.

Further transport diagnostics kept the same executable and scheduler. Pooled adaptive HTTP/2 (session 26905) passed 63/63 files, but E/F/G medians were 28.74/19.40/12.80 versus HTTP/1 at 49.27/41.79/21.94. Fixed HTTP/2 windows (2 MiB stream, 32 MiB connection, adaptive disabled; session 40867) passed 27/27 but remained slower: 40.14/29.71/16.91 versus HTTP/1 at 52.82/46.11/20.76. Negotiated HTTP/2 was confirmed in logs. Both policies are rejected for the production default; HTTP/1 remains selected. Raw files and resource reports: `target/perf-20261106-queued-h2-vs-h1*` and `target/perf-20261106-queued-fixed-h2-vs-h1*`.

Connection diagnostics (terminal session 93778) passed 27/27 each. Auto versus explicit 16: E 47.34/54.47, F 37.49/35.63, G 19.88/22.14 MiB/s. One connection versus 16: E 42.16/49.28, F 40.50/41.71, G 20.24/19.35. These small samples do not justify a blanket connection-policy change. Exact overrides are captured in raw data and table headings, and mixed settings cannot be pooled into one median. The benchmark report suite now has nine tests; all 18 evidence-tooling tests pass. `target/perf-20261106-auto-vs16*` and `target/perf-20261106-single-vs16*` retain all samples.

The report generator now accepts the actual tool set/repeat counts, including preserved baseline binaries and E-only seven-round batches. It rejects incomplete inputs, missing/duplicate trials, mixed transport policies, mixed source files and inconsistent size/hash/timing claims, including under Python optimization. Partial-success medians always show their verified fraction and cannot produce comparison bars. Outputs require a new directory and retain raw data, logs, source snapshots and identity metadata. Eight focused regression tests pass; the original A–E report reproduces unchanged numeric results, and both accepted/rejected seven-round E batches have been exercised. The chart was inspected and its subset labels corrected to avoid clipping. Optional resource tables include measured failed attempts and show counter coverage; missing counters are never zeroes.

Example for a terminal run: `python scripts/benchmark_report.py target/perf-20261106-validation-final-e.json --output target/perf-20261106-validation-final-e-report --scenarios E --idg-label "IDG 022d640"`. Do not run it until `finished_utc` is set and the expected trial count is present.

`scripts/process_metrics.py` provides Windows process-lifetime CPU, peak working-set and peak commit counters from the owned process handle, without a polling thread. A real child allocating 32 MiB and spending CPU verifies the counters remain available after exit; a bad handle verifies errors remain explicit rather than invented zeroes. Both tests pass. These counters are now integrated into the harness via `benchmark_process.py`, outside timing, and label their scope as client-only (excluding child processes and the test server). A reduced-size localhost smoke passed all five clients with correct hashes and no resource-query errors (`target/perf-20261106-harness-resource-smoke.json`); it is an integration check, not a full-size throughput comparison. Earlier A–E results contain no resource counters. The release workflow includes these evidence regressions; hosted execution remains unverified.

Additional independent HTTPS sources are integrated through `scripts/benchmark_sources.py`: pinned official Node 24.11.0 Windows x64 ZIP (36,356,854 bytes) and Python 3.13.0 Windows x64 installer (28,160,096 bytes; download only), preserving Rust E. Publisher metadata and HEAD sizes/range support were checked; a fresh official-metadata check for all three sources passed. Three source-validation tests reject wrong hashes, wrong filenames, duplicate entries and wrong digest algorithms. These are fixed test artifacts, not software-version recommendations. Node/Python transfer measurements are the next step.

`scripts/benchmark_process.py` is the timed-subprocess helper for that integration. Four real subprocess tests cover exact successful output, persistent partial-file/log evidence after failure and forced timeout, explicit fsync failures, and resource queries excluded from transfer timing. Those pass locally; the release workflow includes them. All 17 evidence-tooling tests pass. The engine and its executable remain unchanged from the validated candidate; harness integration occurred after session 30795 terminated.

`idg-get` accepts diagnostic policies `--http1`, `--http2`, `--no-pool`, `--http2-fixed` after the connection count. With no flag it uses the desktop policy. `IDG_TRACE_PROGRESS=1` emits per-second progress; engine logs include negotiated protocol, setup time, active connection limit and queued range count.

The benchmark accepts `--idg-baseline`, `--idg-baseline-mode`, `--idg-mode`, and `--trace-idg`. Every selected mode is retained in commands/metadata. A diagnostic-policy result is not a desktop result unless that policy is also the actual default.

Preserved executables:

- `target/perf-idg-http1-baseline.exe`: original pre-optimization 1.6.5 engine.
- `target/perf-idg-pre-schedule.exe`: corrected probe/idle/integrity candidate before range queueing; diagnostic flags supported.
- `target/perf-idg-queued4.exe`: previous smaller-queue candidate; diagnostic flags supported.

Primary discussion that motivated a tested connection-policy experiment, not proof of our root cause: https://github.com/seanmonstar/reqwest/discussions/2778 .

## Outstanding work and completion audit

Earlier performance sessions 28087, 30795, 5861, 42285, 26905, 40867 and 93778 are terminal. Their raw JSON outputs have `finished_utc` and expected counts. Those transport/connection diagnostics measure engine source 00d30a8; later shared-admission and pause changes are documented separately below. Do not restart terminal runs from an old checkpoint. Runtime checkpoint: `target/performance-active.json`.

The shared-admission build f889a37 completed session 15376 as well: 10/10 B comparison trials, 9/9 A/C/D candidate trials and 27/27 HTTPS comparisons verified exact files. B median was 1594.52 MiB/s versus preserved single-GET build 00d30a8 at 1599.63; A/C/D candidate medians were 62.89/7.98/41.18. E/F/G medians were 45.15/40.47/20.02, versus 38.36/41.57/20.84 for the preserved build and 43.78/41.94/19.72 for curl. These three-round HTTPS samples vary substantially: curl's 17.764-second Python trial and IDG's 3.770-second Python trial are retained. Do not generalize from the winning median on E or hide the slower medians on F/G. Raw files: `target/perf-20261106-shared-host-{b,acd,efg}.json`. These timings precede the pause change and do not measure that later binary.

Concurrent-file and pause failures are now reproduced and fixed as described above. Next audit transport/header handling across origins before release: authenticated downloads currently build separate default-header clients, which may prevent reuse across files and merit an origin-bound credential regression. This is a candidate concern, not a reproduced leak. Use two controlled local origins with dummy Authorization/Cookie/CSRF headers; require same-origin redirects to retain them and cross-origin probe/range requests to omit them, while preserving complete file bytes and the requested User-Agent/referrer. Do not claim this fixed or alter credential forwarding without the real redirect test. Preserve one-use POST redirect semantics, shared admission and network deadlines if refactoring request execution. Desktop runtime, final package/deployment verification and release remain outstanding.

- Run a larger interleaved E comparison against curl, aria2c, Prism and the preserved original IDG using the actual default policy.
- Refresh A–D measurements with the selected candidate; confirm peak localhost performance, server-limited throughput, connection caps and fault recovery are preserved.
- Check additional real HTTPS sources and resource costs; do not generalize from one CDN/archive.
- Audit/refine remaining slow tails using retained traces. Consider lower-overhead initial probing and adaptive scheduling only when measurements justify them.
- Validate final packaged artifacts and any published update before claiming release completion. Do not publish an unstable prototype.
- The full objective is unproven; keep the goal active. A winner in one small exploratory batch is not completion.

## Origin-scoped authenticated requests (unreleased)

Two local origins and dummy credentials reproduced a forwarding bug: after a redirect, Authorization and Cookie reached the second origin on four data requests each; X-CSRF-Token reached it on the probe and four data requests. The original failure is retained in `target/perf-origin-credentials-reproduction.log`. No real credentials were used.

`download/request.rs` now executes each redirect explicitly over the shared transport. Captured credentials remain bound to the source origin; same-origin requests retain them, and cross-origin probe/data requests omit them. Independent files retain separate headers while reusing the same connection pool. Each redirect releases the prior origin lease before waiting for the next origin. Admission wait does not consume network deadlines. Cancellation, pause and invalid redirects release capacity. The updater uses the same redirect path with its existing body deadlines.

Runtime regressions verify exact file bytes, credential isolation, explicit browser User-Agent/referrer, URL Basic authentication, and automatic referrer sanitization. Two authenticated files use one TCP connection instead of two without exchanging their headers. POST 301/302/303 redirects switch to GET; 307/308 preserve the form body. Loops, missing Location and non-HTTP targets fail without replaying the original form request. Updater cross-origin redirects retain size/checksum checks and body timeouts.

Both Windows x64 and x86 passed 79 app tests plus two native-host tests (81 each), with nine external/tool-dependent tests explicitly ignored. Both passed all-target clippy with warnings denied; formatting passed. Logs: `target/perf-scoped-http-tests.log`, `target/perf-scoped-http-x86-tests.log`, and corresponding clippy logs. These checks validate the source, not a packaged/deployed release. Earlier performance measurements precede this change; rebuild and measure the finalized binary before making performance claims. Public stable remains v1.6.5.

## v1.6.6 candidate verification

Versioned build cfbe8f2 uses the origin-scoped engine 93e0fde. Its isolated comparison session44954 is terminal: all35 E transfers and all18 F/G transfers passed exact-size/SHA-256 checks. No compilation, GUI tests or competing artifact downloads ran during that comparison.

E seven-round medians (MiB/s): IDG55.93, original1.6.5 IDG17.70, curl43.33, aria2c37.97, Prism29.43. Rotated order and every slow sample are retained; curl's early57+ and later34-43 samples illustrate temporal variation. This batch is not evidence of universal superiority. Independent Node/Python three-round medians were IDG37.60/18.98, preserved f889a37 build38.97/18.67, curl42.61/18.86. The Node median remains slower than curl and slightly below the prior candidate.

The earlier local session10832 passed24/24 files: candidate versus f889a37 medians A62.98/62.91, B1612.42/1526.90, C7.98/7.98, D42.35/41.55 MiB/s. Its preceding27-file HTTPS batch also verified all files but overlapped desktop compilation. Preserve those timings with the explicit caveat in target/perf-20261106-scoped-http-efg-caveat.json; do not use that batch for speed conclusions.

Raw final files: target/perf-20261106-v166-isolated-{e,fg}.json and target/perf-20261106-scoped-http-local.json. Immutable sibling -report directories retain original logs, source snapshots, binary identities, resource counters and failures. The performance goal remains unproven.

Runtime GUI checks passed all five media size/scale cases and54 interactions on the isolated development build. Real Edge fixture tests passed four-site overlay interactions, homepage mutation/navigation and the closed-tab badge race. Release preview invocation did not test the dialog: that harness is deliberately compiled out of release binaries. Computer Use native-pipe recovery failed, and an existing development process was left untouched. Release-window/tray/cold-start checks therefore remain incomplete. See docs/verification-v1.6.6.md. Hosted CI, actual package integrity and final published version still require completion before claiming release.

## URL authentication without a username (unreleased follow-up)

A controlled server reproduced another authentication failure: a URL with an empty username and a password authenticated the one-byte probe, then all four resolved data ranges omitted Authorization and returned HTTP 401. The failing engine log is target/perf-empty-user-auth-reproduction.log (one accepted request, four rejected requests). Dummy credentials and a local source were used.

RequestContext now captures URL authentication when either a username or a password is present, matching the shared transport's URL parsing. The new real-engine regression completes exact bytes for empty/nonempty usernames, percent-encoded credentials, username-only credentials and explicit Authorization overrides. The redirect regression also exercises empty and encoded credentials: they remain on the source origin and are omitted at another origin; automatic referrers omit credentials and cross-origin query/path secrets.

The fixed tests pass on Windows x64 and x86: 80 app tests plus two native-host tests per architecture, with nine external/tool-dependent checks explicitly ignored. Both all-target clippy runs deny warnings and pass; formatting and whitespace checks pass. Verification logs: target/perf-empty-user-auth-verification.log, target/perf-url-auth-origin-verification.log and target/perf-url-auth-{x64,x86}-{tests,clippy}.log. This follow-up has not been packaged or released; public v1.6.6 remains the previously verified c2d202e build. This is a reliability fix, with no new throughput claim.

## TCP socket limits without HTTP refusal responses (unreleased follow-up)

A real local listener capped physical TCP sockets at four, kept the first file's four responses open, and closed excess connections without an HTTP response. The second file exhausted all three range probes and its full-stream fallback before the first file released capacity. The first file still completed. Evidence: target/perf-tcp-cap-reproduction.log.

The shared admission now counts successful response headers separately from pending requests. Typed reset/refusal/EOF errors can reduce admission to healthy peer responses; a later successful handshake proves additional usable capacity. A closed request with no healthy peer cannot infer a cap. The error classifier uses the existing Hyper error type to recognize clean incomplete-header EOFs without parsing error text. Hyper was already in Cargo.lock; the manifest now declares this direct API use without adding/upgrading any locked package.

Both physical-reset and clean-EOF tests now queue the second file, then complete both exact files. A third queued file cancels without a server request or output file. Malformed HTTP does not infer a capacity limit, a closed form request is sent once, and lease bookkeeping returns to zero. Full suites pass 83 app tests plus two native-host tests on each Windows architecture; nine external/tool-dependent tests remain ignored. Clippy denies warnings on both architectures, and formatting passes. Logs: target/perf-tcp-cap-{all-closures,cancel-verification,negative-verification,bookkeeping,x64-tests,x86-tests,x64-clippy,x86-clippy}.log.

Throughput verification against the preserved v1.6.6 engine is next. This source has not been packaged or released; do not substitute source test results for a published update.

### Refinement: isolated failures, recovered capacity and waiting forms

A new counterexample rejected the first monotonic TCP-cap prototype: one isolated closed connection caused a valid retry to wait behind an unrelated healthy stream (target/perf-tcp-cap-single-reset-reproduction.log). The candidate was not released. Its executable remains target/perf-idg-f681bee.exe, SHA-256 07bcdea539eb5963a0c8a7e398136949df72334986a40a214e1d882ce5419651.

The refined policy requires repeated closures before inferring transport pressure. It admits at most one speculative GET/HEAD connection at a time, with a 50 ms initial delay and exponential backoff capped at one second. Successful headers prove capacity has recovered. Confirmed capacity closures can requeue an idempotent request without consuming its caller's ordinary failure budget; without healthy peers, failure remains bounded. Explicit HTTP 429/503 retains its existing policy.

Forms neither test extra capacity nor replay after ambiguous connection loss. A form waiting at the FIFO head must not prevent a later safe capacity test from waking. That lost-wakeup case was reproduced (target/perf-tcp-cap-form-head-reproduction.log), then fixed by waking both the FIFO head and oldest eligible safe request when admission state changes.

Real-socket tests cover hard RST/EOF caps, queued cancellation, isolated-reset recovery, two initial closures followed by recovered capacity while the first stream remains open, malformed HTTP, one-use form refusal and waiting-form protection. Gate tests verify one trial in flight, safe recovery behind a waiting form, late successful headers and complete lease cleanup. Full Windows x64/x86 suites pass 84 app tests plus two native-host tests each; nine external/tool-dependent tests remain ignored. Clippy denies warnings on both architectures and formatting passes. Logs: target/perf-tcp-adaptive-{x64,x86}-{tests,clippy}.log and target/perf-tcp-cap-{forms-verification,recovery-verification,adaptive-gate-verification}.log.

The next measurement uses a freshly rebuilt CLI for this refined source; the f681bee binary must not be presented as the new candidate. No new throughput result or release is claimed yet.

### Completed throughput and CPU investigation for 7cc7afe

All 68 B/D comparison transfers passed size/SHA-256 checks against the preserved v1.6.6 engine. The first five-round batch enabled progress tracing only on the candidate; its B medians favored the baseline (1550.18 versus 1642.59 MiB/s), and D was 41.53 versus 41.99. These results remain retained separately.

With both programs untraced, the seven-round batch measured B1594.89/1639.39 and D42.17/42.18 (candidate/baseline MiB/s). A further fixed five-round resource-diagnostic batch used the exact same source/executables and added a process cycle query after timing: B1606.72/1539.87, D41.50/41.66. Direction changed despite unchanged executables. The descriptive aggregate of both untraced batches (12 trials per tool/scenario) is B1602.98/1616.68, D41.98/41.75; mean completion times are B1.2787/1.2710 seconds and D6.1492/6.1343. These data do not establish a speed improvement or universal performance equivalence.

Raw files are target/perf-20261106-adaptive-cap-{bd,production-bd,cycles-bd}.json, each with an immutable sibling -report. The descriptive aggregate target/perf-20261106-adaptive-cap-analysis.json records source hashes, instrumentation differences and all input references. The candidate-only traced batch is not pooled with production measurements.

Process-time readings varied sharply for both executables. In the cycle batch, D ranged from 0.266 to 1.969 reported CPU seconds for the candidate, while median CPU cycles were 3.523 billion versus 3.530 billion for the baseline. The CPU-time median alone is not reliable evidence of a CPU regression in this experiment. QueryProcessCycleTime now supplies an independent raw count, retrieved through the owned process handle after timing and process exit. It remains hardware-dependent and is never converted to elapsed seconds; see Microsoft's API definition: https://learn.microsoft.com/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryprocesscycletime . Resource reports include separate cycle coverage, including failed attempts, and reject invalid counters. The real child-process test verifies positive/stable post-exit values and explicit invalid-handle failures. All 18 evidence-tooling regressions pass.

The transport-layout example diagnoses inline future sizes without polling requests or touching files. An experiment boxed contended admission timer state, reducing its own future from 240 to 40 bytes, but request/engine futures remained 1456/4536 bytes. That production edit was reverted; no throughput benefit is claimed. Only the diagnostic remains. Results are target/perf-admission-layout-{before,after}.json. The actual timed candidate remains SHA-256 70e9808e29c314f2740c85668f9d5ff0398fd0e1ecaeb7497dacf95acea55786, engine source 7cc7afe.

The new behavior is verified by the failed-before/passed-after TCP-cap, isolated-reset, capacity-recovery, cancellation and form-safety tests. The performance goal remains active; these reliability fixes do not prove that IDG is fastest across sources or hardware.

### Single-use generated GET intent (pre-publication correction)

A new real-wire regression found that method-only capacity eligibility replayed a generated mail export three times after ambiguous connection loss. Generated exports use GET but must be sent only once. Unpublished candidate c855d6f and its local packages were rejected; public v1.6.6 is not affected by this new adaptive-retry regression.

Request specs now retain explicit replay intent across redirects. Generated GET probes and forms cannot be capacity trials or automatic transport retries, including form redirects to GET for 301/302/303. A queued single-use GET waits for an ordinary slot and is sent once after the healthy stream releases capacity. The regression passes for all four export paths and exact response bytes. Logs: target/perf-one-use-get-{reproduction,verification}.log and target/perf-single-use-queue-verification.log.

Both architectures pass 85 app tests plus two native-host tests, nine ignored, and all-target clippy with warnings denied (target/perf-v167-intent-{x64,x86}-{tests,clippy}.log). Earlier 7cc7afe benchmarks do not measure this final intent change. A fresh comparison and new packages are required before release verification.

### Initial-range reuse: latency gains and rejected slow-tail prototypes

Public v1.6.7 is fully published from f8cd699. Tagged workflow 42294311423 passed both architectures and publishing. All six downloaded asset API digests match, packaged hosts/extension pass, and the production updater verified the actual 115,644,196-byte installer. The supplied video passes MP4/WebM at 480p with audio and MP3 audio. Completed evidence is target/published-v1.6.7/completed-verification.json and PR #3. Packaged GUI/tray/cold-start limitations remain.

A controlled 4 MiB source with 80 ms per request measured v1.6.7 at 0.283 seconds across nine requests, versus curl at 0.117 seconds in one request. With zero added latency, medians were 0.042/0.034 seconds. A one-connection control used two IDG requests and took 0.200 seconds. These isolate request-wave overhead; they are not internet speed claims. Raw evidence: target/perf-20261107-probe-vs-curl*.json.

An unreleased diagnostic reuses a bounded 2 MiB initial partial response as file data. It reserves the retained response's local connection slot before siblings start, validates exact partial headers, plans non-overlapping remaining ranges, and resumes interruption from disk-acknowledged short buffers. Single-use exports/forms retain their original path. Pausing releases resumable capacity; cancellation releases all leases. Small files can complete in one request. Explicit multi-connection counts remain on the production path after the unrestricted prototype regressed a capped 4 MiB/four-connection source from 0.623 to 0.799 seconds.

The restricted automatic prototype measured 2 MiB at 0.121 versus 0.200 seconds; 4 MiB at 0.281 versus 0.444; and capped 4 MiB at 1.046 versus 1.124. However, an 8 MiB source with a 0.25 MiB/s first range and 3 MiB/s siblings regressed to 8.140 versus 2.204 seconds. The larger initial range becomes an uncaught slow tail. Neither prototype is accepted as the production default or released.

All 68 fixed comparison files pass exact size/SHA-256 checks. Both prototype binaries, patches, input hashes, request counts and all losing samples remain in target/perf-20261107-initial-range-analysis.json and referenced files. These measurements predate the later equivalent shared-constant/header-format cleanup. No new production speed gain is claimed.

The next required change is cooperative initial-range handoff: once sibling work finishes, flush and acknowledge the initial stream's buffered cursor, close its response, then redistribute only its unfinished bytes with the same validator. A new stalled-initial-body regression must fail before that change and pass afterward. Recheck pause/cancel, capacity, single-use exports, malformed ranges and exact-byte completion, then repeat both controlled latency/capped/straggler and large B/D comparisons before considering default adoption.

The reusable scripts/benchmark_probe_latency.py now accepts explicit size, connection count, candidate policy, per-connection cap and first-range cap. Its result records every setting and the fixture source hash, rejects nonfinite/negative settings, and preserves existing evidence. Two guard tests join the previous 18 evidence tests; all 20 pass. A recorded three-round rerun reproduces both the automatic 4 MiB latency improvement and the 8 MiB straggler regression; these 12 additional exact files pass, bringing this experiment to 80 verified files. Durable summaries and input hashes are in docs/benchmarks/initial-range-prototypes.json. Both Windows architectures pass 87 app tests and two native-host tests, nine external/tool-dependent tests ignored, and all-target clippy with warnings denied. The experimental policy remains opt-in through the diagnostic CLI; the default desktop path is unchanged.

Reproduce the decisive case with scripts/benchmark_probe_latency.py, the preserved baseline/candidate executables, --candidate-policy reuse-prefix --connections 0 --size-kib 8192 --repeats 3 --latency-ms 80 --per-connection-mib 3 --first-range-mib 0.25 and a new --output path. Do not accept a future candidate from only its fast-source results.

### Cooperative initial-range handoff and compatibility checks (unreleased)

A real-socket test now reproduces the slow-tail issue without relying on internet timing: the initial response delivers 31 KiB plus 19 bytes and stalls while all sibling ranges finish. The old prototype cannot finish before that stream is released. The new worker cooperatively flushes and acknowledges its short buffer, closes the old response, and returns only its unfinished range for redistribution. Ready body data is consumed before considering a handoff. Replacement ranges keep the original validator, connection gate and shared bandwidth limit. Exact byte counts and non-overlapping ranges are checked. File-change, pause and cancellation tests exercise the replacement workers themselves.

An early diagnostic measured the 8 MiB straggler case at 1.406 versus 2.203 seconds, but that executable was subsequently rejected: a gzip-encoded range could silently complete with 93 decoded bytes instead of the full 3,881-byte file. Public v1.6.7 handled that source correctly. The corrected source restores the complete one-byte proof when decoding hides the range length, preserving valid chunked ranges and falling back to a full decoded stream where needed.

A second real source limited requests to 512 KiB and rejected larger ranges with HTTP 403/413. The earlier candidate failed while v1.6.7 completed the exact 4 MiB file. Inconclusive/refused prefixes now restore the original narrow probe. An initial exploratory GET makes one attempt per redirect hop while remaining eligible for safe capacity trials; ordinary GETs retain automatic capacity recovery and single-use requests retain no replay. Typed replay policy keeps those intents separate. Invalid redirects still stop immediately. The final fixture matrix also verifies connection-close and HTTP 503 rejection before completing short ranges.

The default-path run exposed an x86 cancellation race while reserving the retained response's local slot. Early cancellation now returns Cancelled, drops the response lease, and preserves the partial file. Shared range fixtures now correctly clip valid ends beyond EOF. Authentication tests still prove shared transport with separate credentials; source-change tests cover both explicit multi-range and automatic handoff paths.

Both Windows architectures pass 90 app tests plus two native-host tests, nine external/tool-dependent tests ignored, and all-target clippy with warnings denied. All 20 evidence-tool tests pass. The actual corrected CLI passes both gzip comparison files and all eight short-range-limit files with exact size/SHA-256 checks. Logs are target/perf-handoff-cancel-fixed-{x64,x86}-{tests,clippy}.log, target/perf-gzip-prefix-refined-verification.log and target/perf-prefix-cap-verification.log. Durable results and binary/source hashes are in docs/benchmarks/initial-range-handoff.json.

The corrected behavior is now the default on this development branch; the temporary diagnostic engine entry point and CLI flag were removed. Historical benchmark tooling still supports flags used by preserved prototypes. Fresh controlled latency/capped/straggler and large B/D comparisons must measure this corrected source before release acceptance. The latency fixture now generates nonperiodic SHAKE256 bytes outside timing so repeated patterns cannot hide range-offset errors. Its source hash and settings are retained; only expected disconnected-client exceptions are handled, while client exit, size, SHA-256 and sync remain mandatory. Packaged GUI/tray/cold-start checks remain incomplete. Public v1.6.7 is unchanged.

### Completed f663941 comparisons; remaining HTTPS gap

The fresh committed CLI (SHA-256 b5ee403db571ac476b29688bc4ee49e95504e7e7d487fab7a9178645ad19720b) passed all 127 timed transfers against the preserved f8cd699 baseline. Its production entry point is the desktop's engine entry point. All source hashes and raw inputs are checked in docs/benchmarks/initial-range-handoff-comparison.json; the readable report is initial-range-handoff-comparison.md.

At 80 ms per request, median 2 MiB completion improved from 0.198 to 0.118 seconds, 4 MiB from 0.443 to 0.281, and the slow-first-connection case from 2.205 to 1.407. The 4 MiB capped case measured 1.052/1.126 seconds. The zero-latency 4 MiB case measured 0.039/0.037; explicit four-connection capped transfers were similar. Every sample is retained.

Manual B medians were 1679.92/1642.15 MiB/s, while manual D initially favored the baseline at 40.61/42.04. Automatic B/D measured 1533.65/1508.94 and 77.61/78.12. A fixed seven-round additional manual D cohort measured 42.09/42.00; the descriptive aggregate of all ten trials was 41.86/42.02. The test server's file seed does not seed its per-chunk fastrand faults. These results do not establish a new large-file speed gain or universal equivalence.

All 33 first public HTTPS files matched their official publisher hashes. Manual Rust archive medians were curl55.46, aria2c45.41, Prism24.82, candidate48.81 and baseline48.09 MiB/s. Automatic Rust/Node improved to 49.78/43.93 and 40.38/35.79; Python initially lost at 21.94/24.30. A fixed additional seven-round Python cohort measured 22.11/22.54. All ten Python trials give a descriptive 22.07/22.57 MiB/s aggregate, or 1.217/1.190 median seconds. Logs confirm retained-range use and 13 planned connections; setup times vary. The cause of that difference is not established.

Curl still leads the manual Rust workload, so the full speed goal remains unmet. Next isolate one whole GET versus range scheduling with the same transport/segment/writer stack, preserving durable completion and all losing samples. The branch remains unreleased; version/package/publication checks still belong after acceptance. The last independently verified public release remains v1.6.7 f8cd699, confirmed from GitHub's latest stable release metadata. Packaged GUI/tray/cold-start limitations remain.

### Full-GET diagnostic: no consistent speed advantage

The isolated full-GET example uses the same production request, segment and writer modules, with preallocation and durable completion, but deliberately has no range recovery. It is not a new desktop policy. All 36 public Rust/Node/Python transfers passed publisher size/SHA-256 checks. Every path used HTTP/1.1; curl was explicitly restricted to it. Three rotating repetitions cover only three of four order positions, and public-network variation remains material.

Median completion seconds (automatic/manual/full GET/curl) were Rust 1.566/1.414/1.551/1.446, Node 0.755/0.695/0.859/0.795, and Python 1.023/1.051/1.235/0.900. These results do not support replacing segmentation with one full stream or establish universal superiority. Earlier curl cohorts used different options and are not pooled with this experiment.

The actual diagnostic executable also passed six real-socket cases: decoded gzip, redirected gzip, refusal of an unsolicited partial response, refusal of truncated completion, browser-challenge rejection before file creation, and preservation of an existing destination. Both architectures passed all-target clippy. PR workflow 42309795406 passed both Windows builds and its applicable integration checks. Durable raw trials, source/binary hashes, limitations and runtime cases are retained in docs/benchmarks/full-get-diagnostic.json. Production download source remains f663941; public release remains v1.6.7.

The next scheduling experiment should isolate range request-wave cost while retaining a slow-worker regression and short-range compatibility checks. No production schedule change is justified by this diagnostic alone.
