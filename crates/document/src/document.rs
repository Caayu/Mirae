mod history;
mod merge;
mod parts;
mod selecting;

use std::num::NonZeroU32;

use tela_canvas::{DirtyTiles, IRect, Tile, TileCoord};
use tela_color::{PixelDepth, Rgba, StoragePixel};

use self::history::{Change, History, layer_bytes, mask_bytes, pixels_bytes};
pub use self::history::{HistoryAction, HistoryLimits, Revision};
pub use self::merge::LowerMask;
pub use self::parts::{DocumentParts, LayerParts, MAX_TILE_COORD, MaskParts, PartsError};
use crate::layer::LayerMask;
use crate::pixels::{TileBackup, tile_bytes};
use crate::{BlendMode, LayerError, LayerId, LayerPixels, RasterLayer, Selection};

/// Prefix of the names the document gives new layers ("Layer 3").
const DEFAULT_NAME: &str = "Layer";

/// Which part of a layer strokes edit: its pixels or its mask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayerPart {
    #[default]
    Pixels,
    Mask,
}

/// Where a layer goes in the stack, relative to another layer or to the
/// ends. Relative placement (rather than an index) stays meaningful when
/// layers can later be nested in groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Directly above this layer.
    Above(LayerId),
    /// Directly below this layer.
    Below(LayerId),
    /// Above every layer.
    Top,
    /// Below every layer.
    Bottom,
}

/// A raster document: size, paper color and a layer stack.
///
/// Invariants (docs/architecture/invariants.md, L1–L9): layer ids are
/// unique and never reused; there is always at least one layer; the active
/// layer always exists; opacity is finite and in `0..=1`; an operation that
/// returns an error leaves the document unchanged; layers move without
/// copying their tiles; pixels change only through
/// [`Document::modify_tiles`] and [`Document::clear_layer`], and only in
/// the tiles declared to them.
///
/// Every mutation returns the tiles whose composite it invalidated, so
/// callers never decide which tiles an operation affects.
///
/// Every mutation is also recorded in the document's undo history
/// ([`Document::undo`], ADR 0015), except the two the reference application
/// does not record either: [`Document::set_active_layer`] and
/// [`Document::set_visible`]. No-op calls record nothing.
#[derive(Clone, Debug)]
pub struct Document {
    width: u32,
    height: u32,
    background: Rgba,
    /// Storage depth of every layer's pixels (masks are always 8-bit).
    depth: PixelDepth,
    /// Bottom to top; never empty.
    layers: Vec<RasterLayer>,
    /// Always the id of a layer in `layers`.
    active: LayerId,
    /// `Mask` only while the active layer has a mask.
    active_part: LayerPart,
    /// Next id to hand out; greater than every id allocated so far.
    /// `None` once `u32::MAX` has been used.
    next_id: Option<NonZeroU32>,
    history: History,
    /// The active selection (ADR 0024); never empty: an empty result is
    /// no selection (reference 5C S17, S18). Not saved in files (S30).
    selection: Option<Selection>,
    /// Changes whenever the selection does (also by undo/redo), so views of
    /// it (the outline) know when to rebuild. Not content.
    selection_revision: u64,
}

