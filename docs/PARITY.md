# Parity

Tela's long-term goal is very high **functional, behavioural, qualitative
and performance parity with Clip Studio Paint** for professional
illustration workflows, implemented clean-room. This document is the
single place that says, per subsystem, how close Tela is and how we know.
It is **mandatory reading** for every new session (see AI_HANDOFF.md).

No percentages: a number like "80 %" cannot be measured honestly. Each
cell holds one of four states, and every state above "Not implemented"
must be backed by a record in [parity/](parity/).

## States

| State | Meaning |
|---|---|
| **Not implemented** | Tela does not have it. |
| **Implemented / not compared** | Tela has it; nobody has compared it with the reference. |
| **Compared / differences known** | Compared with the reference by a recorded, reproducible procedure; the differences are listed. |
| **Parity validated** | Compared by a recorded procedure and no difference remains **within the stated scope** (the scope is part of the claim). |

Four aspects, assessed separately when useful:

| Aspect | Question |
|---|---|
| **Feature** | Does the capability exist (the operation, the option, the mode)? |
| **Behavioural** | Does it act the same: rules, edge cases, ordering, what happens on misuse? |
| **Quality** | Do the pixels / feel match: rendering, precision, smoothness? |
| **Performance** | Is it as responsive at comparable sizes: latency, throughput, memory? |

## Rules

