# ADR 0004: CPU composition of dirty tiles, GPU display texture with mips

Status: accepted (phase 1; display texture in phase 3). Recorded in phase 3.5.

# Context

Composition must be exact, testable and deterministic (preview == commit,
goldens). Display must be smooth at any zoom and rotation without seams.
Phase 1 drew per-tile textures; filtering could not cross tile borders, so
minified views showed seams and magnified views blocky, aliased texels.

# Decision

- Composition runs on the CPU, per dirty tile, into reusable buffers
  (`Document::compose_tile_into`, no allocation).
- The renderer keeps the whole document in **one** display texture with a
  full mip chain; each uploaded tile also uploads its mip levels, computed
  on the CPU with a 2×2 box filter (tiles are power-of-two aligned, so
  levels of different tiles never mix).
- The fragment shader samples trilinearly when minifying and uses
  anti-aliased nearest when magnifying (flat texels, one-pixel blended
  border), then composites premultiplied over the paper color.
- The renderer receives only tiles and a `ViewTransform`; it never sees the
  document.

# Consequences

- Seam-free at every zoom/rotation; crisp pixels when zoomed in (tests in
  `renderer/tests/display_quality.rs`).
- Composition is unit-testable without a GPU.
- Document size is limited by the GPU's maximum 2D texture size (typically
  8192 or 16384); larger documents need paging (TECHNICAL_DEBT.md).
- CPU composition cost per tile is ~0.11 ms for 2 layers + stroke
  (performance.md); fine for brush-sized updates, a candidate for compute
  shaders later without changing interfaces.

# Alternatives considered

- Per-tile textures with gutters: fixes seams for bilinear but not mips.
- GPU composition now: faster for many layers, but harder to test and to
  keep bit-exact with commits; deferred.
- Texture arrays/atlases: more bookkeeping, same mip problem.
