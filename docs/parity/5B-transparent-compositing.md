# Parity record 5B — translucent-backdrop compositing, merge down, layer workflow

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440 at 100 %, 8-bit sRGB documents. Observed 2026-10-17, **before**
the matching Mirae code was written (the probes, `backdrop_probe` and
`merge_probe`, were the first 5B commit). Black box only: the reference
opened PSD test documents **we wrote** (public PSD format) and exported
PNGs through its normal UI (Arquivo › Exportar (camada única) › .png,
default settings: transparency on, 100 %); its UI was operated with the
mouse (SetCursorPos/mouse_event) and keyboard (keybd_event); results were
read from the exported PNGs, from its layer panel and menus on screen.
Nothing was read from its files; no setting was changed.

Mirae: commits `e49a02c`…, same machine; mouse and keyboard. No physical
tablet (none was needed: nothing here depends on pressure).

## Part 1 — blending over a translucent backdrop (D-4C-1)

### Method

1. `cargo run --release -p tela-corpus --example backdrop_probe -- psd backdrop.psd`
   writes a 1880 × 1784 document: a base layer whose rows are six colours
   (black, white, red 220/40/40, green 40/190/60, blue 40/70/220, sky
   37/149/213) at six alphas (**0, 1, 64, 128, 191, 255**: transparent,
   1/255, 25 %, 50 %, 75 %, opaque), and for each of the 17 modes and
   three layer opacities (**100 %, 60 %, 25 %**) a top layer whose columns
   are eight colours (black, white, grey 128, red 240/20/60, green
   90/220/40, blue 30/40/250, amber 213/149/37, purple 160/90/200) at three
   alphas (255, 160, 64) plus a transparent control column: 36 × 25 swatches
   of 8 px per region, 51 regions, **45 900 combinations**.
2. The reference opened it: no paper layer, the checkerboard showed
   through, every layer had the expected mode and opacity (Adicionar,
   Brilho, Cor, Saturação, Matiz, Exclusão, Diferença, Luz suave, Luz dura,
   … 25/60/100 %).
3. Exported to PNG with transparency (default settings).
4. `backdrop_probe read` sampled every swatch:
   `crates/document/tests/reference/backdrop_without_paper.csv`.

**No paper, confirmed by the pixels**: the reader asserts the gap
between regions is transparent (alpha 0), every transparent-on-transparent
swatch exported alpha 0, and every transparent-top swatch reproduced its
bottom exactly (alpha 1 included); spread 0 in every swatch (no
dithering, no colour transform).

### Hypotheses

Scored by `backdrop_fit` on the 30 600 rows with a translucent bottom
(1 ≤ α ≤ 191) and a visible top; straight colour error in 1/255 where the
measured alpha is not 0, max / mean, and the largest alpha error. Each
model is a public formula or a plain algebraic variation of one:

| Model | Definition |
|---|---|
| W3C | `C = [αs(1 − αb)Cs + αsαb·B + (1 − αs)αb·Cb] / αo`, `αo = αs + αb(1 − αs)` (ADR 0016) |
| W3C + additive α | W3C colour, `α = min(1, αs + αb)` |
| Opaque blend, then over | `Cs' = B(Cb, Cs)` composed over the backdrop |
| Isolated | the layer alone, then source-over (= Normal) |
| Blend of premultiplied values | `co = B(cb, cs)` on premultiplied channels |
| **Nested** | `X = [αs·B + (1 − αs)αb·Cb] / αo`, `C = [αs(1 − αb)Cs + αb·X] / αo` |
| Nested + additive α | nested colour, `α = min(1, αs + αb)` |

