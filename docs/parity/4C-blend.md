# Parity record 4C — blend modes and compositing space

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, 8-bit sRGB
documents. Observed 2026-10-14. Black-box only: the reference opened a
test document **we wrote** and exported PNGs through its normal UI; the
exports were measured. Nothing was read from the reference's files.

## Method

1. `cargo run --release -p tela-corpus --example blend_probe -- psd probe.psd`
   writes a layered document in the **public PSD format** (Adobe Photoshop
   File Formats Specification): a base layer of 10 bottom swatches (white,
   black, grey 128, saturated red, blue, ochre, teal at 50 % alpha,
   transparent, dark, light) and, for each of 17 modes, two top layers of
   10 top swatches (white, black, grey 128, green, magenta, orange, rust at
   38 % alpha, transparent, silver, charcoal) at 100 % and 60 % layer
   opacity, with the mode set by its PSD key.
2. The reference opened it (Arquivo › Abrir). Its layer panel showed every
   layer with the expected mode (normal → Normal, multiply → Multiplicar,
   screen → Tela, overlay → Sobrepor, darken → Escurecer, lighten →
   Clarear, color-dodge → Sobrexposição de cor, color-burn → Subexposição
   de cor, hard-light → Luz dura, soft-light → Luz suave, difference →
   Diferença, exclusion → Exclusão, hue → Matiz, saturation → Saturação,
   color → Cor, luminosity → Brilho, add (PSD *linear dodge*) →
   Adicionar) and opacity (100 / 60 %).
3. Exported with Arquivo › Exportar (camada única) › .png, default settings
   (transparency on, 100 % size, sRGB profile embedded).
4. Added the reference's paper layer (Camada › Nova camada › Papel, white;
   it goes to the bottom) and exported again.
5. `blend_probe read` sampled every swatch into CSV:
   `crates/document/tests/reference/blend_without_paper.csv` and
   `blend_with_paper.csv` (3 400 rows each).

Controls: every swatch whose top pixel is transparent reproduced its
bottom exactly, and no swatch had any internal spread, so import/export
add no colour transform and no dithering.

## Observations

| # | Observation |
|---|---|
| C1 | Over an **opaque** backdrop, the W3C *Compositing and Blending Level 1* formula with its blend functions, computed on **sRGB-encoded** values, reproduces all 17 modes within 1/255 (2/255 for Color and Luminosity) — 2 720 combinations including translucent tops and 60 % opacity. |
| C2 | The same formula in **linear light** misses by 8–196/255 (all modes, even Normal at 60 %). |
| C3 | Over a **fully transparent** backdrop every mode shows the source (W3C formula, exact). |
| C4 | Over a **translucent** backdrop (no paper) the reference departs from the W3C formula: for screen, color-dodge, hard-light and soft-light the result alpha is additive, `min(1, αs + αb)` (e.g. 0.376 + 0.5 → 224/255 measured, 176 by W3C), and in most modes colours differ by up to ~25/255. Not explained by linear light or by treating the backdrop as premultiplied-over-black/white; the rule is unpublished and was not pinned down. |
| C5 | The **paper layer takes part in blending**: Screen, Lighten, Color Dodge, Soft Light, Overlay, Color Burn, Hue, Saturation, Color and Add over empty white paper give white; Difference/Exclusion invert (green 30,200,60 → 225,55,195); Luminosity gives grey; Multiply/Darken/Normal show the source. |
| C6 | With the paper, every combination (translucent and transparent bottoms included, since the paper makes the backdrop opaque) matches the sRGB-encoded W3C formula within 1/255 (2/255 Color, Luminosity); linear light misses by 54–196/255 (`blend_fit`). |
| C7 | "Add" (Adicionar) matches the additive blend function `min(1, Cb + Cs)` inside the W3C formula; the `plus-lighter` Porter-Duff operator does not (up to 54/255 off at 60 % opacity). |
| C8 | The reference offers more modes than probed: Subexposição linear, Subtrair, Escape de brilho, Adicionar (Brilho), Luz vívida, Luz linear, Luz fixa, Mescla definida, Cor mais escura, Cor mais clara, Dividir. |

