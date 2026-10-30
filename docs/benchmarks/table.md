| Scenario | curl (n=1) | aria2c (n=3) | Prism 1.1.7 (n=3) | IDG 1.6.1 (n=3) |
| --- | ---: | ---: | ---: | ---: |
| 1 GiB, 4 MiB/s per connection | 4.00 MiB/s | 63.04 MiB/s | 62.78 MiB/s | 62.96 MiB/s |
| 2 GiB, unlimited localhost | 1,025.42 MiB/s | 377.33 MiB/s | 1,287.33 MiB/s | 1,716.14 MiB/s |
| 256 MiB, four connections at 2 MiB/s | 2.00 MiB/s | 7.98 MiB/s | 7.99 MiB/s | 7.99 MiB/s |
| 256 MiB, 2% random drops per chunk | Failed (1/1) | Failed (3/3) | 30.27 MiB/s | 38.85 MiB/s |
| 76.2 MiB public Rust archive over HTTPS | 52.48 MiB/s | 39.51 MiB/s | 23.62 MiB/s | 16.14 MiB/s |
