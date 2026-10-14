# ADR 0009: Paper textures in canvas space, modulating the opacity ceiling

Status: accepted (phase 3). Recorded in phase 3.5.

# Context

Textured media (pencil, chalk) must show paper grain that stays fixed on
the canvas while the brush moves, looks the same whatever the dab spacing,
and does not wash out as dabs overlap.

# Decision

- A texture is a periodic height field `h ∈ [0, 1]` sampled at **canvas**
  coordinates (with scale and rotation), never in dab space.
- Height fields are generated in code from periodic gradient noise; every
  octave's lattice period divides the texture size, so tiling is seamless
  by construction. No image assets.
- Texture modulates the dab's opacity **ceiling** (not its coverage):
  multiply `c·(1 − s(1 − h))`, subtract `max(0, c − s(1 − h))`, height
  `c·smoothstep(τ ± 0.1, h)` with `τ = s(1 − coverage·c)` (light pressure
  catches peaks, heavy pressure fills valleys).
- Height maps are generated once per pattern and shared (`Arc`, process
  cache); sampling uses bit-mask wrapping.

# Consequences

- Grain is identical for different strokes over the same paper (test
  `texture_canvas_space.rs::grain_is_identical_for_different_strokes_over_the_same_paper`).
- Overlapping dabs converge to the textured ceiling instead of filling it
  in; spacing does not change the look.
- Textured stamping costs ~2× untextured (performance.md).

# Alternatives considered

- Texture in dab space: grain slides with the brush and depends on spacing.
- Modulating coverage: overlapping dabs fill the valleys and the grain
  disappears in dense strokes.
- Image-file textures: would require assets and an import pipeline; not
  needed for the built-in set.
