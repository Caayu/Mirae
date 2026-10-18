# UI boundary

The interface is organised around the canvas with a small design system
(ADR 0029; the audit that led to it: [../quality/UI_AUDIT.md](../quality/UI_AUDIT.md)).
What matters most is still where it sits in the architecture.

## Layout

```
┌──────────────────────── top bar: name •  New Open Save Save As │ Import Export │ Undo Redo ─┐
│ tool   │ tool options (scrolls)      │                         │ Document (size, depth,   │
│ strip  │  Colour (collapsible)       │         canvas          │   export options)        │
│ Brush  │  <active tool name>         │                         │ Layers: commands,        │
│ Eraser │  that tool's options only    │                         │   blend, opacity, clip,  │
│ …      │                             │                         │   mask, rows (scrolls)   │
└────────┴─────────────────────────────┴─────────────────────────┴──────────────────────────┘
```

- The **tool strip** (`toolbar::tool_strip`) returns a `ToolChoice`; the app
  maps it to `CanvasTool` and eraser mode (or opens a transform). The
  active tool is the one button filled with the accent colour.
- The **tool options** show the Colour section, then only the active
  tool's panel (brush, selection, fill, gradient, transform); the panel
  scrolls, so every control is reachable at 800×600.
- The **top bar** (`toolbar::top_bar`) holds file commands and undo/redo;
  the document name has a fixed width and the unsaved dot a fixed place.
- The **right panel** holds the Document section (`document_panel`) and
  the layer panel.
- Panel widths are fixed (`style::{TOOL_STRIP_WIDTH, OPTIONS_WIDTH,
  LAYERS_WIDTH}`); the canvas fits the area they leave (`canvas_area` in
  the app) and keeps its place when the window resizes.

## Design system

`tela_ui::style` is the only place panels take sizes and colours from:

| Token | Values |
|---|---|
| Spacing (`space`) | XS 2, S 4, M 8, L 12, XL 16 pt |
| Controls | height 22 pt; label column 76 pt; tool buttons 30 pt high; corners 4 pt |
| Type (`text`) | body 13, small 11, heading 15, section 11 pt |
| Colours (`color`) | panel `#232428`, chrome `#1C1D21`, surface `#303238` / hover `#3A3D45` / active `#454952`, field `#18191C`, border `#3B3E46`, text `#E4E5E8`, muted `#9CA0A8`, disabled `#666A72`, accent `#4F7FE0` (selection, active tool, focus), warning `#E0A43C` (mask target, Erase modes), danger `#E5534B` |

Pieces: `section` (a muted small-capitals heading), `row` (a labelled
row with the fixed label column; labels truncate), `segmented` (one
choice among a few, selected = accent), `tool_button`, `draw_or_erase`
(the Draw / Erase mode with its warning line), `hint` (muted small text),
`panel_frame`. States: disabled controls are greyed by egui with the
theme's disabled colour; hover lightens the surface; active (pressed)
adds an accent outline; selected uses the accent fill. The theme is
installed once (`style::apply`) when the `UiLayer` is created.

## Rules

1. **egui lives only in `tela-ui`.** No other crate imports egui; the
   boundary test (`crates/app/tests/crate_boundaries.rs`) enforces it.
