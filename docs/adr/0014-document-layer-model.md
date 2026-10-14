# 0014 — Document layer model: stable ids, one mutation path

Phase 4A. Supersedes nothing; makes the document's invariants enforceable.

# Context

Until phase 3.5 `Document` exposed `layers: Vec<RasterLayer>` and
`active_layer: usize` as public fields, and every `RasterLayer` field was
public. Any caller could empty the stack, duplicate a layer, set an
out-of-range active index (a panic) or write pixels without anyone knowing
which tiles changed. The stroke preview found its layer by index, so a
change of the active layer during a stroke would have moved the preview.
Phase 4 adds undo, masks, clipping and file I/O, all of which need to name
layers durably and to see every change.

The reference application's behaviour for the same operations was
observed first (docs/parity/4A-layers.md) so the rules below follow what
professional users expect rather than implementation convenience.

# Decision

1. **Identity is separate from position.** Each layer has a `LayerId`
   (`NonZeroU32`) allocated by a per-document monotonic counter. Ids are
   never reused, not even after removal, so a removed layer can later be
   restored (undo) without colliding. There is no public constructor; keys
   held outside the document (UI rows, files) are turned back into ids by
   validating lookups (`find_layer`). Positions exist only for display
   (`index_of`).
2. **Placement is relative** (`Above(id)`, `Below(id)`, `Top`, `Bottom`),
   which stays meaningful once layers can live in groups.
3. **Lookup is a linear scan** of a `Vec` kept in stack order. Operations
   are per user action, not per pixel, and composition walks the stack in
   order without lookups. An index map would have to be rebuilt on every
   reorder; revisit only with a benchmark showing a need (~1000+ layers).
4. **The document owns the rules.** All fields are private; layers are
   read-only outside the crate. The document has at least one layer; the
   active layer (stored as an id) always exists; new layers are inserted
   where asked, become active and are named "Layer N" with the smallest
   free N; removing the active layer activates the one below, else the one
   above; lock refuses pixel edits, opacity, blend mode and removal but not
   visibility, rename or moves; hidden and locked layers are not paintable
   (`paint_target`). Refused operations return a `LayerError` and change
   nothing.
5. **One pixel mutation path.** Pixels change only through
   `modify_tiles(id, tiles, f)`, which calls `f` for exactly the declared
   tiles (and releases tiles it allocated that stay blank), and
   `clear_layer`. Undo (4B) will snapshot exactly those tiles at this
   point. Every mutation returns the composite tiles it invalidated, so
   callers never decide what to redraw.
6. **Layers move by header only.** Reordering or removing moves the
   ~100-byte `RasterLayer` value; tile buffers stay where they are.
7. **The preview names its layer by id** (`Overlay::target`); the app fixes
   the target when the stroke begins and ignores layer commands until it
   ends (reference behaviour).

# Consequences

- Invariants L1–L9 (docs/architecture/invariants.md) are enforced by the
  type's privacy and checked by a property test against a reference model.
- The app and the UI cannot violate the stack's rules; the UI greys out
  controls the document would refuse, but the document stays the authority.
- 4B can record every change by wrapping the existing mutation methods;
  4D adds the mask surface with the same `modify_tiles` pattern; 4F builds
  documents through a validating constructor.
- Tela has no paper *layer* (the paper colour is a document property), so
  removing the active bottom layer activates the layer above, where the
  reference activates its paper layer.

# Alternatives considered

- **Keep public fields and add helper methods.** Leaves every invariant
  breakable; rejected.
- **Index-based API** (`move_layer(id, index)`). Ambiguous once groups
  exist; rejected in favour of relative placement.
- **`HashMap<LayerId, RasterLayer>` + order `Vec`.** Two structures to keep
  consistent for no measured gain at realistic layer counts.
- **Reusing freed ids / generational ids.** Never reusing is simpler and
  makes restore-after-remove trivially collision-free; 2³² allocations per
  document is not a practical limit (`IdsExhausted` instead of wrapping).
- **Undo in the same subphase.** Rejected (owner decision D1): the API is
  designed to be reversible, and undo's memory and coalescing questions
  are handled separately in 4B.