| mode | W3C | opaque blend | isolated | premultiplied | **nested** | nested α (union / additive) |
|---|---:|---:|---:|---:|---:|---:|
| normal | 14 / 0.54 | 14 / 0.54 | **14 / 0.54** | 235 / 42.2 | 48 / 7.06 | 2 / 51 |
| multiply | **2 / 0.36** | 254 / 31.0 | 191 / 12.9 | 254 / 98.1 | 48 / 3.84 | 1 / 51 |
| screen | 47 / 4.70 | 254 / 29.6 | 191 / 15.7 | 47 / 4.70 | **2 / 0.16** | 51 / **1** |
| overlay | 20 / 1.46 | 254 / 43.4 | 191 / 17.5 | 254 / 92.0 | **2 / 0.14** | **1** / 51 |
| darken | 47 / 3.44 | 254 / 29.9 | 191 / 14.2 | 254 / 82.6 | **2 / 0.12** | **1** / 51 |
| lighten | 47 / 4.05 | 254 / 26.5 | 191 / 13.2 | 109 / 16.4 | **2 / 0.12** | **1** / 51 |
| color-dodge | 40 / 3.08 | 254 / 39.9 | 191 / 18.5 | 254 / 54.4 | **2 / 0.14** | 51 / **1** |
| color-burn | 41 / 2.81 | 254 / 44.2 | 191 / 20.5 | 255 / 106.6 | **2 / 0.16** | **1** / 51 |
| hard-light | 47 / 5.52 | 127 / 20.6 | 95 / 9.9 | 255 / 92.0 | **2 / 0.17** | 51 / **1** |
| soft-light | 12 / 1.25 | 254 / 43.6 | 191 / 16.9 | 254 / 81.4 | **2 / 0.15** | 51 / **1** |
| difference | 47 / 6.54 | 254 / 38.1 | 191 / 19.1 | 172 / 24.3 | **2 / 0.16** | **1** / 51 |
| exclusion | 47 / 6.31 | 254 / 34.9 | 191 / 17.8 | 47 / 6.31 | **2 / 0.17** | **1** / 51 |
| hue | 40 / 3.70 | 254 / 37.7 | 191 / 16.8 | 254 / 65.6 | **2 / 0.17** | **1** / 51 |
| saturation | 27 / 1.69 | 254 / 47.3 | 191 / 19.9 | 254 / 64.1 | **2 / 0.16** | **1** / 51 |
| color | 40 / 3.73 | 254 / 37.4 | 191 / 16.8 | 254 / 63.7 | **2 / 0.18** | **1** / 51 |
| luminosity | 47 / 5.57 | 243 / 29.2 | 183 / 14.5 | 234 / 56.0 | **2 / 0.18** | **1** / 51 |
| add | 47 / 5.39 | 254 / 32.7 | 191 / 18.2 | 63 / 7.40 | **2 / 0.15** | **1** / 51 |

A free least-squares fit of `C = wS·Cs + wB·B(Cb, Cs) + wC·Cb` per mode
and (αs, αb) pair (`backdrop_fit … fit`, residual ≤ 1.7/255 in every
group) reproduces the nested weights to three decimals in all 15 modes,
e.g. Screen at αs 0.6, αb 0.502: fitted 0.372 / 0.471 / 0.157, nested
0.373 / 0.470 / 0.157, W3C 0.373 / 0.376 / 0.251; and the W3C weights in
Multiply (0.373 / 0.381 / 0.245 vs 0.373 / 0.376 / 0.251).

### Observations

| # | Observation |
|---|---|
| T1 | **Multiply** follows the W3C formula over translucent backdrops (2/255). |
| T2 | **Every other non-Normal mode** composes the blended colour over the backdrop first: `C = lerp(Cs, X, αb/αo)` with `X` = `B` at `αs` over `Cb` at `αb`, where W3C uses `lerp(Cb, B, αs)` (the backdrop treated as opaque). Within 2/255 (mean ≤ 0.18) at every backdrop alpha, top alpha and opacity. |
| T3 | **Screen, Color Dodge, Hard Light and Soft Light** have an additive alpha `min(1, αs + αb)` (exact within 1/255); their colour is still T2's, computed with the union alpha. The other 13 modes keep the union `αs + αb(1 − αs)`. |
| T4 | Over an **opaque** backdrop all rules are W3C (as 4C measured with the paper); over a **transparent** one every mode shows the source. |
| T5 | **Normal** is source-over. At a backdrop alpha of **1/255** the reference drops the backdrop's colour and alpha contribution entirely (white at 25 % over black at 1/255 exports pure white at alpha 64, where exact source-over gives alpha 64.75 and a trace of black): an integer-rounding effect of its Normal mode, up to 14/255 in straight colour at very low result alpha, ≤ 1/255 premultiplied. The blend modes do see a 1/255 backdrop (Screen: alpha 153 + 1 = 154). |
| T6 | 4C's single translucent swatch (teal at 50 %, 340 rows) fits the same rules (nested 1/255, W3C up to 25/255). |

