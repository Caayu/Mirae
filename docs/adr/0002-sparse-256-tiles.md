# ADR 0002: Sparse 256×256 tiles

Status: accepted (phase 1). Recorded in phase 3.5.

# Context

Illustration documents are large (4000×3000 and up) and mostly empty per
layer; strokes touch small regions. The engine needs cheap allocation,
precise dirty tracking for recomposition and upload, and (later) undo
snapshots of exactly what a stroke changed.

# Decision

Pixel planes are `TiledSurface<P>`: a hash map from `TileCoord` to fixed
256×256 `Tile<P>`, allocated on first write; absent tiles read as
`P::default()`. Addressing uses Euclidean division so negative coordinates
work. Dirty and touched sets are `DirtyTiles` (ordered `BTreeSet`).

# Consequences

- Memory follows painted area, not document size (invariant T1).
- Composition, upload and (future) undo work per tile; a stroke's
  `touched()` set is exactly the undo footprint.
- Per-tile parallel stamping is natural (ADR 0010).
- 256 px tiles mean a 5 px dab still touches a whole tile's worth of
  bookkeeping; measured overhead is acceptable (performance.md).
- Power-of-two tiles let the renderer build each tile's mip chain
  independently (ADR 0004).
- `tile_coords()` iterates in unspecified order; code needing determinism
  sorts or uses `DirtyTiles`.

# Alternatives considered

- One dense buffer per layer: wasteful for sparse layers, coarse dirty
  regions, expensive snapshots.
- Smaller tiles (64/128): finer granularity but more map overhead and more
  tiles per large dab; 256 balances both for brush sizes up to ~500 px.
- Quadtree/hierarchical tiles: more complex; not needed yet.
