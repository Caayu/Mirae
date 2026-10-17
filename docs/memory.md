# Memory model

What Mirae keeps in memory, from the building blocks to whole documents,
measured at the end of phase 4 (`doc_bench`, `io_bench`; counting
allocator) or derived from the types where stated. KiB/MiB/GiB are 1024-based.

## Building blocks

| Item | Size | Notes |
|---|---:|---|
| Pixel tile, 8-bit | 256 KiB | 256 × 256 × RGBA8, premultiplied (ADR 0002, 0018) |
| Pixel tile, 16-bit | 512 KiB | RGBA16 |
| Mask tile | 64 KiB | 8-bit at every depth; only tiles that hide something exist |
| Selection tile | 64 KiB | 8-bit coverage, stored only where the coverage differs from the fill (ADR 0024); Select All and Invert store none |
| Stroke buffer tile | 256 KiB | f32 coverage per touched tile, plus copy-on-write tail tiles near the pen |
| Layer without pixels | ~0.3 KiB | header, name, empty tile maps (measured: 100 empty layers = 27 KiB) |
| `ComposeScratch` | 3.5 MiB | three f32 tile buffers, a mask buffer, the RGBA8 output (measured 3 670 016 B); one per compositor |
| Display texture (GPU) | w × h × 4 B × 4/3 | RGBA8 with mips; a 4000 × 3000 canvas ≈ 61 MiB of VRAM |

Absent tiles cost nothing: a layer stores only tiles it has painted
(and keeps them, even erased, until cleared).

## Documents

Pixel memory = stored tiles × tile size. A full 4096² layer is 256 tiles
(64 MiB at 8 bits, 128 MiB at 16); a full 4000 × 3000 layer 192 tiles
(48 / 96 MiB).

4096² documents, every layer covering the given share of the canvas
(derived; the measured `doc_bench` rows agree within the randomness of
their coverage):

| Layers | Sparse 5 % | Partial 30 % | Dense 100 % |
|---:|---:|---:|---:|
| 10, 8-bit | 32 MiB (measured 37) | 192 MiB (measured 191) | 640 MiB (measured 640) |
| 10, 16-bit | 64 MiB | 384 MiB | 1.25 GiB (measured) |
| 50, 8-bit | 160 MiB (measured 171) | 960 MiB (measured 966) | 3.1 GiB |
| 50, 16-bit | 320 MiB | 1.9 GiB (measured) | 6.25 GiB |
| 100, 8-bit | 320 MiB (measured 332) | 1.9 GiB | 6.25 GiB |
| 100, 16-bit | 640 MiB (measured 664) | 3.75 GiB | **12.5 GiB** |

Masks add 64 KiB per tile they hide; in the measured documents (every
7th layer masked on a third of its tiles) 2–12 MiB.

## Selections (phase 5C)

Measured with `selection_bench` (4096², live bytes of the counting
allocator):

| Selection | Stored tiles | Selection | History after the step |
|---|---:|---:|---:|
| none | 0 | 0 | 0 |
| small rectangle (200×150) | 4 | 0.25 MiB | ~0 (the step held no previous selection) |
| select all | 0 | 0 | 0 |
| select all, then invert (no selection) | 0 | 0 | 0 |
| anti-aliased ellipse r 1 900 | 148 | 9.25 MiB | ~0 |
| the same feathered by 50 px | 155 | 9.69 MiB | 3.63 MiB (the changed edge tiles) |
| 10 rectangles added one by one | 40 | 2.50 MiB | 0.69 MiB |

A selection costs at most one 64 KiB tile per canvas tile (16 MiB on a
4096² canvas, whatever its shape); select all and invert cost nothing.
Its history holds the previous selection when it is replaced (moved, not
copied) and only the changed tiles otherwise. Feathering needs one tile
column's scratch (the band's rows in f32, ~4 MiB for 4096 rows) besides
its result.

## History

Each step holds the other state of what it changed (ADR 0015): a stroke
the previous content of the tiles it changed (a tile that did not exist
costs nothing until undone), a clear or a removed layer its whole surface
(moved, not copied), a depth conversion every layer's previous pixels.
Measured on a 4096² layer (`doc_bench`): a 25 px stroke over 1000 px
holds 2 MiB (8-bit) / 4 MiB (16-bit); a 500 px stroke across the canvas
12 / 24 MiB; clearing or removing a 64-tile layer 16 / 32 MiB; converting
320 tiles 80 MiB held, 160 MiB reserved.

A **merge down** (5B) holds the previous content of the lower layer's
changed tiles (copied) and the whole upper layer (moved): merging two
dense 4096² layers holds 128 MiB (8-bit) / 256 MiB (16-bit); when the
lower opacity or mask is baked, every lower tile counts as changed
(`layer_bench`).

The default budget is **200 steps and 1 GiB**, counted as each step's
worst case; the oldest steps go first. **A single step larger than the
budget is kept alone** (invariant H4), so the worst case is a depth
conversion of a big document: converting 100 dense 4096² layers from 8 to
16 bits holds the 6.25 GiB of 8-bit pixels in history next to the
12.5 GiB of 16-bit ones (TECHNICAL_DEBT #34).

## Transient memory

| Operation | Extra memory | Measured |
|---|---|---|
| Composition | none per tile (reuses `ComposeScratch`) | 0 allocations (`doc_bench`, `stamp_bench`) |
| Stroke | the stroke buffer's tiles (256 KiB per touched tile) until commit, then the history copy | `history_bench`, `doc_bench` |
| Save / load `.mirae` | one tile buffer + ZIP buffers | 1.3–3 MiB for any document size (`io_bench`) |
| Load | the document being built, next to the open one until it replaces it | — (by design: a failed load must not lose the open document) |
| PNG export | one row of tiles: width × 256 × 4 (8-bit) or × 8 (16-bit) bytes, + `ComposeScratch` | 4096 px wide: 4 / 8 MiB |
| PNG import | one decoded row and the new layer's tiles; an interlaced image is decoded whole first (bounded by `Limits::max_image_decode_bytes`, 2 GiB) | — |
| Depth conversion | the new depth's copy of every layer, while the old one moves into history | 8 → 16: old size + twice the old size |
| Merge down (5B) | the copies of the lower layer's changed tiles (kept for undo) + two tile buffers; the upper layer moves into history; no third full copy | two dense 4096² layers: +66 MiB peak (8-bit), +130 MiB (16-bit) (`layer_bench`) |

## Worst cases, in short

- Pixels: 100 dense 4096² layers at 16 bits = 12.5 GiB, plus up to 1 GiB
  of history, plus any single larger step.
- Loading: the open document and the loaded one coexist briefly.
- Depth conversion of a large document: three times its 8-bit size.
- Opening a hostile file is bounded by `Limits` (16 GiB of declared pixel
  data by default; lower it on smaller machines).
