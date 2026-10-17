# Parity record 5E — fill

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440 at 100 %, 8-bit sRGB documents. Observed 2026-10-17, **before**
any 5E fill code was written (the probe tool, `fill_probe`, and the
measured data were the first 5E commit). Black box only: the reference
opened test documents **we wrote** (`fill_probe psd <kind>`, 400 × 300,
an opaque black layer, an opaque white layer above it and a layer "ink";
the `comp` document has line art spread over several layers) and was
operated with the mouse (SetCursorPos/mouse_event) and keyboard
(keybd_event). Results were read at 100 % zoom, where one screen pixel is
one document pixel (validated in 5C): over white to classify which pixels
the fill reached (fill colour magenta 255, 0, 255), and over white and
over black (the white layer hidden) to decode the ink layer's
premultiplied RGBA exactly (as in 5D). Candidate colour-distance models
were scored on every island (`fill_probe fit`). The tool settings and
History palettes were shown for the observations; tool settings changed
for the probes (close gap, area scaling, anti-aliasing, tolerance) were
set back with the sub-tools' own "reset to default" button; no other
setting was changed. Nothing was read from the reference's files.

Mirae: commits after `9e8474e`, same machine; mouse and the synthetic
Windows Ink pen (`inject_pen`). No physical tablet.

Measured data: `crates/document/tests/reference/fill/` (README there).

## Part 1 — the reference (observed)

### The tool and its settings

| # | Steps | Observation |
|---|---|---|
| BK1 | Fill tool ("Preencher"), sub-tools | **Referir outras camadas** (selected by default), **Referir apenas à camada de edição**, Envolver e preencher, Preencher laço, Caneta restante. |
| BK2 | Tool settings of "Referir apenas à camada de edição" (defaults) | Apply only to connected pixels **on**; close gap ("Fechar intervalo") **on**, level 2 of 5, with "fill narrow areas"; **tolerance ("Tolerância") 10.0** (0–100, one decimal); area scaling **off**; multiple reference **off**; opacity 100; blend mode Normal; anti-aliasing ("Suavização de serrilhado") **on**. |
| BK3 | Tool settings of "Referir outras camadas" (defaults) | The same, but area scaling **on** (shown "10") and multiple reference **on** with **All layers** (other choices: reference layer, selected layers, layers in a folder) and the exclusions **draft layers** and **paper layer** on (text, balloon, editing layer, locked layers off). |
| BK4 | G pressed twice from another tool | The first G selects the Gradient tool, the second the Fill tool: **G cycles gradient and fill**. |

### The region

Close gap off unless stated (the core rule); `conn` document.

| # | Steps | Observation |
|---|---|---|
| BK5 | Seed inside a closed 1-px square; inside a 1-px diamond whose pixels touch only diagonally | Only the inside fills, in both: the diagonal wall holds. |
| BK6 | Seed in the 4-px margin around a 1-px checkerboard inside a box | The margin and the checkerboard's transparent pixels that touch it by a side fill; the interior transparent cells (touching other cells only at corners) stay: **4-connected**. Also with anti-aliasing on. |
| BK7 | Boxes joined by a 1-px and a 3-px corridor | All three boxes fill through both corridors. With close gap **on** (default) the 1-px corridor and a 1-px gap in a wall are closed; a 5-px gap still leaks. |
| BK8 | A box holding a solid square with a hole | The box fills around the square; the hole does not. |
| BK9 | Region touching the canvas's right and bottom edges | Fills to the edges. A click **outside the canvas** fills nothing (and still records a step, BK33). |
| BK10 | Chain of 4-px stripes stepping by one grey level (128, 129, …, 225), reachable only through stripe 128, seed on grey 128 | At 1, 5, 10, 12, 20, 30 % the fill stops at grey 131, 141, 154, 159, 179, 205: every pixel is compared **with the seed's colour**, not with its neighbour (a neighbour rule would cross the whole chain). |

### Tolerance (the `tol` and `alpha` documents, anti-aliasing off)

