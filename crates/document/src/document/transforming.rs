//! Transforming a layer's content (phase 5D, ADR 0025): the session's
//! source taken from the document, and the commit as one history step.
//! The rules are the reference's (docs/parity/5D-transform.md).

use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, Tile, TileCoord, TiledSurface};
use tela_color::{PremulRgba, StoragePixel, with_pixel_type};

use super::history::{Change, HistoryAction, MASK_TILE_BYTES};
use super::{Document, parts};
use crate::pixels::{TileBackup, tile_bytes};
use crate::transform::{Transform, TransformSource};
use crate::{LayerError, LayerId, Selection};

impl Document {
    /// What a transform of layer `id` moves, for a transform session:
    /// with a selection, the selected part of the layer's pixels inside
    /// the selection's bounds (T28–T31); without one, the whole layer and
    /// its mask, inside their content bounds (T27, T33).
    ///
    /// Refused, like the reference (T34–T36), on a hidden
    /// ([`LayerError::Hidden`]) or locked ([`LayerError::Locked`]) layer,
    /// and when there is nothing to transform (an empty layer without a
    /// selection, [`LayerError::NothingToTransform`]). The alpha lock does
    /// not matter (T37). Changes nothing.
    pub fn transform_source(&self, id: LayerId) -> Result<TransformSource, LayerError> {
        let i = self.require_unlocked(id)?;
        let layer = &self.layers[i];
        if !layer.visible {
            return Err(LayerError::Hidden(id));
        }
        let mask = layer.mask.as_ref().filter(|_| self.selection.is_none());
        let bounds = match &self.selection {
            Some(s) => s.bounds(),
            None => {
                let pixels = with_pixel_type!(self.depth, P => {
                    content_bounds(layer.pixels.surface::<P>().expect("the document's depth"))
                });
                pixels.union(&mask.map_or(IRect::default(), |m| content_bounds(&m.hidden)))
            }
        };
        if bounds.is_empty() {
            return Err(LayerError::NothingToTransform(id));
        }
        let (w, h) = (bounds.width() as usize, bounds.height() as usize);
        let mut px = vec![[0.0f32; 4]; w * h];
        with_pixel_type!(self.depth, P => {
            let surface = layer.pixels.surface::<P>().expect("the document's depth");
            for_each_row(surface, bounds, |k, row| {
                for (d, p) in px[k..k + row.len()].iter_mut().zip(row) {
                    let p = p.to_premul();
                    *d = [p.r, p.g, p.b, p.a];
                }
            });
        });
        let cov = self.selection.as_ref().map(|s| {
            let mut cov = vec![0.0f32; w * h];
            let mut line = vec![0u8; w];
            for (j, y) in (bounds.y0..bounds.y1).enumerate() {
                s.read_line(bounds.x0, y, &mut line);
                for ((c, p), &v) in cov[j * w..(j + 1) * w]
                    .iter_mut()
                    .zip(&mut px[j * w..(j + 1) * w])
                    .zip(&line)
                {
                    *c = f32::from(v) / 255.0;
                    *p = p.map(|x| x * *c);
                }
            }
            cov
        });
        let mask = mask.map(|m| {
            let mut hidden = vec![0.0f32; w * h];
            for_each_row(&m.hidden, bounds, |k, row| {
                for (d, &v) in hidden[k..k + row.len()].iter_mut().zip(row) {
                    *d = f32::from(v) / 255.0;
                }
            });
            hidden
        });
        Ok(TransformSource::new(
            id,
            self.depth,
            (self.revision(), self.selection_revision),
            bounds,
            px,
            cov,
            mask,
        ))
    }

