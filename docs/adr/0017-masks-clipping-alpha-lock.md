# 0017 — Layer masks, clipping groups and alpha lock

Phase 4D. Written after the reference was measured and operated
(docs/parity/4D-masks-clipping-alpha-lock.md), as the parity rule
requires: none of the semantics below was chosen before the observations.

# Context

Masks, clipping and "lock transparent pixels" interact with each other,
with layer opacity, blend modes (ADR 0016), the stroke preview (invariant
P2) and undo (ADR 0015). The 4D plan left clipping semantics (D11) and
mask painting / eraser-with-lock semantics (D15) open until the reference
was observed. A PSD probe of our own (34 scenarios, 2 × 2 176 measured
swatches) settled composition; the reference's UI settled painting,
history and refusals.

# Decision

1. **Mask representation.** One optional `LayerMask` per layer:
   `TiledSurface<u8>` of the **hidden amount** (0 = shows the layer), plus
   an `enabled` flag. Conceptually (and in every API) 255 shows and 0
   hides, as in the reference; the inverted storage makes an absent tile
   exactly the state of a new mask (reveal everything), so a new mask
   allocates nothing and tiles that reveal everything are released after
   every edit (invariant M3). `Document::modify_mask` inverts the tile in
   place before and after the caller's closure: callers see reveal values,
   no buffer is allocated. 8-bit masks (D13).
2. **Mask in composition.** The mask multiplies the layer's premultiplied
   pixels by `value/255` before opacity, clipping and blend (reference K4).
   A disabled mask is skipped: identical to no mask (K5, invariant M2).
3. **Mask painting.** A stroke on a mask applies its **alpha** to the
   reveal channel: paint `a + m·(1 − a)` (reveals), erase `a·(1 − m)`
   (hides); the paint colour is ignored (reference M2–M4).
   `StrokeBuffer::paint_alpha_tile` (preview) and `commit_alpha_tile`
   (commit) share one private rule. Strokes on a hidden layer, a locked
   layer or a disabled mask are refused (`paint_target`, M9, M10, A6).
4. **Alpha lock.** A layer flag (`alpha_locked`) that the app turns into
   `PixelAlpha::Preserved` when a pixel stroke begins. Paint is then
   **source-atop** (`out = paint·m·αd + dst·(1 − m)`, alpha unchanged;
   measured at partial coverage, A1–A2). **The eraser still erases** (A3).
   One shared `paint_over` serves preview and commit (P2 extended).
5. **Clipping semantics.** A layer has a `clipped` flag. A clipping group
   is an unclipped base and the clipped layers directly above it. It is
   composed **in isolation**: start from the base (× its mask), apply each
   visible clipped layer (× its mask) with `BlendMode::clip_atop`:
   `co = αs·αb·B(Cb, Cs) + (1 − αs)·cb`, `αo = αb` — source-atop with the
   blend function's result replacing the source colour as if the base were
   opaque (K1; the W3C source-atop mix misses by 50/255, a non-isolated
   alpha limit by up to 215/255). Clipped layers never see layers below
   the base.
6. **Composition order** (per layer, then per group):
   `pixels (+ stroke preview) → mask (+ mask preview) → [clipped: opacity
   and blend via clip_atop onto the group]`, and for the group:
   `base pixels → base mask → clipped layers in stack order → base opacity
   and base blend mode onto the stack`. So the base's opacity and mode
   apply to the whole group (K2), the base's mask limits its clipped
   layers (K3), a clipped layer's mask limits only itself.
7. **Edge cases.** A hidden base hides its group; hidden clipped layers
   are skipped and the group continues (K7). Order matters (K8). An
   unclipped layer ends a group (K9). A clipped layer with no unclipped
   layer anywhere below it is drawn like an ordinary layer (K10; in the
   reference it clips to the opaque paper, which is equivalent). A new
   layer is clipped iff the layer directly above its position is clipped,
   so groups stay whole (C4, C5).
8. **Paper and D-4C-1.** Inside a group only the base's alpha matters, so
   clipping never meets the unresolved translucent-backdrop rule (D-4C-1);
   the probe fits with and without paper. Only group bases (and base-less
   clipped layers) blend with the stack, so only their modes decide
   whether a tile is composed over the paper (ADR 0016, refined).
9. **Edit target and history.** `Document` keeps the edit target
   (`LayerPart::Pixels | Mask`) next to the active layer, outside history
   (M8); selecting a layer targets its pixels (M11); a new mask becomes
   the target (M1). The app captures `StrokeTarget { layer, part, alpha }`
   when a stroke begins, as 4A did for the layer id. Steps (swap-based,
   ADR 0015): add mask, remove mask (the mask moves into history), mask
   enable, mask strokes (`modify_mask`, one call = one step), clip toggle,
   alpha-lock toggle. A locked layer refuses mask add/remove/paint, clip
   and alpha lock, but allows mask enable (A6).
10. **Implementation shape.** `compose_tile_into` walks groups; a group
    without visible clipped content takes the pre-4D path (Normal stays
    bit-identical); otherwise the base goes into a reusable `group` buffer
    (swapped, not copied). `clip_atop` has its own pixel loops: sharing
    `composite`'s loops through a generic made `composite` 5–16 % slower
    in several modes, so `composite` keeps its 4C code verbatim.

# Consequences

- Tela reproduces all 2 × 2 176 measured swatches within 2/255
  (`clip_parity.rs`); planted bugs fail by up to 255/255.
- Memory: a mask tile is 64 KiB (a quarter of a pixel tile); a new mask
  costs nothing; masks keep only tiles that hide something. The compose
  scratch grows from 2.25 MiB to 3.5 MiB (`group` 1 MiB, `mask` 256 KiB).
  A mask stroke's history holds the previous mask tiles (64 KiB each) the
  way a pixel stroke holds 256 KiB ones.
- Performance (`mask_clip_bench`, 2048², full tiles): a stored mask adds
  ~15 % to a single layer; a clipped Normal layer costs about the same as
  an ordinary one, a clipped Multiply less than an unclipped Multiply
  (no general-formula mix); 0 allocations. 4C paths unchanged
  (`blend_bench`, `stamp_bench`, `history_bench` equal within noise).
- Structural edits (move, remove, clip toggle, their undo) invalidate the
  clipped layers whose group changes, not only the edited layer.
- Not implemented: applying a mask to the layer, masking from a
  selection, mask display/overlay modes, layer groups (clipping groups are
  not a general group mechanism; the `group` buffer is generic enough to
  serve one later).

# Alternatives considered

- **Non-isolated clipping** (each clipped layer blended onto the whole
  stack, alpha limited by the base): simpler, rejected by measurement.
- **W3C source-atop** for clipped blend modes: rejected by measurement.
- **Base opacity applied to the base only**: equivalent for the measured
  data (the group composite is linear in the base's alpha) and less
  direct; the group form follows the reference's model (K2).
- **Mask stored as reveal values** with a custom "absent = 255" default:
  needs a non-`Default` absent value in `TiledSurface`, or allocating
  every tile of a new mask; rejected for the inverted storage, invisible
  outside the mask type.
- **Alpha lock by restoring alpha after a normal stroke**: not
  source-atop (different colours at partial coverage, measured); rejected.
- **Generic Porter-Duff parameter for the blend loops**: less code, but
  measurably slower `composite`; rejected (see 10).