impl Document {
    /// White 8-bit document with one empty layer, "Layer 1", which is
    /// active.
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_depth(width, height, PixelDepth::Rgba8)
    }

    /// Like [`Document::new`], storing pixels at `depth`.
    pub fn with_depth(width: u32, height: u32, depth: PixelDepth) -> Self {
        let first = LayerId::new(NonZeroU32::MIN);
        Self {
            width,
            height,
            background: Rgba::WHITE,
            depth,
            layers: vec![RasterLayer::new(first, format!("{DEFAULT_NAME} 1"), depth)],
            active: first,
            active_part: LayerPart::Pixels,
            next_id: NonZeroU32::MIN.checked_add(1),
            history: History::default(),
            selection: None,
            selection_revision: 0,
        }
    }

    /// Canvas width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Canvas height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Paper color shown under all layers.
    pub fn background(&self) -> Rgba {
        self.background
    }

    /// Bits per channel of every layer's pixels. Masks are 8-bit at every
    /// depth (ADR 0018).
    pub fn depth(&self) -> PixelDepth {
        self.depth
    }

    /// `(0, 0)..(width, height)` in canvas pixels.
    pub fn bounds(&self) -> IRect {
        IRect::from_size(self.width, self.height)
    }

    /// Number of layers (at least 1).
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Layers from bottom to top, as a slice (composition).
    pub(crate) fn layer_slice(&self) -> &[RasterLayer] {
        &self.layers
    }

    /// Layers from bottom to top.
    pub fn layers(&self) -> impl DoubleEndedIterator<Item = &RasterLayer> + ExactSizeIterator {
        self.layers.iter()
    }

    pub fn layer(&self, id: LayerId) -> Option<&RasterLayer> {
        self.index_of(id).map(|i| &self.layers[i])
    }

    /// The id whose [`LayerId::get`] is `raw`, if that layer exists. Turns
    /// keys kept outside the document (UI rows) back into ids.
    pub fn find_layer(&self, raw: u32) -> Option<LayerId> {
        self.layers
            .iter()
            .map(RasterLayer::id)
            .find(|id| id.get() == raw)
    }

    /// Position from the bottom (0), for display. Positions change when
    /// layers are added, removed or moved; ids do not.
    pub fn index_of(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|l| l.id() == id)
    }

    /// The layer directly above `id`, if any.
    pub fn layer_above(&self, id: LayerId) -> Option<LayerId> {
        let i = self.index_of(id)?;
        self.layers.get(i + 1).map(RasterLayer::id)
    }

    /// The layer directly below `id`, if any.
    pub fn layer_below(&self, id: LayerId) -> Option<LayerId> {
        let i = self.index_of(id)?;
        i.checked_sub(1).map(|j| self.layers[j].id())
    }

    /// The layer painting targets. Always exists.
    pub fn active_layer(&self) -> LayerId {
        self.active
    }

    /// Makes `id` the active layer, with its pixels as the edit target (as
    /// selecting a layer does in the reference, parity record 4D M11).
    /// Hidden and locked layers can be active. Not recorded in history; a
    /// change of selection ends opacity merging.
    pub fn set_active_layer(&mut self, id: LayerId) -> Result<(), LayerError> {
        self.require(id)?;
        if self.active != id {
            self.history.chain = None;
        }
        self.active = id;
        self.active_part = LayerPart::Pixels;
        Ok(())
    }

    /// What strokes on the active layer edit. Always `Pixels` unless the
    /// active layer has a mask.
    pub fn active_part(&self) -> LayerPart {
        self.active_part
    }

    /// Chooses whether strokes edit the active layer's pixels or its mask.
    /// Not recorded in history (reference M8).
    pub fn set_active_part(&mut self, part: LayerPart) -> Result<(), LayerError> {
        if part == LayerPart::Mask && self.layers[self.active_index()].mask.is_none() {
            return Err(LayerError::NoMask(self.active));
        }
        self.active_part = part;
        Ok(())
    }

    /// Adds an empty layer at `at` and makes it active. It is named
    /// "Layer N" with the smallest N no other layer's name uses. It is
    /// clipped when the layer directly above it is clipped, so a clipping
    /// group stays whole (reference C4, C5). Adding never changes the
    /// composite, so nothing is invalidated.
    pub fn add_layer(&mut self, at: Placement) -> Result<LayerId, LayerError> {
        let index = self.placement_index(at)?;
        let raw = self.next_id.ok_or(LayerError::IdsExhausted)?;
        let id = LayerId::new(raw);
        let mut layer = RasterLayer::new(id, self.default_name(), self.depth);
        layer.clipped = self.layers.get(index).is_some_and(|l| l.clipped);
        let cost = layer_bytes(&layer);
        self.next_id = raw.checked_add(1);
        self.layers.insert(index, layer);
        let before = std::mem::replace(&mut self.active, id);
        self.active_part = LayerPart::Pixels;
        let change = Change::Presence { index, layer: None };
        self.record(HistoryAction::AddLayer, vec![change], before, cost);
        Ok(id)
    }

    /// Adds a layer that already has pixels (an imported image) at `at`,
    /// named `name`, and makes it active: one history step, like
    /// [`Document::add_layer`] followed by a stroke would be two. The same
    /// placement and clipping rules apply. Refused when the pixels are not
    /// at the document's depth or address tiles out of range
    /// ([`MAX_TILE_COORD`]). Returns the new id and the tiles to redraw.
    pub fn add_layer_with_pixels(
        &mut self,
        at: Placement,
        name: impl Into<String>,
        pixels: LayerPixels,
    ) -> Result<(LayerId, DirtyTiles), LayerError> {
        let index = self.placement_index(at)?;
        if pixels.depth() != self.depth {
            return Err(LayerError::WrongDepth {
                document: self.depth,
                edit: pixels.depth(),
            });
        }
        if let Some(c) = pixels.tile_coords().find(|&c| !parts::tile_in_range(c)) {
            return Err(LayerError::TileOutOfRange(c));
        }
        let raw = self.next_id.ok_or(LayerError::IdsExhausted)?;
        let id = LayerId::new(raw);
        let mut layer = RasterLayer::new(id, name.into(), self.depth);
        layer.clipped = self.layers.get(index).is_some_and(|l| l.clipped);
        layer.pixels = pixels;
        let cost = layer_bytes(&layer);
        self.next_id = raw.checked_add(1);
        self.layers.insert(index, layer);
        let before = std::mem::replace(&mut self.active, id);
        self.active_part = LayerPart::Pixels;
        let change = Change::Presence { index, layer: None };
        self.record(HistoryAction::ImportLayer, vec![change], before, cost);
        Ok((id, self.group_tiles(index)))
    }

    /// Removes a layer. If it was active, the layer below becomes active,
    /// or the one above when it was the bottom layer. Locked layers and the
    /// last layer cannot be removed.
    pub fn remove_layer(&mut self, id: LayerId) -> Result<DirtyTiles, LayerError> {
        let i = self.require(id)?;
        if self.layers[i].locked {
            return Err(LayerError::Locked(id));
        }
        if self.layers.len() == 1 {
            return Err(LayerError::LastLayer);
        }
        // Clipped layers above it join another group (or none).
        let dirty = self.group_tiles(i);
        let removed = self.layers.remove(i);
        let before = self.active;
        if self.active == id {
            self.active = self.layers[i.saturating_sub(1)].id();
            self.active_part = LayerPart::Pixels;
        }
        let cost = layer_bytes(&removed);
        let change = Change::Presence {
            index: i,
            layer: Some(Box::new(removed)),
        };
        self.record(HistoryAction::RemoveLayer, vec![change], before, cost);
        Ok(dirty)
    }

    /// Moves a layer to `to`. The active layer does not change. Locked
    /// layers can be moved. Only the layer's header moves; its tiles stay
    /// where they are in memory.
    pub fn move_layer(&mut self, id: LayerId, to: Placement) -> Result<DirtyTiles, LayerError> {
        let from = self.require(id)?;
        if let Placement::Above(anchor) | Placement::Below(anchor) = to {
            self.require(anchor)?;
            if anchor == id {
                return Ok(DirtyTiles::new());
            }
        }
        let mut dirty = self.group_tiles(from);
        let layer = self.layers.remove(from);
        let to_index = self
            .placement_index(to)
            .expect("the anchor was checked and is not the moved layer");
        self.layers.insert(to_index, layer);
        if to_index == from {
            return Ok(DirtyTiles::new());
        }
        let change = Change::Move {
            id,
            at: to_index,
            other: from,
        };
        self.record(HistoryAction::MoveLayer, vec![change], self.active, 0);
        dirty.extend(&self.group_tiles(to_index));
        Ok(dirty)
    }

    /// Shows or hides a layer. Allowed on locked layers. Not recorded in
    /// history (like the reference application, parity record U9).
    pub fn set_visible(&mut self, id: LayerId, visible: bool) -> Result<DirtyTiles, LayerError> {
        let i = self.require(id)?;
        let layer = &mut self.layers[i];
        if layer.visible == visible {
            return Ok(DirtyTiles::new());
        }
        layer.visible = visible;
        let dirty = layer.tiles();
        // Not a history step, but what a file keeps (reference F3).
        self.history.unrecorded += 1;
        Ok(dirty)
    }

    /// Sets the layer opacity, clamped to `0..=1`. Refused on locked layers
    /// and for non-finite values.
    ///
    /// Consecutive opacity changes of the same layer merge into one history
    /// step until something else is recorded, undone or redone, or the
    /// active layer changes: dragging a slider, clicking it, or several
    /// drags in a row undo together (reference behaviour, parity record
    /// U14–U17). No timer is involved.
    pub fn set_opacity(&mut self, id: LayerId, opacity: f32) -> Result<DirtyTiles, LayerError> {
        let i = self.require_unlocked(id)?;
        if !opacity.is_finite() {
            return Err(LayerError::InvalidOpacity);
        }
        let opacity = opacity.clamp(0.0, 1.0);
        let old = self.layers[i].opacity;
        if old == opacity {
            return Ok(DirtyTiles::new());
        }
        self.layers[i].opacity = opacity;
        if self.chain_merges(HistoryAction::Opacity, id) {
            // The merged step now leads elsewhere (a new revision).
            self.renew_newest_step();
        } else {
            let change = Change::Opacity {
                layer: id,
                other: old,
            };
            self.record(HistoryAction::Opacity, vec![change], self.active, 0);
            self.history.chain = Some((HistoryAction::Opacity, id));
        }
        Ok(self.layers[i].shown_tiles())
    }

    /// Sets the blend mode. Refused on locked layers.
    pub fn set_blend_mode(
        &mut self,
        id: LayerId,
        mode: BlendMode,
    ) -> Result<DirtyTiles, LayerError> {
        let i = self.require_unlocked(id)?;
        let old = self.layers[i].blend_mode;
        if old == mode {
            return Ok(DirtyTiles::new());
        }
        self.layers[i].blend_mode = mode;
        let change = Change::Blend {
            layer: id,
            other: old,
        };
        self.record(HistoryAction::BlendMode, vec![change], self.active, 0);
        Ok(self.layers[i].shown_tiles())
    }

    /// Renames a layer. Allowed on locked layers (reference, 4B U11 and
    /// 5B RN6). The document stores any name it is given; the empty-name
    /// and length rules of the rename field belong to the UI (5B RN3,
    /// RN5).
    ///
    /// Consecutive renames of the same layer merge into one history step,
    /// like opacity edits, until something else is recorded, undone or
    /// redone, or the active layer changes (reference, parity record 5B,
    /// RN4).
    pub fn set_name(&mut self, id: LayerId, name: impl Into<String>) -> Result<(), LayerError> {
        let i = self.require(id)?;
        let name = name.into();
        if self.layers[i].name == name {
            return Ok(());
        }
        let cost = name.len().max(self.layers[i].name.len());
        let old = std::mem::replace(&mut self.layers[i].name, name);
        if self.chain_merges(HistoryAction::Rename, id) {
            // The merged step keeps the name from before the chain and now
            // leads to a new state; undoing it will hold the new name.
            self.renew_newest_step();
            self.reserve_newest_step(cost);
        } else {
            let change = Change::Name {
                layer: id,
                other: old,
            };
            self.record(HistoryAction::Rename, vec![change], self.active, cost);
            self.history.chain = Some((HistoryAction::Rename, id));
        }
        Ok(())
    }

    /// Locks or unlocks a layer.
    pub fn set_locked(&mut self, id: LayerId, locked: bool) -> Result<(), LayerError> {
        let i = self.require(id)?;
        if self.layers[i].locked == locked {
            return Ok(());
        }
        self.layers[i].locked = locked;
        let change = Change::Locked {
            layer: id,
            other: !locked,
        };
        self.record(HistoryAction::Lock, vec![change], self.active, 0);
        Ok(())
    }

    /// "Lock transparent pixels". Refused on locked layers (reference A6).
    pub fn set_alpha_locked(&mut self, id: LayerId, locked: bool) -> Result<(), LayerError> {
        let i = self.require_unlocked(id)?;
        if self.layers[i].alpha_locked == locked {
            return Ok(());
        }
        self.layers[i].alpha_locked = locked;
        let change = Change::AlphaLocked {
            layer: id,
            other: !locked,
        };
        self.record(HistoryAction::AlphaLock, vec![change], self.active, 0);
        Ok(())
    }

    /// Clips the layer to the one below (or releases it). Refused on
    /// locked layers (reference A6). The clipped layers above it change
    /// group too, so their tiles are invalidated as well.
    pub fn set_clipped(&mut self, id: LayerId, clipped: bool) -> Result<DirtyTiles, LayerError> {
        let i = self.require_unlocked(id)?;
        if self.layers[i].clipped == clipped {
            return Ok(DirtyTiles::new());
        }
        self.layers[i].clipped = clipped;
        let change = Change::Clipped {
            layer: id,
            other: !clipped,
        };
        self.record(HistoryAction::Clip, vec![change], self.active, 0);
        Ok(self.group_tiles(i))
    }

    /// Gives the layer a mask that reveals everything (no tile allocated)
    /// and, when the layer is the active one, makes the mask the edit
    /// target (reference M1). One mask per layer; refused on locked
    /// layers. Nothing changes on screen, so nothing is invalidated.
    pub fn add_mask(&mut self, id: LayerId) -> Result<DirtyTiles, LayerError> {
        let i = self.require_unlocked(id)?;
        if self.layers[i].mask.is_some() {
            return Err(LayerError::HasMask(id));
        }
        self.layers[i].mask = Some(LayerMask::new());
        if self.active == id {
            self.active_part = LayerPart::Mask;
        }
        let change = Change::MaskPresence {
            layer: id,
            mask: None,
        };
        self.record(HistoryAction::AddMask, vec![change], self.active, 0);
        Ok(DirtyTiles::new())
    }

    /// Deletes the layer's mask (it moves into history as is). Refused on
    /// locked layers (reference A6). If the mask was the edit target, the
    /// pixels become the target.
    pub fn remove_mask(&mut self, id: LayerId) -> Result<DirtyTiles, LayerError> {
        let i = self.require_unlocked(id)?;
        let mask = self.layers[i].mask.take().ok_or(LayerError::NoMask(id))?;
        let dirty = self.layers[i].shown_tiles();
        if self.active == id {
            self.active_part = LayerPart::Pixels;
        }
        let cost = mask_bytes(&mask);
        let change = Change::MaskPresence {
            layer: id,
            mask: Some(Box::new(mask)),
        };
        self.record(HistoryAction::RemoveMask, vec![change], self.active, cost);
        Ok(dirty)
    }

    /// Applies or stops applying the mask without deleting it. Allowed on
    /// locked layers, like visibility, but recorded (reference A6, M6).
    pub fn set_mask_enabled(
        &mut self,
        id: LayerId,
        enabled: bool,
    ) -> Result<DirtyTiles, LayerError> {
        let i = self.require(id)?;
        let layer = &mut self.layers[i];
        let mask = layer.mask.as_mut().ok_or(LayerError::NoMask(id))?;
        if mask.enabled == enabled {
            return Ok(DirtyTiles::new());
        }
        mask.enabled = enabled;
        let dirty = layer.shown_tiles();
        let change = Change::MaskEnabled {
            layer: id,
            other: !enabled,
        };
        self.record(HistoryAction::MaskEnabled, vec![change], self.active, 0);
        Ok(dirty)
    }

    /// The only way to change a mask: calls `f` once for each tile in
    /// `tiles` with that tile's mask values **as the user sees them**
    /// (255 shows the layer, 0 hides it; a tile the mask does not store
    /// arrives as all 255) and for no other tile. Afterwards tiles that
    /// reveal everything are released, so the mask stays sparse. Refused
    /// on locked layers and layers without a mask. One call is one history
    /// step (one stroke = one undo), like [`Document::modify_tiles`].
    pub fn modify_mask(
        &mut self,
        id: LayerId,
        tiles: &DirtyTiles,
        mut f: impl FnMut(TileCoord, &mut Tile<u8>),
    ) -> Result<DirtyTiles, LayerError> {
        let i = self.require_unlocked(id)?;
        let layer = &mut self.layers[i];
        let mask = layer.mask.as_mut().ok_or(LayerError::NoMask(id))?;
        let mut previous = Vec::with_capacity(tiles.len());
        let (mut before, mut after) = (0, 0);
        for coord in tiles.iter() {
            let old = mask.hidden.tile(coord).cloned();
            let tile = mask.hidden.tile_or_insert(coord);
            // Stored inverted (hidden amount); `f` sees reveal values.
            invert(tile);
            f(coord, tile);
            invert(tile);
            if tile.is_blank() {
                mask.hidden.remove_tile(coord);
            }
            let new = mask.hidden.tile(coord);
            if old.as_ref() != new {
                before += usize::from(old.is_some());
                after += usize::from(new.is_some());
                previous.push((coord, old));
            }
        }
        let dirty = if layer.visible && mask.enabled {
            tiles.clone()
        } else {
            DirtyTiles::new()
        };
        let cost = before.max(after) * history::MASK_TILE_BYTES
            + previous.len() * std::mem::size_of::<(TileCoord, Option<Tile<u8>>)>();
        let change = Change::MaskTiles {
            layer: id,
            tiles: previous,
        };
        self.record(HistoryAction::PaintMask, vec![change], self.active, cost);
        Ok(dirty)
    }

    /// Whether a stroke may edit `part` of `id`: the layer must exist, be
    /// visible and not be locked; a mask must exist and be enabled
    /// (reference O8, O10, M9, M10).
    pub fn paint_target(&self, id: LayerId, part: LayerPart) -> Result<(), LayerError> {
        let layer = &self.layers[self.require(id)?];
        if !layer.visible {
            return Err(LayerError::Hidden(id));
        }
        if layer.locked {
            return Err(LayerError::Locked(id));
        }
        if part == LayerPart::Mask {
            let mask = layer.mask.as_ref().ok_or(LayerError::NoMask(id))?;
            if !mask.enabled {
                return Err(LayerError::MaskDisabled(id));
            }
        }
        Ok(())
    }

    /// The only way to change a layer's pixels: calls `f` once for each
    /// tile in `tiles` (allocating it if needed) and for no other tile.
    /// A tile allocated here that is still blank afterwards is released
    /// again, so the layer stays sparse. Refused on locked layers, and
    /// when `P` is not the document's storage ([`LayerError::WrongDepth`]):
    /// the edit states the depth it was written for, and the document
    /// never converts tiles behind its back. Callers that serve both
    /// depths choose `P` once from [`Document::depth`]
    /// (`tela_color::with_pixel_type!`).
    ///
    /// One call is one history step (one stroke = one undo). The previous
    /// content of each declared tile that actually changed is kept for
    /// undo, at the document's depth; this is the only tile copy history
    /// ever makes.
    pub fn modify_tiles<P: StoragePixel>(
        &mut self,
        id: LayerId,
        tiles: &DirtyTiles,
        mut f: impl FnMut(TileCoord, &mut Tile<P>),
    ) -> Result<DirtyTiles, LayerError> {
        let i = self.require_unlocked(id)?;
        let wrong = LayerError::WrongDepth {
            document: self.depth,
            edit: P::DEPTH,
        };
        let layer = &mut self.layers[i];
        let pixels = layer.pixels.surface_mut::<P>().ok_or(wrong)?;
        let mut previous = Vec::with_capacity(tiles.len());
        let (mut before, mut after) = (0, 0);
        for coord in tiles.iter() {
            let old = pixels.tile(coord).cloned();
            f(coord, pixels.tile_or_insert(coord));
            if old.is_none() && pixels.tile(coord).is_some_and(Tile::is_blank) {
                pixels.remove_tile(coord);
            }
            let new = pixels.tile(coord);
            if old.as_ref() != new {
                before += usize::from(old.is_some());
                after += usize::from(new.is_some());
                previous.push((coord, old));
            }
        }
        let dirty = if layer.visible {
            tiles.clone()
        } else {
            DirtyTiles::new()
        };
        let cost = before.max(after) * tile_bytes(self.depth)
            + previous.len() * std::mem::size_of::<(TileCoord, Option<Tile<P>>)>();
        let change = Change::Tiles {
            layer: id,
            tiles: TileBackup::new(previous),
        };
        self.record(HistoryAction::Paint, vec![change], self.active, cost);
        Ok(dirty)
    }

    /// Removes every pixel of a layer. Refused on locked layers. The
    /// surface moves into history as is (no copy); clearing an empty layer
    /// records nothing.
    pub fn clear_layer(&mut self, id: LayerId) -> Result<DirtyTiles, LayerError> {
        let i = self.require_unlocked(id)?;
        if self.layers[i].pixels.tile_count() == 0 {
            return Ok(DirtyTiles::new());
        }
        let dirty = self.layers[i].shown_tiles();
        let empty = LayerPixels::new(self.depth);
        let pixels = std::mem::replace(&mut self.layers[i].pixels, empty);
        let cost = pixels_bytes(&pixels);
        let change = Change::Surface { layer: id, pixels };
        self.record(HistoryAction::ClearLayer, vec![change], self.active, cost);
        Ok(dirty)
    }

    /// Converts every layer's pixels to `depth`: 8 → 16 exactly
    /// (`v · 257`, the composite does not change at all), 16 → 8 to the
    /// nearest 8-bit level (`(v + 128) / 257`). Applies to hidden and
    /// locked layers too: the depth is a property of the document, not of
    /// a layer. Masks are 8-bit at every depth and do not change; ids,
    /// order, properties and the sparse tile layout are kept.
    ///
    /// One history step. It holds the previous pixels of every layer
    /// (moved, not copied); its cost is the larger of the two depths'
    /// buffers, so 8 → 16 reserves the new 16-bit size. Converting to the
    /// current depth does nothing and records nothing. Returns the tiles
    /// where a visible layer has pixels.
    pub fn convert_depth(&mut self, depth: PixelDepth) -> DirtyTiles {
        if depth == self.depth {
            return DirtyTiles::new();
        }
        let mut dirty = DirtyTiles::new();
        let mut held = Vec::with_capacity(self.layers.len());
        let mut cost = 0;
        for layer in &mut self.layers {
            let converted = layer.pixels.converted(depth);
            cost += pixels_bytes(&converted).max(pixels_bytes(&layer.pixels));
            let old = std::mem::replace(&mut layer.pixels, converted);
            dirty.extend(&layer.shown_tiles());
            held.push((layer.id(), old));
        }
        let other = std::mem::replace(&mut self.depth, depth);
        let change = Change::Depth {
            layers: held,
            other,
        };
        self.record(HistoryAction::ConvertDepth, vec![change], self.active, cost);
        dirty
    }

    /// Every tile where a visible layer has pixels (what a full redraw
    /// must compose).
    pub fn content_tiles(&self) -> DirtyTiles {
        let mut tiles = DirtyTiles::new();
        for layer in self.layers.iter().filter(|l| l.visible) {
            for c in layer.pixels.tile_coords() {
                tiles.mark(c);
            }
        }
        tiles
    }

    fn require(&self, id: LayerId) -> Result<usize, LayerError> {
        self.index_of(id).ok_or(LayerError::NotFound(id))
    }

    fn active_index(&self) -> usize {
        self.index_of(self.active)
            .expect("the active layer always exists")
    }

    /// Shown tiles of the layer at `index` and of the clipped layers
    /// directly above it: what changes on screen when that layer enters or
    /// leaves a place in the stack (the clipped layers change base).
    fn group_tiles(&self, index: usize) -> DirtyTiles {
        let mut dirty = self.layers[index].shown_tiles();
        for l in self.layers[index + 1..].iter().take_while(|l| l.clipped) {
            dirty.extend(&l.shown_tiles());
        }
        dirty
    }

    fn require_unlocked(&self, id: LayerId) -> Result<usize, LayerError> {
        let i = self.require(id)?;
        if self.layers[i].locked {
            return Err(LayerError::Locked(id));
        }
        Ok(i)
    }

    /// Index in the current stack where a layer placed at `at` goes.
    fn placement_index(&self, at: Placement) -> Result<usize, LayerError> {
        Ok(match at {
            Placement::Above(anchor) => self.require(anchor)? + 1,
            Placement::Below(anchor) => self.require(anchor)?,
            Placement::Top => self.layers.len(),
            Placement::Bottom => 0,
        })
    }

    /// "Layer N" with the smallest N not used by an existing layer name.
    fn default_name(&self) -> String {
        let mut used: Vec<u32> = self
            .layers
            .iter()
            .filter_map(|l| default_name_number(&l.name))
            .collect();
        used.sort_unstable();
        used.dedup();
        let mut n = 1u32;
        for u in used {
            if u != n {
                break;
            }
            n = n.saturating_add(1);
        }
        format!("{DEFAULT_NAME} {n}")
    }

    #[cfg(test)]
    pub(crate) fn set_next_id_for_test(&mut self, next: Option<NonZeroU32>) {
        self.next_id = next;
    }

    /// The document's whole content (depth, layers in order with ids,
    /// properties, every tile's stored values and masks, and the active
    /// layer), excluding history.
    /// Equal digests mean equal documents, up to 64-bit hash collisions of
    /// tile contents.
    #[cfg(test)]
    pub(crate) fn digest(&self) -> Digest {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let layers = self
            .layers
            .iter()
            .map(|l| {
                let mut tiles: Vec<(TileCoord, u64)> = l
                    .pixels
                    .tile_coords()
                    .map(|c| (c, l.pixels.hash_tile(c).unwrap()))
                    .collect();
                tiles.sort_unstable();
                let mask = l.mask.as_ref().map(|m| {
                    let mut tiles: Vec<(TileCoord, u64)> = m
                        .hidden
                        .tile_coords()
                        .map(|c| {
                            let mut h = DefaultHasher::new();
                            m.hidden.tile(c).unwrap().pixels().hash(&mut h);
                            (c, h.finish())
                        })
                        .collect();
                    tiles.sort_unstable();
                    (m.enabled, tiles)
                });
                LayerDigest {
                    id: l.id().get(),
                    name: l.name.clone(),
                    visible: l.visible,
                    opacity_bits: l.opacity.to_bits(),
                    blend_mode: l.blend_mode,
                    locked: l.locked,
                    alpha_locked: l.alpha_locked,
                    clipped: l.clipped,
                    tiles,
                    mask,
                }
            })
            .collect();
        Digest {
            depth: self.depth,
            active: self.active.get(),
            layers,
            selection: self.selection.as_ref().map(|s| {
                use std::hash::{DefaultHasher, Hash, Hasher};
                let tiles = s
                    .sorted_tiles()
                    .into_iter()
                    .map(|(c, t)| {
                        let mut h = DefaultHasher::new();
                        t.pixels().hash(&mut h);
                        (c, h.finish())
                    })
                    .collect();
                (s.is_inverted_storage(), tiles)
            }),
        }
    }

    /// Checks every structural invariant; used by tests after each step.
    #[cfg(test)]
    pub(crate) fn check_invariants(&self) {
        assert!(!self.layers.is_empty(), "L3: at least one layer");
        let mut ids: Vec<u32> = self.layers.iter().map(|l| l.id().get()).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "L1: ids are unique");
        if let Some(next) = self.next_id {
            assert!(ids.iter().all(|&i| i < next.get()), "L2: next id is fresh");
        }
        assert!(self.index_of(self.active).is_some(), "L4: active exists");
        for l in &self.layers {
            assert!((0.0..=1.0).contains(&l.opacity), "L5: opacity in range");
            assert_eq!(l.pixels.depth(), self.depth, "DP1: one depth per document");
            if let Some(m) = &l.mask {
                for c in m.hidden.tile_coords() {
                    assert!(
                        !m.hidden.tile(c).unwrap().is_blank(),
                        "M3: masks keep no tile that reveals everything"
                    );
                }
            }
        }
        assert!(
            self.active_part == LayerPart::Pixels
                || self.layers[self.active_index()].mask.is_some(),
            "the mask target needs a mask"
        );
        if let Some(s) = &self.selection {
            assert!(!s.is_empty(), "SEL: an empty selection is no selection");
            assert_eq!(s.canvas(), self.bounds(), "SEL: clipped to this canvas");
            s.check();
        }
    }
}

