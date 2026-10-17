# Reference measurements: transforms (phase 5D)

Canvas captures of the reference application (CLIP STUDIO PAINT EX 5.1.5)
transforming **our own** test document (`transform_probe psd`, see
`crates/corpus/examples/transform_probe.rs`): a 400 × 300 canvas whose
layer "ink" holds a 64 × 64 test pattern at (168, 118) over an opaque
white and an opaque black layer. Procedure: docs/parity/5D-transform.md.

Every PNG here is **decoded**, not a plain screenshot: two captures at
100 % zoom (over the white layer, and with the white layer hidden, over
black) give the ink layer's **premultiplied** RGBA, stored as the PNG's
RGBA bytes (`transform_probe decode`). They are not straight-alpha PNGs.

| File | Transform (reference "Redimensionar/girar" unless noted) | Interpolation |
|---|---|---|
| `source.png` | none (the pattern as the reference shows it) | — |
| `s200`, `s110`, `s075`, `s050`, `s045`, `s033`, `s030` | width 200, 110, 75, 50, 45, 33, 30 % with "keep aspect", reference point centre | bicubic (default) |
| `np` | width 150 %, height 60 % | bicubic |
| `r15`, `r45`, `r_m30` | rotation 15°, 45°, −30° | bicubic |
| `pivot30` | rotation 30° about a pivot dragged to (220, 170) | bicubic |
| `flipH` | Edit › Transform › Flip horizontal (width −100 %) | bicubic |
| `nearest_s200`, `nearest_s030`, `nearest_r15` | 200 %, 30 %, 15° | nearest neighbour ("Bordas duras") |
| `bilinear_*`, `average_*` | 200 %, 30 %, 15° | bilinear, "high accuracy (average colours)" (not implemented in Mirae; kept as data) |
| `alock150` | 150 % with the layer's alpha lock on | bicubic |
| `moveHard` | rectangle selection (150, 100)–(200, 160), content dragged +100 px | — |
| `moveFeather` | the same selection feathered 10 px, dragged +100 px | — |
| `mask0`, `pix150` | the ink layer with a mask hiding x 184..216 (y 100..200); then 150 % | bicubic |
| `outback` | moved to x = −12 (partly off the canvas), committed, moved back | — |
| `selection_rot15_fill.png` | **plain capture over white**: rectangle selection (150, 100)–(200, 160) rotated 15°, then filled black on a new layer (ink hidden): coverage = 255 − red | bicubic |
