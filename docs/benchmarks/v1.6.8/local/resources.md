Client process only; excludes descendants and the test server. Includes measured failed attempts. Missing counters are not zeroes.

| Scenario | Tool | Measured / attempted | Median client CPU seconds | Maximum client peak working set MiB | Maximum client peak commit MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| A | curl | 1 / 1 | 1.641 | 10.375 | 3.277 |
| A | aria2c | 3 / 3 | 1.359 | 31.594 | 23.809 |
| A | Prism | 3 / 3 | 0.891 | 36.000 | 33.473 |
| A | IDG | 3 / 3 | 2.406 | 24.203 | 30.355 |
| B | curl | 1 / 1 | 1.359 | 10.426 | 3.277 |
| B | aria2c | 3 / 3 | 2.781 | 22.879 | 15.332 |
| B | Prism | 3 / 3 | 2.031 | 109.668 | 136.652 |
| B | IDG | 3 / 3 | 1.516 | 31.953 | 31.992 |
| C | curl | 1 / 1 | 0.703 | 10.371 | 3.270 |
| C | aria2c | 3 / 3 | 0.531 | 19.078 | 10.125 |
| C | Prism | 3 / 3 | 0.734 | 19.914 | 14.215 |
| C | IDG | 3 / 3 | 0.844 | 17.434 | 13.750 |
| D | curl | 1 / 1 | 0.031 | 10.438 | 3.277 |
| D | aria2c | 3 / 3 | 0.406 | 25.250 | 16.398 |
| D | Prism | 3 / 3 | 0.406 | 32.316 | 32.031 |
| D | IDG | 3 / 3 | 0.156 | 22.953 | 28.832 |

Interpret CPU-time medians with the full trial distribution. CPU cycles are independent hardware-dependent counts, not elapsed time; no conversion to seconds is made.

| Scenario | Tool | Cycle samples / attempted | Median client CPU cycles (millions) |
| --- | --- | ---: | ---: |
| A | curl | 1 / 1 | 8785.288 |
| A | aria2c | 3 / 3 | 5565.050 |
| A | Prism | 3 / 3 | 8365.994 |
| A | IDG | 3 / 3 | 7006.823 |
| B | curl | 1 / 1 | 5191.370 |
| B | aria2c | 3 / 3 | 11668.676 |
| B | Prism | 3 / 3 | 8841.940 |
| B | IDG | 3 / 3 | 6581.648 |
| C | curl | 1 / 1 | 3224.945 |
| C | aria2c | 3 / 3 | 2600.523 |
| C | Prism | 3 / 3 | 3034.403 |
| C | IDG | 3 / 3 | 3270.767 |
| D | curl | 1 / 1 | 88.028 |
| D | aria2c | 3 / 3 | 647.777 |
| D | Prism | 3 / 3 | 3717.131 |
| D | IDG | 3 / 3 | 3412.277 |
