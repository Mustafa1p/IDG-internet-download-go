Client process only; excludes descendants and the test server. Includes measured failed attempts. Missing counters are not zeroes.

| Scenario | Tool | Measured / attempted | Median client CPU seconds | Maximum client peak working set MiB | Maximum client peak commit MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| E | curl | 3 / 3 | 0.281 | 13.016 | 4.074 |
| E | aria2c | 3 / 3 | 0.656 | 29.355 | 18.590 |
| E | Prism | 3 / 3 | 0.484 | 36.938 | 32.914 |
| E | IDG | 3 / 3 | 0.438 | 27.598 | 28.941 |
| F | curl | 3 / 3 | 0.125 | 12.953 | 4.016 |
| F | aria2c | 3 / 3 | 0.281 | 31.000 | 20.309 |
| F | Prism | 3 / 3 | 0.234 | 33.859 | 32.676 |
| F | IDG | 3 / 3 | 0.203 | 26.703 | 28.047 |
| G | curl | 3 / 3 | 0.109 | 13.023 | 4.070 |
| G | aria2c | 3 / 3 | 0.266 | 28.254 | 17.621 |
| G | Prism | 3 / 3 | 0.266 | 31.992 | 29.984 |
| G | IDG | 3 / 3 | 0.094 | 23.051 | 26.750 |

Interpret CPU-time medians with the full trial distribution. CPU cycles are independent hardware-dependent counts, not elapsed time; no conversion to seconds is made.

| Scenario | Tool | Cycle samples / attempted | Median client CPU cycles (millions) |
| --- | --- | ---: | ---: |
| E | curl | 3 / 3 | 1024.790 |
| E | aria2c | 3 / 3 | 2559.826 |
| E | Prism | 3 / 3 | 2113.051 |
| E | IDG | 3 / 3 | 1798.856 |
| F | curl | 3 / 3 | 490.894 |
| F | aria2c | 3 / 3 | 1040.544 |
| F | Prism | 3 / 3 | 1145.501 |
| F | IDG | 3 / 3 | 986.682 |
| G | curl | 3 / 3 | 412.666 |
| G | aria2c | 3 / 3 | 1066.180 |
| G | Prism | 3 / 3 | 1051.410 |
| G | IDG | 3 / 3 | 800.511 |
