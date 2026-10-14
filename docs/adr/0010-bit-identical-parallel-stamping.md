# ADR 0010: Bit-identical per-tile parallel stamping

Status: accepted (phase 3). Recorded in phase 3.5.

# Context

Large brushes (250–500 px) cost milliseconds per dab on one core. The stroke
update `m += (target − m)·flow·cov` is order-dependent per pixel, so naive
parallelism over dabs changes results, breaking determinism, goldens and
preview == commit.

# Decision

Batches of dabs are split **by tile**: list `(tile, dab)` pairs, sort, move
each affected tile out of the surface into a job, run jobs on rayon (each
job owns one tile and applies its dabs in stroke order), move tiles back.
Batches whose estimated work is below `PARALLEL_MIN_WORK` = 60 000 px stay
sequential. No locks, no atomics.

# Consequences

- Results are bit-identical to sequential stamping (test
  `parallel_stamping_is_bit_identical_to_sequential`; goldens render with
  parallelism on).
- Speed-up ~1.7× at 250 px and ~3× at 500 px on 16 threads; small brushes
  unchanged (~1.0×), bounded by tiles per batch (performance.md).
- The threshold is a tuning constant chosen by benchmark; change it only
  with new measurements.

# Alternatives considered

- Parallel over dabs with atomics/locks: non-deterministic and contended.
- Parallel over rows inside a dab: fine-grained overhead per dab, still
  needs ordering across dabs.
- GPU stamping: possible later behind `DabSink`; not needed yet.
