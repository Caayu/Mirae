# Parity record 5A — colour

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440 at 100 %, 8-bit sRGB documents. Observed 2026-10-17, **before**
any 5A code was written (the probe document generator was the first 5A
commit). Black box only: the reference was operated with the mouse
(SetCursorPos/mouse_event) and keyboard (keybd_event); results were read
from screen pixels at 100 % zoom (one screen pixel = one canvas pixel),
from its main-colour swatch (which shows the colour exactly as painted),
its colour palettes, its History palette and its shortcut list. It opened
one test document **we wrote** (`color_probe psd`, public PSD format).
Nothing was read from its files; no setting was changed (no shortcut was
bound; brush opacity was moved with its slider and set back).

Mirae: commits `3ddbaf2`…`d12a5fc` and later, same machine; mouse and the
synthetic Windows Ink pen (`inject_pen`). No physical tablet.

## Part 1 — the reference (observed)

### Colour model and controls

| # | Steps | Observation |
|---|---|---|
| O1 | Look at the colour icons (tool bar, colour wheel and colour slider palettes) | Two colours, **main** and **sub**, plus a **transparent colour**; exactly one is *active* (framed). Strokes paint with the active one. State seen: main black, sub white. |
| O2 | `X`, paint; `X` again, paint | `X` makes the **other slot active**; neither colour changes (black stroke, then white, then black). |
| O3 | Arquivo › Configurações de atalho › Opções › Cor de desenho (read only) | Commands: "Alternar cor principal e subcor" (`X`), "Alternar cor de desenho e cor transparente" (`C`), "… principal, subcor e cor transparente", "Alternar para cor principal / subcor / cor transparente", "Alternar entre cor principal e subcor", **"Alternar cor principal para preto e subcor para branco"** (reset), "Abrir configurações de cores". Only `X` and `C` are bound by default; reset and the second main/sub command are **unbound** (not exercised: binding would change a setting). |
| O4 | Colour slider, RGB tab: R 37, G 149, B 213; paint | Integer fields 0–255. Stroke and swatch exactly 37,149,213. |
| O5 | HSV tab for that colour | h **202**, S **83 %**, V **84 %**: standard HSV on the encoded values, integer degrees and percent. The colour wheel can switch to HLS. |
| O6 | HSV tab: h 120, S 50 %, V 50 %; read the RGB tab; paint | RGB fields show **64,128,64**, but the stroke, the swatch and the hex field are **63,128,63** (`3F803F`): the colour is kept finer than 8 bits; the fields round, painting and hex truncate. |
| O7 | Double-click the main colour: "Configurações de cores" | HSV, RGB, Lab, CMYK and **HEX** (6 digits, `3F803F`). Typing `FF800080` keeps `FF8000` (the field takes 6 characters); applied live. **No alpha** anywhere in the colour UI. |
| O8 | History palette after colour edits, `X`, hex; undo/redo a stroke | Only the strokes are steps. Undo removes the stroke, not the colour; the colour stays. |
| O9 | Colour history palette | Newest first, **each colour once** (black painted twice appears once, moved to the front). A colour enters when a **stroke uses it** or the **eyedropper picks it**; selecting a colour without using it (the intermediate 37,0,0 while typing) does not enter. A transparent pick adds a transparent entry. Capacity not measured. |

### Eyedropper (probe document: swatches 220,80,40 and 37,149,213 at alpha 255/128/64/8/1/0, an opaque 220,80,40 band under an opaque 100,150,250 Multiply band, a 1-px checker)

| # | Steps | Observation |
|---|---|---|
| O10 | Eyedropper tool (`I`); with the pen: right-click, Alt+click | Two sub-tools: **"Selecionar cores exibidas"** (displayed colours, default) and **"Selecionar cor da camada"** (layer colour). Right-click and Alt+click with the pen pick the **displayed** colour (the Multiply overlap gives 86,47,39) and the pen stays selected. |
| O11 | Displayed colours, with the paper layer | The colour **on screen**, paper included, opaque: alpha 128 → 237,167,147; overlap → 86,47,39; Multiply over paper → 100,150,250; alpha 0 → paper white. |
| O12 | Displayed colours, without paper (as the PSD opens) | Translucent pixels give the **straight** colour of the composite: alpha 128, 64, 8 **and 1** all give exactly 220,80,40 (or 37,149,213). Alpha 0: the main colour is unchanged and the **transparent colour becomes active**. |
| O13 | Layer colour, "swatches" layer active | The active layer's own straight colour, ignoring layers above (overlap → 220,80,40); alpha 128…1 exact; alpha 0 → transparent colour active. |
| O14 | Zoom 33 %, pick on the 1-px checker (shown as 127 grey) | Picks **0,0,0 or 255,255,255**: the document pixel under the cursor, not the reduced display. |

