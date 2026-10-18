# Parity record 5F — gradients

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440 at 100 %, 8-bit sRGB documents. Observed 2026-10-17, **before**
any 5F gradient code was written (the probe tool, `gradient_probe`, and
the measured data were the first 5F commit). Black box only: the
reference opened test documents **we wrote** (`gradient_probe psd
<kind>`, 400 × 300, an opaque black layer, an opaque white layer above it
and a layer "ink") and was operated with the mouse
(SetCursorPos/mouse_event) and keyboard (keybd_event). Results were read
at 100 % zoom, where one screen pixel is one document pixel: over white
and over black (the white layer hidden) to decode the ink layer's
premultiplied RGBA exactly (as in 5D and 5E), then reduced to column,
ring and angle means (`gradient_probe cols|rows|ring|angle`), which
average the dither out. Candidate models were fitted to those profiles.
The sub-tool detail dialog and the gradient editor were opened to read
the options; settings changed for the probes (mixing mode, edge
processing, shape, dithering, stop opacity and colour, a third stop) were
set back with the sub-tools' own "Redef. configurações p/ padrão"; the
palettes shown for the observations were hidden again. Nothing was read
from the reference's files.

Mirae: commits after `bd641e0`, same machine; mouse and the synthetic
Windows Ink pen (`inject_pen`). No physical tablet.

Measured data: `crates/document/tests/reference/gradient/` (README
there).

## Part 1 — the reference (observed)

### The tool and its settings

| # | Steps | Observation |
|---|---|---|
| GD1 | G from another tool; sub-tool palette | The **Gradient** tool ("Degradê"); G again cycles to Fill (5E BK4). Twelve sub-tools: Primeiro plano para transparente (main → transparent), **Primeiro plano para plano de fundo** (main → sub, selected), Listra, Listra de cor de fundo, Fotosfera, Azul céu, Céu de meio-dia, Pôr-do-sol, Céu noturno, Arco-íris, Apagar degradê, Degradê de mangá; a second group "Degradê de forma". |
| GD2 | Tool settings | A gradient bar with its stops; **Forma** (shape): Linha reta (linear, default), Círculo, Elipse; **Processamento de borda** (what is drawn beyond the drag): **Não repetir** (default), Repetir, Inverter, Não desenhar; Ajustar ângulo (off, 45°); Onde criar (draw on the editing layer, default, or a gradient layer); Opacidade 100; Modo de combinação Normal. |
| GD3 | Sub-tool detail dialog | Also **Trama** ("adds subtle noise to the gradient; reduces banding", **on**), Começar do centro (greyed), Inverter (reverse the stops), **Suavização de serrilhado** (anti-aliasing, on). No angular/conic shape and no symmetric ("reflected") shape anywhere. |
| GD4 | Gradient editor ("Configurações avançadas…") | Stops ("nós") with **position 0–100**, **opacity 0–100** and a colour: main colour, sub colour or a specified colour; a click on the bar adds a stop with the colour found there; dragging one off the bar removes it. **Modo de combinação** (mixing): Padrão, **Perceptiva** (default), Perceptiva (compatível com Photoshop), Linear (compatível com Photoshop); **Correção de brilho** (Perceptiva only): Desativada, Baixa, Média, **Alta** (default), A mais alta; Curva de taxa de mistura (a curve, off). |
| GD5 | Main → transparent's stops | Main colour at 0 (opacity 100), **white at opacity 0** at 100. |

### Easing and mixing (black → white and red → blue, drag 100 → 300)

