mod history;

use std::num::NonZeroU32;

use tela_canvas::{DirtyTiles, IRect, Tile, TileCoord};
use tela_color::Rgba;

use self::history::{Change, History, layer_bytes};
pub use self::history::{HistoryAction, HistoryLimits};
use crate::{BlendMode, LayerError, LayerId, RasterLayer};

/// Prefix of the names the document gives new layers ("Layer 3").
const DEFAULT_NAME: &str = "Layer";

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
    /// Bottom to top; never empty.
    layers: Vec<RasterLayer>,
    /// Always the id of a layer in `layers`.
    active: LayerId,
    /// Next id to hand out; greater than every id allocated so far.
    /// `None` once `u32::MAX` has been used.
    next_id: Option<NonZeroU32>,
    history: History,
}

impl Document {
    /// White document with one empty layer, "Layer 1", which is active.
    pub fn new(width: u32, height: u32) -> Self {
        let first = LayerId::new(NonZeroU32::MIN);
        Self {
            width,
            height,
            background: Rgba::WHITE,
            layers: vec![RasterLayer::new(first, format!("{DEFAULT_NAME} 1"))],
            active: first,
            next_id: NonZeroU32::MIN.checked_add(1),
            history: History::default(),
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

    /// `(0, 0)..(width, height)` in canvas pixels.
    pub fn bounds(&self) -> IRect {
        IRect::from_size(self.width, self.height)
    }

    /// Number of layers (at least 1).
    pub fn layer_count(&self) -> usize {
        self.layers.len()
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

    /// Makes `id` the active layer. Hidden and locked layers can be active.
    /// Not recorded in history; a change of selection ends opacity merging.
    pub fn set_active_layer(&mut self, id: LayerId) -> Result<(), LayerError> {
        self.require(id)?;
        if self.active != id {
            self.history.opacity_chain = None;
        }
        self.active = id;
        Ok(())
    }

    /// Adds an empty layer at `at` and makes it active. It is named
    /// "Layer N" with the smallest N no other layer's name uses.
    /// Adding never changes the composite, so nothing is invalidated.
    pub fn add_layer(&mut self, at: Placement) -> Result<LayerId, LayerError> {
        let index = self.placement_index(at)?;
        let raw = self.next_id.ok_or(LayerError::IdsExhausted)?;
        let id = LayerId::new(raw);
        let layer = RasterLayer::new(id, self.default_name());
        let cost = layer_bytes(&layer);
        self.next_id = raw.checked_add(1);
        self.layers.insert(index, layer);
        let before = std::mem::replace(&mut self.active, id);
        let change = Change::Presence { index, layer: None };
        self.record(HistoryAction::AddLayer, vec![change], before, cost);
        Ok(id)
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
        let dirty = self.layers[i].shown_tiles();
        let removed = self.layers.remove(i);
        let before = self.active;
        if self.active == id {
            self.active = self.layers[i.saturating_sub(1)].id();
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
        Ok(self.layers[to_index].shown_tiles())
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
        Ok(layer.tiles())
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
        if !self.opacity_merges(id) {
            let change = Change::Opacity {
                layer: id,
                other: old,
            };
            self.record(HistoryAction::Opacity, vec![change], self.active, 0);
            self.history.opacity_chain = Some(id);
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

    /// Renames a layer. Allowed on locked layers.
    pub fn set_name(&mut self, id: LayerId, name: impl Into<String>) -> Result<(), LayerError> {
        let i = self.require(id)?;
        let name = name.into();
        if self.layers[i].name == name {
            return Ok(());
        }
        let cost = name.len().max(self.layers[i].name.len());
        let old = std::mem::replace(&mut self.layers[i].name, name);
        let change = Change::Name {
            layer: id,
            other: old,
        };
        self.record(HistoryAction::Rename, vec![change], self.active, cost);
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

    /// Whether a stroke may paint on `id`: it must exist, be visible and
    /// not be locked.
    pub fn paint_target(&self, id: LayerId) -> Result<(), LayerError> {
        let layer = &self.layers[self.require(id)?];
        if !layer.visible {
            return Err(LayerError::Hidden(id));
        }
        if layer.locked {
            return Err(LayerError::Locked(id));
        }
        Ok(())
    }

    /// The only way to change a layer's pixels: calls `f` once for each
    /// tile in `tiles` (allocating it if needed) and for no other tile.
    /// A tile allocated here that is still blank afterwards is released
    /// again, so the layer stays sparse. Refused on locked layers.
    ///
    /// One call is one history step (one stroke = one undo). The previous
    /// content of each declared tile that actually changed is kept for
    /// undo; this is the only tile copy history ever makes.
    pub fn modify_tiles(
        &mut self,
        id: LayerId,
        tiles: &DirtyTiles,
        mut f: impl FnMut(TileCoord, &mut Tile<[u8; 4]>),
    ) -> Result<DirtyTiles, LayerError> {
        let i = self.require_unlocked(id)?;
        let layer = &mut self.layers[i];
        let mut previous = Vec::with_capacity(tiles.len());
        let (mut before, mut after) = (0, 0);
        for coord in tiles.iter() {
            let old = layer.pixels.tile(coord).cloned();
            f(coord, layer.pixels.tile_or_insert(coord));
            if old.is_none() && layer.pixels.tile(coord).is_some_and(Tile::is_blank) {
                layer.pixels.remove_tile(coord);
            }
            let new = layer.pixels.tile(coord);
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
        let cost = before.max(after) * history::TILE_BYTES
            + previous.len() * std::mem::size_of::<(TileCoord, Option<Tile<[u8; 4]>>)>();
        let change = Change::Tiles {
            layer: id,
            tiles: previous,
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
        let pixels = std::mem::take(&mut self.layers[i].pixels);
        let cost = pixels.tile_count() * history::TILE_BYTES;
        let change = Change::Surface { layer: id, pixels };
        self.record(HistoryAction::ClearLayer, vec![change], self.active, cost);
        Ok(dirty)
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

    /// The document's whole content (layers in order with ids, properties
    /// and every tile's pixels, and the active layer), excluding history.
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
                    .map(|c| {
                        let mut h = DefaultHasher::new();
                        l.pixels.tile(c).unwrap().pixels().hash(&mut h);
                        (c, h.finish())
                    })
                    .collect();
                tiles.sort_unstable();
                LayerDigest {
                    id: l.id().get(),
                    name: l.name.clone(),
                    visible: l.visible,
                    opacity_bits: l.opacity.to_bits(),
                    blend_mode: l.blend_mode,
                    locked: l.locked,
                    tiles,
                }
            })
            .collect();
        Digest {
            active: self.active.get(),
            layers,
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
        }
    }
}

/// See [`Document::digest`].
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Digest {
    pub active: u32,
    pub layers: Vec<LayerDigest>,
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
    /// Sorted by coordinate; absent tiles are absent here too.
    pub tiles: Vec<(TileCoord, u64)>,
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
        d.modify_tiles(id, &tile_with_pixel(), |_, t| *t.get_mut(1, 1) = [255; 4])
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
        assert_eq!(d.paint_target(gone).err(), err);
        assert_eq!(d.clear_layer(gone).err(), err);
        assert_eq!(
            d.modify_tiles(gone, &tile_with_pixel(), |_, _| ()).err(),
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
        assert_eq!(d.paint_target(l2).err(), locked);
        assert_eq!(
            d.modify_tiles(l2, &tile_with_pixel(), |_, _| ()).err(),
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
        assert_eq!(d.paint_target(l1).err(), Some(LayerError::Hidden(l1)));
        d.set_visible(l1, true).unwrap();
        assert_eq!(d.paint_target(l1), Ok(()));
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
                *t.get_mut(0, 0) = [9; 4];
            }
        })
        .unwrap();
        assert_eq!(seen, [TileCoord { x: 0, y: 0 }, TileCoord { x: 2, y: 1 }]);
        let px = d.layer(l1).unwrap().pixels();
        assert_eq!(px.tile_count(), 1, "the untouched new tile was released");
        assert_eq!(px.pixel(0, 0), [9; 4]);
    }
}
