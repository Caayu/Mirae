//! Airbrush deposition must depend on time, never on the frame rate.

use glam::Vec2;
use tela_brush::preset::builtin_presets;
use tela_brush::{
    BrushEngine, BrushInput, BrushPreset, DabLog, StrokeParams, StrokeProcessor, StrokeSettings,
};

fn airbrush() -> BrushPreset {
    builtin_presets()
        .into_iter()
        .find(|p| p.id == "tela.airbrush")
        .unwrap()
}

fn input(x: f32, t: f64, pressure: f32) -> BrushInput {
    BrushInput::from_position(Vec2::new(x, 300.0), t).with_pressure(pressure)
}

/// Pen held still at x=300 for `seconds`, host refreshing at `fps`.
fn hold(preset: &BrushPreset, fps: f64, seconds: f64, pressure: f32) -> tela_brush::StrokeBuffer {
    let mut e = BrushEngine::new(preset.clone());
    e.begin_stroke(input(300.0, 0.0, pressure), 1.0, 42);
    let frames = (seconds * fps).round() as u32;
    for f in 1..=frames {
        e.advance_time(f as f64 / fps);
    }
    e.end_stroke().unwrap()
}

#[test]
fn holding_still_keeps_depositing() {
    let p = airbrush();
    let short = hold(&p, 60.0, 0.2, 1.0).alpha(300, 300);
    let long = hold(&p, 60.0, 1.0, 1.0).alpha(300, 300);
    assert!(short > 0.0);
    assert!(long > short + 0.2, "{short} -> {long}");
}

#[test]
fn same_result_at_30_and_144_fps() {
    let p = airbrush();
    let a = hold(&p, 30.0, 1.0, 0.8);
    let b = hold(&p, 144.0, 1.0, 0.8);
    for y in (180..420).step_by(7) {
        for x in (180..420).step_by(7) {
            assert_eq!(a.alpha(x, y), b.alpha(x, y), "({x},{y})");
        }
    }
}

#[test]
fn moving_strokes_are_frame_rate_independent() {
    let p = airbrush();
    let run = |fps: f64| {
        let mut proc = StrokeProcessor::new();
        let mut log = DabLog::default();
        let params = StrokeParams::new(&p, &StrokeSettings::default(), 1.0, 9);
        proc.begin_with(
            &p,
            &tela_brush::dynamics::Curve::linear(),
            &params,
            input(0.0, 0.0, 1.0),
            &mut log,
        );
        // Samples at 200 Hz delivered at their own timestamps; frames
        // interleaved at `fps`. The pen stops at x=400 for the last 0.3 s.
        let mut next_frame = 1.0 / fps;
        for i in 1..=200 {
            let t = i as f64 * 0.005;
            while next_frame < t {
                proc.advance_time(next_frame, &mut log);
                next_frame += 1.0 / fps;
            }
            proc.push(input((i as f32 * 2.0).min(400.0), t, 1.0), &mut log);
        }
        while next_frame < 1.3 {
            proc.advance_time(next_frame, &mut log);
            next_frame += 1.0 / fps;
        }
        // Both hosts lift the pen at the same instant.
        proc.advance_time(1.3, &mut log);
        proc.finish(&mut log);
        log.all().copied().collect::<Vec<_>>()
    };
    let (a, b) = (run(30.0), run(144.0));
    assert!(a.len() > 100);
    assert_eq!(a, b);
}

#[test]
fn pressure_controls_deposition() {
    let p = airbrush();
    let light = hold(&p, 60.0, 0.5, 0.2).alpha(300, 300);
    let firm = hold(&p, 60.0, 0.5, 1.0).alpha(300, 300);
    assert!(firm > light * 2.0, "{light} vs {firm}");
}

#[test]
fn non_airbrush_presets_ignore_time() {
    let p = BrushPreset::plain(20.0);
    let short = hold(&p, 60.0, 0.1, 1.0).alpha(300, 300);
    let long = hold(&p, 60.0, 2.0, 1.0).alpha(300, 300);
    assert_eq!(short, long);
}