| # | Mode | Observation |
|---|---|---|
| GD6 | Padrão (Standard) | Not linear: an **S-curve**. Slope ≈ 0.5 level/px at both ends, ≈ 2 in the middle, rising linearly: E(u) = **0.38·u + 0.62·q(u)**, q the quadratic ease-in-out (`2u²`, `1 − 2(1 − u)²`), fits the **undithered** capture (`ms_rbw_std_nodither`) with 193 of 211 integers exact and the rest ±1 (rms 0.30 = rounding); smoothstep (rms 1.3), quintic (7.0) and a sinusoidal ease with the same end and middle slopes (0.51) misfit. Red → blue: R and B per channel, R + B = 254 at the middle: interpolation of the sRGB-encoded values by E. |
| GD7 | Linear (compatível com Photoshop) | sRGB-encode(lerp of the **linear-light** values by E): black → white **rms 0.12, max 0.41 levels**; red → blue R 187.8, B 186.5 at the middle. |
| GD8 | Perceptiva (compatível com Photoshop) | Interpolation in **OKLab** (Ottosson 2020) of the linear values by E: red → blue (140.8, 82.5, 161.2) at the middle against OKLab's (141.7, 83.1, 161.2), within ~1 level along the whole ramp; black → white 97.5 (OKLab 97.6). |
| GD9 | **Perceptiva (the default)** with brightness correction | Darker than every public space: black → white at the middle 13.5 (Desativada), 26.6 (Baixa), 41.0 (Média), **54.5 (Alta, default)**, 67.3 (A mais alta); red → blue (49, 24, 42) (Desativada) and **(109, 73, 100)** (Alta), with green appearing as in OKLab. The dark bias stays at the black end whichever end is black (`per3_wb`). Not explained by OKLab on encoded values, CIELab, OKLab with Ottosson's L "toe", or powers of E (Alta ≈ sRGB-decode(E) within ~3 levels, not exactly). |
| GD10 | Three stops (red 0, (255, 128, 128) 50, white 100) | **Each segment is eased on its own**: slope ≈ 0.5 level/px at the middle stop; G = 61.5 ≈ E(½)·128 at a quarter. |

Midpoint (x = 200) of the black → white ramp per mode: Standard 125.9,
Linear 186.5, Perceptual (PS) 97.5, Perceptual (Alta) 54.5.

### Alpha

