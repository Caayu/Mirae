//! Transforms (phase 5D, ADR 0025): scaling, rotating, flipping and moving
//! a layer's content (or the selected part of it) by an explicit affine
//! map, resampled as the reference application does.
//!
//! The rules come from the reference, measured before they were written
//! (docs/parity/5D-transform.md, T16–T24):
//!
//! - **Bicubic** (the reference's default): Keys' cubic with `a = −1` on
//!   premultiplied colour; outside the transformed box nothing (the source
//!   is transparent there); the sample position is clamped to the centres
//!   of the box's edge pixels; the result is un-premultiplied by the raw
//!   (unclamped) alpha and alpha and colour are clamped to `0..=1`.
//!   While the scale is at most ½ on both axes the source is first halved
//!   by 2 × 2 averages (a pyramid), as often as needed.
//! - **Nearest** neighbour: the source pixel whose centre is nearest, ties
//!   to the lower index, no pyramid.
//! - **Geometry**: when the result is axis-aligned (rotation a multiple of
//!   90°), the transformed box's edges are rounded to whole pixels (half
//!   up) and the content is stretched to fill the rounded box, so its
//!   edges are hard and a pure translation moves by whole pixels.
//!   Otherwise the box's sides are anti-aliased by distance (bicubic):
//!   alpha × `clamp(0.5 + d)` per side, `d` the signed distance from the
//!   pixel centre to the side; nearest neighbour keeps hard edges.
//!
//! A [`TransformSource`] holds what one transform session moves, taken
//! from the document when the session begins ([`Document::transform_source`](crate::Document::transform_source)):
//! the box (the selection's bounds, or the layer's content bounds), the
//! box's pixels at the document's depth, the selection's coverage and the
//! layer mask. Previews and the commit ([`Document::apply_transform`](crate::Document::apply_transform))
//! call the same per-tile function, so the preview equals the commit
//! quantized once (invariant TR4).

use std::cell::RefCell;

use tela_canvas::{IRect, TILE_SIZE, TileCoord};
use tela_color::{PixelDepth, PremulRgba};

use crate::LayerId;
use crate::document::Revision;

/// Smallest and largest scale factor a transform may have (absolute
/// value, per axis).
pub const MIN_SCALE: f64 = 0.001;
/// See [`MIN_SCALE`].
pub const MAX_SCALE: f64 = 100.0;

/// How a transform maps the source to the destination, as the user edits
/// it: `x' = pivot + translate + R(rotation) · S(scale) · (x − pivot)`,
/// in document pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformParams {
    /// Document pixels.
    pub translate: [f64; 2],
    /// Per axis; a negative factor flips that axis (the reference's flip
    /// is a width of −100 %, T15).
    pub scale: [f64; 2],
    /// Radians; positive turns clockwise on screen (y points down), like
    /// the reference's angle (T21).
    pub rotation: f64,
    /// The point that scaling and rotation keep in place, in document
    /// pixels (the reference's "reference point", centre by default, T9).
    pub pivot: [f64; 2],
}

impl TransformParams {
    /// No change, about `pivot`.
    pub fn identity(pivot: [f64; 2]) -> Self {
        Self {
            translate: [0.0; 2],
            scale: [1.0; 2],
            rotation: 0.0,
            pivot,
        }
    }

    /// Whether every value is a finite number and each scale factor is
    /// within [`MIN_SCALE`]..=[`MAX_SCALE`] in absolute value.
    pub fn is_valid(&self) -> bool {
        let finite = self
            .translate
            .iter()
            .chain(&self.scale)
            .chain(&self.pivot)
            .chain(std::iter::once(&self.rotation))
            .all(|v| v.is_finite() && v.abs() < 1e9);
        finite
            && self
                .scale
                .iter()
                .all(|s| (MIN_SCALE..=MAX_SCALE).contains(&s.abs()))
    }

    /// The map as a matrix.
    pub fn matrix(&self) -> Affine {
        let (s, c) = self.rotation.sin_cos();
        let [sx, sy] = self.scale;
        let [px, py] = self.pivot;
        let (a, b, cc, d) = (c * sx, s * sx, -s * sy, c * sy);
        Affine {
            a,
            b,
            c: cc,
            d,
            e: px + self.translate[0] - (a * px + cc * py),
            f: py + self.translate[1] - (b * px + d * py),
        }
    }

