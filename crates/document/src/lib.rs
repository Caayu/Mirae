//! The document model.
//!
//! **Responsibility:** the authoritative pixel data ([`RasterLayer`]s of
//! premultiplied RGBA8 tiles), the layer stack with stable [`LayerId`]s and
//! the rules for changing it ([`Document`]), and per-tile composition
//! ([`Document::compose_tile_into`]) with an optional [`Overlay`] for
//! content that is not committed yet (the stroke preview).
//! **Does not:** know about brushes, input, the view or the GPU. Brushes
//! reach it only through the `Overlay` callbacks and by the app committing
//! a finished stroke with [`Document::modify_tiles`].
//! **Depends on:** color, canvas. **Used by:** app, corpus (benchmark).
//!
//! Invariants: layer ids are unique and never reused, the stack is never
//! empty, the active layer always exists, opacity stays in `0..=1`, refused
//! operations change nothing, and pixels change only through
//! `modify_tiles`/`clear_layer` in the tiles declared to them (L1–L9 in
//! docs/architecture/invariants.md). Composition of a tile is a pure
//! function of the layers and the overlay (preview and commit share the
//! overlay's paint function); `compose_tile_into` does not allocate; pixels
//! outside the document are cleared at composition (they may exist in
//! layers).

mod blend;
mod compose;
mod document;
mod error;
#[cfg(test)]
mod history_properties;
mod layer;
#[cfg(test)]
mod properties;

pub use blend::BlendMode;
pub use compose::{ComposeScratch, Overlay, OverlayPaint};
pub use document::{Document, HistoryAction, HistoryLimits, LayerPart, Placement};
pub use error::LayerError;
pub use layer::{LayerId, LayerMask, RasterLayer};
