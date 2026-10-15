# Parity record 4D — layer masks, clipping, alpha lock

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440, 8-bit sRGB documents. Observed 2026-10-14/15, **before** any
4D code was written. Black-box only: the reference opened test documents
**we wrote** (public PSD format) and exported PNGs through its normal UI,
and its UI was operated with the mouse (SendInput) and keyboard
(keybd_event); results were read from exported PNGs, screen pixels at
100 % zoom (one screen pixel = one canvas pixel), the layer panel and the
History palette. Nothing was read from the reference's files, and no
setting was changed (brush size/opacity left at their defaults).

## Part 1 — composition (PSD probe, measured)

### Method

1. `cargo run --release -p tela-corpus --example clip_probe -- psd clip.psd`
   writes 34 scenarios, each an 8 × 8 grid of 12 px swatches. Rows vary
   the base alpha (255, 224, 160, 128, 96, 64, 32, 0; colour 200,160,60);
   columns vary the clipped colour (blue, white, black, grey, green,
   magenta at 50 % alpha, orange at 25 %, transparent) or, in mask
   scenarios, the mask value (255 … 0). Clipping, hidden flags, opacity,
   blend modes and layer masks (including a disabled one and two with a
   default colour outside the mask rectangle) use the fields the PSD
   specification defines. The stack of every scenario is in the probe's
   `SCENARIOS` table.
2. The reference opened it (Arquivo › Abrir). Its layer panel showed the
   clipping bars, masks (the disabled one crossed out), hidden layers,
   opacities and modes as written.
3. Exported with Arquivo › Exportar (camada única) › .png (defaults).
4. Added its paper layer (Camada › Nova camada › Papel, white) and
   exported again.
5. `clip_probe read` sampled every swatch:
   `crates/document/tests/reference/clip_without_paper.csv` and
   `clip_with_paper.csv` (2 176 rows each, spread 0 everywhere: no
   dithering, no colour transform).
6. `clip_probe fit <with> <without>` evaluates three candidate models on
   every cell (8-bit premultiplied inputs, as Tela stores them) and
   prints the largest premultiplied channel error per scenario.

### Candidate models

| Model | Definition |
|---|---|
| **Isolated, opaque-blend atop** | Base (× its mask) starts a group buffer; each visible clipped layer (× its mask, × its opacity) is applied **source-atop** with `Cs' = B(Cb, Cs)`: `co = αs·αg·B(Cg, Cs) + (1 − αs)·cg`, `αo = αg`. The group is then blended onto the stack with the **base's** blend mode and opacity. |
| Isolated, W3C atop | Same, but `Cs' = (1 − αg)·Cs + αg·B(Cg, Cs)` (W3C general formula with source-atop). |
| Non-isolated | Each layer blended onto the running stack; a clipped layer's alpha multiplied by its base's masked alpha. |

### Results (largest error, 1/255, with paper / without paper)

| Scenario | Isolated, opaque-blend | Isolated, W3C atop | Non-isolated |
|---|---:|---:|---:|
| clip_normal, clip_op60, base_op50, base_op50_clip_op60 | 1 / 1 | 1 / 1 | 30–128 |
| clip_multiply, clip_screen, clip_difference, clip_add | ≤ 2 / ≤ 2 | 49–50 | 49–100 |
| below_clip_normal / multiply / screen (opaque layer under the base) | ≤ 2 | 1–50 | 40 |
| below_base_multiply (base in Multiply) | 2 | 2 | 215 |
| below_base_screen_clip_multiply, below_base_op50_clip_multiply | 1 | 24–26 | 90–180 |
| two_clips, three_clips, clips_swapped | 1 | 1–18 | 65–90 |
| base_mask, clip_mask, base_mask_disabled, base_mask_op50 | ≤ 1 | ≤ 1 | 48–128 |
| base_hidden, clip_hidden, group_boundary, orphan_clip | ≤ 1 | ≤ 1 | 0–68 |
| mask_normal, mask_op60, mask_multiply_below, mask_screen_below, mask_disabled, mask_default_black/white | ≤ 2 | ≤ 2 | ≤ 2 |
| clip_multiply_masked_base_op50 | 1 | 21 | 58 |

The 2/255 cases are the same 8-bit Multiply rounding seen in 4C. Full
table: `clip_probe fit` output.

### Observations

