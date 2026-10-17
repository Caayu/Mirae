# Parity record 4G — integration and the phase-4 review

Phase 4G adds no behaviour and does not reopen the reference (none was
needed: nothing new is compared). It re-runs every measured comparison of
phase 4 inside the combined stack, runs a real drawing session in Mirae,
and reviews every recorded difference. Mirae at commit 7caaee7 and later,
Windows 11, 2560×1440.

## Regressions re-run

| Dataset or check | Where | Result |
|---|---|---|
| 4C blend parity (3 400 measured combinations) | `document/tests/blend_parity.rs`, 8- and 16-bit documents | Pass, unchanged tolerances |
| 4D mask and clipping parity (2 × 2 176 swatches) | `document/tests/clip_parity.rs`, both depths | Pass |
| 4C blend sheet, brush goldens | `blend_sheet.rs`, `corpus/tests/golden.rs` | Pass, no golden changed |
| 4B undo semantics | history unit and property tests; walked back and forth in every end-to-end scenario | Pass |
| 4E exact widening, nearest narrowing | `depth_properties.rs`, scenario F | Pass |
| 4F PNG fidelity against the reference's exports | `io/tests/png_parity.rs` | Pass (export with the paper equal at both depths) |
| v1 fixtures, corruption corpus | `io/tests/fixture.rs`, `corrupt.rs`, `properties.rs` | Pass; fixtures untouched (file hashes pinned) |

## Running app (mouse; synthetic Windows Ink pen)

8-bit session: New → mouse stroke and pen strokes with pressure → new
layer, soft brush, opacity 56 %, Multiply → third layer, clipped (the
marker shows outside the base only when unclipped: 115 → 255) → alpha
lock on the base: the eraser still erases (125 → 255), undo and redo
restore the screen exactly → reorder up and back → a mask on the clipped
layer, erased in a band (hidden there, 115 elsewhere) → undo/redo →
Ctrl+S (save dialog) → edit, undo back to the save point: clean →
Save As (Shift+Alt+S) → close (no prompt: clean) → reopen: same content
→ Export PNG (8-bit RGBA, `sRGB`) → Import PNG of that export: a layer
above, the screen unchanged.

16-bit session: New 16-bit on a modified document (prompt: Don't Save) →
pen strokes, faint soft strokes on a new layer, clipped, masked and
erased → Ctrl+S → close → reopen: 16-bit, the layer clipped, its mask the
edit target, history empty → Export PNG at 16 bits (bit depth 16).

Found in the session:

- **Ctrl+Shift+S does nothing on this machine**: another program holds it
  as a global hotkey (`RegisterHotKey` fails with error 1409), so Windows
  never delivers it. Mirae's binding is unchanged; Shift+Alt+S (the
  reference's) works. Not a Mirae defect; noted in docs/testing.md.
- The right panel's width follows its content (the status line, long
  layer rows such as "[editing mask]"), so controls move: a UI weakness
  (TECHNICAL_DEBT #17), not a functional one.
- During a pause of the session someone else used the open Mirae window
  (an export to the Desktop appears in the log, `Untitled.png`); the
  session continued after checking the state, and that file was left
  untouched.

## Review of the recorded differences

Status at the end of phase 4. "Debt" names the TECHNICAL_DEBT item;
"phase 5x" the proposal in [../phase5/PROPOSAL.md](../phase5/PROPOSAL.md).

| # | Still there? | Status now |
|---|---|---|
| D-4A-1 no paper layer | Yes | Accepted (ADR 0014) |
| D-4A-2 no delete confirmation | Yes | Open — debt #17 (decide with the owner; deletion is undoable) |
| D-4A-3 no drag reorder | Yes | Open — debt #17, phase 5B |
| D-4A-4 no rename field | Yes | Open — debt #17, phase 5B |
| D-4A-5 "Layer" vs "Camada" | Yes | Accepted (English UI) |
| D-4A-6 coverage gaps | Yes | Open (coverage) |
| D-4A-7 opacity rendering not compared | **No** | Resolved in 4C (60 % in every mode, `blend_parity`) |
| D-4B-1 byte budget | Yes | Accepted (ADR 0015) |
| D-4B-2 no History panel | Yes | Open — feature (phase 5B or later) |
| D-4B-3 coverage gaps | Yes | Open (coverage) |
| D-4B-4 rename/blend undo not exercisable | Partly | Blend-mode selector exists since 4C; neither undo compared in the app yet — open |
| D-4B-5 shape-assist step | Yes | Not planned |
| D-4B-6 undo speed not compared | Yes | Open (coverage) |
| D-4C-1 translucent backdrop | Yes | Open — debt #20, **prerequisite** for merge down and folders (phase 5B) |
| D-4C-2 11 modes missing | Yes | Open — feature |
| D-4C-3 no merge down | Yes | Deferred — needs D-4C-1 (phase 5B) |
| D-4C-4 colour-managed documents | Yes | Open — colour management, later |
| D-4C-5 composition speed not compared | Yes | Open (coverage); Mirae measured in 4G (`doc_bench`) |
| D-4C-6 mode picker | Yes | Accepted |
| D-4D-1 mask pressure | Yes | Open — needs a physical tablet (debt #1) |
| D-4D-2 apply mask, mask from selection … | Yes | Open — phase 5C (selections) and debt #23 |
| D-4D-3 delete-mask question | Yes | Open — with apply (debt #24) |
| D-4D-4 mask rectangles with outside default | Yes | Accepted — unreachable (Mirae imports no PSD) |
| D-4D-5 colour-dependent checks only by test | Yes (at 4G) | Open — colour picker (debt #22, phase 5A); **closed in 5A** |
| D-4D-6 non-Normal base without paper | Yes | Open — with D-4C-1 |
| D-4D-7 coverage gaps | Yes | Open (coverage) |
| D-4D-8 panel markers | Yes | Accepted |
| D-4E-1 16-bit beyond the reference | Yes | Accepted |
| D-4E-2 16 → 8 rounding vs import truncation | **No** | Decided in 4F (D-4F-3) |
| D-4E-3 no export | **No** | Resolved in 4F |
| D-4E-4 8-bit masks, unmeasured | Yes | Open — measure first (debt #25) |
| D-4E-5 accumulation compared qualitatively | Yes | Open (coverage; needs merge down) |
| D-4E-6 gradient dithering | Yes | Open — gradients (phase 5F) |
| D-4E-7 3 rows at 2/255 in 16-bit | Yes | Accepted |
| D-4E-8 16-bit speed not comparable | Yes | Accepted |
| D-4F-1 one document per window | Yes | Accepted (architecture) |
| D-4F-2 undo to the saved state is clean | Yes | Accepted (deliberate) |
| D-4F-3 16-bit PNG import rounds | Yes | Accepted (deliberate quality) |
| D-4F-4 premultiplied 8-bit at low alpha | Yes | Accepted (ADR 0001) |
| D-4F-5 transparent export with blend layers | Yes | Open — with D-4C-1 (phase 5B) |
| D-4F-6 PNG metadata | Yes | Open — debt #31 |
| D-4F-7 imports are plain raster layers | Yes | Open — transform (phase 5D), debt #32 |
| D-4F-8 Recent, Revert, Save copy, PSD | Yes | Open — debt #29, #32 |
| D-4F-9 prompt wording | Yes | Accepted |
| D-4F-10 corrupt input handled | Yes | Accepted |
| D-4F-11 coverage gaps | Yes | Open (coverage) |

No difference was resolved incidentally by 4G; D-4A-7, D-4E-2 and D-4E-3
had been resolved by later subphases and their records are updated.
