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

Mirae: commits after `ee22784` (`fef166a`, `6f31cd7`, `8ea053b`), same machine; mouse and the synthetic
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

## Part 2 — Mirae design from the observations (ADR 0025)

| Topic | Mirae | From |
|---|---|---|
| Session | `TransformSession` in the app (source + list of edits); Ctrl+T, the panel's Transform, Flip H/V (open a session with the flip, as the reference's menu does); Enter / OK commits one history step; Escape / Cancel discards and restores tool settings changed during the session; undo/redo inside the session walk its edits; the document never changes before the commit | T2–T5, T13–T15 |
| Map | `x' = pivot + translate + R(θ)·S·(x − pivot)`; negative scale = flip; angle clockwise; the pivot's cross can be dragged (same map about another point) | T9, T15, T21 |
| Handles | corners scale about the opposite corner, sides about the opposite side; keep aspect (on by default) makes both proportional, Shift too when it is off (the smaller ratio); inside moves; outside or the round handle rotates; a click does nothing | T6–T12 |
| Fields | W %, H % (H follows with keep aspect), angle, keep aspect, Bicubic / Nearest, Reset | T2, T3, T16 |
| What moves | no selection: content bounds of pixels ∪ mask, both moved; selection: its bounds, `p·s` over `p·(1 − s)`, the selection follows (same resampling, 8-bit, clipped to the canvas); the mask stays when there is a selection (unobserved) | T27–T33 |
| Refusals | hidden, locked: silently (log); empty layer: notice "There is nothing to transform."; alpha lock ignored; clipping kept; off-canvas pixels kept | T34–T39 |
| Resampler | Keys cubic a = −1 premultiplied; pyramid ≤ ½; clamped sample; un-premultiplied by raw alpha; rounded axis-aligned edges; distance anti-aliasing when rotated; nearest neighbour ties down, hard edges | T16–T25 |
| Depth | the same `f32` arithmetic at both depths, quantized once at the document's depth | T26 (8-bit only in the reference) |
| Interruptions | another layer or tool (panel) asks "Apply the transform?" Yes / No / Cancel; tool keys do nothing; New/Open/Save/Save As/import/convert wait (export allowed); closing the window asks the same question first | T41–T43 |
| View | handles in screen space (constant size), box through the view transform | T44, T45 |
| Move tool | K: a drag moves the layer or the selected part, one step per drag (Mirae's: the reference's Move layer tool was not observed) | — |

## Part 3 — Mirae compared

### With the reference's measurements (tests, `document::transform::parity`, both depths)

| Check | Result |
|---|---|
| Content box without a selection (T2) | Equal (168..232 × 118..182) |
| Bicubic 200, 110, 75, 50, 45, 33, 30 % and 150 × 60 % (T17–T20) | Within **3/255**, at most 2 pixels over 2/255 per image (the reference stores straight colour at 8 bits, Mirae premultiplied: D-5D-5); rounded edges equal |
| Bicubic rotations 15°, 45°, −30°, 30° about a moved pivot (T21) | Interior within 2/255; 44–51 of ~3 300 pixels differ by more (max 25–31/255), all on the outermost, partly covered ring (D-5D-1) |
| Nearest neighbour 200 %, 30 %, 15° (T22) | **Equal**, every pixel |
| Flip horizontal (T15) | **Equal** |
| Alpha lock on, 150 % (T37) | Within 3/255 (3 pixels over 2) |
| Move through a hard selection (T30) | **Equal** |
| Move through a 10 px feathered selection (T31) | Within 3/255 (mean 0.3) |
| Layer with a mask, 150 % (T33) | Box equal (union); within 3/255 |
| Rectangle selection rotated 15° (T32) | Area equal (3 000 px²); every pixel within 2/255 except 2 per corner (≤ 23/255, D-5D-2) |
| Moved off the canvas and back (T39) | **Equal** (the original) |

### In the running app (2026-10-17, mouse and synthetic pen; screenshots not committed)

