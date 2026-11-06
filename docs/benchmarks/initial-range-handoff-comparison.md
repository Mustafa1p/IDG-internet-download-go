# Initial response and handoff comparisons

Engine source: f663941d7e05aa4ce6e2306fe41c5b5f4d508be0. Baseline: the preserved v1.6.7 f8cd699 engine. The current candidate remains unreleased.

All 127 timed files passed exact size and SHA-256 checks. Windows x64 and x86 each pass 90 application tests plus two native-host tests; nine external/tool-dependent tests are explicitly ignored. Both clippy runs deny warnings. All 20 evidence-tool checks pass. Separate compatibility fixtures verify decoded gzip data, short-range-only servers, validators, cancellation, pause, and lease cleanup.

## Controlled request latency

Three alternating trials per tool and case. Values include client startup and durable completion. The fixed source uses nonperiodic SHAKE256 bytes; the 80 ms delay applies to each request.

| Case | Candidate median seconds | v1.6.7 median seconds |
| --- | ---: | ---: |
| 256k-80 | 0.118 | 0.197 |
| 2m-80 | 0.118 | 0.198 |
| 4m-80 | 0.281 | 0.443 |
| 4m-capped3-80 | 1.052 | 1.126 |
| 8m-straggler025-80 | 1.407 | 2.205 |
| 4m-zero | 0.039 | 0.037 |
| 4m-manual4-capped3-80 | 0.620 | 0.621 |

The slow-first case gives the first range 0.25 MiB/s and its peers 3 MiB/s. The candidate flushes its initial cursor and redistributes the unfinished bytes instead of waiting for that slow stream. The zero-latency case shows no demonstrated improvement; its absolute startup variation is retained. Explicit multi-connection counts keep the legacy metadata path.

## Large files and injected drops

Three alternating trials per tool for each B/D configuration, followed by one fixed seven-round manual D cohort. Values are median MiB/s.

| Cohort | Case | Candidate | v1.6.7 |
| --- | --- | ---: | ---: |
| manual-bd | B | 1679.92 | 1642.15 |
| manual-bd | D | 40.61 | 42.04 |
| auto-bd | B | 1533.65 | 1508.94 |
| auto-bd | D | 77.61 | 78.12 |
| manual-d-7 | D | 42.09 | 42.00 |

The descriptive ten-trial manual D aggregate is 41.86/42.02 MiB/s (candidate/baseline). The first cohort favored the baseline and the additional cohort was close. No new large-file speed gain is established. The Prism fixture seed selects file bytes; injected faults use fresh unseeded fastrand calls, so clients do not receive a paired fault sequence. All samples remain separate and referenced.

## Public HTTPS

Each file matched its pinned hash and official publisher metadata. The Rust archive is the workload named in the supplied comparison image. Three trials per tool; values are median MiB/s. Competing multi-connection tools use the manual workload count in the first table.

| Tool | Rust archive, manual configuration |
| --- | ---: |
| curl | 55.46 |
| aria2c | 45.41 |
| Prism | 24.82 |
| IDG | 48.81 |
| IDG-baseline | 48.09 |

| Automatic configuration | Candidate | v1.6.7 |
| --- | ---: | ---: |
| E | 49.78 | 43.93 |
| F | 40.38 | 35.79 |
| G | 21.94 | 24.30 |

The fixed additional seven-round Python cohort measured 22.11/22.54 MiB/s. The descriptive ten-trial aggregate is 22.07/22.57; median completion is 1.217/1.190 seconds. The candidate retained its range response and planned 13 connections; it did not fall back to a single stream. Setup times varied, and these results do not establish a cause for the difference.

## Remaining work

Curl led the manual Rust comparison. The automatic Rust/Node results improved, while Python remained slightly slower in the descriptive aggregate. The requested broad speed goal is not achieved. Before release acceptance, isolate whole-stream versus segmented setup cost using the same transport, segment worker and writer; correct reproducible losses and rerun the affected checks. Do not choose a new policy from only its winning cases.

The exact executable hashes, configurations, raw-file hashes, publisher references and every cohort summary are in [the machine-readable comparison](initial-range-handoff-comparison.json). All raw results, logs and immutable reports remain under the referenced target paths. These are same-machine CLI engine measurements, not packaged GUI timings. Release-window, tray, cold-start and all-computer verification remain incomplete. Public v1.6.7 remains the last verified release; no new version has been published from this branch.
