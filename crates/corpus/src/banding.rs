//! Banding measurements for phase 4E (ADR 0018, docs/parity/4E-16bit.md).
//!
//! A [`Profile`] is a line of pixels as the screen shows them (8-bit, over
//! white paper) next to the exact value the operation would give without
//! any storage rounding (`f64`). [`Metrics`] turns it into numbers: how
//! many distinct levels are shown, the largest jump between neighbours,
//! the error against the exact value, the widest flat run and how many
//! pixels lost paint the exact result shows. The scenarios here need only
//! the brush engine and stored tiles; the document-level ones live in the
//! `banding` example.

use glam::Vec2;
use tela_brush::dynamics::BrushProperty;
use tela_brush::{BrushEngine, BrushInput, BrushPreset, PixelAlpha, StrokeBuffer};
use tela_canvas::{TILE_SIZE, Tile, TileCoord};
use tela_color::{PremulRgba, StoragePixel};

/// One line of pixels: shown and exact, in 8-bit display units.
#[derive(Clone, Debug)]
pub struct Profile {
    /// As displayed: 8-bit, over white paper.
    pub shown: Vec<u8>,
    /// The same pixels computed in `f64` without storage rounding, in
    /// 8-bit units (not rounded).
    pub exact: Vec<f64>,
    /// Distinct values stored along the line (any channel layout; at
    /// 16 bits this can exceed 256).
    pub stored_levels: usize,
}

/// What a [`Profile`] shows, in 8-bit display units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Distinct shown values.
    pub levels: usize,
    /// Distinct stored values.
    pub stored_levels: usize,
    /// Largest difference between neighbouring shown pixels.
    pub max_jump: u8,
    /// Largest difference between neighbouring exact values: the jump
    /// the content itself has, which no storage can remove.
    pub exact_max_jump: f64,
    /// Largest |shown − exact|; 0.5 is the best any 8-bit display can do.
    pub max_error: f64,
    /// Mean |shown − exact|.
    pub mean_error: f64,
    /// Longest run of equal shown values inside the varying part of the
    /// line (from the first to the last pixel that differs from paper),
    /// in pixels.
    pub widest_plateau: usize,
    /// Pixels where the exact result has at least one full level of paint
    /// (below 254) but the stored result shows bare paper (255).
    pub lost: usize,
}

impl Metrics {
    /// Measures `p`.
    pub fn of(p: &Profile) -> Self {
        let mut levels = p.shown.clone();
        levels.sort_unstable();
        levels.dedup();
        let max_jump = p
            .shown
            .windows(2)
            .map(|w| w[0].abs_diff(w[1]))
            .max()
            .unwrap_or(0);
        let exact_max_jump = p
            .exact
            .windows(2)
            .map(|w| (w[0] - w[1]).abs())
            .fold(0.0, f64::max);
        let errors: Vec<f64> = p
            .shown
            .iter()
            .zip(&p.exact)
            .map(|(&s, &e)| (f64::from(s) - e).abs())
            .collect();
        let max_error = errors.iter().copied().fold(0.0, f64::max);
        let mean_error = errors.iter().sum::<f64>() / errors.len().max(1) as f64;
        let painted = |i: usize| p.shown[i] != 255;
        let first = (0..p.shown.len()).find(|&i| painted(i));
        let last = (0..p.shown.len()).rev().find(|&i| painted(i));
        let mut widest_plateau = 0;
        if let (Some(a), Some(b)) = (first, last) {
            let mut run = 1;
            for i in a + 1..=b {
                run = if p.shown[i] == p.shown[i - 1] {
                    run + 1
                } else {
                    1
                };
                widest_plateau = widest_plateau.max(run);
            }
        }
        let lost = p
            .shown
            .iter()
            .zip(&p.exact)
            .filter(|(s, e)| **s == 255 && **e < 254.0)
            .count();
        Self {
            levels: levels.len(),
            stored_levels: p.stored_levels,
            max_jump,
            exact_max_jump,
            max_error,
            mean_error,
            widest_plateau,
            lost,
        }
    }
}

/// A horizontal black stroke of a soft round brush through the middle of
/// tile (0, 0), as one finished [`StrokeBuffer`].
pub fn soft_stroke(size: f32, opacity: f32, flow: f32) -> StrokeBuffer {
    let mut preset = BrushPreset::plain(size);
    preset.properties.opacity = BrushProperty::constant(opacity);
    preset.properties.flow = BrushProperty::constant(flow);
    preset.properties.hardness = BrushProperty::constant(0.0);
    let mut e = BrushEngine::new(preset);
    let mid = TILE_SIZE as f32 / 2.0;
    e.begin_stroke(
        BrushInput::from_position(Vec2::new(-size, mid), 0.0),
        1.0,
        1,
    );
    e.add_input_at(
        BrushInput::from_position(Vec2::new(TILE_SIZE as f32 + size, mid), 1.0),
        1.0,
    );
    e.end_stroke().expect("a stroke was begun")
}

/// The same stroke committed `n` times (one commit per stroke, as a user
/// repeating it) into an empty tile stored as `P`; the profile is the
/// column through the middle, top to bottom. Exact:
/// `255 · (1 − m)ⁿ` for the stroke's alpha `m`.
pub fn repeated_strokes<P: StoragePixel>(stroke: &StrokeBuffer, n: u32) -> Profile {
    let c = TileCoord { x: 0, y: 0 };
    let mut tile: Tile<P> = Tile::new();
    for _ in 0..n {
        stroke.commit_tile(c, &mut tile, PixelAlpha::Free);
    }
    let x = TILE_SIZE / 2;
    let white = PremulRgba::new(1.0, 1.0, 1.0, 1.0);
    let mut stored: Vec<P> = Vec::new();
    let mut shown = Vec::new();
    let mut exact = Vec::new();
    for y in 0..TILE_SIZE {
        let p = tile.get(x, y);
        stored.push(p);
        shown.push(p.to_premul().over(white).to_rgba8()[0]);
        let m = f64::from(stroke.alpha(x as i32, y as i32));
        exact.push(255.0 * (1.0 - m).powi(n as i32));
    }
    Profile {
        shown,
        exact,
        stored_levels: distinct(&stored),
    }
}

/// A grey ramp from `from` to `to` (unit range) over `len` pixels, stored
/// as `P` (opaque); shown as stored, exact as the ramp.
pub fn stored_ramp<P: StoragePixel>(from: f64, to: f64, len: usize) -> Profile {
    let mut stored = Vec::with_capacity(len);
    let mut shown = Vec::with_capacity(len);
    let mut exact = Vec::with_capacity(len);
    for i in 0..len {
        let v = from + (to - from) * i as f64 / (len - 1) as f64;
        let p = P::from_premul(PremulRgba::new(v as f32, v as f32, v as f32, 1.0));
        stored.push(p);
        shown.push(p.to_premul().to_rgba8()[0]);
        exact.push(v * 255.0);
    }
    Profile {
        shown,
        exact,
        stored_levels: distinct(&stored),
    }
}

fn distinct<P: StoragePixel>(values: &[P]) -> usize {
    let mut seen: Vec<P> = Vec::new();
    for v in values {
        if !seen.contains(v) {
            seen.push(*v);
        }
    }
    seen.len()
}
