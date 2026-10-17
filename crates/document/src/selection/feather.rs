//! Feathering a selection ("Desfocar borda").
//!
//! Measured on the reference (parity record 5C, S19–S21): a sampled,
//! normalized discrete Gaussian with σ = 0.3 × strength and radius
//! ⌈3σ⌉ reproduces its results within 2/255 from 1 to 50 px; the kernel is
//! renormalized over the canvas (pixels outside the canvas take no part).
//! Applied separably (the canvas is a rectangle, so the renormalization
//! factorizes). Only pixels within the radius of a stored value can change:
//! everything else stays the fill, so the work is bounded by the stored
//! region, not by the canvas.

use tela_canvas::{IRect, Tile, TileCoord};

use super::Selection;

/// σ per pixel of strength (measured, S19).
pub(crate) const SIGMA_PER_PX: f32 = 0.3;

/// The largest strength accepted, in pixels.
pub const MAX_FEATHER: f32 = 1000.0;

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
    let grow = |r: IRect, d: i32| IRect::new(r.x0 - d, r.y0 - d, r.x1 + d, r.y1 + d);
    // Output region: where a stored value is within reach.
    let out = grow(sel.stored_bounds(), radius).intersect(&canvas);
    if out.is_empty() {
        return Vec::new();
    }
    // Input region: the output region plus the kernel, on the canvas.
    let input = grow(out, radius).intersect(&canvas);
    let (w, h) = (out.width() as usize, input.height() as usize);
    // Horizontal pass: rows of the input region, columns of the output.
    let mut line = vec![0.0f32; input.width() as usize];
    let mut horizontal = vec![0.0f32; w * h];
    let mut runs = Vec::new();
    for (row, y) in (input.y0..input.y1).enumerate() {
        for (v, x) in line.iter_mut().zip(input.x0..input.x1) {
            *v = f32::from(sel.coverage(x, y));
        }
        convolve(
            &line,
            input.x0,
            out.x0,
            &weights,
            &mut runs,
            &mut horizontal[row * w..(row + 1) * w],
        );
    }
    // Vertical pass, column by column into the output region.
    let mut column = vec![0.0f32; h];
    let mut result = vec![0.0f32; w * out.height() as usize];
    let mut blurred = vec![0.0f32; out.height() as usize];
    for x in 0..w {
        for (v, row) in column.iter_mut().zip(0..h) {
            *v = horizontal[row * w + x];
        }
        convolve(&column, input.y0, out.y0, &weights, &mut runs, &mut blurred);
        for (y, v) in blurred.iter().enumerate() {
            result[y * w + x] = *v;
        }
    }
    // New stored tiles where the output region changed something.
    let fill = if sel.is_inverted_storage() { 255u8 } else { 0 };
    let mut changed = Vec::new();
    for coord in out.tiles() {
        let old = sel.stored.tile(coord);
        let mut tile = old.cloned().unwrap_or_default();
        let part = coord.rect().intersect(&out);
        let (ox, oy) = coord.origin();
        for y in part.y0..part.y1 {
            for x in part.x0..part.x1 {
                let v = result[(y - out.y0) as usize * w + (x - out.x0) as usize];
                *tile.get_mut((x - ox) as u32, (y - oy) as u32) =
                    (v.round().clamp(0.0, 255.0) as u8) ^ fill;
            }
        }
        let new = (!tile.is_blank()).then_some(tile);
        if new.as_ref() != old {
            changed.push((coord, new));
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
