//! Selections (phase 5C, ADR 0024): which part of the canvas edits may
//! change, as an 8-bit coverage per pixel (0 = not selected, 255 = fully
//! selected, values between = partly: anti-aliased edges, feathering).
//!
//! The rules come from the reference application, measured before they
//! were written (docs/parity/5C-selection.md): shapes are clipped to the
//! canvas; Add, Subtract and Intersect are the saturating sum, the
//! saturating difference and the truncated product of 8-bit coverages
//! (S14–S16); an empty result is no selection (S17, S18); the feather is a
//! discrete Gaussian renormalized over the canvas (S19–S21); a stroke's
//! alpha is multiplied by the coverage (S22–S26).

mod feather;
mod outline;
mod raster;

use tela_canvas::{IRect, TILE_SIZE, Tile, TileCoord, TileCoverage, TiledSurface};

pub use self::feather::MAX_FEATHER;
pub(crate) use self::feather::feather_tiles;
pub use self::outline::OUTLINE_THRESHOLD;
pub(crate) use self::raster::rasterize;

/// How a new shape combines with the current selection (the reference's
/// "Modo de seleção": New, Add, Subtract, Intersect).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionOp {
    /// The shape replaces the selection.
    #[default]
    Replace,
    /// `min(255, a + b)`: covered by either.
    Add,
    /// `max(0, a − b)`: the shape is removed.
    Subtract,
    /// `⌊a·b / 255⌋`: covered by both.
    Intersect,
}

impl SelectionOp {
    /// The measured 8-bit rule (parity record 5C, S14–S16). `Replace`
    /// keeps the shape.
    pub fn combine(self, a: u8, b: u8) -> u8 {
        match self {
            SelectionOp::Replace => b,
            SelectionOp::Add => a.saturating_add(b),
            SelectionOp::Subtract => a.saturating_sub(b),
            SelectionOp::Intersect => ((u16::from(a) * u16::from(b)) / 255) as u8,
        }
    }
}

/// A selection shape in document pixels, as a selection tool produces it
/// (the tool maps screen points to the document once, through the view).
#[derive(Clone, Debug, PartialEq)]
pub enum SelectionShape {
    /// An axis-aligned rectangle between two corners (any order). Its
    /// corners snap to the nearest pixel corner, so it is never partial
    /// (reference S7, S8).
    Rect([f32; 2], [f32; 2]),
    /// A closed polygon (lasso, a rectangle drawn in a rotated view),
    /// filled with the even-odd rule (reference S11). Closed implicitly.
    Polygon(Vec<[f32; 2]>),
    /// An ellipse: centre, semi-axes and the rotation of its first axis
    /// (radians, counter-clockwise in document space).
    Ellipse {
        center: [f32; 2],
        radii: [f32; 2],
        rotation: f32,
    },
}

/// A non-empty selection: coverage over the canvas, 0 outside it.
///
/// Stored sparsely as `coverage XOR fill`, where `fill` is 0 or 255: an
/// absent tile is `fill` everywhere, so selecting everything and inverting
/// touch no tile. Stored tiles are never blank (they would equal `fill`)
/// and their pixels outside the canvas are 0.
#[derive(Clone, Debug)]
pub struct Selection {
    stored: TiledSurface<u8>,
    inverted: bool,
    canvas: IRect,
}

/// Equal storage: the same coverage everywhere (storage is canonical:
/// blank tiles are never kept).
impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&self.stored, &other.stored);
        self.inverted == other.inverted
            && self.canvas == other.canvas
            && a.tile_count() == b.tile_count()
            && a.tile_coords().all(|c| a.tile(c) == b.tile(c))
    }
}

impl Selection {
    /// Everything on a canvas (no tile stored).
    pub(crate) fn all(canvas: IRect) -> Self {
        Self {
            stored: TiledSurface::new(),
            inverted: true,
            canvas,
        }
    }

    /// A selection with exactly the coverage `tiles` holds (absent = 0;
    /// values outside the canvas must be 0). May be empty.
    pub(crate) fn from_coverage(canvas: IRect, tiles: TiledSurface<u8>) -> Self {
        Self {
            stored: tiles,
            inverted: false,
            canvas,
        }
    }

    fn fill(&self) -> u8 {
        if self.inverted { 255 } else { 0 }
    }

    /// The canvas the selection is clipped to.
    pub fn canvas(&self) -> IRect {
        self.canvas
    }

    /// Coverage of document pixel `(x, y)`, `0..=255`; 0 outside the
    /// canvas.
    pub fn coverage(&self, x: i32, y: i32) -> u8 {
        let c = &self.canvas;
        if x < c.x0 || y < c.y0 || x >= c.x1 || y >= c.y1 {
            return 0;
        }
        self.stored.pixel(x, y) ^ self.fill()
    }

