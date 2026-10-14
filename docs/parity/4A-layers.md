# Parity record 4A — layer stack and layer properties

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440. Input: mouse via SendInput, keyboard via keybd_event.
Observed 2026-10-14 on a new 1600×1200 illustration (one raster layer
"Camada 1" above a "Papel" paper layer). Black-box only: the layer panel
and canvas were read from screen captures; no file of the application was
opened.

## Procedure and observations

Each step starts from the state left by the previous one. "Active" is
the layer marked with the pen icon (the editing target).

| # | Steps | Observation |
|---|---|---|
| O1 | Click "new raster layer" with Camada 1 active; select Camada 1; click it again | Each new layer is inserted **directly above the active layer** and **becomes active**. Defaults: 100 %, Normal, visible, unlocked. Names: "Camada 2", then "Camada 3" (inserted between 1 and 2). |
| O2 | Delete the active middle layer (Camada 3); create a layer | Deletion asks for confirmation ("Excluir camada selecionada. Você deseja prosseguir?", with "do not show again"). After deletion the **layer below** becomes active. The next new layer is again "Camada 3". |
| O3 | Select the bottom raster layer (Camada 1), delete it; create a layer | The **layer below** (the paper layer) becomes active. The new layer is named "Camada 1": the default name uses the **smallest positive number not in use**. |
| O4 | Delete the active top layer repeatedly until only the paper layer is left; try to delete it | Each time the **layer below** becomes active. With one layer left the delete button is **disabled**: the stack never becomes empty. |
| O5 | Create three layers; drag the bottom raster layer (not active) above the top | The layer moves to the drop position and **becomes active** (dragging a row selects it). |
| O6 | Hide a non-active layer | The active layer does not change. |
| O7 | Hide the active layer | It stays active; the opacity and blend-mode controls become disabled. |
| O8 | Draw a stroke on the hidden active layer; show it again | **Nothing is painted** (the area stays empty after showing it). No dialog appears. Drawing after showing it paints normally. |
| O9 | Select a hidden layer by clicking its row | It becomes active while staying hidden. |
| O10 | Lock the active layer; draw on it | **Nothing is painted.** Opacity, blend mode and **delete** are disabled; new-layer buttons stay enabled; a lock icon appears on the row. |
| O11 | With the layer still locked: toggle visibility; drag it to another position | Both work: lock does **not** prevent visibility changes or reordering. |
| O12 | Unlock; set opacity to 50 (field accepts whole percent); measure an existing stroke; draw a new stroke and measure its darkest pixel while the button is held and after release | Darkest pixel 141 for the old stroke, **141 during** the new stroke and **141 after** release: the live stroke is shown under the layer's opacity, identical to the committed result. |
| O13 | Press the new-layer shortcut (Ctrl+Shift+N) outside a stroke | A layer "Camada 4" is created above the active layer, without a dialog. |
| O14 | Press Ctrl+Shift+N **while drawing** (button held), keep drawing, release | **No layer is created**, neither during nor after the stroke (the command is not queued). The whole stroke lands on the original layer (hiding that layer removes all of it). |

## Reference behaviour (summary)

1. New layer: directly above the active layer, becomes active, default
   name "<Layer> N" with N the smallest unused positive number.
2. Selection is independent of visibility and lock; hidden and locked
   layers can be active.
3. Deleting the active layer activates the layer below. The last remaining
   layer cannot be deleted. Deletion asks for confirmation.
4. Reordering moves the layer; in the panel, dragging also selects it.
5. Hidden layers cannot be painted on. Hiding never changes the selection.
6. Opacity is set in whole percent; the stroke preview respects the layer's
   opacity exactly.
7. Lock blocks painting, opacity, blend mode and deletion, but not
   visibility or reordering.
8. Layer commands issued during a stroke are ignored; the stroke finishes
   on the layer it started on.

## Tela design derived from the observations

| Reference behaviour | Tela 4A |
|---|---|
| 1 | `add_layer` inserts above the active layer and activates it; the document assigns "Layer N" (smallest unused N). |
| 2 | `set_active_layer` accepts hidden and locked layers. |
| 3 | Removing the active layer activates the layer below, or the one above if it was the bottom layer (Tela has no paper layer; see differences). Removing the last layer is an error. |
| 4 | `move_layer` keeps the selection unchanged; the panel's move buttons act on the active layer, so the moved layer is active, as after a drag. |
| 5 | The document reports whether a layer can be painted (`paint_target`), refusing hidden and locked layers; the app does not start a stroke on them. |
| 6 | Opacity stored as `f32`, edited as whole percent in the panel; preview under opacity was already invariant P3. |
| 7 | Locked layers refuse pixel edits, opacity, blend mode and removal (`LayerError::Locked`); visibility, rename and move are allowed. |
| 8 | The stroke records its target layer when it begins; layer commands are ignored while a stroke is in progress. |

## Tela results

Pending: filled in when 4A closes.

## Differences

Pending: filled in when 4A closes. Known by design so far:

- Tela has no paper layer; the paper colour is a document property. When
  the bottom layer is removed while active, the reference activates the
  paper layer below; Tela activates the layer above.
- The reference's last undeletable layer is the paper layer; in Tela it is
  the last raster layer.