    /// Whether the result is axis-aligned: the rotation is a multiple of
    /// 90°.
    pub fn is_axis_aligned(&self) -> bool {
        let q = self.rotation / std::f64::consts::FRAC_PI_2;
        (q - q.round()).abs() < 1e-9
    }
}

/// An affine map `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn apply(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        [
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        ]
    }

    /// `self` after `first`: `x ↦ self(first(x))`.
    pub fn after(&self, first: &Affine) -> Affine {
        Affine {
            a: self.a * first.a + self.c * first.b,
            b: self.b * first.a + self.d * first.b,
            c: self.a * first.c + self.c * first.d,
            d: self.b * first.c + self.d * first.d,
            e: self.a * first.e + self.c * first.f + self.e,
            f: self.b * first.e + self.d * first.f + self.f,
        }
    }

    /// The inverse map, if the matrix is invertible.
    pub fn inverse(&self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Affine {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }
}

/// How a transform resamples pixels (the reference's "interpolation
/// method"; its bilinear and "average colours" modes are not implemented,
/// D-5D-3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interpolation {
    /// The reference's default ("hard-edged bicubic", T18–T21).
    #[default]
    Bicubic,
    /// Nearest neighbour, for pixel art (T22).
    Nearest,
}

/// A transform as committed: the map and how pixels are resampled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub params: TransformParams,
    pub interpolation: Interpolation,
}

/// The effective map of a transform of the box `source` (after rounding
/// an axis-aligned result to whole pixels) and what follows from it.
#[derive(Clone, Debug)]
pub(crate) struct Geometry {
    pub inverse: Affine,
    /// Hard edges at whole pixels (axis-aligned) or anti-aliased sides.
    pub axis_aligned: bool,
    /// The source box's corners in destination pixels, in order.
    pub corners: [[f64; 2]; 4],
    /// Inward unit normals and a point of each side, for the distance
    /// anti-aliasing of rotated boxes.
    sides: [([f64; 2], [f64; 2]); 4],
    /// Destination pixels that can receive content.
    pub dest: IRect,
    /// Pyramid levels (halvings) the bicubic samples from.
    pub level: u32,
    /// The effective scale is ±1 on both axes and the map axis-aligned:
    /// pixels are copied, never resampled.
    pub copy: bool,
}

