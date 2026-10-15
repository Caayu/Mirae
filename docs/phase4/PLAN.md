# Phase 4 plan — layers and document

Approved by the owner on 2026-10-14 (sequence 4A → 4B → 4C → 4D → 4E →
4F → 4G).

| Subphase | Status |
|---|---|
| 4A | **Complete** (parity gate: [parity/4A-layers.md](../parity/4A-layers.md)) |
| 4B | **Complete** (parity gate: [parity/4B-undo-redo.md](../parity/4B-undo-redo.md); ADR 0015) |
| 4C–4G | Awaiting the owner's approval | **Each subphase needs the owner's explicit approval before it
starts**, and each ends with a parity gate ([PARITY.md](../PARITY.md)).

## Rules for the whole phase

- Crate boundaries stay as they are (except the new `tela-io` crate in
  4F): `document` knows nothing of brush, renderer, UI or tablet; the
  renderer only receives finished RGBA8 tiles; the UI never sees the
  `Document`.
- No domain logic in `app`: it maps actions to document calls.
- 16-bit storage is not mixed with the layer API refactor; I/O only after
  the model is stable.
- No abstractions for future features without a concrete need; not all
  technical debt is fixed here.
- Any change to a documented invariant is called out explicitly and gets
  an ADR. Determinism, goldens and benchmarks are preserved.
- Semantics the reference defines are **observed before being
  implemented** (parity rule 5).

## Current code facts that shape the plan

