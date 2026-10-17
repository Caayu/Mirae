//! The connected region: a span flood over lazily built distance maps.
//!
//! The flood is the classic span (scanline) fill with an explicit stack
//! (Heckbert's "seed fill" in *Graphics Gems*, 1990; the combined-scan
//! form): each popped span is extended left and right, and the runs found
//! push their neighbouring rows. No recursion, so no depth limit; the
//! stack holds spans, not pixels. Membership is a bitset per tile
//! touched, so the region of an empty 4096² canvas holds 2 MiB of bits.
//!
//! Distances come from `load`, called once per tile on first use: a tile
//! of one distance (absent, one colour, outside the canvas or the
//! selection) is a single value; others keep a `u16` map (128 KiB).

use std::collections::{BTreeMap, HashMap};

use tela_canvas::{IRect, TILE_SIZE, TileCoord};

use super::{FAR, FillRegion, FillStats, N, RegionTile, WORDS, edge_coverage};

/// One tile's distances from the seed on the 16-bit scale
/// ([`super::quantize`]); [`FAR`] outside the domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TileDistances {
    Uniform(u16),
    Map(Box<[u16]>),
}

impl TileDistances {
    /// A map, or a uniform value when every entry is the same.
    pub(crate) fn from_map(map: Vec<u16>) -> Self {
        debug_assert_eq!(map.len(), N);
        if map.iter().all(|&d| d == map[0]) {
            TileDistances::Uniform(map[0])
        } else {
            TileDistances::Map(map.into_boxed_slice())
        }
    }

    #[inline]
    fn get(&self, i: usize) -> u16 {
        match self {
            TileDistances::Uniform(d) => *d,
            TileDistances::Map(m) => m[i],
        }
    }
}

struct TileState {
    dist: TileDistances,
    bits: Option<Box<[u64; WORDS]>>,
}

/// The flood's working state over the canvas.
pub(crate) struct Grid<F: FnMut(TileCoord) -> TileDistances> {
    canvas: IRect,
    threshold: u16,
    index: HashMap<TileCoord, usize>,
    tiles: Vec<(TileCoord, TileState)>,
    load: F,
    /// The tile looked up last (most lookups hit it).
    last: Option<(TileCoord, usize)>,
    stack_peak: usize,
}

/// A span of row `y` from `x0` to `x1` (inclusive) to scan, reached from
/// row `y − dy`.
#[derive(Clone, Copy)]
struct Span {
    x0: i32,
    x1: i32,
    y: i32,
    dy: i32,
}

impl<F: FnMut(TileCoord) -> TileDistances> Grid<F> {
    /// `threshold`: the largest distance included (16-bit scale).
    pub(crate) fn new(canvas: IRect, threshold: u16, load: F) -> Self {
        Self {
            canvas,
            threshold,
            index: HashMap::new(),
            tiles: Vec::new(),
            load,
            last: None,
            stack_peak: 0,
        }
    }

    #[inline]
    fn slot(&mut self, c: TileCoord) -> usize {
        if let Some((lc, i)) = self.last
            && lc == c
        {
            return i;
        }
        let i = match self.index.get(&c) {
            Some(&i) => i,
            None => {
                let dist = (self.load)(c);
                self.tiles.push((c, TileState { dist, bits: None }));
                self.index.insert(c, self.tiles.len() - 1);
                self.tiles.len() - 1
            }
        };
        self.last = Some((c, i));
        i
    }

    #[inline]
    fn locate(x: i32, y: i32) -> (TileCoord, usize) {
        let c = TileCoord {
            x: x >> 8,
            y: y >> 8,
        };
        let i = ((y & 255) as u32 * TILE_SIZE + (x & 255) as u32) as usize;
        (c, i)
    }

    #[inline]
    fn on_canvas(&self, x: i32, y: i32) -> bool {
        x >= self.canvas.x0 && x < self.canvas.x1 && y >= self.canvas.y0 && y < self.canvas.y1
    }

    /// The distance at `(x, y)`; [`FAR`] off the canvas.
    pub(crate) fn distance(&mut self, x: i32, y: i32) -> u16 {
        if !self.on_canvas(x, y) {
            return FAR;
        }
        let (c, i) = Self::locate(x, y);
        let s = self.slot(c);
        self.tiles[s].1.dist.get(i)
    }

    /// Not filled yet and within the threshold.
    #[inline]
    fn inside(&mut self, x: i32, y: i32) -> bool {
        if !self.on_canvas(x, y) {
            return false;
        }
        let (c, i) = Self::locate(x, y);
        let s = self.slot(c);
        let t = &self.tiles[s].1;
        if t.bits
            .as_ref()
            .is_some_and(|b| b[i / 64] >> (i % 64) & 1 == 1)
        {
            return false;
        }
        t.dist.get(i) <= self.threshold
    }

    #[inline]
    fn set(&mut self, x: i32, y: i32) {
        let (c, i) = Self::locate(x, y);
        let s = self.slot(c);
        let bits = self.tiles[s]
            .1
            .bits
            .get_or_insert_with(|| Box::new([0; WORDS]));
        bits[i / 64] |= 1 << (i % 64);
    }

