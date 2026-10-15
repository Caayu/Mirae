# 0018 — 16-bit storage: per-document depth, same values, more precision

Phase 4E. Written after the reference was observed
(docs/parity/4E-16bit.md, part 1) and after banding, memory and speed
were measured (`banding`, `depth_bench`; docs/performance.md).

# Context

Layers stored premultiplied, sRGB-encoded RGBA8. Composition already
runs in f32 (ADR 0004, 0016), so 8-bit rounding happens only where values
are stored: at every stroke commit. Repeated faint paint (glazing,
airbrush passes, soft edges) therefore loses what falls below half a
level each time. The phase plan proposed `[u16; 4]` storage per document,
a `StoragePixel` trait, a generic commit and 8-bit masks (D13), with the
8-bit default (D12) to be re-evaluated after measuring.

The reference was observed first: it has **no** 16-bit documents, no
depth conversion and 8-bit exports only (E1–E3); it refuses 16-bit PSDs
(E4); a 16-bit PNG opens with its low byte dropped (E5, E6); 8-bit
content survives exactly (E7); its repeated low-opacity paint and
airbrush passes show 8-bit accumulation artefacts (E8, E9). A 16-bit
document is therefore a capability beyond the reference; parity applies
to Tela's 8-bit documents, which keep the 8-bit behaviour.

# Decision

1. **Format.** A document stores its raster layers as premultiplied
   **sRGB-encoded** RGBA with `u8` or `u16` channels: the same values in
   the same space (ADR 0001, 0016), quantized more or less finely. No
   float storage, no linear light, no colour management in this phase.
   `[u16; 4]` was confirmed only after checking that it keeps the f32
   composition path unchanged (composition decodes and is otherwise
   identical), that 8 → 16 → 8 is exact, that every blend formula is
   unchanged, and that it costs ~0–7 % in composition (below).
2. **Depth per document.** `PixelDepth { Rgba8, Rgba16 }` is a property
   of `Document`; every raster layer has that depth (invariant D1).
   Layers do not choose their own depth.
3. **Masks stay 8-bit** at every depth (D13): a document's raster depth
   is not its mask depth.
4. **Conversion rules** (`tela_color`, tested on every value):
   - 8 → 16: `v · 257`, exact; widened values decode to bit-identical
     f32, so 8-bit content composes identically after widening.
   - 16 → 8: nearest level, `(v + 128) / 257` (no ties exist); equal to
     decoding and quantizing with `unorm8`, so a converted single layer
     looks as it did. 8 → 16 → 8 is the identity.
   - 16-bit decode is `(v · 1/257) · 1/255`: a single multiplication by
     `1/65535` differs in the last bit for 134 of the 256 widened levels
     and would break the bit-identity above.
   - The reference's only visible 16 → 8 reduction (opening a 16-bit
     image) drops the low byte instead (E5). That is its import, not a
     depth conversion it offers; Tela's conversion rounds, and the
     difference is recorded (D-4E-2) for 4F's 16-bit import.
5. **Types.** `tela_color::StoragePixel` (implemented for `[u8; 4]` and
   `[u16; 4]`: decode, quantize, exact 8-bit equivalents) and
   `PixelDepth`; `with_pixel_type!` turns a run-time depth into a type
   once per call. Generic code is monomorphized per depth; there is no
   dynamic dispatch per pixel and no generic `Document<P>`. In
   `tela-document`, `LayerPixels` holds the storage and is the only
   module that matches on the depth; `RasterLayer::pixels()` returns it
   read-only with typed views (`rgba8`, `rgba16`, `surface::<P>`).
6. **One pixel-mutation path, typed.** `Document::modify_tiles<P>` keeps
   the 4A rules (declared tiles only, sparse, hidden/locked, one history
   step) unchanged and refuses an edit written for the other depth with
   `LayerError::WrongDepth` before changing anything. The document never
   converts tiles behind a caller's back.
7. **Brush engine unaware of documents.** The stroke is the same f32
   mask with the same paint rules (`paint_over`, `alpha_over`); only
   `StrokeBuffer::commit_tile<P>` decodes and quantizes through
   `StoragePixel`. Preview is f32 at every depth; commit quantizes once to
   the storage (invariant P2 at both depths).
8. **History.** Tile backups keep the depth they were recorded at; costs
   use the real tile size (512 KiB per 16-bit tile); H1–H11 hold at both
   depths. `convert_depth` is one step (D14): every layer's previous
   pixels move into history (no copy), its cost is the larger of the two
   depths' buffers, undo/redo swap them back bit-exactly. It applies to
   hidden and locked layers (the depth is a document property, not a
   layer edit), leaves masks, ids, order, properties and the sparse
   layout untouched, and does nothing (records nothing) for the current
   depth.
