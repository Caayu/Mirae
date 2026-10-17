//! Gradients (phase 5F, ADR 0027): where a gradient lies on the document
//! and what it paints into each pixel.
//!
//! A [`Gradient`] is plain data captured when the drag begins: a shape
//! between two document points ([`GradientGeometry`]), what happens
//! beyond them ([`GradientExtend`]), the colours ([`ColorRamp`], from
//! `tela_color::gradient`) and whether the result is dithered. For each
//! pixel it gives a position `t` (`0` at the start, `1` at the end), the
//! colour there, and how much of the pixel it covers; the paint rules are
//! the brush commit's and the fill's (source-over, source-atop with the
//! alpha lock, erase, mask reveal/hide), applied by **one** function that
//! the preview and the commit both call, which quantizes once at the
//! document's depth with the dither threshold of the pixel (invariants
//! GR2, GR5, GR9).
//!
//! Measured rules of the reference application (docs/parity/5F-gradients.md):
//! linear gradients project on the drag (GD17); repeated gradients
//! anti-alias their wrap (GD20), reflected ones mirror smoothly (GD21),
//! "nothing beyond the ends" anti-aliases the region's edge by area
//! (GD14, GD15); the drag's geometry does not depend on the selection
//! (GD31). Mirae samples each pixel at its centre (the reference at its
//! corner, GD16, D-5F-2) and adds angular and symmetric ("reflected")
//! shapes the reference lacks.

use tela_canvas::{IRect, TILE_SIZE, TileCoord};
use tela_color::gradient::{ColorRamp, GradientStops, Mixing};
use tela_color::{PremulRgba, Rgba, StoragePixel};

#[cfg(test)]
mod parity;
#[cfg(test)]
mod tests;

/// Pixels per tile.
const N: usize = (TILE_SIZE * TILE_SIZE) as usize;
/// Below this length in document pixels a drag has no direction: nothing
/// is drawn (GD24).
pub const MIN_LENGTH: f64 = 1e-3;

/// How the position `t` of a pixel follows from the drag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GradientShape {
    /// The pixel's projection on the drag (the reference's "Linha reta").
    #[default]
    Linear,
    /// The distance from the start over the drag's length (a circle).
    Radial,
    /// Axis-aligned ellipse centred on the start with radii `|dx|`,
    /// `|dy|` (the reference's "Elipse", without its rotation step,
    /// GD19, D-5F-3).
    Elliptical,
    /// The angle around the start, clockwise on screen from the drag's
    /// direction, one turn = `0..1` (not in the reference).
    Angular,
    /// The linear position mirrored about the start: `|t|`, the same
    /// gradient on both sides (not a shape in the reference).
    Reflected,
}

/// What a gradient paints beyond its ends (`t < 0` or `t > 1`): the
/// reference's "Processamento de borda" (GD2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GradientExtend {
    /// The end colours continue ("Não repetir", the default).
    #[default]
    Clamp,
    /// The gradient starts again every length ("Repetir"); the wrap is
    /// anti-aliased (GD20).
    Repeat,
    /// It runs back and forth ("Inverter", GD21).
    Reflect,
    /// Nothing is painted beyond the ends; the edge is anti-aliased by
    /// area ("Não desenhar", GD14).
    Empty,
}

/// Where a gradient lies: a shape between two document points (pixel
/// units, `(0, 0)` the canvas's top-left corner; pixel `(x, y)` is the
/// square `[x, x + 1) × [y, y + 1)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientGeometry {
    pub shape: GradientShape,
    pub start: [f64; 2],
    pub end: [f64; 2],
}

impl GradientGeometry {
    /// Whether the drag is too short to define the gradient (an ellipse
    /// needs both radii): nothing is drawn (GD24). Also for coordinates
    /// that are not finite.
    pub fn is_degenerate(&self) -> bool {
        let [dx, dy] = self.delta();
        if !(dx.is_finite()
            && dy.is_finite()
            && self.start[0].is_finite()
            && self.start[1].is_finite())
        {
            return true;
        }
        match self.shape {
            GradientShape::Elliptical => dx.abs() < MIN_LENGTH || dy.abs() < MIN_LENGTH,
            _ => dx.hypot(dy) < MIN_LENGTH,
        }
    }

    fn delta(&self) -> [f64; 2] {
        [self.end[0] - self.start[0], self.end[1] - self.start[1]]
    }
}

/// What a gradient does to its target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GradientPaint {
    /// Paint the gradient over the pixels (recolour keeping alpha with the
    /// alpha lock); on a mask, reveal by the gradient's alpha.
    #[default]
    Draw,
    /// Remove by the gradient's alpha (the eraser end or eraser mode); on
    /// a mask, hide.
    Erase,
}

