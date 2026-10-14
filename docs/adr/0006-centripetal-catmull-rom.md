# ADR 0006: Centripetal Catmull–Rom as the default interpolation

Status: accepted (phase 2). Recorded in phase 3.5.

# Context

Samples are sparse during fast strokes (a fast circle may have ~19
samples). Connecting them with straight chords produces visible polygon
kinks; some curve fits overshoot or form cusps with uneven sample spacing.
Interpolation is separate from stabilization: stabilizers move samples,
interpolators connect them.

# Decision

Three interpolators exist (linear, centripetal Catmull–Rom — Yuksel et al.
2011 —, smoothed Bézier); **centripetal Catmull–Rom** is the default. The
path is flattened into chords of ~1.5 screen px (clamped to 0.1–1 canvas
px). From `cargo run --release -p tela-brush --example interpolation_bench`:

| interpolator | fast circle rms px | max turn ° | corner max px |
|---|---:|---:|---:|
| linear | 1.66 | 20.0 | 1.40 |
| Catmull–Rom (centripetal) | 0.30 | 0.7 | 1.93 |
| smoothed Bézier | 0.30 | 0.7 | 4.67 |

Full table: `docs/phase2/interpolation_bench.md`.

# Consequences

- Passes through every sample, cusp-free by construction with uneven
  spacing, tight on abrupt direction changes (tests in
  `interpolation_comparison.rs` and `brush::interpolate`).
- Runs one sample behind the input (~4 ms at 240 Hz); the end of the
  stroke uses a mirrored phantom neighbour, slightly flattening the last
  segment.

# Alternatives considered

- Linear: exact but kinked at low sample rates.
- Smoothed Bézier: same smoothness, rounds sharp corners more (4.67 px).
- Uniform Catmull–Rom: forms cusps/loops with uneven spacing.
