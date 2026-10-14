//! Dab placement must depend on the path, not on the device's sample rate.
//!
//! Tolerance: equal dab count, centers within 0.01 px and radii within
//! 0.005 px of the 1000-sample reference.

use glam::Vec2;
use tela_brush::dynamics::{BrushProperty, Curve, DynamicsSource, Effector};
use tela_brush::interpolate::InterpolationKind;
use tela_brush::{BrushInput, BrushSettings, Dab, DabLog, StrokeProcessor};

const POS_TOL: f32 = 0.01;
const RADIUS_TOL: f32 = 0.005;

fn pressure_brush() -> BrushSettings {
    BrushSettings {
        size: BrushProperty::constant(20.0)
            .with(Effector::new(DynamicsSource::Pressure, Curve::linear())),
        spacing: 0.15,
        ..BrushSettings::plain(20.0)
    }
}

/// Draws `path(u)` with `n` evenly spaced samples; pressure is `pressure(u)`.
fn draw(
    settings: &BrushSettings,
    n: usize,
    path: impl Fn(f32) -> Vec2,
    pressure: impl Fn(f32) -> f32,
) -> Vec<Dab> {
    let sample = |i: usize| {
        let u = i as f32 / (n - 1) as f32;
        BrushInput::from_position(path(u), u as f64).with_pressure(pressure(u))
    };
    let mut p = StrokeProcessor::new();
    let mut log = DabLog::default();
    p.begin(settings, &Curve::linear(), 1.0, sample(0), &mut log);
    for i in 1..n {
        p.push(sample(i), &mut log);
    }
    p.finish(&mut log);
    log.all().copied().collect()
}

fn assert_equivalent(reference: &[Dab], other: &[Dab], label: &str) {
    assert_eq!(reference.len(), other.len(), "{label}: dab count");
    let (max_dp, max_dr) = reference
        .iter()
        .zip(other)
        .map(|(a, b)| ((a.center - b.center).length(), (a.radius - b.radius).abs()))
        .fold((0.0f32, 0.0f32), |(p, r), (dp, dr)| (p.max(dp), r.max(dr)));
    eprintln!(
        "{label}: {} dabs, max Δpos {max_dp:.2e} px, max Δr {max_dr:.2e} px",
        other.len()
    );
    for (i, (a, b)) in reference.iter().zip(other).enumerate() {
        let dp = (a.center - b.center).length();
        let dr = (a.radius - b.radius).abs();
        assert!(
            dp < POS_TOL && dr < RADIUS_TOL,
            "{label} dab {i}: Δpos {dp} Δr {dr}"
        );
    }
}

const COUNTS: [usize; 5] = [2, 7, 33, 101, 1000];

fn line(u: f32) -> Vec2 {
    Vec2::new(50.0, 80.0).lerp(Vec2::new(950.0, 380.0), u)
}

#[test]
fn straight_stroke_with_varying_pressure_is_sample_count_invariant() {
    let ramp = |u: f32| 0.2 + 0.8 * u;
    for interpolation in InterpolationKind::ALL {
        let settings = BrushSettings {
            interpolation,
            ..pressure_brush()
        };
        let reference = draw(&settings, 1000, line, ramp);
        assert!(reference.len() > 100);
        for n in COUNTS {
            assert_equivalent(
                &reference,
                &draw(&settings, n, line, ramp),
                &format!("{interpolation:?} n={n}"),
            );
        }
    }
}

#[test]
fn tapers_do_not_break_invariance() {
    let mut settings = pressure_brush();
    settings.taper.start = 60.0;
    settings.taper.end = 90.0;
    let constant = |_| 1.0;
    let reference = draw(&settings, 1000, line, constant);
    for n in COUNTS {
        assert_equivalent(
            &reference,
            &draw(&settings, n, line, constant),
            &format!("n={n}"),
        );
    }
    // Both ends really are tapered.
    assert!(reference.first().unwrap().radius < 1.5);
    assert!(reference.last().unwrap().radius < 1.5);
}

#[test]
fn spacing_matches_size_everywhere_on_a_curve() {
    let settings = pressure_brush();
    let circle = |u: f32| {
        Vec2::new(500.0, 500.0) + 200.0 * Vec2::from_angle(std::f32::consts::TAU * 0.9 * u)
    };
    let wave = |u: f32| 0.3 + 0.7 * (0.5 + 0.5 * (u * 12.0).sin());
    for n in [7, 33, 101, 1000] {
        let dabs = draw(&settings, n, circle, wave);
        for w in dabs.windows(2) {
            let gap = (w[1].center - w[0].center).length();
            let expected = 2.0 * w[0].radius * settings.spacing;
            assert!(
                (gap - expected).abs() <= expected * 0.02 + 0.01,
                "n={n}: gap {gap} vs {expected}"
            );
        }
    }
}

#[test]
fn dense_curved_strokes_converge() {
    let settings = pressure_brush();
    let circle = |u: f32| {
        Vec2::new(500.0, 500.0) + 200.0 * Vec2::from_angle(std::f32::consts::TAU * 0.9 * u)
    };
    let wave = |u: f32| 0.4 + 0.6 * u;
    let reference = draw(&settings, 1000, circle, wave);
    let coarse = draw(&settings, 101, circle, wave);
    // A 101-sample circle is not the same curve as a 1000-sample one, but the
    // dab path must agree to well under a pixel.
    let n = reference.len().min(coarse.len());
    assert!(reference.len().abs_diff(coarse.len()) <= 1);
    for i in 0..n {
        assert!(
            (reference[i].center - coarse[i].center).length() < 0.5,
            "dab {i}"
        );
    }
}

#[test]
fn pressure_changes_never_jump_between_dabs() {
    let settings = pressure_brush();
    let wave = |u: f32| 0.5 + 0.5 * (u * 9.0).sin();
    for n in [33, 101, 1000] {
        let dabs = draw(&settings, n, line, wave);
        // |dr/ds| <= max|dp/du| * (du/ds) * (size/2) = 4.5 * (1/949) * 10.
        let max_slope = 4.5 / 949.0 * 10.0;
        for w in dabs.windows(2) {
            let ds = (w[1].center - w[0].center).length();
            let dr = (w[1].radius - w[0].radius).abs();
            assert!(
                dr <= max_slope * ds * 1.1 + 1e-3,
                "n={n}: Δr {dr} over {ds}px"
            );
        }
    }
}
