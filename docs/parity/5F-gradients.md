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
