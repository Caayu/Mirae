//! Comparative evidence behind the default interpolator, on sparsely sampled
//! noise-free strokes (the worst case: a fast stroke with few samples).

use tela_brush::analysis::{Metrics, Scenario, max_turn_degrees};
use tela_brush::interpolate::{InterpolationKind, Interpolator, PathInterpolator};
use tela_brush::pipeline::StrokePoint;

fn sparse(name: &str, rate: f64) -> Scenario {
    Scenario::corpus()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap()
        .clean()
        .rate(rate)
}

fn interpolate(kind: InterpolationKind, s: &Scenario) -> Vec<StrokePoint> {
    let mut it = Interpolator::new(kind);
    let mut out = vec![];
    for i in s.samples() {
        it.push(i.into(), 1.0, &mut out);
    }
    it.finish(1.0, &mut out);
    out
}

fn turn(kind: InterpolationKind, s: &Scenario) -> f32 {
    max_turn_degrees(interpolate(kind, s).iter().map(|p| p.pos), 0.5)
}

#[test]
fn curved_interpolators_remove_artificial_kinks() {
    for s in [
        sparse("fast circle", 60.0),
        sparse("sine", 40.0),
        sparse("S", 25.0),
    ] {
        let linear = turn(InterpolationKind::Linear, &s);
        assert!(linear > 15.0, "{}: linear only kinks {linear}°", s.name);
        for kind in [InterpolationKind::CatmullRom, InterpolationKind::Bezier] {
            let t = turn(kind, &s);
            assert!(t < 3.0, "{} {kind:?}: {t}°", s.name);
        }
    }
}

#[test]
fn curves_follow_the_true_path_better_than_chords() {
    let s = sparse("fast circle", 60.0);
    let linear = Metrics::measure(&interpolate(InterpolationKind::Linear, &s), &s);
    let cr = Metrics::measure(&interpolate(InterpolationKind::CatmullRom, &s), &s);
    assert!(
        cr.rms_error < linear.rms_error * 0.25,
        "{cr:?} vs {linear:?}"
    );
}

#[test]
fn default_keeps_abrupt_direction_changes_tight() {
    let s = sparse("corner", 30.0);
    let cr = Metrics::measure(&interpolate(InterpolationKind::CatmullRom, &s), &s);
    let bz = Metrics::measure(&interpolate(InterpolationKind::Bezier, &s), &s);
    assert_eq!(InterpolationKind::default(), InterpolationKind::CatmullRom);
    assert!(
        cr.max_error <= bz.max_error,
        "catmull-rom {cr:?} vs bezier {bz:?}"
    );
    assert!(cr.max_error < 2.5, "{cr:?}");
    for kind in InterpolationKind::ALL {
        let m = Metrics::measure(&interpolate(kind, &s), &s);
        assert!(m.overshoot < 0.5, "{kind:?} overshoots {}", m.overshoot);
    }
}

#[test]
fn fast_straight_stroke_with_few_samples_stays_straight() {
    let s = sparse("fast line", 20.0);
    assert!(s.samples().len() <= 8);
    for kind in InterpolationKind::ALL {
        let m = Metrics::measure(&interpolate(kind, &s), &s);
        assert!(m.max_error < 0.01, "{kind:?}: {m:?}");
    }
}

#[test]
fn noisy_input_after_stabilization_stays_smooth() {
    use tela_brush::stabilizer::{StabilizerKind, StrokeStabilizer, stabilizer};
    let s = Scenario::corpus()
        .into_iter()
        .find(|s| s.name == "circle")
        .unwrap();
    let mut st = stabilizer(StabilizerKind::default(), 50.0);
    let mut it = Interpolator::new(InterpolationKind::default());
    let mut out = vec![];
    for i in s.samples() {
        it.push(st.push(i.into(), 1.0), 1.0, &mut out);
    }
    it.finish(1.0, &mut out);
    let m = Metrics::measure(&out, &s);
    assert!(m.rms_error < 1.0, "{m:?}");
}