### Painting with colour

| # | Steps | Observation |
|---|---|---|
| O15 | G-pen 10 px, brush opacity 51 %, colour 37,149,213 over white paper; two separate strokes crossing over 100,150,250 | One stroke: **144,201,234** = 0.51·C + 0.49·paper. Over 100,150,250: 68,149,231; the second stroke compounds: 52,149,222 (same formula applied again). Within one stroke the test was inconclusive (the passes did not overlap); no claim. |
| O16 | "Lock transparent pixels" on the swatches layer, pure blue across row 1 | Alpha 255 → 0,0,255; alpha 128 → **127,127,255** over paper; alpha 0 → nothing. Edge pixel over the alpha-128 swatch **186,148,198** = source-atop `paint·m·αd + dst·(1 − m)` with m ≈ 0.466 in all three channels. |
| O17 | New layer clipped to the Multiply layer, pure red stroke across row 3 | Outside the base: nothing; over the Multiply band on the 220,80,40 band: **220,0,0**; over the band on paper: **255,0,0** — the 4D model (ADR 0017) with colour. |
| O18 | Masks with colour | Already observed in 4D (M4): red, black, tan and white reveal identically. |
| O19 | Blend modes with colour | Measured in 4C (3 400 combinations, `blend_parity.rs`). |

## Part 2 — Mirae design from the observations (ADR 0021)

- **Colour value**: `tela_color::Color`, opaque, straight, `f32`
  sRGB-encoded channels, always valid (COL1). No alpha (O7): how much
  paint lands is the brush's opacity/flow/coverage.
- **Tool state** (`app::colors::ColorState`): main, sub, active slot; `X`
  toggles the active slot (O2); Swap button exchanges the values; `D` /
  Reset sets black on white with main active (O3's command, unbound
  there); recent colours newest first, unique, 24, fed by strokes and
  picks (O9); 16 built-in swatches of our own, add/remove, 64 max. Not
  saved in documents or history (O8).
- **Panel**: HSV square and hue bar, RGB 0–255, HSV in degrees/percent,
  6-digit hex (O4–O7), all showing one colour; the colour keeps `f32`
  precision (O6), the fields round.
- **Eyedropper**: canvas (= O11, composite over the paper) and layer
  (= O13); right-click and Alt (also with the pen) pick the canvas (O10);
  the document pixel at any zoom (O14).
- **Stroke**: captures the colour at its start (COL2); `(C·m, m)` over
  the destination, source-atop with alpha lock, colour ignored by the
  eraser and on masks (COL3, COL4).

## Part 3 — Mirae compared (running app and tests)

8-bit session, then converted to 16-bit, mouse unless stated:

| Check | Mirae | Reference | Result |
|---|---|---|---|
| Red swatch, stroke | 255,0,0; fields R 255, H 0 S 100 V 100, `#FF0000` | — | consistent |
| Hex `#0000FF`, stroke | 0,0,255 | — | consistent |
| `X`, Swap, Reset (`D`) | X frames the sub; Swap exchanges values (sub active paints blue); D gives black on white, main active | O2, O3 | X = reference; Swap/Reset are Mirae commands (reference has them unbound) |
| RGB 37,149,213 readout | H 202, S 83, V 84, `#2595D5` | O5 | **equal** |
| 51 % stroke of 37,149,213 over paper | **144,201,234**; over red 144,76,109; two strokes 89,174,223 (both = 0.51·C + 0.49·below) | O15 144,201,234; compounding | **equal** |
| Right-click / Alt+click / Alt + synthetic pen | pick the composite (red, blue, light blue); pen and tool unchanged | O10 | equal |
| Right-click outside the document | colour unchanged | — | — |
| Layer pick, transparent pixel | colour unchanged | O13: transparent colour active | **differs** (D-5A-2) |
| Layer pick, 51 % paint stored at 8 bits | 37,149,**214** | O13: exact | **differs** (D-5A-3) |
| Layer pick, 51 % paint stored at 16 bits | 37,149,213 | exact | equal |
| Recent colours | blue, red after two strokes; picks added; unique, newest first | O9 | equal (capacity Mirae's) |
| Airbrush (magenta), chalk (green, textured), calligraphy, marker | each in its colour (chalk keeps r = b) | — | consistent |
| Alpha lock, green across strokes | opaque → 0,192,0; 51 % pixels → 125,223,125 (alpha kept); paper untouched | O16 | same rule; test reproduces the reference pixels within 1/255 |
| Clipped red layer, Normal then Multiply | Normal: red only inside the base; Multiply: over blue 0,0,0, over the 51 % stroke 144,125,125 | O17 | same rule |
| Clipped red in Screen, Overlay, Darken, Hard Light, Soft Light, Difference, Hue, Saturation, Color, Luminosity | e.g. Overlay over the 51 % stroke 163,147,213; Hue over blue 94,0,0; Luminosity over blue 54,54,255 | 4C formulas | values as the W3C formulas predict (the 4C dataset is the quantitative check) |
| Mask: erase a band, paint red on it; undo; paint blue on it | identical pixels across the stroke and its edges; undo/redo exact | 4D M4 | equal |
| Convert to 16-bit | composite unchanged; 51 % stroke 144,201,234 again | — | consistent |
| Save As `.mirae`, reopen, save again | second file byte-identical to the first | — | bit-exact (IO1, IO2) |
| Export PNG | pixels as on screen (51 % paint 144,201,234 at both storage depths; pen magenta 255,0,255) | — | consistent |
| Synthetic pen, magenta | 255,0,255 | — | consistent |

Tests: `color::paint::tests` (HSV readout of O5, hex, picking at
alpha 255/128/1/0), `color/tests/properties.rs`, `brush::stroke::tests::
preserved_alpha_recolours_like_the_reference` (O16),
`…::colored_paint_follows_the_documented_formula` (O15),
`document::compose::tests::composite_pixel_is_what_the_reference_eyedropper_picks`
(O11), `…::colored_preview_matches_commit_at_both_depths`,
`brush/tests/color_properties.rs`, `app::colors::tests`.

## Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-5A-1 | No **transparent colour** (the reference's third slot; `C`). Mirae erases with the eraser (`E`, stylus eraser end). | Feature | Open — decide with the owner; `C` is "clear layer" in Mirae |
| D-5A-2 | Picking a transparent pixel (layer mode) keeps the colour; the reference switches to its transparent colour (O12, O13). Over Mirae's always-opaque paper, canvas picks are always opaque, as the reference's with paper (O11). | Behaviour | Follows from D-5A-1 |
| D-5A-3 | Layer picks of translucent pixels: the reference returns the exact straight colour even at alpha 1 (O12, O13); Mirae un-premultiplies its premultiplied storage, so 8-bit pixels at low alpha come back up to a few levels off (at alpha 1/255, 0 or 255 per channel); 16-bit storage is exact at ordinary alphas. Same root as D-4F-4. | Quality | Accepted (ADR 0001 premultiplied storage); 16-bit documents reduce it |
| D-5A-4 | Painting/hex of a colour that is not on an 8-bit level: the reference truncates (63.75 → 63, O6); Mirae rounds (64) in the fields, the hex, and when the commit quantizes. | Quality (≤ 1 level) | Accepted: rounding is Mirae's rule everywhere (D-4F-3) |
| D-5A-5 | Eyedropper is a toggle in the colour panel plus right-click/Alt; the reference's `I` tool shortcut is not bound (Mirae uses `I` for the interpolation cycle). | Feature/UI | Open (shortcut map, later) |
| D-5A-6 | No colour wheel, HLS, Lab or CMYK fields, intermediate/approximate colour palettes or colour sets from files; swatches and recent colours are not saved between sessions. | Feature | Open — colour sets and preferences persistence later (debt #35) |
| D-5A-7 | Recent-colour capacity: Mirae keeps 24; the reference's capacity was not measured. | Behaviour | Open (coverage) |
| D-5A-8 | The reference's reset and second main/sub command are unbound and were not exercised; Mirae binds reset to `D` and offers Swap as a button. | Behaviour | Open (coverage); not comparable without changing reference settings |

Not observed: colour pick with a real pen, brush flow alone with colour
(flow is a stamping property, colour-independent by construction),
within-stroke overlap of colour at partial opacity, colour history
capacity, the reference's behaviour in colour-managed documents.