impl Geometry {
    /// `None` when the map is not usable (invalid parameters, an empty
    /// box).
    pub(crate) fn new(source: IRect, params: &TransformParams) -> Option<Geometry> {
        if source.is_empty() || !params.is_valid() {
            return None;
        }
        let raw = params.matrix();
        let (x0, y0, x1, y1) = (
            f64::from(source.x0),
            f64::from(source.y0),
            f64::from(source.x1),
            f64::from(source.y1),
        );
        let box_corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        let axis_aligned = params.is_axis_aligned();
        let forward = if axis_aligned {
            // Round the transformed box's edges to whole pixels and
            // stretch the content to fill it (T17).
            let c = box_corners.map(|p| raw.apply(p));
            let lo = |i: usize| c.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min);
            let hi = |i: usize| c.iter().map(|p| p[i]).fold(f64::NEG_INFINITY, f64::max);
            let mut fit = Affine::IDENTITY;
            for (axis, (l, h)) in [(lo(0), hi(0)), (lo(1), hi(1))].into_iter().enumerate() {
                let rl = (l + 0.5).floor();
                let rh = (h + 0.5).floor().max(rl + 1.0);
                let k = (rh - rl) / (h - l);
                if axis == 0 {
                    fit.a = k;
                    fit.e = rl - l * k;
                } else {
                    fit.d = k;
                    fit.f = rl - l * k;
                }
            }
            // Snap the matrix's zero terms (a rotation by a multiple of
            // 90° leaves sin/cos rounding residue).
            let mut m = fit.after(&raw);
            for v in [&mut m.a, &mut m.b, &mut m.c, &mut m.d] {
                if v.abs() < 1e-12 {
                    *v = 0.0;
                }
            }
            m
        } else {
            raw
        };
        let inverse = forward.inverse()?;
        let corners = box_corners.map(|p| forward.apply(p));
        let centre = [
            corners.iter().map(|p| p[0]).sum::<f64>() / 4.0,
            corners.iter().map(|p| p[1]).sum::<f64>() / 4.0,
        ];
        let sides = std::array::from_fn(|k| {
            let (p, q) = (corners[k], corners[(k + 1) % 4]);
            let (ex, ey) = (q[0] - p[0], q[1] - p[1]);
            let len = (ex * ex + ey * ey).sqrt();
            let mut n = [-ey / len, ex / len];
            if n[0] * (centre[0] - p[0]) + n[1] * (centre[1] - p[1]) < 0.0 {
                n = [-n[0], -n[1]];
            }
            (n, p)
        });
        let (mut lx, mut ly, mut hx, mut hy) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for p in corners {
            (lx, ly, hx, hy) = (lx.min(p[0]), ly.min(p[1]), hx.max(p[0]), hy.max(p[1]));
        }
        // Results beyond the tiles a layer can address are not usable.
        let reach = f64::from(crate::MAX_TILE_COORD) * f64::from(TILE_SIZE) - 2.0;
        if [lx, ly, hx, hy].iter().any(|v| v.abs() > reach) {
            return None;
        }
        // One pixel of margin for the anti-aliased sides.
        let dest = IRect::new(
            (lx.floor() - 1.0) as i32,
            (ly.floor() - 1.0) as i32,
            (hx.ceil() + 1.0) as i32,
            (hy.ceil() + 1.0) as i32,
        );
        let sx = forward.a.hypot(forward.b);
        let sy = forward.c.hypot(forward.d);
        let mut level = 0;
        let (mut s, mut side) = (sx.max(sy), source.width().max(source.height()));
        while s <= 0.5 && side > 1 {
            s *= 2.0;
            side = (side + 1) / 2;
            level += 1;
        }
        let copy = axis_aligned && (sx - 1.0).abs() < 1e-12 && (sy - 1.0).abs() < 1e-12;
        Some(Geometry {
            inverse,
            axis_aligned,
            corners,
            sides,
            dest,
            level,
            copy,
        })
    }

    /// How much of destination pixel centre `p` the box covers: 0 or 1
    /// with hard edges, the product of `clamp(0.5 + d)` over the sides
    /// otherwise (T21).
    fn edge_coverage(&self, p: [f64; 2], hard: bool, source: IRect) -> f32 {
        if hard || self.axis_aligned {
            let [u, v] = self.inverse.apply(p);
            let inside = u >= f64::from(source.x0)
                && u < f64::from(source.x1)
                && v >= f64::from(source.y0)
                && v < f64::from(source.y1);
            return if inside { 1.0 } else { 0.0 };
        }
        let mut c = 1.0;
        for (n, q) in &self.sides {
            let d = n[0] * (p[0] - q[0]) + n[1] * (p[1] - q[1]);
            c *= (0.5 + d).clamp(0.0, 1.0);
            if c == 0.0 {
                break;
            }
        }
        c as f32
    }
}

/// One level of the source pyramid: dense over the (reduced) box.
#[derive(Clone, Debug)]
struct Level {
    w: usize,
    h: usize,
    /// Premultiplied colour, times the selection's coverage.
    px: Vec<[f32; 4]>,
    /// The mask's hidden amount (`0..=1`), when the mask moves.
    mask: Option<Vec<f32>>,
    /// The selection's coverage (`0..=1`), when there is a selection.
    cov: Option<Vec<f32>>,
}

impl Level {
    /// The next level: 2 × 2 averages anchored at the box's corner;
    /// pixels beyond the box count as transparent (T20).
    fn halved(&self) -> Level {
        let (w, h) = (self.w.div_ceil(2), self.h.div_ceil(2));
        let get = |i: usize, j: usize| (i < self.w && j < self.h).then(|| j * self.w + i);
        let mut px = vec![[0.0f32; 4]; w * h];
        for j in 0..h {
            for i in 0..w {
                let mut acc = [0.0f32; 4];
                for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    if let Some(k) = get(2 * i + di, 2 * j + dj) {
                        for (a, s) in acc.iter_mut().zip(self.px[k]) {
                            *a += s;
                        }
                    }
                }
                px[j * w + i] = acc.map(|a| a * 0.25);
            }
        }
        let half = |src: &Vec<f32>| {
            let mut out = vec![0.0f32; w * h];
            for j in 0..h {
                for i in 0..w {
                    let mut acc = 0.0;
                    for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        if let Some(k) = get(2 * i + di, 2 * j + dj) {
                            acc += src[k];
                        }
                    }
                    out[j * w + i] = acc * 0.25;
                }
            }
            out
        };
        Level {
            w,
            h,
            px,
            mask: self.mask.as_ref().map(half),
            cov: self.cov.as_ref().map(half),
        }
    }
}

