# Phase 5 — plan

The phase-5 proposal ([PROPOSAL.md](PROPOSAL.md), "Editing fundamentals")
is the plan's outline. The owner approves **one subphase at a time**;
nothing else is started.

| Subphase | Status | Record |
|---|---|---|
| 5A Colour | **Approved 2026-10-17, complete** (parity gate recorded) | below; [parity/5A-color.md](../parity/5A-color.md); [ADR 0021](../adr/0021-paint-colour-and-colour-tool-state.md) |
| 5B Layer workflow + D-4C-1 probe, merge down | **Approved 2026-10-17, complete** (parity gate recorded) | below; [parity/5B-transparent-compositing.md](../parity/5B-transparent-compositing.md); [ADR 0022](../adr/0022-blending-over-translucent-backdrops.md), [ADR 0023](../adr/0023-merge-down-and-layer-workflow.md) |
| 5C Selections | **Approved 2026-10-17, complete** (parity gate recorded) | below; [parity/5C-selection.md](../parity/5C-selection.md); [ADR 0024](../adr/0024-selections.md) |
| 5D Transform | Proposed, not approved | [PROPOSAL.md](PROPOSAL.md) |
| 5E Fill | Proposed, not approved | [PROPOSAL.md](PROPOSAL.md) |
| 5F Gradients | Proposed, not approved | [PROPOSAL.md](PROPOSAL.md) |
| 5G Integration | Proposed, not approved | [PROPOSAL.md](PROPOSAL.md) |

## 5A — Colour (approved brief, summarized)

Turn Mirae from a black-only painter into a colour painting system:
main and sub colour, swap and reset, a colour panel (HSV picker, RGB,
HSV, hex), the eyedropper, recent colours, basic swatches; colour as data
of the stroke (captured at its start, never read from global state
during it); correct at 8 and 16 bits; alpha lock, masks, clipping and
blend modes with colour; the colour-dependent 4C/4D app checks; a parity
gate against the reference. Out of scope: fill, gradients, selections,
transform, colour management/ICC/wide gamut/linear/HDR/proofing, CMYK,
Lab workflow, palette libraries, cloud palettes, materials, layer
thumbnails, merge down.

### Decisions (details in ADR 0021)

