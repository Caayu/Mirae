//! Fill (phase 5E, ADR 0026): which pixels a fill changes, and how much.
//!
//! A **connected fill** (the bucket) starts at a seed pixel and takes every
//! pixel 4-connected to it whose colour in the *reference* is within the
//! tolerance of the seed's colour there, inside the *domain* (the canvas,
//! and the selection's pixels of coverage above 0). The reference is what
//! decides the region; the *target* is what changes, so a fill can read
//! the composed picture and write an empty layer. A **selection fill**
//! (Edit › Fill) takes the whole domain. Both are measured rules of the
//! reference application (docs/parity/5E-fill.md, BK5–BK35):
//!
//! - distance: the largest difference of the straight (un-premultiplied)
//!   channels R, G, B, A; when either pixel is fully transparent only the
//!   alpha difference counts (colour under alpha 0 is invisible, BK13);
//! - included: distance ≤ `min(round(2.55 · t), 254) / 255` for a
//!   tolerance of `t` percent (BK11, BK14), compared with the seed's
//!   colour, not with neighbours (BK10);
//! - anti-aliasing: every pixel outside the region but 4-adjacent to it
//!   gets coverage `(1 − d) / (1 − T)` (BK15–BK17);
//! - the selection's coverage multiplies what is written (BK28).
//!
//! The region is computed tile by tile from per-tile distance maps built
//! on first use, so absent tiles and tiles of one colour cost no memory
//! and nothing is materialized in the document; the flood is an explicit
//! span stack (no recursion, any canvas size).

use std::collections::BTreeMap;

use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, TileCoord};
use tela_color::{Color, PremulRgba};

mod flood;
#[cfg(test)]
mod parity;
#[cfg(test)]
mod tests;

pub(crate) use flood::{Grid, TileDistances};

use crate::{LayerId, LayerPart};

/// Pixels per tile.
const N: usize = (TILE_SIZE * TILE_SIZE) as usize;
/// 64-bit words per tile bitset.
const WORDS: usize = N / 64;
/// A distance that never matches and gives no anti-aliased coverage:
/// outside the canvas or the selection.
pub(crate) const FAR: u16 = u16::MAX;

/// How different a pixel may be from the seed and still be filled, in
/// tenths of a percent (`0..=1000`, the reference's field: 0–100 with one
/// decimal, default 10, BK2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tolerance(u16);

impl Tolerance {
    /// The reference's default, 10 %.
    pub const DEFAULT: Self = Self(100);
    /// Only pixels of the seed's exact colour.
    pub const ZERO: Self = Self(0);
    /// 100 %: everything but pixels a full 255 apart in a channel (BK14).
    pub const MAX: Self = Self(1000);

    /// From a percentage, rounded to a tenth and clamped to `0..=100`
    /// (not a number is 0).
    pub fn from_percent(percent: f32) -> Self {
        if percent.is_nan() {
            return Self::ZERO;
        }
        Self((percent.clamp(0.0, 100.0) * 10.0).round() as u16)
    }

    /// From tenths of a percent, clamped to `0..=1000`.
    pub fn from_tenths(tenths: u16) -> Self {
        Self(tenths.min(1000))
    }

    pub fn tenths(self) -> u16 {
        self.0
    }

    pub fn percent(self) -> f32 {
        f32::from(self.0) / 10.0
    }

    /// The largest 8-bit channel difference included: `round(2.55 · t)`,
    /// halves up, at most 254 (measured: 0.1 % → 0, 0.5 % → 1, 1 % → 3,
    /// 10 % → 26, 100 % → 254; BK11, BK14).
    pub fn threshold8(self) -> u8 {
        ((u32::from(self.0) * 255 + 500) / 1000).min(254) as u8
    }

    /// [`Tolerance::threshold8`] on the 16-bit distance scale
    /// (`d · 65535`): an 8-bit level is 257 steps, so 8-bit content
    /// compares exactly as the reference's integers do.
    pub(crate) fn threshold16(self) -> u16 {
        u16::from(self.threshold8()) * 257
    }
}

impl Default for Tolerance {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What a connected fill compares colours in (the *reference*; what it
/// changes is the [`FillTarget`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillReference {
    /// The target alone: the layer's pixels, or its mask's values when the
    /// mask is the target ("refer only to the editing layer", BK2, BK25).
    Target,
    /// Every visible layer composed over transparency, without the paper,
    /// with opacity, blend modes, masks and clipping applied ("refer to
    /// other layers" with all layers, BK3, BK29, BK30).
    #[default]
    Composite,
}

/// What a fill changes: a layer's pixels or its mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillTarget {
    pub layer: LayerId,
    pub part: LayerPart,
}

/// What a fill puts into its region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillPaint {
    /// Paint the opaque colour (premultiplied `C·m` over the pixel, or
    /// recoloured keeping alpha on an alpha-locked layer, BK19, BK23). On
    /// a mask it reveals; the colour does not matter there (BK25).
    Color(Color),
    /// Remove: the region becomes transparent (the reference's transparent
    /// colour, BK24), or hidden on a mask.
    Erase,
}

