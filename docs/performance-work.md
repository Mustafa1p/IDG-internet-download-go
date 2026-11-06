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
