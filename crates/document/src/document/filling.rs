//! Filling a layer's pixels or mask (phase 5E, ADR 0026): the region from
//! the reference, the paint into the target, one history step. The rules
//! are the reference's (docs/parity/5E-fill.md).

use tela_canvas::{DirtyTiles, TILE_SIZE, Tile, TileCoord, TileCoverage};
use tela_color::{PremulRgba, StoragePixel, with_pixel_type};

use super::history::HistoryAction;
use super::{Document, LayerPart};
use crate::compose::ComposeScratch;
use crate::fill::{
    self, ConnectedFill, FAR, FillPaint, FillReference, FillRegion, FillTarget, Grid, TileDistances,
};
use crate::{LayerError, LayerMask, LayerPixels};

/// Pixels per tile.
const N: usize = (TILE_SIZE * TILE_SIZE) as usize;

/// Where a connected fill reads its colours.
enum Source<'a> {
    Pixels(&'a LayerPixels),
    Mask(&'a LayerMask),
    Composite(Box<ComposeScratch>),
}

impl Source<'_> {
    /// Reads tile `c` into `out`, or returns its single colour when the
    /// whole tile has one without reading (absent tiles, empty
    /// composites). A mask reads as opaque grey of its reveal value, so
    /// the distance is the reveal difference (BK25).
    fn read(&mut self, doc: &Document, c: TileCoord, out: &mut [PremulRgba]) -> Option<PremulRgba> {
        match self {
            Source::Pixels(p) => (!p.decode_tile(c, out)).then_some(PremulRgba::TRANSPARENT),
            Source::Mask(m) => match m.hidden_tile(c) {
                None => Some(PremulRgba::new(1.0, 1.0, 1.0, 1.0)),
                Some(t) => {
                    for (o, &h) in out.iter_mut().zip(t.pixels()) {
                        let r = f32::from(255 - h) / 255.0;
                        *o = PremulRgba::new(r, r, r, 1.0);
                    }
                    None
                }
            },
            Source::Composite(scratch) => {
                if doc.compose_tile_transparent_into(c, scratch) {
                    out.copy_from_slice(scratch.premultiplied());
                    None
                } else {
                    Some(PremulRgba::TRANSPARENT)
                }
            }
        }
    }
}

