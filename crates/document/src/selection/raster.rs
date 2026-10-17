//! Rasterizing selection shapes into 8-bit coverage.
//!
//! One filler for every shape: a closed polygon with the **even-odd** rule
//! (reference S11; rectangles and ellipses are polygons too). Anti-aliased
//! coverage is computed on 16 sub-scanlines per pixel row, each with the
//! exact horizontal coverage of its spans; without anti-aliasing a pixel
//! is in when its centre is. Deterministic: fixed sub-scanlines, f64
//! geometry, a fixed accumulation order. The ellipse is flattened finely
//! enough that its polygon stays within 0.01 px of the curve, and scaled
//! so that its area is the ellipse's.

use tela_canvas::{IRect, TILE_SIZE, Tile, TileCoord, TiledSurface};

use super::SelectionShape;

/// Sub-scanlines per pixel row when anti-aliasing.
const SUBROWS: u32 = 16;
/// Largest distance between an ellipse and its polygon, in pixels.
const ELLIPSE_TOLERANCE: f64 = 0.01;
/// Bounds on an ellipse polygon's vertex count.
const ELLIPSE_SEGMENTS: (f64, f64) = (16.0, 16_384.0);

/// Rounds half down: a rectangle corner at `x.5` goes to `x` (measured:
/// a drag ending at 150.5 selected up to 150, at 120.75 up to 121; S8).
fn snap(v: f32) -> f64 {
    (f64::from(v) - 0.5).ceil()
}

/// The polygon a shape fills, in document pixels. Non-finite input gives
/// an empty polygon.
fn polygon(shape: &SelectionShape) -> Vec<[f64; 2]> {
    let finite = |p: &[f32; 2]| p[0].is_finite() && p[1].is_finite();
    match shape {
        SelectionShape::Rect(a, b) => {
            if !finite(a) || !finite(b) {
                return Vec::new();
            }
            let (x0, x1) = (snap(a[0].min(b[0])), snap(a[0].max(b[0])));
            let (y0, y1) = (snap(a[1].min(b[1])), snap(a[1].max(b[1])));
            vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
        }
        SelectionShape::Polygon(points) => points
            .iter()
            .filter(|p| finite(p))
            .map(|p| [f64::from(p[0]), f64::from(p[1])])
            .collect(),
        SelectionShape::Ellipse {
            center,
            radii,
            rotation,
        } => {
            if !finite(center) || !finite(radii) || !rotation.is_finite() {
                return Vec::new();
            }
            let (rx, ry) = (f64::from(radii[0]).abs(), f64::from(radii[1]).abs());
            let r = rx.max(ry);
            if r <= 0.0 {
                return Vec::new();
            }
            // Chord sagitta r(1 − cos(π/n)) ≈ r·π²/(2n²) ≤ tolerance.
            let n = (std::f64::consts::PI * (r / (2.0 * ELLIPSE_TOLERANCE)).sqrt())
                .ceil()
                .clamp(ELLIPSE_SEGMENTS.0, ELLIPSE_SEGMENTS.1) as u32;
            let (s, c) = f64::from(*rotation).sin_cos();
            let (cx, cy) = (f64::from(center[0]), f64::from(center[1]));
            // The inscribed polygon's area is (n/2)·sin(2π/n)·rx·ry; scale
            // it up to π·rx·ry.
            let step = std::f64::consts::TAU / f64::from(n);
            let k = (step / step.sin()).sqrt();
            let (rx, ry) = (rx * k, ry * k);
            (0..n)
                .map(|i| {
                    let t = step * f64::from(i);
                    let (x, y) = (rx * t.cos(), ry * t.sin());
                    [cx + c * x - s * y, cy + s * x + c * y]
                })
                .collect()
        }
    }
}

/// Coverage of `shape` on `canvas`: tiles only where some pixel is
/// covered, values outside the canvas 0. A shape with no area (a click, a
/// line) covers nothing.
pub(crate) fn rasterize(
    shape: &SelectionShape,
    anti_alias: bool,
    canvas: IRect,
) -> TiledSurface<u8> {
    fill_even_odd(&polygon(shape), anti_alias, canvas)
}

struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Edge {
    fn x_at(&self, y: f64) -> f64 {
        self.x0 + (y - self.y0) * (self.x1 - self.x0) / (self.y1 - self.y0)
    }
}

