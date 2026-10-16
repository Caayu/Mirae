# Golden tests

Golden tests catch visual regressions in brush output that unit tests
cannot express. Decision record: [ADR 0012](adr/0012-golden-visual-corpus.md).

## What is compared

For every built-in preset, `tela_corpus::render_sheet` draws twelve
standard trajectories into a 1280×960 sheet (4×3 cells of 320 px) on white
paper and compares it with `crates/corpus/golden/<name>.png`:

| | col 1 | col 2 | col 3 | col 4 |
|---|---|---|---|---|
| row 1 | horizontal line | diagonal line | S curve | circle |
| row 2 | spiral | zigzag | pressure ramp (0.02 → 1) | velocity ramp |
| row 3 | crosshatch (12 strokes) | taper (forced 40/70 px) | slow stroke + 0.5 s rest | fast stroke (17 samples) |

Rendering is deterministic: simulated 200 Hz pen with pressure, fixed
seeds (`0x5EED_0000 + n`), fixed stroke settings (One Euro 20, centripetal
Catmull–Rom, jitter 0.35 px — independent of app defaults), simulated time
(airbrush), parallel stamping on. The eraser sheet starts from a filled
sheet.

## Naming and metadata

- File name = preset id without the `tela.` namespace:
  `tela.ink-pen` → `ink-pen.png` (`tela_corpus::golden_file_name`).
- `crates/corpus/golden/README.md` lists each file, its preset id and size.
- `golden_directory_matches_the_builtin_presets` fails if a PNG has no
  preset or a preset has no PNG.

## Tolerances

`tela_corpus::Thresholds::default()`:

| Threshold | Value | Why |
|---|---|---|
| Per-pixel tolerance | 2/255 per channel | Absorbs rounding differences of float math. |
| Differing pixels | ≤ 0.1% | A few edge pixels may round differently. |
| Mean channel error | ≤ 0.5/255 | Catches broad, subtle shifts. |
| Max channel error | ≤ 64/255 | Catches any localized gross change. |

Sensitivity is tested: a brush 10% larger fails
(`a_visible_change_fails_the_thresholds`). On mismatch the test writes the
actual image to the system temp directory
(`tela-golden-actual-<name>.png`) and prints the diff statistics.

## Updating references (only deliberately)

1. Make the change; look at the new sheets first:
   `cargo run --release -p tela-corpus -- <some temp dir>` writes all sheets
   there without touching the references.
2. If the change is intended, regenerate:

   ```sh
   TELA_UPDATE_GOLDEN=1 cargo test --release -p tela-corpus --test golden
   ```

   (PowerShell: `$env:TELA_UPDATE_GOLDEN = "1"; cargo test ...; Remove-Item Env:TELA_UPDATE_GOLDEN`.)
   Only the exact value `1` updates; each rewritten file is printed.
3. Run the tests again without the variable; commit the PNGs in the same
   commit as the change, and say in the message which presets changed and
   why.

`cargo run --release -p tela-corpus -- <dir>` requires a directory and
refuses to write into `crates/corpus/golden`, so the test-based command
above is the only way to replace references.

## Adding a preset

Add the JSON and its id (see `crates/brush/src/preset/builtin.rs`), run the
update command once, inspect the new PNG, add a row to
`crates/corpus/golden/README.md`.

## Blend sheet (phase 4C)

`crates/document/tests/blend_sheet.rs` renders every blend mode at 100 %
and 60 % opacity over the probe swatches and the white paper (as
displayed) and compares it with `crates/document/tests/golden/blend_sheet.png`
(± 1/255). Update only deliberately, after looking at it:
`TELA_UPDATE_GOLDEN=1 cargo test --release -p tela-document --test blend_sheet`.
The reference's agreement is checked separately, numerically, by
`blend_parity.rs` against measured data (`tests/reference/`).

## Integrated session (phase 4G)

`crates/corpus/tests/pipeline.rs` builds one 1100 × 700 drawing session
with real brushes through every phase-4 feature (layers, blend modes,
clipping, a mask, alpha lock, opacity) and compares its composite over
the paper, as the screen shows it, with
`crates/corpus/tests/golden/document_session.png`, with the default
thresholds above. It pins how the features combine; the brush goldens and
the blend sheet pin them apart. Update only deliberately, after looking at
it: `TELA_UPDATE_GOLDEN=1 cargo test --release -p tela-corpus --test pipeline`.

## Known limitation

The references were produced and verified on one machine (Windows, x86-64,
Rust 1.99). Different CPUs/compilers could in principle exceed the
tolerances through floating-point differences (e.g. FMA contraction); this
has not been observed or tested. See TECHNICAL_DEBT.md.
