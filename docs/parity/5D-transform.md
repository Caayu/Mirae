# Parity record 5D — transform

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440 at 100 %, 8-bit sRGB documents. Observed 2026-10-17, **before**
any 5D transform code was written (the probe tool, `transform_probe`, and
the measured images were the first 5D commit). Black box only: the
reference opened one test document **we wrote** (`transform_probe psd
tr.psd`: an opaque black layer, an opaque white layer above it and a layer
"ink" holding a 64 × 64 test pattern at (168, 118), centre (200, 150), on a
400 × 300 canvas) and was operated with the mouse
(SetCursorPos/mouse_event) and keyboard (keybd_event). The pattern: a
one-pixel black/white checker, coloured 4 × 4 blocks, a red alpha ramp
(alpha 4…252), a soft blue Gaussian blob, an isolated opaque red square
and an anti-aliased green line; transparent pixels carry a hidden colour
(pure green) so that interpolating straight colour would show it.

The ink layer was read at 100 % zoom (one screen pixel is one document
pixel; validated in 5C) twice, over the white layer and with the white
layer hidden (over black), which gives its **premultiplied** RGBA exactly
(colour = capture over black, alpha = 255 − (white − black);
`transform_probe decode`; the untransformed pattern decodes with 0
error). Candidate resampling models were scored on every pixel
(`transform_probe fit`). The tool settings palette and the History
palette were shown for the observations and hidden again; interpolation,
"keep aspect" and the reference point were set back to their defaults.
Nothing was read from the reference's files.

Mirae: commits after `56974bf`, same machine; mouse and the synthetic
Windows Ink pen (`inject_pen`). No physical tablet.

Measured images: `crates/document/tests/reference/transform/` (README
there).

## Part 1 — the reference (observed)

### Commands and the session

| # | Steps | Observation |
|---|---|---|
| T1 | Editar › Transformar | Redimensionar/girar (**Ctrl+T**), Redimensionar, Girar, Transformação livre (**Ctrl+Shift+T**), Distorcer, Inclinar, Perspectiva, **Virar horizontalmente**, **Inverter verticalmente**, Transformação de malha, Distorção de marionete; OK and Cancelar (enabled while a transform is open). |
| T2 | Ctrl+T on the ink layer, no selection | A **session** opens: a box around the layer's **content bounds** (exactly 168..232 × 118..182: the non-transparent pixels), 8 handles (corners and sides), a rotation handle above the top side, a centre marker (the pivot), and a floating bar with OK / Cancelar. The title gets `*` at once. The tool settings show the transform's settings: Modo, **Ponto de referência (Centro)**, Manter a imagem original (off), **Proporção de redimensionamento l/h %**, **Manter relação de proporção (on)**, **Ângulo de rotação**, Ajustar posição, **Método de interpolação**; buttons reset, flip H, flip V, OK, cancel. |
| T3 | Typing 200 in "l" and Enter in the field | Width and height become 200 (aspect kept); the **preview** shows the result; the session stays open (Enter in a field does not commit). |
| T4 | Enter on the canvas | **Commits**: the box disappears, the tool settings return to the active tool. |
| T5 | Escape | **Cancels**: the layer is bit-exact as before (0 difference). Tool settings changed during a cancelled session (interpolation, keep aspect) revert to what they were; changed during a committed one, they persist (they are tool settings, not part of the step). |
| T6 | A click (no drag) outside the box | Nothing: neither commit nor cancel; the session stays open. |
| T7 | A drag outside the box | **Rotates** about the pivot. |
| T8 | A drag inside the box | **Moves** the content. A drag of 0.5 document px (4 screen px at 800 %) moved it by **exactly 1 px** (bit-exact copy): pure translations are whole pixels (see T17). |
| T9 | Dragging the centre marker to (220, 170), then rotation 30° | The **pivot moves** ("Ponto de referência" becomes "Posição livre"); the rotation is about the new pivot (fit with pivot (220, 170): interior exact, see T16). Reference point options: Centro, four corners, Topo, Esquerda, Direita, Fundo, Posição livre. |
| T10 | Corner handle dragged by (−28, −10), "keep aspect" on | Scales about the **opposite corner**, **proportionally** (116 %/116 %); Shift changes nothing. |
| T11 | Side handle dragged by +30, "keep aspect" on | Proportional too (147 %/147 %). |
| T12 | "Keep aspect" off: corner drag; corner drag with Shift | Free (144 %/116 %); **Shift** makes it proportional (116 %/116 %, the smaller factor). |
| T13 | History palette during a session in which width, angle and width again were typed | Sub-steps "Redimensionar/girar, Transformar imagem, Girar imagem, Transformar imagem"; **Ctrl+Z inside the session undoes the last edit** (the rotation went, the scale stayed, "Girar imagem" became redoable). After Enter they collapse into **one** step "Redimensionar/girar". |
| T14 | Ctrl+Z after a commit; Ctrl+Y | Undo restores the layer **bit-exactly** (0 difference); redo restores the committed result bit-exactly. Cancel records nothing. A commit without any change leaves the pixels identical. |
| T15 | Flip horizontal (menu) | Opens a Scale/Rotate session with width **−100 %** (flip is a negative scale about the reference point) and the flip button highlighted; after Enter the layer is the **exact mirror** of its content box (0 difference). |

