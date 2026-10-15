use std::fmt;
use std::num::NonZeroU32;

use tela_canvas::TiledSurface;

use crate::BlendMode;

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

/// Pixel layer stored as premultiplied sRGB RGBA8 tiles.
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
    pub(crate) pixels: TiledSurface<[u8; 4]>,
}

impl RasterLayer {
    pub(crate) fn new(id: LayerId, name: String) -> Self {
        Self {
            id,
            name,
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            locked: false,
            pixels: TiledSurface::new(),
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

    /// Premultiplied sRGB-encoded RGBA8, sparse.
    pub fn pixels(&self) -> &TiledSurface<[u8; 4]> {
        &self.pixels
    }
}
