//! Undo/redo (ADR 0015).
//!
//! A linear history of steps, owned by [`Document`] and recorded by its
//! mutation methods, so nothing can change the document without being
//! recorded (except the documented non-historical operations: selecting
//! the active layer and showing/hiding a layer, which the reference
//! application does not record either; docs/parity/4B-undo-redo.md).
//!
//! Every change is stored as **the other state** of what it touched, and
//! applying it swaps that state with the document's: undoing and redoing
//! move tiles, surfaces and whole layers back and forth without copying
//! them. The only copy is made when a stroke is recorded (the touched
//! tiles' previous content). Tiles absent before an edit are stored as
//! absent and come back absent.

use std::collections::VecDeque;
use std::mem::size_of;

use tela_canvas::{DirtyTiles, TILE_SIZE, Tile, TileCoord};
use tela_color::PixelDepth;

use super::Document;
use crate::layer::LayerMask;
use crate::pixels::TileBackup;
use crate::{BlendMode, LayerId, LayerPixels, RasterLayer};

/// Bytes of one mask tile (8-bit).
pub(super) const MASK_TILE_BYTES: usize = (TILE_SIZE * TILE_SIZE) as usize;

/// How much history a document keeps. A policy, not an invariant: the
/// defaults (200 steps, like the reference's default; 1 GiB) can be changed
/// at any time with [`Document::set_history_limits`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryLimits {
    /// Most steps kept (undoable plus redoable). 0 disables history.
    pub max_steps: usize,
    /// Most bytes the steps may hold, counted as each step's worst case
    /// (see [`Document::history_reserved_bytes`]). A single step larger
    /// than this is still kept, alone.
    pub max_bytes: usize,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_steps: 200,
            max_bytes: 1 << 30,
        }
    }
}

/// A document's persistent content at one moment, see
/// [`Document::revision`]. Opaque; only equality is meaningful.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Revision {
    /// Serial of the newest undoable step (or of the newest step dropped,
    /// when nothing is left to undo). Serials are never reused.
    step: u64,
    /// Count of non-recorded changes to persistent state.
    unrecorded: u64,
}

/// What a history step did, e.g. to label undo/redo buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryAction {
    /// Pixels changed through [`Document::modify_tiles`] (a stroke).
    Paint,
    ClearLayer,
    AddLayer,
    RemoveLayer,
    MoveLayer,
    Opacity,
    BlendMode,
    Rename,
    Lock,
    /// "Lock transparent pixels" toggled.
    AlphaLock,
    /// Clipping toggled.
    Clip,
    AddMask,
    RemoveMask,
    /// Mask enabled or disabled.
    MaskEnabled,
    /// A mask changed through [`Document::modify_mask`] (a stroke).
    PaintMask,
    /// The document changed depth ([`Document::convert_depth`]).
    ConvertDepth,
    /// A layer added with pixels ([`Document::add_layer_with_pixels`],
    /// an imported image).
    ImportLayer,
}

impl HistoryAction {
    /// Steps that add or remove layers restore the active layer recorded
    /// with them; every other undo/redo keeps the current selection
    /// (reference observations U4–U8, U10).
    fn restores_active(self) -> bool {
        matches!(
            self,
            HistoryAction::AddLayer | HistoryAction::RemoveLayer | HistoryAction::ImportLayer
        )
    }
}

