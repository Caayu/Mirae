# ADR 0001: Premultiplied alpha for all compositing

Status: accepted (phase 1). Recorded in phase 3.5.

# Context

Painting composites many semi-transparent layers, strokes and soft edges.
With straight (unassociated) alpha, `over` needs a division by the result
alpha, is not associative, and filtering (mipmaps, bilinear sampling) bleeds
the color of fully transparent pixels into edges, producing dark fringes.

# Decision

All compositing uses premultiplied alpha. `tela-color` keeps two distinct
types so they cannot be mixed: `Rgba` (straight, user-facing colors) and
`PremulRgba` (compositing). `over` is `src + dst·(1 − αs)`. Layers store
premultiplied sRGB-encoded RGBA8; the GPU texture holds the same values and
the shader composites premultiplied over the paper.

# Consequences

- `over` is associative and division-free (tested:
  `color::tests::over_is_associative`).
- Box-filtered mips and trilinear sampling are correct at alpha edges.
- Erasing is a simple scale: `dst·(1 − m)`.
- 8-bit premultiplied storage loses color precision at low alpha (a reason
  for 16-bit storage later, TECHNICAL_DEBT.md).
- Converting to straight alpha (`unpremultiply`) is needed only at
  user-facing boundaries (color pickers, export).

# Alternatives considered

- Straight alpha everywhere: simpler to read, but non-associative and
  fringe-prone under filtering.
- Straight storage with premultiplication at composition: pays the
  conversion on every composition and still needs care in mip generation.
