# 0023 — Merge down, rename steps and layer reordering by drag

Phase 5B. Written after the reference was observed
([../parity/5B-transparent-compositing.md](../parity/5B-transparent-compositing.md),
parts 2–4) and after D-4C-1 was resolved ([ADR 0022](0022-blending-over-translucent-backdrops.md)).

# Context

Merge down was deferred since 4C (debt #21, D-4C-3) because merging onto
a translucent lower layer is exactly the translucent-backdrop rule that
was unknown. With ADR 0022 the rule is known. The brief also asked not
to assume that merge down is "compose the pair, store it as Normal
100 %", but to measure what happens to properties, masks, clipping,
opacity, blend mode, name, id and the active layer, and to complete the
layer panel's rename and drag-and-drop reorder after observing the
reference.

# Decision

1. **`Document::merge_down(id, LowerMask)`** merges layer `id` into the
   layer directly below it. The pair is composed **alone**, over
   transparency, exactly as the compositor composes those two layers:
   lower pixels × lower mask (if applied) × lower opacity, then the
   upper layer × its enabled mask with its opacity and blend mode
   (`composite`, ADR 0022), or `clip_atop` when the upper layer is
   clipped and the lower one is its group base (ADR 0017). Nothing below
   the pair and not the paper takes part (reference: md_under_*, paper).
2. **The lower layer survives**: id, name, position, blend mode, clip
   flag, alpha lock; opacity becomes 100 % (baked in). The upper layer is
   removed; its id is never reused (L2). The merged layer becomes active
   with its pixels as the edit target.
3. **Masks.** An enabled upper mask is applied, a disabled one dropped.
   An enabled lower mask: `LowerMask::Apply` bakes it and deletes it,
   `LowerMask::Keep` keeps it and merges the lower pixels unmasked (the
   reference asks Apply / Keep / Cancel; the app asks the same question).
   A disabled lower mask is kept, without a question.
4. **Refusals** (errors, document unchanged): no layer below
   (`NoLayerBelow`), either layer locked (`Locked`) or hidden (`Hidden`).
   The reference disables its command in these cases; merging onto its
   paper layer rasterizes the paper, which Mirae has no equivalent of
   (the paper is a document property, ADR 0014): the bottom layer simply
   has nothing below.
5. **Clipping layers above** keep their flag and clip to whatever base
   the stack gives them afterwards (merging a group base down makes its
   clipped layers clip to the merged layer, as in the reference).
6. **History**: one step (`MergeDown`) made of the existing changes —
   the lower layer's changed tiles (`Tiles`), its opacity, its applied
   mask (`MaskPresence`) and the upper layer (`Presence`) — so undo and
   redo swap, never copy, and restore ids, order, pixels, masks,
   properties and the active layer bit for bit. Only the tiles that
   change are held: those under the upper layer's pixels, or every lower
   tile when its opacity or mask is baked in. No new history mechanism.
7. **Precision**: the composite is `f32` and is quantized once to the
   document's depth; 16-bit documents never pass through 8 bits.
8. **Renames merge like opacity edits**: consecutive renames of the same
   layer are one history step until something else is recorded, undone,
   redone, or the active layer changes (reference RN4). Invariant H8
   covers renames too. The rename field (double-click, Enter commits,
   Escape cancels, focus loss commits, an empty name keeps the old one,
   64 characters at most) is UI; the document stores any name.
9. **Drag-and-drop reorder** calls `Document::move_layer` with a
   relative placement computed from the drop position (above or below
   the row under the pointer); the panel never keeps indices. Clipping
   flags move with their layers and groups are recomputed by position
   (4D C2), so no rule lives in the UI.

# Consequences

- `merge_parity.rs` rebuilds 22 measured scenarios through `Document`
  and matches every swatch within 2/255 at both depths; three planted
  alternatives (no `clip_atop`, `clip_atop` for every clipped layer,
  opacity not baked) fail by 48–97 swatches.
- Merge down preserves the picture exactly where the reference does:
  the pair alone, or a Normal pair over anything (Normal is associative).
  It changes the picture, as in the reference, where a non-Normal pair
  blended with something under its translucent pixels (layers below or
  the paper), where a lower mask is kept, and where a clipping base is
  merged down (MD1 states this scope).
- New invariants MD1–MD6, LR1–LR3, RN1–RN2; H8 is extended to renames;
  the property models include merge.
- No file-format change: the result uses existing structures.
- Memory: the step holds the upper layer plus the previous content of
  the lower layer's changed tiles; the live document holds the merged
  tiles. No third full copy beyond what undo needs.

# Alternatives considered

- **Merge as compose-then-Normal-100 %** (the brief's warning): the
  reference keeps the lower layer's blend mode; resetting it would change
  the picture of every non-Normal lower layer.
- **Merge the composite of everything below** (appearance always kept):
  not what the reference does, and it would bake other layers' content
  into the merged one.
- **A dedicated history change holding both whole layers**: simpler, but
  holds the untouched lower tiles too; the existing changes compose.
- **Separate steps for every rename**: what Mirae did before 5B; the
  reference merges consecutive renames of one layer.
