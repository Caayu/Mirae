# Golden brush sheets

Reference images for `crates/corpus/tests/golden.rs`. Each file is one
built-in preset rendered by `tela_corpus::render_sheet` over the twelve
standard trajectories. Full policy: [docs/golden-tests.md](../../../docs/golden-tests.md).

| File | Preset id | Size |
|---|---|---|
| `ink-pen.png` | `tela.ink-pen` | 1280×960 |
| `pencil.png` | `tela.pencil` | 1280×960 |
| `soft-round.png` | `tela.soft-round` | 1280×960 |
| `marker.png` | `tela.marker` | 1280×960 |
| `airbrush.png` | `tela.airbrush` | 1280×960 |
| `chalk.png` | `tela.chalk` | 1280×960 |
| `calligraphy.png` | `tela.calligraphy` | 1280×960 |
| `eraser.png` | `tela.eraser` | 1280×960 |

File name = preset id without the `tela.` namespace
(`tela_corpus::golden_file_name`). A test fails if a file here has no
matching built-in preset or a preset has no file.

## Layout of a sheet

A 4×3 grid of 320 px cells, drawn by a simulated 200 Hz pen with pressure,
fixed seeds, fixed stroke settings and simulated time (deterministic):

| | col 1 | col 2 | col 3 | col 4 |
|---|---|---|---|---|
| row 1 | horizontal line | diagonal line | S curve | circle |
| row 2 | spiral | zigzag | pressure ramp (0.02 → 1) | velocity ramp (accelerating) |
| row 3 | crosshatch (12 strokes) | taper (forced 40/70 px) | slow stroke + 0.5 s rest | fast stroke (17 samples) |

The eraser sheet starts from a filled sheet so the removal is visible.

## Regenerating

Only deliberately, after a visual change you have inspected:

```sh
TELA_UPDATE_GOLDEN=1 cargo test --release -p tela-corpus --test golden
```

Only the exact value `1` updates. To render sheets elsewhere for
inspection: `cargo run --release -p tela-corpus -- <dir>` (it refuses to
write into this directory).

## Thresholds

A pixel differs when any channel moves by more than 2/255. A sheet passes
when at most 0.1% of its pixels differ, the mean channel error is at most
0.5/255 and no channel moves by more than 64/255. A brush 10% larger fails
(tested).
