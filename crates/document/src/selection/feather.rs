//! Feathering a selection ("Desfocar borda").
//!
//! Measured on the reference (parity record 5C, S19–S21): a sampled,
//! normalized discrete Gaussian with σ = 0.3 × strength and radius
//! ⌈3σ⌉ reproduces its results within 2/255 from 1 to 50 px; the kernel is
//! renormalized over the canvas (pixels outside the canvas take no part).
//! Applied separably (the canvas is a rectangle, so the renormalization
//! factorizes). Only pixels within the radius of a stored value can change:
//! everything else stays the fill, so the work is bounded by the stored
//! region, not by the canvas. The region is processed one tile column at a
//! time, so the scratch memory is one band, whatever the selection's size.

use tela_canvas::{IRect, TILE_SIZE, Tile, TileCoord};

use super::Selection;

/// σ per pixel of strength (measured, S19).
pub(crate) const SIGMA_PER_PX: f32 = 0.3;

/// The largest strength accepted, in pixels.
pub const MAX_FEATHER: f32 = 1000.0;

/// Columns gathered at once for the vertical pass.
const BLOCK: usize = 16;

/// The stored tiles feathering `sel` by `strength` px changes, with their
/// new content (`None` = absent). Reads only; changes nothing.
pub(crate) fn feather_tiles(sel: &Selection, strength: f32) -> Vec<(TileCoord, Option<Tile<u8>>)> {
    let sigma = SIGMA_PER_PX * strength;
    if !sigma.is_finite() || sigma <= 0.0 {
        return Vec::new();
    }
    let radius = (3.0 * sigma).ceil().max(1.0) as i32;
    let weights: Vec<f32> = (-radius..=radius)
        .map(|k| (-((k * k) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let canvas = sel.canvas();
    let grow = |r: IRect, dx: i32, dy: i32| IRect::new(r.x0 - dx, r.y0 - dy, r.x1 + dx, r.y1 + dy);
    // Output region: where a stored value is within reach.
    let out = grow(sel.stored_bounds(), radius, radius).intersect(&canvas);
    if out.is_empty() {
        return Vec::new();
    }
    let fill = if sel.is_inverted_storage() { 255u8 } else { 0 };
    // Input rows: the output rows plus the kernel, on the canvas.
    let rows = grow(out, 0, radius).intersect(&canvas);
    let h = rows.height() as usize;
    let (mut bytes, mut line, mut runs, mut horizontal) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut column = vec![0.0f32; h];
    let mut blurred = vec![0.0f32; out.height() as usize];
    let mut block = vec![0.0f32; BLOCK * h];
    let mut changed = Vec::new();
    let t = TILE_SIZE as i32;
    for tx in out.x0.div_euclid(t)..=(out.x1 - 1).div_euclid(t) {
        // One band: the output columns of this tile column.
        let band = IRect::new(tx * t, out.y0, tx * t + t, out.y1).intersect(&out);
        let bw = band.width() as usize;
        let input = grow(band, radius, 0).intersect(&canvas);
        bytes.resize(input.width() as usize, 0u8);
        line.resize(input.width() as usize, 0.0f32);
        horizontal.clear();
        horizontal.resize(bw * h, 0.0f32);
        for (row, y) in (rows.y0..rows.y1).enumerate() {
            sel.read_line(input.x0, y, &mut bytes);
            for (l, &b) in line.iter_mut().zip(&bytes) {
                *l = f32::from(b);
            }
            convolve(
                &line,
                input.x0,
                band.x0,
                &weights,
                &mut runs,
                &mut horizontal[row * bw..(row + 1) * bw],
            );
        }
        // The band's tiles, starting from their current content.
        let ty0 = band.y0.div_euclid(t);
        let mut tiles: Vec<(TileCoord, Tile<u8>)> = (ty0..=(band.y1 - 1).div_euclid(t))
            .map(|ty| {
                let c = TileCoord { x: tx, y: ty };
                (c, sel.stored.tile(c).cloned().unwrap_or_default())
            })
            .collect();
        // Vertical pass, gathering a block of columns per sweep of rows.
        for first in (0..bw).step_by(BLOCK) {
            let n = BLOCK.min(bw - first);
            for (row, chunk) in horizontal.chunks_exact(bw).enumerate() {
                for (i, &v) in chunk[first..first + n].iter().enumerate() {
                    block[i * h + row] = v;
                }
            }
            for i in 0..n {
                column.copy_from_slice(&block[i * h..(i + 1) * h]);
                convolve(&column, rows.y0, band.y0, &weights, &mut runs, &mut blurred);
                let x = (band.x0 + (first + i) as i32).rem_euclid(t) as u32;
                for (y, &v) in (band.y0..band.y1).zip(&blurred) {
                    let tile = &mut tiles[(y.div_euclid(t) - ty0) as usize].1;
                    *tile.get_mut(x, y.rem_euclid(t) as u32) =
                        (v.round().clamp(0.0, 255.0) as u8) ^ fill;
                }
            }
        }
        for (c, tile) in tiles {
            let old = sel.stored.tile(c);
            let new = (!tile.is_blank()).then_some(tile);
            if new.as_ref() != old {
                changed.push((c, new));
            }
        }
    }
    changed
}

/// One pass of the renormalized kernel: `input` starts at coordinate
/// `start` and covers every canvas position the kernel can reach from the
/// outputs, which start at `out_start`. Positions outside `input` are
/// outside the canvas and take no part. `runs` is scratch.
fn convolve(
    input: &[f32],
    start: i32,
    out_start: i32,
    weights: &[f32],
    runs: &mut Vec<usize>,
    out: &mut [f32],
) {
    // runs[j]: last index of the run of equal values starting at j.
    runs.clear();
    runs.resize(input.len(), 0);
    for j in (0..input.len()).rev() {
        runs[j] = if j + 1 < input.len() && input[j + 1] == input[j] {
            runs[j + 1]
        } else {
            j
        };
    }
    let r = (weights.len() / 2) as i32;
    let n = input.len() as i32;
    for (i, o) in out.iter_mut().enumerate() {
        let centre = out_start + i as i32 - start;
        let (lo, hi) = ((centre - r).max(0), (centre + r).min(n - 1));
        // A uniform window keeps its value exactly (the kernel is
        // renormalized), which skips most of a large selection.
        if runs[lo as usize] >= hi as usize {
            *o = input[lo as usize];
            continue;
        }
        let (mut sum, mut norm) = (0.0f32, 0.0f32);
        for j in lo..=hi {
            let wgt = weights[(j - centre + r) as usize];
            sum += wgt * input[j as usize];
            norm += wgt;
        }
        *o = sum / norm;
    }
}