### Decision (ADR 0022)

The rule of T1–T3 replaces the W3C formula for translucent backdrops;
opaque backdrops keep the exact pre-5B expression, so everything composed
over the paper is unchanged **bit for bit** (hash of the f32 composites of
10-layer stacks in all 17 modes over the paper equal to commit 81daa76).
Normal stays exact source-over (T5 not reproduced: C1).
`Document::compose_tile_transparent_into` composes without the paper.

### Mirae results

| Check | Result |
|---|---|
| `backdrop_parity.rs` (8-bit) | All 45 900 rows within 2/255 premultiplied, except 22 Color Dodge / Burn rows at ≤ 5/255: an 8-bit premultiplied top at alpha 64/255 stores its colour a few levels off and the division amplifies it (root of D-5A-3) |
| `backdrop_parity.rs` (16-bit) | All 45 900 rows within 2/255 |
| Planted alternative | W3C for Lighten misses by > 20/255 (`the_w3c_formula_does_not_explain_the_data`) |
| `blend_parity.rs` | The 3 400 with-paper rows unchanged; the 340 translucent rows of 4C now asserted (`every_mode_matches_the_reference_without_paper`) |
| `clip_parity.rs`, blend sheet, goldens, preview == commit | Unchanged (pass with unchanged tolerances) |
| Transparent PNG export (app) | A document with a Screen layer exports transparent (refused before 5B, D-4F-5); the export equals `compose_tile_transparent_into` (`png.rs`) |

## Part 2 — merge down

### Method

`cargo run --release -p tela-corpus --example merge_probe -- psd <dir>`
writes one 160 × 160 document per scenario: every layer covers an 8 × 8
grid of 16 px swatches, rows = eight bottom swatches (red, red at 50 %,
sky at 75 % and 25 %, green, black at 50 %, white at 25 %, transparent),
columns = eight top swatches (green, green at 50 %, blue at 63 %, amber
at 25 %, black, white at 50 %, grey at 78 %, transparent); masks hide by
column (255, 160, 96, 0, …). For each document: export (before), select
the layer to merge, **Camada › Fundir com camada abaixo (Ctrl+E)**, look
at the layer panel and the screen, export (after), undo (Ctrl+Z), panel,
export, redo (Ctrl+Y), panel. Exports read with `merge_probe read`; the
after (and, where merging changed the picture, before) results are
`crates/document/tests/reference/merge_down.csv`. Locks, alpha lock and
the paper were set through the reference's UI on copies of the
documents.

### Observations