/// A connected (bucket) fill: where it starts and how it decides its
/// region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConnectedFill {
    /// The document pixel clicked.
    pub seed: (i32, i32),
    pub reference: FillReference,
    pub tolerance: Tolerance,
    /// Partly cover the pixels just outside the region (BK15).
    pub anti_alias: bool,
}

/// How much a fill computation read and held, for benchmarks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FillStats {
    /// Pixels in the region (coverage 1 before the selection).
    pub pixels: u64,
    /// Anti-aliased edge pixels with coverage above 0.
    pub edge_pixels: u64,
    /// Reference tiles read (decoded or composed) while flooding.
    pub tiles_read: usize,
    /// Distance maps kept (tiles of more than one distance).
    pub maps: usize,
    /// Peak temporary bytes: distance maps, region bitsets, the span stack
    /// and the edge list.
    pub temp_bytes: usize,
}

/// One tile of a region: every pixel, or some (bit `x` of row `y` is
/// `words[y · 4 + x / 64] >> (x % 64)`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RegionTile {
    Full,
    Bits(Box<[u64; WORDS]>),
}

impl RegionTile {
    pub(crate) fn contains(&self, x: u32, y: u32) -> bool {
        match self {
            RegionTile::Full => true,
            RegionTile::Bits(w) => {
                let i = (y * TILE_SIZE + x) as usize;
                w[i / 64] >> (i % 64) & 1 == 1
            }
        }
    }

    fn count(&self) -> u64 {
        match self {
            RegionTile::Full => N as u64,
            RegionTile::Bits(w) => w.iter().map(|v| u64::from(v.count_ones())).sum(),
        }
    }
}

/// The pixels a fill changes and their coverage before the selection: 1
/// inside the region, the anti-aliased coverage on its edge, 0 elsewhere.
/// Sparse: only tiles the region or its edge reach are stored.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FillRegion {
    tiles: BTreeMap<TileCoord, RegionTile>,
    /// Tile-local pixel index and coverage in `(0, 1)`, ascending.
    edge: BTreeMap<TileCoord, Vec<(u16, f32)>>,
    stats: FillStats,
}

impl FillRegion {
    pub(crate) fn new(
        tiles: BTreeMap<TileCoord, RegionTile>,
        edge: BTreeMap<TileCoord, Vec<(u16, f32)>>,
        mut stats: FillStats,
    ) -> Self {
        stats.pixels = tiles.values().map(RegionTile::count).sum();
        stats.edge_pixels = edge.values().map(|e| e.len() as u64).sum();
        Self { tiles, edge, stats }
    }

    /// Every pixel of `rect` (a selection fill's domain before the
    /// selection's coverage).
    pub(crate) fn rect(rect: IRect) -> Self {
        let mut tiles = BTreeMap::new();
        for c in rect.tiles() {
            let r = c.rect();
            let inside = r.intersect(&rect);
            if inside == r {
                tiles.insert(c, RegionTile::Full);
                continue;
            }
            let mut w = Box::new([0u64; WORDS]);
            let (ox, oy) = c.origin();
            for y in inside.y0..inside.y1 {
                for x in inside.x0..inside.x1 {
                    let i = ((y - oy) as u32 * TILE_SIZE + (x - ox) as u32) as usize;
                    w[i / 64] |= 1 << (i % 64);
                }
            }
            tiles.insert(c, RegionTile::Bits(w));
        }
        Self::new(tiles, BTreeMap::new(), FillStats::default())
    }

    /// Whether nothing is filled.
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty() && self.edge.is_empty()
    }

    /// Whether document pixel `(x, y)` is in the region (coverage 1).
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let c = TileCoord::containing(x, y);
        let (ox, oy) = c.origin();
        self.tiles
            .get(&c)
            .is_some_and(|t| t.contains((x - ox) as u32, (y - oy) as u32))
    }

    /// Coverage of document pixel `(x, y)` before the selection: 1 in the
    /// region, the anti-aliased edge's value next to it, 0 elsewhere.
    pub fn coverage(&self, x: i32, y: i32) -> f32 {
        if self.contains(x, y) {
            return 1.0;
        }
        let c = TileCoord::containing(x, y);
        let (ox, oy) = c.origin();
        let i = ((y - oy) as u32 * TILE_SIZE + (x - ox) as u32) as u16;
        self.edge
            .get(&c)
            .and_then(|e| e.binary_search_by_key(&i, |p| p.0).ok().map(|k| e[k].1))
            .unwrap_or(0.0)
    }

    /// Writes tile `c`'s coverage (row-major, 256 × 256) into `out`;
    /// returns whether any value is above 0.
    pub(crate) fn tile_coverage(&self, c: TileCoord, out: &mut [f32]) -> bool {
        out.fill(0.0);
        let mut any = false;
        match self.tiles.get(&c) {
            Some(RegionTile::Full) => {
                out.fill(1.0);
                any = true;
            }
            Some(RegionTile::Bits(w)) => {
                for (k, word) in w.iter().enumerate() {
                    let mut v = *word;
                    while v != 0 {
                        out[k * 64 + v.trailing_zeros() as usize] = 1.0;
                        v &= v - 1;
                    }
                }
                any = true;
            }
            None => {}
        }
        if let Some(e) = self.edge.get(&c) {
            for &(i, v) in e {
                out[usize::from(i)] = v;
                any = true;
            }
        }
        any
    }

    /// Every tile the fill may change (the region's and its edge's).
    pub fn tiles(&self) -> DirtyTiles {
        let mut d = DirtyTiles::new();
        for c in self.tiles.keys().chain(self.edge.keys()) {
            d.mark(*c);
        }
        d
    }

    /// Pixels of coverage 1.
    pub fn pixel_count(&self) -> u64 {
        self.stats.pixels
    }

    /// What computing the region cost.
    pub fn stats(&self) -> FillStats {
        self.stats
    }
}

