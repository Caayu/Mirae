# Performance

## Hot path

Per input sample (pen at 200+ Hz) and per frame:

```
input sample                       BrushInput (Copy), no allocation
 -> pressure curve                 Curve::eval (binary search + Hermite)
 -> velocity                       EMA, O(1)
 -> jitter filter                  writes into reused Vec
 -> stabilizer                     O(1) (One Euro) / O(window) (moving avg)
 -> interpolation + flattening     writes chords into reused Vec
 -> spacing                        DabSpacer, O(dabs), reused Vec<DabSite>
 -> effectors                      per dab: per property, per effector O(1)
 -> scatter                        per dab: count copies into reused Vec<Dab>
 -> dab                            Dab (Copy)
 -> tip sampling                   per pixel: procedural falloff or trilinear mip sample
 -> texture                        per pixel: bilinear height sample (bit-mask wrap)
 -> stroke buffer                  per pixel: m += (target - m)*flow*cov
                                   per batch: sequential, or per-tile parallel
per frame:
 -> tail rebuild                   re-emits the last taper.end px (if any)
 -> composition                    per dirty tile: layers + overlay into ComposeScratch
 -> upload                         per dirty tile: CPU mips + queue.write_texture
```

## Where zero allocation is expected

| Stage | Expectation | How it is checked |
|---|---|---|
| Stamping a dab into existing tiles, sequential path | **0 allocations** | `stamp_bench` (`allocs/dab (hot path)` = `0.000`; the benchmark's hot-path table uses the sequential path) and `brush/tests/allocation.rs` |
| Stroke processor per sample (filters, interpolation, spacing, dynamics, scatter) + sequential stamping | **0 allocations** after buffers and tiles exist | `brush/tests/allocation.rs` (12 px and 400 px brushes, 2800 samples) |
| Parallel stamping (batches ≥ 60 000 px of work) | **amortized, inside rayon**: submitting a batch from the UI thread pushes a job onto rayon's injector queue (crossbeam `Injector`), which allocates one block per 63 jobs; measured 9 allocations in 2800 samples of a 400 px brush | `brush/tests/allocation.rs` bounds it (≤ 1 per 100 samples) |
| Dirty-tile bookkeeping | `StrokeBuffer::take_dirty` hands its `BTreeSet` away once per frame, so the next frame's marks allocate a few set nodes (per frame, not per dab) | Design; documented here |
| Composition of a dirty tile | **0 allocations** (`compose_tile_into` + `ComposeScratch`) | `stamp_bench` composition line |
| First touch of a tile | allocates the tile (by design, ADR 0002) | — |
| Parallel batch | reuses `BatchScratch` vectors; tiles are moved, not copied | — |
| Commit | one scratch tile buffer per stroke | — |

## Parallelism

Per-tile parallel stamping on the rayon pool when a batch's estimated work
(sum of dab bounding-box areas) is at least `PARALLEL_MIN_WORK` = 60 000
pixels; bit-identical to sequential (ADR 0010, concurrency.md). Do not
change the threshold without re-running the benchmark.

## Current numbers

Machine: AMD Ryzen 7 5800X (8 cores, 16 threads), 32 GB, AMD Radeon RX 7700 XT,
Windows 11, Rust 1.99,
release profile. Command:

```sh
cargo run --release -p tela-corpus --example stamp_bench
```

### Single-thread stamping (phase 3.5 run; phase 3 run in [phase3/stamp_bench.md](phase3/stamp_bench.md))

| brush | 5 px | 25 px | 100 px | 250 px | 500 px |
|---|---:|---:|---:|---:|---:|
| hard round (dabs/s) | 375 972 | 133 170 | 10 784 | 1 888 | 487 |
| soft round | 377 264 | 120 097 | 9 527 | 1 614 | 411 |
| textured | 358 618 | 77 170 | 5 751 | 967 | 246 |
| scatter | 361 899 | 128 818 | 10 744 | 1 880 | 486 |

Throughput for large dabs: ~120 Mpx/s hard, ~103 Mpx/s soft, ~62 Mpx/s
textured. Tiles touched per dab: 1.03 (5 px) to 8.9 (500 px).
Allocations per dab: **0.000** for every row (sequential path; see the
table above for the parallel path).

### Single vs parallel (batches of one input sample)

| brush | 5 px | 25 px | 100 px | 250 px | 500 px |
|---|---:|---:|---:|---:|---:|
| hard round | 1.01× | 1.00× | 0.99× | 1.69× | 3.26× |
| soft round | 0.99× | 1.00× | 1.00× | 1.68× | 3.13× |
| textured | 1.00× | 1.00× | 0.99× | 1.69× | 2.98× |

### Composition

2 layers + stroke overlay: **0.107 ms** per 256×256 tile (phase 3: 0.113 ms;
phase 4E: 0.107–0.110 ms),
0 allocations in 840 tiles.

### Phase 5A — colour

Stamping never reads the colour (the stroke is an alpha mask); the colour
enters only in `paint_over` (preview) and `commit_tile`, as one more
multiply per channel that was already there (`paint.scale(m)`). Measured
with `stamp_bench` (new table at the end of its output), soft 120 px
stroke of 400 dabs, per touched tile:

| depth | colour | preview (2-layer composition + overlay) | commit | allocations |
|---|---|---:|---:|---:|
| 8-bit | black | 0.111 ms | 0.280 ms | 0 |
| 8-bit | 37,149,213 | 0.111 ms | 0.280 ms | 0 |
| 16-bit | black | 0.111 ms | 0.324 ms | 0 |
| 16-bit | 37,149,213 | 0.110 ms | 0.315 ms | 0 |

Stamping after 5A, same machine (single thread, dabs/s), against the
baseline run that opened 5A: hard 5 px 360 248 (371 305), hard 500 px 478
(474), soft 500 px 411 (389), textured 500 px 247 (239), scatter 5 px
345 455 (347 953); allocations per dab 0.000 everywhere; composition
0.112 ms (0.141 ms in the baseline run): all within run-to-run noise. The
airbrush uses the same stamping and commit; it is colour-independent by
the same argument. The colour panel's picker is a 9 × 9 vertex-coloured
mesh rebuilt per frame (no texture upload); not measurable at UI rate.
An eyedropper pick composes the one tile under the cursor (the cost of a
composed tile above, more with many layers: see 4G), only on click/drag.

### Phase 4A

Composition reads layers through the new API and compares the overlay
target by id. Measured interleaved against the pre-4A commit on the same
machine state (this machine was slower that day than when 0.107 ms was
recorded): before 0.111 / 0.112 / 0.123 ms, after 0.114 / 0.116 / 0.123 ms
per tile, 0 allocations — within run-to-run noise. `commit_into` no longer
allocates its 1 MiB scratch (row buffer on the stack in `commit_tile`).

### Phase 4G — integration, documents at scale

`cargo run --release -p tela-corpus --example doc_bench` (full run about
a minute; `-- --quick` takes ~3 s and is part of `check.ps1 -Bench`, which
fails if composition allocates). Same machine; raw output of the run is
summarised here, memory in [memory.md](memory.md).

**Regression check of 4F against 4E** (composition, `stamp_bench`, three
interleaved runs in a temporary worktree of 497b006): 4E 0.111 / 0.118 /
0.111 ms, 4F 0.116 / 0.113 / 0.122 ms per tile — inside run-to-run noise
(±5 %). The brush, canvas and color crates did not change in phase 4F–4G.
`stamp_bench` in 4G: 374 666 dabs/s (5 px hard), 481 (500 px), 0.000
allocations in all 20 rows; parallel speed-up at 500 px hard varied
2.8–3.3× between runs (noise, not a regression). `blend_bench` and
`history_bench` are faster than their 4C/4B records (Normal 10 layers
1.48 vs 2.0 ms; history ~2×): machine state, not code.

**One tile against the layer count** (every layer draws in the tile,
opacity 0.85; ms per 256² tile; 0 allocations in every row):

| layers' mode | bits | 1 | 10 | 50 | 100 |
|---|---:|---:|---:|---:|---:|
| Normal | 8 | 0.374 | 1.604 | 6.394 | 12.426 |
| Normal | 16 | 0.331 | 1.502 | 6.894 | 13.665 |
| Multiply | 8 | 0.527 | 3.204 | 14.982 | 29.682 |
| Soft Light | 8 | 0.656 | 4.347 | 22.197 | 44.442 |
| Soft Light | 16 | 0.672 | 4.643 | 23.093 | 46.034 |
| Normal + mask | 8 | 0.373 | 1.931 | 8.970 | 18.096 |
| clipping (every 2nd) | 8 | 0.320 | 1.479 | 6.572 | 13.248 |
| mask + clipping | 8 | 0.383 | 2.030 | 9.435 | 18.811 |

Cost is linear in layers (~125 µs per Normal layer per tile, ~300
Multiply, ~445 Soft Light; a mask adds ~55 µs). A temporary profiling
split (not committed) put one layer's cost per tile at: decode of the
stored tile 64 µs (8-bit) / 69 µs (16-bit), Normal blend 65 µs, Multiply
351 µs, Soft Light 532 µs; a plain copy 18 µs. The bottleneck is per-pixel
`f32` work, single-threaded, with nothing cached below the stroke target
(TECHNICAL_DEBT #33).

**Realistic 4096² documents** (mostly Normal; every 10th of three layers
in Multiply/Screen/Overlay, every 5th clipped, every 7th masked; stroke
frame = 4 tiles):

| document | layers | bits | stored tiles | pixels MiB | full redraw ms | stroke frame ms | save s | load s | file MiB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| sparse 5 % | 10 | 8 | 149 | 37 | 46 | 1.23 | 0.24 | 0.08 | 14 |
| sparse 5 % | 10 | 16 | 149 | 74 | 48 | 1.20 | 0.96 | 0.20 | 58 |
| sparse 5 % | 50 | 8 | 683 | 171 | 162 | 3.41 | 1.08 | 0.33 | 62 |
| sparse 5 % | 50 | 16 | 683 | 342 | 169 | 3.43 | 4.36 | 0.90 | 263 |
| sparse 5 % | 100 | 8 | 1 328 | 332 | 281 | 5.32 | 2.13 | 0.65 | 122 |
| sparse 5 % | 100 | 16 | 1 328 | 664 | 296 | 5.66 | 8.42 | 1.84 | 513 |
| partial 30 % | 10 | 8 | 765 | 191 | 192 | 3.61 | 1.25 | 0.37 | 70 |
| partial 30 % | 10 | 16 | 765 | 382 | 185 | 3.75 | 4.97 | 1.02 | 295 |
| partial 30 % | 50 | 8 | 3 864 | 966 | 756 | 11.26 | 6.65 | 2.05 | 356 |
| partial 30 % | 50 | 16 | 3 864 | 1 932 | 770 | 11.68 | 25.98 | 5.48 | 1 491 |
| dense 100 % | 10 | 8 | 2 560 | 640 | 562 | 8.82 | 4.54 | 1.34 | 233 |
| dense 100 % | 10 | 16 | 2 560 | 1 280 | 615 | 12.41 | 16.35 | 3.60 | 984 |

Masks add 0–12 MiB. Full redraw happens on opening and on visibility or
mode changes; a stroke frame is what drawing costs per update.

**History on a 4096² document** (commit includes the history copy):

| operation | bits | tiles | commit ms | undo / redo ms | held MiB |
|---|---:|---:|---:|---:|---:|
| stroke 25 px × 1 000 px | 8 / 16 | 8 | 4.8 / 5.6 | ≤ 0.01 | 2 / 4 |
| stroke 500 px × 3 000 px | 8 / 16 | 50 | 47.3 / 49.6 | ≤ 0.01 | 12 / 24 |
| mask stroke 500 px × 3 000 px | 8 / 16 | 50 | 36.2 / 35.1 | ≤ 0.01 | 0 (3 reserved) |
| clear or remove a layer | 8 / 16 | 64 | 0.0 | ≤ 0.01 | 16 / 32 |
| convert depth | 8 → 16 / 16 → 8 | 320 | 40.6 / 43.5 | ≤ 0.08 | 80 / 160 |

`io_bench` reproduced 4F (4096² × 10 full: 8-bit save 2.06 s / load
0.82 s; 16-bit 6.36 / 1.65 s; peak extra memory 1.3–3 MiB).

**What this means** (no optimization was done in 4G): typical documents
(≤ 50 layers, mostly Normal, partial coverage) draw inside a 60 Hz budget;
about 100 overlapping Normal layers, or 40 Soft Light layers, under a
stroke exceed it. Saving a large 16-bit document blocks for many seconds
(TECHNICAL_DEBT #28).

### Phase 4F — document I/O

`cargo run --release -p tela-io --example io_bench` (Ryzen 7 5800X, NVMe,
default compression = deflate level 2; files on the system temp
directory). Raw = decoded pixel and mask bytes; MB/s = raw MB per second;
peak extra = the largest allocation beyond what was live before the
save, or beyond the loaded document, during the operation (counting
allocator). Content: layers cycle through a painted texture (opaque
gradient with ±3 levels of noise), translucent soft bands with gaps, and
a flat fill.

| document | bits | tiles | raw MiB | file MiB | ratio | save s | save MB/s | load s | load MB/s | save peak extra MiB | load peak extra MiB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| small 1000² ×3 | 8 | 28 | 7 | 0.9 | 7.79 | 0.02 | 337 | 0.01 | 511 | 1.7 | 1.3 |
| small 1000² ×3 | 16 | 28 | 14 | 3.8 | 3.71 | 0.07 | 216 | 0.02 | 612 | 1.9 | 1.5 |
| 4096² ×10 full | 8 | 2560 | 640 | 83.6 | 7.65 | 1.90 | 353 | 0.74 | 902 | 2.2 | 1.8 |
| 4096² ×10 full | 16 | 2560 | 1280 | 314.9 | 4.06 | 6.05 | 222 | 1.57 | 856 | 2.4 | 2.0 |
| 4096² ×10 sparse 10 % | 8 | 255 | 64 | 8.7 | 7.31 | 0.22 | 306 | 0.08 | 867 | 1.7 | 1.3 |
| 4096² ×10 sparse 10 % | 16 | 255 | 128 | 33.0 | 3.87 | 0.62 | 216 | 0.17 | 788 | 2.0 | 1.6 |
| 4096² ×10 full, masks + clipping | 8 | 5120 | 800 | 86.0 | 9.31 | 2.18 | 384 | 1.08 | 774 | 2.7 | 2.3 |
| 4096² ×10 full, masks + clipping | 16 | 5120 | 1440 | 317.3 | 4.54 | 6.32 | 239 | 2.01 | 751 | 3.0 | 2.5 |

Compression levels (2048² × 6 full layers, same content):

| bits | compression | file MiB | ratio | save s | save MB/s | load s | load MB/s |
|---:|---|---:|---:|---:|---:|---:|---:|
| 8 | stored | 96.0 | 1.00 | 0.26 | 384 | 0.07 | 1379 |
| 8 | deflate 1 | 19.1 | 5.03 | 0.20 | 508 | 0.12 | 841 |
| 8 | **deflate 2** | 11.2 | 8.57 | 0.26 | 389 | 0.09 | 1088 |
| 8 | deflate 3 | 9.6 | 9.96 | 0.31 | 323 | 0.09 | 1174 |
| 8 | deflate 6 | 9.1 | 10.55 | 0.60 | 167 | 0.08 | 1217 |
| 8 | deflate 9 | 9.0 | 10.70 | 1.95 | 52 | 0.10 | 1017 |
| 16 | stored | 192.0 | 1.00 | 0.54 | 370 | 0.11 | 1766 |
| 16 | deflate 1 | 53.2 | 3.61 | 0.56 | 360 | 0.21 | 955 |
| 16 | **deflate 2** | 45.3 | 4.24 | 0.81 | 248 | 0.22 | 910 |
| 16 | deflate 3 | 44.5 | 4.31 | 0.94 | 213 | 0.21 | 949 |
| 16 | deflate 6 | 44.5 | 4.31 | 1.15 | 176 | 0.22 | 929 |
| 16 | deflate 9 | 46.9 | 4.09 | 2.77 | 73 | 0.27 | 737 |

- **Default level 2**: within 2–6 % of the smallest file at 40–70 % of
  level 6's time. Level 1 doubles 8-bit files (zlib-rs's fastest level
  matches poorly); levels above 3 cost up to 7× the time for a few
  percent; level 9 is even larger than 6 on 16-bit data.
- **Memory**: saving and loading stream one tile at a time: 1.3–3 MiB of
  extra memory for every document size (0.9 MB to 1.4 GiB of pixels).
- **Bottleneck**: deflate on one core. Stored saves at ~380 MB/s
  including the disk; 16-bit data compresses worse (noisy low bytes). A
  full 16-bit 4096² × 10 document takes ~6 s to save on the UI thread: the
  case for background (and per-tile parallel) saving in a later phase.
  Loading is 750–900 MB/s.
- Sizes depend on content: the synthetic noise is a hard case; flat and
  sparse layers compress far better (a blank tile is ~2.5 KB).
- Hot paths unchanged: `stamp_bench` 0.000 allocations per dab in every
  row; composition 0.108–0.109 ms per tile in three standalone runs
  against 0.107 at the 4F baseline (the paper-backdrop decision became a
  shared function; one run inside `check.ps1 -Bench`, right after the
  build, read 0.127).
### Phase 4E — 16-bit storage

`depth_bench` (whole documents; 2048², every tile filled with the
`blend_bench` filler; two runs, medians of 5 repetitions; ms per composed
256×256 tile):

| stack | 8-bit | 16-bit | 16 / 8 | allocations |
|---|---:|---:|---:|---:|
| 1 layer | 0.335 / 0.316 | 0.330 / 0.321 | 0.99 / 1.02 | 0 |
| 10 layers, Normal | 1.528 / 1.497 | 1.617 / 1.552 | 1.06 / 1.04 | 0 |
| 10 layers, Multiply | 3.469 / 3.394 | 3.584 / 3.616 | 1.03 / 1.07 | 0 |
| 2 layers, top Screen | 0.701 / 0.677 | 0.692 / 0.668 | 0.99 / 0.99 | 0 |
| 2 layers, top Luminosity | 0.998 / 0.941 | 1.003 / 0.950 | 1.01 / 1.01 | 0 |
| 1 layer + fully stored mask | 0.379 / 0.366 | 0.386 / 0.387 | 1.02 / 1.06 | 0 |
| base + 1 clipped, Normal | 0.468 / 0.460 | 0.485 / 0.473 | 1.04 / 1.03 | 0 |
| base + 9 clipped, Multiply | 2.496 / 2.457 | 2.541 / 2.519 | 1.02 / 1.03 | 0 |

Strokes on a filled layer (soft round across the diagonal; commit
includes the history copy of the touched tiles):

| brush | tiles | bits | preview ms/tile | commit ms | commit, alpha locked ms | undo µs | redo µs | history held |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 8 px | 17 | 8 | 0.37 / 0.34 | 5.18 / 5.08 | 5.18 / 5.07 | 3–4 | 2 | 4.0 MiB |
| 8 px | 17 | 16 | 0.34 / 0.34 | 6.69 / 6.52 | 6.65 / 6.49 | 4 | 2 | 8.0 MiB |
| 200 px | 28 | 8 | 0.28 / 0.28 | 8.69 / 9.07 | 8.37 / 9.02 | 5 | 3 | 5.5 MiB (reserved 6.8) |
| 200 px | 28 | 16 | 0.28 / 0.35 | 10.24 / 13.45 | 10.35 / 12.98 | 4–5 | 3 | 11.0 MiB (reserved 13.5) |

Depth conversion of a 10-layer 2048² document (640 tiles): 8 → 16
**96.8 ms**, 16 → 8 **92.2 ms** (800 allocations: one tile buffer per
tile plus the surfaces), history reserves 320 MiB (the 16-bit size of the
larger direction); undoing either takes ~0.09 ms (swap).

Memory:

| item | 8-bit | 16-bit |
|---|---:|---:|
| pixel tile 256 × 256 × 4 channels | 256 KiB | 512 KiB |
| mask tile (8-bit at every depth) | 64 KiB | 64 KiB |
| `ComposeScratch` (f32 at every depth) | 3.50 MiB | 3.50 MiB |
| 1 full 4000 × 3000 layer (192 tiles; computed) | 48 MiB | 96 MiB |
| 10 / 50 / 100 full layers (computed) | 480 MiB / 2.3 GiB / 4.7 GiB | 960 MiB / 4.7 GiB / 9.4 GiB |
| 10 / 50 / 100 sparse layers, one 200 px stroke each (27 tiles per layer; measured) | 68 / 338 / 675 MiB | 135 / 675 / 1350 MiB |
| history of a stroke over painted tiles | 256 KiB per tile | 512 KiB per tile |

Regressions, measured interleaved against the end of 4D (fb73bd3), two
runs each: `stamp_bench` within noise (e.g. hard 500 px 493 → 488 dabs/s,
soft 100 px 9211 → 9351), **allocs/dab 0.000**, composition line
0.116 / 0.110 → 0.110 / 0.107 ms; `blend_bench` **4–23 % faster** in
every mode (Normal 2 layers 0.505 / 0.510 → 0.445 / 0.440 ms, 10 layers
1.806 → 1.475 ms; decoding now runs in its own loop before blending), 0
allocations; `mask_clip_bench` 10–23 % faster (1 layer 0.353 → 0.313,
base + 5 clipped Normal 1.304 → 1.003), commits equal (0.333 → 0.330
ms/tile); `history_bench` within noise (huge brush on painted tiles
25.46 / 24.99 → 24.98 / 24.16 ms; undo/redo µs).

Banding (`cargo run --release -p tela-corpus --example banding`): the
table is in [parity/4E-16bit.md](parity/4E-16bit.md) (section "Banding");
it runs in under 2 s.

Assessment: the phase 4 target of ≤ 1.3× was met by composition
(≤ 1.07×) and undo/redo (equal); the stroke commit with its history copy
costs 1.2–1.5× (twice the bytes copied and quantized), still a few ms for
a 28-tile stroke; memory doubles. No hot path was optimized: nothing
measured called for it.

### Phase 4D — masks, clipping, alpha lock

`mask_clip_bench` (2048² document, every tile filled with the
`blend_bench` filler; clipped/upper layers at 80 %; ms per composed
256×256 tile, two runs):

| stack | ms/tile | allocations |
|---|---:|---:|
| 1 layer | 0.349 / 0.344 | 0 |
| 1 layer + new (empty) mask, 0 tiles stored | 0.347 / 0.346 | 0 |
| 1 layer + fully stored mask | 0.399 / 0.402 | 0 |
| 2 layers, Normal (4C baseline) | 0.506 / 0.515 | 0 |
| base + 1 clipped, Normal | 0.516 / 0.526 | 0 |
| 2 layers, top Multiply | 0.725 / 0.712 | 0 |
| base + 1 clipped, Multiply | 0.610 / 0.610 | 0 |
| 6 layers, Normal | 1.146 / 1.137 | 0 |
| base + 5 clipped, Normal | 1.211 / 1.206 | 0 |
| base + 5 clipped, Multiply | 1.666 / 1.664 | 0 |
| base and clipped layer both with stored masks | 0.622 / 0.622 | 0 |

Stroke operations (120 px soft round across the diagonal, 28 tiles):

| operation | ms per tile |
|---|---:|
| preview on pixels | 0.304 / 0.301 |
| preview on pixels, alpha locked | 0.303 / 0.300 |
| preview on the mask | 0.300 / 0.303 |
| commit, alpha free (with the history copy) | 0.327 / 0.330 |
| commit, alpha locked (with the history copy) | 0.324 / 0.317 |

Mask erase stroke over a new mask (27 tiles stored): commit 3.4 ms,
undo/redo 3–4 µs (swap), history holds 1 770 168 bytes after undo
(27 × 64 KiB + per-entry overhead).

- A new mask costs nothing (no tile); a fully stored mask adds ~15 % to a
  single layer. A clipped Normal layer costs 2–6 % more than an ordinary
  one (group buffer); a clipped Multiply costs less than an unclipped one
  (the atop mix is simpler than the W3C general formula).
- Alpha lock costs nothing measurable in preview or commit.
- Memory: a mask tile is 64 KiB; masks store only tiles that hide
  something; history keeps 64 KiB per changed mask tile. The compose
  scratch grew from 2.25 MiB to 3.5 MiB (`group` 1 MiB, `mask` 256 KiB),
  allocated once.
- Unchanged from 4C (4C commit 552310b built in a worktree, run
  interleaved in the same session): `blend_bench` within run-to-run
  noise for every mode; `stamp_bench` composition 0.106 ms per tile in
  both, allocations per dab 0.000; `history_bench` equal. A first version
  that shared the blend loops through a generic was 5–16 % slower in
  several modes and was replaced (ADR 0017, decision 10).

### Phase 4C — blend modes

`blend_bench` (2048² document, every tile filled with mixed opaque and
translucent pixels; bottom layer Normal, the others at 80 % in the mode;
ms per composed 256×256 tile):

| mode | 2 layers ms/tile | × Normal | 10 layers ms/tile | × Normal | allocations |
|---|---:|---:|---:|---:|---:|
| normal | 0.507 | 1.00 | 2.006 | 1.02 | 0 |
| multiply | 0.750 | 1.48 | 3.794 | 1.93 | 0 |
| screen | 0.734 | 1.45 | 3.800 | 1.94 | 0 |
| overlay | 0.806 | 1.59 | 4.340 | 2.21 | 0 |
| darken | 0.670 | 1.32 | 3.365 | 1.71 | 0 |
| lighten | 0.691 | 1.36 | 3.307 | 1.69 | 0 |
| color-dodge | 0.832 | 1.64 | 4.681 | 2.39 | 0 |
| color-burn | 0.905 | 1.78 | 5.339 | 2.72 | 0 |
| hard-light | 0.789 | 1.55 | 4.181 | 2.13 | 0 |
| soft-light | 0.887 | 1.75 | 5.270 | 2.69 | 0 |
| difference | 0.713 | 1.40 | 3.365 | 1.71 | 0 |
| exclusion | 0.735 | 1.45 | 3.881 | 1.98 | 0 |
| hue | 1.625 | 3.20 | 11.487 | 5.85 | 0 |
| saturation | 1.712 | 3.37 | 12.482 | 6.36 | 0 |
| color | 1.048 | 2.06 | 6.724 | 3.43 | 0 |
| luminosity | 1.044 | 2.06 | 6.504 | 3.31 | 0 |
| add | 0.712 | 1.40 | 3.452 | 1.76 | 0 |

- Normal is unchanged (same code); the per-tile cost here is higher than
  `stamp_bench`'s composition line because every tile of every layer is
  full.
- Separable modes cost 1.3–1.8× Normal per layer pair; the non-separable
  Hue/Saturation ~3.3× (luminosity/saturation math per pixel), Color and
  Luminosity ~2×. A tile with a blend layer also composes over the paper
  (no extra pass).
- `set_sat` sorts the three channels with three comparisons instead of a
  general sort (Hue/Saturation ~15 % faster, measured).
- `stamp_bench` after 4C: composition 0.106–0.116 ms per tile (Normal path,
  within the run-to-run range), allocations per dab 0.000.

### Phase 4B — undo history

`history_bench` (4096² document, real brush-engine strokes, medians of 9):

| stroke | size px | layer | tiles | commit, no history ms | commit + history ms | undo ms | redo ms | held after commit | held after undo | reserved |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| small | 5 | empty | 2 | 0.563 | 0.576 | 0.001 | 0.000 | 0.00 MiB | 0.50 MiB | 0.50 MiB |
| small | 5 | painted | 2 | 0.537 | 0.695 | 0.002 | 0.000 | 0.50 MiB | 0.50 MiB | 0.50 MiB |
| medium | 25 | empty | 6 | 2.732 | 2.371 | 0.003 | 0.001 | 0.00 MiB | 1.50 MiB | 1.50 MiB |
| medium | 25 | painted | 6 | 3.060 | 4.060 | 0.004 | 0.001 | 1.50 MiB | 1.50 MiB | 1.50 MiB |
| large | 100 | empty | 19 | 12.696 | 12.329 | 0.006 | 0.004 | 0.00 MiB | 4.75 MiB | 4.75 MiB |
| large | 100 | painted | 19 | 10.852 | 13.534 | 0.008 | 0.004 | 4.75 MiB | 4.75 MiB | 4.75 MiB |
| huge brush | 500 | empty | 74 | 49.629 | 49.646 | 0.018 | 0.013 | 0.00 MiB | 18.50 MiB | 18.50 MiB |
| huge brush | 500 | painted | 74 | 43.215 | 56.410 | 0.022 | 0.013 | 18.50 MiB | 18.50 MiB | 18.50 MiB |

- Undo and redo are swaps: microseconds regardless of stroke size.
- Recording costs ~0.17 ms per changed tile that already had pixels (a
  256 KiB copy plus a comparison); nothing measurable over empty tiles.
- Memory: exactly 256 KiB per changed tile per step (plus a few hundred
  bytes per step). "Held" is what the history holds now; "reserved" is the
  step's larger direction, on which the byte limit is enforced.
- Observed, not changed in 4B: the commit itself costs ~0.6 ms per tile
  even without history, because `commit_tile` converts every row of the
  tile, including rows the stroke does not touch (TECHNICAL_DEBT #18).
- `stamp_bench` after 4B: allocations per dab 0.000 on every line;
  composition 0.105 ms per tile, 0 allocations (unchanged; 4B does not
  touch these paths).

### Phase 3 → 3.5

The phase 3.5 refactors did not touch pixel loops; all differences are
within run-to-run noise (±2%). Example: hard round 500 px 488 → 487 dabs/s,
parallel 3.24× → 3.26×.

### Input latency (phase 2, unchanged)

Synthetic Windows Ink pen at ~220–230 Hz: input age ~3 ms, pipeline 15–70
µs per sample, composition 0.2–0.7 ms per frame, sample → GPU submit
~9–10 ms. Details in [phase2/README.md](phase2/README.md). Measure in the
app with F3 (title bar) or `RUST_LOG=tela::latency=debug`.

## Known limits

- Single-thread throughput bounds small/medium brushes; parallelism helps
  only when a batch spans several tiles (≥ ~250 px brushes).
- Textured stamping costs ~2× untextured (height sampling per pixel).
- Saving runs on the UI thread: ~2 s for a full 8-bit 4096² × 10
  document, ~6 s at 16 bits (deflate-bound; 4F).
- 16-bit documents double pixel and history memory (ADR 0018); the
  1 GiB history budget holds half as many 16-bit tiles.
- CPU composition scales with layers × dirty tiles, single-threaded:
  ~12 ms per tile for 100 Normal layers, ~44 ms for 100 Soft Light (4G);
  directions in TECHNICAL_DEBT #33 (cache below the stroke target,
  parallel dirty tiles, GPU later).
- Saving 50 partial 16-bit 4096² layers takes ~26 s (4G, `doc_bench`).
- The display texture must fit the GPU's max texture size.

## Rules

- Measure before and after any change to a hot path; record numbers in the
  commit message or here.
- Never trade determinism (bit-identical results) for speed.
- Keep `allocs/dab` at 0.000 and `brush/tests/allocation.rs` passing.
