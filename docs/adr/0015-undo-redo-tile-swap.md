# 0015 — Undo/redo: history inside Document, swap-based steps

Phase 4B. Builds on ADR 0014 (one pixel-mutation path, stable ids).

# Context

Tela had no undo (debt #4). Phase 4A routed every document change through
`Document` methods and every pixel change through `modify_tiles`, which
visits exactly the tiles a stroke declares. The reference application's
undo behaviour was observed before designing this
(docs/parity/4B-undo-redo.md): what is a step, which undos restore the
active layer, how opacity edits group, what happens during a stroke.

# Decision

1. **The history lives inside `Document`** (a private `document::history`
   submodule) and every mutation method records its own step. Callers
   cannot change the document without recording, by construction. The
   only non-historical operations are those the reference does not record
   either: `set_active_layer` and `set_visible`. No-op calls record
   nothing. A separate history crate or an app-owned history was rejected:
   both allow unrecorded mutations, after which replaying inverses
   corrupts the document.
2. **Linear history**: an undo stack and a redo stack; recording a step
   clears redo.
3. **Steps store the other state and swap.** A step is a list of changes
   (`Tiles`, `Surface`, `Presence`, `Move`, property changes), each
   holding the state that is not in the document. Applying a change swaps
   it with the document's, so the same record serves undo and redo:
   - `Tiles`: the previous content of each tile a stroke changed (`None`
     for a tile that did not exist, so it comes back absent). The only
     copy history makes is here, once, when the stroke is recorded; undo
     and redo move buffers.
   - `Surface`: clearing a layer moves its whole surface into the history
     (no copy at all).
   - `Presence`: removing a layer moves the `RasterLayer` into the
     history; undo moves it back, with the same `LayerId`, position,
     properties and tiles. Ids are never reused, so this cannot collide.
   - `Move` and property changes swap small values.
4. **One `modify_tiles` call is one step**, so one stroke is one undo.
   Nothing is recorded per dab, tile or frame.
5. **Active layer**: each step stores the active layer before and after
   it; undoing/redoing add or remove steps restores it, every other
   undo/redo keeps the current selection (reference U4–U8, U10).
6. **Opacity coalescing by chain, not by gesture or time**: a new opacity
   value merges into the newest step when that step is an opacity change
   of the same layer and nothing has interrupted it (another recorded
   step, an undo or redo that happened, a change of the active layer).
   This is what the reference does (U14–U17): a click and a drag, or
   drags seconds apart, undo together. It needs no gesture API and no
   timer.
7. **Limits are policy**: `HistoryLimits { max_steps: 200, max_bytes:
   1 GiB }` by default, changeable at run time. The byte limit is enforced
   on each step's **larger direction**, fixed when it is recorded (a
   stroke over empty tiles holds nothing until it is undone, then holds
   the new tiles); that keeps the budget valid after any sequence of
   undos and redos. A step larger than the budget is kept alone. Oldest
   steps are dropped first, then the farthest redoable ones.
8. **No compression** in this phase.

# Consequences

- Invariants H1–H10 (docs/architecture/invariants.md) hold by
  construction and are property-tested against a model that is a stack of
  full-content digests.
- Undo and redo cost microseconds even for strokes over dozens of tiles;
  recording a stroke costs one 256 KiB copy (plus comparison) per changed
  tile that already had pixels (docs/performance.md, `history_bench`).
- Memory is 256 KiB per changed tile per step; a 500 px brush across a
  4096² canvas holds ~18.5 MiB per stroke, so the 1 GiB default holds
  about 55 such strokes before dropping old ones.
- 16-bit storage (4E) doubles tile size; the byte budget and the
  per-step cost adapt automatically. Masks (4D) add surfaces recorded
  with the same changes.
- The app must ignore undo/redo during a stroke (the stroke's preview is
  built on the current layer content).

# Alternatives considered

- **Copy-on-write tiles (`Arc`)**: would also avoid copies, but changes
  `TiledSurface` for every user and adds refcount checks on the stamping
  path; swapping gives the same zero-copy undo/redo with no canvas change.
- **Snapshotting the whole layer or document**: memory proportional to
  the document, not to the edit; rejected.
- **Command/inverse-function history** (re-running operations): pixel
  edits are not invertible (quantization, erase), so tiles must be kept
  anyway; storing states is simpler and bit-exact.
- **Gesture-delimited coalescing** (begin/end signals from the UI, as the
  4B brief proposed): differs from the reference, where separate drags on
  the same layer merge. Rejected by the owner at 4B approval.
- **Recording visibility** (as the 4B brief listed): the reference does
  not; recorded visibility would also make undo stop at every eye toggle.
  Rejected by the owner at 4B approval.