/// A reference pixel's distance from the seed, `0..=1`: the largest
/// straight channel difference; with a fully transparent pixel on either
/// side, the alpha difference alone (BK11–BK13). `seed` is the seed's
/// straight colour, `None` when it is transparent.
#[inline]
pub(crate) fn distance(seed: Option<[f32; 4]>, p: PremulRgba) -> f32 {
    let Some(s) = seed else {
        return p.a.clamp(0.0, 1.0);
    };
    if p.a <= 0.0 || p.a.is_nan() {
        return s[3];
    }
    let inv = 1.0 / p.a;
    let q = [
        (p.r * inv).clamp(0.0, 1.0),
        (p.g * inv).clamp(0.0, 1.0),
        (p.b * inv).clamp(0.0, 1.0),
        p.a.min(1.0),
    ];
    let mut d = 0.0f32;
    for (a, b) in s.iter().zip(&q) {
        d = d.max((a - b).abs());
    }
    d
}

/// The seed's straight colour for [`distance`] (`None`: transparent).
pub(crate) fn seed_color(p: PremulRgba) -> Option<[f32; 4]> {
    if p.a <= 0.0 || p.a.is_nan() {
        return None;
    }
    let inv = 1.0 / p.a;
    Some([
        (p.r * inv).clamp(0.0, 1.0),
        (p.g * inv).clamp(0.0, 1.0),
        (p.b * inv).clamp(0.0, 1.0),
        p.a.min(1.0),
    ])
}

/// A distance on the 16-bit scale: 8-bit content lands exactly on
/// multiples of 257.
#[inline]
pub(crate) fn quantize(d: f32) -> u16 {
    (d * 65535.0).round().clamp(0.0, 65535.0) as u16
}

/// Anti-aliased coverage of an edge pixel at distance `d` (16-bit scale)
/// for threshold `t`: `(1 − d) / (1 − T)` (BK15), 0 for [`FAR`].
#[inline]
pub(crate) fn edge_coverage(d: u16, t: u16) -> f32 {
    if d <= t {
        return 1.0;
    }
    f32::from(FAR - d) / f32::from(FAR - t)
}

/// The paint rules, applied to one tile of premultiplied pixels with
/// per-pixel coverage `m` (region coverage × selection coverage): the
/// same formulas as a brush stroke's commit (ADR 0021, 0017): colour
/// `C·m` over the pixel, source-atop when the alpha is locked, erase
/// `dst·(1 − m)` (not stopped by the alpha lock, like the eraser, 4D A3).
pub(crate) fn paint_pixels(paint: FillPaint, alpha_locked: bool, px: &mut [PremulRgba], m: &[f32]) {
    match paint {
        FillPaint::Erase => {
            for (d, &m) in px.iter_mut().zip(m) {
                if m > 0.0 {
                    *d = d.scale(1.0 - m);
                }
            }
        }
        FillPaint::Color(c) if alpha_locked => {
            for (d, &m) in px.iter_mut().zip(m) {
                if m > 0.0 && d.a > 0.0 {
                    let s = c.premultiplied(m);
                    let keep = 1.0 - s.a;
                    *d = PremulRgba::new(
                        s.r * d.a + d.r * keep,
                        s.g * d.a + d.g * keep,
                        s.b * d.a + d.b * keep,
                        d.a,
                    );
                }
            }
        }
        FillPaint::Color(c) => {
            for (d, &m) in px.iter_mut().zip(m) {
                if m > 0.0 {
                    *d = c.premultiplied(m).over(*d);
                }
            }
        }
    }
}

/// The paint rules on a mask's reveal values (`0..=1`): painting reveals
/// `v + m(1 − v)`, erasing hides `v(1 − m)`; the colour is ignored (BK25,
/// as mask strokes, ADR 0017).
pub(crate) fn paint_mask(paint: FillPaint, values: &mut [f32], m: &[f32]) {
    for (v, &m) in values.iter_mut().zip(m) {
        if m > 0.0 {
            *v = match paint {
                FillPaint::Erase => *v * (1.0 - m),
                FillPaint::Color(_) => m + *v * (1.0 - m),
            };
        }
    }
}