`tol`: opaque grey 128 with 1 568 4 × 4 islands: each channel alone ±1…127,
alpha −1…−253, six channel combinations, 803 random colours (some
translucent); `alpha`: three zones seeded transparent, opaque black, opaque
white, with islands differing in alpha only, hidden colours under alpha 0,
low alphas of black, white and red.

| # | Steps | Observation |
|---|---|---|
| BK11 | Fills at 0, 0.1, 0.5, 1, 2, 4, 5, 10, 12, 20, 30, 49.9, 50, 60, 75, 100 % | The islands reached are exactly those whose **largest straight 8-bit channel difference** (R, G, B or A) from the seed is at most `T = round(t · 2.55)` (half up): 0 → 0, 0.1 → 0, 0.5 → 1, 1 → 3, 2 → 5, 4 → 10, 5 → 13, 10 → 26, 12 → 31, 20 → 51, 30 → 77. **0 misfits** on every capture. Euclidean RGB(A), sums, luma, weighted ("redmean"), HSV and CIE76 distances misfit 2–413 islands per capture; `ceil` misfits at 0.1, 2 and 4 %. |
| BK12 | Straight vs premultiplied | Straight: premultiplied colour misfits 16–24 islands at 20–30 % (`tol`) and 68 at 90 % (black zone of `alpha`, translucent white islands). |
| BK13 | Transparent seed; islands of alpha 0 holding hidden colours (red, white, grey, green, blue, 1/1/1) | Filled at every tolerance, even 0: **colour under alpha 0 does not count** (it is invisible). Low-alpha islands of black, white and red are reached when their alpha ≤ T, whatever their colour. Model "straight max difference; when either pixel has alpha 0, the alpha difference alone": **0 misfits on all 31 captures** (16 `tol`, 15 `alpha`). |
| BK14 | 100 % | Everything is reached **except pixels differing by a full 255** in a channel (an opaque pixel against a transparent seed, BK21): the effective threshold is `min(round(2.55 t), 254)`. |

### Anti-aliasing (the default)

