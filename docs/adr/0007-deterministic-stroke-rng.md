# ADR 0007: Deterministic, counter-based stroke randomness

Status: accepted (phase 3). Recorded in phase 3.5.

# Context

Scatter, jitter and random/noise effectors need randomness. The provisional
tail is rebuilt every frame, so a stateful generator would give different
values each rebuild (flicker) and make preview differ from commit. Tests,
goldens, replays and future undo/redo need strokes to be reproducible.

# Decision

`StrokeRng` holds only a mixed seed. Every value is a pure function
`f(seed, stream, index)` (SplitMix64 mixing): `stream` identifies the
consumer (scatter axes, rotation, scale, noise, and one stream per effector
of each property: `EFFECTORS + slot·64 + i`), `index` is the dab number.
Airbrush dabs use indices from `1 << 40` so they never coincide with
movement dabs. The app increments the seed per stroke.

# Consequences

- Same preset + seed + samples ⇒ bit-identical dabs (invariants D1–D3).
- Tail rebuilds are stable; evaluation order does not matter.
- At most 64 effectors per property (`BrushProperty::MAX_EFFECTORS`,
  enforced by validation since phase 3.5) so streams stay disjoint.
- Property `slot` numbers are part of the format's behaviour: changing one
  changes every existing preset's random pattern.

# Alternatives considered

- A seeded stateful PRNG advanced per dab: order-dependent; breaks tail
  rebuilds.
- `rand` crate: unnecessary dependency for a 10-line mixer; and its stream
  layout could change across versions.