    /// The coverage of one tile, for edits (no allocation).
    pub fn tile(&self, coord: TileCoord) -> TileCoverage<'_> {
        let rect = coord.rect();
        let clip = rect.intersect(&self.canvas);
        if clip.is_empty() {
            return TileCoverage::Empty;
        }
        let stored = self.stored.tile(coord);
        match (stored, self.inverted) {
            (None, false) => TileCoverage::Empty,
            (None, true) if clip == rect => TileCoverage::Full,
            _ => TileCoverage::Partial {
                stored,
                xor: self.fill(),
                clip: [
                    (clip.x0 - rect.x0) as u32,
                    (clip.y0 - rect.y0) as u32,
                    (clip.x1 - rect.x0) as u32,
                    (clip.y1 - rect.y0) as u32,
                ],
            },
        }
    }

    /// Coverage of row `y` from `x0` for `out.len()` pixels (0 off the
    /// canvas), read a tile row at a time.
    pub(crate) fn read_line(&self, x0: i32, y: i32, out: &mut [u8]) {
        let mut row = [0u8; TILE_SIZE as usize];
        let end = x0 + out.len() as i32;
        let mut x = x0;
        while x < end {
            let c = TileCoord::containing(x, y);
            let (ox, oy) = c.origin();
            let seg_end = (ox + TILE_SIZE as i32).min(end);
            self.tile(c).row((y - oy) as u32, &mut row);
            out[(x - x0) as usize..(seg_end - x0) as usize]
                .copy_from_slice(&row[(x - ox) as usize..(seg_end - ox) as usize]);
            x = seg_end;
        }
    }

    /// Whether no pixel has any coverage. A [`Document`](crate::Document)
    /// never keeps an empty selection (it becomes "no selection").
    pub fn is_empty(&self) -> bool {
        if !self.inverted {
            return self.stored.tile_count() == 0;
        }
        // Everything is selected except what stored tiles take away: empty
        // only if every canvas tile is stored and takes everything away.
        self.canvas.tiles().all(|c| {
            let Some(t) = self.stored.tile(c) else {
                return false;
            };
            let clip = c.rect().intersect(&self.canvas);
            let (ox, oy) = c.origin();
            (clip.y0..clip.y1).all(|y| {
                t.row((y - oy) as u32)[(clip.x0 - ox) as usize..(clip.x1 - ox) as usize]
                    .iter()
                    .all(|&v| v == 255)
            })
        })
    }

    /// Smallest rectangle holding every pixel with coverage, computed from
    /// the stored tiles each time (never out of date). Empty if the
    /// selection is.
    pub fn bounds(&self) -> IRect {
        let mut bounds = IRect::default();
        let fill = self.fill();
        for c in self.canvas.tiles() {
            let clip = c.rect().intersect(&self.canvas);
            match self.stored.tile(c) {
                None if self.inverted => bounds = bounds.union(&clip),
                None => {}
                Some(t) => {
                    let (ox, oy) = c.origin();
                    for y in clip.y0..clip.y1 {
                        let row = t.row((y - oy) as u32);
                        for x in clip.x0..clip.x1 {
                            if row[(x - ox) as usize] != fill {
                                bounds = bounds.union(&IRect::new(x, y, x + 1, y + 1));
                            }
                        }
                    }
                }
            }
        }
        bounds
    }

    /// Bounds of the pixels whose coverage differs from the fill (where a
    /// feather can change anything).
    fn stored_bounds(&self) -> IRect {
        let mut bounds = IRect::default();
        for c in self.stored.tile_coords() {
            let t = self.stored.tile(c).expect("listed");
            let (ox, oy) = c.origin();
            for y in 0..TILE_SIZE {
                let row = t.row(y);
                if let Some(x0) = row.iter().position(|&v| v != 0) {
                    let x1 = row.iter().rposition(|&v| v != 0).expect("found one") + 1;
                    let y = oy + y as i32;
                    bounds = bounds.union(&IRect::new(ox + x0 as i32, y, ox + x1 as i32, y + 1));
                }
            }
        }
        bounds
    }

    /// Whether everything outside the stored tiles is selected.
    pub fn is_inverted_storage(&self) -> bool {
        self.inverted
    }

    /// Number of stored tiles (64 KiB each).
    pub fn stored_tile_count(&self) -> usize {
        self.stored.tile_count()
    }

    /// Bytes of tile buffers held.
    pub(crate) fn bytes(&self) -> usize {
        self.stored.tile_count() * MASK_TILE_BYTES
    }

    /// Stored tiles in a deterministic order (digests, tests).
    #[cfg(test)]
    pub(crate) fn sorted_tiles(&self) -> Vec<(TileCoord, &Tile<u8>)> {
        let mut v: Vec<_> = self
            .stored
            .tile_coords()
            .map(|c| (c, self.stored.tile(c).expect("listed")))
            .collect();
        v.sort_unstable_by_key(|(c, _)| *c);
        v
    }

    /// The stored tiles `op` with `shape` (coverage, absent = 0, 0 outside
    /// the canvas) produces where `shape` has tiles, for Add and Subtract,
    /// which never change the fill: `(coord, new stored tile or None)`.
    /// Reads the current tiles; changes nothing.
    pub(crate) fn combined_tiles(
        &self,
        shape: &TiledSurface<u8>,
        op: SelectionOp,
    ) -> Vec<(TileCoord, Option<Tile<u8>>)> {
        debug_assert!(matches!(op, SelectionOp::Add | SelectionOp::Subtract));
        let fill = self.fill();
        let mut coords: Vec<TileCoord> = shape.tile_coords().collect();
        coords.sort_unstable();
        coords
            .into_iter()
            .filter_map(|c| {
                let s = shape.tile(c).expect("listed");
                let old = self.stored.tile(c);
                let mut new = Tile::<u8>::new();
                let clip = c.rect().intersect(&self.canvas);
                let (ox, oy) = c.origin();
                for y in clip.y0..clip.y1 {
                    let ly = (y - oy) as u32;
                    for x in clip.x0..clip.x1 {
                        let lx = (x - ox) as u32;
                        let a = old.map_or(0, |t| t.get(lx, ly)) ^ fill;
                        *new.get_mut(lx, ly) = op.combine(a, s.get(lx, ly)) ^ fill;
                    }
                }
                let new = (!new.is_blank()).then_some(new);
                (new.as_ref() != old).then_some((c, new))
            })
            .collect()
    }

    /// The intersection with `shape`, built anew (its fill is 0 whatever
    /// this one's is).
    pub(crate) fn intersected(&self, shape: &TiledSurface<u8>) -> Selection {
        let mut out = TiledSurface::new();
        let mut row = [0u8; TILE_SIZE as usize];
        for c in shape.tile_coords() {
            let s = shape.tile(c).expect("listed");
            let current = self.tile(c);
            let mut new = Tile::<u8>::new();
            for y in 0..TILE_SIZE {
                current.row(y, &mut row);
                for ((n, &b), &a) in new.row_mut(y).iter_mut().zip(s.row(y)).zip(&row) {
                    *n = SelectionOp::Intersect.combine(a, b);
                }
            }
            if !new.is_blank() {
                out.insert_tile(c, new);
            }
        }
        Selection::from_coverage(self.canvas, out)
    }

    /// Swaps stored tiles with `tiles` (`None` = absent): applying the
    /// same list twice restores the selection exactly.
    pub(crate) fn swap_tiles(&mut self, tiles: &mut [(TileCoord, Option<Tile<u8>>)]) {
        for (coord, held) in tiles.iter_mut() {
            let current = self.stored.remove_tile(*coord);
            if let Some(t) = held.take() {
                self.stored.insert_tile(*coord, t);
            }
            *held = current;
        }
    }

    /// Flips the selection (coverage `255 − c` on the canvas) without
    /// touching a tile.
    pub(crate) fn invert(&mut self) {
        self.inverted = !self.inverted;
    }

    /// Feathers by `strength` px into the tiles that change (see
    /// [`feather_tiles`]); changes nothing itself.
    pub(crate) fn feathered_tiles(&self, strength: f32) -> Vec<(TileCoord, Option<Tile<u8>>)> {
        feather_tiles(self, strength)
    }

    /// Checks the storage rules (tests).
    #[cfg(test)]
    pub(crate) fn check(&self) {
        for (c, t) in self.sorted_tiles() {
            assert!(!t.is_blank(), "SEL1: no blank stored tile at {c:?}");
            let clip = c.rect().intersect(&self.canvas);
            assert!(!clip.is_empty(), "stored tile off the canvas: {c:?}");
            let (ox, oy) = c.origin();
            for y in 0..TILE_SIZE as i32 {
                for x in 0..TILE_SIZE as i32 {
                    let inside = x + ox >= clip.x0
                        && x + ox < clip.x1
                        && y + oy >= clip.y0
                        && y + oy < clip.y1;
                    if !inside {
                        assert_eq!(t.get(x as u32, y as u32), 0, "off-canvas bytes are 0");
                    }
                }
            }
        }
    }
}

/// Bytes of one 8-bit tile.
pub(crate) const MASK_TILE_BYTES: usize = (TILE_SIZE * TILE_SIZE) as usize;

#[cfg(test)]
mod parity;
#[cfg(test)]
mod tests;
