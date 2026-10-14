//! Sparse tiled pixel storage, dirty-region tracking and the canvas view
//! transform. Nothing here knows about layers, brushes or the GPU.

mod rect;
mod tile;
mod view;

pub use rect::IRect;
pub use tile::{DirtyTiles, TILE_SIZE, Tile, TileCoord, TiledSurface};
pub use view::ViewTransform;
