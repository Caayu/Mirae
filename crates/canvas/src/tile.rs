use std::collections::{BTreeSet, HashMap};

use crate::IRect;

/// Edge length of a square tile in pixels.
pub const TILE_SIZE: u32 = 256;
const TILE_PIXELS: usize = (TILE_SIZE * TILE_SIZE) as usize;

/// Tile index; tile `(x, y)` covers pixels `[x*256, x*256+256)` horizontally.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileCoord {
    pub x: i32,
    pub y: i32,
}

impl TileCoord {
    pub fn containing(px: i32, py: i32) -> Self {
        let t = TILE_SIZE as i32;
        Self {
            x: px.div_euclid(t),
            y: py.div_euclid(t),
        }
    }

    /// Top-left pixel of the tile.
    pub fn origin(self) -> (i32, i32) {
        let t = TILE_SIZE as i32;
        (self.x * t, self.y * t)
    }

    pub fn rect(self) -> IRect {
        let (x, y) = self.origin();
        let t = TILE_SIZE as i32;
        IRect::new(x, y, x + t, y + t)
    }
}

/// A square block of pixels, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile<P> {
    pixels: Box<[P]>,
}

impl<P: Copy + Default> Tile<P> {
    pub fn new() -> Self {
        Self::filled(P::default())
    }

    pub fn filled(p: P) -> Self {
        Self {
            pixels: vec![p; TILE_PIXELS].into_boxed_slice(),
        }
    }

    /// `x` and `y` are local to the tile.
    pub fn get(&self, x: u32, y: u32) -> P {
        self.pixels[(y * TILE_SIZE + x) as usize]
    }

    pub fn get_mut(&mut self, x: u32, y: u32) -> &mut P {
        &mut self.pixels[(y * TILE_SIZE + x) as usize]
    }

    pub fn pixels(&self) -> &[P] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [P] {
        &mut self.pixels
    }

    pub fn row(&self, y: u32) -> &[P] {
        let s = (y * TILE_SIZE) as usize;
        &self.pixels[s..s + TILE_SIZE as usize]
    }

    pub fn row_mut(&mut self, y: u32) -> &mut [P] {
        let s = (y * TILE_SIZE) as usize;
        &mut self.pixels[s..s + TILE_SIZE as usize]
    }
}

impl<P: Copy + Default> Default for Tile<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Copy + Default + PartialEq> Tile<P> {
    /// True when every pixel equals the default value, so the tile can be
    /// dropped from a sparse surface without changing its contents.
    pub fn is_blank(&self) -> bool {
        let d = P::default();
        self.pixels.iter().all(|p| *p == d)
    }
}

/// Unbounded sparse pixel plane. Absent tiles read as `P::default()`.
#[derive(Clone, Debug, Default)]
pub struct TiledSurface<P> {
    tiles: HashMap<TileCoord, Tile<P>>,
}

impl<P: Copy + Default> TiledSurface<P> {
    /// Empty plane (no tiles allocated).
    pub fn new() -> Self {
        Self {
            tiles: HashMap::new(),
        }
    }

    /// The tile at `c`, if allocated.
    pub fn tile(&self, c: TileCoord) -> Option<&Tile<P>> {
        self.tiles.get(&c)
    }

    pub fn tile_mut(&mut self, c: TileCoord) -> Option<&mut Tile<P>> {
        self.tiles.get_mut(&c)
    }

    /// Returns the tile, allocating a blank one first if needed.
    pub fn tile_or_insert(&mut self, c: TileCoord) -> &mut Tile<P> {
        self.tiles.entry(c).or_default()
    }

    pub fn insert_tile(&mut self, c: TileCoord, t: Tile<P>) -> Option<Tile<P>> {
        self.tiles.insert(c, t)
    }

    pub fn remove_tile(&mut self, c: TileCoord) -> Option<Tile<P>> {
        self.tiles.remove(&c)
    }

    /// Allocated tiles, in **unspecified** order (callers that need
    /// determinism sort or collect into [`DirtyTiles`]).
    pub fn tile_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles.keys().copied()
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
    }

    pub fn pixel(&self, x: i32, y: i32) -> P {
        let c = TileCoord::containing(x, y);
        let (ox, oy) = c.origin();
        self.tiles
            .get(&c)
            .map_or_else(P::default, |t| t.get((x - ox) as u32, (y - oy) as u32))
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, p: P) {
        let c = TileCoord::containing(x, y);
        let (ox, oy) = c.origin();
        *self
            .tile_or_insert(c)
            .get_mut((x - ox) as u32, (y - oy) as u32) = p;
    }
}

/// Set of tiles whose derived data (composite, GPU texture, ...) is stale.
/// Ordered so consumers process tiles deterministically.
#[derive(Clone, Debug, Default)]
pub struct DirtyTiles {
    tiles: BTreeSet<TileCoord>,
}

impl DirtyTiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark(&mut self, c: TileCoord) {
        self.tiles.insert(c);
    }

    pub fn mark_rect(&mut self, r: IRect) {
        self.tiles.extend(r.tiles());
    }

    pub fn extend(&mut self, other: &DirtyTiles) {
        self.tiles.extend(other.tiles.iter().copied());
    }

    pub fn contains(&self, c: TileCoord) -> bool {
        self.tiles.contains(&c)
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles.iter().copied()
    }

    pub fn take(&mut self) -> Vec<TileCoord> {
        std::mem::take(&mut self.tiles).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containing_handles_negative_coordinates() {
        assert_eq!(TileCoord::containing(0, 0), TileCoord { x: 0, y: 0 });
        assert_eq!(TileCoord::containing(255, 256), TileCoord { x: 0, y: 1 });
        assert_eq!(TileCoord::containing(-1, -256), TileCoord { x: -1, y: -1 });
        assert_eq!(TileCoord::containing(-257, 0), TileCoord { x: -2, y: 0 });
    }

    #[test]
    fn surface_is_sparse_and_reads_default() {
        let mut s = TiledSurface::<u8>::new();
        assert_eq!(s.pixel(1000, -1000), 0);
        assert_eq!(s.tile_count(), 0);
        s.set_pixel(-3, 700, 9);
        assert_eq!(s.pixel(-3, 700), 9);
        assert_eq!(s.tile_count(), 1);
        assert!(s.tile(TileCoord { x: -1, y: 2 }).is_some());
    }

    #[test]
    fn blank_detection() {
        let mut t = Tile::<f32>::new();
        assert!(t.is_blank());
        *t.get_mut(10, 20) = 0.5;
        assert!(!t.is_blank());
    }

    #[test]
    fn dirty_tiles_dedupe_and_drain() {
        let mut d = DirtyTiles::new();
        d.mark_rect(IRect::new(0, 0, 300, 10));
        d.mark(TileCoord { x: 1, y: 0 });
        assert_eq!(d.len(), 2);
        assert_eq!(d.take().len(), 2);
        assert!(d.is_empty());
    }
}
