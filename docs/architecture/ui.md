# UI boundary

The UI is deliberately minimal and functional (it is not designed yet).
What matters is where it sits in the architecture.

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
   reference application; the document remains the authority.
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
   that wanders over the panel keeps painting.

## Frame sequence

```
window event -> UiLayer::on_window_event (egui-winit) -> consumed?
redraw:
  App::build_ui: UiLayer::build(|ui| BrushPanel::show(ui, PanelModel{..}))
     -> occupied rects -> InkBackend::set_passthrough
  App::apply(PanelAction)
  Renderer::render_with(frame, Some(&mut UiLayer))   // canvas, then UI
```

## Known gaps

- Preset edits live only in memory; nothing is saved (TECHNICAL_DEBT.md).
- Preset names are cloned into a `Vec<String>` each frame (UI-rate, not a
  hot path).
- No visual design, no layout persistence, no theming.
- Keyboard shortcuts are handled in the app (`App::on_key`), not in the UI
  crate; they call the same domain APIs.
- The layer panel has no drag-and-drop reordering and no rename field; it
  asks no confirmation before deleting (the deletion is undoable).
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
