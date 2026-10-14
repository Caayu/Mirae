# ADR 0012: Golden visual corpus for brushes

Status: accepted (phase 3); references moved to `crates/corpus/golden` in
phase 3.5.

# Context

Brush output is visual; unit tests check specific properties but cannot
catch "this brush now looks different". Changes to spacing, dynamics,
textures or stamping can alter every preset subtly.

# Decision

- `tela-corpus` renders each built-in preset over twelve standard
  trajectories (lines, curves, spiral, zigzag, pressure and velocity ramps,
  crosshatch, forced taper, rest, sparse fast stroke) into a 1280×960 sheet,
  with fixed seeds, fixed stroke settings and simulated time.
- References are PNGs in `crates/corpus/golden/`, compared with
  tolerances: per-pixel tolerance 2/255; at most 0.1% differing pixels;
  mean error ≤ 0.5/255; max error ≤ 64/255. A 10% larger brush must fail
  (tested).
- References change only with `TELA_UPDATE_GOLDEN=1` (exact value); a test
  rejects orphaned or missing references.

# Consequences

- Visual regressions surface in `cargo test`.
- Tolerances absorb small floating-point differences (e.g. other CPUs or
  compilers) but this has only been verified on one machine
  (TECHNICAL_DEBT.md).
- Every deliberate visual change requires regenerating and reviewing the
  affected sheets in the same commit.

# Alternatives considered

- Exact (bit) comparison: brittle across platforms.
- Perceptual metrics (SSIM etc.): more tolerant of meaningful changes; the
  simple thresholds are easier to reason about.
- Screenshot tests of the app: depend on GPU, window and UI; the corpus
  tests the engine directly.
