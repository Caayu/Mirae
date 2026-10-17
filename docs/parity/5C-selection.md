# Parity record 5C — selections

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440 at 100 %, 8-bit sRGB documents. Observed 2026-10-17, **before**
any 5C selection code was written (the probe, `selection_probe`, and the
measured images were the first 5C commit). Black box only: the reference
opened one test document **we wrote** (`selection_probe psd sel.psd`: an
opaque white layer "paper" under an empty layer "ink", 400 × 300) and was
operated with the mouse (SetCursorPos/mouse_event) and keyboard
(keybd_event). A selection's coverage was made visible by filling the ink
layer through it with black (Editar › Preencher, Alt+Del) and reading the
canvas at 100 % zoom, where one screen pixel is one document pixel
(validated: a screen read and the reference's own PNG export of the same
state are bit-identical); coverage = `255 − red`. Its History palette,
menus, tool settings and selection outline were read from the screen.
Nothing was read from its files. Two palettes (tool settings, History)
were shown for the observations and hidden again; the anti-aliasing and
selection-mode buttons of the selection sub-tools were clicked and set
back to their defaults; no other setting was changed.

Mirae: commits after `18e1c2e`, same machine; mouse and the synthetic
Windows Ink pen (`inject_pen`). No physical tablet.

Measured images (canvas captures, our own document):
`crates/document/tests/reference/selection/` (see its README).

## Part 1 — the reference (observed)

### Commands, tools, shortcuts

| # | Steps | Observation |
|---|---|---|
| S1 | Selecionar menu | **Selecionar tudo** Ctrl+A, **Cancelar seleção** (deselect) Ctrl+D, Selecionar novamente (reselect) Ctrl+Shift+D, **Inverter área selecionada** Ctrl+Shift+I, Expandir/Encolher área selecionada, **Desfocar borda** (blur the border: the feather), Selecionar gama de cores, Máscara rápida, Converter para camada de seleção. With no selection everything but Select All (and the range/quick-mask/convert items) is disabled. |
| S2 | Tool bar | "Área de seleção" tool, shortcut **M**, sub-tools Laço (lasso), Retângulo, Elipse, Polilinha, Caneta de seleção, Apagar seleção, Encolher seleção, Laço magnético. |
| S3 | Tool settings of Retângulo / Elipse / Laço | "Modo de seleção": **New, Add, Subtract, Intersect** buttons. "Suavização de serrilhado" (anti-aliasing): None / Weak / Medium / Strong. Defaults: **rectangle None, ellipse Medium, lasso Medium**. Rectangle and ellipse also have aspect and angle options (not used). |
| S4 | Rectangle drag with modifiers, a selection present | **Shift held at the press: Add**; **Alt: Subtract**; **Shift+Alt: Intersect**; no modifier: the tool's mode (New = replace). **Shift during the drag** constrains to a square (the smaller side). Alt does not draw from the centre. |
| S5 | A click without a drag, rectangle tool, mode New, selection present | The selection is **removed** (an empty replacement). |
| S6 | Ctrl+Shift+I sent with keybd_event | No effect in this session; the menu item works (the shortcut is listed in the menu). Not investigated further. |

### Shapes

| # | Steps | Observation |
|---|---|---|
| S7 | Rectangle (AA None), mouse from document (50, 40) to (150, 120) at 100 % | Exactly `[50, 150) × [40, 120)`: 8 000 full pixels, no partial pixel. |
| S8 | Rectangle at 400 % zoom from (50.25, 40.25) to (150.5, 120.75), AA None and AA Strong | Both `[50, 150) × [40, 121)`, **no partial pixel even with AA Strong**: an unrotated rectangle snaps to whole pixels (corners rounded to the nearest pixel corner). |
| S9 | Rectangle in a view rotated 30° | The rectangle follows the **screen** axes: a rotated rectangle in the document (hard-edged with AA None). The ellipse too (S10). |
| S10 | Ellipse in the box (60, 50)–(181, 131), AA None, Weak, Medium, Strong | Box exact (bounds 60..181 × 50..131). **Weak, Medium and Strong are bit-identical**: one-pixel anti-aliased band (341 partial pixels); area 7 668.7 px against π·60.5·40.5 = 7 697.7. AA None = the anti-aliased result thresholded at 128 (7 673 full pixels). The edge values are not those of an exact-area or supersampled ellipse (best fit: radii 0.1 px smaller, single pixels still up to 57/255 off; the reference evidently flattens the curve its own way). |
| S11 | Lasso pentagram (self-intersecting), AA Medium | The centre pentagon is **not selected**: **even-odd** fill. The path is closed on release. Sharp input corners come out slightly rounded (the reference smooths the lasso path). |
| S12 | Rectangle dragged from outside the canvas (−40, −30) to (100, 90) | Allowed; the selection is **clipped to the canvas**: exactly `[0, 100) × [0, 90)`; the outline is drawn only inside the canvas. |
| S13 | Selection while the active layer is hidden; then another layer selected | Selecting works on a hidden layer; the selection **stays** when the active layer changes: it belongs to the document, not to a layer. |