impl Document {
    /// The region a connected (bucket) fill of `target` from `fill.seed`
    /// would change, with each pixel's coverage before the selection.
    /// Read-only. The region is every pixel 4-connected to the seed whose
    /// colour in the reference is within the tolerance of the seed's, on
    /// the canvas and where the selection covers more than 0 (BK5–BK14,
    /// BK26); with anti-aliasing, the pixels just outside it are partly
    /// covered (BK15). Empty when the seed is off the canvas or outside
    /// the selection (BK9, BK27).
    ///
    /// Refused for an unknown layer, or a mask target on a layer without a
    /// mask. Hidden and locked layers can be measured; filling them is
    /// refused by [`Document::fill_connected`].
    pub fn fill_region(
        &self,
        target: FillTarget,
        fill: &ConnectedFill,
    ) -> Result<FillRegion, LayerError> {
        let layer = &self.layers[self.require(target.layer)?];
        let mut source = match (fill.reference, target.part) {
            (FillReference::Composite, _) => Source::Composite(Box::default()),
            (FillReference::Target, LayerPart::Pixels) => Source::Pixels(&layer.pixels),
            (FillReference::Target, LayerPart::Mask) => Source::Mask(
                layer
                    .mask
                    .as_ref()
                    .ok_or(LayerError::NoMask(target.layer))?,
            ),
        };
        if target.part == LayerPart::Mask && layer.mask.is_none() {
            return Err(LayerError::NoMask(target.layer));
        }
        let canvas = self.bounds();
        let (sx, sy) = fill.seed;
        if sx < 0 || sy < 0 || sx >= canvas.x1 || sy >= canvas.y1 {
            return Ok(FillRegion::default());
        }
        let mut buf = vec![PremulRgba::TRANSPARENT; N];
        let seed_tile = TileCoord::containing(sx, sy);
        let seed_px = source.read(self, seed_tile, &mut buf).unwrap_or_else(|| {
            let (ox, oy) = seed_tile.origin();
            buf[(sy - oy) as usize * TILE_SIZE as usize + (sx - ox) as usize]
        });
        let seed = fill::seed_color(seed_px);
        let threshold = fill.tolerance.threshold16();
        let load = |c: TileCoord| -> TileDistances {
            let rect = c.rect();
            let clip = rect.intersect(&canvas);
            let coverage = self.selection_tile(c);
            if clip.is_empty() || matches!(coverage, TileCoverage::Empty) {
                return TileDistances::Uniform(FAR);
            }
            let uniform = source.read(self, c, &mut buf);
            let whole = clip == rect && matches!(coverage, TileCoverage::Full);
            if let (Some(u), true) = (uniform, whole) {
                return TileDistances::Uniform(fill::quantize(fill::distance(seed, u)));
            }
            let (ox, oy) = c.origin();
            let mut map = vec![FAR; N];
            let mut row = [0u8; TILE_SIZE as usize];
            for y in (clip.y0 - oy) as u32..(clip.y1 - oy) as u32 {
                coverage.row(y, &mut row);
                for x in (clip.x0 - ox) as u32..(clip.x1 - ox) as u32 {
                    if row[x as usize] == 0 {
                        continue;
                    }
                    let i = (y * TILE_SIZE + x) as usize;
                    let p = uniform.unwrap_or(buf[i]);
                    map[i] = fill::quantize(fill::distance(seed, p));
                }
            }
            TileDistances::from_map(map)
        };
        let mut grid = Grid::new(canvas, threshold, load);
        if !grid.flood(fill.seed) {
            return Ok(FillRegion::default());
        }
        Ok(grid.into_region(fill.anti_alias))
    }

    /// Fills the region of a connected fill (see [`Document::fill_region`])
    /// with `paint`, limited by the selection's coverage (BK28), as **one**
    /// history step ([`HistoryAction::Fill`]). Colour paints over the
    /// pixels (recolouring them keeping alpha when the layer's alpha is
    /// locked, BK23), erase removes them (BK24); on a mask, colour reveals
    /// and erase hides (BK25). The layer's opacity, blend mode and
    /// clipping are not involved: they apply when composing (BK20–BK22).
    ///
    /// Refused, changing nothing, on a hidden or locked layer or a missing
    /// or disabled mask ([`Document::paint_target`], BK31). A fill that
    /// changes nothing (seed off the canvas or the selection, the region
    /// already that colour) records nothing. Returns the tiles that
    /// changed.
    pub fn fill_connected(
        &mut self,
        target: FillTarget,
        fill: &ConnectedFill,
        paint: FillPaint,
    ) -> Result<DirtyTiles, LayerError> {
        self.paint_target(target.layer, target.part)?;
        let region = self.fill_region(target, fill)?;
        self.apply_fill(target, &region, paint)
    }

    /// Fills the whole selection with `paint` (its coverage scales what is
    /// written), or the whole canvas without a selection (Edit › Fill,
    /// BK32), as one history step. The same paint rules and refusals as
    /// [`Document::fill_connected`].
    pub fn fill_selection(
        &mut self,
        target: FillTarget,
        paint: FillPaint,
    ) -> Result<DirtyTiles, LayerError> {
        self.paint_target(target.layer, target.part)?;
        let rect = self
            .selection
            .as_ref()
            .map_or_else(|| self.bounds(), |s| s.bounds());
        self.apply_fill(target, &FillRegion::rect(rect), paint)
    }

