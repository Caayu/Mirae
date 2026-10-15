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
| 4A Layer properties: visibility, opacity, lock | CDK (whole-percent opacity, lock rules equal) | CDK (rename on a locked layer not compared) | INC (opacity rendering compared in 4C) | — | [4A](parity/4A-layers.md) |
| 4A Layer operations during a stroke | PV — scope: new-layer command during a mouse stroke | PV — same scope: command ignored, stroke stays on its layer | — | — | [4A](parity/4A-layers.md) |
| 4B Undo/redo: strokes, clear, layer create/delete/reorder, lock, opacity | CDK (no History panel; memory budget in addition to steps) | PV — scope: observations U1–U10, U13–U18 and U20 with the mouse (steps, redo invalidation, active-layer rules, opacity merging, visibility outside history, undo ignored mid-stroke, shortcuts); the 200-step limit (U19) was not exercised in either app | PV — scope: pixel counts after undo/redo equal before/after in both apps (Tela also bit-exact by test) | INC (measured in Tela only) | [4B](parity/4B-undo-redo.md) |
| 4B Undo/redo: rename, blend mode | CDK (no rename field; blend modes in 4C) | INC (API tested; not exercisable in the UI) | — | — | [4B](parity/4B-undo-redo.md) |
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
| 4E 16-bit colour depth | NI | NI | NI | NI | — |
| 4F Native file open/save | NI | NI | — | NI | — |
| 4F PNG import/export | NI | NI | NI | NI | — |
| 4F Desktop workflow (Ctrl+O/S, Save As, native dialog, drag-and-drop) | NI | NI | — | — | — |

### Later phases (not started)

Vector layers, selections, transforms, fill, gradients, rulers, symmetry,
perspective, colour mixing / wet media, text, layer groups, reference and
correction layers, colour management, animation: **NI**.

## Gate log

Newest first. Each entry: subphase, date, what was implemented, compared,
and the remaining differences (details in the linked record).

| Subphase | Date | Summary |
|---|---|---|
| **4D gate** | 2026-10-15 | Reference observed first: a PSD probe of our own (34 clipping/mask scenarios, 2 × 2 176 swatches, with and without paper) and UI operation (mask creation/painting/erase/enable/delete, alpha lock, clip toggle, history, refusals, new layers in a group). Decision by evidence: clipping groups composed in isolation with source-atop where the blend result replaces the colour (W3C atop off by 50/255, non-isolated by up to 215/255); base opacity, mode and mask apply to the group; masks scale alpha, painting reveals by stroke alpha whatever the colour, erasing hides; alpha lock is source-atop and does not stop the eraser. Tela matches every probed swatch within 2/255 (`clip_parity`), preview == commit in 126 combinations, verified in the running app with mouse and synthetic pen. 4C paths unchanged in output and speed. Remaining: apply/selection/show-area mask features, delete-mask prompt, pressure not comparable, colour-dependent checks only by test, D-4C-1 for non-Normal bases ([details](parity/4D-masks-clipping-alpha-lock.md#differences)). |
| **4C gate** | 2026-10-14 | Reference measured first with a PSD probe of our own (17 modes × 2 opacities × 100 colour pairs, with and without its paper layer). Decision by evidence: W3C formulas on sRGB-encoded values (≤ 1–2/255 off) rather than linear light (54–196/255 off), and the paper as backdrop for blend tiles, as in the reference. Tela's `Document` reproduces all 3 400 with-paper measurements within 1/255 (2/255 Color, Luminosity). Normal bit-identical. Remaining: translucent-backdrop behaviour without paper (D-4C-1), 11 extra reference modes, merge down deferred ([details](parity/4C-blend.md#differences)). |
| **4B gate** | 2026-10-14 | Reference observed first (20 cases); Tela implements linear, swap-based undo/redo inside `Document` with the reference's semantics: each stroke/clear/create/delete/reorder/lock/rename/blend/opacity is a step, selection and visibility are not, add/remove undos restore the active layer, consecutive opacity edits merge until interrupted, undo is ignored during a stroke, 200 steps (+1 GiB) by default, Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z. Compared in the running app for all observations exercisable in Tela's UI. Two points of the 4B brief changed to follow the reference (visibility not recorded; separate opacity drags merge); **both confirmed by the owner as intended behaviour**. Remaining differences: no History panel, rename/blend not yet in the UI, a few untested edge cases ([details](parity/4B-undo-redo.md#differences)). |
| **4A gate** | 2026-10-14 | Implemented: layer ids, create (above active, smallest free "Layer N"), select, remove (below else above; last and locked refused), reorder (Up/Down), visibility, whole-percent opacity, lock (pixels, opacity, blend, removal), hidden/locked not paintable, layer commands ignored during a stroke. Compared with the reference in the running app for all 14 recorded observations (mouse; one synthetic-pen check in Tela). Remaining differences: no paper layer (by design), no delete confirmation, no drag reorder, no rename field, a few behaviours not compared yet ([details](parity/4A-layers.md#differences)). |
| 4A (reference observation, before implementation) | 2026-10-14 | Reference behaviour for layer creation, selection, deletion, reorder, visibility, opacity, lock and mid-stroke commands observed and recorded in [parity/4A-layers.md](parity/4A-layers.md); Tela side pending. |
