//! The document's selection (phase 5C, ADR 0024): which pixels edits may
//! change. Every change is a history step, as in the reference (parity
//! record 5C, S28, S29); none changes the composite.

use tela_canvas::{Tile, TileCoord, TileCoverage};

use super::Document;
use super::history::{Change, HistoryAction, selection_tiles_bytes};
use crate::selection::{MAX_FEATHER, rasterize};
use crate::{Selection, SelectionOp, SelectionShape};

impl Document {
    /// The active selection, if any. A selection is never empty: without
    /// one, edits are not limited (invariant SEL2).
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some()
    }

    /// Changes whenever the selection changes, undo and redo included
    /// (for views of the selection, such as its outline).
    pub fn selection_revision(&self) -> u64 {
        self.selection_revision
    }

    /// How much edits may change each pixel of tile `coord`: the
    /// selection's coverage, or [`TileCoverage::Full`] without a selection.
    /// Strokes pass it to `StrokeBuffer::commit_tile_within` (and the
    /// preview to `paint_over_tile_within`), so both apply the same limit.
    pub fn selection_tile(&self, coord: TileCoord) -> TileCoverage<'_> {
        self.selection
            .as_ref()
            .map_or(TileCoverage::Full, |s| s.tile(coord))
    }

    /// Selection coverage of document pixel `(x, y)`, `0..=255`: 255
    /// everywhere on the canvas without a selection (edits unrestricted),
    /// 0 outside the canvas with one.
    pub fn selection_coverage(&self, x: i32, y: i32) -> u8 {
        self.selection.as_ref().map_or(255, |s| s.coverage(x, y))
    }

    /// Selects `shape` (anti-aliased or not) combined with the current
    /// selection by `op` (reference S14–S16). Without a selection, Add is
    /// a replacement and Subtract and Intersect select nothing. A result
    /// without coverage is no selection (S5, S17, S18). Returns whether
    /// anything changed; a change is one history step.
    pub fn select(&mut self, shape: &SelectionShape, anti_alias: bool, op: SelectionOp) -> bool {
        let coverage = rasterize(shape, anti_alias, self.bounds());
        match (op, self.selection.as_ref()) {
            (SelectionOp::Replace, _) | (SelectionOp::Add, None) => {
                let new = Selection::from_coverage(self.bounds(), coverage);
                self.replace_selection(HistoryAction::Select, Some(new))
            }
            (SelectionOp::Subtract | SelectionOp::Intersect, None) => false,
            (SelectionOp::Intersect, Some(current)) => {
                let new = current.intersected(&coverage);
                self.replace_selection(HistoryAction::Select, Some(new))
            }
            (SelectionOp::Add | SelectionOp::Subtract, Some(current)) => {
                let tiles = current.combined_tiles(&coverage, op);
                self.change_selection_tiles(HistoryAction::Select, tiles, false)
            }
        }
    }

    /// Selects the whole canvas (no tile is stored). Recorded unless
    /// everything is selected already.
    pub fn select_all(&mut self) -> bool {
        let all = Selection::all(self.bounds());
        self.replace_selection(HistoryAction::SelectAll, Some(all))
    }

    /// Removes the selection (moved into history, not copied). Nothing to
    /// do without one.
    pub fn deselect(&mut self) -> bool {
        self.replace_selection(HistoryAction::Deselect, None)
    }

    /// Inverts the selection on the canvas without touching a tile;
    /// inverting everything leaves no selection (S17). Without a
    /// selection there is nothing to invert (the reference disables the
    /// command, S1).
    pub fn invert_selection(&mut self) -> bool {
        if self.selection.is_none() {
            return false;
        }
        self.change_selection_tiles(HistoryAction::InvertSelection, Vec::new(), true)
    }

    /// Blurs the selection's border by `strength` pixels (clamped to
    /// [`MAX_FEATHER`]), with the reference's measured kernel (S19–S21).
    /// Refused (returns `false`) without a selection or for a strength
    /// that is not a positive number. A selection blurred away entirely
    /// becomes no selection.
    pub fn feather_selection(&mut self, strength: f32) -> bool {
        let Some(current) = self.selection.as_ref() else {
            return false;
        };
        if strength.is_nan() || strength <= 0.0 {
            return false;
        }
        let tiles = current.feathered_tiles(strength.min(MAX_FEATHER));
        self.change_selection_tiles(HistoryAction::FeatherSelection, tiles, false)
    }

    /// Replaces the whole selection (an empty one becomes `None`); the old
    /// one moves into history. Records nothing if nothing changes.
    fn replace_selection(&mut self, action: HistoryAction, new: Option<Selection>) -> bool {
        let new = new.filter(|s| !s.is_empty());
        if new == self.selection {
            return false;
        }
        let bytes = |s: &Option<Selection>| s.as_ref().map_or(0, Selection::bytes);
        let cost = bytes(&new).max(bytes(&self.selection));
        let old = std::mem::replace(&mut self.selection, new);
        let change = Change::SelectionSwap {
            other: old.map(Box::new),
        };
        self.record(action, vec![change], self.active, cost);
        self.selection_revision += 1;
        true
    }

    /// Swaps `tiles` into the current selection (inverting it too when
    /// `flip`); records the old tiles. A result without coverage is
    /// turned back and recorded as a removal of the whole selection.
    fn change_selection_tiles(
        &mut self,
        action: HistoryAction,
        mut tiles: Vec<(TileCoord, Option<Tile<u8>>)>,
        flip: bool,
    ) -> bool {
        if tiles.is_empty() && !flip {
            return false;
        }
        let selection = self
            .selection
            .as_mut()
            .expect("callers check for a selection");
        let mut cost = selection_tiles_bytes(&tiles);
        selection.swap_tiles(&mut tiles);
        if flip {
            selection.invert();
        }
        if selection.is_empty() {
            selection.swap_tiles(&mut tiles);
            if flip {
                selection.invert();
            }
            return self.replace_selection(action, None);
        }
        cost = cost.max(selection_tiles_bytes(&tiles));
        let change = Change::SelectionTiles { tiles, flip };
        self.record(action, vec![change], self.active, cost);
        self.selection_revision += 1;
        true
    }
}

#[cfg(test)]
mod tests;
