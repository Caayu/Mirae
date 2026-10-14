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

Tela at commit 06ccba9, Windows 11, 2560×1440, maximized window, the same
mouse/keyboard driving method; one pen check with the synthetic Windows
Ink pen (`inject_pen … pressure`). Measurements are darkest pixel / count of
non-white pixels in a screen region, as for the reference. Each behaviour
is also covered by a unit or property test (named).

| Ref. | Tela behaviour | How verified |
|---|---|---|
| O1 | Ctrl+Shift+N / "New" inserts directly above the active layer, activates it, names it "Layer 2"; a layer added between 1 and 2 is "Layer 3". Defaults 100 %, Normal, visible, unlocked. | App; `new_layers_go_where_placed_and_become_active` |
| O2 | Deleting the active middle layer activates the layer below; the next new layer reuses the freed number. **No confirmation.** | `removing_the_active_layer_activates_the_one_below_else_above`, `default_names_reuse_the_smallest_free_number` |
| O3 | Default names use the smallest free number. Deleting the active **bottom** layer activates the layer **above** (Tela has no paper layer). | App (deleted active bottom "Layer 2" → "Layer 1" active); same tests |
| O4 | With one layer left, "Delete" is disabled and `remove_layer` returns `LastLayer`. | App; same test |
| O5 | Reordering with Up/Down acts on the active layer, which stays active (as after a drag in the reference). No drag-and-drop. | App ("Down" kept "Layer 2" active); `moving_keeps_identity_selection_and_tile_memory` |
| O6 | Hiding a non-active layer does not change the selection. | App (hiding Layer 1 kept Layer 2 active) |
| O7 | Hiding the active layer keeps it active; the opacity slider is disabled. | App; `hidden_layers_can_be_active_but_not_painted` |
| O8 | A stroke on a hidden layer paints nothing (region empty after showing it). | App (strip measured: only the other layer's line); `paint_target` |
| O9 | A hidden layer can be selected. | App |
| O10 | A locked layer takes no paint (region measured `min=255 ink=0`); opacity and Delete are disabled; New stays enabled. | App; `lock_blocks_pixels_opacity_blend_and_removal_only` |
| O11 | A locked layer can be hidden and moved (Up stayed enabled). | Same test; app |
| O12 | Opacity is edited in whole percent; the stroke preview is shown under the layer's opacity (P3). At 46 % the existing stroke's darkest pixel went 7 → 141. | App; `preview_matches_committed_result` |
| O13 | Ctrl+Shift+N creates a layer above the active one, without a dialog. | App |
| O14 | Ctrl+Shift+N pressed **during** a mouse stroke is ignored (still 2 layers after release); the whole stroke is on the starting layer (hiding it leaves `min=255 ink=0`). | App; `preview_targets_its_layer_by_id_not_by_position` |

## Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-4A-1 | No paper layer: removing the active bottom layer activates the layer above instead of the paper layer; the last undeletable layer is a raster layer, not the paper. | Behavioural, by design (paper colour is a document property) | Accepted (ADR 0014) |
| D-4A-2 | No delete confirmation dialog. | Feature/behavioural | Open — revisit after undo (4B); TECHNICAL_DEBT #17 |
| D-4A-3 | No drag-and-drop reordering in the panel (Up/Down buttons only); in the reference dragging also selects the dragged layer. | Feature (UI) | Open — TECHNICAL_DEBT #17 |
| D-4A-4 | No rename field in the panel (the API supports renaming). | Feature (UI) | Open — TECHNICAL_DEBT #17 |
| D-4A-5 | Default name prefix is "Layer" (English UI) where the reference's pt-BR UI shows "Camada"; the numbering rule is the same. | Localization | Accepted |
| D-4A-6 | Not compared yet: renaming a locked layer (Tela allows it), deleting a hidden layer, layer commands other than "new layer" during a stroke, pen (instead of mouse) strokes in the reference, performance with many layers. | Coverage gap | Open |
| D-4A-7 | Opacity rendering was not compared numerically between the two applications (different brushes). Numeric comparison of compositing belongs to the 4C gate. | Coverage gap | Deferred to 4C |