    /// Commits `transform` of `source` as **one** history step (T13,
    /// T14): the layer's pixels (what stays, with the transformed content
    /// composed over it, quantized once at the document's depth), its
    /// mask when it moves, and the selection, which follows the content
    /// (T32). Exactly what the preview showed (invariant TR4).
    ///
    /// Refused, changing nothing, when the document changed since
    /// `source` was taken ([`LayerError::StaleTransform`]), for invalid
    /// parameters ([`LayerError::InvalidTransform`]), when the layer is
    /// now hidden or locked, or the result would reach tiles beyond
    /// [`MAX_TILE_COORD`](crate::MAX_TILE_COORD). A transform that changes
    /// nothing records nothing. Pixels may land beyond the canvas and are
    /// kept (T39); the selection is clipped to the canvas.
    pub fn apply_transform(
        &mut self,
        source: &TransformSource,
        transform: &Transform,
    ) -> Result<DirtyTiles, LayerError> {
        let id = source.layer;
        let i = self.require_unlocked(id)?;
        if !self.layers[i].visible {
            return Err(LayerError::Hidden(id));
        }
        if source.revision != self.revision()
            || source.selection_revision != self.selection_revision
            || source.depth != self.depth
        {
            return Err(LayerError::StaleTransform);
        }
        let Some(dest) = source.dest_bounds(&transform.params) else {
            return Err(LayerError::InvalidTransform);
        };
        let tiles = source.affected_tiles(&transform.params);
        if let Some(c) = tiles.iter().find(|&c| !parts::tile_in_range(c)) {
            return Err(LayerError::TileOutOfRange(c));
        }

        let mut changes = Vec::new();
        let mut cost = 0;
        let selection = self.selection.as_ref();
        let layer = &mut self.layers[i];
        let (backup, before, after, entries) = with_pixel_type!(self.depth, P => {
            let surface = layer.pixels.surface_mut::<P>().expect("the document's depth");
            let (b, before, after) = transform_pixels(surface, source, transform, &tiles, selection);
            let entries = b.len() * std::mem::size_of::<(TileCoord, Option<Tile<P>>)>();
            (TileBackup::new(b), before, after, entries)
        });
        if entries > 0 {
            cost += before.max(after) * tile_bytes(self.depth) + entries;
            changes.push(Change::Tiles {
                layer: id,
                tiles: backup,
            });
        }
        if source.moves_mask()
            && let Some(mask) = layer.mask.as_mut()
        {
            let previous = transform_mask(&mut mask.hidden, source, transform, &tiles);
            if !previous.is_empty() {
                let held = previous.iter().filter(|(_, t)| t.is_some()).count();
                cost += held.max(previous.len()) * MASK_TILE_BYTES
                    + std::mem::size_of_val(previous.as_slice());
                changes.push(Change::MaskTiles {
                    layer: id,
                    tiles: previous,
                });
            }
        }
        if source.is_selection() {
            let moved = transformed_selection(source, transform, dest, self.bounds());
            let moved = moved.filter(|s| !s.is_empty());
            if moved != self.selection {
                let bytes = |s: &Option<Selection>| s.as_ref().map_or(0, Selection::bytes);
                cost += bytes(&moved).max(bytes(&self.selection));
                let old = std::mem::replace(&mut self.selection, moved);
                changes.push(Change::SelectionSwap {
                    other: old.map(Box::new),
                });
                self.selection_revision += 1;
            }
        }
        if changes.is_empty() {
            return Ok(DirtyTiles::new());
        }
        self.record(HistoryAction::Transform, changes, self.active, cost);
        Ok(tiles)
    }
}

/// Calls `f(dense index, stored row segment)` for every row of `surface`
/// inside `bounds` that has a tile (absent tiles are left as they are in
/// the caller's zeroed buffer).
fn for_each_row<T: Copy + Default>(
    surface: &TiledSurface<T>,
    bounds: IRect,
    mut f: impl FnMut(usize, &[T]),
) {
    let w = bounds.width() as usize;
    for c in bounds.tiles() {
        let Some(tile) = surface.tile(c) else {
            continue;
        };
        let r = c.rect().intersect(&bounds);
        let (ox, oy) = c.origin();
        for y in r.y0..r.y1 {
            let row = tile.row((y - oy) as u32);
            let seg = &row[(r.x0 - ox) as usize..(r.x1 - ox) as usize];
            let k = (y - bounds.y0) as usize * w + (r.x0 - bounds.x0) as usize;
            f(k, seg);
        }
    }
}

/// Smallest rectangle holding every stored value that is not the default
/// (a transparent pixel, a mask value that hides nothing).
fn content_bounds<T: Copy + Default + PartialEq>(surface: &TiledSurface<T>) -> IRect {
    let blank = T::default();
    let mut bounds = IRect::default();
    for c in surface.tile_coords() {
        let tile = surface.tile(c).expect("listed");
        let (ox, oy) = c.origin();
        for y in 0..TILE_SIZE {
            let row = tile.row(y);
            if let Some(x0) = row.iter().position(|v| *v != blank) {
                let x1 = row.iter().rposition(|v| *v != blank).expect("found one") + 1;
                let y = oy + y as i32;
                bounds = bounds.union(&IRect::new(ox + x0 as i32, y, ox + x1 as i32, y + 1));
            }
        }
    }
    bounds
}