### Combining (partial coverage)

A = rectangle (100, 80)–(300, 220) feathered 50 px (partial almost
everywhere), B = anti-aliased ellipse (150, 100)–(271, 181) whose edge
band lies inside A's gradient; A, B and each combination measured;
`selection_probe fit-op` scores each rule on the 286 pixels where both
are partial.

| # | Operation | Rules scored (max error /255) | Observation |
|---|---|---|---|
| S14 | Add | max(a, b) 22 · a + b − ab 17 · **min(1, a + b) 0** | **Saturating sum**, exact on every pixel. |
| S15 | Subtract | a(1 − b) 17 · min(a, 1 − b) 22 · **max(0, a − b) 0** | **Saturating difference**, exact. |
| S16 | Intersect | min(a, b) 17 · max(0, a + b − 1) 16 · ab 1 · **⌊ab/255⌋ 0** (8-bit values) | **Product, truncated** to 8 bits, exact. |
| S17 | Invert (Select All, then invert) | The result has no coverage and becomes **no selection** (Deselect, Invert, Expand disabled). |
| S18 | Subtract a rectangle covering the whole selection | Also **no selection**. An empty selection never exists. |

### Feather ("Desfocar borda")

Dialog "Força" in px with two decimals; applied **immediately** to the
current selection (the outline moves to the 50 % contour, corners
rounded); one history step.

| # | Steps | Observation |
|---|---|---|
| S19 | Rectangle (100, 80)–(300, 220) blurred by 1, 3, 10, 20, 50 | A **Gaussian** blur of the coverage: a continuous Gaussian fits with σ = 0.19, 0.87, 2.97, 6.04, 15.08 (max 2/255 from 3 px up); a **sampled, normalized discrete Gaussian with σ = 0.3 × strength and radius ⌈3σ⌉** reproduces **every strength within 2/255** (`fit-kernel`). Profile symmetric about the old edge (v(99) + v(100) = 254). |
| S20 | Rectangle crossing the canvas edge (−5, −5)–(100, 90), blur 20 | 255 up to the canvas edge: the selection is clipped (an unclipped one would give ≈ 209 at x = 0). |
| S21 | Rectangle (−5, 40)–(10, 90), blur 20, row 65 | x = 0 → 228, x = 9 → 128: the kernel is **renormalized over the canvas** (pixels outside the canvas do not take part); edge replication would give 240 / 136. |

### Editing through a selection

Selection A (above); a hard 10 px G-pen (or the 30 px hard eraser)
scrubbed densely back and forth over A's gradient **inside one stroke**;
compared on 13 000 pixels (`selection_probe rule`).

