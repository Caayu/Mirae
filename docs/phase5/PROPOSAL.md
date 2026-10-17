# Proposal: the next macrophase

> Status 2026-10-17: the owner approved **5A (colour)** only; it is
> complete ([PLAN.md](PLAN.md)). Everything else below remains a
> proposal, not approved and not started.

Written at the end of phase 4 (4G) for the owner's decision. Nothing here
is started. The goal is unchanged: professional parity with Clip Studio
Paint, clean-room. The order below is chosen by **dependencies and parity
value**, not by ease.

## Where phase 4 leaves Mirae

A real raster document: layers with stable ids, undo/redo, 17 measured
blend modes, masks, clipping, alpha lock, 8/16-bit storage, `.mirae` files,
PNG exchange, and the desktop file workflow — all measured against the
reference where it defines behaviour (docs/PARITY.md, "Phase 4 parity
summary"). What a working illustrator still cannot do in Mirae, in order
of how often they would hit it:

1. **Choose a colour.** The app paints black only (TECHNICAL_DEBT #22).
   This blocks real use and the colour-dependent app checks (D-4D-5).
2. **Select, move and transform.** No selection, no transform: imported
   images cannot be placed (D-4F-7); masks cannot come from a selection
   (D-4D-2); nothing can be repositioned.
3. **Fill.** No bucket fill: flats are painted by hand.
4. **Organize layers.** No rename field, drag reorder, thumbnails, merge
   down or folders (D-4A-3/4, D-4C-3); folders need the
   translucent-backdrop rule the reference uses (D-4C-1, unmeasured).
5. Gradients, vector layers, rulers/perspective, comic tools, colour
   management — valuable, but each builds on 1–4.

## Dependencies

```
colour (main/sub, picker, eyedropper) ──────────────┐
D-4C-1 probe (translucent backdrop) ─► merge down ─► layer folders ─► GPU compositing decision
selection model ─┬─► strokes limited to a selection (commit path, P2)
                 ├─► mask from selection (D-4D-2)
                 ├─► transform (layer, selection, imported images)
                 └─► fill (bucket, fill selection) ─► gradients (dithering, E10)
transform ─► vector layers (control points), rulers/perspective
```

Selections touch the stroke commit path (a selection limits where paint
lands) and the file format (whether a document keeps its selection —
observe first); transform needs resampling whose quality must be
measured against the reference.

## Recommended phase 5 — "Editing fundamentals"

Each subphase observes the reference first and ends with a parity gate,
as in phase 4.

| Subphase | Scope | Why here |
|---|---|---|
| 5A Colour | Main/sub colour, a colour panel (HSV wheel, RGB/HSV values), eyedropper (canvas and layer sampling as the reference does), colour history; re-run the colour-dependent 4C/4D app checks | Unblocks real use; small; no dependency |
| 5B Layer workflow + D-4C-1 | Probe the reference's blending over a translucent backdrop (no paper) and fit it; merge down; rename field; drag reorder; thumbnails; the delete-confirmation decision; transparent export of blend layers (D-4F-5) | Closes known differences; prerequisite for folders |
| 5C Selections | Selection model (8-bit coverage, sparse tiles like masks), rectangle, ellipse, lasso, polygon, magic wand (tolerance measured), add/subtract/intersect/invert, strokes limited to the selection, mask from selection, history; persistence decided after observing the reference | Foundation for transform, fill, masks |
| 5D Transform | Move layer/selection, scale, rotate, flip, free transform; resampling quality measured against the reference; placing imported images | Needs 5C; resolves D-4F-7 |
| 5E Fill | Bucket fill (contiguous or not, tolerance, current layer vs all layers, gap closing as measured), fill selection | Needs 5C |
| 5F Gradients | Linear/radial, foreground→background, dithering decided by measurement (E10) | Needs 5C/5E's region model |
| 5G Integration | End-to-end scenarios with the new tools, benchmarks, documentation | As 4G |

## After phase 5 (order recommended)

1. **Layer folders**: pass-through and isolated folders, folder blend
   modes, clipping to folders, folder masks; with them, the **GPU
   compositing decision** using the 4G scaling numbers (100 layers cost
   ~12 ms per tile in Normal and ~44 ms in Soft Light on one core).
2. **Vector layers** (the reference's inking workflow: editable strokes,
   line width correction, vector eraser).
3. **Rulers, symmetry, perspective.**
4. **Advanced brushes**: colour mixing, wet media, brush management and
   preset persistence (#8).
5. **Comic tools** (frames, balloons, text) and **colour management**
   (soft proofing, CMYK export), each when its workflow is targeted.

## Housekeeping that is not a macrophase

- Renaming the `tela-*` crates (and the `tela` binary) to `mirae-*`:
  mechanical, touches every manifest, import and doc; do it as its own
  commit series between phases, not mixed with features.
- Background saving and autosave (#28, #29) can be scheduled into any
  phase; the serializer is ready for it.
- Physical tablet validation (#1) whenever hardware is available.
