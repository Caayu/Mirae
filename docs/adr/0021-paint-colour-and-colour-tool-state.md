# 0021 — The paint colour and the colour tool state

Phase 5A. Measured against the reference first
([../parity/5A-color.md](../parity/5A-color.md)).

# Context

Until 5A the app painted black: `BrushEngine::color` was an `Rgba`
(straight, with alpha) that nothing changed. Real colour needs a value
type the user edits, a place for the main/sub colours, an eyedropper on
premultiplied storage at two depths, and rules for how the colour meets
opacity, flow, coverage, alpha lock, masks and the eraser. The reference
defines most of the behaviour (O1–O17): two slots of which one paints,
no alpha in the colour, colour changes outside the history, a colour
history fed by strokes and picks, an eyedropper on the displayed
composite or the layer, at the document pixel whatever the zoom.

# Decision

1. **`tela_color::Color`** is the colour a user paints with: opaque,
   straight, sRGB-encoded (the document's encoding, ADR 0016), `f32`
   channels clamped to `0..=1` with NaN mapped to 0 (always valid, COL1).
   It is **not** premultiplied (there is no alpha to premultiply by) and
   **not** quantized: 8-bit fields and hex are views of it, rounded to the
   nearest level; a colour picked in a 16-bit document or dragged in the
   HSV square keeps its precision until a commit quantizes once to the
   storage (ADR 0018). `Hsv` is computed on the encoded channels (the
   reference's HSV); `Hsv::follow` keeps hue and saturation where a colour
   has none so controls do not jump. `Rgba` and `PremulRgba` keep their
   roles (straight with alpha; compositing and storage).
2. **No alpha in the colour.** The reference's colour UI has none (O7);
   transparency is painted with the eraser. Hex is `#RRGGBB` only;
   `#RRGGBBAA` is refused. Colour management stays out: `Color` names its
   encoding in its documentation, so a later colour-managed type can sit
   beside it.
3. **The stroke owns its colour.** `BrushEngine::color` is a `Color`,
   copied into the `StrokeBuffer` when the stroke begins, exactly like the
   preset, the pressure curve and the app's `StrokeTarget` (COL2). The
   brush engine never reads UI or app state.
4. **Explicit formula.** The stroke mask `m` already combines coverage,
   flow (the per-dab approach) and opacity (the ceiling) — unchanged
   since phase 3. The colour only says what lands: the source is
   `(C·m, m)` premultiplied; paint is `src over dst`; with alpha lock it is
   source-atop `rgb = C·m·αd + rgb_d·(1 − m)`, `α = αd` (COL5); erase is
   `dst·(1 − m)` and mask strokes are `a + m(1 − a)` / `a(1 − m)`, both
   ignoring `C` (COL3, COL4). Layer opacity, mask and blend mode apply at
   composition. For every caller before 5A (alpha 1) this is the same
   value as `Rgba::premultiply`, so goldens did not change.
5. **Colour state is tool state of the session**, owned by the app
   (`app::colors::ColorState`: main, sub, active slot, recent colours,
   swatches), not by the document: it is not saved in `.mirae` v1 and
   nothing about it is a history step (reference O8). The UI shows it
   through `ColorModel` and returns `ColorAction`s (ADR 0011);
   `tela-ui` may depend on `tela-color` for that.
6. **Eyedropper.** Two sources, as in the reference: *canvas* =
   `Document::composite_pixel`, every visible layer over the paper,
   opaque, `f32` (O11); *layer* = the active layer's stored pixel
   un-premultiplied with `Color::from_premultiplied` (O13), which returns
   `None` for alpha 0 (the colour does not change) and clamps channels, so
   low alphas can never produce NaN or out-of-range values (COL6). Both
   read the document pixel under the cursor at any zoom (O14).
   Right-click and Alt pick the canvas with any tool (O10).

# Consequences

- Painting in any colour is the same code path as before; preview ==
  commit holds for colours by the shared `paint_over` (tested with seven
  colours, both depths, alpha lock and clipping).
- Picking translucent 8-bit pixels in layer mode is limited by
  premultiplied 8-bit storage (D-5A-3); 16-bit documents pick exactly.
- Colours chosen in one session are not restored with a document; a
  preferences system can persist the recent colours and swatches later
  without touching the file format.
- No transparent colour (D-5A-1): picking transparency keeps the colour.

# Alternatives considered

- **A colour with alpha (`Rgba`) as the user colour.** Rejected: the
  reference has none, and two alphas (colour and brush opacity) would
  multiply invisibly; the brush already has opacity and flow.
- **Storing the user colour premultiplied or as RGBA8.** Rejected:
  premultiplication is a storage/compositing convention, and 8 bits would
  waste 16-bit documents' precision (O6 shows the reference keeps more
  than 8 bits too).
- **Colour in the document (saved, undoable).** Rejected: the reference
  records no colour change, and colour is a property of the tool, not of
  the picture; it would also change `.mirae` v1.
- **Eyedropper on the display texture.** Rejected: the display is
  reduced when zoomed out; the reference reads the document pixel (O14).