/// Keys' cubic convolution kernel with `a = −1` (T18).
fn keys(t: f32) -> f32 {
    let t = t.abs();
    if t < 1.0 {
        (t - 2.0) * t * t + 1.0
    } else if t < 2.0 {
        ((-t + 5.0) * t - 8.0) * t + 4.0
    } else {
        0.0
    }
}

/// Weights and first index of the four taps around level coordinate `x`
/// (pixel centres at `i + 0.5`).
fn taps(x: f32) -> (isize, [f32; 4]) {
    let x = x - 0.5;
    let i0 = x.floor();
    let t = x - i0;
    (
        i0 as isize - 1,
        [keys(t + 1.0), keys(t), keys(1.0 - t), keys(2.0 - t)],
    )
}

/// What one destination pixel receives, before it meets what stays.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Sample {
    /// Premultiplied colour, `0..=1`, colour ≤ alpha.
    pub px: [f32; 4],
    /// The mask's hidden amount.
    pub mask: f32,
    /// The selection's coverage.
    pub cov: f32,
}

/// What a transform session moves (see the module documentation). Built by
/// [`Document::transform_source`](crate::Document::transform_source); valid for the document state it
/// was taken from (the session refuses to apply once the document
/// changed).
#[derive(Debug)]
pub struct TransformSource {
    pub(crate) layer: LayerId,
    pub(crate) depth: PixelDepth,
    pub(crate) revision: Revision,
    pub(crate) selection_revision: u64,
    /// The box: the selection's bounds or the layer's content bounds.
    pub(crate) bounds: IRect,
    /// Whether a selection limits what moves (the rest stays, T29–T31).
    pub(crate) selected: bool,
    /// Whether the layer's mask moves with its pixels (no selection,
    /// T33).
    pub(crate) moves_mask: bool,
    /// Level 0 and the reduced levels built so far.
    levels: RefCell<Vec<Level>>,
}

