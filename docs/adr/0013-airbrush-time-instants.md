# ADR 0013: Airbrush deposition at fixed time instants

Status: accepted (phase 3). Recorded in phase 3.5.

# Context

An airbrush deposits paint while the pen is down, even at rest. Emitting
dabs "per frame" or "per sample" makes the result depend on the frame rate
and the device rate; extrapolating past the last sample creates beads when
input arrives late or in batches.

# Decision

- Dabs are emitted at instants `t₀ + k/rate`, fixed at stroke start; each
  takes its position by interpolating the processed path in time.
- The host calls `advance_time(now)` every frame. Deposition past the last
  sample happens only up to `now − lag − 30 ms`, and only if the input has
  not moved beyond the rest threshold (jitter threshold, ≥ 0.5 screen px)
  since the last motion. `lag` is the measured delivery latency (decaying
  maximum, ×0.98 per sample).
- While moving, time instants within one spacing step of distance-based
  dabs are skipped; distance dabs carry the dwell time:
  `flow' = 1 − (1 − flow)^min(step/speed · rate, 1)`.

# Consequences

- Identical dab lists at 30 and 144 fps (invariant S3).
- No beads with late/batched input; slow strokes deposit more paint than
  fast ones (S4).
- The rest position is the last processed path point, which lags the raw
  input by one sample (TECHNICAL_DEBT.md).

# Alternatives considered

- Per-frame emission: frame-rate dependent.
- Per-sample emission: device-rate dependent, nothing at rest.
- Extrapolating to `now`: beads under late delivery (observed and fixed in
  phase 3).
