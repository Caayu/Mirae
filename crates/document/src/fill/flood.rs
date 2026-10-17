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

impl TileState {
    /// Pixel `i` is within the threshold and not filled yet.
    #[inline]
    fn open(&self, i: usize, threshold: u16) -> bool {
        self.dist.get(i) <= threshold
            && self
                .bits
                .as_ref()
                .is_none_or(|b| b[i / 64] >> (i % 64) & 1 == 0)
    }

    /// How many pixels from `i` (stepping by `step`, +1 or −1, at most
    /// `n`, within one tile row) are open: a uniform tile answers without
    /// looking at pixels until something is filled in it.
    #[inline]
    fn run(&self, i: usize, n: usize, step: isize, threshold: u16) -> usize {
        match (&self.dist, &self.bits) {
            (TileDistances::Uniform(d), _) if *d > threshold => 0,
            (TileDistances::Uniform(_), None) => n,
            (TileDistances::Uniform(_), Some(b)) => unfilled_run(b, i, n, step),
            _ => (0..n)
                .position(|k| !self.open(i.wrapping_add_signed(step * k as isize), threshold))
                .unwrap_or(n),
        }
    }

    /// Marks pixels `i..i + n` (one tile row) as filled, a word at a time.
    fn set_run(&mut self, mut i: usize, n: usize) {
        if n == 0 {
            return;
        }
        let bits = self.bits.get_or_insert_with(|| Box::new([0; WORDS]));
        let end = i + n;
        while i < end {
            let (w, b) = (i / 64, i % 64);
            let take = (64 - b).min(end - i);
            let mask = if take == 64 {
                u64::MAX
            } else {
                ((1u64 << take) - 1) << b
            };
            bits[w] |= mask;
            i += take;
        }
    }
}

/// How many bits from `i` (stepping by `step`, at most `n`) are 0, a word
/// at a time.
fn unfilled_run(bits: &[u64; WORDS], i: usize, n: usize, step: isize) -> usize {
    let mut done = 0;
    while done < n {
        let at = i.wrapping_add_signed(step * done as isize);
        let (w, b) = (at / 64, at % 64);
        if step > 0 {
            let take = (64 - b).min(n - done);
            let word = bits[w] >> b;
            let word = if take == 64 {
                word
            } else {
                word & ((1 << take) - 1)
            };
            if word != 0 {
                return done + word.trailing_zeros() as usize;
            }
            done += take;
        } else {
            let take = (b + 1).min(n - done);
            // Bits b, b − 1, …, b − take + 1, highest first.
            let word = bits[w] << (63 - b);
            let word = if take == 64 {
                word
            } else {
                word & !(u64::MAX >> take)
            };
            if word != 0 {
                return done + word.leading_zeros() as usize;
            }
            done += take;
        }
    }
    n
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

    /// Fills row `y` rightwards from `x` while the pixels are inside;
    /// returns the first `x` that is not (`x` itself if it is not). Runs
    /// a tile row at a time: a whole uniform row in a few word writes.
    fn extend_right(&mut self, mut x: i32, y: i32) -> i32 {
        loop {
            if !self.on_canvas(x, y) {
                return x;
            }
            let (c, i) = Self::locate(x, y);
            let s = self.slot(c);
            let n = (((x | 255) + 1).min(self.canvas.x1) - x) as usize;
            let threshold = self.threshold;
            let t = &mut self.tiles[s].1;
            let run = t.run(i, n, 1, threshold);
            t.set_run(i, run);
            x += run as i32;
            if run < n {
                return x;
            }
        }
    }

    /// Fills row `y` leftwards from `x` (inclusive) while the pixels are
    /// inside; returns the leftmost pixel filled (`x + 1` if none).
    fn extend_left(&mut self, mut x: i32, y: i32) -> i32 {
        loop {
            if !self.on_canvas(x, y) {
                return x + 1;
            }
            let (c, i) = Self::locate(x, y);
            let s = self.slot(c);
            let n = (x - (x & !255).max(self.canvas.x0) + 1) as usize;
            let threshold = self.threshold;
            let t = &mut self.tiles[s].1;
            let run = t.run(i, n, -1, threshold);
            t.set_run(i + 1 - run, run);
            x -= run as i32;
            if run < n {
                return x + 1;
            }
        }
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
                x = self.extend_left(x0 - 1, y);
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
                x0 = self.extend_right(x0, y);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfilled_runs_match_a_bit_by_bit_count() {
        let mut bits = Box::new([0u64; WORDS]);
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        for w in bits.iter_mut().take(16) {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            // Sparse bits so runs are long.
            *w = seed & (seed >> 3) & (seed >> 5);
        }
        let bit = |i: usize| bits[i / 64] >> (i % 64) & 1 == 1;
        for row in 0..4 {
            let base: usize = row * 256;
            for start in 0..256 {
                for (step, n) in [(1isize, 256 - start), (-1, start + 1)] {
                    let i = base + start;
                    let expected = (0..n)
                        .position(|k| bit(i.wrapping_add_signed(step * k as isize)))
                        .unwrap_or(n);
                    assert_eq!(
                        unfilled_run(&bits, i, n, step),
                        expected,
                        "row {row} from {start} step {step}"
                    );
                }
            }
        }
    }
}
