# ADR 0005: One Euro filter as the default stabilizer

Status: accepted (phase 2). Recorded in phase 3.5.

# Context

Pen input carries sensor noise and hand tremor. Smoothing trades jitter for
lag; a fixed-window filter that removes jitter on slow lines adds tens of
milliseconds of lag on fast strokes. Stabilization is measured in **screen**
pixels and seconds, so it feels identical at any zoom and sample rate.

# Decision

Four stabilizers exist (none, moving average, exponential, One Euro —
Casiez et al. 2012) behind one 0–100 strength scale; **One Euro** is the
default at strength 20. The choice comes from a synthetic benchmark
(200 Hz, 0.4 px noise, 0.5 px 9 Hz tremor, 0.5 ms timing jitter;
`cargo run --release -p tela-brush --example stabilizer_bench`):

| algorithm @ 50 | slow jitter px | fast-line lag ms | fast-circle rms px |
|---|---:|---:|---:|
| moving average | 0.119 | 17.4 | 4.05 |
| exponential | 0.152 | 12.0 | 5.88 |
| One Euro | 0.095 | 5.9 | 2.04 |

(raw input jitter 0.408 px). Full table: `docs/phase2/stabilizer_bench.md`.

# Consequences

- No setting of the alternatives reaches both lower jitter and lower lag
  (test `stabilizer_comparison.rs::one_euro_is_not_dominated_by_the_alternatives`).
- Speed-adaptive cutoff: slow strokes are smooth, fast strokes stay
  responsive.
- At the end of a stroke the filter catches up to the last raw sample
  (`catch_up`), so strokes end where the pen lifted.

# Alternatives considered

- Moving average / exponential: simpler, kept as options; dominated on the
  jitter–lag trade-off.
- Pulled-string ("lazy brush") stabilizers: different feel, useful as an
  extra option later; not a default.