- `Document` has public `layers: Vec<RasterLayer>` / `active_layer: usize`;
  an out-of-range index panics (debt #10).
- `Overlay::above_layer` is an index taken from `active_layer` every
  frame, so changing the active layer mid-stroke would move the preview.
- `StrokeBuffer::commit_into(&mut TiledSurface<[u8; 4]>)` writes straight
  into a layer: there is no single point where pixel changes pass (undo
  needs one).
- Composition runs in premultiplied, sRGB-**encoded** f32 (`from_rgba8`
  divides by 255, no linearization).
- `tela-ui` may not depend on `tela-document`.

## 4A — Document model, layer API, stable ids

**Goal.** Replace the public fields with an API where invalid states are
unrepresentable or refused with typed errors: stable ids, an always-valid
active layer, mutation only through `Document`.

**Crates.** `document` (main), `brush` (additive `commit_tile`), `app`
(migration, stroke target captured at begin, mid-stroke guard), `ui`
(minimal layer panel via a view model), `corpus` (bench migration).

**API (sketch).**

```rust
pub struct LayerId(NonZeroU32);   // opaque; no public constructor; get() -> u32
pub enum LayerError { NotFound(LayerId), LastLayer, Locked(LayerId), Hidden(LayerId), InvalidOpacity, IdsExhausted }
pub enum Placement { Above(LayerId), Below(LayerId), Top, Bottom }

impl RasterLayer { id, name, visible, opacity, blend_mode, locked, pixels(&self) }  // getters only
impl Document {
    layer_count, layers() /* bottom→top iterator */, layer(id), find_layer(raw), index_of(id),
    layer_above(id), layer_below(id), active_layer(), set_active_layer(id),
    add_layer(Placement) -> Result<LayerId>   // default name "Layer N", smallest unused N
    remove_layer(id), move_layer(id, Placement),
    set_visible / set_opacity / set_name / set_blend_mode / set_locked,   // -> Result<DirtyTiles>
    paint_target(id) -> Result<(), LayerError>  // refuses hidden and locked layers
    modify_tiles(id, &DirtyTiles, FnMut(TileCoord, &mut Tile<[u8; 4]>)) -> Result<DirtyTiles>
    clear_layer(id) -> Result<DirtyTiles>
}
pub struct Overlay<'a> { pub target: LayerId, pub paint, pub covers }
impl StrokeBuffer { pub fn commit_tile(&self, TileCoord, &mut Tile<[u8; 4]>) }  // commit_into built on it
```

Every mutation returns the composite tiles it invalidated, so the app
never decides which tiles an operation affects.

**Design points.**

| Point | Decision |
|---|---|
| Id | `NonZeroU32`, opaque, allocated by a monotonic per-document counter, **never reused**; UI/IO only obtain ids through validating lookups. |
| Identity vs position | Separate. Public API speaks ids and `Placement`; indices only for display (`index_of`). `Placement` is relative, so it survives future groups. |
| Lookup | `Vec<RasterLayer>` in stack order, linear scan by id; composition iterates in order and compares `id == overlay.target` (no lookups). Revisit with a benchmark above ~1000 layers. |
| Active layer | Stored as `LayerId`; always exists. |
| Remove active | Layer below becomes active, else the one above (reference O2–O4; Tela has no paper layer). Removing the last layer → `LastLayer`. |
| Lock | Blocks pixel edits, opacity, blend mode and removal; visibility, rename and move allowed (reference O10–O11). |
| Hidden | Can be active; cannot be painted (`paint_target` → `Hidden`) (reference O8–O9). |
| During a stroke | The stroke captures its target id at begin; layer commands are ignored while a stroke is in progress (reference O14). |
| Groups later | ids + `Placement`, iterator instead of slices, one id allocator for all node kinds; no `parent`/`LayerKind`/`Group` now. |
| Encapsulation | Private fields, getters only, no public `&mut RasterLayer`; pixels change only through `modify_tiles`/`clear_layer`. |
| Errors | `Result` for anything a stale UI id can trigger; `Option` for getters; `expect` only for internal invariants. |
| No tile copies | Moves/removals move the ~100-byte layer header; tile buffers stay put (pointer-identity test). |
| Undo readiness | Every operation has its inverse data available; a crate-private `insert_at(index, layer)` restores exact positions in 4B. |
| Masks/clipping later | `modify_tiles` is the pattern for the mask surface; new properties are private field + getter + setter returning `DirtyTiles`. |

**Invariants added.** L1 ids unique · L2 ids never reused · L3 ≥ 1 layer ·
L4 active exists · L5 opacity finite in `[0,1]` · L6 failed operations
leave the document unchanged · L7 move/remove/re-insert copy no tiles ·
L8 composition independent of id values · L9 `modify_tiles` touches only
the declared tiles. P1–P3 keep their meaning (tests move from index to
id). **No documented invariant changes meaning.**

**Migration.** Expand → migrate → contract: add the API beside the fields,
migrate callers, make fields private, delete `active_layer_mut`.

**Tests.** Unit tests per operation and error; property tests (state
machine against a reference model) for L1–L9; pointer identity; preview ==
commit; goldens unchanged; real-app run with `inject_pen`.

**Benchmarks.** `stamp_bench` composition line unchanged (±5 %, 0 allocs);
`commit_tile` without per-tile allocation.

**ADR 0014** — document layer model. **Parity gate**: layer creation,
selection, removal, removal of the active layer, reorder, visibility,
opacity, lock, and their interaction with a stroke
([parity/4A-layers.md](../parity/4A-layers.md)).

**Commits.** (1) `LayerId`/`LayerError`/allocator; (2) encapsulated layer
properties; (3) stack API beside the fields; (4) `modify_tiles`,
`clear_layer`, `commit_tile`; (5) overlay by id, app/bench migration,
fields private; (6) property tests; (7) layer panel, docs, ADR, parity.

## 4B — Undo/redo

**As built** (ADR 0015): the reference was observed first and two points
of this plan changed to match it — visibility is *not* a history step, and
opacity edits merge by an uninterrupted chain (separate drags on one layer
undo together) instead of per gesture. Original plan text follows.

Linear history of **every** document mutation, owned by `Document` and
recorded automatically (no unrecorded mutation path). Tile edits store the
other state of each touched tile and **swap** on undo/redo (no copies after
the commit's single copy; absent tiles restored as absent). Removed layers
and cleared surfaces move into history. Limits in bytes and steps (default
1 GiB / 200, configurable), oldest evicted; property changes of one
interactive gesture coalesce; changing the active layer is not a step;
undo is ignored while a stroke is in progress. Invariants H1 undo∘redo =
identity, H2 k undos = state k steps ago, H3 a new edit clears redo, H4
memory within budget, H5 no tile copies on undo/redo, H6 no unrecorded
mutation. ADR 0015. Parity gate: undo granularity and what is undoable,
compared with the reference.

## 4C — Blend modes and compositor

W3C Compositing and Blending Level 1 modes (separable and non-separable)
plus `plus-lighter`; mode dispatch outside the per-pixel loop; **Normal
stays bit-identical** to today. The compositing space is **not frozen
yet**: each mode is checked against (1) the public definition, (2)
numeric tests, (3) behavioural tests against the reference (colour
samples of known layer pairs). Results are recorded in
`parity/4C-blend.md` before ADR 0016 makes the compositing space a
permanent decision. Blend-sheet golden in `document/tests`. Optional
"merge down". Parity gate: per-mode numeric comparison with the reference.

## 4D — Masks, clipping, alpha lock

Before implementation, observe and record the reference's behaviour for:
clipping with partial alpha, several clipped layers, opacity, different
blend modes and layers below the base; painting, erasing and pressure on a
mask; the eraser on an alpha-locked layer. Then implement equivalent
behaviour in Tela's own architecture. Masks are 8-bit (inverted storage so
absent tiles reveal). Alpha lock changes the stroke compositing (amends
ADR 0003, extends invariant P2). ADR 0017. Parity gate on all of the above.

## 4E — 16-bit storage

Per-document depth, `[u16; 4]` premultiplied in the current encoding;
`StoragePixel` trait in `color`; generic `commit_tile`; renderer
unchanged; masks stay 8-bit. 8 → 16 exact, 16 → 8 rounded;
`convert_depth` is one heavy undo step. 8-bit stays the **temporary**
default, re-evaluated after the 4E benchmarks and behaviour tests. Extends
invariant T3. Banding metric recorded. ADR 0018. Parity gate: banding and
depth conversion compared with the reference.

## 4F — Document I/O

New crate `tela-io` (document, canvas, color, serde, serde_json, png,
zip); `Document::from_parts` validates every invariant; zip container +
JSON manifest + per-tile entries; atomic save; strict structural errors,
lenient unknown values; v1 fixture kept forever; PNG export/import.
During development a fixed path / drag-and-drop may be used, but the
**Definition of Done** includes the normal desktop workflow: Ctrl+O,
Ctrl+S, Save As, native file dialog, drag-and-drop. `rfd` (or similar) is
allowed only in `app`; no core crate depends on it. ADRs 0019–0020.
Parity gate: file workflow and PNG fidelity.

## 4G — Integration, benchmarks, hardening

End-to-end headless scenarios (layers, modes, masks, clipping, undo,
save/load); `doc_bench` (layers × modes × depth, undo memory, save/load);
real-app scenarios; documentation and phase report; final parity summary.

## Decisions

| # | Decision | Status |
|---|---|---|
| D1 | Undo separate in 4B | Approved |
| D2 | Minimal layer panel in 4A via a UI view model | Approved |
| D3 | Document always has ≥ 1 layer | Approved |
| D4 | Active after removal: below, else above | Approved (matches reference O2–O4 except the paper layer) |
| D5 | `NonZeroU32` ids, never reused; relative `Placement` | Approved |
| D6 | History inside `Document`, recorded automatically | Approved |
| D7 | Undo limits 1 GiB / 200 steps, configurable; active change not a step; gesture coalescing | Approved |
| D8 | W3C modes + plus-lighter in 4C | Approved, with the compositing-space rule above |
| D9 | Blend-sheet golden in `document/tests` | Approved |
| D10 | Merge down in 4C; duplicate layer out | Approved |
| D11 | Clipping semantics | **Determined by observing the reference before 4D** |
| D12 | 8-bit default | Approved as temporary; re-evaluate after 4E |
| D13 | 8-bit masks | Approved for this phase |
| D14 | `convert_depth` as a heavy undo step | Approved |
| D15 | Mask painting/erasing/pressure; eraser with alpha lock | **Determined by observing the reference before 4D** |
| D16 | File dialogs | Fixed path/drag-and-drop during development; full desktop workflow in the 4F DoD; dialog crate only in `app` |
| D17 | Corrupt tile on load is an error | Approved |
| D18 | Save off-canvas pixels in v1 | Approved |
| D19 | zip container; presets not stored in documents | Approved |

## Largest risks

1. Preview == commit across modes × masks × clipping × lock × depth.
2. Undo memory with large brushes and 16-bit.
3. The file format becomes a permanent compatibility promise.
4. CPU composition scaling with many layers (measured in 4G; solved later).
5. UI crate growth.
6. Still no physical tablet.
