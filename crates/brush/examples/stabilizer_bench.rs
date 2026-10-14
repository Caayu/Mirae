//! Compares stabilizers over the synthetic corpus.
//!
//! ```sh
//! cargo run --release -p tela-brush --example stabilizer_bench
//! ```

use tela_brush::analysis::{Metrics, Scenario};
use tela_brush::pipeline::StrokePoint;
use tela_brush::stabilizer::{StabilizerKind, StrokeStabilizer, stabilizer};

fn run(kind: StabilizerKind, strength: f32, scenario: &Scenario) -> Metrics {
    let mut s = stabilizer(kind, strength);
    let out: Vec<StrokePoint> = scenario
        .samples()
        .into_iter()
        .map(|i| s.push(i.into(), 1.0))
        .collect();
    Metrics::measure(&out, scenario)
}

fn main() {
    let corpus = Scenario::corpus();
    let by_name = |n: &str| corpus.iter().find(|s| s.name == n).unwrap();
    let raw = run(StabilizerKind::None, 0.0, by_name("slow line"));
    println!(
        "raw input: slow-line jitter {:.3}px, rms {:.3}px\n",
        raw.jitter, raw.rms_error
    );

    println!(
        "| algorithm | strength | slow jitter px | slow rms px | circle rms px | fast-line lag ms | fast-circle lag ms | fast-circle rms px | corner overshoot px |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    for kind in StabilizerKind::ALL {
        let strengths: &[f32] = if kind == StabilizerKind::None {
            &[0.0]
        } else {
            &[10.0, 25.0, 50.0, 75.0, 100.0]
        };
        for &st in strengths {
            let slow = run(kind, st, by_name("slow line"));
            let circle = run(kind, st, by_name("circle"));
            let fast_line = run(kind, st, by_name("fast line"));
            let fast_circle = run(kind, st, by_name("fast circle"));
            let corner = run(kind, st, by_name("corner"));
            println!(
                "| {} | {st:.0} | {:.3} | {:.3} | {:.2} | {:.1} | {:.1} | {:.2} | {:.2} |",
                kind.name(),
                slow.jitter,
                slow.rms_error,
                circle.rms_error,
                fast_line.lag_ms,
                fast_circle.lag_ms,
                fast_circle.rms_error,
                corner.overshoot,
            );
        }
    }
}
