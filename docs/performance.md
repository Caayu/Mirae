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
| Stamping a dab into existing tiles | **0 allocations** | `stamp_bench` counting allocator: `allocs/dab (hot path)` must print `0.000` |
| Stroke processor per sample | no allocation after buffers grow (all stages write into owned, reused buffers) | Design; reviewed in phase 3.5 |
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
Allocations per dab: **0.000** for every row.

### Single vs parallel (batches of one input sample)

| brush | 5 px | 25 px | 100 px | 250 px | 500 px |
|---|---:|---:|---:|---:|---:|
| hard round | 1.01× | 1.00× | 0.99× | 1.69× | 3.26× |
| soft round | 0.99× | 1.00× | 1.00× | 1.68× | 3.13× |
| textured | 1.00× | 1.00× | 0.99× | 1.69× | 2.98× |

### Composition

2 layers + stroke overlay: **0.107 ms** per 256×256 tile (phase 3: 0.113 ms),
0 allocations in 840 tiles.

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
- Keep `allocs/dab` at 0.000.