/// One reversible change. Each variant holds the state that is *not* in
/// the document; [`Document::apply`] swaps it with the document's.
#[derive(Clone, Debug)]
pub(super) enum Change {
    /// The other state of some tiles of a layer (`None` = absent).
    Tiles {
        layer: LayerId,
        tiles: TileBackup,
    },
    /// The other state of a layer's whole surface (clear).
    Surface {
        layer: LayerId,
        pixels: LayerPixels,
    },
    /// A layer at `index`: in the document when `layer` is `None`, held
    /// here otherwise (add/remove).
    Presence {
        index: usize,
        layer: Option<Box<RasterLayer>>,
    },
    /// Layer `id` sits at `at`; applying moves it to `other`, then the two
    /// swap roles.
    Move {
        id: LayerId,
        at: usize,
        other: usize,
    },
    Opacity {
        layer: LayerId,
        other: f32,
    },
    Blend {
        layer: LayerId,
        other: BlendMode,
    },
    Name {
        layer: LayerId,
        other: String,
    },
    Locked {
        layer: LayerId,
        other: bool,
    },
    AlphaLocked {
        layer: LayerId,
        other: bool,
    },
    Clipped {
        layer: LayerId,
        other: bool,
    },
    /// A layer's mask: in the document when `mask` is `None`, held here
    /// otherwise (add/remove mask).
    MaskPresence {
        layer: LayerId,
        mask: Option<Box<LayerMask>>,
    },
    MaskEnabled {
        layer: LayerId,
        other: bool,
    },
    /// The other state of some stored mask tiles (`None` = absent).
    MaskTiles {
        layer: LayerId,
        tiles: Vec<(TileCoord, Option<Tile<u8>>)>,
    },
    /// Every layer's pixels at the `other` depth (convert depth); masks
    /// are 8-bit at every depth and stay.
    Depth {
        layers: Vec<(LayerId, LayerPixels)>,
        other: PixelDepth,
    },
}

impl Change {
    /// Bytes this change holds right now.
    fn held_bytes(&self) -> usize {
        match self {
            Change::Tiles { tiles, .. } => tiles.held_bytes(),
            Change::Surface { pixels, .. } => pixels_bytes(pixels),
            Change::Depth { layers, .. } => layers.iter().map(|(_, p)| pixels_bytes(p)).sum(),
            Change::Presence { layer, .. } => layer.as_deref().map_or(0, layer_bytes),
            Change::Name { other, .. } => other.len(),
            Change::MaskTiles { tiles, .. } => {
                tiles.len() * size_of::<(TileCoord, Option<Tile<u8>>)>()
                    + tiles.iter().filter(|(_, t)| t.is_some()).count() * MASK_TILE_BYTES
            }
            Change::MaskPresence { mask, .. } => mask.as_deref().map_or(0, mask_bytes),
            Change::Move { .. }
            | Change::Opacity { .. }
            | Change::Blend { .. }
            | Change::Locked { .. }
            | Change::AlphaLocked { .. }
            | Change::Clipped { .. }
            | Change::MaskEnabled { .. } => 0,
        }
    }
}

/// Pixel and mask buffers plus name of a layer (container overhead not
/// counted).
pub(super) fn layer_bytes(layer: &RasterLayer) -> usize {
    size_of::<RasterLayer>()
        + layer.name.len()
        + pixels_bytes(&layer.pixels)
        + layer.mask.as_ref().map_or(0, mask_bytes)
}

/// Tile buffers of a layer's pixels at their depth.
pub(super) fn pixels_bytes(pixels: &LayerPixels) -> usize {
    pixels.tile_count() * pixels.tile_bytes()
}

/// Tile buffers of a mask.
pub(super) fn mask_bytes(mask: &LayerMask) -> usize {
    mask.hidden.tile_count() * MASK_TILE_BYTES
}

#[derive(Clone, Debug)]
pub(super) struct Step {
    action: HistoryAction,
    /// Unique within the document (never reused): identifies the state
    /// this step leads to, see [`Document::revision`].
    serial: u64,
    changes: Vec<Change>,
    active_before: LayerId,
    active_after: LayerId,
    /// The most this step can hold in either direction, fixed when it is
    /// recorded; the budget is enforced on these.
    cost: usize,
}

impl Step {
    fn held_bytes(&self) -> usize {
        size_of::<Step>() + self.changes.iter().map(Change::held_bytes).sum::<usize>()
    }
}

