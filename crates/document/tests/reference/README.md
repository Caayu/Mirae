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
