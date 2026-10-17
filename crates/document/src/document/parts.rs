//! Building a document from parts: the validated entry point for code
//! that reconstructs a document it did not edit (loading a file).
//!
//! [`Document::from_parts`] checks every structural invariant (L1–L5,
//! DP1, M3 and the edit-target rule) and refuses anything else, so a
//! caller — however malformed its input — can never construct an invalid
//! document. The parts are plain data; the persisted format lives
//! elsewhere (`tela-io`), and this crate knows nothing about files.

use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU32;

use tela_canvas::{TileCoord, TiledSurface};
use tela_color::{PixelDepth, Rgba};

use super::history::History;
use super::{Document, LayerPart, invert};
use crate::layer::LayerMask;
use crate::{BlendMode, LayerId, LayerPixels, RasterLayer};

/// Largest tile coordinate magnitude a document accepts: every pixel of
/// such a tile has an `i32` coordinate (`x · 256` and `x · 256 + 255` do
/// not overflow).
pub const MAX_TILE_COORD: i32 = i32::MAX / tela_canvas::TILE_SIZE as i32;

/// Whether a tile at `c` addresses only representable pixels.
pub(crate) fn tile_in_range(c: TileCoord) -> bool {
    let r = -MAX_TILE_COORD - 1..=MAX_TILE_COORD;
    r.contains(&c.x) && r.contains(&c.y)
}

/// Everything a document is made of, except its history.
#[derive(Debug)]
pub struct DocumentParts {
    /// Canvas size in pixels, at least 1 × 1.
    pub width: u32,
    pub height: u32,
    /// Paper colour, straight alpha; opaque, channels in `0..=1`.
    pub background: Rgba,
    /// Storage depth; every layer's pixels must have it.
    pub depth: PixelDepth,
    /// Bottom to top; at least one.
    pub layers: Vec<LayerParts>,
    /// Raw id of the active layer.
    pub active_layer: u32,
    /// What strokes edit; `Mask` only if the active layer has a mask.
    pub active_part: LayerPart,
    /// The next id the document will hand out (greater than every
    /// layer's id), or `None` once `u32::MAX` has been allocated.
    pub next_layer_id: Option<u32>,
}

/// One layer of [`DocumentParts`].
#[derive(Debug)]
pub struct LayerParts {
    /// Raw id, non-zero and unique in the document.
    pub id: u32,
    pub name: String,
    pub visible: bool,
    /// Finite, in `0..=1` (not clamped: anything else is refused).
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub locked: bool,
    pub alpha_locked: bool,
    pub clipped: bool,
    /// At the document's depth.
    pub pixels: LayerPixels,
    pub mask: Option<MaskParts>,
}

/// A layer mask as the user sees it.
#[derive(Debug)]
pub struct MaskParts {
    pub enabled: bool,
    /// How much each pixel shows the layer: 255 fully, 0 not at all. A
    /// tile that is not stored shows everything, and no stored tile may
    /// be all 255 (it would be indistinguishable from an absent one).
    pub reveal: TiledSurface<u8>,
}

/// Why [`Document::from_parts`] refused its parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartsError {
    /// Width or height is 0, or larger than `i32::MAX`.
    InvalidSize { width: u32, height: u32 },
    /// The paper colour is not an opaque colour with channels in `0..=1`.
    InvalidPaper,
    /// A document has at least one layer.
    NoLayers,
    /// Layer ids are non-zero.
    ZeroId,
    /// Two layers share this id.
    DuplicateId(u32),
    /// The next id is not greater than every layer's id (or is 0).
    StaleNextId { next: u32, largest: u32 },
    /// No layer has the active id.
    ActiveNotFound(u32),
    /// The mask is the edit target but the active layer has no mask.
    MaskTargetWithoutMask,
    /// The layer's opacity is not finite or not in `0..=1`.
    InvalidOpacity { layer: u32 },
    /// The layer's pixels are not at the document's depth.
    WrongDepth { layer: u32 },
    /// A tile addresses pixels outside the `i32` range.
    TileOutOfRange { layer: u32, coord: TileCoord },
    /// A stored mask tile shows everything (only absent tiles may).
    MaskTileRevealsAll { layer: u32, coord: TileCoord },
}

