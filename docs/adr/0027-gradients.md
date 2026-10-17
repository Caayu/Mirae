# 0027 — Gradients: the reference's easing and mixing, one function for preview and commit

Phase 5F. Written after the reference was observed and measured
([../parity/5F-gradients.md](../parity/5F-gradients.md), GD1–GD38): none
of the rules below was chosen before the observations.

# Context

A gradient paints a whole region at once from a few stops and a drag.
What it paints depends on how a pixel's position follows from the drag
(shape, what happens beyond the ends), how colours are mixed between
stops (the colour space, how alpha enters), how the result is quantized
(dithering, 8 or 16 bits), and how it meets the document (selection,
masks, alpha lock, history). It is also the first tool in Mirae whose
interactive preview changes the whole canvas at every pointer move.
The reference defines most of the behaviour; it previews only a guide
line, so a live preview is Mirae's own (the brief asked for one).

# Decision

1. **Colour side in `tela-color`** (`tela_color::gradient`): stops
   (`GradientStop`: position, opaque `Color`, opacity; `GradientStops`
   sorted stably, valid, at least one; equal positions make a hard
   edge), the colour at a position (`ColorRamp`) and a sampled form for
   speed (`RampTable`). The UI shows the same ramp without depending on
   the document.
2. **The reference's easing** (GD6, GD10): every segment between two
   stops is eased by `E(u) = 0.38·u + 0.62·q(u)`, `q` the quadratic
   ease-in-out — measured on the reference's undithered output (193 of
   211 integers exact, the rest ±1) and confirmed in linear light (rms
   0.12 level). No mode of the reference mixes linearly in `u`.
3. **Mixing** (`Mixing`): the sRGB-encoded values (the reference's
   "Padrão", GD6), linear light ("Linear", GD7) or OKLab of the linear
   values ("Perceptiva (compatível com Photoshop)", GD8, Ottosson 2020).
   The reference's own default "Perceptiva" with its brightness
   correction matches no public space tested (GD9); Mirae's default is
   OKLab, the nearest of the three (D-5F-1).
4. **Alpha** (GD11–GD13): colour and alpha are interpolated straight and
   independently; a stop of opacity 0 has no colour — its segment takes
   the other end's colour (the same as premultiplied interpolation there).
5. **Geometry** (`tela_document::gradient`): `GradientGeometry` (shape,
   start, end in document pixels) with linear (projection, GD17),
   radial (distance over the drag's length), elliptical (axis-aligned,
   radii `|dx|`, `|dy|`, GD19 without its rotation step), angular
   (clockwise turn from the drag's direction) and reflected (`|t|`);
   `GradientExtend` clamp / repeat / reflect / empty (the reference's
   four, GD2). The repeat wrap and the angular seam are blended by the
   pixel's footprint and the empty extend's edge is covered by area
   (GD14, GD20). Pixels are sampled at their centres; the press is the
   pointer's document position through the one view mapping (D-5F-2:
   the reference's colour phase differs by ≈ ½–¾ px).
6. **Paint rules are the brush's and the fill's** (ADR 0017, 0021,
   0026): source-over (GD29), source-atop with the alpha lock (GD30),
   erase by the gradient's alpha not stopped by the lock, on a mask
   reveal/hide by its alpha whatever the colours (GD33); the selection's
   coverage limits and scales what changes but not the geometry (GD31,
   GD32); opacity, blend mode and clipping are not baked (GD34, GD35).
7. **One per-pixel function for preview and commit.** `Job::each`
   renders the tile's paint, applies the rule and quantizes each changed
   pixel once at the document's depth with its dither threshold; the
   commit writes that value, the preview (`GradientPreview`, the
   compositor's overlay) decodes it back. The preview is therefore the
   commit bit for bit (GR5), and it borrows the document read-only, so
   it cannot change it (GR2).
8. **Dithering** (GD3, GD25–GD28): on by default; Mirae's own 8 × 8
   ordered (Bayer) matrix anchored to document pixels, threshold
   `(B + ½)/64` in `StoragePixel::from_premul_dithered`, one level of
   the storage's precision (1/255 or 1/65535), mean-preserving. The
   reference's pattern is not reproduced and its half-level bias is not
   copied (D-5F-4). 16-bit documents dither at 16 bits: never through
   8 bits (GR9).
9. **Speed** (measured, `gradient_bench`): the ramp is sampled per
   segment (`RampTable`, 4097 premultiplied points, linear inside a
   segment, within half a 16-bit level of the exact value), the extend
   rule chosen once per tile with a monomorphized loop, uniform and
   missed tiles short-cut. The app composes the preview's tiles on screen
   first and at most ~12 ms per frame, the rest in the next frames.
10. **History**: one `HistoryAction::Gradient` step through the existing
    tile edits (the changed tiles' previous content); none for a
    degenerate drag or a gradient that changes nothing (GD24). Refused,
    changing nothing, on hidden or locked layers and missing or disabled
    masks.
11. **App**: `CanvasTool::Gradient`; G cycles fill and gradient (BK4,
    GD1); a `GradientSession` captures the tool's settings and the main
    and sub colours at the press and keeps the press in document
    coordinates; Shift snaps to 45° (GD23); Escape cancels (the reference
    ignores it, GD38); the eraser end or eraser mode erases. Settings and
    stops are session state (not in the document or its file).

# Consequences

- The reference's Standard, Linear and Perceptual (PS) gradients are
  reproduced within 1.5 levels in column means at both depths; circles,
  the ellipse, repeat and reverse within ~1 px of position; alpha,
  transparent stops, source-over, alpha lock, selections and masks within
  1.5–2 levels (`gradient::parity`, the record's Part 3).
- A full 2048² preview frame costs ~100 ms and a commit ~80 ms; 4096²
  four times that (numbers in docs/performance.md); undo/redo are tile
  swaps (~0.01 ms). The preview allocates nothing per frame.
- No format change: a gradient is ordinary tiles.
- Not implemented: the reference's own perceptual mixing and brightness
  correction, the ellipse's rotation step, its "radius = min(|dx|, |dy|)"
  circle, the gradient layer ("Onde criar"), the tool's own opacity and
  blend mode, the mixing-rate curve, gradient maps (D-5F-1…10).

# Alternatives considered

- **Linear `u` (no easing)**: every mode of the reference eases (GD6);
  rejected by measurement.
- **Premultiplied interpolation everywhere**: misses GD12 by ~40 levels
  (straight G 83 instead of 126 at the middle of red → white at 50 %).
- **Mixing only in sRGB-encoded values**: the reference offers three
  public mixes and its default is perceptual; Mirae offers the three it
  can reproduce.
- **Reproducing the reference's dither**: its generator is not public and
  the brief forbids reverse-engineering it; its half-level bias would
  darken every dithered gradient.
- **Dithering 16-bit documents at 8-bit amplitude**: would add 8-bit
  noise to 16-bit precision (the brief: never sacrifice 16-bit
  precision to imitate 8-bit artefacts).
- **An f32 preview and a quantized commit** (as transforms do, TR4):
  would make GR5 approximate; quantizing in the preview costs little.
- **A lookup table over the whole `t` range**: smears hard edges; per
  segment it keeps them.
- **Recomposing every covered tile per frame**: ~0.4 s per frame at
  4096²; a per-frame budget with on-screen tiles first keeps the UI
  responsive and converges in a few frames.
- **GPU gradients**: not needed for the measured costs (the brief
  excluded them without an unavoidable reason).
