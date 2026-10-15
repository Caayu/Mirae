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

Tela at commit 9b7358e, Windows 11, 2560×1440, maximized; same driving and
measuring method (mouse; one check with the synthetic Windows Ink pen).
Each behaviour is also covered by a unit or property test.

| Ref. | Tela behaviour | How verified |
|---|---|---|
| U1 | One stroke is one step; Ctrl+Z removes it (strip ink 4986 → 0), Ctrl+Y and Ctrl+Shift+Z restore it (4986). The Undo button does the same. | App; H1/H7 tests |
| U2 | Ten strokes undone one by one to nothing, five redone with identical ink counts per stroke. | App; property test |
| U3 | A new step after undo disables Redo; creating a layer after undoing strokes discarded them. | App; `new_edits_discard_redo` |
| U4 | Undoing a new layer removes it and reactivates the previous layer; redo restores it, active. | App; `active_layer_after_undo_follows_the_reference` |
| U5 | Undoing a creation after selecting another layer reactivates the layer active before the creation. | App (Layer 3 created, Layer 1 selected, undo → Layer 2 active) |
| U6 | Undoing a deletion restores the layer with its id, position, name and pixels (ink 5985 → 0 → 5985) and makes it active; redo deletes it again and the layer below becomes active. | App; `removed_layers_come_back_with_their_id_position_and_pixels` |
| U7 | Undoing a stroke keeps the current selection. | `active_layer_after_undo_follows_the_reference` |
| U8 | Undoing a reorder restores the order and keeps the selection made afterwards. | App; same test |
| U9 | Visibility is not a step: clear, hide, undo → the clear is undone, the layer stays hidden. | App; `visibility_is_outside_history` |
| U10 | Lock is a step; undo unlocks (Delete and opacity re-enabled). | App; H6 test |
| U11 | Rename is a step (also on a locked layer). No rename field in the panel yet, so checked by tests only. | `every_recorded_mutation_is_one_step_and_the_others_none` |
| U12 | Blend mode is a step. Only `Normal` exists until 4C, so checked by the API only. | `every_recorded_mutation…` (no-op case); `Change::Blend` swap |
| U13 | Clear (C or "Clear layer") is one step; undo restores the strokes (ink 0 → 5984). Clearing an empty layer records nothing. | App; `clearing_moves_the_surface_into_history` |
| U14–U15 | Two opacity drags in a row are one step: 100 → 28 %, one undo → 100 % (ink back to 5984). No timer. | App; `opacity_edits_merge_until_something_interrupts_them` |
| U16 | A selection change between two drags makes two steps (undo → 50 %, undo → 100 %). | App; same test |
| U17 | Any recorded step (even one undone afterwards) ends the merge. | Same test; property test |
| U18 | Ctrl+Z during a stroke is ignored: the stroke lands, earlier strokes stay. | App |
| U19 | 200 steps by default (`HistoryLimits`), plus a 1 GiB byte budget of Tela's own. | `step_limit_drops_the_oldest_steps`, `byte_limit_keeps_an_oversized_step_alone` |
| U20 | Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z. | App |

## Differences

| # | Difference | Kind | Status |
|---|---|---|---|
| D-4B-1 | Tela also limits history by memory (1 GiB default, worst case per step); the reference shows only a step count in its settings. | Behavioural, by design | Accepted (ADR 0015) |
| D-4B-2 | No History panel (list of steps, click to jump); Tela has Undo/Redo buttons with the next step's name as a tooltip. | Feature (UI) | Open |
| D-4B-3 | Not compared: whether an undo/redo *between* two opacity drags ends the merge in the reference (Tela: it does), whether a no-op undo/redo does (Tela: it does not), behaviour at the 200-step limit, opacity typed into the numeric field, undo of other properties merging (Tela merges only opacity). | Coverage gap | Open |
| D-4B-4 | Rename and blend mode undo were compared in the reference but cannot be exercised in Tela's UI yet (no rename field; blend modes arrive in 4C). | Coverage gap | Open — 4C for blend modes, TECHNICAL_DEBT #17 for rename |
| D-4B-5 | The reference's shape-assist step ("Ajustar Formas Inteligentes") has no Tela equivalent. | Feature (outside 4B) | Not planned in phase 4 |
| D-4B-6 | Undo/redo speed and memory were measured in Tela only; not compared with the reference. | Coverage gap | Open |

## Changes to the 4B brief caused by the observations

1. **Visibility is not recorded** (the brief listed it as historical).
2. **Opacity merging is by uninterrupted chain**, so two separate drags on
   the same layer with nothing in between are **one** undo (the brief
   asked for two). A selection change, another step, or an undo/redo
   splits them. Switching to per-gesture splitting is a one-line change
   (end the chain on the UI's drag-end signal) if the owner prefers it.