2. **The UI owns no authoritative state.** Everything it edits is lent to
   it for one frame through `PanelModel`:

   ```rust
   pub struct PanelModel<'a> {
       pub preset_names: &'a [String],
       pub selected: usize,
       pub eraser_mode: bool,
       pub preset: &'a mut BrushPreset,       // owned by App.tools
       pub stroke: &'a mut StrokeSettings,    // owned by App.engine
       pub pressure_curve: &'a mut Curve,     // owned by App.engine
   }
   ```

   `BrushPanel` itself only remembers which property's effectors are shown.
   The colour panel (`ColorPanel::show`, phase 5A) receives a `ColorModel`
   (main and sub colour, which one is active, recent colours, swatches,
   eyedropper on/off and source) and returns a `ColorAction` (`Set`,
   `SelectMain`, `Toggle`, `Swap`, `Reset`, `AddSwatch`, `RemoveSwatch`,
   `SetPicker`, `SetPickSource`). It keeps only the HSV value it is editing
   (so the hue survives greys) and the hex text while it is typed; an edit
   remembers the colour it produced, so the next frame does not re-derive
   HSV from RGB and nothing drifts or loops. The picker is drawn with
   vertex-coloured meshes (a 9 × 9 grid, no texture, rebuilt each frame at
   negligible cost). `tela-ui` depends on `tela-color` for `Color`/`Hsv`.
   The selection panel (`selection_panel`, phase 5C) receives a
   `SelectionModel` (whether the selection tool is the canvas tool, its
   shape, mode, anti-aliasing, feather strength, whether a selection
   exists) and returns a `SelectionAction` (`Brush`, `Tool`, `Mode`,
   `AntiAlias`, `SelectAll`, `Deselect`, `Invert`, `Feather`,
   `FeatherStrength`); it owns nothing. `SelectionKind` and
   `SelectionMode` are UI enums the app maps to the document's
   `SelectionShape`/`SelectionOp`, so `tela-ui` does not depend on
   `tela-document`. `selection_outline` paints marching ants (a white
   line under a black dashed one whose offset moves with time) on egui's
   background layer, under the panels, from loops the app has already
   mapped to the screen: presentation only. The transform panel
   (`transform_panel`, phase 5D) receives a `TransformModel` (whether a
   session is open, the Move tool, width and height in percent, angle,
   keep aspect, nearest) and returns a `TransformAction`; it owns
   nothing and knows no `TransformParams` (the app converts).
   `transform_box` paints the box, square handles of constant screen
   size (`HANDLE`), the rotation handle and the pivot's cross from points
   the app mapped to the screen; the app hit-tests the same points.
   `apply_transform_prompt` asks "Apply the transform?" (Yes / No /
   Cancel) as the other prompts do. The fill panel (`fill_panel`, phase
   5E) receives a `FillModel` (the tool active, canvas or layer
   reference, tolerance in percent, anti-aliasing, whether a selection
   exists) and returns a `FillAction`; it knows no document types (the
   app converts to `FillReference` and `Tolerance`). The gradient panel
   (`gradient_panel`, phase 5F) receives a `GradientModel` (the tool
   active, its shape and ends as UI enums `GradientKind` /
   `GradientEdge`, the `Mixing`, dithering, the stops as `StopRow`s with
   a `StopColor` of main, sub or a fixed colour, the selected stop, and
   the main and sub colours to draw them) and returns a
   `GradientAction` (tool, shape, ends, mixing, dither, preset, select,
   position, opacity, colour, "set to the active colour", add, remove,
   reverse); it draws the ramp with `tela_color::gradient::ColorRamp`
   (no document types) and owns nothing: the stops live in the app's
   `GradientTool`. `gradient_guide` paints the drag's line and, for
   radial and elliptical gradients, their outline from points the app
   mapped to the screen: presentation only, like the reference's guide.
3. **Commands the UI cannot fulfil are returned, not executed.**
   `BrushPanel::show` returns a `PanelAction` (`SelectPreset`,
   `ToggleEraser`, `ClearLayer`, `Layer(..)`); the app applies it. The layer
   panel (`layer_panel`) gets one `LayerRow` per layer (key, name,
   visibility, lock, opacity, blend, active, clipped, alpha lock, mask
   state, whether the mask is the edit target) and returns a `LayerAction`; keys are
   mapped back to layers with `Document::find_layer`, so a row that went
   stale is simply refused. `tela-ui` does not depend on `tela-document`.
   Controls the document would refuse are greyed out (delete on a locked
   or last layer, opacity on a locked or hidden layer), mirroring the
   reference application; the document remains the authority. The
   document bar (`document_bar`) gets a `DocumentInfo` (file name,
   modified, size, bits per channel, the last file status) and the app's
   `ExportChoice` (transparent, 16-bit), and returns a `DocumentCommand`
   (`New(bits)`, `Convert(bits)`, `Open`, `Save`, `SaveAs`, `ImportPng`,
   `ExportPng`); the app ignores it during a stroke. The unsaved-changes
   prompt and error notices (`prompt::unsaved_prompt`, `prompt::notice`)
   are egui modals drawn while the app says a question is open; they
   return the answer (`PanelAction::Prompt`, `DismissNotice`) and the
   area they block (added to the passthrough regions). They are egui, not
   native message boxes, because a message box opened inside winit's
   event handler is never shown (ADR 0020).
4. **Domain rules stay in the domain crates.** The UI calls the same APIs as
   any other caller, e.g. `StrokeOverrides::edit_resolved` for stroke
   settings (the "editing a setting drops the preset's override" rule lives
   in `tela-brush`, used by both the panel and keyboard shortcuts), and
   `Curve::insert_point/move_point/remove_point` for the curve editor, which
   keep curves valid by construction.
5. **Drawing goes through the renderer's generic hook.** `UiLayer`
   implements `tela_renderer::FrameOverlay`; the renderer does not know what
   it draws.