/// See [`Document::digest`].
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Digest {
    /// Storage depth; tile hashes are of the stored values at this depth.
    pub depth: PixelDepth,
    pub active: u32,
    pub layers: Vec<LayerDigest>,
    /// The selection: inverted storage and stored tiles.
    pub selection: Option<(bool, Vec<(TileCoord, u64)>)>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayerDigest {
    pub id: u32,
    pub name: String,
    pub visible: bool,
    pub opacity_bits: u32,
    pub blend_mode: BlendMode,
    pub locked: bool,
    pub alpha_locked: bool,
    pub clipped: bool,
    /// Sorted by coordinate; absent tiles are absent here too.
    pub tiles: Vec<(TileCoord, u64)>,
    /// Enabled flag and stored (hidden-amount) tiles, sorted.
    pub mask: Option<(bool, Vec<(TileCoord, u64)>)>,
}

/// Turns stored hidden amounts into reveal values and back.
fn invert(tile: &mut Tile<u8>) {
    for v in tile.pixels_mut() {
        *v = 255 - *v;
    }
}

/// `N` if `name` is exactly a default name "Layer N" (N ≥ 1, no leading
/// zeros), so renamed layers can keep their number reserved.
fn default_name_number(name: &str) -> Option<u32> {
    let digits = name.strip_prefix(DEFAULT_NAME)?.strip_prefix(' ')?;
    if digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

impl RasterLayer {
    /// Every allocated tile.
    fn tiles(&self) -> DirtyTiles {
        let mut tiles = DirtyTiles::new();
        for c in self.pixels.tile_coords() {
            tiles.mark(c);
        }
        tiles
    }

    /// The tiles whose composite this layer affects (none when hidden).
    fn shown_tiles(&self) -> DirtyTiles {
        if self.visible {
            self.tiles()
        } else {
            DirtyTiles::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile_with_pixel() -> DirtyTiles {
        let mut t = DirtyTiles::new();
        t.mark(TileCoord { x: 0, y: 0 });
        t
    }

    /// Paints one opaque pixel at (1, 1) into `id`.
    fn paint(d: &mut Document, id: LayerId) -> DirtyTiles {
        d.modify_tiles(id, &tile_with_pixel(), |_, t| *t.get_mut(1, 1) = [255u8; 4])
            .unwrap()
    }

    fn names(d: &Document) -> Vec<&str> {
        d.layers().map(RasterLayer::name).collect()
    }

    #[test]
    fn new_document_has_one_active_layer() {
        let d = Document::new(64, 64);
        assert_eq!(d.layer_count(), 1);
        assert_eq!(names(&d), ["Layer 1"]);
        assert_eq!(d.layer(d.active_layer()).unwrap().name(), "Layer 1");
        d.check_invariants();
    }

    #[test]
    fn new_layers_go_where_placed_and_become_active() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        let l2 = d.add_layer(Placement::Above(l1)).unwrap();
        assert_eq!(d.active_layer(), l2);
        let l3 = d.add_layer(Placement::Above(l1)).unwrap();
        // Inserted between 1 and 2, like the reference (parity record O1).
        assert_eq!(names(&d), ["Layer 1", "Layer 3", "Layer 2"]);
        assert_eq!(d.active_layer(), l3);
        d.add_layer(Placement::Bottom).unwrap();
        d.add_layer(Placement::Top).unwrap();
        assert_eq!(
            names(&d),
            ["Layer 4", "Layer 1", "Layer 3", "Layer 2", "Layer 5"]
        );
        d.check_invariants();
    }

    #[test]
    fn default_names_reuse_the_smallest_free_number() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        let l2 = d.add_layer(Placement::Top).unwrap();
        d.add_layer(Placement::Top).unwrap();
        d.remove_layer(l2).unwrap();
        let l = d.add_layer(Placement::Top).unwrap();
        assert_eq!(d.layer(l).unwrap().name(), "Layer 2");
        d.set_name(l1, "Sketch").unwrap();
        let l = d.add_layer(Placement::Top).unwrap();
        assert_eq!(d.layer(l).unwrap().name(), "Layer 1");
        // Only exact default names reserve a number.
        for odd in ["Layer 05", "Layer", "Layer x", "layer 6", "Layer  6"] {
            assert_eq!(default_name_number(odd), None, "{odd}");
        }
        assert_eq!(default_name_number("Layer 6"), Some(6));
    }

    #[test]
    fn ids_are_never_reused() {
        let mut d = Document::new(64, 64);
        let a = d.add_layer(Placement::Top).unwrap();
        d.remove_layer(a).unwrap();
        let b = d.add_layer(Placement::Top).unwrap();
        assert_ne!(a, b);
        assert!(b.get() > a.get());
        assert_eq!(d.find_layer(a.get()), None);
        assert_eq!(d.find_layer(b.get()), Some(b));
    }

    #[test]
    fn ids_run_out_instead_of_wrapping() {
        let mut d = Document::new(64, 64);
        d.set_next_id_for_test(NonZeroU32::new(u32::MAX));
        let last = d.add_layer(Placement::Top).unwrap();
        assert_eq!(last.get(), u32::MAX);
        let before = d.layer_count();
        assert_eq!(
            d.add_layer(Placement::Top).err(),
            Some(LayerError::IdsExhausted)
        );
        assert_eq!(d.layer_count(), before);
        d.check_invariants();
    }

    #[test]
    fn removing_the_active_layer_activates_the_one_below_else_above() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        let l2 = d.add_layer(Placement::Top).unwrap();
        let l3 = d.add_layer(Placement::Top).unwrap();
        // Middle → below.
        d.set_active_layer(l2).unwrap();
        d.remove_layer(l2).unwrap();
        assert_eq!(d.active_layer(), l1);
        // Bottom → the one above (there is no paper layer below).
        d.remove_layer(l1).unwrap();
        assert_eq!(d.active_layer(), l3);
        // Last → refused.
        assert_eq!(d.remove_layer(l3).err(), Some(LayerError::LastLayer));
        d.check_invariants();
    }

    #[test]
    fn removing_another_layer_keeps_the_selection() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        let l2 = d.add_layer(Placement::Top).unwrap();
        d.remove_layer(l1).unwrap();
        assert_eq!(d.active_layer(), l2);
    }

    #[test]
    fn moving_keeps_identity_selection_and_tile_memory() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        paint(&mut d, l1);
        let l2 = d.add_layer(Placement::Top).unwrap();
        let l3 = d.add_layer(Placement::Top).unwrap();
        let ptr = d
            .layer(l1)
            .unwrap()
            .pixels()
            .rgba8()
            .unwrap()
            .tile(TileCoord { x: 0, y: 0 })
            .unwrap()
            .pixels()
            .as_ptr();
        let dirty = d.move_layer(l1, Placement::Above(l3)).unwrap();
        assert_eq!(
            d.layers().map(RasterLayer::id).collect::<Vec<_>>(),
            [l2, l3, l1]
        );
        assert_eq!(d.active_layer(), l3);
        assert!(dirty.contains(TileCoord { x: 0, y: 0 }));
        let moved = d
            .layer(l1)
            .unwrap()
            .pixels()
            .rgba8()
            .unwrap()
            .tile(TileCoord { x: 0, y: 0 })
            .unwrap()
            .pixels()
            .as_ptr();
        assert_eq!(ptr, moved, "tiles are not copied when moving");
        // Moving to where it already is, or relative to itself, is a no-op.
        assert!(d.move_layer(l1, Placement::Top).unwrap().is_empty());
        assert!(d.move_layer(l1, Placement::Below(l1)).unwrap().is_empty());
        d.move_layer(l1, Placement::Below(l2)).unwrap();
        assert_eq!(d.index_of(l1), Some(0));
        assert_eq!(d.layer_above(l1), Some(l2));
        assert_eq!(d.layer_below(l1), None);
        d.check_invariants();
    }

    #[test]
    fn unknown_ids_are_errors_and_change_nothing() {
        let mut d = Document::new(64, 64);
        let gone = d.add_layer(Placement::Top).unwrap();
        d.remove_layer(gone).unwrap();
        let l1 = d.active_layer();
        let err = Some(LayerError::NotFound(gone));
        assert_eq!(d.set_active_layer(gone).err(), err);
        assert_eq!(d.add_layer(Placement::Above(gone)).err(), err);
        assert_eq!(d.move_layer(l1, Placement::Below(gone)).err(), err);
        assert_eq!(d.move_layer(gone, Placement::Top).err(), err);
        assert_eq!(d.remove_layer(gone).err(), err);
        assert_eq!(d.set_visible(gone, false).err(), err);
        assert_eq!(d.set_opacity(gone, 0.5).err(), err);
        assert_eq!(d.set_name(gone, "x").err(), err);
        assert_eq!(d.paint_target(gone, LayerPart::Pixels).err(), err);
        assert_eq!(d.clear_layer(gone).err(), err);
        assert_eq!(
            d.modify_tiles(gone, &tile_with_pixel(), |_, _: &mut Tile<[u8; 4]>| ())
                .err(),
            err
        );
        assert_eq!(d.layer_count(), 1);
        assert_eq!(d.active_layer(), l1);
        d.check_invariants();
    }

    #[test]
    fn lock_blocks_pixels_opacity_blend_and_removal_only() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        let l2 = d.add_layer(Placement::Top).unwrap();
        paint(&mut d, l2);
        d.set_locked(l2, true).unwrap();
        let locked = Some(LayerError::Locked(l2));
        assert_eq!(d.paint_target(l2, LayerPart::Pixels).err(), locked);
        assert_eq!(
            d.modify_tiles(l2, &tile_with_pixel(), |_, _: &mut Tile<[u8; 4]>| ())
                .err(),
            locked
        );
        assert_eq!(d.clear_layer(l2).err(), locked);
        assert_eq!(d.set_opacity(l2, 0.5).err(), locked);
        assert_eq!(d.set_blend_mode(l2, BlendMode::Normal).err(), locked);
        assert_eq!(d.remove_layer(l2).err(), locked);
        // Reference record O9–O11: still selectable, hideable, movable.
        d.set_active_layer(l2).unwrap();
        assert!(!d.set_visible(l2, false).unwrap().is_empty());
        d.move_layer(l2, Placement::Below(l1)).unwrap();
        d.set_name(l2, "Inks").unwrap();
        assert_eq!(d.layer(l2).unwrap().opacity(), 1.0);
        assert!(d.layer(l2).unwrap().pixels().tile_count() == 1);
        d.set_locked(l2, false).unwrap();
        d.remove_layer(l2).unwrap();
        d.check_invariants();
    }

    #[test]
    fn hidden_layers_can_be_active_but_not_painted() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        d.set_visible(l1, false).unwrap();
        assert_eq!(d.active_layer(), l1);
        assert_eq!(
            d.paint_target(l1, LayerPart::Pixels).err(),
            Some(LayerError::Hidden(l1))
        );
        d.set_visible(l1, true).unwrap();
        assert_eq!(d.paint_target(l1, LayerPart::Pixels), Ok(()));
    }

    #[test]
    fn opacity_is_clamped_and_must_be_finite() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        d.set_opacity(l1, 1.7).unwrap();
        assert_eq!(d.layer(l1).unwrap().opacity(), 1.0);
        d.set_opacity(l1, -2.0).unwrap();
        assert_eq!(d.layer(l1).unwrap().opacity(), 0.0);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                d.set_opacity(l1, bad).err(),
                Some(LayerError::InvalidOpacity)
            );
        }
        assert_eq!(d.layer(l1).unwrap().opacity(), 0.0);
    }

    #[test]
    fn invalidation_follows_visibility_and_content() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        assert!(d.set_opacity(l1, 0.5).unwrap().is_empty(), "no content");
        assert_eq!(paint(&mut d, l1).len(), 1);
        assert_eq!(d.set_opacity(l1, 0.25).unwrap().len(), 1);
        assert!(d.set_opacity(l1, 0.25).unwrap().is_empty(), "unchanged");
        assert_eq!(d.set_visible(l1, false).unwrap().len(), 1);
        assert!(d.set_opacity(l1, 1.0).unwrap().is_empty(), "hidden");
        assert!(paint(&mut d, l1).is_empty(), "hidden");
        assert!(d.content_tiles().is_empty());
        d.set_visible(l1, true).unwrap();
        assert_eq!(d.content_tiles().len(), 1);
        assert_eq!(d.clear_layer(l1).unwrap().len(), 1);
        assert_eq!(d.layer(l1).unwrap().pixels().tile_count(), 0);
    }

    #[test]
    fn modify_tiles_touches_only_declared_tiles_and_stays_sparse() {
        let mut d = Document::new(1024, 1024);
        let l1 = d.active_layer();
        let mut declared = DirtyTiles::new();
        declared.mark(TileCoord { x: 0, y: 0 });
        declared.mark(TileCoord { x: 2, y: 1 });
        let mut seen = Vec::new();
        d.modify_tiles(l1, &declared, |c, t| {
            seen.push(c);
            if c.x == 0 {
                *t.get_mut(0, 0) = [9u8; 4];
            }
        })
        .unwrap();
        assert_eq!(seen, [TileCoord { x: 0, y: 0 }, TileCoord { x: 2, y: 1 }]);
        let px = d.layer(l1).unwrap().pixels().rgba8().unwrap();
        assert_eq!(px.tile_count(), 1, "the untouched new tile was released");
        assert_eq!(px.pixel(0, 0), [9; 4]);
    }

    #[test]
    fn masks_start_revealing_everything_and_become_the_target() {
        let mut d = Document::new(64, 64);
        let l1 = d.active_layer();
        let l2 = d.add_layer(Placement::Top).unwrap();
        assert_eq!(d.active_part(), LayerPart::Pixels);
        assert!(
            d.add_mask(l2).unwrap().is_empty(),
            "nothing changes on screen"
        );
        let mask = d.layer(l2).unwrap().mask().unwrap();
        assert!(mask.enabled());
        assert_eq!(mask.tile_count(), 0, "M3: no tile allocated");
        assert_eq!(mask.value(5, 5), 255);
        assert_eq!(d.active_part(), LayerPart::Mask, "reference M1");
        assert_eq!(d.add_mask(l2).err(), Some(LayerError::HasMask(l2)));
        // Another layer's mask does not take the target.
        d.add_mask(l1).unwrap();
        assert_eq!(d.active_part(), LayerPart::Mask);
        // Selecting a layer targets its pixels (reference M11).
        d.set_active_layer(l2).unwrap();
        assert_eq!(d.active_part(), LayerPart::Pixels);
        d.set_active_part(LayerPart::Mask).unwrap();
        d.remove_mask(l2).unwrap();
        assert_eq!(d.active_part(), LayerPart::Pixels);
        assert_eq!(
            d.set_active_part(LayerPart::Mask).err(),
            Some(LayerError::NoMask(l2))
        );
        d.check_invariants();
    }

    #[test]
    fn modify_mask_shows_reveal_values_and_keeps_the_mask_sparse() {
        let mut d = Document::new(1024, 1024);
        let l = d.active_layer();
        assert_eq!(
            d.modify_mask(l, &tile_with_pixel(), |_, _| ()).err(),
            Some(LayerError::NoMask(l))
        );
        d.add_mask(l).unwrap();
        let mut declared = tile_with_pixel();
        declared.mark(TileCoord { x: 1, y: 0 });
        d.modify_mask(l, &declared, |c, t| {
            assert!(t.pixels().iter().all(|&v| v == 255), "absent = reveal");
            if c.x == 0 {
                *t.get_mut(3, 3) = 40;
            }
        })
        .unwrap();
        let m = d.layer(l).unwrap().mask().unwrap();
        assert_eq!(m.value(3, 3), 40);
        assert_eq!(m.value(4, 3), 255);
        assert_eq!(m.tile_count(), 1, "untouched tile released");
        // Revealing everything again releases the tile.
        d.modify_mask(l, &tile_with_pixel(), |_, t| *t.get_mut(3, 3) = 255)
            .unwrap();
        assert_eq!(d.layer(l).unwrap().mask().unwrap().tile_count(), 0, "M3");
        d.check_invariants();
    }

    #[test]
    fn lock_refuses_mask_clip_and_alpha_lock_edits_but_not_mask_enable() {
        let mut d = Document::new(64, 64);
        let l = d.active_layer();
        d.add_mask(l).unwrap();
        d.set_locked(l, true).unwrap();
        let locked = Some(LayerError::Locked(l));
        assert_eq!(d.remove_mask(l).err(), locked);
        assert_eq!(
            d.modify_mask(l, &tile_with_pixel(), |_, _| ()).err(),
            locked
        );
        assert_eq!(d.set_clipped(l, true).err(), locked);
        assert_eq!(d.set_alpha_locked(l, true).err(), locked);
        assert_eq!(
            d.paint_target(l, LayerPart::Mask).err(),
            locked,
            "reference A6"
        );
        d.set_mask_enabled(l, false).unwrap();
        assert!(!d.layer(l).unwrap().mask().unwrap().enabled());
        let l2 = d.add_layer(Placement::Top).unwrap();
        d.set_locked(l2, true).unwrap();
        assert_eq!(d.add_mask(l2).err(), Some(LayerError::Locked(l2)));
        d.check_invariants();
    }

    #[test]
    fn mask_strokes_need_a_visible_layer_and_an_enabled_mask() {
        let mut d = Document::new(64, 64);
        let l = d.active_layer();
        assert_eq!(
            d.paint_target(l, LayerPart::Mask).err(),
            Some(LayerError::NoMask(l))
        );
        d.add_mask(l).unwrap();
        assert_eq!(d.paint_target(l, LayerPart::Mask), Ok(()));
        d.set_mask_enabled(l, false).unwrap();
        assert_eq!(
            d.paint_target(l, LayerPart::Mask).err(),
            Some(LayerError::MaskDisabled(l)),
            "reference M10"
        );
        assert_eq!(d.paint_target(l, LayerPart::Pixels), Ok(()));
        d.set_mask_enabled(l, true).unwrap();
        d.set_visible(l, false).unwrap();
        assert_eq!(
            d.paint_target(l, LayerPart::Mask).err(),
            Some(LayerError::Hidden(l)),
            "reference M9"
        );
    }

    #[test]
    fn new_layers_inside_a_clipping_group_are_clipped() {
        let mut d = Document::new(64, 64);
        let base = d.active_layer();
        let clip = d.add_layer(Placement::Top).unwrap();
        d.set_clipped(clip, true).unwrap();
        // Reference C4: between the base and a clipped layer.
        let inside = d.add_layer(Placement::Above(base)).unwrap();
        assert!(d.layer(inside).unwrap().clipped());
        // Reference C5: above the topmost clipped layer.
        let above = d.add_layer(Placement::Top).unwrap();
        assert!(!d.layer(above).unwrap().clipped());
    }

    #[test]
    fn documents_store_one_depth_and_refuse_edits_for_the_other() {
        let mut d = Document::with_depth(64, 64, PixelDepth::Rgba16);
        assert_eq!(d.depth(), PixelDepth::Rgba16);
        assert_eq!(Document::new(8, 8).depth(), PixelDepth::Rgba8, "D12");
        let l = d.active_layer();
        let before = d.digest();
        let wrong = d.modify_tiles(l, &tile_with_pixel(), |_, t: &mut Tile<[u8; 4]>| {
            *t.get_mut(1, 1) = [255; 4]
        });
        assert_eq!(
            wrong.err(),
            Some(LayerError::WrongDepth {
                document: PixelDepth::Rgba16,
                edit: PixelDepth::Rgba8
            })
        );
        assert_eq!(d.digest(), before, "L6: refused, nothing changed");
        assert_eq!(d.history_len(), 0, "and nothing recorded");
        d.modify_tiles(l, &tile_with_pixel(), |_, t: &mut Tile<[u16; 4]>| {
            *t.get_mut(1, 1) = [1000, 2000, 3000, 4000]
        })
        .unwrap();
        let px = d.layer(l).unwrap().pixels();
        assert_eq!(px.rgba16().unwrap().pixel(1, 1), [1000, 2000, 3000, 4000]);
        assert!(px.rgba8().is_none());
        assert_eq!(px.pixel_rgba8(1, 1), [4, 8, 12, 16], "nearest 8-bit level");
        let l2 = d.add_layer(Placement::Top).unwrap();
        assert_eq!(d.layer(l2).unwrap().pixels().depth(), PixelDepth::Rgba16);
        d.check_invariants();
    }

    /// A small stack exercising every layer property, at 8 bits.
    fn varied_document() -> (Document, [LayerId; 3]) {
        let mut d = Document::new(600, 300);
        let a = d.active_layer();
        let b = d.add_layer(Placement::Top).unwrap();
        let c = d.add_layer(Placement::Top).unwrap();
        let mut tiles = DirtyTiles::new();
        tiles.mark(TileCoord { x: 0, y: 0 });
        tiles.mark(TileCoord { x: 2, y: 1 });
        for (k, id) in [a, b, c].into_iter().enumerate() {
            d.modify_tiles(id, &tiles, |_, t: &mut Tile<[u8; 4]>| {
                for (i, p) in t.pixels_mut().iter_mut().enumerate() {
                    let v = ((i * (k + 3)) % 256) as u8;
                    *p = [v / 2, v / 3, v / 4, v];
                }
            })
            .unwrap();
        }
        d.add_mask(b).unwrap();
        d.modify_mask(b, &tiles, |_, t| *t.get_mut(5, 5) = 17)
            .unwrap();
        d.set_clipped(c, true).unwrap();
        d.set_blend_mode(c, BlendMode::Multiply).unwrap();
        d.set_opacity(b, 0.6).unwrap();
        d.set_alpha_locked(a, true).unwrap();
        d.set_locked(a, true).unwrap();
        d.set_visible(b, false).unwrap();
        (d, [a, b, c])
    }

    fn composite(d: &Document) -> Vec<Option<Vec<[u8; 4]>>> {
        let mut coords: Vec<TileCoord> = d.content_tiles().iter().collect();
        coords.push(TileCoord { x: 1, y: 0 });
        coords.iter().map(|&c| d.compose_tile(c, None)).collect()
    }

    #[test]
    fn widening_keeps_structure_composite_and_masks_and_is_one_step() {
        let (mut d, ids) = varied_document();
        let eight = d.digest();
        let shown = composite(&d);
        let (steps, reserved) = (d.history_len(), d.history_reserved_bytes());
        let tiles: usize = d.layers().map(|l| l.pixels().tile_count()).sum();
        let dirty = d.convert_depth(PixelDepth::Rgba16);
        assert_eq!(d.depth(), PixelDepth::Rgba16);
        assert_eq!(dirty.len(), 2, "the tiles where visible layers draw");
        assert_eq!(d.history_len(), steps + 1, "one step");
        assert_eq!(d.next_undo(), Some(HistoryAction::ConvertDepth));
        let sixteen = d.digest();
        // Ids, order, properties, masks and the sparse layout are kept.
        for (a, b) in eight.layers.iter().zip(&sixteen.layers) {
            assert_eq!(
                (a.id, &a.name, a.visible, a.opacity_bits),
                (b.id, &b.name, b.visible, b.opacity_bits)
            );
            assert_eq!(
                (a.blend_mode, a.locked, a.alpha_locked, a.clipped),
                (b.blend_mode, b.locked, b.alpha_locked, b.clipped)
            );
            assert_eq!(a.mask, b.mask, "masks are 8-bit at every depth");
            let coords = |l: &LayerDigest| l.tiles.iter().map(|t| t.0).collect::<Vec<_>>();
            assert_eq!(coords(a), coords(b));
        }
        assert_eq!(sixteen.active, eight.active);
        // Exact widening: same composite, bit for bit, locked layer too.
        assert_eq!(composite(&d), shown);
        for id in ids {
            let p = d.layer(id).unwrap().pixels();
            assert_eq!(
                p.rgba16().unwrap().pixel(7, 3),
                p.pixel_rgba8(7, 3).map(tela_color::widen8)
            );
        }
        // The step reserves the 16-bit size of every converted tile.
        let added = d.history_reserved_bytes() - reserved;
        assert!(added >= tiles * tile_bytes(PixelDepth::Rgba16), "{added}");
        assert!(d.history_bytes() <= d.history_reserved_bytes());
        // Converting to the current depth does nothing.
        assert!(d.convert_depth(PixelDepth::Rgba16).is_empty());
        assert_eq!(d.history_len(), steps + 1);
        // Undo and redo are bit-exact (H1 at both depths).
        d.undo().unwrap();
        assert_eq!(d.digest(), eight);
        d.check_invariants();
        d.redo().unwrap();
        assert_eq!(d.digest(), sixteen);
        d.check_invariants();
        // 8 → 16 → 8 is the identity for 8-bit content, as a new step.
        d.convert_depth(PixelDepth::Rgba8);
        assert_eq!(d.digest(), eight);
        assert_eq!(d.history_len(), steps + 2);
    }

    #[test]
    fn narrowing_rounds_to_8_bits_and_undo_brings_the_precision_back() {
        let mut d = Document::with_depth(64, 64, PixelDepth::Rgba16);
        let l = d.active_layer();
        d.modify_tiles(l, &tile_with_pixel(), |_, t: &mut Tile<[u16; 4]>| {
            *t.get_mut(1, 1) = [128, 129, 32896 + 128, 65535];
        })
        .unwrap();
        let sixteen = d.digest();
        d.convert_depth(PixelDepth::Rgba8);
        assert_eq!(
            d.layer(l).unwrap().pixels().rgba8().unwrap().pixel(1, 1),
            [0, 1, 128, 255]
        );
        // The eight-bit layer is editable with 8-bit edits now.
        let edit = d.modify_tiles(l, &tile_with_pixel(), |_, t: &mut Tile<[u8; 4]>| {
            *t.get_mut(2, 2) = [9; 4]
        });
        assert!(edit.is_ok());
        d.undo().unwrap();
        d.undo().unwrap();
        assert_eq!(d.digest(), sixteen, "the 16-bit values come back exactly");
        d.check_invariants();
    }

    #[test]
    fn group_changes_invalidate_the_clipped_layers_too() {
        let mut d = Document::new(1024, 256);
        let base = d.active_layer();
        let a = TileCoord { x: 0, y: 0 };
        let b = TileCoord { x: 1, y: 0 };
        let c = TileCoord { x: 2, y: 0 };
        let paint_at = |d: &mut Document, id: LayerId, coord: TileCoord| {
            let mut t = DirtyTiles::new();
            t.mark(coord);
            d.modify_tiles(id, &t, |_, t| *t.get_mut(1, 1) = [255u8; 4])
                .unwrap();
        };
        paint_at(&mut d, base, a);
        let mid = d.add_layer(Placement::Top).unwrap();
        paint_at(&mut d, mid, b);
        let top = d.add_layer(Placement::Top).unwrap();
        paint_at(&mut d, top, c);
        d.set_clipped(top, true).unwrap();
        // Clipping `mid` moves `top` into the base's group.
        let dirty = d.set_clipped(mid, true).unwrap();
        assert!(dirty.contains(b) && dirty.contains(c));
        d.set_clipped(mid, false).unwrap();
        // Removing `mid` leaves `top` clipped to the base.
        let dirty = d.remove_layer(mid).unwrap();
        assert!(dirty.contains(b) && dirty.contains(c));
        let dirty = d.undo().unwrap();
        assert!(dirty.contains(b) && dirty.contains(c));
        // Moving `mid` away re-parents `top` as well.
        let dirty = d.move_layer(mid, Placement::Bottom).unwrap();
        assert!(dirty.contains(b) && dirty.contains(c));
        let dirty = d.move_layer(mid, Placement::Above(base)).unwrap();
        assert!(dirty.contains(b) && dirty.contains(c));
    }
}
