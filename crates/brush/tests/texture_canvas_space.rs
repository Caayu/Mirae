//! The paper texture belongs to the canvas, not to the dab.

use glam::Vec2;
use tela_brush::dynamics::BrushProperty;
use tela_brush::texture::{TextureMode, TexturePattern, TextureSettings};
use tela_brush::{BrushEngine, BrushInput, BrushPreset, StrokeBuffer};

fn textured(spacing: f32, mode: TextureMode) -> BrushPreset {
    let mut p = BrushPreset::plain(60.0);
    p.properties.spacing = BrushProperty::constant(spacing);
    p.properties.hardness = BrushProperty::constant(1.0);
    // Height mode fills the grain completely at full opacity (a firm
    // press), so test it with a lighter touch.
    p.properties.opacity = BrushProperty::constant(if mode == TextureMode::Height {
        0.4
    } else {
        1.0
    });
    p.texture = Some(TextureSettings {
        pattern: TexturePattern::PaperRough,
        mode,
        ..Default::default()
    });
    p
}

fn stroke(settings: BrushPreset, from: Vec2, to: Vec2) -> StrokeBuffer {
    let mut e = BrushEngine::new(settings);
    let n = 40;
    e.begin_stroke(BrushInput::from_position(from, 0.0), 1.0, 0);
    for i in 1..=n {
        let t = i as f32 / n as f32;
        e.add_input(BrushInput::from_position(from.lerp(to, t), t as f64 * 0.4));
    }
    e.end_stroke().unwrap()
}

#[test]
fn grain_is_identical_for_different_strokes_over_the_same_paper() {
    for mode in [TextureMode::Multiply, TextureMode::Height] {
        // Horizontal stroke, dense spacing.
        let a = stroke(
            textured(0.05, mode),
            Vec2::new(0.0, 200.0),
            Vec2::new(500.0, 205.0),
        );
        // Diagonal stroke in the other direction, sparse spacing.
        let b = stroke(
            textured(0.2, mode),
            Vec2::new(420.0, 120.0),
            Vec2::new(80.0, 290.0),
        );
        let mut compared = 0;
        let mut varied = (f32::MAX, 0f32);
        for y in 196..210 {
            for x in 230..270 {
                let (pa, pb) = (a.alpha(x, y), b.alpha(x, y));
                assert!((pa - pb).abs() < 1e-4, "{mode:?} ({x},{y}): {pa} vs {pb}");
                varied = (varied.0.min(pa), varied.1.max(pa));
                compared += 1;
            }
        }
        assert!(compared > 400);
        assert!(
            varied.1 - varied.0 > 0.2,
            "{mode:?}: texture visible ({varied:?})"
        );
    }
}

#[test]
fn texture_strength_zero_is_untextured() {
    let mut s = textured(0.1, TextureMode::Multiply);
    s.properties.texture_strength = BrushProperty::constant(0.0);
    let b = stroke(s, Vec2::new(0.0, 100.0), Vec2::new(300.0, 100.0));
    for x in 50..250 {
        assert!(b.alpha(x, 100) > 0.999);
    }
}