impl fmt::Display for PartsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PartsError::InvalidSize { width, height } => {
                write!(f, "invalid canvas size {width} × {height}")
            }
            PartsError::InvalidPaper => f.write_str("the paper colour is not an opaque colour"),
            PartsError::NoLayers => f.write_str("a document needs at least one layer"),
            PartsError::ZeroId => f.write_str("layer id 0 is not valid"),
            PartsError::DuplicateId(id) => write!(f, "two layers have id {id}"),
            PartsError::StaleNextId { next, largest } => {
                write!(
                    f,
                    "next layer id {next} is not above the largest id {largest}"
                )
            }
            PartsError::ActiveNotFound(id) => write!(f, "the active layer {id} does not exist"),
            PartsError::MaskTargetWithoutMask => {
                f.write_str("the mask is the edit target but the active layer has none")
            }
            PartsError::InvalidOpacity { layer } => write!(f, "layer {layer}: invalid opacity"),
            PartsError::WrongDepth { layer } => {
                write!(
                    f,
                    "layer {layer}: pixels at another depth than the document's"
                )
            }
            PartsError::TileOutOfRange { layer, coord } => {
                write!(
                    f,
                    "layer {layer}: tile ({}, {}) out of range",
                    coord.x, coord.y
                )
            }
            PartsError::MaskTileRevealsAll { layer, coord } => write!(
                f,
                "layer {layer}: stored mask tile ({}, {}) shows everything",
                coord.x, coord.y
            ),
        }
    }
}

impl std::error::Error for PartsError {}

impl Document {
    /// Builds a document from its parts, with an empty history, after
    /// checking every structural invariant (ADR 0014, 0017, 0018). Invalid
    /// parts are refused with the first problem found; nothing is
    /// repaired or clamped, so what is built is exactly what was given.
    pub fn from_parts(parts: DocumentParts) -> Result<Self, PartsError> {
        let DocumentParts {
            width,
            height,
            background: paper,
            depth,
            layers: given,
            active_layer,
            active_part,
            next_layer_id,
        } = parts;
        let max = i32::MAX as u32;
        if width == 0 || height == 0 || width > max || height > max {
            return Err(PartsError::InvalidSize { width, height });
        }
        let unit = |v: f32| (0.0..=1.0).contains(&v);
        if ![paper.r, paper.g, paper.b].into_iter().all(unit) || paper.a != 1.0 {
            return Err(PartsError::InvalidPaper);
        }
        if given.is_empty() {
            return Err(PartsError::NoLayers);
        }
        let mut ids = HashSet::with_capacity(given.len());
        let mut layers = Vec::with_capacity(given.len());
        for l in given {
            let raw = NonZeroU32::new(l.id).ok_or(PartsError::ZeroId)?;
            if !ids.insert(l.id) {
                return Err(PartsError::DuplicateId(l.id));
            }
            layers.push(build_layer(LayerId::new(raw), l, depth)?);
        }
        let largest = layers.iter().map(|l| l.id().get()).max().unwrap_or(0);
        let next_id = match next_layer_id {
            None => None,
            Some(next) if next > largest => NonZeroU32::new(next),
            Some(next) => return Err(PartsError::StaleNextId { next, largest }),
        };
        let active = layers
            .iter()
            .map(RasterLayer::id)
            .find(|id| id.get() == active_layer)
            .ok_or(PartsError::ActiveNotFound(active_layer))?;
        let active_has_mask = layers.iter().any(|l| l.id() == active && l.mask.is_some());
        if active_part == LayerPart::Mask && !active_has_mask {
            return Err(PartsError::MaskTargetWithoutMask);
        }
        Ok(Self {
            width,
            height,
            background: paper,
            depth,
            layers,
            active,
            active_part,
            next_id,
            history: History::default(),
            selection: None,
            selection_revision: 0,
        })
    }

    /// The raw id the next new layer will get, or `None` once every id has
    /// been used (ids are never reused, L2). Persisted so that ids stay
    /// unique across saving and loading.
    pub fn next_layer_id(&self) -> Option<u32> {
        self.next_id.map(NonZeroU32::get)
    }
}

fn build_layer(id: LayerId, l: LayerParts, depth: PixelDepth) -> Result<RasterLayer, PartsError> {
    let raw = id.get();
    if !l.opacity.is_finite() || !(0.0..=1.0).contains(&l.opacity) {
        return Err(PartsError::InvalidOpacity { layer: raw });
    }
    if l.pixels.depth() != depth {
        return Err(PartsError::WrongDepth { layer: raw });
    }
    if let Some(coord) = l.pixels.tile_coords().find(|&c| !tile_in_range(c)) {
        return Err(PartsError::TileOutOfRange { layer: raw, coord });
    }
    let mask = match l.mask {
        None => None,
        Some(MaskParts {
            enabled,
            reveal: mut hidden,
        }) => {
            let coords: Vec<TileCoord> = hidden.tile_coords().collect();
            for coord in coords {
                if !tile_in_range(coord) {
                    return Err(PartsError::TileOutOfRange { layer: raw, coord });
                }
                let tile = hidden.tile_mut(coord).expect("listed coordinate");
                invert(tile);
                if tile.is_blank() {
                    return Err(PartsError::MaskTileRevealsAll { layer: raw, coord });
                }
            }
            Some(LayerMask { enabled, hidden })
        }
    };
    let mut layer = RasterLayer::new(id, l.name, depth);
    layer.visible = l.visible;
    layer.opacity = l.opacity;
    layer.blend_mode = l.blend_mode;
    layer.locked = l.locked;
    layer.alpha_locked = l.alpha_locked;
    layer.clipped = l.clipped;
    layer.pixels = l.pixels;
    layer.mask = mask;
    Ok(layer)
}