/// A gradient ready to paint: geometry, colours and options, all captured
/// when the operation began (the colours resolved from the main and sub
/// colours then).
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    geometry: GradientGeometry,
    ramp: ColorRamp,
    extend: GradientExtend,
    dither: bool,
    /// Precomputed per shape (see `Gradient::position`).
    frame: Frame,
}

/// The per-pixel position as cheap arithmetic.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    origin: [f64; 2],
    /// Linear/reflected: the drag over its squared length; radial: 1 over
    /// the radius (both components); elliptical: 1 over each radius.
    k: [f64; 2],
    /// Angular: the drag's angle.
    angle: f64,
    /// `|∇t|` where it is constant (linear, reflected, radial).
    width: f64,
}

impl Gradient {
    pub fn new(
        geometry: GradientGeometry,
        stops: &GradientStops,
        mixing: Mixing,
        extend: GradientExtend,
        dither: bool,
    ) -> Self {
        let [dx, dy] = geometry.delta();
        let len2 = dx * dx + dy * dy;
        let len = len2.sqrt();
        let frame = match geometry.shape {
            GradientShape::Linear | GradientShape::Reflected => Frame {
                origin: geometry.start,
                k: [dx / len2, dy / len2],
                angle: 0.0,
                width: 1.0 / len,
            },
            GradientShape::Radial => Frame {
                origin: geometry.start,
                k: [1.0 / len, 1.0 / len],
                angle: 0.0,
                width: 1.0 / len,
            },
            GradientShape::Elliptical => Frame {
                origin: geometry.start,
                k: [1.0 / dx.abs(), 1.0 / dy.abs()],
                angle: 0.0,
                width: 0.0,
            },
            GradientShape::Angular => Frame {
                origin: geometry.start,
                k: [0.0, 0.0],
                angle: dy.atan2(dx),
                width: 0.0,
            },
        };
        Self {
            geometry,
            ramp: ColorRamp::new(stops, mixing),
            extend,
            dither,
            frame,
        }
    }

    pub fn geometry(&self) -> GradientGeometry {
        self.geometry
    }

    pub fn extend(&self) -> GradientExtend {
        self.extend
    }

    pub fn dither(&self) -> bool {
        self.dither
    }

    pub fn ramp(&self) -> &ColorRamp {
        &self.ramp
    }

    /// Whether the gradient draws nothing at all (a degenerate drag).
    pub fn is_empty(&self) -> bool {
        self.geometry.is_degenerate()
    }

    /// The position `t` at document point `(x, y)` before the extend rule,
    /// and `|∇t|` there (how much `t` changes over one pixel, for
    /// anti-aliasing).
    pub fn position(&self, x: f64, y: f64) -> (f64, f64) {
        let f = &self.frame;
        let (px, py) = (x - f.origin[0], y - f.origin[1]);
        match self.geometry.shape {
            GradientShape::Linear => (px * f.k[0] + py * f.k[1], f.width),
            GradientShape::Reflected => ((px * f.k[0] + py * f.k[1]).abs(), f.width),
            GradientShape::Radial => (px.hypot(py) * f.k[0], f.width),
            GradientShape::Elliptical => {
                let (u, v) = (px * f.k[0], py * f.k[1]);
                let t = u.hypot(v);
                let w = if t > 1e-9 {
                    (u * f.k[0]).hypot(v * f.k[1]) / t
                } else {
                    f.k[0].max(f.k[1])
                };
                (t, w)
            }
            GradientShape::Angular => {
                let a = py.atan2(px) - f.angle;
                let t = (a / std::f64::consts::TAU).rem_euclid(1.0);
                let r = px.hypot(py).max(0.5);
                (t, 1.0 / (std::f64::consts::TAU * r))
            }
        }
    }

