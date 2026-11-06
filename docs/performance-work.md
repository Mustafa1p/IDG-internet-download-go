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

## Verification completed

Current candidate: formatting, warning-free clippy and full Rust bin suites on x64 and x86. Each architecture passed 62 app/engine tests and two native-host tests; nine explicit external/tool-dependent tests remain ignored.

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

`idg-get` accepts diagnostic policies `--http1`, `--http2`, `--no-pool`, `--http2-fixed` after the connection count. With no flag it uses the desktop policy. `IDG_TRACE_PROGRESS=1` emits per-second progress; engine logs include negotiated protocol, setup time, active connection limit and queued range count.

The benchmark accepts `--idg-baseline`, `--idg-baseline-mode`, `--idg-mode`, and `--trace-idg`. Every selected mode is retained in commands/metadata. A diagnostic-policy result is not a desktop result unless that policy is also the actual default.

Preserved executables:

- `target/perf-idg-http1-baseline.exe`: original pre-optimization 1.6.5 engine.
- `target/perf-idg-pre-schedule.exe`: corrected probe/idle/integrity candidate before range queueing; diagnostic flags supported.
- `target/perf-idg-queued4.exe`: previous smaller-queue candidate; diagnostic flags supported.

Primary discussion that motivated a tested connection-policy experiment, not proof of our root cause: https://github.com/seanmonstar/reqwest/discussions/2778 .

## Outstanding work and completion audit

The preceding E validation session `28087` is terminal and must not be restarted. The next validation will run A�D and a fresh E batch for the refined build. Runtime checkpoint: `target/performance-active.json`.

- Run a larger interleaved E comparison against curl, aria2c, Prism and the preserved original IDG using the actual default policy.
- Refresh A–D measurements with the selected candidate; confirm peak localhost performance, server-limited throughput, connection caps and fault recovery are preserved.
- Check additional real HTTPS sources and resource costs; do not generalize from one CDN/archive.
- Audit/refine remaining slow tails using retained traces. Consider lower-overhead initial probing and adaptive scheduling only when measurements justify them.
- Validate final packaged artifacts and any published update before claiming release completion. Do not publish an unstable prototype.
- The full objective is unproven; keep the goal active. A winner in one small exploratory batch is not completion.
