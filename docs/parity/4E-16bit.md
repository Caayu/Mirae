# Parity record 4E — 16-bit storage

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440. Observed 2026-10-15, **before** any 4E storage code was
written. Black-box only: the reference's UI was operated with the mouse
(SendInput) and keyboard (keybd_event); it opened test images **we
wrote** in public formats (PSD, PNG) and exported through its normal UI;
results were read from the exported files (TIFF header, PNG pixels) and
from screen pixels at 100 % zoom. Nothing was read from the reference's
files and no preference was changed (tool settings left at their
defaults). No representation is inferred: the records below say what
can be seen and measured.

## Part 1 — the reference (observed)

### Method

1. **Document creation and properties.** Arquivo › Novo (Ctrl+N) and
   Editar › Propriedades da tela de pintura: every field and drop-down
   was inspected.
2. **Conversion commands.** Every Editar entry was inspected for a
   colour-depth conversion.
3. **Export options.** Arquivo › Exportar (camada única) › .tif, .psd and
   .png: the settings dialog of each, including its "Cor de expressão"
   list and "Configurações avançadas de cores". A TIFF export of an RGB
   document was decoded (baseline TIFF 6.0 tags).
4. **16-bit input.** `cargo run --release -p tela-corpus --example
   depth_probe -- psd depth16.psd` writes a 16-bit layered PSD (public
   format) in the two layer layouts the specification allows (plain
   layer info, and the `Lr16` block); `… -- png16 ramps16.png` writes a
   16-bit RGB PNG (`docs/parity/4E/ramps16.png`) with four 1024 px
   ramps:
   - **Fine**: grey 128 → 132 (8-bit units) in 16-bit steps,
   - **Exact8**: grey `x/4` stored exactly as `v·257` (8-bit content),
   - **Gain16**: dark grey 0.04 → 0.08 in 16-bit steps,
   - **Gain8**: the same ramp rounded to 8-bit levels (control).
   The PNG was opened (Arquivo › Abrir) and exported to PNG unchanged
   (`reference_plain_export.png`); then Editar › Correção de tonalidade
   › Correção de nível was applied with the input handles dragged to
   about 8 and 18 (a ×25 gain that makes sub-8-bit detail visible) and
   exported again (`reference_levels_export.png`).
   `depth_probe read-png` / `read-levels <png> 8 18.25` compare each
   band with three predictions: the 16-bit values kept (**full**),
   rounded to 8 bits first (**round8**, `(v + 128) / 257`), or with the
   low byte dropped first (**truncate8**, `v >> 8`). The levels
   parameters 8 / 18.25 were fitted on the Exact8 control band.
5. **Repeated low-opacity paint.** New default document (1600 × 1200,
   white paper). Repeated 80 times: new layer (Ctrl+Shift+N), fill black
   (Alt+Del), layer opacity 1 %, merge with the layer below (Ctrl+E);
   the screen value over the paper was read after each merge
   (`reference_merge_accumulation.csv`).
6. **Airbrush.** Aerógrafo › Suave at its defaults (150 px), black, on a
   new layer: 20 identical horizontal mouse passes; the layer alone was
   exported to PNG (transparency on) and the alpha profile across the
   stroke read.
7. **Gradient.** Degradê › "Primeiro plano para plano de fundo" (black →
   white) dragged across a new layer; one exported row of 1600 px read.

### Observations