`blend_fit` output (with paper, error in 1/255 per channel):

| mode | sRGB-encoded max | mean | linear-light max | mean |
|---|---:|---:|---:|---:|
| normal | 1 | 0.013 | 68 | 8.940 |
| multiply | 2 | 0.240 | 69 | 8.083 |
| screen | 1 | 0.042 | 56 | 6.895 |
| overlay | 1 | 0.155 | 79 | 9.512 |
| darken | 1 | 0.010 | 68 | 6.490 |
| lighten | 1 | 0.003 | 56 | 4.517 |
| color-dodge | 1 | 0.077 | 166 | 14.390 |
| color-burn | 2 | 0.092 | 196 | 14.500 |
| hard-light | 1 | 0.162 | 105 | 17.103 |
| soft-light | 1 | 0.042 | 54 | 6.663 |
| difference | 1 | 0.020 | 128 | 37.878 |
| exclusion | 1 | 0.062 | 120 | 33.368 |
| hue | 1 | 0.067 | 61 | 6.935 |
| saturation | 1 | 0.058 | 65 | 6.790 |
| color | 2 | 0.080 | 62 | 8.173 |
| luminosity | 2 | 0.077 | 89 | 14.292 |
| add | 1 | 0.017 | 79 | 10.738 |

(`blend_fit` uses f32 throughout; Tela's stored 8-bit premultiplied path,
checked by `blend_parity.rs`, stays within 1/255 for multiply and
color-burn too.)

## Tela design derived from the observations (ADR 0016)

| Obs. | Tela 4C |
|---|---|
| C1, C2, C6 | W3C formula and blend functions on sRGB-encoded values; linear light rejected. |
| C5 | A tile that draws a non-Normal layer is composed over the opaque paper colour; Normal-only tiles keep the transparent backdrop (bit-identical output). |
| C3 | Over a transparent backdrop the formula shows the source (unit test). |
| C7 | `BlendMode::Add` = additive blend function in the W3C formula. |
| C4 | Not reproduced; unreachable in Tela today (the paper is always under the stack). Recorded as D-4C-1. |

## Tela results

| Check | Result |
|---|---|
| `blend_parity::every_mode_matches_the_reference_over_its_paper` | All 3 400 with-paper combinations, composed by `Document` from 8-bit premultiplied layers, within 1/255 (2/255 Color, Luminosity) of the reference. Removing the paper backdrop (planted bug) gives 450 mismatches of up to 255/255. |
| `blend_parity::formula_matches_the_reference_without_paper_except_over_translucent_backdrops` | The 2 720 rows with opaque or transparent bottoms match; the 340 translucent-bottom rows are counted, not asserted (D-4C-1). |
| Blend sheet | Tela's render of the probe layout (golden `crates/document/tests/golden/blend_sheet.png`) inspected side by side with the reference's export: indistinguishable. |
| Running app | Black stroke on a Screen layer over paper vanishes (strip ink 5985 → 0); undo/redo restores and re-applies; the selector is disabled on locked layers. |

## Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-4C-1 | Blending over a translucent backdrop without paper (C4) follows W3C in Tela, not the reference. | Behavioural / quality | Open — must be resolved before layer groups or merge down. 4F does not resolve it: a transparent PNG export where it would apply is refused (D-4F-5); exports with the paper are unaffected. Data: `blend_without_paper.csv`. |
| D-4C-2 | Modes the reference has and Tela does not (C8). | Feature | Open — each needs a public definition or a measured fit. |
| D-4C-3 | "Merge down" not implemented (deferred: merging onto a translucent lower layer is exactly D-4C-1). | Feature | Deferred |
| D-4C-4 | Only 8-bit sRGB documents compared; the reference's colour-managed documents were not probed. | Coverage | Open for colour management; 16-bit: the reference has no 16-bit documents (parity 4E, E1), and Tela's 16-bit documents match its 8-bit data within 2/255 (D-4E-7) |
| D-4C-5 | Composition speed not compared with the reference (Tela measured only, `blend_bench`). | Coverage | Open |
| D-4C-6 | Tela's blend-mode picker is a plain list; the reference's labels are Portuguese/its own naming. | UI | Accepted |
