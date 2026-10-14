| brush | size px | dabs | dabs/s | Mpx/s | tiles/dab | allocs/dab (hot path) |
|---|---:|---:|---:|---:|---:|---:|
| hard round | 5 | 20000 | 374593 | 18.4 | 1.03 | 0.000 |
| hard round | 25 | 20000 | 131767 | 96.1 | 1.10 | 0.000 |
| hard round | 100 | 4000 | 11050 | 115.0 | 2.25 | 0.000 |
| hard round | 250 | 640 | 1907 | 121.1 | 3.96 | 0.000 |
| hard round | 500 | 200 | 488 | 123.0 | 8.89 | 0.000 |
| soft round | 5 | 20000 | 368698 | 18.1 | 1.03 | 0.000 |
| soft round | 25 | 20000 | 119425 | 87.1 | 1.10 | 0.000 |
| soft round | 100 | 4000 | 9555 | 99.4 | 2.25 | 0.000 |
| soft round | 250 | 640 | 1609 | 102.2 | 3.96 | 0.000 |
| soft round | 500 | 200 | 411 | 103.6 | 8.89 | 0.000 |
| textured | 5 | 20000 | 354904 | 17.4 | 1.03 | 0.000 |
| textured | 25 | 20000 | 76423 | 55.7 | 1.10 | 0.000 |
| textured | 100 | 4000 | 5686 | 59.2 | 2.25 | 0.000 |
| textured | 250 | 640 | 959 | 60.9 | 3.96 | 0.000 |
| textured | 500 | 200 | 243 | 61.2 | 8.89 | 0.000 |
| scatter | 5 | 20000 | 356799 | 17.5 | 1.03 | 0.000 |
| scatter | 25 | 20000 | 127903 | 93.2 | 1.17 | 0.000 |
| scatter | 100 | 4000 | 10840 | 112.8 | 2.11 | 0.000 |
| scatter | 250 | 640 | 1897 | 120.5 | 3.92 | 0.000 |
| scatter | 500 | 200 | 479 | 120.7 | 8.73 | 0.000 |

single-thread vs parallel (16 hardware threads), batches of one input sample:

| brush | size px | dabs/batch | single dabs/s | parallel dabs/s | speed-up |
|---|---:|---:|---:|---:|---:|
| hard round | 5 | 20 | 368141 | 371562 | 1.01x |
| hard round | 25 | 4 | 125111 | 125012 | 1.00x |
| hard round | 100 | 1 | 10799 | 10695 | 0.99x |
| hard round | 250 | 1 | 1864 | 3178 | 1.70x |
| hard round | 500 | 1 | 479 | 1553 | 3.24x |
| soft round | 5 | 20 | 346026 | 345828 | 1.00x |
| soft round | 25 | 4 | 112808 | 112541 | 1.00x |
| soft round | 100 | 1 | 9345 | 9298 | 0.99x |
| soft round | 250 | 1 | 1580 | 2680 | 1.70x |
| soft round | 500 | 1 | 403 | 1275 | 3.16x |
| textured | 5 | 20 | 348320 | 351193 | 1.01x |
| textured | 25 | 4 | 75437 | 75639 | 1.00x |
| textured | 100 | 1 | 5719 | 5696 | 1.00x |
| textured | 250 | 1 | 960 | 1598 | 1.67x |
| textured | 500 | 1 | 243 | 724 | 2.98x |

composition (2 layers + stroke overlay): 0.113 ms per 256x256 tile, 0 allocations in 840 tiles
