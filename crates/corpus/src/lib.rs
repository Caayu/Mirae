//! Brush corpus: standard trajectories, deterministic rendering of a preset
//! over all of them into one image ("sheet"), PNG I/O and tolerant image
//! comparison for golden tests.

pub mod image;
pub mod trajectory;

use glam::Vec2;
use tela_brush::interpolate::InterpolationKind;
use tela_brush::preset::PaintMode;
use tela_brush::stabilizer::StabilizerKind;
use tela_brush::{BrushEngine, BrushPreset, StrokeSettings};
use tela_canvas::{TILE_SIZE, TiledSurface};
use tela_color::{PremulRgba, Rgba};

pub use crate::image::{Diff, Image, Thresholds};
pub use crate::trajectory::{CELL, COLUMNS, ROWS, Trajectory, trajectories};

/// Renders `preset` over every standard trajectory: a `COLUMNS x ROWS`
/// grid of `CELL`-sized cells on white paper.
pub fn render_sheet(preset: &BrushPreset) -> Image {
    let (w, h) = (CELL * COLUMNS, CELL * ROWS);
    let mut layer = TiledSurface::<[u8; 4]>::new();
    if preset.compositing.mode == PaintMode::Erase {
        // An eraser needs paint to remove: start from a filled sheet.
        for c in tela_canvas::IRect::from_size(w, h).tiles() {
            *layer.tile_or_insert(c) = tela_canvas::Tile::filled([70, 72, 84, 255]);
        }
    }
    let mut seed = 0x5EED_0000u64;
    for (i, traj) in trajectories().iter().enumerate() {
        let origin = Vec2::new(
            ((i as u32 % COLUMNS) * CELL) as f32,
            ((i as u32 / COLUMNS) * CELL) as f32,
        );
        let mut p = preset.clone();
        if traj.force_taper {
            p.taper.start = p.taper.start.max(40.0);
            p.taper.end = p.taper.end.max(70.0);
        }
        let mut engine = BrushEngine::new(p);
        engine.color = Rgba::new(0.08, 0.08, 0.1, 1.0);
        // Fixed user settings so goldens do not change with app defaults.
        engine.stroke_defaults = StrokeSettings {
            stabilizer: StabilizerKind::OneEuro,
            stabilization: 20.0,
            interpolation: InterpolationKind::CatmullRom,
            jitter_threshold: 0.35,
        };
        for stroke in &traj.strokes {
            seed += 1;
            let mut samples = stroke.iter().map(|s| s.translated(origin));
            let Some(first) = samples.next() else {
                continue;
            };
            engine.begin_stroke(first, 1.0, seed);
            for s in samples {
                // Simulated time: the host refreshes at every sample.
                engine.advance_time(s.timestamp);
                engine.add_input(s);
            }
            if let Some(last) = stroke.last() {
                engine.advance_time(last.time + traj.hold_after);
            }
            if let Some(buffer) = engine.end_stroke() {
                buffer.commit_into(&mut layer);
            }
        }
    }
    flatten(&layer, w, h)
}

/// Composites a layer over white paper into a straight RGBA8 image.
pub fn flatten(layer: &TiledSurface<[u8; 4]>, w: u32, h: u32) -> Image {
    let mut img = Image::new(w, h);
    for c in layer.tile_coords() {
        let tile = layer.tile(c).expect("listed");
        let (ox, oy) = c.origin();
        for ty in 0..TILE_SIZE as i32 {
            for tx in 0..TILE_SIZE as i32 {
                let (x, y) = (ox + tx, oy + ty);
                if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                    continue;
                }
                let p = PremulRgba::from_rgba8(tile.get(tx as u32, ty as u32));
                let white = PremulRgba::new(1.0, 1.0, 1.0, 1.0);
                img.set(x as u32, y as u32, p.over(white).to_rgba8());
            }
        }
    }
    img
}
