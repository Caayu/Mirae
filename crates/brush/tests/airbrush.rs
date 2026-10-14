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

/// Airbrush stroke along y=300 at `speed` canvas px/s, 200 Hz samples.
fn moving(speed: f32) -> tela_brush::StrokeBuffer {
    let p = airbrush();
    let mut e = BrushEngine::new(p);
    let step = speed / 200.0;
    let n = (2000.0 / step) as usize;
    e.begin_stroke(input(0.0, 0.0, 1.0), 1.0, 3);
    for i in 1..=n {
        let t = i as f64 / 200.0;
        e.advance_time(t);
        e.add_input(input(i as f32 * step, t, 1.0));
    }
    e.end_stroke().unwrap()
}

#[test]
fn fast_strokes_deposit_evenly_without_beads() {
    let s = moving(14_000.0);
    let line: Vec<f32> = (400..1600).map(|x| s.alpha(x, 300)).collect();
    let mean = line.iter().sum::<f32>() / line.len() as f32;
    assert!(mean > 0.01, "something is deposited");
    // Beads are local: compare each pixel with its ±30 px neighbourhood
    // (slow trends from the pen accelerating are legitimate).
    let ripple = (30..line.len() - 30)
        .map(|i| {
            let local = line[i - 30..=i + 30].iter().sum::<f32>() / 61.0;
            (line[i] - local).abs() / local
        })
        .fold(0.0, f32::max);
    assert!(ripple < 0.02, "beads: local deviation {ripple}");
}

#[test]
fn slower_strokes_deposit_more_paint() {
    let mean =
        |s: &tela_brush::StrokeBuffer| (400..1600).map(|x| s.alpha(x, 300)).sum::<f32>() / 1200.0;
    let (slow, fast) = (mean(&moving(600.0)), mean(&moving(6000.0)));
    assert!(slow > fast * 1.5, "slow {slow} vs fast {fast}");
}

#[test]
fn delayed_input_delivery_does_not_create_beads() {
    // The host clock runs 50 ms ahead of the samples, as when the system
    // delivers pen input in late batches: the processed path lags "now" by
    // more than the hold delay while the pen is still moving.
    let mut e = BrushEngine::new(airbrush());
    let step = 70.0;
    e.begin_stroke(input(0.0, 0.0, 1.0), 1.0, 3);
    // Samples arrive in batches of four, one batch per 60 Hz frame, and
    // frames also run while a batch is late.
    for batch in 0..8 {
        let frame = batch as f64 * 4.0 / 200.0 + 0.05;
        e.advance_time(frame);
        e.advance_time(frame + 0.008);
        for j in 1..=4 {
            let i = batch * 4 + j;
            e.add_input_at(input(i as f32 * step, i as f64 / 200.0, 1.0), frame);
        }
    }
    let s = e.end_stroke().unwrap();
    let line: Vec<f32> = (400..1700).map(|x| s.alpha(x, 300)).collect();
    let ripple = (30..line.len() - 30)
        .map(|i| {
            let local = line[i - 30..=i + 30].iter().sum::<f32>() / 61.0;
            (line[i] - local).abs() / local
        })
        .fold(0.0, f32::max);
    assert!(ripple < 0.02, "beads: local deviation {ripple}");
}

#[test]
fn rest_after_late_delivery_still_builds_up() {
    let run = |rest: f64| {
        let mut e = BrushEngine::new(airbrush());
        e.begin_stroke(input(0.0, 0.0, 1.0), 1.0, 3);
        for i in 1..=20 {
            let t = i as f64 / 200.0;
            e.add_input_at(input(i as f32 * 10.0, t, 1.0), t + 0.05);
        }
        let mut now = 0.15;
        while now < 0.15 + rest {
            e.advance_time(now);
            now += 1.0 / 60.0;
        }
        e.end_stroke().unwrap().alpha(200, 300)
    };
    let (short, long) = (run(0.1), run(1.0));
    assert!(
        long > short + 0.2,
        "resting must keep depositing: {short} -> {long}"
    );
}
