# 0025 — Transform: an explicit affine session, the reference's resampler

Phase 5D. Written after the reference was observed and measured
([../parity/5D-transform.md](../parity/5D-transform.md), T1–T45): none of
the rules below was chosen before the observations.

# Context

Moving, scaling, rotating and flipping content is the first edit that
resamples pixels. It touches the document model (what is transformed,
what stays), quality (the resampler, premultiplied alpha, 16-bit
precision), history (one step for a whole session), the selection (which
must follow the content), masks, the preview/commit invariant (P2) and the
app (a modal session with handles, interruptions, file commands). The
reference defines the behaviour and its pixels.

# Decision

1. **An explicit, transactional session.** The app holds a
   `TransformSession` (outside the document, its history and its file):
   the `TransformSource` taken from the document when it began and a list
   of `TransformParams` edits (undo/redo inside the session walk them,
   T13). The document does not change until `Document::apply_transform`
   commits **one** history step; a cancel drops the session (TR1–TR3).
2. **The map**: `x' = pivot + translate + R(θ)·S(scale)·(x − pivot)` in
   document pixels (`TransformParams` → `Affine`), θ clockwise on screen,
   a flip is a negative scale (T15), the pivot movable without moving the
   content (T9). Handles are hit and drawn in screen space (constant size,
   T44); the box is mapped through the one view transform (T45).
3. **What is transformed** (T27–T33): without a selection, the active
   layer's content bounds and its mask (linked, as in the reference),
   whatever the edit target; with a selection, the box is the selection's
   bounds, `pixel · s` moves and `pixel · (1 − s)` stays, the moved part
   composed `over` what stays; the selection follows the content (the same
   resampling, 8-bit, clipped to the canvas). The mask moves only without
   a selection (the reference's behaviour with both was not observed).
   Hidden and locked layers are refused, an empty layer is "nothing to
   transform", the alpha lock does not apply (T34–T37); pixels beyond the
   canvas are kept (T39).
4. **The resampler is the reference's measured one** (T16–T22): Keys'
   cubic with `a = −1` on **premultiplied** colour, transparent outside the
   box, the sample clamped to the edge pixels' centres, un-premultiplied
   by the raw alpha and clamped; a 2 × 2 pyramid while both scales are ≤ ½;
   axis-aligned results have their edges rounded to whole pixels (moves
   and flips are exact copies), rotated ones are anti-aliased by distance
   (`clamp(0.5 + d)` per side). Nearest neighbour (ties to the lower
   pixel, hard edges) is the second option, for pixel art. The reference's
   bilinear and "average colours" modes are not implemented (D-5D-3).
5. **One function for preview and commit.** `TransformSource::
   transform_pixels_tile` (and `transform_mask_tile`) compute a tile's new
   content in `f32` from the stored tile; the preview composes it through
   a new `OverlayPaint::Layer` (pixels and mask), the commit quantizes it
   once at the document's depth (TR4, TR6). 16-bit documents never pass
   through 8 bits.
6. **The source is a dense premultiplied `f32` snapshot** of the box
   (16 bytes per pixel, plus 4 per pixel of coverage or mask when present)
   with lazily built pyramid levels; the commit backs up only the changed
   tiles (history as for a stroke).
7. **Interruptions** (T41–T43): choosing another layer or tool asks
   "Apply the transform?" (Yes / No / Cancel); New, Open, Save, Save As,
   import and depth conversion wait; export is allowed; closing the window
   asks the same question first (Mirae's choice: the reference disables its
   quit command). Tool shortcuts do nothing during a session.

# Consequences

- The reference's measured images are reproduced: scales within 3/255
  (the reference stores straight colour at 8 bits after the transform,
  Mirae premultiplied), nearest neighbour, flips, moves and moves through
  a hard selection exactly, through a feathered selection within 3/255,
  rotations inside a one-pixel edge ring (D-5D-1), the rotated
  selection's edges except two pixels per corner (D-5D-2).
- Preview equals commit bit for bit after the single quantization (60
  combinations), and through the compositor within 1/255.
- No format change: a committed transform is ordinary pixels, mask tiles
  and a selection; sessions are never saved.
- Cost (`transform_bench`): ~64 ns per destination pixel for the cubic on
  one core: a 256² selection previews in 2–14 ms, a 2048² layer rotated
  in ~270 ms per change (D-5D-6, debt #40); the snapshot of 2048² content
  is 64 MiB.
- 5E–5G and later work (free transform, perspective, mesh) can reuse the
  session, the source and the overlay; only the map and the sampler
  would change.

# Alternatives considered

- **Transforming the document incrementally while dragging**: every
  move would resample the previous result (cumulative blur) and create
  history; rejected by the brief and by the reference's one-step history.
- **Straight-colour interpolation**: shows the hidden colour of
  transparent pixels and dark fringes (scored 2.2 mean, 74 max against the
  reference).
- **Catmull–Rom (`a = −0.5`), Mitchell, Lanczos, bilinear, area
  filters** as the default: each scored worse against the reference's
  default (0.3–14 mean); the reference's own bilinear is documented, not
  implemented.
- **Exact-area anti-aliasing of rotated edges**: its ramp steps by tan θ;
  the reference's by sin θ (distance).
- **A storage-depth snapshot** (4 or 8 bytes per pixel, decoded per tap):
  a quarter of the memory, but every bicubic tap would decode and multiply
  by the coverage; kept as a direction if memory matters (debt #40).
- **Moving the mask with a selection too**: unobserved in the reference;
  not guessed.