    /// The premultiplied paint at the centre of document pixel `(x, y)`:
    /// the colour and alpha of the ramp at the pixel's position after the
    /// extend rule, times the anti-aliased coverage of the region's edge
    /// ([`GradientExtend::Empty`]) or blended across a wrap
    /// ([`GradientExtend::Repeat`], the angular seam).
    pub fn sample(&self, x: i32, y: i32) -> PremulRgba {
        let (t, w) = self.position(f64::from(x) + 0.5, f64::from(y) + 0.5);
        if !t.is_finite() {
            return PremulRgba::TRANSPARENT;
        }
        let angular = self.geometry.shape == GradientShape::Angular;
        match (self.extend, angular) {
            (GradientExtend::Reflect, true) => self.at(1.0 - (2.0 * t - 1.0).abs()),
            (GradientExtend::Reflect, false) => {
                let r = t.rem_euclid(2.0);
                self.at(if r > 1.0 { 2.0 - r } else { r })
            }
            (GradientExtend::Repeat, _) | (_, true) => self.wrapped(t, w),
            (GradientExtend::Clamp, false) => self.at(t),
            (GradientExtend::Empty, false) => {
                // Area of the pixel's footprint `[t − w/2, t + w/2]` inside
                // `[0, 1]` (GD14: a boundary through the centre covers ½).
                let (lo, hi) = (t - w / 2.0, t + w / 2.0);
                let inside = if w > 0.0 {
                    ((hi.min(1.0) - lo.max(0.0)) / w).clamp(0.0, 1.0)
                } else if (0.0..=1.0).contains(&t) {
                    1.0
                } else {
                    0.0
                };
                if inside <= 0.0 {
                    return PremulRgba::TRANSPARENT;
                }
                self.at(t).scale(inside as f32)
            }
        }
    }

    /// The ramp at `t` (clamped), premultiplied.
    fn at(&self, t: f64) -> PremulRgba {
        premultiply(self.ramp.eval(t as f32))
    }

    /// The ramp at `t`'s fractional part; where the pixel's footprint
    /// crosses a whole number the two sides are blended by area (GD20).
    fn wrapped(&self, t: f64, w: f64) -> PremulRgba {
        let (lo, hi) = (t - w / 2.0, t + w / 2.0);
        let k = hi.floor();
        if w <= 0.0 || w >= 1.0 || k <= lo {
            return self.at(t.rem_euclid(1.0));
        }
        let left = ((k - lo) / w) as f32;
        let a = self.at(((lo + k) / 2.0).rem_euclid(1.0));
        let b = self.at(((k + hi) / 2.0).rem_euclid(1.0));
        let r = 1.0 - left;
        PremulRgba::new(
            a.r * left + b.r * r,
            a.g * left + b.g * r,
            a.b * left + b.b * r,
            a.a * left + b.a * r,
        )
    }

    /// The range of positions in tile `c` before the extend rule, for
    /// shortcuts: `None` when the position cannot be bounded cheaply
    /// (angular; elliptical uses its radial bound).
    fn tile_range(&self, c: TileCoord) -> Option<(f64, f64)> {
        let r = c.rect();
        let (x0, y0, x1, y1) = (
            f64::from(r.x0) + 0.5,
            f64::from(r.y0) + 0.5,
            f64::from(r.x1) - 0.5,
            f64::from(r.y1) - 0.5,
        );
        let f = &self.frame;
        match self.geometry.shape {
            GradientShape::Linear | GradientShape::Reflected => {
                let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
                    .map(|(x, y)| (x - f.origin[0]) * f.k[0] + (y - f.origin[1]) * f.k[1]);
                let lo = corners.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = corners.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                if self.geometry.shape == GradientShape::Reflected {
                    let far = lo.abs().max(hi.abs());
                    let near = if lo <= 0.0 && hi >= 0.0 {
                        0.0
                    } else {
                        lo.abs().min(hi.abs())
                    };
                    Some((near, far))
                } else {
                    Some((lo, hi))
                }
            }
            GradientShape::Radial | GradientShape::Elliptical => {
                // Nearest and farthest points of the pixel-centre box in the
                // scaled frame.
                let (kx, ky) = (f.k[0], f.k[1]);
                let (ax, bx) = ((x0 - f.origin[0]) * kx, (x1 - f.origin[0]) * kx);
                let (ay, by) = ((y0 - f.origin[1]) * ky, (y1 - f.origin[1]) * ky);
                let near = |a: f64, b: f64| {
                    if a <= 0.0 && b >= 0.0 {
                        0.0
                    } else {
                        a.abs().min(b.abs())
                    }
                };
                let far = |a: f64, b: f64| a.abs().max(b.abs());
                Some((
                    near(ax, bx).hypot(near(ay, by)),
                    far(ax, bx).hypot(far(ay, by)),
                ))
            }
            GradientShape::Angular => None,
        }
    }

    /// Whether tile `c` certainly gets no paint (the region beyond the
    /// ends with [`GradientExtend::Empty`]).
    pub fn misses_tile(&self, c: TileCoord) -> bool {
        if self.is_empty() {
            return true;
        }
        if self.extend != GradientExtend::Empty {
            return false;
        }
        // Half a pixel's footprint of margin for the anti-aliased edge.
        let margin = self.frame.width.max(0.0) + 1e-9;
        match self.tile_range(c) {
            Some((lo, hi)) => hi < -margin || lo > 1.0 + margin,
            None => false,
        }
    }