    /// Paints `region` into the target, one step if anything changed.
    fn apply_fill(
        &mut self,
        target: FillTarget,
        region: &FillRegion,
        paint: FillPaint,
    ) -> Result<DirtyTiles, LayerError> {
        if region.is_empty() {
            return Ok(DirtyTiles::new());
        }
        let i = self.require_unlocked(target.layer)?;
        let mut tiles = region.tiles();
        let mut m = vec![0.0f32; N];
        let mut row = [0u8; TILE_SIZE as usize];
        // Region coverage × selection coverage, per pixel of tile `c`.
        let mut coverage = |c: TileCoord, within: &TileCoverage, m: &mut [f32]| {
            if !region.tile_coverage(c, m) {
                return false;
            }
            if !matches!(within, TileCoverage::Full) {
                for (y, line) in m
                    .as_chunks_mut::<{ TILE_SIZE as usize }>()
                    .0
                    .iter_mut()
                    .enumerate()
                {
                    within.row(y as u32, &mut row);
                    for (v, &s) in line.iter_mut().zip(&row) {
                        *v *= f32::from(s) / 255.0;
                    }
                }
            }
            true
        };
        let edit = match target.part {
            LayerPart::Pixels => {
                let layer = &self.layers[i];
                let locked = layer.alpha_locked;
                // Where the paint cannot create content, absent tiles stay
                // absent without being visited (invariant FL10).
                if locked || paint == FillPaint::Erase {
                    tiles = only_present(&tiles, |c| layer.pixels.has_tile(c));
                }
                with_pixel_type!(self.depth, P => {
                    self.tile_edit::<P>(target.layer, &tiles, true, |c, tile, within| {
                        if coverage(c, &within, &mut m) {
                            paint_tile(tile, &m, paint, locked);
                        }
                    })?
                })
            }
            LayerPart::Mask => {
                // An absent mask tile reveals everything: revealing more
                // cannot change it.
                if let (FillPaint::Color(_), Some(mask)) = (paint, &self.layers[i].mask) {
                    tiles = only_present(&tiles, |c| mask.hidden_tile(c).is_some());
                }
                self.mask_edit(target.layer, &tiles, true, |c, tile, within| {
                    if coverage(c, &within, &mut m) {
                        paint_mask_tile(tile, &m, paint);
                    }
                })?
            }
        };
        if edit.changed.is_empty() {
            return Ok(DirtyTiles::new());
        }
        self.record(
            HistoryAction::Fill,
            vec![edit.change],
            self.active,
            edit.cost,
        );
        Ok(if edit.shown {
            edit.changed
        } else {
            DirtyTiles::new()
        })
    }
}

fn only_present(tiles: &DirtyTiles, present: impl Fn(TileCoord) -> bool) -> DirtyTiles {
    let mut out = DirtyTiles::new();
    for c in tiles.iter().filter(|&c| present(c)) {
        out.mark(c);
    }
    out
}

/// Paints one stored tile where `m > 0`, quantizing each changed pixel
/// once at the document's depth (16-bit content never passes through 8
/// bits, invariant FL8); untouched pixels keep their stored values.
fn paint_tile<P: StoragePixel>(tile: &mut Tile<P>, m: &[f32], paint: FillPaint, locked: bool) {
    for (p, &mi) in tile.pixels_mut().iter_mut().zip(m) {
        if mi > 0.0 {
            let mut px = [p.to_premul()];
            fill::paint_pixels(paint, locked, &mut px, &[mi]);
            *p = P::from_premul(px[0]);
        }
    }
}

/// Paints one tile of mask reveal values (as the user sees them) where
/// `m > 0`.
fn paint_mask_tile(tile: &mut Tile<u8>, m: &[f32], paint: FillPaint) {
    for (v, &mi) in tile.pixels_mut().iter_mut().zip(m) {
        if mi > 0.0 {
            let mut r = [f32::from(*v) / 255.0];
            fill::paint_mask(paint, &mut r, &[mi]);
            *v = tela_color::unorm8(r[0]);
        }
    }
}