#[cfg(test)]
mod tests {
    use tela_canvas::{DirtyTiles, Tile};

    use super::*;
    use tela_color::StoragePixel;

    use crate::{LayerError, Placement};

    /// Parts read back through the public getters, as a file writer would.
    fn to_parts(d: &Document) -> DocumentParts {
        let layers = d
            .layers()
            .map(|l| LayerParts {
                id: l.id().get(),
                name: l.name().to_owned(),
                visible: l.visible(),
                opacity: l.opacity(),
                blend_mode: l.blend_mode(),
                locked: l.locked(),
                alpha_locked: l.alpha_locked(),
                clipped: l.clipped(),
                pixels: l.pixels().clone(),
                mask: l.mask().map(|m| {
                    let mut reveal = TiledSurface::new();
                    for c in m.tile_coords() {
                        let t = reveal.tile_or_insert(c);
                        for (d, v) in t.pixels_mut().iter_mut().zip(m.reveal_tile(c).unwrap()) {
                            *d = v;
                        }
                    }
                    MaskParts {
                        enabled: m.enabled(),
                        reveal,
                    }
                }),
            })
            .collect();
        DocumentParts {
            width: d.width(),
            height: d.height(),
            background: d.background(),
            depth: d.depth(),
            layers,
            active_layer: d.active_layer().get(),
            active_part: d.active_part(),
            next_layer_id: d.next_layer_id(),
        }
    }

    fn varied(depth: PixelDepth) -> Document {
        let mut d = Document::with_depth(700, 300, depth);
        let a = d.active_layer();
        let b = d.add_layer(Placement::Top).unwrap();
        let c = d.add_layer(Placement::Top).unwrap();
        let mut tiles = DirtyTiles::new();
        tiles.mark(TileCoord { x: 0, y: 0 });
        tiles.mark(TileCoord { x: -1, y: 2 });
        tela_color::with_pixel_type!(depth, P => {
            for id in [a, c] {
                d.modify_tiles(id, &tiles, |_, t: &mut Tile<P>| {
                    *t.get_mut(3, 4) = P::from_rgba8([10, 20, 30, 40]);
                })
                .unwrap();
            }
        });
        d.add_mask(b).unwrap();
        d.modify_mask(b, &tiles, |_, t| *t.get_mut(1, 1) = 9)
            .unwrap();
        d.set_mask_enabled(b, false).unwrap();
        d.set_clipped(c, true).unwrap();
        d.set_blend_mode(c, BlendMode::Overlay).unwrap();
        d.set_opacity(a, 0.37).unwrap();
        d.set_alpha_locked(a, true).unwrap();
        d.set_locked(c, true).unwrap();
        d.set_visible(a, false).unwrap();
        d.set_name(b, "Ink \u{2712}").unwrap();
        let gone = d.add_layer(Placement::Top).unwrap();
        d.remove_layer(gone).unwrap();
        d.set_active_layer(b).unwrap();
        d.set_active_part(LayerPart::Mask).unwrap();
        d
    }

    #[test]
    fn parts_round_trip_exactly_at_both_depths() {
        for depth in PixelDepth::ALL {
            let d = varied(depth);
            let back = Document::from_parts(to_parts(&d)).unwrap();
            assert_eq!(back.digest(), d.digest());
            assert_eq!(back.active_part(), LayerPart::Mask);
            assert_eq!(
                back.next_layer_id(),
                d.next_layer_id(),
                "removed ids stay used"
            );
            assert_eq!((back.width(), back.height()), (700, 300));
            assert!(!back.can_undo() && !back.can_redo(), "no history");
            back.check_invariants();
        }
    }

