| Scenario | curl (n=1) | aria2c (n=3) | Prism (n=3) | IDG 1.6.8 (x64) (n=3) |
| --- | ---: | ---: | ---: | ---: |
| 1 GiB, 4 MiB/s per connection | 4.00 MiB/s | 63.00 MiB/s | 62.77 MiB/s | 62.79 MiB/s |
| 2 GiB, unlimited localhost | 874.38 MiB/s | 370.69 MiB/s | 1,326.28 MiB/s | 1,687.07 MiB/s |
| 256 MiB, four connections at 2 MiB/s | 2.00 MiB/s | 7.98 MiB/s | 7.99 MiB/s | 7.98 MiB/s |
| 256 MiB, 2% random drops per chunk | Failed (1/1) | Failed (3/3) | 34.49 MiB/s | 42.04 MiB/s |