6. **Input routing is explicit.** `UiLayer::on_window_event` runs first; the
   app skips canvas handling for input egui consumed or that lands on the
   panel outside a stroke. The UI reports the rectangles it occupies
   (`UiLayer::occupied`); the app hands them to the Windows Ink backend as
   passthrough regions, so a pen contact that **starts** over the panel is
   delivered to Windows as mouse input (and reaches egui), while a stroke
   that wanders over the panel keeps painting. Whether a press is the
   UI's is decided by egui's answer **and** the panels' rectangles under
   the current pointer: egui's answer comes from the last frame's pointer,
   and a click faster than a frame (or the first after the window moved)
   used to reach the canvas (UI_AUDIT Q-04). Redraw events are not passed
   back as redraw requests (the app was redrawing while idle, Q-06).
7. **Inspection for tests.** `UiLayer::set_inspect(true)` (only in the
   end-to-end test mode, ADR 0028) keeps egui's AccessKit description of
   every widget each frame (`WidgetInfo`: role, label, value, state,
   rectangle and the visible, clipped part). It changes nothing drawn.

## Frame sequence

```
window event -> UiLayer::on_window_event (egui-winit) -> consumed?
redraw:
  App::build_ui: selection outline traced (once per selection change) and
                 mapped through the view
                 UiLayer::build(|ui| selection_outline(..); top_bar,
                                tool_strip, tool options (ColorPanel + the
                                active tool's panel), document_panel + layer_panel)
     -> occupied rects -> InkBackend::set_passthrough
  App::apply(PanelAction)
  Renderer::render_with(frame, Some(&mut UiLayer))   // canvas, then UI
```

## Known gaps

- Preset edits live only in memory; nothing is saved (TECHNICAL_DEBT.md).
- Preset names are cloned into a `Vec<String>` each frame (UI-rate, not a
  hot path).
- One dark theme; no layout persistence; tools and rows are labelled with
  text, not icons (egui's bundled fonts have no coherent icon set).
- The interface is tested at 100 % display scaling only (UI_AUDIT).
- A gradient or fill committed over a whole 4000×3000 canvas blocks the
  window ~0.3 s without a busy indicator (UI_AUDIT Q-14, debt #43).
- Keyboard shortcuts are handled in the app (`App::on_key`), not in the UI
  crate; they call the same domain APIs.
- The layer panel (5B) renames by double-click (`LayerAction::Rename`),
  reorders by drag and drop (`LayerAction::MoveTo { key, target, above }`:
  keys and a relative placement, never indices; the app calls
  `Document::move_layer`) and merges down (`LayerAction::MergeDown`, also
  Ctrl+E). Its only own state is presentation state in egui memory: the
  name being typed and egui's drag payload. The lower-mask question
  before a merge is a modal in `prompt.rs` (`merge_mask_prompt`); the app
  holds the pending merge (`App::merging`). It has no thumbnails and asks
  no confirmation before deleting (the deletion is undoable). The panel
  has a fixed width and truncates long names.
- The blend-mode selector receives labels and an index (`LayerRow::blend`)
  and returns `LayerAction::SetBlendMode(index)`; the app maps it to
  `BlendMode::ALL`.
- Clipping, alpha lock and the mask (add, delete, on/off, edit pixels or
  mask) are `LayerAction`s on the active layer (`SetClipped`,
  `SetAlphaLocked`, `AddMask`, `RemoveMask`, `SetMaskEnabled`, `EditMask`);
  the rows show text markers ("> " clipped, "α", "[mask]", "[mask off]",
  "[editing mask]"), not thumbnails.
- Undo/Redo buttons (`history_bar`) receive only labels (`HistoryButtons`)
  and return a `HistoryCommand`; the app maps document `HistoryAction`s to
  text.
- The canvas tool (`app::tools::CanvasTool`: brush, eyedropper,
  selection) decides what a left drag does; `M` picks the selection
  tool, `B` the brush, choosing a preset or the eraser returns to the
  brush. The selection tool has a crosshair cursor; there is no custom
  cursor per shape, no selection launcher bar and no command to hide the
  outline. While a selection exists the window redraws eight times a
  second to move the ants (one composed-tile-free frame each).
- The colour panel has no colour wheel, HLS/Lab/CMYK fields or colour
  sets from files; recent colours and swatches are not saved between
  sessions (TECHNICAL_DEBT #35). The eyedropper has no shortcut of its own
  (`I` cycles the interpolation); right-click and Alt pick the canvas.