    /// Floods from `seed`; returns false (filling nothing) when the seed
    /// itself is outside the domain or the threshold.
    pub(crate) fn flood(&mut self, seed: (i32, i32)) -> bool {
        let (sx, sy) = seed;
        if !self.inside(sx, sy) {
            return false;
        }
        let mut stack = vec![
            Span {
                x0: sx,
                x1: sx,
                y: sy,
                dy: 1,
            },
            Span {
                x0: sx,
                x1: sx,
                y: sy - 1,
                dy: -1,
            },
        ];
        while let Some(Span { mut x0, x1, y, dy }) = stack.pop() {
            let mut x = x0;
            if self.inside(x, y) {
                while self.inside(x - 1, y) {
                    self.set(x - 1, y);
                    x -= 1;
                }
                if x < x0 {
                    stack.push(Span {
                        x0: x,
                        x1: x0 - 1,
                        y: y - dy,
                        dy: -dy,
                    });
                }
            }
            while x0 <= x1 {
                while self.inside(x0, y) {
                    self.set(x0, y);
                    x0 += 1;
                }
                if x0 > x {
                    stack.push(Span {
                        x0: x,
                        x1: x0 - 1,
                        y: y + dy,
                        dy,
                    });
                }
                if x0 - 1 > x1 {
                    stack.push(Span {
                        x0: x1 + 1,
                        x1: x0 - 1,
                        y: y - dy,
                        dy: -dy,
                    });
                }
                x0 += 1;
                while x0 < x1 && !self.inside(x0, y) {
                    x0 += 1;
                }
                x = x0;
            }
            self.stack_peak = self.stack_peak.max(stack.len());
        }
        true
    }

    /// The flooded region and, with `anti_alias`, its edge: every pixel
    /// outside it, 4-adjacent to it, with coverage above 0 (BK15, BK16).
    pub(crate) fn into_region(mut self, anti_alias: bool) -> FillRegion {
        let mut region: BTreeMap<TileCoord, RegionTile> = BTreeMap::new();
        let mut maps = 0;
        let mut bitsets = 0;
        for (c, t) in &self.tiles {
            maps += usize::from(matches!(t.dist, TileDistances::Map(_)));
            if let Some(b) = &t.bits {
                bitsets += 1;
                let tile = if b.iter().all(|&w| w == u64::MAX) {
                    RegionTile::Full
                } else {
                    RegionTile::Bits(b.clone())
                };
                region.insert(*c, tile);
            }
        }
        let mut edge = BTreeMap::new();
        if anti_alias {
            edge = self.edge(&region);
        }
        let edge_bytes: usize = edge.values().map(|e: &Vec<(u16, f32)>| e.len() * 8).sum();
        let stats = FillStats {
            tiles_read: self.tiles.len(),
            maps,
            temp_bytes: maps * N * 2
                + bitsets * WORDS * 8
                + self.stack_peak * std::mem::size_of::<Span>()
                + edge_bytes,
            ..FillStats::default()
        };
        FillRegion::new(region, edge, stats)
    }

    /// The anti-aliased edge of `region`, tile by tile, from whole rows of
    /// bits: a pixel is on the edge when it is not in the region and one of
    /// its four neighbours is (the neighbours across a tile border come
    /// from the adjacent tiles' bits).
    fn edge(
        &mut self,
        region: &BTreeMap<TileCoord, RegionTile>,
    ) -> BTreeMap<TileCoord, Vec<(u16, f32)>> {
        const FULL: [u64; 4] = [u64::MAX; 4];
        let row = |c: TileCoord, y: u32| -> [u64; 4] {
            match region.get(&c) {
                None => [0; 4],
                Some(RegionTile::Full) => FULL,
                Some(RegionTile::Bits(w)) => {
                    let k = y as usize * 4;
                    [w[k], w[k + 1], w[k + 2], w[k + 3]]
                }
            }
        };
        let mut candidates: Vec<TileCoord> = Vec::new();
        for c in region.keys() {
            for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
                candidates.push(TileCoord {
                    x: c.x + dx,
                    y: c.y + dy,
                });
            }
        }
        candidates.sort();
        candidates.dedup();
        let mut edge = BTreeMap::new();
        for c in candidates {
            if c.rect().intersect(&self.canvas).is_empty() {
                continue;
            }
            let (ox, oy) = c.origin();
            let (left, right) = (TileCoord { x: c.x - 1, ..c }, TileCoord { x: c.x + 1, ..c });
            let (up, down) = (TileCoord { y: c.y - 1, ..c }, TileCoord { y: c.y + 1, ..c });
            let mut out = Vec::new();
            for y in 0..TILE_SIZE {
                let cur = row(c, y);
                let above = if y > 0 {
                    row(c, y - 1)
                } else {
                    row(up, TILE_SIZE - 1)
                };
                let below = if y + 1 < TILE_SIZE {
                    row(c, y + 1)
                } else {
                    row(down, 0)
                };
                let lcarry = row(left, y)[3] >> 63;
                let rcarry = row(right, y)[0] & 1;
                for w in 0..4 {
                    let from_left = cur[w] << 1 | if w > 0 { cur[w - 1] >> 63 } else { lcarry };
                    let from_right = cur[w] >> 1
                        | if w < 3 {
                            (cur[w + 1] & 1) << 63
                        } else {
                            rcarry << 63
                        };
                    let mut ring = (from_left | from_right | above[w] | below[w]) & !cur[w];
                    while ring != 0 {
                        let x = (w * 64) as u32 + ring.trailing_zeros();
                        ring &= ring - 1;
                        let d = self.distance(ox + x as i32, oy + y as i32);
                        let v = edge_coverage(d, self.threshold);
                        if v > 0.0 {
                            out.push(((y * TILE_SIZE + x) as u16, v));
                        }
                    }
                }
            }
            if !out.is_empty() {
                edge.insert(c, out);
            }
        }
        edge
    }
}
