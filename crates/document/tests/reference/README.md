# Reference measurements (black-box)

Measured outputs of the parity reference application (CLIP STUDIO PAINT EX
5.1.5, Windows) for the phase 4C blend-mode gate. Provenance and method:
[docs/parity/4C-blend.md](../../../../docs/parity/4C-blend.md).

- The inputs are Tela's own: a PSD test document written by
  `cargo run -p tela-corpus --example blend_probe -- psd probe.psd`
  (17 modes × 2 layer opacities × 10 bottom × 10 top swatches).
- The reference opened it through its normal UI and exported a PNG through
  its normal UI; `blend_probe read` turned the PNG into these CSV files.
- `blend_without_paper.csv`: the document as imported (no paper layer).
- `blend_with_paper.csv`: the same after adding the reference's white paper
  layer at the bottom.

Columns: mode (Tela name), layer opacity (0–255), bottom and top swatch
names and straight-alpha RGBA, measured straight-alpha RGBA, and `spread`
(largest channel difference inside the swatch; 0 everywhere: no
dithering). Nothing here comes from the reference application's files;
these are observed results of our own inputs.

## Phase 4D: masks and clipping

Provenance and method: [docs/parity/4D-masks-clipping-alpha-lock.md](../../../../docs/parity/4D-masks-clipping-alpha-lock.md).

- Input: `cargo run -p tela-corpus --example clip_probe -- psd clip.psd`
  (34 scenarios: clipping groups with base alpha, opacity, blend modes,
  masks, hidden layers, several clipped layers, a layer below, a
  clipped layer without a base; masks alone), opened and exported the
  same way.
- `clip_without_paper.csv`: as imported; `clip_with_paper.csv`: after
  adding the reference's white paper layer.

Columns: scenario, row (base alpha index), column (clipped colour or mask
value index), measured straight-alpha RGBA, spread (0 everywhere).

## Phase 5B: translucent backdrops and merge down

Provenance and method: [docs/parity/5B-transparent-compositing.md](../../../../docs/parity/5B-transparent-compositing.md).

- `backdrop_without_paper.csv`: `cargo run -p tela-corpus --example
  backdrop_probe -- psd backdrop.psd` (17 modes x 3 layer opacities x
  36 backdrops: six colours at alpha 0, 1, 64, 128, 191, 255 x 25 tops:
  eight colours at alpha 255, 160, 64, plus a transparent control),
  opened without paper and exported with transparency; the same columns
  as `blend_without_paper.csv` (45 900 rows, spread 0 everywhere).
- `merge_down.csv`: the `merge_probe` scenarios (one small PSD each),
  exported after the reference's "merge with layer below" (and, where
  merging changed the picture, before it): `scenario,row,col,r,g,b,a`,
  64 swatches per scenario.

## Phase 5C: selections

Provenance and method: [docs/parity/5C-selection.md](../../../../docs/parity/5C-selection.md).

`selection/*.png`: 400 × 300 captures of the reference's canvas at 100 %
zoom (bit-identical to its PNG export, checked once), our own document
from `cargo run -p tela-corpus --example selection_probe -- psd sel.psd`:
an opaque white layer under a layer filled with black through a
selection. Coverage of a pixel = `255 - red`. Coordinates in the names
are document pixels of the drag (`m5` = −5).

- `rect_*`, `ellipse_*_hard|aa`, `lasso_pentagram`: shapes (S7–S11);
  `rect_subpixel_400pct` the 400 % drag of S8, `rect_rotated_view_30` S9.
- `feather_<px>_rect_*`: the rectangle blurred by "Desfocar borda" (S19–S21).
- `op_b_*`, `op_add`, `op_subtract`, `op_intersect`: B alone and A combined
  with B; A is `feather_50_rect_100_80_300_220` (S14–S16).
- `paint_one_stroke`, `paint_two_strokes`, `mask_reveal_stroke`: strokes
  through selection A (S22, S23, S26).
