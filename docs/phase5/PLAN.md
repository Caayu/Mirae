# Phase 5 — plan

The phase-5 proposal ([PROPOSAL.md](PROPOSAL.md), "Editing fundamentals")
is the plan's outline. The owner approves **one subphase at a time**;
nothing else is started.

| Subphase | Status | Record |
|---|---|---|
| 5A Colour | **Approved 2026-10-17, complete** (parity gate recorded) | below; [parity/5A-color.md](../parity/5A-color.md); [ADR 0021](../adr/0021-paint-colour-and-colour-tool-state.md) |
| 5B Layer workflow + D-4C-1 probe, merge down | Proposed, **not approved, not started** | [PROPOSAL.md](PROPOSAL.md) |
| 5C Selections | Proposed, not approved | [PROPOSAL.md](PROPOSAL.md) |
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