| # | Observation |
|---|---|
| K1 | A clipping group is composed **in isolation**: the clipped layers are applied onto the base only (they never see layers below the base), **source-atop** (the group's alpha is the base's), and the blend function's result replaces the clipped colour as if the base were opaque (`Cs' = B(Cb, Cs)`). The W3C source-atop mix and a non-isolated alpha limit both miss (table). |
| K2 | The group then meets the stack through the **base's** blend mode and opacity: base opacity 50 % shows a clipped opaque blue at 50 % (`base_op50`); a Multiply/Screen base multiplies/screens the whole group (`below_base_*`). |
| K3 | The **base's mask** limits the clipped layers too (`base_mask`); a clipped layer's own mask limits only that layer (`clip_mask`). |
| K4 | A **mask** multiplies the layer's alpha by `value/255` before the layer's opacity and blend mode (`mask_*`, all modes). |
| K5 | A **disabled** mask is exactly no mask (`mask_disabled`, `base_mask_disabled`). |
| K6 | Outside the stored mask rectangle the mask's default colour applies (`mask_default_black/white`). |
| K7 | A **hidden base** hides the whole group (`base_hidden`); a hidden clipped layer is skipped and the group continues above it (`clip_hidden`). |
| K8 | Several clipped layers apply bottom to top; swapping two changes the result (`two_clips` vs `clips_swapped`). |
| K9 | An unclipped layer ends a group; clipped layers above it clip to it (`group_boundary`). |
| K10 | A clipped layer with **no layer below it** shows like an ordinary layer (`orphan_clip`, with and without paper). |
| K11 | With and without the paper the same model fits: inside a group only the base's alpha matters, so the unresolved translucent-backdrop rule of 4C (D-4C-1) never enters clipping. It still applies where a **non-Normal base** (or plain layer) blends onto a translucent backdrop without paper, which no scenario exercised and Tela cannot produce (its paper is always under the stack). |

## Part 2 — UI behaviour (operated, measured on screen)

Test document: `clip_probe ui` (800 × 600: an "alpha" layer with
200,160,60 bands at alpha 255 / 128 / 0 and a 0→255 ramp over the top
half; a "solid" opaque blue 40,90,220 layer over the bottom half), plus
the reference's white paper. Tools: Caneta › G-pen (10 px, 100 %),
Borracha › Duro (30 px), colours from the default colour set, measured as
the main-colour swatch: black 0,0,0, white 255,255,255, red 255,50,100,
tan 215,145,90.

### Alpha lock ("Bloquear pixels transparentes")

| # | Steps | Observation |
|---|---|---|
| A1 | Lock on, black pen across the bands; same stroke with lock off | Locked: opaque band → black; 50 % band → **black at 50 %** (127 over paper); empty band → **nothing**; ramp → 255 − α (191, 127, 63): alpha never changes, colour is replaced. Unlocked: black everywhere. |
| A2 | Vertical profiles across both strokes (anti-aliased edges) | Coverage `m` from the unlocked stroke over the opaque band (edge pixel 106/200 → m = 0.47; 5/200 → 0.975). Over the 50 % band the locked edge pixels are 180,170,143 and 130,129,128 — exactly **source-atop** (`out = paint·m·αd + dst·(1 − m)`: 180.5, 169.9, 143.4 and 130.0). |
| A3 | Lock on, hard eraser across the bands | **The eraser erases** (opaque band → paper): the lock does not apply to erasing. |
| A4 | History; undo/redo | Each toggle is a step ("Bloquear/Desbloquear pixels transparentes"); undo/redo switch it off/on. |
| A5 | Alpha-locked layer with a mask; erase on the mask | The mask edit works: the lock concerns pixels only. |
| A6 | Fully locked layer ("Bloquear camada") | Clicking clip and alpha lock does nothing and records nothing; "create mask" is disabled; "Excluir/Aplicar máscara" disabled; painting the mask is refused; **enable/disable mask still works** and is recorded. |
| A7 | Toggle the lock during a stroke | Not exercisable with one mouse (the button cannot be clicked while dragging); 4A O14 already shows layer commands are ignored during a stroke. |

### Layer masks

| # | Steps | Observation |
|---|---|---|
| M1 | Select "solid", "Criar máscara de camada" | The new mask is **white: reveals everything** (no visible change). The mask thumbnail becomes the edit target (framed). One step ("Criar máscara de camada"). The create button is then disabled: **one mask per layer**. |
| M2 | Pen strokes on the (white) mask in black, tan, red and white | No visible change: painting can only reveal. |
| M3 | Eraser on the mask | **Hides** the layer (paper shows). |
| M4 | Pen strokes in black, red, tan and white over the erased band | **All reveal identically**, with identical anti-aliased edges (154,177,239 at coverage ≈ 0.47 = 47 % reveal): the mask takes the **stroke's alpha**; hue and lightness of the colour are ignored. |
| M5 | Synthetic pen (Windows Ink injection) at pressure 0.2 and 1.0 on the mask, then on pixels | Identical 10 px strokes in both cases, also on pixels: the reference does not read this synthetic device's pressure. **Pressure on masks not observable here.** |
| M6 | "Habilitar máscara" off/on | Off shows the layer unmasked (thumbnail crossed out). Each toggle is a step ("Alternar habilitar máscara"); undo/redo restore it. |
| M7 | Undo/redo mask strokes | Restore and re-apply them exactly (screen pixels). |
| M8 | Click the pixel thumbnail, paint; click the mask thumbnail, paint | Strokes go to the framed target. **Switching target is not a step.** |
| M9 | Hide the layer, erase on its mask, show it | **Refused**: nothing changed, no step (like pixels, 4A O8). |
| M10 | Disable the mask, erase on it, enable it | **Refused**: nothing changed, no step. |
| M11 | Select another layer (row click), then back | The target is **pixels** after each selection; the mask target is not remembered. |
| M12 | Camada › Máscara de camada › Excluir máscara; undo; redo | One step ("Excluir máscara de camada"); the layer shows unmasked; undo brings the mask back with its content; redo removes it. Pixels untouched. |
| M13 | Delete button while the **mask** is the target | Deletes the mask, after asking whether to **apply** it to the layer first (Aplicar / Excluir / Cancelar). "Aplicar máscara à camada" also exists in the menu. |

### Clipping ("Cortar para camada abaixo")

| # | Steps | Observation |
|---|---|---|
| C1 | Toggle on the upper layer | Red bar on the row; the layer shows only where the layer below has alpha (and its mask reveals). One step ("Alternar recorte"); undo/redo toggle it. |
| C2 | Move the base above the clipped layer | The clipped layer **keeps its flag** and, now directly above the paper, shows like a normal layer. Groups are decided by position. |
| C3 | Delete the base; undo | Same as C2 (the clipped layer shows normally over the paper); undo restores the group. |
| C4 | New layer (Ctrl+Shift+N) with the base active, clipped layer above | The new layer is inserted above the base **already clipped**: the group stays intact. |
| C5 | New layer with the topmost clipped layer active | The new layer is **not** clipped. Rule observed: a new layer is clipped iff the layer directly above its position is clipped. |
| C6 | Locked layer | Clip toggle refused (A6). |

## Reference behaviour (summary)

1. Clipping group = base + the clipped layers directly above it; composed
   in isolation with opaque-blend source-atop (K1); the group meets the
   stack through the base's mode, opacity and mask (K2, K3); hidden base
   hides the group, hidden clipped layers are skipped (K7); order matters
   (K8); a clipped layer with nothing below is ordinary (K10).
2. Mask = alpha multiplier `value/255` applied to the layer before
   opacity and blend (K4); disabled ≡ absent (K5); new mask reveals all
   (M1); one per layer.
3. Mask painting: the stroke's alpha **reveals**, whatever its colour;
   the eraser hides (M2–M4).
4. Alpha lock: paint is source-atop (A1, A2); the eraser still erases
   (A3).
5. History: create mask, delete mask, mask enable toggle, mask strokes,
   clip toggle and alpha-lock toggle are steps; switching pixel/mask
   target is not (A4, M1, M6–M8, M12, C1).
6. Refused: strokes on a hidden layer's or a disabled mask (M9, M10);
   on a locked layer: mask creation/deletion/painting, clip and alpha
   lock toggles; mask enable is allowed there (A6).
7. Selecting a layer targets its pixels (M11); a new mask becomes the
   target (M1).
8. A new layer inserted below a clipped layer is clipped (C4, C5).

Not observed: pressure on masks (M5), commands during a stroke (A7),
undo/redo of create mask followed by the target state, masks on a
16-bit or colour-managed document.