impl TransformSource {
    /// `px`, `cov` and `mask` are dense over `bounds`, row by row:
    /// premultiplied colour already multiplied by the coverage, the
    /// selection's coverage and the mask's hidden amount, all `0..=1`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        layer: LayerId,
        depth: PixelDepth,
        (revision, selection_revision): (Revision, u64),
        bounds: IRect,
        px: Vec<[f32; 4]>,
        cov: Option<Vec<f32>>,
        mask: Option<Vec<f32>>,
    ) -> Self {
        let (w, h) = (bounds.width() as usize, bounds.height() as usize);
        debug_assert_eq!(px.len(), w * h);
        Self {
            layer,
            depth,
            revision,
            selection_revision,
            bounds,
            selected: cov.is_some(),
            moves_mask: mask.is_some(),
            levels: RefCell::new(vec![Level {
                w,
                h,
                px,
                mask,
                cov,
            }]),
        }
    }

    /// The layer the transform edits.
    pub fn layer(&self) -> LayerId {
        self.layer
    }

    /// The box the transform's handles surround, in document pixels.
    pub fn bounds(&self) -> IRect {
        self.bounds
    }

    /// The default pivot: the box's centre (the reference's "Centro").
    pub fn centre(&self) -> [f64; 2] {
        let b = self.bounds;
        [
            (f64::from(b.x0) + f64::from(b.x1)) / 2.0,
            (f64::from(b.y0) + f64::from(b.y1)) / 2.0,
        ]
    }

    /// Whether only the selected part moves (the rest stays).
    pub fn is_selection(&self) -> bool {
        self.selected
    }

    /// Whether the layer's mask moves with its pixels.
    pub fn moves_mask(&self) -> bool {
        self.moves_mask
    }

    /// The box's corners after `params` (rounded like the result when
    /// axis-aligned), in document pixels, in order; `None` for invalid
    /// parameters.
    pub fn corners(&self, params: &TransformParams) -> Option<[[f64; 2]; 4]> {
        Geometry::new(self.bounds, params).map(|g| g.corners)
    }

    /// Every tile whose content the transform changes: the box's tiles
    /// (what leaves) and the tiles the result reaches.
    pub fn affected_tiles(&self, params: &TransformParams) -> tela_canvas::DirtyTiles {
        let mut tiles = tela_canvas::DirtyTiles::new();
        tiles.mark_rect(self.bounds);
        if let Some(g) = Geometry::new(self.bounds, params) {
            tiles.mark_rect(g.dest);
        }
        tiles
    }

    /// Bytes held: the dense snapshot and the pyramid levels built so far.
    pub fn bytes(&self) -> usize {
        self.levels
            .borrow()
            .iter()
            .map(|l| {
                let n = l.w * l.h;
                n * 16 + l.mask.as_ref().map_or(0, |_| n * 4) + l.cov.as_ref().map_or(0, |_| n * 4)
            })
            .sum()
    }

    /// Builds the pyramid down to `level`.
    fn ensure_level(&self, level: u32) {
        let mut levels = self.levels.borrow_mut();
        while levels.len() <= level as usize {
            let next = levels.last().expect("level 0 exists").halved();
            levels.push(next);
        }
    }

    /// The transformed content for each pixel of tile `coord` (`f`
    /// receives the tile-local index and the sample), for pixels the
    /// transformed box reaches. `None` for invalid parameters.
    pub(crate) fn render_tile(
        &self,
        t: &Transform,
        coord: TileCoord,
        mut f: impl FnMut(usize, Sample),
    ) -> Option<()> {
        let g = Geometry::new(self.bounds, &t.params)?;
        let rect = coord.rect().intersect(&g.dest);
        if rect.is_empty() {
            return Some(());
        }
        let nearest = t.interpolation == Interpolation::Nearest || g.copy;
        let level = if nearest { 0 } else { g.level };
        self.ensure_level(level);
        let levels = self.levels.borrow();
        let base = &levels[0];
        let lv = &levels[level as usize];
        let k = (1u32 << level) as f32;
        let (ox, oy) = coord.origin();
        let (bx, by) = (f64::from(self.bounds.x0), f64::from(self.bounds.y0));
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let p = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                let edge = g.edge_coverage(p, nearest, self.bounds);
                if edge <= 0.0 {
                    continue;
                }
                let [u, v] = g.inverse.apply(p);
                // Box-local source position.
                let (lu, lv_) = (u - bx, v - by);
                let s = if nearest {
                    sample_nearest(base, lu, lv_)
                } else {
                    sample_cubic(lv, (lu as f32) / k, (lv_ as f32) / k)
                };
                let i = ((y - oy) as u32 * TILE_SIZE + (x - ox) as u32) as usize;
                f(
                    i,
                    Sample {
                        px: s.px.map(|c| c * edge),
                        mask: s.mask * edge,
                        cov: s.cov * edge,
                    },
                );
            }
        }
        Some(())
    }

    /// The new content of one tile of the layer's pixels: what stays
    /// (`stored · (1 − s)` with a selection, nothing without one) with the
    /// transformed content composed over it. `stored` is the tile's
    /// current premultiplied pixels and is replaced. The preview shows
    /// exactly this; the commit stores it quantized once. Returns `false`
    /// (leaving `stored` untouched) for invalid parameters.
    pub fn transform_pixels_tile(
        &self,
        t: &Transform,
        coord: TileCoord,
        coverage: &tela_canvas::TileCoverage,
        stored: &mut [PremulRgba],
    ) -> bool {
        if Geometry::new(self.bounds, &t.params).is_none() {
            return false;
        }
        // What stays.
        if self.selected {
            let mut row = [0u8; TILE_SIZE as usize];
            for y in 0..TILE_SIZE {
                coverage.row(y, &mut row);
                let line = &mut stored[(y * TILE_SIZE) as usize..((y + 1) * TILE_SIZE) as usize];
                for (p, &c) in line.iter_mut().zip(&row) {
                    if c != 0 {
                        *p = p.scale(1.0 - f32::from(c) / 255.0);
                    }
                }
            }
        } else {
            let r = coord.rect().intersect(&self.bounds);
            let (ox, oy) = coord.origin();
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    stored[((y - oy) as u32 * TILE_SIZE + (x - ox) as u32) as usize] =
                        PremulRgba::TRANSPARENT;
                }
            }
        }
        self.render_tile(t, coord, |i, s| {
            let [r, g, b, a] = s.px;
            stored[i] = PremulRgba::new(r, g, b, a).over(stored[i]);
        });
        true
    }

    /// The new mask of one tile as reveal values (1 shows the layer),
    /// when the mask moves: what was in the box goes, the transformed
    /// mask arrives. `reveal` holds the current values and is replaced.
    pub fn transform_mask_tile(&self, t: &Transform, coord: TileCoord, reveal: &mut [f32]) -> bool {
        if !self.moves_mask || Geometry::new(self.bounds, &t.params).is_none() {
            return false;
        }
        let r = coord.rect().intersect(&self.bounds);
        let (ox, oy) = coord.origin();
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                reveal[((y - oy) as u32 * TILE_SIZE + (x - ox) as u32) as usize] = 1.0;
            }
        }
        self.render_tile(t, coord, |i, s| {
            reveal[i] = (1.0 - s.mask).clamp(0.0, 1.0);
        });
        true
    }

    /// The selection's coverage after the transform for one tile, or
    /// `None` without a selection (T32). Zero outside the canvas.
    pub(crate) fn transform_coverage_tile(
        &self,
        t: &Transform,
        coord: TileCoord,
        out: &mut [f32],
    ) -> bool {
        if !self.selected {
            return false;
        }
        out.fill(0.0);
        self.render_tile(t, coord, |i, s| out[i] = s.cov.clamp(0.0, 1.0))
            .is_some()
    }

    /// Bounds of the destination pixels, `None` for invalid parameters.
    pub fn dest_bounds(&self, params: &TransformParams) -> Option<IRect> {
        Geometry::new(self.bounds, params).map(|g| g.dest)
    }
}