| # | Steps | Observation |
|---|---|---|
| GD11 | Main → transparent, main red (`ft_red`) | Straight colour stays exactly red; alpha falls along E (128 at the middle). The white of the opacity-0 stop never shows. |
| GD12 | The white stop at opacity 50, Padrão (`ft_w50_std`) | Alpha = lerp of the opacities by E (191.6 at the middle); the **straight** colour = lerp of the colours by E, independently of alpha (straight G = B = 125.9 = 255·E): **straight interpolation**, not premultiplied (premultiplied would give straight G ≈ 83). |
| GD13 | The white stop at opacity 1 (`ft_w1_std`) | The white shows fully in the straight colour (G = 126 at the middle, alpha 129.5): only **opacity 0** is special — a fully transparent stop contributes **no colour** (the segment takes the other end's colour; the same as premultiplied interpolation for that segment). |

### Geometry

| # | Steps | Observation |
|---|---|---|
| GD14 | Linear, "Não desenhar", drag 100 → 300 at 100 % | Drawn exactly on **[100.5, 300.5]**: pixels 100 and 300 half covered (alpha 127.5, 126.5), the edge anti-aliased by area. The press maps to the centre of the screen pixel under the cursor. |
| GD15 | The same at 400 % zoom (`z400_nodraw`) | Drawn on [181.73, 221.73]: the edge pixels 27 % and 73 % covered — presses keep **sub-pixel** precision, length exactly 40.0. |
| GD16 | Colour phase (fits of GD6, GD7 and GD15) | The colour ramp starts **≈ 0.55–0.8 px after** the drawn region (101.1–101.3 at 100 %, 182.28 at 400 %), with the drag's length: the colour is evaluated at the pixel's top-left corner rather than its centre. |
| GD17 | Diagonal drag (100, 50) → (300, 250) | Constant along lines perpendicular to the drag (means equal within 0.03): t is the **projection** on the drag. |
| GD18 | Círculo, drag (150, 100) → (250, 200); (200, 150) → (300, 190); a horizontal drag | Centre at the press; **radius min(|dx|, |dy|)** (100; 40); a horizontal drag draws nothing. The ring profile follows E. |
| GD19 | Elipse, drag (200, 150) → (300, 200) | **Two steps**: the drag sets the centre (the press) and the radii (|dx|, |dy|) = (100, 50); then moving the mouse rotates the ellipse by the angle swept around the centre from the drag's end; a click commits. Committed unrotated: t = √((dx/100)² + (dy/50)²) (the vertical profile is the horizontal one at half scale). |
| GD20 | "Repetir", drag 150 → 250 | E repeats every 100 px in both directions; the wrap pixel is **anti-aliased** (its mean 125.5 between 253 and 0). |
| GD21 | "Inverter", drag 150 → 250 | A triangle wave, mirrored at both ends (symmetric about 150 and 250), smooth. |
| GD22 | Drag from (−100, 150) to (500, 150) | Allowed; the ramp is laid out beyond the canvas (its middle at x = 200). |
| GD23 | Shift held during the drag (100, 100) → (300, 130) | The guide line snaps to the horizontal; the result equals the plain 100 → 300 drag (the end projected on the snapped direction). |
| GD24 | A click without a drag, inside and outside the canvas | Nothing drawn, **no history step**. |

### Dithering ("Trama")

| # | Steps | Observation |
|---|---|---|
| GD25 | Two identical drags | **Bit-identical**: deterministic. |
| GD26 | Pixels of one column | An **ordered** pattern: neighbours anti-correlated (−0.40 right, −0.78 down), period 8 vertically (rows 0 and 8 equal, 0 and 4 not), **one level** of amplitude, at most two levels per column; columns whose value is a whole level get no noise. |
| GD27 | Trama off (`ms_rbw_std_nodither`) | A plain integer ramp (rounding; ceiling fits as well); the dithered column means are **0.48 level lower** on average: the reference's dither is biased by about half a level. |
| GD28 | Long dark ramp, black → (20, 20, 20) over 400 px | The same ordered pattern, one level, two levels per column at most. |

### Writing into the document

| # | Steps | Observation |
|---|---|---|
| GD29 | Main → transparent (black) over opaque red, translucent green and a blue alpha ramp (`over_*`) | **Source-over**: within 1 level of `gradient + before·(1 − α)` on every column. |
| GD30 | The same with the layer's alpha lock | **Source-atop**: colour within 0.5 level of `gradient·αd + before·(1 − α)`, alpha unchanged, transparent pixels untouched. |
| GD31 | Rectangle selection (150, 100)–(250, 200), drag 100 → 300 | Written exactly inside the selection; the values are those of the unselected gradient (the **geometry comes from the drag**, not from the selection). |
| GD32 | The selection feathered 10 px | The gradient's alpha scaled by the selection's coverage (the 5C feather), within 1–2/255 where α is large. |
| GD33 | The layer's mask (hiding everything) as the target: main → transparent; black → white | The mask's reveal becomes the gradient's **alpha** (as `ft_red`); an opaque black → white reveals **everything**: colour ignored (4D, 5E BK25). |
| GD34 | Layer in Multiply at 50 %, then set to Normal 100 % | The stored pixels equal a gradient on a Normal layer: **opacity and blend mode not baked**. |
| GD35 | A clipped layer, then unclipped | The whole layer holds the gradient: clipping only limits the display. |
| GD36 | History palette | One step "Degradê/Degradê/<sub-tool>" per gradient; undo restores. |
| GD37 | During the drag | Only a **guide line** (and for circles and ellipses their outline) is shown; the gradient appears at the release. |
| GD38 | Escape during the drag | **Ignored**: the gradient is drawn at the release. |

Not observed: the "Onde criar" gradient layer, the tool's own opacity and
blend mode, "Curva de taxa de mistura", "Degradê de forma", the
reference's speed and memory, rotated and mirrored views, a real pen, and
anything in 16-bit (the reference has no 16-bit documents).

## Part 2 — Mirae design from the observations (ADR 0027)

| Topic | Mirae | From |
|---|---|---|
| Stops | `GradientStop` (position 0–1, opaque colour, opacity 0–1), `GradientStops` sorted stably, equal positions a hard edge; the panel's stops take the main colour, the sub colour or a fixed colour, resolved when the drag begins | GD4, GD5 |
| Easing | every segment eased by `E(u) = 0.38u + 0.62q(u)` | GD6, GD10 |
| Mixing | Standard (sRGB-encoded), Linear (linear light), Perceptual (OKLab, default) | GD6–GD9 |
| Alpha | straight colour and alpha, interpolated independently; an opacity-0 stop has no colour | GD11–GD13 |
| Shapes | linear, radial, elliptical (axis-aligned, radii `|dx|`, `|dy|`), angular, reflected | GD17–GD19 (angular and reflected: Mirae's) |
| Ends | clamp, repeat (wrap blended by area), reflect, empty (edge covered by area) | GD2, GD14, GD20, GD21 |
| Geometry | the press and the pointer through the one view mapping, document coordinates, pixel centres; Shift snaps to 45° on screen; a drag shorter than 0.001 px draws nothing | GD15, GD22–GD24 |
| Dither | on by default; Mirae's own 8 × 8 ordered matrix anchored to document pixels, one level of the storage's precision, mean-preserving | GD3, GD25–GD28 |
| Document | source-over, source-atop with alpha lock, erase (eraser end or E), masks by the gradient's alpha; selection limits and scales; opacity, blend, clipping not baked; one step; refusals as strokes | GD29–GD36 |
| Preview | live: the compositor's overlay runs the commit's function (bit-identical); a guide line and the radial/elliptical outline; Escape cancels | GD37, GD38 (Mirae goes further) |
| Tool | G cycles fill and gradient; panel: shape, ends, mixing, dither, presets main → sub and main → transparent, stop editor | GD1, GD2 |

## Part 3 — Mirae compared

### With the reference's measurements (tests, `document::gradient::parity`)

The reference's captures are dithered (its own pattern, biased about
half a level, GD27), so ramps are compared through column (or ring)
means, which average the dither out; Mirae draws undithered there. Steep
ramps (circles, the ellipse, 100-pixel repeats) are compared as
**positions**: each mean is turned back into the distance it encodes
(the inverse of the easing), so the claim is in pixels. Measured worst
cases (`TELA_PARITY_REPORT=1`):

| Check | Result |
|---|---|
| Black → white and red → blue in Standard, Linear and Perceptual (PS), 8 and 16 bits, every channel (GD6–GD8) | within **1.5 levels** (column means) |
| Three undithered stops, Standard, pixel by pixel (GD10, GD27) | within **2 levels**; 68 % of the pixels equal |
| Diagonal drag (GD17) | within 1.25 levels (means along the isolines) |
| Circles of radius 100 and 40 (GD18) | within **0.97 and 0.67 px** of position |
| Ellipse 100 × 50 along both axes (GD19) | within 1.39 px (single dithered pixels) |
| Repeat (GD20) | within 0.66 px; both wrap within one pixel of the same places |
| Reverse (GD21) | within 1.19 px (the reference's colour phase appears with both signs on mirrored segments) |
| "Nothing beyond the ends" at 100 % and 400 % zoom (GD14, GD15) | alpha within 1.5 and 1.0 levels; colour within 2.25 (the colour phase, D-5F-2) |
| Main → transparent, white at opacity 0, 50 and 1 % (GD11–GD13) | within 1.5 levels, every channel |
| Over opaque, translucent and alpha-ramp pixels, with and without alpha lock (GD29, GD30) | within 2 levels; alpha unchanged with the lock |
| Hard and 10 px feathered selections (GD31, GD32) | hard: the same pixels changed, alpha within 2 per pixel; feathered: within 4 per pixel, 2.9 in column means (Mirae's 5C feather) |
| Mask: main → transparent and black → white (GD33) | reveal within 1.5 levels; black → white reveals everything |
| The reference's default "Perceptiva" (GD9) | **not reproduced** (D-5F-1) |

### In the running app (2026-10-17, mouse and synthetic pen; screenshots not committed)

A 4000 × 3000 document, the release build, `RUST_LOG=tela::gradient=debug`.

| # | Steps | Mirae |
|---|---|---|
| 1, 13, 15 | G, black → white across the canvas; a screenshot during the drag | The live preview with the guide line; on release the same picture, one step (192 tiles, 212 ms) |
| 2 | Red main, blue sub, the same drag | OKLab red → blue (purple middle) |
| 3 | Main to clear, a diagonal over it | Red fading to transparent over the existing gradient (source-over) |
| 4, 5 | Radial (guide circle during the drag), angular, reflected with Mirror ends | As designed: the angular seam along the drag, mirrored stripes |
| 6 | Add a stop, set it to yellow | Three stops, each segment eased |
| 7 | Rectangle selection, a black → blue gradient across it | Only inside the selection, crisp edges (the geometry from the drag) |
| 8 | The selection feathered 120 px | Soft edges |
| 9 | New layer, a radial disc ("None" ends), Lock alpha, a red → white linear gradient | Only the disc recoloured, alpha kept; the commit visited the layer's 29 tiles only |
| 10 | Add mask, eraser mode (E), G, main → transparent across the disc | The mask hides the disc progressively ("Strokes edit: mask") |
| 12 | New layer clipped to it, a vertical gradient | Visible only through the disc and its mask |
| 11 | New layer in Multiply, red → white | Multiplied over the picture |
| 14 | A drag, Escape before the release | The screen identical to before (0 of ~75 000 sampled pixels differ); no step (the next undo removed the Multiply gradient) |
| 16 | Undo, redo | Undo removes the last gradient; redo restores the screen pixel for pixel |
| 17 | Wheel zoom ×4 and the view rotated 30°, a radial drag | The guide circle and the result agree; the centre at the press in document space |
| 18 | To 16-bit, a long dark black → grey gradient | Smooth; commit 153 ms |
| 19 | Save As `.mirae` (Shift+Alt+S), New, Open | Reopened identical on screen (0 sampled pixels differ) |
| 20 | Export PNG | 4000 × 3000, the composed picture |
| pen | The synthetic Windows Ink pen dragging a radial gradient (`inject_pen drag:…`) | Drawn like the mouse (the title shows "pen") |

## Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-5F-1 | **The reference's default mixing**: "Perceptiva" with brightness correction (Alta by default) is darker than every public space tested (black → white middle 54.5 against OKLab's 97.5, GD9); Mirae offers Standard, Linear and OKLab (the reference's other three modes, reproduced) and defaults to OKLab, the nearest. Brightness correction is not offered. | Quality | Open (needs more probes of its colour model) |
| D-5F-2 | **Colour phase**: the reference's colour ramp sits ≈ 0.55–0.8 px after its drawn region (GD16; as if evaluated at the pixel's corner for linear ramps, ~0.7 px late in distance for circles); Mirae samples pixel centres from the exact press point, and a mouse position is a pixel corner in Mirae but the pixel's centre in the reference (GD14). Up to ~2 levels on the steepest 8-bit ramps, under 1 px of position. | Quality | Accepted (sub-pixel) |
| D-5F-3 | **Shapes and drag mapping**: the reference's circle has radius min(|dx|, |dy|) (a horizontal drag draws nothing, GD18) — Mirae's radial takes the drag's length; the reference's ellipse has a second step that rotates it (GD19) — Mirae's is axis-aligned; Mirae adds angular and reflected shapes the reference does not have. | Feature / UI | Mirae's design |
| D-5F-4 | **Dither pattern**: Mirae's 8 × 8 Bayer matrix, mean-preserving, at the storage's precision (16-bit documents dither at 1/65535); the reference's ordered pattern (period 8, GD26) is not reproduced and is biased ≈ ½ level low (GD27). Same amplitude (one level at 8 bits) and determinism. | Quality | Deliberate |
| D-5F-5 | **Live preview and cancel**: Mirae previews the gradient itself during the drag and Escape cancels; the reference shows a guide line only and ignores Escape (GD37, GD38). | Behaviour | Mirae goes further (the brief) |
| D-5F-6 | **Options not implemented**: the gradient layer ("Onde criar"), the tool's own opacity and blend mode, angle snapping as a setting (Shift snaps), the mixing-rate curve, the reference's preset gradients beyond main → sub and main → transparent, "Degradê de forma", stop colours beyond main, sub and a fixed (active) colour. | Feature | Not implemented |
| D-5F-7 | **Large previews**: a whole-canvas preview costs ~0.4 s per frame at 4096² (one core); Mirae composes the tiles on screen first within ~12 ms per frame and the rest over the next frames, so the far parts of a huge canvas lag the pointer. The reference's preview cost is not comparable (guide only). | Performance | Measured (debt #43) |
| D-5F-8 | **History of a no-op**: a drag without length records nothing in both (GD24); a gradient that changes no pixel records nothing in Mirae (rule H6), not observed in the reference. | Behaviour | As 5E (D-5E-4) |
| D-5F-9 | **16-bit**: the reference has no 16-bit documents; Mirae's 16-bit gradients pass the same parity tests as 8-bit ones (identical tolerances). | Coverage | Accepted |
| D-5F-10 | **Not observed**: rotated and mirrored views in the reference, the reference's speed and memory, a real pen. | Coverage | Open |

## After the gate: approval revoked, quality gate (2026-10-17)

The owner revoked the approval of 5F. The E2E/UI audit
([../quality/UI_AUDIT.md](../quality/UI_AUDIT.md)) found the gradient's
problems in the application around the engine, not in the measured rules
above: the brush's eraser mode silently made the gradient erase (Q-05),
the whole-canvas preview lagged and starved tiles (Q-02), a ghost preview
stayed after a focus loss (Q-03), the stop buttons were out of reach at
the default window (Q-01). They are fixed; the desktop E2E suite compares
the running app's gradients with the engine applied to the same geometry
on every shape and end, preview included. Changes to the description
above: the gradient has its own **Draw / Erase** mode (Draw by default);
"eraser mode (E)" in the app session (step 10) is now that Erase mode;
the eraser end still erases. Nothing measured above changed.
**Status: implementation present, owner approval pending.**
