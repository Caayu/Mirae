# ADR 0003: Separate stroke buffer; preview and commit share one path

Status: accepted (phase 1, extended in phases 2–3). Recorded in phase 3.5.

# Context

Stamping dabs directly into a layer makes overlapping dabs accumulate
without bound, makes opacity behave like flow, cannot show a provisional
end taper, and gives no clean boundary for undo. Previews computed
differently from the final result drift visually.

# Decision

A stroke accumulates in its own f32 alpha mask (`StrokeBuffer`, sparse
tiles). Each dab pulls covered pixels toward its opacity ceiling:
`if target > m { m += (target − m)·flow·coverage }`. Committed dabs live in
one surface; the provisional tail (the last `taper.end` px, re-emitted as if
the pen lifted now) lives in copy-on-write tiles seeded from it. The mask is
composited onto its target layer by one function, `paint_over_tile`, used
both for the live preview (through the document's `Overlay`) and for
`commit_into`. Paint is premultiplied `color·m over dst`; erase is
destination-out `dst·(1 − m)`.

# Consequences

- Opacity is a true ceiling and flow the build-up rate, independent of
  dab overlap (tests in `brush::stroke`).
- Preview equals the committed result for paint and erase (invariants P1–P3).
- The live end taper is possible without guessing the stroke's end.
- `touched()` is the exact undo footprint for phase 4.
- Memory: one f32 per touched pixel during a stroke (4 B/px), freed at
  commit.

# Alternatives considered

- Direct stamping into the layer: no ceiling semantics, no provisional
  tail, harder undo.
- Re-rasterizing the whole stroke each frame: simple, but cost grows with
  stroke length.
