//! Comparative evidence behind the default stabilizer and the 0–100 scale.

use tela_brush::analysis::{Metrics, Scenario};
use tela_brush::pipeline::StrokePoint;
use tela_brush::stabilizer::{StabilizerKind, StrokeStabilizer, stabilizer};

fn scenario(name: &str) -> Scenario {
    Scenario::corpus()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap()
}

fn run(kind: StabilizerKind, strength: f32, s: &Scenario) -> Metrics {
    let mut f = stabilizer(kind, strength);
    let out: Vec<StrokePoint> = s
        .samples()
        .into_iter()
        .map(|i| f.push(i.into(), 1.0))
        .collect();
    Metrics::measure(&out, s)
}

/// (slow-line jitter, fast-circle lag)
fn tradeoff(kind: StabilizerKind, strength: f32) -> (f32, f32) {
    (
        run(kind, strength, &scenario("slow line")).jitter,
        run(kind, strength, &scenario("fast circle")).lag_ms,
    )
}

#[test]
fn more_strength_means_less_jitter_and_more_lag() {
    for kind in [
        StabilizerKind::MovingAverage,
        StabilizerKind::Exponential,
        StabilizerKind::OneEuro,
    ] {
        let mut prev = tradeoff(kind, 0.0);
        for st in [10.0, 25.0, 50.0, 75.0, 100.0] {
            let cur = tradeoff(kind, st);
            assert!(
                cur.0 < prev.0,
                "{kind:?}@{st}: jitter {} !< {}",
                cur.0,
                prev.0
            );
            assert!(cur.1 > prev.1, "{kind:?}@{st}: lag {} !> {}", cur.1, prev.1);
            prev = cur;
        }
    }
}

#[test]
fn default_scale_meets_its_behavioural_targets() {
    let kind = StabilizerKind::default();
    let (raw_jitter, raw_lag) = tradeoff(kind, 0.0);
    assert!(
        raw_lag.abs() < 0.5,
        "0 must be (practically) no stabilization"
    );

    // Low values: remove micro-jitter without perceptible lag.
    let (j10, l10) = tradeoff(kind, 10.0);
    assert!(
        j10 < raw_jitter * 0.65,
        "10: jitter {j10} vs raw {raw_jitter}"
    );
    assert!(l10 < 3.0, "10: lag {l10} ms");

    // Middle values: line-art quality, still responsive.
    let (j50, l50) = tradeoff(kind, 50.0);
    assert!(j50 < raw_jitter * 0.3, "50: jitter {j50}");
    assert!(l50 < 10.0, "50: lag {l50} ms");

    // Top end: very smooth, latency allowed.
    let (j100, _) = tradeoff(kind, 100.0);
    assert!(j100 < raw_jitter * 0.1, "100: jitter {j100}");
}

#[test]
fn one_euro_is_not_dominated_by_the_alternatives() {
    for reference in [50.0, 75.0] {
        let (oe_jitter, oe_lag) = tradeoff(StabilizerKind::OneEuro, reference);
        for kind in [StabilizerKind::MovingAverage, StabilizerKind::Exponential] {
            for st in (5..=100).step_by(5) {
                let (j, l) = tradeoff(kind, st as f32);
                assert!(
                    !(j <= oe_jitter && l <= oe_lag),
                    "{kind:?}@{st} ({j:.3}px, {l:.1}ms) dominates one-euro@{reference} ({oe_jitter:.3}px, {oe_lag:.1}ms)"
                );
            }
        }
    }
}

#[test]
fn no_stabilizer_overshoots_a_sharp_corner() {
    let corner = scenario("corner");
    for kind in StabilizerKind::ALL {
        for st in [25.0, 50.0, 100.0] {
            let m = run(kind, st, &corner);
            // Allow the sensor noise itself (a few sigma) but no systematic overshoot.
            assert!(m.overshoot < 2.0, "{kind:?}@{st}: {}", m.overshoot);
        }
    }
}

#[test]
fn behaviour_is_independent_of_sample_rate() {
    // The same slow line at 120 Hz and 480 Hz gets comparable smoothing.
    let base = scenario("slow line");
    let lag = |rate| {
        let s = base.clone().rate(rate);
        run(StabilizerKind::OneEuro, 50.0, &s).lag_ms
    };
    let (a, b) = (lag(120.0), lag(480.0));
    assert!(
        (a - b).abs() < a.max(b) * 0.35 + 1.0,
        "120 Hz {a} ms vs 480 Hz {b} ms"
    );
}
