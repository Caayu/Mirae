# Parity record 4B — undo/redo

Reference: CLIP STUDIO PAINT EX 5.1.5 (trial), Windows 11, pt-BR UI,
2560×1440. Input: mouse via SendInput, keyboard via keybd_event.
Observed 2026-10-14 on the scratch illustration left from the 4A session
(layers "Camada 1–5", paper layer, Pencil tool). Black-box only: results
were read from the layer panel, the **History palette** (its list of steps
is part of the visible UI; the current step is highlighted, undone steps
are greyed) and pixel measurements of the canvas (darkest pixel / count of
non-white pixels in a strip). The two settings pages quoted were only
read; nothing was changed.

## Procedure and observations

| # | Steps | Observation |
|---|---|---|
| U1 | Draw one stroke; Ctrl+Z; Ctrl+Y; Ctrl+Z; Ctrl+Shift+Z | The stroke is one History entry ("Desenhar/…"). Ctrl+Z removes it (strip ink 4175 → 0 apart from the cursor outline); Ctrl+Y restores it (4175); **Ctrl+Shift+Z also redoes**. |
| U2 | Draw 3 strokes; undo twice; redo once | Three entries. After 2 undos only stroke 1 remains; redo brings stroke 2 back; the undone entry stays **greyed** at the end of the list. |
| U3 | (after U2) Undo; draw a new stroke; Ctrl+Y | The greyed (redoable) entries **disappear** when the new stroke is recorded; Ctrl+Y then does nothing. |
| U4 | Create a layer (Ctrl+Shift+N); undo; redo | Undo removes it and the previously active layer is active again; redo restores it at the same place, with the same name, active. |
| U5 | Create a layer; select another layer; undo the creation | The layer that was active **before the creation** becomes active again (not the one selected afterwards). |
| U6 | Paint on a layer; delete it; select another layer; undo; redo | Undo restores the layer at its position, with its name and pixels (strip 3957 → 0 → 3954), and makes it active again (the selection made in between is replaced). Redo deletes it again and the layer below becomes active, as after the original deletion. |
| U7 | Draw on layer A; select layer B; undo | The stroke disappears; **B stays active** (undoing a stroke does not change the selection). |
| U8 | Drag a layer to another position (the drag selects it); select another layer; undo | The order is restored; **the selection made afterwards is kept**. |
| U9 | Hide a layer; undo | **Visibility is not a History step.** The layer stays hidden; the undo applies to the previous step. Several hide/show toggles in the 4A session also left no entries. No preference to change this was found ("Camada/quadro" page read). |
| U10 | Lock a layer; select another; undo; redo | "Bloquear/Desbloquear camada" is a step; undo unlocks, redo locks; the selection is not changed. |
| U11 | Rename (double-click the name, type, Enter); undo; redo | "Alterar nome da camada" is a step; undo/redo restore the names. A **locked** layer can be renamed, and the rename is undoable. |
| U12 | Change the blend mode to Multiply; undo; redo | "Alterar modo de combinação da camada" is a step; undo/redo switch Normal ↔ Multiply. |
| U13 | Draw on a layer that also holds an older stroke; press Delete; undo; redo | Delete clears the **whole** active layer ("Excluir"); undo restores both strokes; redo clears again. Delete on a hidden layer records nothing. |
| U14 | Click the opacity slider (100 → 32), then drag it (32 → 91); undo | **One** "Alterar opacidade da camada" entry; one undo returns to 100. |
| U15 | Drag opacity, wait 3 s, drag again; undo. Repeated with a 10 s pause | Still **one** entry each time; one undo returns to the value before the first drag. Merging does not depend on time. |
| U16 | Drag opacity; select another layer and back; drag again | **Two** entries: changing the selection ends the merge. Opacity on a different layer is a separate entry. |
| U17 | Opacity step on layer A; draw a stroke; undo the stroke; drag A's opacity again; undo | A **new** entry; undo returns to the value before this drag. The recorded (then undone) stroke ended the merge. |
| U18 | Press Ctrl+Z while drawing (button held), keep drawing, release | The undo is **ignored**: the stroke completes and stays; nothing earlier was undone, then or after release. |
| U19 | Preferences › Desempenho (read only) | "Desfazer nível" **200** (configurable); "Desfazer intervalo entre ações" 0 ms (an optional time grouping, off by default). The 200-step limit was not exercised. |
| U20 | Shortcuts | Ctrl+Z undo; Ctrl+Y redo; Ctrl+Shift+Z redo. |

Steps refused by the reference (a stroke on a hidden or locked layer)
record nothing. Holding the pointer still during a stroke can add a
separate "Ajustar Formas Inteligentes" entry (a shape-assist feature, not
related to undo; Tela has no equivalent yet).

## Reference behaviour (summary)

1. Linear history; a new step discards the redoable steps. Default depth
   200 steps, configurable.
2. Steps: each completed stroke, clear layer, new layer, delete layer,
   reorder, lock/unlock, rename, blend mode, opacity.
3. **Not** steps: selecting a layer, showing/hiding a layer.
4. Undo/redo restore pixels, names, positions and properties exactly
   (as far as pixel measurements show).
5. Active layer: undoing or redoing a **create/delete** sets the active
   layer recorded with that step (before it for undo, after it for redo);
   every other undo/redo leaves the current selection alone.
6. Opacity changes on the same layer merge into one step as long as
   nothing else is recorded and the selection does not change, however
   many clicks/drags and however long the pauses.
7. Undo/redo requests during a stroke are ignored, not queued.

## Tela design derived from the observations

| Reference | Tela 4B |
|---|---|
| 1 | Linear history in `Document`; recording clears redo; limits `max_steps` (default 200) **and** `max_bytes` (default 1 GiB, Tela's own memory policy). |
| 2 | Recorded: `modify_tiles` (one call = one stroke), `clear_layer`, `add_layer`, `remove_layer`, `move_layer`, `set_locked`, `set_name`, `set_blend_mode`, `set_opacity`. |
| 3 | `set_active_layer` and `set_visible` are **not recorded** (documented non-historical operations). This differs from the 4B brief, which listed visibility as historical; the reference was followed. |
| 4 | Swap-based tile history; removed layers and cleared surfaces move into the history; bit-exact by construction. |
| 5 | Each step stores the active layer before/after; only add/remove steps restore it. |
| 6 | Opacity coalescing by an "open chain": a new opacity value merges into the newest step when that step is an opacity change of the same layer and nothing has broken the chain (another recorded step, undo, redo, or a change of the active layer). No timer. This differs from the 4B brief ("two separate drags = two undos"); the reference was followed. |
| 7 | The app ignores undo/redo while a stroke is in progress. |

## Tela results

Pending: filled in when 4B closes.

## Differences

Pending: filled in when 4B closes.
