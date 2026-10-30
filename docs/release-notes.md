IDG v1.6.1 - faster local transfers and stronger reconnects

- Bounded disk writer with acknowledged offset writes, buffer reuse and best-effort NTFS/ReFS sparse setup.
- Connection-limit adaptation: refused ranges queue behind accepted connections instead of exhausting retries.
- Short received buffers survive interrupted streams; productive reconnects reset the consecutive-failure budget.
- Preserve byte-range/server-validator checks, existing files, shared speed limits, pause/cancel and durable completion.

Same-machine, SHA-256-verified engine benchmarks against Prism 1.1.7:

| Scenario | Prism median | IDG median |
| --- | ---: | ---: |
| 2 GiB unlimited localhost | 1,287.33 MiB/s | 1,716.14 MiB/s (+33.3%) |
| 256 MiB with random connection drops | 30.27 MiB/s | 38.85 MiB/s (+28.3%) |
| Four connections at 2 MiB/s | 7.99 MiB/s | 7.99 MiB/s |
| Public HTTPS archive | 23.62 MiB/s | 16.14 MiB/s |

Three trials per engine and scenario on one Windows machine; all successful downloads
verified by SHA-256. Timing includes startup and file sync. These are scoped engine
measurements, not a universal internet speed claim. Full method, every trial, logs
and reproduction commands: https://github.com/Mustafa1p/IDG-internet-download-go/blob/main/docs/benchmarks.md

Windows x64/x86 installers, portable packages and the matching browser extension are included.
Optional subscription-service music audio remains experimental/unverified as documented
in the preceding release. No new music-service compatibility is claimed.