1. **Black-box only.** The reference is observed as a user would: run it,
   operate its UI, look at and measure its output. Never read its binaries,
   memory, install files, presets, brushes, settings, databases or other
   user data, and never use earlier binary analysis
   ([CLEAN_ROOM.md](CLEAN_ROOM.md#black-box-comparison)).
2. **Reproducible.** Each comparison records the reference version and
   environment, the exact steps, the observation (with measurements where
   possible) and the Tela result, in `docs/parity/<subphase>-<topic>.md`.
3. **Input device stated.** No physical tablet exists on the development
   machine. Comparisons driven by mouse or synthetic pen say so; pen feel
   cannot be "Parity validated" without real hardware.
4. **Familiar, not identical UI.** Parity is about behaviour a
   professional artist relies on, not about copying the reference's
   interface, names or layout. Where Tela's architecture differs on
   purpose (e.g. paper colour as a document property instead of a layer),
   the closest equivalent behaviour is chosen and the difference recorded.
5. **Parity gate.** From phase 4 on, every subphase ends with a parity
   gate: what was implemented, what was compared, which differences
   remain, recorded here and in its `parity/` file. A subphase is not done
   until its gate is recorded. Semantics that the reference defines
   (clipping, masks, lock behaviours, blend results) are observed
   **before** they are implemented, not chosen by preference.

## Reference

| | |
|---|---|
| Application | CLIP STUDIO PAINT EX **5.1.5** (trial), Windows 11 Pro, pt-BR UI |
| Machine | Ryzen 7 5800X, Radeon RX 7700 XT, 2560×1440 at 100 % scale |
| Input | Mouse (SendInput) unless a record says otherwise |
| Method | UI driven from PowerShell (SetCursorPos/mouse_event/keybd_event), screen captures and pixel measurements of the screen |

## Matrix

Abbreviations in cells: **NI** not implemented · **INC** implemented /
not compared · **CDK** compared / differences known · **PV** parity
validated · — not applicable.

### Phases 1–3 (before parity gates existed)

Nothing built in phases 1–3 has been compared with the reference yet.

| Subsystem | Feature | Behavioural | Quality | Performance | Record |
|---|---|---|---|---|---|
| Canvas view (pan, zoom, rotate, display quality) | INC | INC | INC | INC | — |
| Pen input (pressure, tilt, rotation, eraser end) | INC | INC | INC | INC | — (no physical tablet) |
| Stroke smoothing (stabilization, interpolation) | INC | INC | INC | INC | — |
| Dab spacing and tapers | INC | INC | INC | INC | — |
| Brush tips, dynamics (effectors), textures, scatter | INC | INC | INC | INC | — |
| Airbrush | INC | INC | INC | INC | — |
| Eraser | INC | INC | INC | INC | — |
| Built-in brush set | INC (own designs) | — | INC | — | — |
| Brush settings UI | INC (functional) | INC | — | — | — |

### Phase 4 — layers and document

| Subsystem | Feature | Behavioural | Quality | Performance | Record |
|---|---|---|---|---|---|
| 4A Layer stack: create, select, remove, reorder | CDK (no drag reorder, no delete confirmation, no rename field) | CDK (no paper layer: removing the bottom layer activates the one above) | — | INC | [4A](parity/4A-layers.md) |
| 4A Layer properties: visibility, opacity, lock | CDK (whole-percent opacity, lock rules equal; no rename field) | CDK (renaming a locked layer allowed in both, compared in 4B; visibility counts as a modification in both, 4F F3) | PV — scope: layer opacity 60 % in all 17 modes over the paper, within 1–2/255 (measured in 4C, `blend_parity`) | — | [4A](parity/4A-layers.md) |
| 4A Layer operations during a stroke | PV — scope: new-layer command during a mouse stroke | PV — same scope: command ignored, stroke stays on its layer | — | — | [4A](parity/4A-layers.md) |
| 4B Undo/redo: strokes, clear, layer create/delete/reorder, lock, opacity | CDK (no History panel; memory budget in addition to steps) | PV — scope: observations U1–U10, U13–U18 and U20 with the mouse (steps, redo invalidation, active-layer rules, opacity merging, visibility outside history, undo ignored mid-stroke, shortcuts); the 200-step limit (U19) was not exercised in either app | PV — scope: pixel counts after undo/redo equal before/after in both apps (Tela also bit-exact by test) | INC (measured in Tela only) | [4B](parity/4B-undo-redo.md) |
| 4B Undo/redo: rename, blend mode | CDK (no rename field; the blend-mode selector exists since 4C) | INC (both are one step by test; the reference observed them, U11–U12; not compared in Mirae's running app) | — | — | [4B](parity/4B-undo-redo.md) |
| 4C Blend modes (16 + Normal: multiply … luminosity, add) | CDK (11 reference modes not implemented; no merge down) | PV — scope: all 17 modes over the reference's paper, 8-bit sRGB, 100 % and 60 % opacity, opaque/translucent/transparent bottoms and tops (3 400 combinations, `blend_parity.rs`); CDK over a translucent backdrop without paper (D-4C-1, unreachable in Tela today) | PV — same scope: within 1/255 (2/255 Color, Luminosity) | INC (Tela measured only) | [4C](parity/4C-blend.md) |
| 4C Compositing space | — | PV — sRGB-encoded blending, paper as a backdrop (measured) | PV — same scope | — | [4C](parity/4C-blend.md) |
| 4D Layer masks: create, delete, enable, edit target, history, refusals | CDK (no apply-to-layer, mask from selection, show-area or unlink; D-4D-2) | PV — scope: M1, M6–M12 with the mouse (new mask reveals all and becomes the target, one per layer, enable/create/delete/strokes are steps, target switch is not, selection targets pixels, refusals on hidden/locked layers and disabled masks); the delete button's apply question differs (D-4D-3) | PV — scope: mask composition (K4–K6: 9 mask scenarios × 64 swatches, opacity 60 %, Multiply, Screen, disabled), with and without paper, within 2/255 | INC (Tela measured only) | [4D](parity/4D-masks-clipping-alpha-lock.md) |
| 4D Mask painting | PV — scope: brush strokes on a mask | PV — scope: mouse strokes in four colours (M2–M4: the stroke's alpha reveals, colour ignored); pressure not compared (D-4D-1) | INC (one edge measurement in the reference; no systematic comparison) | INC | [4D](parity/4D-masks-clipping-alpha-lock.md) |
| 4D Mask erase | PV — scope: eraser on a mask | PV — scope: mouse (M3: hides; refused on hidden layers and disabled masks) | INC | INC | [4D](parity/4D-masks-clipping-alpha-lock.md) |
| 4D Alpha lock (lock transparent pixels) | PV — scope: toggle, history step, refusal on locked layers | PV — scope: A1–A6 with the mouse (paint keeps alpha, nothing on empty pixels, eraser still erases, step, refusals); toggling during a stroke not exercisable (A7) | PV — scope: source-atop at full and partial coverage; the reference's edge pixels reproduced within 1/255 | INC | [4D](parity/4D-masks-clipping-alpha-lock.md) |
| 4D Clipping | PV — scope: clip toggle, groups by position, new layers inside a group | PV — scope: K7–K10 and C1–C5 (hidden base/clipped, order, group boundary, base-less clipped layer, flag kept on moves, base deleted, new-layer rule, undo/redo) | PV — scope: 34 probe scenarios (2 × 2 176 swatches) within 2/255, with and without paper | INC (Tela measured: `mask_clip_bench`) | [4D](parity/4D-masks-clipping-alpha-lock.md) |
| 4D Clipping with opacity | — | PV — scope: base 50 %, clipped 60 %, both (K2) | PV — same swatches within 2/255 (`clip_parity`) | — | [4D](parity/4D-masks-clipping-alpha-lock.md) |
| 4D Clipping with blend modes | — | PV — scope: clipped Multiply, Screen, Difference, Add; base Multiply, Screen; a layer below the base; the other 12 modes not probed inside a group | PV — same scope, within 2/255; a non-Normal base over a translucent backdrop without paper is D-4C-1 (D-4D-6) | — | [4D](parity/4D-masks-clipping-alpha-lock.md) |
| 4D Clipping with masks | — | PV — scope: base mask limits the group, clipped mask limits its layer, disabled base mask, mask + base opacity, mask + clipped Multiply (K3) | PV — same scope, within 2/255 (`clip_parity`) | — | [4D](parity/4D-masks-clipping-alpha-lock.md) |
| 4E 8-bit storage (default) | PV — scope: 8-bit RGB is the reference's only document depth (E1–E3) and Tela's default | PV — scope: 8-bit content survives a 16-bit round trip exactly in both (E7; Tela by test) | CDK (accumulation artefacts seen in both, compared qualitatively only; Tela rounds to nearest; D-4E-5) | INC (Tela measured: `depth_bench`) | [4E](parity/4E-16bit.md) |
| 4E 16-bit storage | CDK (the reference has no 16-bit documents; D-4E-1) | CDK (same rules as 8-bit: masks, clipping, alpha lock, history, preview == commit by test) | CDK (against the reference's 8-bit measurements: within 2/255; banding measured in Tela only) | INC (Tela measured: composition 0.99–1.07×, commit 1.2–1.5×, memory 2×) | [4E](parity/4E-16bit.md) |
| 4E 8 → 16 conversion | CDK (no conversion in the reference; E2) | CDK (exact; matches the reference keeping 8-bit content exact, E7) | — (bit-identical composite, by test) | INC | [4E](parity/4E-16bit.md) |
| 4E 16 → 8 conversion | CDK (no conversion in the reference; E2) | CDK (nearest level; the reference's 16-bit import drops the low byte, D-4E-2) | — | INC | [4E](parity/4E-16bit.md) |
| 4E Brush accumulation (repeated faint paint) | — | CDK (8-bit artefacts in both apps, qualitative; D-4E-5) | CDK (Tela measured at both depths: 16-bit within 0.75 level of exact, 8-bit off by 25–136; reference 8-bit only) | INC | [4E](parity/4E-16bit.md) |
| 4E Gradients | NI (no gradient tool) | NI | INC (stored ramps measured in Tela; the reference's gradient tool dithers, D-4E-6) | — | [4E](parity/4E-16bit.md) |
| 4E Masks in a 16-bit document | CDK (masks 8-bit at every depth, D13; no 16-bit documents in the reference) | CDK — scope: the 4D mask scenarios in 16-bit documents match the reference's 8-bit measurements within 2/255; the app's mask erase/reveal works | CDK (same scope) | INC | [4E](parity/4E-16bit.md) |
| 4E Clipping in a 16-bit document | CDK (no 16-bit documents in the reference) | CDK — scope: the 34 clipping/mask scenarios (2 × 2 176 swatches) within 2/255 of the reference's 8-bit measurements | CDK (same scope) | INC (`depth_bench`: clipping 1.02–1.04×) | [4E](parity/4E-16bit.md) |
| 4E Blend modes in a 16-bit document | CDK (no 16-bit documents in the reference) | CDK — scope: all 17 modes, 3 400 combinations, within 2/255 of the reference's 8-bit measurements (3 rows at 2/255 where 8-bit is at 1, D-4E-7) | CDK (same scope) | INC (`depth_bench`) | [4E](parity/4E-16bit.md) |
| 4F Save workflow (Save, Save As, shortcuts, native dialog, title marker) | CDK (no Save copy, Revert, Recent files; D-4F-8) | PV — scope: F1, F2, F3, F5, F6, F11 with the mouse (new document clean, selection not a modification, visibility is, native save dialog with the name prefilled, save in place, title without extension, Ctrl+S / Shift+Alt+S); CDK: undo back to the saved state is clean in Mirae (D-4F-2, deliberate) | — | INC (Mirae measured: `io_bench`) | [4F](parity/4F-file-workflow.md) |
| 4F Open workflow | CDK (one document per window, D-4F-1; PNG files are imported, not opened as documents, D-4F-8) | CDK — scope: native open dialog, last folder, content restored; a failed open keeps the document; corrupt files are named errors where the reference crashed once (D-4F-10) | — | INC (Mirae measured) | [4F](parity/4F-file-workflow.md) |
| 4F Unsaved changes protection | PV — scope: close and quit ask Save / Don't Save / Cancel (F7, F10) | CDK — same three answers and default, Cancel keeps the document, Don't Save discards, Save saves first; also asked on New/Open/drop, which the reference never needs (D-4F-1); wording D-4F-9 | — | — | [4F](parity/4F-file-workflow.md) |
| 4F Document persistence (`.mirae`) | INC — Mirae's own format, nothing to match (ADR 0019) | INC — bit-exact save/load, sparse layout, masks, clipping, alpha lock, ids at both depths (IO1–IO8, by test) | — | INC (`io_bench`: 4096² × 10 layers 8-bit saved in 1.9 s, loaded in 0.7 s; peak extra memory ≤ 3 MiB) | [4F](parity/4F-file-workflow.md) |
| 4F PNG export with the paper | PV — scope: 8-bit RGBA, opaque | PV — scope: the reference's own export of the same imported image, 1600 × 1200, both document depths | PV — same scope: every pixel equal (`png_parity.rs`); metadata differs (D-4F-6) | INC | [4F](parity/4F-file-workflow.md) |
| 4F Transparent PNG export | CDK (refused where a non-Normal layer blends with the stack, D-4F-5 / D-4C-1) | PV — scope: Normal stacks: opaque content and, in 16-bit documents, translucent content equal the reference's export pixel for pixel | CDK (8-bit documents move colour at low alpha, D-4F-4; colour under zero alpha, D-4F-6) | INC | [4F](parity/4F-file-workflow.md) |
| 4F PNG import | CDK (raster layer, no image material layer or transform handles, D-4F-7) | PV — scope: a new layer above the active one, named after the file, centred and rounded down (F15); one undo step | PV — scope: RGB8 at both depths and RGBA8 in 16-bit documents equal the reference; CDK RGBA8 in 8-bit documents (D-4F-4) | INC | [4F](parity/4F-file-workflow.md) |
| 4F 16-bit PNG behaviour | CDK (16-bit export, which the reference lacks, E3) | CDK — deliberate: 16 bits kept or rounded, the reference truncates (D-4F-3; measured: one level in 22 of 80 swatches) | CDK (same) | — | [4F](parity/4F-file-workflow.md) |

### Later phases (not started)

Vector layers, selections, transforms, fill, gradients, rulers, symmetry,
perspective, colour mixing / wet media, text, layer groups, reference and
correction layers, colour management, animation: **NI**.

## Phase 4 parity summary

Reviewed at the end of phase 4 (4G, [parity/4G-integration.md](parity/4G-integration.md)).
Every state above is backed by its subphase record; nothing was promoted
because internal tests pass. "PV" always means: compared with the
reference by a recorded procedure, within the scope written in the cell.

### Parity validated (within the stated scopes)

- **Layer commands during a stroke** are ignored and the stroke stays on
  its layer (4A, mouse).
- **Undo/redo semantics** — what is a step, redo invalidation, the active
  layer after undo, opacity merging, visibility outside the history, undo
  ignored mid-stroke, shortcuts (4B U1–U10, U13–U18, U20, mouse).
- **Blend modes** — all 17 over the reference's paper, at 100 % and 60 %
  layer opacity, 3 400 combinations within 1/255 (2/255 Color,
  Luminosity); the sRGB-encoded compositing space and the paper as a
  backdrop (4C).
- **Masks** — creation, target, enable, history, refusals (M1, M6–M12);
  mask composition with and without paper (K4–K6); painting reveals and
  erasing hides (M2–M4) (4D, mouse).
- **Alpha lock** (A1–A6) and **clipping** — groups, order, boundaries,
  base-less clipped layers, new layers inside a group, undo, and the 34
  probe scenarios (2 × 2 176 swatches) within 2/255, including opacity,
  four clipped modes, two base modes and masks (4D).
- **8-bit documents**: 8-bit is the only depth of the reference and
  Mirae's default; 8-bit content survives a 16-bit round trip (4E).
- **Unsaved-changes protection** on close and quit: Save / Don't save /
  Cancel (4F F7, F10).
- **PNG**: export with the paper equals the reference's own export pixel
  for pixel at both document depths; import placement (centred, rounded
  down); RGB8 imports, and RGBA8 imports in 16-bit documents, equal the
  reference's (4F).

### Compared, differences known

Layer UI (no paper layer by design, no drag reorder, rename field or
delete confirmation; D-4A-1…4) · no History panel (D-4B-2) · 11 reference
blend modes missing, no merge down (D-4C-2, D-4C-3) · mask apply, mask
from selection, show-area and unlink missing, delete-mask question
(D-4D-2, D-4D-3) · 16-bit documents and conversion, which the reference
lacks (D-4E-1) · 16-bit PNG import rounds or keeps 16 bits where the
reference truncates (D-4F-3, deliberate) · premultiplied 8-bit colour at
very low alpha after import (D-4F-4) · one document per window (D-4F-1)
· undoing to the saved state is clean (D-4F-2, deliberate) · transparent
export refused where a non-Normal layer blends with the stack (D-4F-5) ·
PNG metadata (D-4F-6) · imports are plain raster layers (D-4F-7) · no
Recent files, Revert, Save copy, PSD (D-4F-8).

### Not measured yet

Everything that needs a **physical tablet** (pen feel, pressure on masks:
D-4D-1) · the reference's blending over a **translucent backdrop without
paper** (data kept, no model: D-4C-1, D-4D-6) · **performance against the
reference** (every Performance cell is INC: Mirae measured only) ·
colour-dependent checks in Mirae's running app (it paints black only:
D-4D-5) · colour-managed documents (D-4C-4) · the 200-step limit, rename
and blend-mode undo in Mirae's UI, a few edge cases (D-4A-6, D-4B-3,
D-4B-4, D-4D-7) · drag-and-drop and closing a clean document in the
reference (D-4F-11) · phases 1–3 (brush engine, input, view): never
compared.

### Out of scope of phase 4

Vector layers, selections, transforms, fill, gradients, text, layer
folders, reference and correction layers, colour management, animation,
the reference's own file format and PSD.

### Regression protection

What keeps each validated claim from regressing silently (all run in
`cargo test` and therefore in `scripts/check.ps1`):

| Validated claim | Automated | Manual only |
|---|---|---|
| 4A commands during a stroke | — (the app ignores commands while `engine.is_stroking()`) | app check (parity 4A, O14) |
| 4B undo semantics | `document::document::history::tests` (one test per observation), `history_properties.rs`, end-to-end walks in `corpus/tests/scenarios.rs`, `random_sessions.rs` | Ctrl+Y / Ctrl+Shift+Z bindings |
| 4C blend parity | `document/tests/blend_parity.rs` (reference data), `blend_sheet.rs` (golden) | — |
| 4D masks, clipping, alpha lock | `document/tests/clip_parity.rs` (reference data, both depths), unit tests per observation, `mask_clip_properties.rs`, scenarios C–E | — |
| 4E 8-bit default and exact widening | `document::document::tests::documents_store_one_depth…`, `depth_properties.rs`, `blend_parity` / `clip_parity` at 16 bits, scenario F | — |
| 4F unsaved-changes prompt | — (app) | app check (parity 4F, part 2) |
| 4F PNG export/import | `io/tests/png_parity.rs` (the reference's exports), `io/tests/png.rs` | — |
| Integrated appearance | `corpus/tests/pipeline.rs` (golden of a combined session) | — |

## Gate log

Newest first. Each entry: subphase, date, what was implemented, compared,
and the remaining differences (details in the linked record).

| Subphase | Date | Summary |
|---|---|---|
| **4G gate** (end of phase 4) | 2026-10-16 | No new behaviour and no new comparison with the reference. Every measured dataset of 4C–4F re-run in the integrated stack at both depths (blend parity, clip parity, PNG parity, goldens, v1 fixtures): all pass with unchanged tolerances. New end-to-end tests (scenarios A–G, a whole-stack session with an integrated golden, 16 randomized sessions, stress) and a real 8- and 16-bit session in the running app (mouse and synthetic pen: layers, Multiply, clipping, alpha lock, mask, undo/redo, save, reopen, export, import). Review of every recorded difference: D-4A-7 resolved in 4C, D-4E-2 and D-4E-3 in 4F; the rest unchanged and assigned to debt or the phase-5 proposal. Summary of phase-4 parity [above](#phase-4-parity-summary) ([details](parity/4G-integration.md)). |
| **4F gate** | 2026-10-15 | Reference observed first: new/modified/saved states in the title and tabs, the save and open dialogs, the close and quit prompts (Save / Don't save / Cancel), the shortcuts, PNG export settings and files (RGBA8, sRGB ICC, paper composed over white), PNG import of test images of our own (a centred new layer; straight colour kept exactly; 16-bit truncated), and corrupt files (a corrupt PNG crashed it). Mirae adds its own `.mirae` v1 format (ZIP + JSON manifest + raw tiles; ADR 0019) and `tela-io` (ADR 0020): bit-exact save/load at both depths, atomic save, explicit limits, strict versioning, hostile-input tests, v1 fixtures as a contract; the desktop workflow (Ctrl+N/O/S, Save As, native dialogs, drag-and-drop, `*` marker from the history's revision, the unsaved-changes prompt before anything replaces or closes the document). Compared: PNG export with the paper equals the reference's own export pixel for pixel at both depths; opaque imports and translucent imports in 16-bit documents equal too; workflow checked in the running app. Remaining: one document per window, undo to the saved state is clean (deliberate), 16-bit import rounds instead of truncating (deliberate), premultiplied 8-bit colour at low alpha, transparent export refused for non-Normal stack blending (D-4C-1), PNG metadata, no Recent/Revert/Save copy ([details](parity/4F-file-workflow.md#differences)). |
| **4E gate** | 2026-10-15 | Reference observed first (new-document, properties, conversion and export UI; a 16-bit probe of our own in PSD and PNG; repeated 1 % paint, airbrush passes, gradient tool): it has **no 16-bit documents**, no depth conversion and 8-bit exports; a 16-bit PNG opens with its low byte dropped; 8-bit content survives exactly; its 8-bit accumulation shows plateaus and lost faint tails. Tela adds per-document 16-bit storage (premultiplied sRGB-encoded `u16`, masks 8-bit), exact 8 → 16 (bit-identical composite), nearest-level 16 → 8, conversion as one undoable step; 8-bit stays the default (ADR 0018). Measured: repeated faint strokes within 0.75 level of exact at 16 bits against 25–136 levels off at 8; composition 0.99–1.07×, commit 1.2–1.5×, memory 2×; the 4C/4D parity data within 2/255 in 16-bit documents; preview == commit at 16 bits (126 cases); verified in the running app with mouse and synthetic pen. Remaining: 16 → 8 rounding vs the reference's import truncation (4F), 8-bit masks, qualitative-only accumulation comparison, no dithering/gradient tool ([details](parity/4E-16bit.md#differences)). |
| **4D gate** | 2026-10-15 | Reference observed first: a PSD probe of our own (34 clipping/mask scenarios, 2 × 2 176 swatches, with and without paper) and UI operation (mask creation/painting/erase/enable/delete, alpha lock, clip toggle, history, refusals, new layers in a group). Decision by evidence: clipping groups composed in isolation with source-atop where the blend result replaces the colour (W3C atop off by 50/255, non-isolated by up to 215/255); base opacity, mode and mask apply to the group; masks scale alpha, painting reveals by stroke alpha whatever the colour, erasing hides; alpha lock is source-atop and does not stop the eraser. Tela matches every probed swatch within 2/255 (`clip_parity`), preview == commit in 126 combinations, verified in the running app with mouse and synthetic pen. 4C paths unchanged in output and speed. Remaining: apply/selection/show-area mask features, delete-mask prompt, pressure not comparable, colour-dependent checks only by test, D-4C-1 for non-Normal bases ([details](parity/4D-masks-clipping-alpha-lock.md#differences)). |
| **4C gate** | 2026-10-14 | Reference measured first with a PSD probe of our own (17 modes × 2 opacities × 100 colour pairs, with and without its paper layer). Decision by evidence: W3C formulas on sRGB-encoded values (≤ 1–2/255 off) rather than linear light (54–196/255 off), and the paper as backdrop for blend tiles, as in the reference. Tela's `Document` reproduces all 3 400 with-paper measurements within 1/255 (2/255 Color, Luminosity). Normal bit-identical. Remaining: translucent-backdrop behaviour without paper (D-4C-1), 11 extra reference modes, merge down deferred ([details](parity/4C-blend.md#differences)). |
| **4B gate** | 2026-10-14 | Reference observed first (20 cases); Tela implements linear, swap-based undo/redo inside `Document` with the reference's semantics: each stroke/clear/create/delete/reorder/lock/rename/blend/opacity is a step, selection and visibility are not, add/remove undos restore the active layer, consecutive opacity edits merge until interrupted, undo is ignored during a stroke, 200 steps (+1 GiB) by default, Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z. Compared in the running app for all observations exercisable in Tela's UI. Two points of the 4B brief changed to follow the reference (visibility not recorded; separate opacity drags merge); **both confirmed by the owner as intended behaviour**. Remaining differences: no History panel, rename/blend not yet in the UI, a few untested edge cases ([details](parity/4B-undo-redo.md#differences)). |
| **4A gate** | 2026-10-14 | Implemented: layer ids, create (above active, smallest free "Layer N"), select, remove (below else above; last and locked refused), reorder (Up/Down), visibility, whole-percent opacity, lock (pixels, opacity, blend, removal), hidden/locked not paintable, layer commands ignored during a stroke. Compared with the reference in the running app for all 14 recorded observations (mouse; one synthetic-pen check in Tela). Remaining differences: no paper layer (by design), no delete confirmation, no drag reorder, no rename field, a few behaviours not compared yet ([details](parity/4A-layers.md#differences)). |
| 4A (reference observation, before implementation) | 2026-10-14 | Reference behaviour for layer creation, selection, deletion, reorder, visibility, opacity, lock and mid-stroke commands observed and recorded in [parity/4A-layers.md](parity/4A-layers.md); Tela side pending. |