fn fill_even_odd(points: &[[f64; 2]], anti_alias: bool, canvas: IRect) -> TiledSurface<u8> {
    let mut out = TiledSurface::new();
    if points.len() < 3 {
        return out;
    }
    let (mut min, mut max) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in points {
        for i in 0..2 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    let to_i32 = |v: f64| v.clamp(f64::from(i32::MIN / 2), f64::from(i32::MAX / 2)) as i32;
    let area = IRect::new(
        to_i32(min[0].floor()),
        to_i32(min[1].floor()),
        to_i32(max[0].ceil()),
        to_i32(max[1].ceil()),
    )
    .intersect(&canvas);
    if area.is_empty() {
        return out;
    }
    let mut edges: Vec<Edge> = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .filter(|(a, b)| a[1] != b[1])
        .map(|(a, b)| {
            let (a, b) = if a[1] < b[1] { (a, b) } else { (b, a) };
            Edge {
                x0: a[0],
                y0: a[1],
                x1: b[0],
                y1: b[1],
            }
        })
        .collect();
    edges.sort_by(|a, b| a.y0.total_cmp(&b.y0));

    let width = area.width() as usize;
    let (bx0, bx1) = (f64::from(area.x0), f64::from(area.x1));
    let (samples, weight) = if anti_alias {
        (SUBROWS, 1.0 / f64::from(SUBROWS))
    } else {
        (1, 1.0)
    };
    // acc: partial pixels; delta: running sums of fully covered runs.
    let mut acc = vec![0.0f64; width];
    let mut delta = vec![0.0f64; width + 1];
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    let mut xs: Vec<f64> = Vec::new();
    let mut row = vec![0u8; width];
    for y in area.y0..area.y1 {
        acc.fill(0.0);
        delta.fill(0.0);
        for k in 0..samples {
            let sy = if anti_alias {
                f64::from(y) + (f64::from(k) + 0.5) * weight
            } else {
                f64::from(y) + 0.5
            };
            while next < edges.len() && edges[next].y0 <= sy {
                active.push(next);
                next += 1;
            }
            active.retain(|&e| edges[e].y1 > sy);
            xs.clear();
            xs.extend(
                active
                    .iter()
                    .filter(|&&e| edges[e].y0 <= sy)
                    .map(|&e| edges[e].x_at(sy)),
            );
            xs.sort_by(f64::total_cmp);
            for span in xs.as_chunks::<2>().0 {
                let (l, r) = (span[0].max(bx0) - bx0, span[1].min(bx1) - bx0);
                if r <= l {
                    continue;
                }
                if anti_alias {
                    let (il, ir) = (l.floor() as usize, r.floor() as usize);
                    if il == ir {
                        acc[il] += (r - l) * weight;
                    } else {
                        acc[il] += (il as f64 + 1.0 - l) * weight;
                        delta[il + 1] += weight;
                        delta[ir] -= weight;
                        if ir < width {
                            acc[ir] += (r - ir as f64) * weight;
                        }
                    }
                } else {
                    // Pixels whose centre lies in [l, r).
                    let first = (l - 0.5).ceil().max(0.0) as usize;
                    let end = ((r - 0.5).ceil() as usize).min(width);
                    if first < end {
                        delta[first] += 1.0;
                        delta[end] -= 1.0;
                    }
                }
            }
        }
        let mut run = 0.0;
        let mut any = false;
        for (x, v) in row.iter_mut().enumerate() {
            run += delta[x];
            *v = (255.0 * (acc[x] + run).clamp(0.0, 1.0)).round() as u8;
            any |= *v > 0;
        }
        if any {
            write_row(&mut out, area.x0, y, &row);
        }
    }
    out
}

/// Writes one row of coverage starting at `(x0, y)`, allocating only the
/// tiles that receive a non-zero value.
fn write_row(out: &mut TiledSurface<u8>, x0: i32, y: i32, row: &[u8]) {
    let mut x = x0;
    let end = x0 + row.len() as i32;
    while x < end {
        let coord = TileCoord::containing(x, y);
        let (ox, oy) = coord.origin();
        let seg_end = (ox + TILE_SIZE as i32).min(end);
        let values = &row[(x - x0) as usize..(seg_end - x0) as usize];
        if values.iter().any(|&v| v > 0) {
            let tile: &mut Tile<u8> = out.tile_or_insert(coord);
            let line = tile.row_mut((y - oy) as u32);
            line[(x - ox) as usize..(seg_end - ox) as usize].copy_from_slice(values);
        }
        x = seg_end;
    }
}