    /// The single paint every pixel of tile `c` gets, when the whole tile
    /// lies before the first stop or after the last one with
    /// [`GradientExtend::Clamp`] (most tiles of a short linear drag).
    fn uniform_tile(&self, c: TileCoord) -> Option<PremulRgba> {
        if self.extend != GradientExtend::Clamp {
            return None;
        }
        let (lo, hi) = self.tile_range(c)?;
        let (first, last) = self.ramp.bounds();
        if hi <= f64::from(first) {
            Some(self.at(f64::from(first)))
        } else if lo >= f64::from(last) {
            Some(self.at(f64::from(last)))
        } else {
            None
        }
    }

    /// Writes the paint of every pixel of tile `c` (see
    /// [`Gradient::sample`]) into `out`.
    pub fn render_tile(&self, c: TileCoord, out: &mut [PremulRgba]) {
        if self.misses_tile(c) {
            out.fill(PremulRgba::TRANSPARENT);
            return;
        }
        if let Some(p) = self.uniform_tile(c) {
            out.fill(p);
            return;
        }
        let (ox, oy) = c.origin();
        for (i, o) in out.iter_mut().enumerate().take(N) {
            let (x, y) = (
                (i % TILE_SIZE as usize) as i32,
                (i / TILE_SIZE as usize) as i32,
            );
            *o = self.sample(ox + x, oy + y);
        }
    }
}

fn premultiply(c: Rgba) -> PremulRgba {
    PremulRgba::new(c.r * c.a, c.g * c.a, c.b * c.a, c.a)
}

/// How a gradient's paint `s` (premultiplied, edge coverage included)
/// meets one pixel `d` whose selection coverage is `sel` (`0..=1`): the
/// brush commit's rules (ADR 0017, 0021) and the fill's (ADR 0026).
/// Colour over, source-atop with the alpha lock (alpha kept), erase
/// `d·(1 − α)` not stopped by the lock. `None` where nothing changes
/// (no paint or no coverage): the stored value stays as it is.
#[inline]
pub(crate) fn blend_pixel(
    paint: GradientPaint,
    locked: bool,
    d: PremulRgba,
    s: PremulRgba,
    sel: f32,
) -> Option<PremulRgba> {
    let m = s.a * sel;
    if m <= 0.0 {
        return None;
    }
    Some(match paint {
        GradientPaint::Erase => d.scale(1.0 - m),
        GradientPaint::Draw if locked => {
            let keep = 1.0 - m;
            let a = d.a;
            PremulRgba::new(
                (s.r * sel * a + d.r * keep).min(a),
                (s.g * sel * a + d.g * keep).min(a),
                (s.b * sel * a + d.b * keep).min(a),
                a,
            )
        }
        GradientPaint::Draw => {
            let o = s.scale(sel).over(d);
            PremulRgba::new(o.r.min(o.a), o.g.min(o.a), o.b.min(o.a), o.a.min(1.0))
        }
    })
}

/// A mask's reveal value `v` (`0..=1`) after a gradient whose alpha is
/// `m` (selection included): reveal `v + m(1 − v)` or hide `v(1 − m)`;
/// the colour is ignored (GD33, as mask strokes and fills).
#[inline]
pub(crate) fn blend_mask(paint: GradientPaint, v: f32, m: f32) -> Option<f32> {
    if m <= 0.0 {
        return None;
    }
    Some(match paint {
        GradientPaint::Draw => v + m * (1.0 - v),
        GradientPaint::Erase => v * (1.0 - m),
    })
}

/// The stored value of one changed pixel: quantized once, at the
/// document's depth, with document pixel `(x, y)`'s dither threshold when
/// the gradient dithers (round to nearest otherwise).
#[inline]
pub(crate) fn quantize<P: StoragePixel>(p: PremulRgba, threshold: f32) -> P {
    P::from_premul_dithered(p, threshold)
}

/// The canvas pixels of tile `c`, tile-local (`x0, y0, x1, y1`), or
/// `None` when the tile is off the canvas.
pub(crate) fn canvas_clip(canvas: IRect, c: TileCoord) -> Option<[u32; 4]> {
    let r = c.rect();
    let i = r.intersect(&canvas);
    if i.is_empty() {
        return None;
    }
    Some([
        (i.x0 - r.x0) as u32,
        (i.y0 - r.y0) as u32,
        (i.x1 - r.x0) as u32,
        (i.y1 - r.y0) as u32,
    ])
}