### Pixels (bicubic is the default)

| # | Steps | Observation |
|---|---|---|
| T16 | Interpolation options | "Suavizar bordas (bilinear)", "Bordas duras (vizinho mais próximo)", **"Bordas nítidas (bicúbicas)" (the default)**, "Alta precisão (cores médias)". |
| T17 | Output geometry, scaled 200, 110, 75, 50, 45, 33, 30 % and 150 × 60 % | The transformed box's **edges are rounded to whole pixels** (half up: 33 % → 189.44..210.56 → 189..211; 30 % → 190..210; 110 % → 165..235; 60 % height → 131..169) and the content is **stretched to fill the rounded box** (effective scale = rounded size / source size). Edges are hard. Pure translations therefore move by whole pixels (T8). |
| T18 | Bicubic, 200 % | Reproduced on **every pixel within 1/255** (mean 0.004 over 12 768 pixels) by: **Keys cubic with a = −1** on **premultiplied** colour; pixels outside the content box are transparent; the sample position is clamped to the centres of the box's edge pixels; output only where the pixel centre maps inside the box; the result is **un-premultiplied by the raw (unclamped) alpha**, alpha and straight colour clamped to `0..1` (stored straight at 8 bits). Rejected: straight-colour interpolation (2.2 mean, 74 max: the hidden colour and dark fringes would show), a = −0.5/−0.75, Mitchell, B-spline, Lanczos 2/3, bilinear, edge replication, mirroring, renormalized kernels at the edge, anti-ringing clamps (each scored worse; 0.3–14 mean, 30–146 max). |
| T19 | Bicubic, 75 %, 110 %, 150 × 60 % | The same model within 2/255 (no prefilter). |
| T20 | Bicubic, 50 %, 45 %, 33 %, 30 % | A **pyramid**: while the scale is at most ½ on both axes the box is halved by 2 × 2 averages (anchored at its corner), then the same cubic samples the reduced level: within **2/255** on every pixel (the 1-px checker becomes a uniform 127, the blocks alias as a cubic of the halved image would). Without the pyramid: 17–42 mean. |
| T21 | Bicubic, rotation 15°, 45°, −30°; about a moved pivot (30°) | Positive angles turn **clockwise on screen**. The interior is the same model within 2/255; the rotated box's **edges are anti-aliased by distance**: alpha × `clamp(0.5 + d)` per side (d = signed distance from the pixel centre to the side, in pixels; product over the sides): the edge ramp steps by 255·sin θ (66/255 at 15°), not by the exact pixel area (255·tan θ). Remaining: the colour of the outermost, partly covered pixel ring (≈1.5 % of the pixels, up to 24–28/255; the reference picks a "purer" source colour there) and a few corner pixels. |
| T22 | Nearest neighbour, 200 %, 30 %, 15° | Exact on every pixel: the source pixel whose centre is nearest, **ties to the lower index** (`⌈u⌉ − 1`), **no pyramid, no edge anti-aliasing** (hard edges also when rotated). |
| T23 | Bilinear, 200 %, 30 %, 15° | Within 2/255 by the bicubic pipeline with a bilinear kernel (pyramid, clamped sample, un-premultiplied by the raw alpha, distance-anti-aliased edges). |
| T24 | "High accuracy (average colours)", 200 %, 30 %, 15° | 200 % equals nearest neighbour; reduction is an area average (closest model 2.1 mean, 12 max); rotation not modelled (1.8 mean, 25 max). |
| T25 | Partially transparent pixels (the ramp, the blob, the line), the opaque red square beside transparency | No hidden-colour or dark fringe anywhere: interpolation is premultiplied (T18). |
| T26 | 8-bit document | The reference has only 8-bit documents (4E); straight colour is stored at 8 bits after the transform. |