| # | Observation |
|---|---|
| E1 | **No 16-bit documents.** The new-document dialog offers only the basic expression colour Cor / Cinza / Monocromático, size, resolution, paper colour, colour profile; canvas properties the same. No bit-depth field anywhere. |
| E2 | **No depth conversion.** No command converts a document between depths (the closest, "Cor de expressão" of layers and exports, is colour/grey/monochrome). |
| E3 | **Exports are 8-bit.** TIFF, PSD and PNG export settings offer "Cor de expressão": Detecção automática / Duotone (limiar) / Duotone (tonalização) / Cinza / Cor RGB / Cor CMYK — no depth. A TIFF export of an RGB document has `BitsPerSample = 8,8,8`; PNG exports are RGBA 8-bit. |
| E4 | **16-bit PSD refused.** Both layouts of our 16-bit PSD give "Formato de arquivo não suportado" (8-bit PSDs from the same writer open, phases 4C/4D). |
| E5 | **16-bit PNG opens, as 8-bit content with the low byte dropped.** Exported unchanged, every band equals **truncate8** exactly (Fine 0/1024 pixels off, Gain16 0/1024, Exact8 0/1024); full and round8 miss 10 (Fine) and 439 (Gain16) pixels by 1. Fine steps at x = 127, 382, 637, 892, where `v >> 8` changes (rounding would step at 128, …). |
| E6 | **The detail below 8 bits is gone after opening.** After the ×25 levels gain the Gain16 band has 10 levels with jumps of 26 at the same x as E5's steps; full precision would give ~250 levels. Per model, pixels off / largest error: full 844/24, round8 820/26, truncate8 775/3 — truncate8 as close as the 8-bit controls (Exact8 36/3, Gain8 733/3; the reference's levels curve is not exactly the linear model, hence the control's error). |
| E7 | **8-bit content survives exactly.** The Exact8 band (`v·257`) exports as `v` (E5), i.e. 8 → 16 → reference → 8 is the identity for 8-bit values. |
| E8 | **Repeated 1 % paint loses about half its effect.** Each merge of a black 1 % layer darkened the result by exactly one level: 253, 252, …, 174 after 80 merges. Exact accumulation `255·0.99^k` gives 114 at k = 80 (the first merge is 2 levels, 253). |
| E9 | **Airbrush accumulation shows 8-bit artefacts.** After 20 passes the alpha profile across the stroke is 255 in the core, then a **27 px plateau stuck at 254**, then falls … 49, 40, 39, 26, 20 and **jumps from 20 to 0**: the faint tail of each pass never builds up, and the near-opaque part never reaches 255. |
| E10 | **The gradient tool dithers.** The 1600 px black → white row has all 256 levels, no step larger than 1, and 221 local reversals (… 52 53 52 53 …): the tool adds noise to its output. |

Not observable (no 16-bit document in the reference): 16-bit
composition, masks, clipping, alpha lock or blend modes in a 16-bit
document, 8 → 16 / 16 → 8 conversion of a document and its history, a
16-bit export. Layer opacity and merge in E8 are 8-bit behaviour only.

### What this means for Tela

- The reference stores **8 bits per channel** as far as anything can be
  observed (E1–E8). A 16-bit document in Tela is a capability **beyond**
  the reference: there is nothing to match in 16-bit, only the 8-bit
  behaviour, which Tela's 8-bit documents must keep.
- 8-bit content must survive 8 → 16 → 8 exactly (E7).
- The reference's only 16 → 8 reduction that can be seen (opening a
  16-bit image, E5) drops the low byte. That is a property of its
  import, not a depth conversion (it has none, E2).
- Accumulation artefacts of 8-bit storage are visible in the reference
  (E8, E9); measuring them in Tela at both depths is the banding test.
- Dithering (E10) belongs to the reference's gradient tool; Tela has no
  gradient tool yet, and 4E adds none (no dithering in this phase).

## Part 2 — Tela

### Design derived from the observations (ADR 0018)

| Obs. | Tela 4E |
|---|---|
| E1, E2 | Tela adds what the reference lacks: a per-document depth (`PixelDepth`, 8 or 16 bits per channel) and `Document::convert_depth`, one undoable step. 8-bit stays the default (D12), so new documents behave like the reference's. |
| E7 | 8 → 16 is exact (`v · 257`) and widened content composes **bit-identically**; 8 → 16 → 8 is the identity. |
| E5 | 16 → 8 rounds to the nearest level (`(v + 128) / 257`), keeping how a converted document looks; the reference's import truncates instead (D-4E-2). |
| E3 | The display stays 8-bit at every depth; storage precision is not display precision. No export yet (4F). |
| E8, E9 | Measured in Tela at both depths (banding below): 16-bit removes the accumulation artefacts the reference shows at 8 bits. |
| E10 | No dithering in this phase (Tela has no gradient tool). |
| — | Masks stay 8-bit at every depth (D13); blend formulas, paper backdrop, clipping and mask order are unchanged (ADR 0016, 0017). |

### Tela results

Tela at commit 1df2a26 (later commits are documentation), Windows 11,
2560×1440.