| # | Decision | Reason |
|---|---|---|
| A1 | `Color`: opaque, straight, `f32`, sRGB-encoded, no alpha; hex `#RRGGBB` | The reference's colour has no alpha (O7); brush opacity/flow already decide how much paint lands; `f32` keeps 16-bit precision (O6) |
| A2 | Stroke source `(C·m, m)`; `m` = coverage × flow build-up under the opacity ceiling (unchanged) | One explicit product; identical to the old path at alpha 1 (goldens unchanged) |
| A3 | Colour state (main, sub, active slot, recent, swatches) is app-owned session tool state; not in `.mirae` v1, not history | The reference records no colour change (O8); colour belongs to the tool |
| A4 | `X` switches the active slot (reference); `D` resets to black on white (unbound in the reference, free in Mirae); Swap is a panel button | Reference shortcuts where they exist; no conflict with Mirae's keys |
| A5 | Eyedropper: canvas (composite over the paper) and layer; right-click and Alt with any tool; document pixel at any zoom; alpha 0 keeps the colour | O10–O14; Mirae has no transparent colour (D-5A-1) |
| A6 | Recent colours: newest first, unique, 24, from strokes and picks | O9 (capacity Mirae's) |
| A7 | Swatches: 16 built-ins of our own, add/remove, 64 max, not persisted | No preferences system yet (debt #35) |
| A8 | No new golden | The integrated session golden already paints eight colours with independent channels; per-pixel colour math is covered by numeric tests |

### Outcome

Implemented as planned; gate recorded in [PARITY.md](../PARITY.md) and
[parity/5A-color.md](../parity/5A-color.md). Tests 401 → 434. Colour adds
no measurable cost (`stamp_bench` colour table). Remaining differences:
D-5A-1…8.

## 5B — Layer workflow, translucent backdrops, merge down (approved brief, summarized)

Probe the reference's blending over a translucent backdrop without paper
(D-4C-1) with a rich corpus (backdrop alpha 0, 1/255, 25, 50, 75, 100 %;
black, white, red, green, blue, 37/149/213; all 17 modes; 100/60/25 %),
compare public hypotheses and adopt a rule only if it explains the data;
then observe and implement merge down (opacity, modes, masks, clipping,
locks, hidden, paper, names, active layer, undo/redo; one step; both
depths; transactional; no format change), the rename field, drag-and-drop
reorder (clipping rules unchanged, ids not indices), the layer panel's
selection feedback, thumbnails only if simple; invariants MD1–MD6,
LR1–LR3, RN1; performance and memory measured; a real-app session at
8 and 16 bits; the parity gate. Out of scope: selections, transform,
fill, gradients, layer folders, vector layers, GPU compositing, colour
management, more blend modes, autosave, a format v2.

### Decisions

| # | Decision | Reason |
|---|---|---|
| B1 | D-4C-1 resolved by a measured rule (ADR 0022): Multiply W3C; the other modes "nested"; additive alpha for Screen, Color Dodge, Hard Light, Soft Light | 45 900 combinations within 2/255; W3C up to 47/255 off; six candidates scored and free per-alpha fits |
| B2 | Opaque backdrops keep the pre-5B expression (a W3C-only loop over the paper; alpha ≥ 1 − 2⁻²⁰ counts as opaque) | Everything on screen bit-identical and as fast as 5A |
| B3 | Merge down composes the pair alone; the lower layer keeps id, name, mode, clip, alpha lock; opacity baked; masks as the reference (question for the lower one) | MO1–MO17 |
| B4 | Merge is one step built from the existing history changes | No new mechanism; only changed tiles held |
| B5 | Consecutive renames merge into one step | Reference RN4 |
| B6 | Drag reorder = `move_layer` with a relative placement by id | Document stays the authority (LR3) |
| B7 | No thumbnails in 5B | Invalidation and per-row textures are not simple enough (D-5B-6) |
| B8 | No format change | Merged documents use existing structures (round-trip test) |

### Outcome

Implemented as decided; gate recorded in [PARITY.md](../PARITY.md) and
[parity/5B-transparent-compositing.md](../parity/5B-transparent-compositing.md).
Tests 434 → 455. Remaining differences: D-5B-1…9. 5C not started.

## 5C — Selections (approved brief, summarized)

A professional raster selection system as the base for transform, fill,
gradients and copy/paste: rectangle, ellipse and freehand lasso; Replace,
Add, Subtract, Intersect; Select All, Deselect, Invert; feather; the
selection as document state (not a layer, a mask, UI state or a
temporary bitmap), 8-bit coverage, sparse, with Select All and Invert
cheap on large canvases; marching ants; strokes, erasing and mask
strokes limited by it (preview == commit, 0 allocations per dab); undo
and persistence as the reference behaves (no automatic v2 of `.mirae`);
zoom and rotation through one screen→document mapping; the reference
observed first for every rule (28 probes listed in the brief); property
and random tests, the preview == commit matrix, performance and memory
measured, a real-app session, invariants, ADR, the parity gate. Out of
scope: transform, moving pixels, copy/cut/paste, fill, gradients, magic
wand, colour selection, quick mask, selection from a layer, vector
selection, GPU processing.

### Decisions (details in ADR 0024)

| # | Decision | Reason |
|---|---|---|
| C1 | `Document::selection() -> Option<Selection>`; 8-bit coverage stored as `coverage XOR fill` in sparse tiles | Partial coverage measured (AA, feather); Select All and Invert store nothing |
| C2 | An empty result is no selection | S5, S17, S18 |
| C3 | Shapes: one even-odd polygon filler with 16 sub-scanlines; rectangles snap to pixels; screen points mapped through the view once | S7–S12 |
| C4 | Operators: saturating sum, saturating difference, truncated product | Exact on the reference's doubly partial pixels (S14–S16) |
| C5 | Feather: discrete Gaussian σ = 0.3·px, radius ⌈3σ⌉, renormalized over the canvas | Within 2/255 from 1 to 50 px (S19–S21) |
| C6 | Strokes scaled by the coverage (`m·s`) in the shared preview/commit rule (`TileCoverage` in `tela-canvas`) | S22–S26; the brush stays document-free |
| C7 | Selection changes are history steps, swap-based; ignored during a stroke | S27–S29 |
| C8 | Not saved; `.mirae` v1 unchanged | S30 (the reference drops it even in its own format); the owner's rule on v1 |
| C9 | Tool architecture: `CanvasTool` (brush, eyedropper, selection) in the app; `SelectionTool`/`SelectionDrag` hold the selection tool's state | One place decides what a drag does, no framework |
| C10 | Invariants SEL1–SEL10 (S1–S5 are the sampling invariants) | Name clash |

### Outcome

Implemented as decided; gate recorded in [PARITY.md](../PARITY.md) and
[parity/5C-selection.md](../parity/5C-selection.md). Tests 455 (+1 for
a 5B defect the baseline run found) → 503. Remaining differences:
D-5C-1…8. 5D not started.
