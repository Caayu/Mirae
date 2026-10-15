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