| # | Steps | Observation |
|---|---|---|
| BK15 | `tol` at 10 % and 30 %, anti-aliasing on, decoded | Region pixels become the fill colour. Every **excluded pixel 4-adjacent to the region** receives the fill colour composed **over** it with coverage `c = (255 − d) / (255 − T)` (d its distance from the seed, T the threshold): the islands' edge and corner pixels match within **2/255** (1 290 islands at 10 %, 487 at 30 %, mean 0.002); island centres (not adjacent) are untouched. |
| BK16 | The checkerboard's isolated cells (distance 0, diagonal neighbours of the region) with anti-aliasing on | Untouched: the edge ring is 4-adjacent only. |
| BK17 | Anti-aliased ring of line art (alpha 128 at its inner edge), seed inside, AA on / off | Off: the 50 % edge pixel stays as it was, the inside becomes opaque fill colour. On: the edge pixel gets `c ≈ 0.55` of the fill over it (alpha 128 → 198), as BK15 predicts. |
| BK18 | "Referir outras camadas" with its defaults (close gap, area scaling) on a 1-px box drawn on another layer | The fill grows **one pixel under the line** (the line's pixel gets alpha 252): area scaling. Not part of the core rule. |

### What is written

| # | Steps | Observation |
|---|---|---|
| BK19 | Opaque and translucent pixels inside the region, opacity 100 | Become the opaque fill colour (the fill composed over them at full coverage). |
| BK20 | Layer opacity 50 %, fill, opacity back to 100 % | The filled pixels are opaque: **layer opacity is not baked**. |
| BK21 | Layer in Difference, fill, back to Normal | The filled pixels are the fill colour (shown as its difference over white while in Difference): **the blend mode is not baked**. |
| BK22 | A clipped layer as the target (its base empty there) | While clipped the fill shows only inside the base; after unclipping the whole region holds the fill: pixels are written normally, clipping only affects the display. |
| BK23 | Alpha lock ("bloquear pixels transparentes") on; tolerance 100 % and 10 % from a transparent seed | Included pixels are **recoloured keeping their alpha** (`premultiplied = fill · α`, exact); transparent ones stay transparent; excluded ones unchanged. |
| BK24 | Transparent colour selected, seed on an opaque block | The block's pixels become transparent: a fill with the transparent colour **erases** its region. |
| BK25 | The layer's **mask** as the target (mask hides everything except a revealed box outline), seed inside the box | Only the box's inside is revealed: on a mask the region is computed **on the mask's values**, and the fill reveals (the colour does not matter). |

### Selection

| # | Steps | Observation |
|---|---|---|
| BK26 | Two disjoint rectangle selections over one empty region, seed in one | **Only the clicked rectangle fills**: the selection limits the region's **connectivity** (pixels outside it stop the flood), not only what is written. |
| BK27 | Seed outside the selection | Nothing is filled (a step is still recorded, BK33). |
| BK28 | Rectangle selection feathered 10 px, seed inside, decoded | The fill's alpha equals the selection's coverage on every pixel (1, 3, 8, 16, 30, 51, 78, 110, 144, 176, 203, …), including pixels of coverage 1/255: every pixel with coverage > 0 belongs to the domain and the coverage multiplies the fill (no binarization). |

### Composed reference ("Referir outras camadas", close gap, area scaling and AA off)

`comp`: one closed box per layer above the white layer, an empty top layer
"fill" as the target.

| # | Box | Observation |
|---|---|---|
| BK29 | Normal black box; anti-aliased ring | Hold (the fill stays inside). |
| BK29b | Grey 200 at 30 % layer opacity (shown ~238 over white); black in Screen (invisible over white); black clipped to an empty base; black on a hidden layer; white on white | **Leak**: the reference is the **displayed composite** (opacity, blend mode, clipping and visibility applied), not the layers' raw pixels. |
| BK30 | The same with the black and white layers hidden and a reference **paper layer** added under the stack | The white box now **holds**, and so do the 30 % box and the Screen box (over transparency they are visible), while the clipped and hidden ones still leak: the composite is taken **without the paper** (the default exclusion, BK3) — the stack composed over transparency. |

### Commands, refusals, history, view

| # | Steps | Observation |
|---|---|---|
| BK31 | Fill on a **hidden** layer; on a **locked** layer | Nothing happens, no message, no step (title unchanged for the hidden one). |
| BK32 | Edit › Preencher (Alt+Del) **without a selection** (enabled) | Fills the **whole layer** with the colour; with alpha lock it recolours keeping alpha. With a selection: the coverage (5C S25). |
| BK33 | History palette | Each fill is one step "Preencher/Preencher/Referir apenas à camada de edição"; a click that changes nothing (outside the selection or the canvas) **is recorded too**. Undo restores the layer. |
| BK34 | 400 % zoom, a click on one isolated checkerboard pixel | Exactly that document pixel fills. |
| BK35 | View rotated 30°, a click inside the diamond | The diamond fills as unrotated: the seed is the document pixel under the cursor. |

Not observed: non-contiguous fill ("connected pixels" off), close gap and
area scaling in detail, the reference-layer/selected-layer reference
modes, "fill up to the vector path", the "Envolver e preencher" and
"Preencher laço" sub-tools, a mask target with the composed reference, a
translucent seed, opacity below 100 %, the reference's speed and memory, a
real pen.

## Part 2 — Mirae design from the observations (ADR 0026)

| Topic | Mirae | From |
|---|---|---|
| Operations | `Document::fill_connected` (bucket), `Document::fill_selection` (Edit › Fill: the selection, or the canvas without one), `Document::fill_region` (the region alone, read-only) | BK1, BK32 |
| Reference ≠ target | `FillReference::Target` (the layer's pixels, or the mask's values when the mask is the target) or `Composite` (the visible stack over transparency, without the paper, through the compositor); `FillTarget` = layer + pixels or mask | BK2, BK3, BK25, BK29, BK30 |
| Region | 4-connected flood from the seed; compared with the seed; largest straight channel difference, alpha alone where either alpha is 0; included ≤ `min(round(2.55 t), 254)` levels; domain = canvas ∩ selection coverage > 0 | BK5–BK14, BK26, BK27 |
| Anti-aliasing | the 4-adjacent excluded ring gets `(1 − d)/(1 − T)` | BK15–BK17 |
| Selection | its coverage multiplies the fill (no binarization) | BK28 |
| Paint | colour over (opaque at full coverage), source-atop with alpha lock, erase destination-out, masks reveal/hide; opacity, blend and clipping not baked | BK19–BK25 |
| Refusals | hidden, locked, missing or disabled mask: nothing, no step | BK31 |
| History | one step `Fill`; a fill that changes nothing records nothing (D-5E-4) | BK33 |
| Depth | distances on a 16-bit scale (8-bit content exact), each changed pixel quantized once at the document's depth | — (8-bit only in the reference) |
| Tool | G; a press fills at the document pixel under the cursor (any zoom and rotation); Alt still picks; eraser end / E erases; panel: Bucket, Fill selection / Fill layer, refer to Layer / Canvas, AA, tolerance 0–100 % in tenths; Alt+Delete; defaults canvas, 10 %, AA on | BK2–BK4, BK24, BK32, BK34, BK35 |

## Part 3 — Mirae compared

### With the reference's measurements (tests, `document::fill::parity`, both depths)

| Check | Result |
|---|---|
| Islands reached at 0, 0.1, 0.5, 1, 2, 4, 5, 10, 12, 20, 30, 49.9, 50, 60, 75, 100 % (1 568 islands each) and the one-level chain (BK10–BK14) | **16-bit: equal**, every island and the chain's end at every tolerance. **8-bit: equal except 2 islands at 30 %** (translucent colours exactly on the threshold, D-5E-1) |
| Islands of the three alpha zones at 0, 10, 30, 50, 90 % (hidden colours, low alphas, alpha-only differences; BK13) | **Equal** at both depths (1 005 islands × 5) |
| Anti-aliased edges at 10 % and 30 % (BK15), the anti-aliased ring with AA on and off (BK17) | Within **2/255** on every pixel (8-bit: outside the two D-5E-1 islands) |
| Fill through a 10 px feathered selection (BK28) | Within **2/255** (Mirae's feather, 5C) |
| Alpha lock at 100 % and 10 %, no lock at 100 %, Edit › Fill without a selection with and without alpha lock (BK14, BK23, BK32) | Within **1/255** |
| Diamond, checkerboard, corridors over white (BK5–BK7) | Within **2/255** (0 pixels over) |
| Composite reference: opacity, Screen, clipping to an empty base, hidden layer, white on white, paper excluded (BK29, BK30) | Same walls and leaks (unit test on an equivalent stack: `the_composite_reference_is_the_visible_stack_without_paper`) |
| Mask target (BK25), refusals (BK31), opacity/blend/clipping not baked (BK20–BK22), selection limits connectivity (BK26, BK27) | Same behaviour (unit tests) |

### In the running app (2026-10-17, mouse and synthetic pen; screenshots not committed)

| # | Steps | Mirae | Reference |
|---|---|---|---|
| 1–3 | Closed and open contours (ink pen), G, red, a click inside each | The closed box fills (18 tiles, 12.5 ms); the open one leaks to the whole 4000 × 3000 canvas (186 tiles, 61 ms) | BK5, BK7 |
| 13 | Undo, redo, undo × 2 | Each fill back and forth, exactly as before on screen | BK33 |
| 6–7 | New empty layer; Canvas: a click between the contours; Layer: a click inside the box | Canvas: fills around the lines of the layer below (lines stay visible); Layer: the empty layer is one region, the box and its lines are covered | BK29, BK2 |
| — | A second fill over a filled area | Grows into its own anti-aliased edge (those pixels are within the tolerance of the fill colour) | BK15 (the rule) |
| 8 | Rectangle selection over the open contour's lower half, a click inside it | Fills only inside the selection; the contour's inside, reachable only through the unselected opening, stays empty; a click outside the selection: nothing, no step | BK26, BK27 |
| 9 | The selection feathered 40 px, the same click | Soft-edged fill | BK28 |
| 10 | Lock alpha on the line layer, Fill layer (panel) | The lines recoloured, alpha kept, nothing elsewhere | BK23, BK32 |
| 11 | Layer 2 filled red around the lines; Add mask; eraser mode (E) + bucket on the mask | The red region hidden on the mask (the canvas white again) | BK25, BK24 |
| 12 | Layer 3 clipped to layer 2, Layer reference, a full-layer fill in blue | Blue only where layer 2 shows | BK22 |
| 14–15 | Zoomed in and the view rotated 30°, a click inside the box | The box's inside fills (18 tiles, 16 ms) | BK34, BK35 |
| pen | The synthetic Windows Ink pen pressed on the canvas (orange, layer 3) | Fills like the mouse (186 tiles, 195 ms; the title shows "pen") | — |
| 16 | To 16-bit, a fill in cyan | Fills (186 tiles, 205 ms) | — |
| 17–18 | Save As `.mirae`, New, Open; Export PNG | Reopened identical on screen; the 4000 × 3000 PNG shows the same picture | — |
| 4–5 | New document, a box, zoomed on its edge: tolerance 0 % and 90 % (typed in the panel) | 0 %: the fill stops at the line's inner anti-aliased edge; 90 %: it crosses where the stroke's tapered ends meet (that junction is within 90 % of transparent) | BK11 |
| keys | Alt+Delete with a selection | Fills the selection (after a fix: synthetic key events arrive as the numpad Delete; the shortcut now matches the logical key) | BK32 |

## Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-5E-1 | **8-bit premultiplied storage at the threshold**: Mirae compares straight colour recovered from premultiplied 8-bit storage; for translucent pixels it can differ from the reference's straight value by a fraction of a level (at alpha 186, straight 205 and 206 store the same value). 2 of the 25 088 `tol` island classifications differ (30 %, colours exactly 77 levels away); none at 16 bits. | Quality | Deliberate (premultiplied storage, ADR 0001; as D-5D-5) |
| D-5E-2 | **Gap closing and area scaling not implemented**: the reference's "close gap" (on by default in both sub-tools: closes 1-px gaps and corridors, BK7) and "area scaling" (on by default when referring to other layers: the fill grows one pixel under the line, BK18). Mirae's fill leaks through 1-px gaps and stops at the line's anti-aliased edge. | Feature | Out of 5E's scope (owner's brief) |
| D-5E-3 | **Other fill options and sub-tools**: non-contiguous fill, the reference-layer / selected-layers / folder modes and their exclusions (draft, text, balloon, locked layers), "fill up to the vector path", "do not fill this colour", the fill's own opacity and blend mode, "Envolver e preencher", "Preencher laço", "Caneta restante", Edit › Advanced fill. | Feature | Not implemented |
| D-5E-4 | **A fill that changes nothing**: the reference records a history step (even a click outside the canvas or the selection, BK33); Mirae records nothing (rule H6). | Behaviour | Deliberate |
| D-5E-5 | **Erasing with the bucket**: the reference fills with its transparent colour (BK24); Mirae has no transparent colour (debt #36) and erases with the stylus eraser end or eraser mode (E). | UI | Mirae's mapping |
| D-5E-6 | **G**: the reference cycles gradient and fill (BK4); Mirae has no gradient tool, G selects the fill. | UI | As the gradient tool is missing |
| D-5E-7 | **Speed**: Mirae fills synchronously on one core: small regions in milliseconds, a whole 4096² background around line art through the composite in ~270 ms; the reference's speed was not measured. | Performance | Not compared |
| D-5E-8 | **Keyboard focus**: after typing in a panel field, egui keeps focus and Alt+Delete (like other shortcuts) goes to the field until something else is clicked (D-5C-7). | UI | Open (UI polish) |
| D-5E-9 | **Unobserved corners**: a mask target with the composed reference, the transparent colour on an alpha-locked layer (Mirae erases, as the eraser), a translucent seed, the fill tool below 100 % opacity. | Behaviour | Not compared |

Not observed: a real pen; the reference's performance and memory; fills
in 16-bit documents (the reference has none).
