//! Sparse tiled pixel storage, dirty-region tracking and the canvas view
//! transform.
//!
//! **Responsibility:** [`TiledSurface`] (an unbounded plane of 256×256
//! [`Tile`]s allocated on first write), [`TileCoord`] addressing,
//! [`DirtyTiles`] sets, integer rectangles ([`IRect`]) and the
//! screen↔canvas [`ViewTransform`].
//! **Does not:** know about layers, colors, brushes, input or the GPU; the
//! pixel type is generic.
//! **Depends on:** `glam`. **Used by:** brush, document, renderer, app.
//!
//! Units: canvas pixels (document space, integer pixel `(x, y)` covers
//! `[x, x+1) × [y, y+1)`, centre at `+0.5`) and screen pixels (window
//! client area, physical). Only [`ViewTransform`] converts between them.
//!
//! Invariants: a tile that was never written is not allocated; tile
//! addressing uses Euclidean division, so negative coordinates work;
//! `DirtyTiles` iterates in a deterministic order.

mod rect;
mod tile;
mod view;

pub use rect::IRect;
pub use tile::{DirtyTiles, TILE_SIZE, Tile, TileCoord, TiledSurface};
pub use view::ViewTransform;