### What is transformed

| # | Steps | Observation |
|---|---|---|
| T27 | No selection | The **whole layer** (its content bounds) is transformed; nothing stays behind. |
| T28 | Rectangle selection (150, 100)–(200, 160) over part of the pattern, Ctrl+T | The box is the **selection's bounds** (not the content's). |
| T29 | That selection scaled 200 % | Only selected content is transformed; the transformed content is composed **over** the unselected pixels that stay (the scaled translucent ramp over the blob: alpha 168 where `over` predicts 171, blue 95 vs 93). |
| T30 | That selection moved +100 px | The original place becomes **transparent** (already in the preview); the moved content is an exact copy (0 difference). |
| T31 | Feathered selection (10 px), moved +100 px | **Split by the coverage**: what moves is `pixel · s`, what stays is `pixel · (1 − s)`, and the moved part is composed over what stays — within 3/255 everywhere using Mirae's selection model (mean 0.30); a binary cut (hole only where s = 255) is 23.6 mean, 253 max. |
| T32 | Selection after the transform | The **selection follows the content**: after 200 % the marching ants surround the scaled rectangle (125..225 × 70..190); after a 15° rotation the selection's coverage (filled on a new layer) has the rotated rectangle's area (3 000.4 for 50 × 60) and the **same distance anti-aliasing** on its edges (236, 170, 105, 38 along a row). The floating selection bar comes back. |
| T33 | Layer mask (hiding x 184..216), Ctrl+T with the **mask** as target; then with the **pixels** as target | The box is the **union** of the pixels' and the mask's bounds (168..232 × 100..200); pixels **and** mask are transformed **together** (the mask is linked to its layer, chain icon) whatever the target: both results are identical. The mask is resampled by the same cubic (its edge after 150 %: 213, 43 where the cubic predicts 212, 42.5). |
| T34 | Layer locked, Ctrl+T | **Refused silently** (no session; the transform button is disabled). |
| T35 | Layer hidden, Ctrl+T | Refused silently. |
| T36 | Empty layer, Ctrl+T | A message **"Não há objeto para transformar."** (nothing to transform), OK. |
| T37 | Alpha lock ("bloquear pixels transparentes") on, 150 % | **No effect**: the content grows beyond its old alpha as without the lock (within 3/255 of the model). |
| T38 | The ink layer clipped to "white"; "white" (the base) scaled 50 % | Only the base is transformed; the clipped layer stays where it is and still shows only inside the (smaller) base: the clipping relation is kept. Transforming a clipped layer changes its own pixels only (T27). |
| T39 | Content moved to x = −12 (partly off the canvas), committed; a second session | The second box extends beyond the canvas (off-canvas pixels were **kept**); moved back, the layer is **bit-exact** as at the start. |
| T40 | Layer opacity and blend mode | Not changed by a transform (properties stay; nothing in the UI changes). |

### Interruptions, view

| # | Steps | Observation |
|---|---|---|
| T41 | Selecting another layer during a session | A modal **"Aplicar transformação?" — Sim / Não / Cancelar**. Cancelar returns to the session. |
| T42 | Clicking another tool during a session | The same question; **Não** discards the transform (the layer is unchanged) and switches the tool. A tool's shortcut key alone did nothing. |
| T43 | Ctrl+S during a session; the File menu | Ctrl+S does nothing; **Novo, Abrir, Fechar, Salvar, Salvar como, Reverter and Sair are disabled** while the session is open (exports are not). |
| T44 | Session at 800 % zoom | The handles keep their **screen size**; the box follows the document. |
| T45 | View rotated 10° | The box is **document-aligned** (it turns with the canvas); handles keep their screen size. |

Not observed: the Move layer tool (it could not be activated by
automation in this session), Free Transform's distortions (out of 5D's
scope), perspective, mesh, "keep original image", the reference's
behaviour on an empty selection, a mask with a selection, 90° rotations,
anisotropic reductions (one axis ≤ ½, the other not), performance and
memory of the reference, a real pen.