| # | Steps | Observation |
|---|---|---|
| S22 | One dense pen stroke | Result = coverage **exactly** (0 error): the selection **multiplies the stroke's alpha** (`m·s`), it does not limit each dab (that would build up above `s`). |
| S23 | Two such strokes | `s + s(1 − s)` exactly: each stroke `m·s` composed over the previous. |
| S24 | Eraser stroke over a black layer | `1 − s` (only the eraser's on-screen cursor differs): erase is `dst·(1 − m·s)`. |
| S25 | Fill (Alt+Del) | Coverage itself (the measuring method); consistent with S14–S16, whose predictions used it. |
| S26 | Layer mask as the target, pen stroke revealing through A | Revealed amount = `s` exactly: mask strokes are limited the same way. |
| S27 | Ctrl+D pressed in the middle of a pen stroke | **Ignored**: the selection stays and the **whole stroke** is clipped by it. |

### History and files

| # | Steps | Observation |
|---|---|---|
| S28 | History palette | Every selection change is a **step of the document history**: "Área de seleção/…/Retângulo" (or Elipse; also with Add/Subtract/Intersect), "Cancelar seleção", "Selecionar tudo", "Inverter área selecionada", "Desfocar borda". |
| S29 | Undo after a fill; undo again; redo | The fill goes, the selection stays; the next undo removes the selection; redo brings it back. Undo of Add and of Subtract restores the previous selection. |
| S32 | A fresh document (title without `*`), a rectangle selection; then undo | The title gets the **modified** marker `*` from the selection alone; undoing it leaves the `*` (as in 4F: the reference never clears it by undo). |
| S30 | Save as CLIP STUDIO format with a selection, close, reopen | **No selection** after reopening (Reselect disabled too): the reference does **not persist** the active selection, even in its own format. History is not kept either. |

### Display

| # | Observation |
|---|---|
| S31 | Marching ants: a dashed black/white line of constant screen width at every zoom, along the 50 % contour (a feathered rectangle's outline sits on its old edge with rounded corners). A floating "selection launcher" bar appears under the selection. No shading of the unselected area by default. |

S6 explained after the fact: on this machine **Ctrl+Shift+I is registered
as a global hotkey by another program** (`RegisterHotKey` fails with
error 1409, as for Ctrl+Shift+S, AI_HANDOFF §9), so it reaches neither
application; the menu item and Mirae's Invert button work.

## Part 2 — Mirae design from the observations (ADR 0024)

| Topic | Mirae | From |
|---|---|---|
| Model | `Document::selection() -> Option<&Selection>`: 8-bit coverage, sparse `coverage XOR fill`, clipped to the canvas; never empty | S12, S13, S17, S18 |
| Shapes | Rectangle (snaps to pixels unless the view is rotated), ellipse, freehand lasso; one even-odd polygon filler, 16 sub-scanlines when anti-aliased; screen points mapped through the view once | S7–S12 |
| Anti-aliasing | On/off per shape; defaults rectangle off, ellipse and lasso on | S3, S10 |
| Operators | New / Add / Subtract / Intersect; Shift, Alt, Shift+Alt at the press; Shift during the drag squares; a click in New deselects | S4, S5, S14–S16 |
| Commands | Select all (Ctrl+A), Deselect (Ctrl+D), Invert (Ctrl+Shift+I and a button), Feather (strength in px, 0.1–1000) | S1, S19 |
| Feather | Discrete Gaussian σ = 0.3·px, radius ⌈3σ⌉, renormalized over the canvas | S19–S21 |
| Editing | Stroke alpha × coverage (`m·s`) in the shared preview/commit rule; paint, alpha lock, erase, masks | S22–S26 |
| During a stroke | Selection commands and undo ignored | S27 |
| History | Every change one step, swap-based; a modification (title `*`) | S28, S29, S32 |
| Files | Not saved; `.mirae` v1 unchanged | S30 |
| Display | Marching ants on the 50 % contour, white line + moving black dashes, constant screen width; crosshair cursor for the selection tool | S31 |
| Tool keys | M selection tool; B brush (Mirae's) | S2 |

## Part 3 — Mirae compared

### With the reference's measurements (tests, `document::selection::parity`)

| Check | Result |
|---|---|
| Rectangle (S7), 400 % sub-pixel drag with AA Strong (S8) | **Equal**, every pixel |
| Rectangle in a 30° view (S9) | Same behaviour (screen-aligned, a rotated rectangle in the document); pixels not compared (the reference's drag positions in a rotated view were not recorded precisely) |
| Ellipse, AA (S10) | Area within 0.4 % (7 694 vs 7 669 px); interior and exterior equal; 345 edge pixels differ by more than 2, at most 68/255 (D-5C-1) |
| Ellipse, hard (S10) | ≤ 40 pixels differ (the reference thresholds its own flattened curve) |
| Lasso pentagram (S11) | Even-odd centre equal; 185 of 7 691 covered pixels differ, at the tips the reference rounds (D-5C-2) |
| Add, Subtract, Intersect on the reference's own A and B (S14–S16) | **Equal**, every pixel (0/255) |
| Feather 1, 3, 10, 20, 50 px (S19); edge renormalization (S21) | **Within 2/255** everywhere |
| One and two dense strokes through A, a mask stroke (S22, S23, S26) | **Equal** (0/255; two strokes ≤ 1/255) |
| Erase through A (S24) | `1 − s` within 1/255 |

### In the running app (2026-10-17, mouse and synthetic pen; screenshots not committed)

| # | Steps | Mirae | Reference |
|---|---|---|---|
| 1–3 | Rectangle, brush stroke across its border, Ctrl+D | Stroke only inside; preview as committed; ants gone | S22 |
| 4–7 | Ellipse; Shift-drag ellipse (Add, squared to a circle); Alt-drag rectangle (Subtract); Shift+Alt rectangle (Intersect) | Union, notch, intersection as expected; one outline each | S4, S14–S16 |
| 8 | Lasso, concave 7-point path | Filled polygon | S11 |
| 9 | Feather 30 px, stroke across | Rounded outline on the 50 % contour; the stroke fades at the edge | S19, S22 |
| 10 | Invert (button; Ctrl+Shift+I held by another program) | Canvas border + inner loop | S17 |
| 11 | Eraser across lines in and out of the inverted selection | Erased only outside the lasso | S24 |
| 12 | Mask added, eraser on the mask along a line crossing the feathered lasso | Hidden only where selected, soft at the edge | S26 |
| 13 | New layer | Selection unchanged | S13 |
| 14 | Undo × 3, redo × 3 (mask stroke, layer, selection steps) | Each step back and forth | S29 |
| 15 | Zoom in (wheel), rectangle | Correct document rectangle | — |
| 16 | View rotated 30° (Shift+wheel), rectangle and Shift-ellipse; fit view | Rectangle and circle follow the screen; a rotated rectangle in the document | S9 |
| 17 | Convert to 16-bit, stroke through the rotated selection | Clipped to it | — |
| 18 | Save As `.mirae`, reopen | Content restored, **no selection** | S30 |
| pen | Rectangle with the synthetic pen, pen stroke through it | Same as the mouse | — |
| S27 | Ctrl+D in the middle of a mouse stroke | Ignored: selection kept, whole stroke clipped | S27 |
| keys | Ctrl+A, Ctrl+D | Select all (border ants), deselect | S1 |

## Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-5C-1 | **Ellipse edge pixels**: the reference flattens its ellipse its own way; Mirae's edge is the exact-area polygon of the analytic ellipse. Up to 68/255 on single edge pixels, area within 0.4 %, hard ellipses ≤ 40 pixels apart. | Quality | Accepted (not reproducible without guessing the reference's internals) |
| D-5C-2 | **Lasso smoothing**: the reference rounds sharp turns of a freehand path; Mirae keeps the path as drawn (every point ≥ 0.5 screen px apart). | Quality | Open — a stroke-smoothing option for the lasso could reuse the stabilizer later |
| D-5C-3 | **Anti-aliasing levels**: the reference offers None/Weak/Medium/Strong (bit-identical at 100 % in our probe); Mirae has on/off. | Feature | Accepted unless a difference is measured |
| D-5C-4 | **Not implemented sub-tools and commands**: polyline, selection pen/eraser, shrink selection, magnetic lasso, reselect (Ctrl+Shift+D), expand/shrink, colour range, quick mask, convert to selection layer, selection launcher bar, hiding the outline. | Feature | Out of 5C scope (owner's brief); later subphases |
| D-5C-5 | **Modified marker after undo**: undoing a selection back to the saved state is clean in Mirae (D-4F-2, deliberate); the reference keeps `*` (S32). | Behaviour | Deliberate (as D-4F-2) |
| D-5C-6 | **Off-canvas pixels**: without a selection Mirae's brush still stores pixels beyond the canvas (debt #14); with any selection it does not (the selection is clipped). Invisible; it is why SEL3 is stated on the canvas. | Behaviour | Open with debt #14 |
| D-5C-7 | **Text-field focus**: after typing in a panel field (feather strength), egui keeps keyboard focus and shortcuts reach the field until something else is clicked. | UI | Open (UI polish) |
| D-5C-8 | **Feather speed**: Mirae feathers a 3 800 px ellipse by 50 px in ~0.2 s on one core; the reference's speed was not measured. | Performance | Not compared |

Not observed: selection with a real pen; the reference's behaviour on a
selection change during a stroke with a pen; feather above 50 px; the
reference's anti-aliasing levels at other zooms; performance and memory
of the reference.
