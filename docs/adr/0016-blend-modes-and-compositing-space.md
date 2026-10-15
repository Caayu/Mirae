# 0016 — Blend modes: W3C formulas, sRGB-encoded space, paper as backdrop

Phase 4C. Written after the parity measurements (docs/parity/4C-blend.md),
as the owner required: the compositing space was not to be frozen just
because the compositor already worked that way.

# Context

Tela composed only `Normal` layers, in premultiplied f32 values decoded
straight from the stored sRGB-encoded bytes (no linearization), and put
the paper colour under the result in the display shader. Blend modes make
the space and the backdrop observable. The reference application was
measured black-box: a PSD test document of our own (17 modes × 2 layer
opacities × 100 colour pairs, including black, white, grey, saturated,
translucent and transparent pixels) opened and exported through its
normal UI, once as imported and once with its paper layer added.

# Decision

1. **Formulas.** Every non-Normal mode uses the general formula of W3C
   *Compositing and Blending Level 1* with that specification's blend
   functions (separable §9: multiply, screen, overlay, darken, lighten,
   color-dodge, color-burn, hard-light, soft-light, difference,
   exclusion; non-separable §10: hue, saturation, color, luminosity).
   `Add` uses the additive blend function `B = min(1, Cb + Cs)` inside
   the same formula; the Porter-Duff `plus-lighter` operator did not match
   the reference with translucent layers and was rejected. Layer opacity
   scales the source alpha.
2. **Space: sRGB-encoded values**, as stored. Measured against the
   reference with its paper: every mode within 1/255 (2/255 for Color and
   Luminosity) on all 3 400 combinations; blending in linear light misses
   by 54–196/255 depending on the mode (`blend_fit`). This is now a
   decision, not an accident; color management and wide-gamut work must
   revisit it explicitly.
3. **The paper is the bottom backdrop for blending.** In the reference the
   paper is a layer, so blend modes see it (Screen over empty white paper
   is white; Difference inverts against it). A tile in which any visible
   non-Normal layer draws is composed over the opaque paper colour. Tiles
   with only Normal layers keep the transparent backdrop and today's
   bit-identical output, with the display adding the paper as before.
4. **Normal keeps its original code path** (`over`), bit-identical.
5. **Implementation shape.** The mode is matched once per layer and tile;
   each mode runs a monomorphized pixel loop. Fully transparent source
   pixels are skipped in every mode (identity). Composition stays
   allocation-free.

# Consequences

- Tela matches the reference's blend results over its paper within the
  measured tolerance; `blend_parity.rs` keeps it that way with the
  reference's own numbers.
- A tile containing a blend layer is opaque after composition; at the
  boundary with Normal-only tiles the paper is added at different points
  (CPU f32 vs display), which can differ by 1/255 in Normal content.
- Blending against a **translucent backdrop** (no paper underneath) does
  not follow the W3C formula in the reference (alpha behaves additively
  for some modes, colours differ by up to ~25/255). Tela cannot produce
  that situation today because its paper is always under the stack; it
  becomes relevant for exports without the paper (4F), layer groups and
  "merge down", and is recorded as difference D-4C-1 with the measured
  data kept in the repository.
- Non-separable modes cost about 3× Normal per layer; separable ones
  1.3–1.8× (`blend_bench`).

# Alternatives considered

- **Linear-light blending** (physically motivated, used by some
  compositing systems): rejected by measurement — far from the reference
  for every mode, including Normal at partial opacity.
- **Paper only in the display shader** (status quo): wrong for every mode
  whose result over white differs from its source (Screen, Lighten,
  Dodge, Add, Difference, Exclusion, Hue, Saturation, Color, Luminosity,
  Overlay, Soft Light, Color Burn).
- **Always composing over the paper**, also for Normal-only tiles: simpler
  but changes Normal's output (rounding), which had to stay bit-identical.
- **Matching the reference's translucent-backdrop behaviour now:** its
  rule is not a published one and could not be pinned down from the data
  without guessing; it is not reachable in Tela yet. Revisit with a
  dedicated probe when transparent export or groups need it.