/// Nearest neighbour at box-local position `(u, v)`: the pixel `⌈u⌉ − 1`
/// (ties to the lower index, T22); nothing outside the box.
fn sample_nearest(l: &Level, u: f64, v: f64) -> Sample {
    // A position on a pixel boundary (a tie) can arrive a rounding error
    // above it; it still belongs to the lower pixel.
    const TIE: f64 = 1e-6;
    let (i, j) = ((u - TIE).ceil() - 1.0, (v - TIE).ceil() - 1.0);
    if i < 0.0 || j < 0.0 || i >= l.w as f64 || j >= l.h as f64 {
        return Sample::default();
    }
    let k = j as usize * l.w + i as usize;
    Sample {
        px: l.px[k],
        mask: l.mask.as_ref().map_or(0.0, |m| m[k]),
        cov: l.cov.as_ref().map_or(0.0, |c| c[k]),
    }
}

/// The measured bicubic at level-local position `(u, v)` (T18): the
/// position clamped to the edge pixels' centres, transparent beyond the
/// box, un-premultiplied by the raw alpha, clamped.
fn sample_cubic(l: &Level, u: f32, v: f32) -> Sample {
    let u = u.clamp(0.5, l.w as f32 - 0.5);
    let v = v.clamp(0.5, l.h as f32 - 0.5);
    let (i0, wx) = taps(u);
    let (j0, wy) = taps(v);
    let mut px = [0.0f32; 4];
    let (mut mask, mut cov) = (0.0f32, 0.0f32);
    for (dj, &wj) in wy.iter().enumerate() {
        let j = j0 + dj as isize;
        if j < 0 || j >= l.h as isize || wj == 0.0 {
            continue;
        }
        let row = j as usize * l.w;
        let mut rpx = [0.0f32; 4];
        let (mut rm, mut rc) = (0.0f32, 0.0f32);
        for (di, &wi) in wx.iter().enumerate() {
            let i = i0 + di as isize;
            if i < 0 || i >= l.w as isize || wi == 0.0 {
                continue;
            }
            let k = row + i as usize;
            let p = l.px[k];
            for (a, s) in rpx.iter_mut().zip(p) {
                *a += wi * s;
            }
            if let Some(m) = &l.mask {
                rm += wi * m[k];
            }
            if let Some(c) = &l.cov {
                rc += wi * c[k];
            }
        }
        for (a, s) in px.iter_mut().zip(rpx) {
            *a += wj * s;
        }
        mask += wj * rm;
        cov += wj * rc;
    }
    let raw = px[3];
    let out = if raw > 0.0 {
        let a = raw.min(1.0);
        let c = |v: f32| (v / raw).clamp(0.0, 1.0) * a;
        [c(px[0]), c(px[1]), c(px[2]), a]
    } else {
        [0.0; 4]
    };
    Sample {
        px: out,
        mask: mask.clamp(0.0, 1.0),
        cov: cov.clamp(0.0, 1.0),
    }
}

#[cfg(test)]
mod parity;
#[cfg(test)]
mod tests;
