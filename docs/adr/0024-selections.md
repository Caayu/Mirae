# 0024 — Selections: document state, 8-bit coverage, measured operators

Phase 5C. Written after the reference was observed and measured
([../parity/5C-selection.md](../parity/5C-selection.md), S1–S32): none
of the rules below was chosen before the observations.

# Context

Selections limit where edits land and are the basis of transform, fill,
gradients and copy/paste in later subphases. They touch the document
model (what a selection is, when it exists), the brush's shared
preview/commit rule (invariant P2), history (ADR 0015), the file format
(ADR 0019, v1 is a compatibility contract) and the view (tools work at
any zoom and rotation). The reference defines most of the behaviour.

# Decision

1. **The selection belongs to the document**, not to a layer, the UI or
   a hidden layer: `Document::selection() -> Option<&Selection>`. It
   survives switching, hiding, adding and removing layers (S13) and
   never changes the composite: a selection is not a mask.
2. **Representation**: an 8-bit coverage per pixel (0 = not selected, 255
   = selected, between = anti-aliased or feathered), clipped to the
   canvas (S12, S20), kept as a sparse `TiledSurface<u8>` of
   `coverage XOR fill` with `fill` 0 or 255: an absent tile is `fill`
   everywhere, so **Select All and Invert store and touch no tile**
   (Invert flips `fill`; stored values invert with it). Stored tiles are
   never blank and are 0 off the canvas. Bounds are computed from the
   tiles on demand, never cached, so they cannot disagree with them.
3. **No selection, empty, full**: an empty selection never exists. A
   result without coverage (invert of everything, subtracting
   everything, an empty intersection, a click in New mode) is **no
   selection** (S5, S17, S18). No selection does not limit edits; a full
   selection limits them to the canvas, which equals no selection
   on the canvas (SEL2, SEL3).
4. **Shapes** are rasterized by one even-odd polygon filler (S11):
   anti-aliased coverage on 16 sub-scanlines with exact horizontal
   coverage, otherwise pixel centres. Rectangles snap to whole pixels
   (S7, S8); ellipses are polygons within 0.01 px of the curve with its
   exact area. Tools keep their points in screen pixels and map them
   through the view **once**, at the end of the drag, so rectangles and
   ellipses follow the screen axes in a rotated view (S9). Anti-aliasing
   is per tool: rectangle off, ellipse and lasso on (S3; the reference's
   Weak/Medium/Strong were bit-identical, S10).
5. **Operators** (measured on doubly partial pixels, S14–S16): Add
   `min(255, a + b)`, Subtract `max(0, a − b)`, Intersect `⌊a·b/255⌋`,
   on 8-bit coverages. Shift, Alt and Shift+Alt at the press choose Add,
   Subtract and Intersect; Shift during a drag squares the shape (S4).
6. **Feather** (S19–S21): a sampled, normalized discrete Gaussian with
   `σ = 0.3 × strength`, radius `⌈3σ⌉`, renormalized over the canvas,
   applied separably and only where a stored value is within reach.
7. **Editing through a selection** (S22–S26): the selection multiplies
   the finished stroke's alpha, `m·s`, for paint, alpha-preserving paint,
   erase and mask strokes; not per dab. It is one optional per-tile
   coverage (`tela_canvas::TileCoverage`) passed to the brush's shared
   rule (`paint_over_tile_within` for the preview,
   `commit_tile_within` through `Document::modify_tiles_in_selection`
   for the commit, and the alpha-channel equivalents), so preview equals
   commit by construction and the brush never sees the document. Full
   coverage takes the unchanged unrestricted path, empty coverage
   changes nothing. Selection commands (and undo) are ignored during a
   stroke, as in the reference (S27): the stroke sees the selection it
   began with without a snapshot.
8. **History**: every change is one step (S28, S29), swap-based (ADR
   0015): replacing, selecting all, deselecting and intersecting move
   the whole selection into the step; adding, subtracting and
   feathering move only the changed tiles; inverting records a flag.
   Selection changes are modifications (the revision moves, S32).
9. **Persistence**: the active selection is **not saved**, like the
   reference (S30). `.mirae` v1 does not change: a document with a
   selection saves the same bytes as without one; a loaded document has
   no selection.
10. **Display**: the marching ants follow the 50 % contour (S31),
    traced once per selection change (`Selection::outline`), mapped
    through the view each frame and painted by the UI under its panels.
    Presentation only: nothing in the document, the composite or an
    export.

# Consequences

- The reference's measured results are reproduced: rectangles and the
  rotated-view rectangle exactly, the operators exactly on its own
  inputs, the feather within 2/255 from 1 to 50 px, strokes through a
  selection exactly (`selection::parity` tests). Ellipse edge pixels
  differ by up to 68/255 (area within 0.4 %), the lasso's rounded tips
  are not reproduced (D-5C-1, D-5C-2).
- A full selection is the unrestricted path bit for bit on the canvas;
  beyond the canvas it keeps strokes from leaving pixels (debt #14).
- Select All, Invert and Deselect cost no memory and microseconds on any
  canvas; a 4096² feathered selection is ~10 MiB; history holds only
  changed tiles (`selection_bench`).
- 5D–5F can query `Document::selection_coverage`, `selection_tile` and
  `Selection::bounds`; transform will need to move the stored tiles and
  the fill together.
- The `.mirae` format, the brush's per-dab path and composition are
  unchanged; 0 allocations per dab.

# Alternatives considered

- **`bool` per pixel**: cannot represent anti-aliasing or feather, both
  measured in the reference.
- **Selection as a hidden layer or a layer mask**: ties it to a layer,
  enters composition, saves it in files; the reference keeps it apart.
- **Storing Select All as every canvas tile**: 16 MiB on a 4096² canvas
  for no information; the XOR fill costs nothing.
- **An explicit empty selection** distinct from none: the reference has
  no such state.
- **Max/min or `a + b − ab` / `a·b` operators**: rejected by measurement
  (16–22/255 off).
- **Limiting each dab**: builds up above the coverage; the reference
  ends exactly at the coverage after a dense stroke.
- **Snapshotting the selection per stroke**: unnecessary while selection
  changes are refused during a stroke (they are in the reference too).
- **Saving the selection in an optional manifest field**: possible in v1
  (unknown fields load with a warning in older builds), but the
  reference does not keep it and the owner's rule was not to change v1
  without need. Revisit with "reselect" or saved selections.