| # | Scenario | Observation |
|---|---|---|
| MO1 | Normal over Normal | One layer remains: the **lower** one, with its name, `100 % Normal`, active. Export identical before and after (no paper). One undo restores both layers, the **upper one active** again; redo merges again. |
| MO2 | Upper opacity 60 %; upper Multiply; upper Screen 60 %; upper Hue | The result shows exactly what the pair showed (export identical). |
| MO3 | Lower opacity 60 % | Result `100 % Normal`: the lower opacity is **baked** into the pixels; picture identical. |
| MO4 | Lower Multiply; both non-Normal (Multiply 80 % under Screen 60 %) | The result **keeps the lower layer's mode** (`100 % Multiplicar`), opacity baked; with nothing under the pair the picture is identical. |
| MO5 | Something under the pair: an opaque amber layer under (a) a Multiply lower layer with a Normal top, (b) a Normal lower layer with a Screen 60 % top | The picture **changes** (up to 134/255 and 128/255): the pair is composed **alone**, over transparency, then stored; the merged layer (a) keeps Multiply, (b) is Normal. |
| MO6 | The paper added (Camada › Nova camada › Papel), Screen 60 % top merged into a translucent lower layer | The picture **changes** where the lower layer is translucent (Screen no longer sees the paper): the paper does not enter the merge. |
| MO7 | Merging the bottom raster layer **onto the paper** | Allowed: the result is a raster layer named "Papel", opaque: the paper is rasterized. Without a paper, the bottom layer's command is **disabled** (nothing below). |
| MO8 | Upper layer masked; upper mask disabled | An enabled upper mask is **applied** (picture identical); a disabled one is **dropped** (picture identical, unmasked content merged). |
| MO9 | Lower layer masked (enabled) | A question: "Há uma máscara de camada na camada abaixo. Você quer aplicar a máscara antes da fusão?" **Aplicar** (default) / **Segurar** (keep) / **Cancelar**. Apply: the mask is baked and gone, picture identical. Keep: the merged layer keeps the mask and the lower pixels are merged **unmasked**, so the mask now also hides the upper content (picture changes, up to 125/255). Cancel: nothing happens. |
| MO10 | Lower mask disabled | No question; merged unmasked, the disabled mask kept on the result. |
| MO11 | Upper clipped to the lower layer (its base); clipped Multiply 80 % | The result is the **group composite** (source-atop), unclipped, picture identical. |
| MO12 | Two clipped layers above a base, the upper merged into the lower | The result stays **clipped**; its pixels are the upper composed over the lower normally (not atop), so the group shows the same (≤ 1/255). |
| MO13 | The base of a clipping group merged into the layer below | Merged; the clipped layer above stays clipped and now clips to the merged layer: the picture changes (up to 213/255). |
| MO14 | Upper hidden; lower hidden; both hidden | The command is **disabled** (menu greyed out); Ctrl+E does nothing; the document is not modified. |
| MO15 | Upper locked; lower locked (both) | Nothing happens (refused, no question). |
| MO16 | Lower alpha-locked; upper alpha-locked | Merged normally (pixels equal to the plain merge: alpha lock does not restrict the merge); the result keeps the **lower** layer's alpha lock. |
| MO17 | History | Each merge is **one** undo step; undo restores the active layer that was active before (the upper one). |

### Mirae semantics (ADR 0023)

`Document::merge_down(id, LowerMask)`: the pair composed alone over
transparency (masks, opacities, modes, ADR 0022 rule; `clip_atop` for a
clipped upper layer onto its base, plain composite onto a clipped lower
layer) and stored in the lower layer, which keeps id, name, position,
blend mode, clip flag and alpha lock; opacity 100 % (baked); lower mask
applied or kept as asked (`LowerMask::Apply` / `Keep`; the app asks
Apply / Keep / Cancel like MO9); upper enabled mask applied, disabled
dropped; refused without a layer below and when either layer is locked or
hidden (`NoLayerBelow`, `Locked`, `Hidden`; the button is disabled then);
the merged layer active; one swap-based step. Mirae has no paper layer
(ADR 0014), so MO7's rasterization has no equivalent: the bottom layer
cannot be merged (D-5B-1).

### Mirae results

| Check | Result |
|---|---|
| `merge_parity.rs` (8 and 16 bits) | All 22 scenarios × 64 swatches (19 after-merge exports, 3 before-merge exports) within 2/255; planted alternatives (no `clip_atop`; `clip_atop` for every clipped layer; lower opacity not baked) fail by 84 / 48 / 97 swatches |
| `…::merging_keeps_the_picture_where_the_reference_does` | The 14 scenarios whose export did not change in the reference do not change in Mirae either (≤ 1/255, both depths) |
| Unit tests (`merge::tests`) | Identity of the survivor; refusals change nothing; one step, undo/redo bit-exact with the active layer; 16-bit precision kept; paper never merged in; history holds only changed tiles; kept lower mask |
| Property models | Random merges (succeeding and refused) among every other edit, undo and redo, against both models (stack and snapshots) |
| Save/load | A merged document saves, loads, composes and re-saves identically at both depths (`round_trip.rs`); no format change |

## Part 3 — rename

Reference: a layer with two layers, then a locked one; names read from the
panel; history by Ctrl+Z/Ctrl+Y.