| # | Steps | Mirae | Reference |
|---|---|---|---|
| 1–2 | Three coloured strokes; rectangle selection over their crossing; Ctrl+T | Box on the selection's bounds, 8 square handles, round rotation handle, pivot cross; marching ants hidden; panel with W/H/angle/keep aspect/Bicubic/Nearest/Reset/OK/Cancel | T2, T28 |
| 3–6 | Drag inside; corner drag; drag outside | Moves (a hole where the selection was, unselected strokes stay); 132 %/132 % proportional about the opposite corner; rotation 18° about the pivot | T7, T8, T10, T30 |
| 7 | Escape | The document as before (screen identical); ants back | T5 |
| 8 | The same, Enter | Committed; the selection follows the rotated content; preview and commit identical on screen except the box/ants | T4, T32, TR4 |
| 9–10 | Ctrl+Z, Ctrl+Y | Back to the original exactly; the result exactly (screen captures equal apart from the ants) | T14 |
| 11–13 | Ellipse selection feathered 10 px, Ctrl+T, dragging the pivot; Flip H | The pivot moved, the content did not; Flip H opened a session at W −100 % with the soft-edged content mirrored | T9, T15, T31 |
| 14 | New empty layer, Ctrl+T | Notice "There is nothing to transform." | T36 |
| 15 | Green stroke on the new layer, clipped to Layer 1, transformed | Moves its own pixels, still shown only inside the base | T38 |
| 16 | Click Layer 1 during the session | "Apply the transform?" Yes / No / Cancel; Yes committed and Layer 1 became active | T41 |
| 17 | Mask on Layer 1, a stripe erased on it, Ctrl+T rotate | The box covers pixels and mask; the hidden stripe turns with the content | T33 |
| 18 | Zoom 2.3× and view rotated 30°, Ctrl+T, side handle | Box turned with the canvas, handles at their screen size; 103.4 %/103.4 % with keep aspect | T11, T44, T45 |
| 19 | To 16-bit; Move tool (K) dragged with the **synthetic pen** | The layer (with its mask) moved, one step | — |
| 20 | Save As `.mirae`, New, Open it | Reopened identical on screen (no session or selection kept) | — |
| 21 | Export PNG | The committed picture, 4000 × 3000 | — |
| 22 | Ctrl+T, drag, Ctrl+Z, Ctrl+S, Escape | The drag undone inside the session (screen identical to its start); Ctrl+S ignored, the title unmodified | T13, T43 |

## Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-5D-1 | **Rotated edge ring colour**: the outermost, partly covered pixels of a rotated box get a "purer" source colour in the reference (up to 24–31/255 on ~1.5 % of the pixels); Mirae samples the clamped position. Alpha matches (distance anti-aliasing). | Quality | Open: the reference's rule for that ring was not identified (supersampling, mirroring, renormalization and other edge models scored worse) |
| D-5D-2 | **Rotated selection corners**: two pixels per corner up to 23/255 apart (the product of the sides' coverages vs the reference's own corner rule; the minimum scored worse). | Quality | Open |
| D-5D-3 | **Interpolation modes**: the reference's bilinear ("Suavizar bordas", measured: the same pipeline with a bilinear kernel) and "Alta precisão (cores médias)" are not implemented; Mirae offers the default (bicubic) and nearest neighbour. | Feature | Accepted for 5D (the brief: the default's parity, no filters for completeness) |
| D-5D-4 | **Not implemented**: free transform's distortions (distort, skew, perspective, mesh, puppet), "keep original image", reference-point presets other than centre and a dragged pivot, "adjust position", the floating selection bar, transforming several layers or folders. | Feature | Out of 5D's scope |
| D-5D-5 | **Storage after a transform**: the reference stores straight colour at 8 bits, Mirae premultiplied (ADR 0001): up to 3/255 apart on a few pixels of a scaled image. | Quality | Deliberate |
| D-5D-6 | **Speed of large previews**: ~64 ns per destination pixel on one core: whole 2048² layers preview in 70–285 ms per change, 4096² in 0.27–1.1 s; selections of common size in milliseconds. The reference's speed was not measured. | Performance | Open (debt #40) |
| D-5D-7 | **A mask with a selection**: Mirae transforms only the pixels then; the reference's behaviour was not observed. | Behaviour | Open (not guessed) |
| D-5D-8 | **Closing the window during a session**: Mirae asks "Apply the transform?" first; the reference disables its quit command (T43). | Behaviour | Deliberate (no state is lost) |
| D-5D-9 | **Unmeasured corners of the resampler**: odd-sized pyramid levels (Mirae: beyond the box counts as transparent), anisotropic reductions (one axis ≤ ½: no pyramid in Mirae), 90° rotations. | Quality | Not compared |
| D-5D-10 | **Move tool**: Mirae's K tool (one transform per drag) is its own design; the reference's Move layer tool was not observed (could not be activated by automation). | Feature | Not compared |
| D-5D-11 | **Sub-steps in a History panel**: the reference lists a session's edits while it is open; Mirae has no History panel (D-4B-2) but undo/redo walk them the same way. | UI | As D-4B-2 |

Not observed: a real pen; the reference's performance and memory;
behaviour with several layers selected; Shift during rotation (angle
snapping).