/// The undo and redo stacks of one document.
#[derive(Clone, Debug, Default)]
pub(super) struct History {
    /// Oldest first.
    undo: VecDeque<Step>,
    /// Most recently undone last.
    redo: Vec<Step>,
    limits: HistoryLimits,
    /// Sum of `cost` over both stacks.
    reserved: usize,
    /// Layer whose opacity edits still merge into the newest step. Broken
    /// by any other recorded step, undo, redo or a change of the active
    /// layer (reference observations U14–U17).
    pub(super) opacity_chain: Option<LayerId>,
    /// Last serial handed to a step.
    serial: u64,
    /// Serial of the newest step dropped from the undo end (0 if none):
    /// the state the document is in when nothing is left to undo.
    base: u64,
    /// Changes to persistent state that are not history steps (showing or
    /// hiding a layer), counted so that [`Document::revision`] sees them.
    pub(super) unrecorded: u64,
}

impl History {
    /// Drops the oldest steps (then the farthest redoable ones) until the
    /// limits hold. The newest step survives the byte limit on its own.
    fn enforce(&mut self) {
        loop {
            let steps = self.undo.len() + self.redo.len();
            let over_steps = steps > self.limits.max_steps;
            let over_bytes = self.reserved > self.limits.max_bytes && steps > 1;
            if !over_steps && !over_bytes {
                return;
            }
            let dropped = match self.undo.pop_front() {
                Some(s) => {
                    self.base = s.serial;
                    s
                }
                None => self.redo.remove(0),
            };
            self.reserved -= dropped.cost;
            if self.undo.is_empty() {
                self.opacity_chain = None;
            }
        }
    }
}

