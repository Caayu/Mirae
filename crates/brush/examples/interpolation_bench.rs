//! Compares path interpolators on sparsely sampled, noise-free strokes.
//!
//! ```sh
//! cargo run --release -p tela-brush --example interpolation_bench
//! ```

use std::time::Instant;

use tela_brush::analysis::{Metrics, Scenario, max_turn_degrees};
use tela_brush::interpolate::{InterpolationKind, Interpolator, PathInterpolator};
use tela_brush::pipeline::StrokePoint;

fn sparse_corpus() -> Vec<(Scenario, f64)> {
    let by = |n: &str| {
        Scenario::corpus()
            .into_iter()
            .find(|s| s.name == n)
            .unwrap()
            .clean()
    };
    vec![
        (by("fast circle"), 60.0),
        (by("sine"), 40.0),
        (by("S"), 25.0),
        (by("fast line"), 20.0),
        (by("corner"), 30.0),
    ]
}

fn main() {
    println!(
        "| interpolator | scenario | samples | rms px | max px | max turn ° | overshoot px | ns/sample |"
    );
    println!("|---|---|---:|---:|---:|---:|---:|---:|");
    for kind in InterpolationKind::ALL {
        for (scenario, rate) in sparse_corpus() {
            let s = scenario.rate(rate);
            let input: Vec<StrokePoint> = s.samples().into_iter().map(Into::into).collect();
            let mut out = Vec::with_capacity(4096);
            let mut it = Interpolator::new(kind);
            const REPS: u32 = 200;
            let start = Instant::now();
            for _ in 0..REPS {
                out.clear();
                it.reset();
                for p in &input {
                    it.push(*p, 1.0, &mut out);
                }
                it.finish(1.0, &mut out);
            }
            let ns = start.elapsed().as_nanos() as f64 / (REPS as f64 * input.len() as f64);
            let m = Metrics::measure(&out, &s);
            let turn = if s.shape.corner().is_some() {
                f32::NAN
            } else {
                max_turn_degrees(out.iter().map(|p| p.pos), 0.5)
            };
            println!(
                "| {} | {} | {} | {:.2} | {:.2} | {:.1} | {:.2} | {:.0} |",
                kind.name(),
                s.name,
                input.len(),
                m.rms_error,
                m.max_error,
                turn,
                m.overshoot,
                ns
            );
        }
    }
}