9. **Display stays 8-bit.** `compose_tile_into` composes in f32 and
   outputs RGBA8 for the renderer at every storage depth: storage
   precision is not display precision. A 16-bit document holds more
   than the screen shows; no dithering, no 10/16-bit display.
10. **Default: 8-bit** (D12 decided). New documents are `Rgba8`;
    16-bit is one command away (New 16-bit, To 16-bit), and 8 → 16 is
    lossless at any time.

# Consequences

Measured (docs/performance.md, phase 4E):

- **Quality.** Repeated faint strokes stay within 0.75 of a display level
  of the exact result at 16 bits; at 8 bits they miss by 25–136 levels
  and lose 24–48 edge pixels (60 strokes at 3 %, 200 at 1 %, 400 at
  0.4 %). A ×10 gain over a dark gradient shows 103 levels with jumps
  of 1 at 16 bits, 11 levels with jumps of 11 at 8 bits. A single stroke
  or a plain gradient shows the same at both depths. In the running app,
  60 strokes of a 3 % soft brush give a smooth edge at 16 bits and, at
  8 bits, a hard edge and an 8-pixel plateau — the artefacts observed in
  the reference (E9). Preview and commit differ in 5 304 channels at
  16 bits against 328 755 at 8 bits over the 126-case matrix (both
  within 1/255).
- **Speed.** Composition 0.99–1.07× the 8-bit time (1 and 10 layers,
  modes, mask, clipping), 0 allocations; stroke commit with its history
  copy 1.2–1.5×; undo/redo unchanged (swaps, microseconds); converting
  640 tiles ~95 ms, its undo ~0.1 ms. 8-bit paths are unchanged or faster
  (decoding moved to a separate loop).
- **Memory.** Pixel tiles and their history double (256 → 512 KiB);
  masks (64 KiB tiles) and the compose scratch (3.5 MiB) do not change.
  A full 4000 × 3000 layer is 48 MiB at 8 bits, 96 MiB at 16; 100 such
  layers 4.8 GiB against 9.6 GiB. The 1 GiB history budget holds half as
  many 16-bit tiles.
- **Parity.** 8-bit documents behave as before (goldens, 4C and 4D
  parity tests unchanged). 16-bit documents match the reference's 8-bit
  measurements within 2/255 (blend modes: 3 of 3 400 rows move from 1 to
  2/255, as the unrounded formula does — consistent with the reference's
  numbers carrying its own 8-bit rounding;
  masks and clipping within 2/255 as at 8 bits).
- **Default rationale.** 16-bit improves exactly the accumulation cases
  and costs nothing visible elsewhere, at twice the memory and a slower
  commit. The reference is 8-bit, so 8-bit documents reproduce what
  artists of the reference expect, at half the memory and history; and
  widening later is exact. The owner can make 16-bit the default by
  changing `Document::new`; nothing else depends on it.

# Alternatives considered

- **f16 or f32 storage.** f16 has an 11-bit mantissa — coarser than u16
  near 1 — and f32 quadruples memory; neither is needed: u16 already
  keeps every measured accumulation within a display level. Rejected
  without evidence of need.
- **16 bits in linear light** (or straight alpha). Changes appearance
  and the blend space chosen by measurement (ADR 0016); out of scope
  (no colour management in 4E).
- **Per-layer depth.** More states (mixed stacks, clipping across
  depths, conversion of single layers) with no observed use; the
  reference has no depth at all.
- **`Document8` / `Document16` or a generic `Document<P>`.** Parallel
  maintenance or a type parameter leaking into every API (app, UI,
  history, undo) for something only storage and commit need.
- **Converting inside `modify_tiles`** (accepting 8-bit edits on 16-bit
  documents). Silent precision loss of untouched pixels; refused instead.
- **Narrowing by truncation** (`v >> 8`, as the reference's import does).
  Biased darker by half a level on average and changes how a converted
  document looks; the nearest level keeps the display.
- **Dithering on conversion or display.** Not part of this phase; would
  make conversions non-deterministic in appearance and hide the banding
  the measurements are meant to show.
- **16-bit masks.** Excluded by D13 for this phase. Repeated faint mask
  strokes will band at 8 bits the way pixel strokes did; that was not
  measured here and is recorded as D-4E-4 to revisit with evidence.
- **16-bit default.** Possible (see consequences); rejected for now for
  memory and reference parity, and because widening later is lossless.
