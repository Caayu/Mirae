use std::fmt;
use std::num::NonZeroU32;

use tela_canvas::{Tile, TileCoord, TiledSurface};
use tela_color::PixelDepth;

use crate::{BlendMode, LayerPixels};

/// Stable identity of a layer within one [`Document`](crate::Document).
///
/// Ids are allocated by the document, are never reused (not even after the
/// layer is removed) and say nothing about the layer's position in the
/// stack. There is no public constructor: callers obtain ids from the
/// document, and turn external keys back into ids with
/// [`Document::find_layer`](crate::Document::find_layer), which checks that
/// the layer exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(NonZeroU32);

impl LayerId {
    pub(crate) fn new(raw: NonZeroU32) -> Self {
        Self(raw)
    }

    /// The raw value, usable as an opaque key (UI rows, files).
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl fmt::Display for LayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "layer #{}", self.0)
    }
}

/// Pixel layer stored as premultiplied sRGB-encoded RGBA tiles at the
/// document's depth (8 or 16 bits per channel; its mask is always 8-bit).
///
/// Read-only outside this crate: properties change through
/// [`Document`](crate::Document) methods, which keep the document's
/// invariants (valid opacity, lock rules, tile tracking).
#[derive(Clone, Debug)]
pub struct RasterLayer {
    id: LayerId,
    pub(crate) name: String,
    pub(crate) visible: bool,
    pub(crate) opacity: f32,
    pub(crate) blend_mode: BlendMode,
    pub(crate) locked: bool,
    pub(crate) alpha_locked: bool,
    pub(crate) clipped: bool,
    pub(crate) pixels: LayerPixels,
    pub(crate) mask: Option<LayerMask>,
}

impl RasterLayer {
    pub(crate) fn new(id: LayerId, name: String, depth: PixelDepth) -> Self {
        Self {
            id,
            name,
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            locked: false,
            alpha_locked: false,
            clipped: false,
            pixels: LayerPixels::new(depth),
            mask: None,
        }
    }

    pub fn id(&self) -> LayerId {
        self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Hidden layers are skipped by composition and cannot be painted on.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// `0..=1`, applied at composition.
    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    /// How the layer combines with what is below.
    pub fn blend_mode(&self) -> BlendMode {
        self.blend_mode
    }

    /// Locked layers refuse pixel edits, opacity and blend-mode changes and
    /// removal; they can still be shown, hidden, renamed and moved.
    pub fn locked(&self) -> bool {
        self.locked
    }

    /// Premultiplied sRGB-encoded RGBA at the document's depth, sparse.
    pub fn pixels(&self) -> &LayerPixels {
        &self.pixels
    }

    /// "Lock transparent pixels": strokes may change colour but not alpha
    /// (the eraser still erases; parity record 4D A1–A3).
    pub fn alpha_locked(&self) -> bool {
        self.alpha_locked
    }

    /// Clipped to the layer below: shown only where its clipping base has
    /// alpha (docs/adr/0017). A clipped layer with no unclipped layer
    /// anywhere below it is drawn like an ordinary layer.
    pub fn clipped(&self) -> bool {
        self.clipped
    }

    /// The layer mask, if the layer has one.
    pub fn mask(&self) -> Option<&LayerMask> {
        self.mask.as_ref()
    }
}

/// A layer mask: how much of its layer each pixel shows (one per layer).
/// Masks are 8-bit in documents of every depth (decision D13, ADR 0018):
/// the raster depth of a document is not its mask depth.
///
/// Conceptually 255 shows the layer and 0 hides it, like the masks of the
/// reference application. It is **stored inverted**, as the amount each
/// pixel *hides*, so that an absent tile is exactly the state of a new
/// mask (reveal everything): a fresh mask allocates nothing, and tiles that
/// reveal everything again are released (docs/adr/0017).
#[derive(Clone, Debug)]
pub struct LayerMask {
    pub(crate) enabled: bool,
    /// Hidden amount per pixel, 0 = shows the layer; never holds a tile of
    /// zeros.
    pub(crate) hidden: TiledSurface<u8>,
}

impl LayerMask {
    pub(crate) fn new() -> Self {
        Self {
            enabled: true,
            hidden: TiledSurface::new(),
        }
    }

    /// A disabled mask is kept but not applied (equivalent to no mask).
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// How much the mask shows the layer at one pixel: 255 fully, 0 not
    /// at all.
    pub fn value(&self, x: i32, y: i32) -> u8 {
        255 - self.hidden.pixel(x, y)
    }

    /// What one stored tile shows, row-major (255 shows the layer), or
    /// `None` where the mask stores nothing there (it shows everything).
    pub fn reveal_tile(&self, c: TileCoord) -> Option<impl ExactSizeIterator<Item = u8> + '_> {
        self.hidden
            .tile(c)
            .map(|t| t.pixels().iter().map(|h| 255 - h))
    }

    /// Tiles that hide something (the rest of the mask reveals).
    pub fn tile_count(&self) -> usize {
        self.hidden.tile_count()
    }

    /// The tiles that hide something.
    pub fn tile_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.hidden.tile_coords()
    }

    /// Stored hidden amounts of one tile, if it hides anything (crate
    /// internal: composition and history).
    pub(crate) fn hidden_tile(&self, c: TileCoord) -> Option<&Tile<u8>> {
        self.hidden.tile(c)
    }
}