/// Previous content of changed tiles (`None` = absent).
type Backup<P> = Vec<(TileCoord, Option<Tile<P>>)>;

/// Writes the transformed pixels into every tile of `tiles` at depth `P`
/// (blank results are released). Returns the previous content of the
/// tiles that changed and how many were stored before and after.
fn transform_pixels<P: StoragePixel>(
    surface: &mut TiledSurface<P>,
    source: &TransformSource,
    transform: &Transform,
    tiles: &DirtyTiles,
    selection: Option<&Selection>,
) -> (Backup<P>, usize, usize) {
    let mut scratch = vec![PremulRgba::TRANSPARENT; (TILE_SIZE * TILE_SIZE) as usize];
    let mut previous = Vec::new();
    let (mut before, mut after) = (0, 0);
    for coord in tiles.iter() {
        let old = surface.tile(coord).cloned();
        match &old {
            Some(t) => {
                for (d, p) in scratch.iter_mut().zip(t.pixels()) {
                    *d = p.to_premul();
                }
            }
            None => scratch.fill(PremulRgba::TRANSPARENT),
        }
        let coverage = selection.map_or(tela_canvas::TileCoverage::Full, |s| s.tile(coord));
        source.transform_pixels_tile(transform, coord, &coverage, &mut scratch);
        let mut new = Tile::<P>::new();
        for (d, s) in new.pixels_mut().iter_mut().zip(&scratch) {
            *d = P::from_premul(*s);
        }
        let new = (!new.is_blank()).then_some(new);
        if new != old {
            before += usize::from(old.is_some());
            after += usize::from(new.is_some());
            match new {
                Some(t) => {
                    surface.insert_tile(coord, t);
                }
                None => {
                    surface.remove_tile(coord);
                }
            }
            previous.push((coord, old));
        }
    }
    (previous, before, after)
}

/// Writes the transformed mask (stored as hidden amounts) into every tile
/// of `tiles`; tiles that hide nothing are released (invariant M3).
/// Returns the previous content of the tiles that changed.
fn transform_mask(
    hidden: &mut TiledSurface<u8>,
    source: &TransformSource,
    transform: &Transform,
    tiles: &DirtyTiles,
) -> Vec<(TileCoord, Option<Tile<u8>>)> {
    let mut reveal = vec![0.0f32; (TILE_SIZE * TILE_SIZE) as usize];
    let mut previous = Vec::new();
    for coord in tiles.iter() {
        let old = hidden.tile(coord).cloned();
        match &old {
            Some(t) => {
                for (r, h) in reveal.iter_mut().zip(t.pixels()) {
                    *r = f32::from(255 - h) / 255.0;
                }
            }
            None => reveal.fill(1.0),
        }
        source.transform_mask_tile(transform, coord, &mut reveal);
        let mut new = Tile::<u8>::new();
        for (d, r) in new.pixels_mut().iter_mut().zip(&reveal) {
            *d = tela_color::unorm8(1.0 - r);
        }
        let new = (!new.is_blank()).then_some(new);
        if new != old {
            match new {
                Some(t) => {
                    hidden.insert_tile(coord, t);
                }
                None => {
                    hidden.remove_tile(coord);
                }
            }
            previous.push((coord, old));
        }
    }
    previous
}

/// The selection after the transform: the transformed coverage, clipped
/// to the canvas, 8-bit (T32). `None` if nothing is covered.
fn transformed_selection(
    source: &TransformSource,
    transform: &Transform,
    dest: IRect,
    canvas: IRect,
) -> Option<Selection> {
    let mut tiles = TiledSurface::<u8>::new();
    let mut cov = vec![0.0f32; (TILE_SIZE * TILE_SIZE) as usize];
    for coord in dest.intersect(&canvas).tiles() {
        if !source.transform_coverage_tile(transform, coord, &mut cov) {
            continue;
        }
        let clip = coord.rect().intersect(&canvas);
        let (ox, oy) = coord.origin();
        let mut tile = Tile::<u8>::new();
        for y in clip.y0..clip.y1 {
            for x in clip.x0..clip.x1 {
                let i = ((y - oy) as u32 * TILE_SIZE + (x - ox) as u32) as usize;
                *tile.get_mut((x - ox) as u32, (y - oy) as u32) = tela_color::unorm8(cov[i]);
            }
        }
        if !tile.is_blank() {
            tiles.insert_tile(coord, tile);
        }
    }
    let s = Selection::from_coverage(canvas, tiles);
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests;