    #[test]
    fn invalid_parts_are_refused() {
        type Case = (&'static str, fn(&mut DocumentParts), PartsError);
        let size = |width, height| PartsError::InvalidSize { width, height };
        let cases: Vec<Case> = vec![
            ("zero width", |p| p.width = 0, size(0, 300)),
            ("huge height", |p| p.height = u32::MAX, size(700, u32::MAX)),
            (
                "translucent paper",
                |p| p.background.a = 0.5,
                PartsError::InvalidPaper,
            ),
            (
                "NaN paper",
                |p| p.background.g = f32::NAN,
                PartsError::InvalidPaper,
            ),
            ("no layers", |p| p.layers.clear(), PartsError::NoLayers),
            ("zero id", |p| p.layers[0].id = 0, PartsError::ZeroId),
            (
                "duplicate id",
                |p| p.layers[1].id = p.layers[0].id,
                PartsError::DuplicateId(1),
            ),
            (
                "stale next id",
                |p| p.next_layer_id = Some(3),
                PartsError::StaleNextId {
                    next: 3,
                    largest: 3,
                },
            ),
            (
                "missing active",
                |p| p.active_layer = 99,
                PartsError::ActiveNotFound(99),
            ),
            (
                "mask target without mask",
                |p| p.active_layer = 1,
                PartsError::MaskTargetWithoutMask,
            ),
            (
                "NaN opacity",
                |p| p.layers[2].opacity = f32::NAN,
                PartsError::InvalidOpacity { layer: 3 },
            ),
            (
                "opacity above 1",
                |p| p.layers[0].opacity = 1.5,
                PartsError::InvalidOpacity { layer: 1 },
            ),
            (
                "wrong depth",
                |p| p.depth = PixelDepth::Rgba16,
                PartsError::WrongDepth { layer: 1 },
            ),
        ];
        for (what, spoil, want) in cases {
            let mut p = to_parts(&varied(PixelDepth::Rgba8));
            spoil(&mut p);
            assert_eq!(Document::from_parts(p).err(), Some(want), "{what}");
        }
    }

    #[test]
    fn tiles_out_of_range_and_blank_mask_tiles_are_refused() {
        let far = TileCoord {
            x: MAX_TILE_COORD + 1,
            y: 0,
        };
        let mut p = to_parts(&varied(PixelDepth::Rgba8));
        let mut s = TiledSurface::<[u8; 4]>::new();
        s.insert_tile(far, Tile::new());
        p.layers[0].pixels = LayerPixels::from_surface(s);
        assert_eq!(
            Document::from_parts(p).err(),
            Some(PartsError::TileOutOfRange {
                layer: 1,
                coord: far
            })
        );
        let mut p = to_parts(&varied(PixelDepth::Rgba8));
        let c = TileCoord { x: 4, y: 4 };
        let mask = p.layers[1].mask.as_mut().unwrap();
        mask.reveal.insert_tile(c, Tile::filled(255));
        assert_eq!(
            Document::from_parts(p).err(),
            Some(PartsError::MaskTileRevealsAll { layer: 2, coord: c })
        );
        // The extreme in-range coordinates address representable pixels.
        for x in [-MAX_TILE_COORD - 1, MAX_TILE_COORD] {
            assert!(tile_in_range(TileCoord { x, y: x }));
            let (ox, _) = TileCoord { x, y: 0 }.origin();
            assert!(ox.checked_add(255).is_some());
        }
    }

    #[test]
    fn exhausted_ids_load_and_stay_exhausted() {
        let mut p = to_parts(&varied(PixelDepth::Rgba8));
        p.next_layer_id = None;
        let mut d = Document::from_parts(p).unwrap();
        assert_eq!(
            d.add_layer(Placement::Top).err(),
            Some(LayerError::IdsExhausted)
        );
    }

    #[test]
    fn imported_layers_are_one_step_and_follow_the_placement_rules() {
        let mut d = varied(PixelDepth::Rgba8);
        let below = d.layers().nth(2).unwrap().id();
        let mut s = TiledSurface::<[u8; 4]>::new();
        s.set_pixel(5, 5, [1, 2, 3, 4]);
        let steps = d.history_len();
        let before = d.digest();
        let pixels = LayerPixels::from_surface(s);
        let (id, dirty) = d
            .add_layer_with_pixels(Placement::Below(below), "photo", pixels)
            .unwrap();
        assert_eq!(d.history_len(), steps + 1);
        assert_eq!(d.next_undo(), Some(crate::HistoryAction::ImportLayer));
        assert_eq!(d.active_layer(), id);
        let l = d.layer(id).unwrap();
        assert_eq!(l.name(), "photo");
        assert_eq!(l.pixels().pixel_rgba8(5, 5), [1, 2, 3, 4]);
        assert!(l.clipped(), "inserted into a clipping group");
        assert!(dirty.contains(TileCoord { x: 0, y: 0 }));
        d.undo().unwrap();
        assert_eq!(d.digest(), before, "undo restores the active layer too");
        d.redo().unwrap();
        assert_eq!(d.active_layer(), id);
        // The other depth is refused and changes nothing.
        let now = d.digest();
        let s16 = LayerPixels::from_surface(TiledSurface::<[u16; 4]>::new());
        assert!(matches!(
            d.add_layer_with_pixels(Placement::Top, "x", s16),
            Err(LayerError::WrongDepth { .. })
        ));
        assert_eq!(d.digest(), now);
        d.check_invariants();
    }
}
