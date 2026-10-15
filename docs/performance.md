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

2 layers + stroke overlay: **0.107 ms** per 256×256 tile (phase 3: 0.113 ms),
0 allocations in 840 tiles.

### Phase 4A

Composition reads layers through the new API and compares the overlay
target by id. Measured interleaved against the pre-4A commit on the same
machine state (this machine was slower that day than when 0.107 ms was
recorded): before 0.111 / 0.112 / 0.123 ms, after 0.114 / 0.116 / 0.123 ms
per tile, 0 allocations — within run-to-run noise. `commit_into` no longer
allocates its 1 MiB scratch (row buffer on the stack in `commit_tile`).

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
- CPU composition scales with layers × dirty tiles; many layers will want
  GPU composition or cached groups (phase 4+).
- The display texture must fit the GPU's max texture size.

## Rules

- Measure before and after any change to a hot path; record numbers in the
  commit message or here.
- Never trade determinism (bit-identical results) for speed.
- Keep `allocs/dab` at 0.000 and `brush/tests/allocation.rs` passing.
