# 0022 — Blending over a translucent backdrop (D-4C-1 resolved)

Phase 5B. Amends [ADR 0016](0016-blend-modes-and-compositing-space.md),
decision 1, for backdrops that are neither opaque nor transparent; 0016
stays in force for everything composed over the paper. Written after the
reference was measured ([../parity/5B-transparent-compositing.md](../parity/5B-transparent-compositing.md)).

# Context

ADR 0016 chose the W3C *Compositing and Blending Level 1* general formula
on sRGB-encoded values, measured against the reference over its paper.
Its consequences already recorded that over a **translucent** backdrop
(no paper underneath) the reference departs from that formula — additive
alpha in some modes, colours up to ~25/255 off — and that the rule could
not be pinned down from one translucent swatch (D-4C-1, debt #20). Mirae
never composed over a translucent backdrop on screen (the paper is under
every blend tile), so the gap only blocked transparent export of blend
layers (refused, D-4F-5), merge down (5B) and layer folders (later).

Phase 5B measured it with a dedicated probe: 17 modes × 3 layer
opacities (100, 60, 25 %) × 36 backdrops (black, white, red, green, blue,
37/149/213, each at alpha 0, 1, 64, 128, 191, 255) × 25 tops (eight
colours at alpha 255, 160, 64, and a transparent control) = 45 900
combinations, opened in the reference without paper and exported with
transparency (no dithering, transparent outside the swatches). Six
candidate rules were scored (`backdrop_fit`), and a free least-squares
fit of `C = wS·Cs + wB·B(Cb, Cs) + wC·Cb` per mode and alpha pair
showed which structure the data has.

# Decision

1. **Normal** stays source-over, unchanged (bit-identical, invariant C1).
2. **Multiply** keeps the W3C general formula (within 2/255 everywhere;
   the alternative below misses it by 48/255).
3. **Every other mode** composes the blended colour over the backdrop
   before the W3C mix (the measured weights, to three decimals, in every
   one of the 15 modes and every alpha pair):

   ```text
   αo = αs + αb·(1 − αs)
   X  = [αs·B(Cb, Cs) + (1 − αs)·αb·Cb] / αo
   C  = [αs·(1 − αb)·Cs + αb·X] / αo
   ```

   Equivalently, W3C writes `C = lerp(Cs, lerp(Cb, B, αs), αb/αo)`; the
   reference uses `lerp(Cs, X, αb/αo)`, where `X` is `B` at `αs` composed
   over `Cb` at `αb` instead of over an opaque `Cb`.
4. **Screen, Color Dodge, Hard Light and Soft Light** have an additive
   alpha `min(1, αs + αb)` with the colour of rule 3 (premultiplied by the
   additive alpha). Every other mode keeps the union `αo`.
5. All rules equal the W3C formula over an opaque backdrop and show the
   source over a transparent one. The code takes the pre-5B expression
   exactly when `αb ≥ 1`, so every composite over the paper (every tile
   on screen that contains a blend layer, every export with the paper,
   `blend_parity`, `clip_parity`, the blend sheet and the session golden)
   is unchanged **bit for bit**.
6. Clipped layers keep `clip_atop` (ADR 0017: their backdrop is the
   group, and only the base's alpha matters). A clipping group whose base
   is non-Normal meets a translucent stack through `composite`, so it
   follows these rules (D-4D-6); that case was not probed separately.

# Consequences

- `crates/document/tests/backdrop_parity.rs` reproduces all 45 900
  measurements through `Document` within 2/255 premultiplied (16-bit
  documents everywhere; 8-bit documents too, except 22 Color Dodge / Burn
  rows at ≤ 5/255, where 8-bit premultiplied storage of a 64/255 top
  loses colour that the division amplifies — the root of D-5A-3). The
  340 translucent rows of the 4C data, previously only counted, are now
  asserted in `blend_parity.rs`.
- Invariant C3 changes: the result alpha is the union except for the four
  additive modes over a translucent backdrop. C3 is rewritten, not
  dropped; the property test asserts the new alpha.
- `Document::compose_tile_transparent_into` composes without the paper:
  transparent PNG export no longer has to refuse blend layers (D-4F-5)
  and merge down can compose two layers alone.
- The paper still takes part in blending on screen (ADR 0016, 3): a
  composite over transparency with the paper put under it afterwards is
  **not** the composite over the paper when a blend layer draws.
- Cost: one comparison per pixel on the opaque path; the translucent path
  is a few more multiplications (measured in `blend_bench`, 5B).
- At a backdrop alpha of 1/255 the reference's **Normal** mode drops the
  backdrop's colour and alpha contribution entirely (an integer-rounding
  effect: colour within 1/255 premultiplied, up to 14/255 straight at
  very low result alpha); Mirae keeps exact source-over (C1).

# Alternatives considered

- **W3C everywhere (0016 as is):** off by up to 47/255 (mean 3–6) over
  translucent backdrops in 15 modes; rejected by measurement.
- **W3C colour with additive alpha:** fixes the alpha of four modes, not
  the colour; rejected.
- **Blend as if the backdrop were opaque, then source-over (`Cs' = B`):**
  up to 254/255 off; rejected.
- **Isolated composition (the layer alone, then Normal):** what Normal
  does; up to 191/255 off for the others; rejected.
- **The blend function applied to premultiplied values** (`co = B(cb, cs)`):
  47–255/255 off; rejected. Linear light was already excluded in 4C, and
  the free fit leaves no residual another variant could explain (≤ 1.7/255
  per alpha group for rule 3).
- **Leaving D-4C-1 open:** not needed — one rule with two documented
  per-mode switches explains every measurement within the 8-bit export
  precision.