impl Document {
    /// Undoes the newest step. Returns the tiles whose composite changed,
    /// or `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Option<DirtyTiles> {
        let mut step = self.history.undo.pop_back()?;
        let mut dirty = DirtyTiles::new();
        for change in step.changes.iter_mut().rev() {
            dirty.extend(&self.apply(change));
        }
        if step.action.restores_active() {
            self.active = step.active_before;
        }
        self.keep_target_valid();
        self.history.opacity_chain = None;
        self.history.redo.push(step);
        Some(dirty)
    }

    /// Redoes the most recently undone step. Returns the tiles whose
    /// composite changed, or `None` when there is nothing to redo.
    pub fn redo(&mut self) -> Option<DirtyTiles> {
        let mut step = self.history.redo.pop()?;
        let mut dirty = DirtyTiles::new();
        for change in step.changes.iter_mut() {
            dirty.extend(&self.apply(change));
        }
        if step.action.restores_active() {
            self.active = step.active_after;
        }
        self.keep_target_valid();
        self.history.opacity_chain = None;
        self.history.undo.push_back(step);
        Some(dirty)
    }

    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// Number of undoable steps.
    pub fn history_len(&self) -> usize {
        self.history.undo.len()
    }

    /// Number of redoable steps.
    pub fn redo_len(&self) -> usize {
        self.history.redo.len()
    }

    /// What the next undo would revert.
    pub fn next_undo(&self) -> Option<HistoryAction> {
        self.history.undo.back().map(|s| s.action)
    }

    /// What the next redo would re-apply.
    pub fn next_redo(&self) -> Option<HistoryAction> {
        self.history.redo.last().map(|s| s.action)
    }

    /// Bytes the history holds now: tile buffers, removed layers (their
    /// tiles and names), cleared surfaces, old names, plus a fixed size per
    /// step and per tile entry. Not counted: allocator and hash-map
    /// overhead, which is small next to 256 KiB (8-bit) or 512 KiB (16-bit)
    /// tiles.
    pub fn history_bytes(&self) -> usize {
        let h = &self.history;
        h.undo
            .iter()
            .chain(h.redo.iter())
            .map(Step::held_bytes)
            .sum()
    }

    /// The figure the byte limit is enforced on: each step's larger
    /// direction (e.g. a stroke over empty tiles holds nothing until it is
    /// undone, then holds the new tiles). Always ≥ [`Document::history_bytes`].
    pub fn history_reserved_bytes(&self) -> usize {
        self.history.reserved
    }

    pub fn history_limits(&self) -> HistoryLimits {
        self.history.limits
    }

    /// Changes the limits; steps beyond them are dropped at once, oldest
    /// first.
    pub fn set_history_limits(&mut self, limits: HistoryLimits) {
        self.history.limits = limits;
        self.history.enforce();
    }

    /// Records a step that has just been applied: clears redo, ends the
    /// opacity chain and enforces the limits. `cost` is the step's larger
    /// direction, computed by the caller, which knows both states.
    pub(super) fn record(
        &mut self,
        action: HistoryAction,
        changes: Vec<Change>,
        active_before: LayerId,
        cost: usize,
    ) {
        let h = &mut self.history;
        for dropped in h.redo.drain(..) {
            h.reserved -= dropped.cost;
        }
        let cost = cost + size_of::<Step>();
        h.reserved += cost;
        h.serial += 1;
        h.undo.push_back(Step {
            action,
            serial: h.serial,
            changes,
            active_before,
            active_after: self.active,
            cost,
        });
        h.opacity_chain = None;
        h.enforce();
    }

    /// Identifies the document's persistent content (what a file keeps)
    /// at this moment: it changes with every recorded step, undo, redo and
    /// non-recorded change of persistent state (visibility), and comes
    /// back when undo or redo returns to an earlier state. Selecting a
    /// layer or the edit target does not change it (reference F2).
    ///
    /// The app keeps the revision of its last save (its save point): the
    /// document is modified while `revision() != saved`. Undoing back to
    /// the saved state is clean again; a visibility change counts until
    /// the next save even if it is reverted (no save point exists for
    /// state outside the history, as in the reference, F3).
    pub fn revision(&self) -> Revision {
        let h = &self.history;
        Revision {
            step: h.undo.back().map_or(h.base, |s| s.serial),
            unrecorded: h.unrecorded,
        }
    }

    /// The newest step's content changed in place (an opacity edit merged
    /// into it): it now leads to a new state, so it gets a new serial.
    pub(super) fn renew_newest_step(&mut self) {
        let h = &mut self.history;
        if let Some(step) = h.undo.back_mut() {
            h.serial += 1;
            step.serial = h.serial;
        }
    }

    /// Whether a new opacity value for `id` merges into the newest step.
    pub(super) fn opacity_merges(&self, id: LayerId) -> bool {
        let h = &self.history;
        h.opacity_chain == Some(id)
            && h.redo.is_empty()
            && h.undo
                .back()
                .is_some_and(|s| s.action == HistoryAction::Opacity)
    }

    /// Swaps a change's stored state with the document's. Returns the
    /// composite tiles affected.
    fn apply(&mut self, change: &mut Change) -> DirtyTiles {
        match change {
            Change::Tiles { layer, tiles } => {
                let layer = self.layer_mut_for_history(*layer);
                let mut dirty = DirtyTiles::new();
                layer.pixels.swap_tiles(tiles, |c| dirty.mark(c));
                if layer.visible {
                    dirty
                } else {
                    DirtyTiles::new()
                }
            }
            Change::Surface { layer, pixels } => {
                let layer = self.layer_mut_for_history(*layer);
                let mut dirty = layer.shown_tiles();
                std::mem::swap(&mut layer.pixels, pixels);
                dirty.extend(&layer.shown_tiles());
                dirty
            }
            Change::Presence { index, layer } => match layer.take() {
                Some(held) => {
                    self.layers.insert(*index, *held);
                    self.group_tiles(*index)
                }
                None => {
                    let dirty = self.group_tiles(*index);
                    let removed = self.layers.remove(*index);
                    *layer = Some(Box::new(removed));
                    dirty
                }
            },
            Change::Move { id, at, other } => {
                let mut dirty = self.group_tiles(*at);
                let moved = self.layers.remove(*at);
                debug_assert_eq!(moved.id(), *id, "history replays on the state it recorded");
                self.layers.insert(*other, moved);
                dirty.extend(&self.group_tiles(*other));
                std::mem::swap(at, other);
                dirty
            }
            Change::Opacity { layer, other } => {
                let layer = self.layer_mut_for_history(*layer);
                std::mem::swap(&mut layer.opacity, other);
                layer.shown_tiles()
            }
            Change::Blend { layer, other } => {
                let layer = self.layer_mut_for_history(*layer);
                std::mem::swap(&mut layer.blend_mode, other);
                layer.shown_tiles()
            }
            Change::Name { layer, other } => {
                std::mem::swap(&mut self.layer_mut_for_history(*layer).name, other);
                DirtyTiles::new()
            }
            Change::Locked { layer, other } => {
                std::mem::swap(&mut self.layer_mut_for_history(*layer).locked, other);
                DirtyTiles::new()
            }
            Change::AlphaLocked { layer, other } => {
                std::mem::swap(&mut self.layer_mut_for_history(*layer).alpha_locked, other);
                DirtyTiles::new()
            }
            Change::Clipped { layer, other } => {
                let i = self
                    .index_of(*layer)
                    .expect("history replays on the state it recorded");
                std::mem::swap(&mut self.layers[i].clipped, other);
                self.group_tiles(i)
            }
            Change::MaskPresence { layer, mask } => {
                let layer = self.layer_mut_for_history(*layer);
                let dirty = layer.shown_tiles();
                match mask.take() {
                    Some(held) => layer.mask = Some(*held),
                    None => *mask = layer.mask.take().map(Box::new),
                }
                dirty
            }
            Change::MaskEnabled { layer, other } => {
                let layer = self.layer_mut_for_history(*layer);
                let mask = layer
                    .mask
                    .as_mut()
                    .expect("history replays on the state it recorded");
                std::mem::swap(&mut mask.enabled, other);
                layer.shown_tiles()
            }
            Change::Depth { layers, other } => {
                let mut dirty = DirtyTiles::new();
                for (id, held) in layers.iter_mut() {
                    let layer = self.layer_mut_for_history(*id);
                    dirty.extend(&layer.shown_tiles());
                    std::mem::swap(&mut layer.pixels, held);
                    dirty.extend(&layer.shown_tiles());
                }
                std::mem::swap(&mut self.depth, other);
                dirty
            }
            Change::MaskTiles { layer, tiles } => {
                let layer = self.layer_mut_for_history(*layer);
                let visible = layer.visible;
                let mask = layer
                    .mask
                    .as_mut()
                    .expect("history replays on the state it recorded");
                let mut dirty = DirtyTiles::new();
                for (coord, held) in tiles.iter_mut() {
                    let current = mask.hidden.remove_tile(*coord);
                    if let Some(tile) = held.take() {
                        mask.hidden.insert_tile(*coord, tile);
                    }
                    *held = current;
                    dirty.mark(*coord);
                }
                if visible && mask.enabled {
                    dirty
                } else {
                    DirtyTiles::new()
                }
            }
        }
    }

    /// After undo/redo: the mask stays the edit target only if the active
    /// layer still has a mask.
    fn keep_target_valid(&mut self) {
        let i = self.active_index();
        if self.layers[i].mask.is_none() {
            self.active_part = super::LayerPart::Pixels;
        }
    }

    fn layer_mut_for_history(&mut self, id: LayerId) -> &mut RasterLayer {
        let i = self
            .index_of(id)
            .expect("history replays on the state it recorded, so its layers exist");
        &mut self.layers[i]
    }
}

#[cfg(test)]
mod tests;
