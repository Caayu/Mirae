# Brush corpus

One sheet per built-in preset, rendered by `cargo run --release -p tela-corpus`
and used as golden images by `cargo test -p tela-corpus --test golden`.

Each sheet is a 4x3 grid of 320 px cells, drawn by a simulated 200 Hz pen
with pressure, fixed seeds and simulated time (deterministic):

| | col 1 | col 2 | col 3 | col 4 |
|---|---|---|---|---|
| row 1 | horizontal line | diagonal line | S curve | circle |
| row 2 | spiral | zigzag | pressure ramp (0.02 → 1) | velocity ramp (accelerating) |
| row 3 | crosshatch (12 strokes) | taper (forced 40/70 px) | slow stroke + 0.5 s rest | fast stroke (17 samples) |

Eraser presets start from a filled sheet so the removal is visible.

## Golden thresholds

A pixel differs when any channel moves by more than 2/255. A sheet passes
when at most 0.1% of its pixels differ, the mean channel error is at most
0.5/255 and no channel moves by more than 64/255. A brush 10% larger fails
(tested). Update deliberately with `TELA_UPDATE_GOLDEN=1`.