| # | Steps | Observation |
|---|---|---|
| RN1 | Double-click the layer name | An inline text field over the name, the **whole name selected**. |
| RN2 | Type, Escape | Cancelled: the old name stays. |
| RN3 | Type, Enter; type, click elsewhere | Both **commit**. Delete everything, Enter: the **old name stays** (an empty name is refused). |
| RN4 | Rename "first" then "second" (Enter each), Ctrl+Z | **One** undo restores the original name: consecutive renames of the same layer are one step. With a lock on/off (two steps) in between, they are separate steps. |
| RN5 | Paste 300 characters, Enter | The name keeps **64** characters. |
| RN6 | Lock the layer, rename | **Allowed** (as 4B U11). |

Mirae: double-click opens the field with the name selected; Enter or a
click elsewhere commits, Escape cancels, an empty name changes nothing,
64 characters at most (`layer_panel::tests::renames_follow_the_reference_field`);
`Document::set_name` merges consecutive renames of one layer
(`consecutive_renames_of_one_layer_are_one_step`, H8); renaming a locked
layer is allowed. Checked in the running app (RN1–RN3, RN6).

## Part 4 — drag-and-drop reorder

Reference: a three-layer document (an opaque "under" layer, "bottom",
and "clip" clipped to it); rows dragged with the mouse.

| # | Steps | Observation |
|---|---|---|
| DR1 | Drag a row over another | A **red horizontal line** marks the insertion point; a ghost of the row follows the pointer. |
| DR2 | Drop on the upper / lower half of a row; above the top row; below the bottom row | Inserted above / below that row; to the top; to the bottom. |
| DR3 | — | The dragged layer becomes **active** (pressing its row selects it). |
| DR4 | Move the clipped layer's base away; move a layer between a base and its clipped layer | The clip flag **stays with its layer**; groups follow positions: the clipped layer clips to whatever is directly below (a layer with nothing unclipped below shows a grey, inactive clip bar and draws as an ordinary layer). |
| DR5 | Move the clipped layer back above a base | It clips to it again (red bar). |
| DR6 | Escape during a drag, then release | Cancelled: the order does not change. |
| DR7 | Lock a layer, drag it; Ctrl+Z | Locked layers **move**; one undo restores the order. |

Mirae: rows drag with a red insertion line (above or below the row under
the pointer), the dragged layer becomes active, Escape cancels; the drop
is `Document::move_layer` with a relative placement by id (LR1–LR3,
`reorder.rs`: five clipping cases at both depths compared with the stack
built directly in the final order, undo, locked layers move, a drop in
place records nothing). Checked in the running app (DR1–DR3).

## Part 5 — the layer panel

