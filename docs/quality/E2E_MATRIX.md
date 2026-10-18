# Desktop E2E matrix

What the desktop end-to-end suite (`crates/e2e/tests/`, layer L3/L4 of
[QUALITY_GATE.md](QUALITY_GATE.md)) covers, per feature, and what it does
not. Every row runs the real `tela.exe` with OS input and checks the app's
state, the document's pixels (queries, digests, exported files) and, where
said, the screen. States as in the gate: **automated pass**, **manually
verified**, **not tested**, **failed**, **blocked**.

The latest full run and its result are recorded in
[STATUS.md](../STATUS.md#desktop-e2e); run it with
`powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1`.

Test files: `smoke.rs`, `gradient.rs`, `ui_layout.rs`, `files_history.rs`,
`paint_color_layers.rs`, `select_transform_fill.rs`, `scenarios.rs`,
`performance.rs`.

## Canvas

| Item | Test | State |
|---|---|---|
| New document (8 / 16-bit, from the New menu) | `files_history::save_reopen…`, `scenarios::c_…` | automated pass |
| Zoom (wheel, about the cursor) | `paint_color_layers::canvas_view…` | automated pass |
| Pan (middle button; Space + drag) | `canvas_view…`, `gradient::zoomed_rotated…` | automated pass |
| Rotation (Shift + wheel) | `canvas_view…`, `gradient::zoomed_rotated…` | automated pass |
| Home fits the whole document | `canvas_view…` | automated pass |
| Painting at the document's edges | `canvas_view…` | automated pass |
| Large canvas | The default 4000×3000 (12 Mpx) in every test; larger sizes cannot be made from the UI | automated pass (4000×3000 only) |
| Window resize and maximize | `ui_layout::essential_controls…`, `performance` | automated pass |
| Display scaling other than 100 % | — | not tested |

## Brush

| Item | Test | State |
|---|---|---|
| Every built-in preset paints | `paint_color_layers::brush_presets…` | automated pass |
| Size, opacity fields | `brush_presets…` | automated pass |
| Flow, spacing, hardness, angle, aspect, scatter, texture fields | — (L1/L2 and goldens only) | not tested (E2E) |
| Synthetic pen with pressure; pen eraser end | `brush_presets…` | automated pass (synthetic pen only) |
| Stabilization, stabilizer, interpolation | — (L1/L2) | not tested (E2E) |
| Airbrush | the Airbrush preset paints in `brush_presets…` | automated pass (presence only) |
| Eraser (strip, E), B returns to the brush | `brush_presets…`, `ui_layout::the_active_tool…` | automated pass |
| Colours in strokes | `colours…`, scenarios A–C | automated pass |
| Preview / commit | Every stroke checked after commit; the preview path is covered by GR5/P2 tests (L1) | automated pass (commit), not tested on screen during a stroke |

## Colour

| Item | Test | State |
|---|---|---|
| Main / sub, X, Swap, Reset | `colours…` | automated pass |
| RGB, HSV, hex fields | `colours…` | automated pass |
| Eyedropper: tool, right-click, Alt | `colours…` | automated pass |
| Recent colours | `colours…` | automated pass |
| Swatches: add | `colours…` (count only) | automated pass (weak) |
| Swatches: pick, remove (right-click) | — | not tested |

## Layers

| Item | Test | State |
|---|---|---|
| Create, select, delete (refused while locked) | `layers_create…` | automated pass |
| Rename (double-click, type, Enter) | `layers_create…` | automated pass |
| Up / Down; drag-and-drop reorder | `layers_create…` | automated pass |
| Visibility | `visibility_toggle_and_the_pen…` | automated pass |
| Lock | `layers_create…` | automated pass |
| Opacity, blend mode | `layers_create…`, `files_history…`, `masks…` | automated pass |
| Alpha lock | `select_transform_fill::fill…`, `gradient::gradients_with…`, scenario B | automated pass |
| Merge down (with the lower-mask question) | `layers_create…`, scenario B | automated pass |
| Undo / redo of layer steps | `layers_create…` | automated pass |

## Masks and clipping

| Item | Test | State |
|---|---|---|
| Add mask (becomes the edit target) | `masks_and_clipping…` | automated pass |
| Paint / erase the mask | `masks…`, scenario A, C | automated pass |
| Enable / disable | `masks…` | automated pass |
| Delete mask | — | not tested |
| Clipping; several clipped layers; with Multiply | `masks…`, scenario A | automated pass |

## Selections

| Item | Test | State |
|---|---|---|
| Rectangle, ellipse, lasso | `selections_shapes…` | automated pass |
| Add (mode button), Subtract (Alt), Intersect (Shift+Alt) | `selections_shapes…` | automated pass |
| Select All, Deselect, Invert | `selections_shapes…` | automated pass |
| Feather | `selections_shapes…`, `gradient::gradients_with…` | automated pass |
| Painting and filling limited by the selection | `selections_shapes…` | automated pass |

## Transform

| Item | Test | State |
|---|---|---|
| Move tool | `move_flip_scale…` | automated pass |
| Scale (W field), rotate (Angle field), flip | `move_flip_scale…`, scenario B | automated pass |
| Handles dragged on the canvas, pivot | — | not tested |
| Preview / Apply / Cancel / Escape / Enter | `move_flip_scale…`, `performance` | automated pass |
| Another tool during a session asks first | `move_flip_scale…`, scenario D | automated pass |
| Zoomed and rotated views | — | not tested (E2E) |

## Fill

| Item | Test | State |
|---|---|---|
| Bucket, reference Canvas / Layer | `fill_bucket…` | automated pass |
| Tolerance field | `fill_bucket…` (the field only) | automated pass (weak) |
| Selection, alpha lock, mask (Erase mode) | `fill_bucket…` | automated pass |
| Draw / Erase mode not inherited from the eraser | `gradient::the_eraser_does_not…` | automated pass |
| Edit › Fill (Alt+Delete, the panel button) | `select_transform_fill`, scenarios | automated pass |
| Undo / redo | `fill_bucket…` | automated pass |

## Gradient

| Item | Test | State |
|---|---|---|
| The tool; G cycles fill and gradient; the strip marks it | `gradient::g_cycles…` | automated pass |
| Linear, radial, ellipse, angular, reflected | `every_shape_and_end…` (oracle, preview and screen) | automated pass |
| Ends: extend, repeat, mirror, none | `every_shape_and_end…` | automated pass |
| Colours; add, remove, reverse stops; fixed colour; position | `stops_edited…` | automated pass |
| Dragging a stop marker | — | not tested |
| Preview while dragging; release applies | `every_shape_and_end…`, `performance` | automated pass |
| Escape cancels; undo; redo; repeating | `escape_cancels…` | automated pass |
| Focus lost mid-drag | `losing_the_focus…` | automated pass |
| Selection, feathered selection, mask, alpha lock, opacity, blend, clipping | `gradients_with_selection…` | automated pass |
| 8-bit and 16-bit | `sixteen_bit…`, scenario C | automated pass |
| Zoom, rotation, pan | `zoomed_rotated…` | automated pass |
| Save, reopen, export | scenarios A, C; `files_history` | automated pass |
| The eraser does not turn it into an eraser; Erase mode | `the_eraser_does_not…` | automated pass |

## Files

| Item | Test | State |
|---|---|---|
| New, Open, Save, Save As (native dialogs) | `files_history…`, scenarios B, C | automated pass |
| Import PNG | `import_png…`, scenario B | automated pass |
| Export PNG 8 and 16 bits (pixels equal the composite) | `save_reopen_and_export…`, scenarios A, C | automated pass |
| Transparent export | — | not tested (E2E) |
| Unsaved-changes question (Cancel, Save, Don't Save; New and Close) | `unsaved_changes…` | automated pass |
| Hostile file refused, document kept | `a_hostile_file…` | automated pass |
| Drag and drop of files | — | blocked (OLE drag and drop from another process is not automated) |

## History

| Item | Test | State |
|---|---|---|
| Undo / redo in every family above | per family | automated pass |
| Redo invalidated by a new action | `history_save_point…` | automated pass |
| Merged steps (consecutive opacity edits) | `history_save_point…` | automated pass |
| Modified state, return to the save point | `unsaved_changes…` | automated pass |

## Workflows

| Scenario | Test | State |
|---|---|---|
| A — complete illustration | `scenarios::a_complete_illustration` | automated pass |
| B — complex editing | `scenarios::b_complex_editing` | automated pass |
| C — 16-bit document | `scenarios::c_sixteen_bit_document` | automated pass |
| D — interruptions | `scenarios::d_interruptions` | automated pass |
| E — long session | `scenarios::e_long_session` | automated pass |

## Interface (L4)

See [QUALITY_GATE.md](QUALITY_GATE.md#interface-gate-automated-ui_layoutrs):
reachability at five window sizes, overlaps, browsing without edits,
stability, the active tool, focus, panel clicks after moving the window
(`ui_layout.rs`), and the review screenshots (`audit_screenshots`).