#### Measured tests

| Check | Result |
|---|---|
| Conversions (`tela_color`) | 8 → 16 → 8 identity for all 256 values; widened values decode to bit-identical f32; narrowing equals `unorm8` of the decode for all 65 536 values, error ≤ ½ level; 16-bit f32 round trip exact for all 65 536 values. |
| `blend_parity` in 16-bit documents | All 3 400 rows within 2/255 of the reference (3 rows at 2/255, over the translucent teal: Multiply ×2, Color Burn; the reference's 8-bit rounding is in its numbers). 8-bit unchanged (1/255, 2/255 Color/Luminosity). |
| `clip_parity` in 16-bit documents | All 2 × 2 176 swatches within 2/255, with and without paper (worst: `clip_multiply`, the same cell as at 8 bits). |
| Preview == commit, 16 bits | The 4D matrix (126 cases: pixels/mask × paint/erase × alpha lock × Normal/Multiply/Screen × mask absent/enabled/disabled × unclipped/clipped/base) within 1/255; 5 304 differing channels against 328 755 at 8 bits. Brush level: the commit is the preview quantized once (≤ ½ level of the storage) at both depths. |
| 8-bit content widened | Composes identically in every mode, with masks (enabled, disabled), clipping, opacity, paper backdrop (`widened_8_bit_content_composes_identically_in_every_mode`, and a property test over random stacks). |
| History | Stroke undo/redo bit-exact and swap-based at 16 bits (H1, H5, H9); 512 KiB per tile counted; the byte budget at 16 bits; a conversion undone and redone through later steps; random sequences with conversions (`history_matches_a_stack_of_snapshots`). |
| Model | Depth and wrong-depth edits in the layer-stack model test; refused edits change nothing (L6). |

#### Banding (`cargo run --release -p tela-corpus --example banding`)

8-bit display line over white against the exact `f64` result; the
number in parentheses is the content's own largest jump.

| scenario | bits | shown levels | stored levels | max jump (exact) | max error | mean error | widest plateau px | lost px |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 4000 px gradient 0 → 1 | 8 | 256 | 256 | 1 (0.1) | 0.50 | 0.250 | 16 | 0 |
| 4000 px gradient 0 → 1 | 16 | 256 | 4000 | 1 (0.1) | 0.50 | 0.250 | 16 | 0 |
| dark gradient under a ×10 gain | 8 | 11 | 11 | 11 (0.1) | 8.97 | 3.479 | 101 | 0 |
| dark gradient under a ×10 gain | 16 | 103 | 1024 | 1 (0.1) | 0.52 | 0.250 | 11 | 0 |
| 1 soft stroke, opacity 0.15 | 8 | 34 | 34 | 1 (0.6) | 0.50 | 0.187 | 30 | 0 |
| 1 soft stroke, opacity 0.15 | 16 | 34 | 101 | 1 (0.6) | 0.50 | 0.187 | 30 | 0 |
| 60 soft strokes, opacity 0.03 | 8 | 56 | 56 | 32 (4.8) | 25.03 | 2.841 | 20 | 24 |
| 60 soft strokes, opacity 0.03 | 16 | 78 | 101 | 5 (4.8) | 0.58 | 0.186 | 14 | 0 |
| 200 soft strokes, opacity 0.01 | 8 | 50 | 50 | 17 (5.1) | 80.88 | 13.797 | 20 | 46 |
| 200 soft strokes, opacity 0.01 | 16 | 77 | 100 | 6 (5.1) | 0.75 | 0.209 | 8 | 0 |
| 400 soft strokes, opacity 0.004 | 8 | 33 | 33 | 14 (7.6) | 136.14 | 57.071 | 25 | 48 |
| 400 soft strokes, opacity 0.004 | 16 | 54 | 85 | 8 (7.6) | 0.69 | 0.228 | 60 | 0 |

The ×10 gain is the probe's: in the reference (E6) the 16-bit ramp shows
10 levels with jumps of 26 after its levels gain, as Tela's 8-bit
storage does; Tela's 16-bit storage keeps the ramp.

#### Running app (mouse; synthetic Windows Ink pen where stated)

Soft Round 200 px, opacity 0.03, flow 0.35, 60 identical mouse strokes,
screen read at zoom ≈ 1.09 (`4E/tela_app_accumulation.txt`):

| | 8-bit document | 16-bit document |
|---|---|---|
| Centre after 1 / 10 / 20 / 40 / 60 strokes | 248 / 193 / 146 / 84 / 49 | 248 / 193 / 145 / 83 / 47 |
| Edge profile | 255 … 255, **234, 207**, then **195 for 8 px**, then 192, 184 … | 255 … 255, **254, 253, 252, 251, 250, 248, 247, 245, 242** … (smooth) |
| Faint tail starts | 46 px from the profile's top | 33 px (13 px of tail kept) |

| Check | Result |
|---|---|
| New 8-bit / New 16-bit | Title and panel show the depth; the canvas clears; painting works at both depths. |
| To 8-bit, undo, redo, To 16-bit | The 140-pixel profile across the 16-bit build-up did not change on screen at any step (max difference 0); the title follows the depth through undo/redo. |
| Alpha lock (16-bit) | An opaque stroke over the build-up adds nothing outside it (255 stays 255); without the lock the same stroke paints there (0); undo/redo restore. |
| Clipping (16-bit) | A clipped layer's stroke shows nothing outside the base (255); unclipping shows it (0); clipping again hides it. |
| Mask (16-bit) | Erasing on the base's mask hides the base and its clipped layer (47 → 255); brushing on the mask reveals them again (255 → 47). |
| Opacity (16-bit) | Base at 50 %: 47 → 151 (= 255 − 0.5 · 208), the clipped layer with it. |
| Undo | 12 undos walk back through opacity, mask strokes, mask, clip toggles, the new layer, lock toggles and the conversion to the earlier 8-bit state (centre 47, title 8-bit). |
| Synthetic pen (16-bit) | `inject_pen … pressure`: pressure-ramped line and S-curve with tapers, as in 8-bit documents. |

Colour-dependent checks remain test-only (the app paints black; D-4D-5). Update 5A: a coloured 51 % stroke at 16 bits shows the same pixels as at 8 bits in the running app, and layer picks at 16 bits are exact where 8-bit storage is a level off (parity record 5A).

### Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-4E-1 | Tela has 16-bit documents and depth conversion; the reference has neither (E1, E2). | Feature (beyond the reference) | Accepted — 8-bit stays the default, so new documents behave like the reference's |
| D-4E-2 | 16 → 8: Tela rounds to the nearest level; the reference's import of a 16-bit image drops the low byte (E5). | Behavioural | Decided in 4F: import rounds (8-bit documents) or keeps 16 bits; a deliberate quality difference, D-4F-3 ([4F](4F-file-workflow.md#differences)) |
| D-4E-3 | The reference exports 8-bit only (E3); Tela has no export yet. | Coverage | Resolved in 4F: 8-bit PNG export by default (equal to the reference's with the paper), 16-bit on request ([4F](4F-file-workflow.md)) |
| D-4E-4 | Masks are 8-bit at every depth (D13); faint repeated mask strokes were not measured. | Quality | Open — measure before deciding on 16-bit masks |
| D-4E-5 | 8-bit accumulation compared qualitatively only: the reference shows a plateau at 254 and a hard tail edge after 20 airbrush passes (E9), Tela's 8-bit documents a hard edge and a plateau after 60 soft strokes; the brushes differ, so the numbers are not comparable. The reference's repeated 1 % merges (E8) have no Tela equivalent (no merge down, D-4C-3). | Coverage | Open |
| D-4E-6 | The reference's gradient tool dithers (E10); Tela has no gradient tool and no dithering. | Feature | Open (gradient tool phase) |
| D-4E-7 | In 16-bit documents three blend rows land 2/255 from the reference's 8-bit measurements (Multiply, Color Burn over the translucent teal), where 8-bit documents are within 1/255. | Quality | Accepted — the reference has no 16-bit documents; the unrounded formula lands 2/255 away in the same modes too (`blend_fit`: Multiply and Color Burn max 2/255, parity record 4C) |
| D-4E-8 | Speed and memory of 16-bit work cannot be compared (no 16-bit in the reference); Tela measured only (`depth_bench`). | Coverage | Accepted |