| Item | Mirae 5B |
|---|---|
| Create, select, delete, reorder (Up/Down, drag), visibility, opacity, lock, alpha lock, clipping, blend mode (all 17 implemented modes), mask (add, delete, enable, edit pixels/mask), merge down | All in the right panel; merge down also Ctrl+E (the reference's binding) |
| Active row | Highlighted; double-click renames; the line under the mask controls says which part strokes edit |
| Mask | Row marker `[m]`, `[M]` while strokes edit the mask, `[m off]` when disabled |
| Hidden / locked / clipped | Dimmed name / `Lock` toggle selected / indented `>` |
| Width | Fixed (270 pt); long names truncate (debt #17: controls no longer move) |
| Thumbnails | Not implemented (deferred, see D-5B-6) |

## Part 6 — the running app

Mirae, 8-bit, mouse (no tablet needed): five layers created, renamed by
double-click (Enter, Escape, empty name, focus loss), painted red, blue,
green, orange and black; "clipme" dragged above "base" (red insertion
line seen) and clipped; "sky" set to Screen 57 % (blue vanishes over the
paper, pink over red, as 4C C5); a mask added to "green" and erased in
one place; "top" alpha-locked and recoloured white (only its own pixels
changed); merge down of the clipped layer into its base (picture
unchanged), then of the masked layer into the Screen layer (mask
applied; the merged layer keeps Screen, so green now screens: the
picture changes as in the reference's MO5); undo × 2 (layers, mask,
57 %, Screen and the active layer come back), redo × 2; Save As, reopen
(same picture, `session8`), transparent PNG export with the Screen layer
(4000 × 3000 RGBA8). The question of MO9 appeared for a masked lower
layer (Escape cancelled, Enter applied).

16-bit: a new 16-bit document, three layers, red band and a blue stroke in
Screen 57 % on the layer above; merge down (over the paper the blue
appears where the pair was translucent, as MO6), undo, redo, Save As,
reopen (same pixels), 16-bit PNG export (4000 × 3000 RGBA16).

## Performance and memory (Mirae)

`layer_bench` (Ryzen 7 5800X, release):

| Merge down | 8-bit ms | 16-bit ms | undo / redo ms | step held | peak extra |
|---|---:|---:|---:|---:|---:|
| 1 tile, Normal | 0.80 | 0.87 | < 0.01 | 0.5 / 1.0 MiB | 2.3 / 2.5 MiB |
| 4096², Normal over Normal | 111 | 131 | 0.06 / 0.05 | 128 / 256 MiB | 66 / 130 MiB |
| 4096², Soft Light 60 % | 223 | 247 | 0.06 / 0.05 | 128 / 256 MiB | 66 / 130 MiB |
| 4096², lower opacity baked | 124 | 144 | 0.08 / 0.07 | 128 / 256 MiB | 66 / 130 MiB |
| 4096², clipped + both masks | 140 | 162 | 0.08 / 0.07 | 160 / 288 MiB | 66 / 130 MiB |

"Step held" is the history step: the previous content of the lower
layer's changed tiles plus the whole upper layer (moved, not copied).
"Peak extra" is the most memory the merge allocated beyond the document
before it: one layer's worth of tiles (the copies kept for undo) plus
~2 MiB of buffers — no third full copy. Reorder (100 layers) 0.3 µs,
rename 0.1 µs. Composition over the paper: `blend_bench` equal to the
pre-5B numbers within noise (A/B against 81daa76).

Not measured against the reference (it was not timed): every Performance
cell stays INC.

## Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-5B-1 | Merging the bottom layer onto the paper: the reference rasterizes its paper layer into a layer named "Papel" (MO7); Mirae's paper is a document property (ADR 0014), so the bottom layer has nothing to merge into (refused, button disabled). | Behaviour | Accepted (design); revisit if a paper layer is ever added |
| D-5B-2 | Normal over a backdrop at alpha 1/255: the reference drops the backdrop entirely (T5), Mirae keeps exact source-over (≤ 1/255 premultiplied). | Quality | Accepted (C1) |
| D-5B-3 | 8-bit documents: 22 of 45 900 translucent-backdrop combinations (Color Dodge / Burn over a 64/255 top) land up to 5/255 from the reference; 16-bit documents within 2/255. | Quality (storage precision) | Accepted (same root as D-5A-3) |
| D-5B-4 | A non-Normal **clipping-group base** over a translucent backdrop follows the measured rule through `composite`; that combination (D-4D-6) was not probed separately. | Coverage | Open |
| D-5B-5 | Merge down with other structures was not probed: a clipped upper layer onto an unclipped lower layer that is not its base, merges inside layer folders (Mirae has none), merging selected/visible layers, merge with a vector or other non-raster layer. | Coverage / feature | Open (folders later) |
| D-5B-6 | No layer thumbnails (the reference shows pixel and mask thumbnails). Deferred: needs per-layer downsampled previews kept up to date on every edit, and GPU textures per row; not simple enough for 5B's scope. | Feature | Open (debt #17) |
| D-5B-7 | No ghost row under the pointer while dragging (the reference shows one); Mirae shows the insertion line and a grabbing cursor. | UI | Accepted |
| D-5B-8 | The reference's other merge commands ("Mesclar camadas selecionadas", "Mesclar camadas visíveis", "Mesclar visíveis em nova camada", "Achatar imagem", "Transferir para camada inferior") are not implemented. | Feature | Open |
| D-5B-9 | Speed of merge, rename and reorder not compared with the reference. | Coverage | Open |
