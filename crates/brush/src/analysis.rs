//! Reproducible synthetic strokes and quality metrics, used by the comparative
//! tests and by `examples/stroke_bench.rs` to choose default algorithms on
//! evidence rather than on the look of a single drawing.
//!
//! A scenario is a known ground-truth path traversed in time, sampled like a
//! device would (fixed rate with timing jitter) and corrupted with sensor
//! noise plus physiological tremor. Outputs are scored against the truth.

use glam::Vec2;

use crate::BrushInput;
use crate::dynamics::InputCaps;
use crate::pipeline::StrokePoint;

/// Stateful SplitMix64 for generating synthetic input (sensor noise,
/// tremor, timing jitter): tiny, fast, and identical on every platform,
/// which is what a reproducible test corpus needs. Not used while drawing;
/// strokes use the stateless [`crate::rng::StrokeRng`].
#[derive(Clone, Debug)]
pub struct SyntheticRng(u64);

impl SyntheticRng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Standard normal (Box–Muller).
    pub fn gaussian(&mut self) -> f64 {
        let u1 = self.uniform().max(1e-12);
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

/// Ground-truth path, parametrized by `u` in `[0, 1]`.
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Line {
        from: Vec2,
        to: Vec2,
    },
    Circle {
        center: Vec2,
        radius: f32,
    },
    /// Horizontal sine wave.
    Sine {
        origin: Vec2,
        length: f32,
        amplitude: f32,
        periods: f32,
    },
    /// One vertical sine period: an "S".
    S {
        origin: Vec2,
        height: f32,
        amplitude: f32,
    },
    /// Sharp direction change at `corner`.
    Corner {
        from: Vec2,
        corner: Vec2,
        to: Vec2,
    },
}

impl Shape {
    pub fn at(&self, u: f32) -> Vec2 {
        use std::f32::consts::TAU;
        match *self {
            Shape::Line { from, to } => from.lerp(to, u),
            Shape::Circle { center, radius } => {
                center + radius * Vec2::new((TAU * u).cos(), (TAU * u).sin())
            }
            Shape::Sine {
                origin,
                length,
                amplitude,
                periods,
            } => origin + Vec2::new(length * u, amplitude * (TAU * periods * u).sin()),
            Shape::S {
                origin,
                height,
                amplitude,
            } => origin + Vec2::new(amplitude * (TAU * u).sin(), height * u),
            Shape::Corner { from, corner, to } => {
                // Constant speed along both legs.
                let (l1, l2) = ((corner - from).length(), (to - corner).length());
                let s = u * (l1 + l2);
                if s <= l1 {
                    from.lerp(corner, s / l1)
                } else {
                    corner.lerp(to, (s - l1) / l2)
                }
            }
        }
    }

    pub fn corner(&self) -> Option<(Vec2, Vec2)> {
        match *self {
            Shape::Corner { from, corner, .. } => Some((corner, (corner - from).normalize())),
            _ => None,
        }
    }
}

/// How a scenario is sampled and corrupted.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: &'static str,
    pub shape: Shape,
    /// Seconds to traverse the shape.
    pub duration: f64,
    /// Samples per second.
    pub rate: f64,
    /// Standard deviation of white sensor noise, screen pixels.
    pub noise: f32,
    /// Amplitude of a 9 Hz tremor, screen pixels.
    pub tremor: f32,
    /// Standard deviation of sample timing, seconds.
    pub timing_jitter: f64,
    pub seed: u64,
}

impl Scenario {
    pub fn new(name: &'static str, shape: Shape, duration: f64) -> Self {
        Self {
            name,
            shape,
            duration,
            rate: 200.0,
            noise: 0.4,
            tremor: 0.5,
            timing_jitter: 0.000_5,
            seed: 0x7E1A,
        }
    }

    pub fn clean(mut self) -> Self {
        self.noise = 0.0;
        self.tremor = 0.0;
        self.timing_jitter = 0.0;
        self
    }

    pub fn rate(mut self, rate: f64) -> Self {
        self.rate = rate;
        self
    }

    /// The standard corpus covering slow/fast, straight/curved and corners.
    pub fn corpus() -> Vec<Scenario> {
        let c = Vec2::new(500.0, 400.0);
        vec![
            Scenario::new(
                "slow line",
                Shape::Line {
                    from: Vec2::new(100.0, 300.0),
                    to: Vec2::new(400.0, 340.0),
                },
                2.5,
            ),
            Scenario::new(
                "fast line",
                Shape::Line {
                    from: Vec2::new(100.0, 300.0),
                    to: Vec2::new(1000.0, 500.0),
                },
                0.3,
            ),
            Scenario::new(
                "circle",
                Shape::Circle {
                    center: c,
                    radius: 150.0,
                },
                1.2,
            ),
            Scenario::new(
                "fast circle",
                Shape::Circle {
                    center: c,
                    radius: 150.0,
                },
                0.3,
            ),
            Scenario::new(
                "sine",
                Shape::Sine {
                    origin: Vec2::new(100.0, 400.0),
                    length: 800.0,
                    amplitude: 120.0,
                    periods: 2.0,
                },
                1.5,
            ),
            Scenario::new(
                "S",
                Shape::S {
                    origin: Vec2::new(500.0, 100.0),
                    height: 600.0,
                    amplitude: 150.0,
                },
                0.9,
            ),
            Scenario::new(
                "corner",
                Shape::Corner {
                    from: Vec2::new(100.0, 600.0),
                    corner: Vec2::new(400.0, 100.0),
                    to: Vec2::new(520.0, 600.0),
                },
                1.0,
            ),
        ]
    }

    /// Dense ground truth as `(time, position)`.
    pub fn truth(&self) -> Vec<(f64, Vec2)> {
        const N: usize = 4000;
        (0..=N)
            .map(|i| {
                let u = i as f32 / N as f32;
                (u as f64 * self.duration, self.shape.at(u))
            })
            .collect()
    }

    /// Device samples (mouse-like `caps`, pressure fixed at 1).
    pub fn samples(&self) -> Vec<BrushInput> {
        let mut rng = SyntheticRng::new(self.seed);
        let n = (self.duration * self.rate).round().max(1.0) as usize;
        let tremor_phase = rng.uniform() * std::f64::consts::TAU;
        (0..=n)
            .map(|i| {
                let ideal_t = i as f64 / self.rate;
                let t = if i == 0 || i == n {
                    ideal_t
                } else {
                    ideal_t + rng.gaussian() * self.timing_jitter
                };
                let u = (t / self.duration).clamp(0.0, 1.0) as f32;
                let tremor = self.tremor
                    * Vec2::new(
                        (std::f64::consts::TAU * 9.0 * t + tremor_phase).sin() as f32,
                        (std::f64::consts::TAU * 9.0 * t + tremor_phase * 1.7).cos() as f32,
                    );
                let noise = self.noise * Vec2::new(rng.gaussian() as f32, rng.gaussian() as f32);
                let mut input = BrushInput::from_position(self.shape.at(u) + tremor + noise, t);
                input.caps = InputCaps::NONE;
                input
            })
            .collect()
    }
}

/// Quality of a processed stroke against its ground truth.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    /// RMS of the high-frequency part of the error, pixels.
    pub jitter: f32,
    /// RMS distance to the true path, pixels.
    pub rms_error: f32,
    /// Largest distance to the true path, pixels.
    pub max_error: f32,
    /// Mean delay behind the true pen position, milliseconds.
    pub lag_ms: f32,
    /// Distance travelled past a sharp corner, pixels.
    pub overshoot: f32,
}

fn nearest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> (Vec2, f32) {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t, t)
}

/// Nearest point on the truth polyline: `(point, time)`.
fn nearest_truth(p: Vec2, truth: &[(f64, Vec2)]) -> (Vec2, f64) {
    let mut best = (truth[0].1, truth[0].0, f32::INFINITY);
    for w in truth.windows(2) {
        let (q, t) = nearest_on_segment(p, w[0].1, w[1].1);
        let d = (q - p).length_squared();
        if d < best.2 {
            best = (q, w[0].0 + (w[1].0 - w[0].0) * t as f64, d);
        }
    }
    (best.0, best.1)
}

impl Metrics {
    /// Scores `output` positions (with their timestamps) against `scenario`.
    pub fn measure(output: &[StrokePoint], scenario: &Scenario) -> Self {
        Self::measure_points(output.iter().map(|p| (p.time, p.pos)), scenario)
    }

    pub fn measure_points(
        output: impl IntoIterator<Item = (f64, Vec2)>,
        scenario: &Scenario,
    ) -> Self {
        let truth = scenario.truth();
        let out: Vec<(f64, Vec2)> = output.into_iter().collect();
        if out.is_empty() {
            return Self::default();
        }
        let near: Vec<(Vec2, f64)> = out.iter().map(|&(_, p)| nearest_truth(p, &truth)).collect();
        let err: Vec<Vec2> = out
            .iter()
            .zip(&near)
            .map(|((_, p), (q, _))| *p - *q)
            .collect();

        let n = err.len() as f32;
        let rms_error = (err.iter().map(|e| e.length_squared()).sum::<f32>() / n).sqrt();
        let max_error = err.iter().map(|e| e.length()).fold(0.0, f32::max);

        const K: usize = 5;
        let mut hf = 0.0;
        for i in 0..err.len() {
            let (lo, hi) = (i.saturating_sub(K), (i + K + 1).min(err.len()));
            let mean = err[lo..hi].iter().copied().sum::<Vec2>() / (hi - lo) as f32;
            hf += (err[i] - mean).length_squared();
        }
        let jitter = (hf / n).sqrt();

        // Lag over the middle of the stroke, away from start/end transients
        // and from the turnaround of closed shapes.
        let (t0, t1) = (scenario.duration * 0.1, scenario.duration * 0.9);
        let lags: Vec<f64> = out
            .iter()
            .zip(&near)
            .filter(|((t, _), _)| *t > t0 && *t < t1)
            .map(|((t, _), (_, tt))| t - tt)
            .collect();
        let lag_ms = if lags.is_empty() {
            0.0
        } else {
            (lags.iter().sum::<f64>() / lags.len() as f64 * 1000.0) as f32
        };

        let overshoot = scenario.shape.corner().map_or(0.0, |(c, dir)| {
            out.iter()
                .map(|(_, p)| (*p - c).dot(dir))
                .fold(0.0, f32::max)
        });

        Self {
            jitter,
            rms_error,
            max_error,
            lag_ms,
            overshoot,
        }
    }
}

/// Largest change of direction, in degrees, between consecutive chords of a
/// dense polyline: the size of the worst visible "kink". Chords shorter than
/// `min_chord` are merged so quantization does not count as a kink.
pub fn max_turn_degrees(points: impl IntoIterator<Item = Vec2>, min_chord: f32) -> f32 {
    let mut kept: Vec<Vec2> = Vec::new();
    for p in points {
        if kept.last().is_none_or(|l| (p - *l).length() >= min_chord) {
            kept.push(p);
        }
    }
    kept.windows(3)
        .map(|w| {
            let (a, b) = ((w[1] - w[0]).normalize(), (w[2] - w[1]).normalize());
            a.dot(b).clamp(-1.0, 1.0).acos().to_degrees()
        })
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_turn_measures_kinks() {
        let square = [
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
        ];
        assert!((max_turn_degrees(square, 0.5) - 90.0).abs() < 1e-3);
        let straight = (0..10).map(|i| Vec2::new(i as f32, 0.0));
        assert_eq!(max_turn_degrees(straight, 0.5), 0.0);
    }

    #[test]
    fn rng_is_deterministic_and_roughly_normal() {
        let mut a = SyntheticRng::new(42);
        let mut b = SyntheticRng::new(42);
        assert_eq!(a.next_u64(), b.next_u64());
        let n = 20_000;
        let xs: Vec<f64> = (0..n).map(|_| a.gaussian()).collect();
        let mean = xs.iter().sum::<f64>() / n as f64;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        assert!(
            mean.abs() < 0.03 && (var - 1.0).abs() < 0.05,
            "{mean} {var}"
        );
    }

    #[test]
    fn clean_samples_score_zero() {
        for s in Scenario::corpus() {
            let s = s.clean();
            let pts: Vec<StrokePoint> = s.samples().into_iter().map(Into::into).collect();
            let m = Metrics::measure(&pts, &s);
            assert!(
                m.rms_error < 0.01 && m.jitter < 0.01 && m.lag_ms.abs() < 0.5,
                "{}: {m:?}",
                s.name
            );
            assert!(m.overshoot < 0.01);
        }
    }

    #[test]
    fn noise_is_measured_as_jitter() {
        let s = &Scenario::corpus()[0];
        let pts: Vec<StrokePoint> = s.samples().into_iter().map(Into::into).collect();
        let m = Metrics::measure(&pts, s);
        assert!(m.jitter > 0.2 && m.rms_error > m.jitter, "{m:?}");
    }

    #[test]
    fn delayed_output_is_measured_as_lag() {
        let s = Scenario::corpus()[1].clone().clean();
        // Re-stamp each sample 20 ms late.
        let pts = s
            .samples()
            .into_iter()
            .map(|i| (i.timestamp + 0.020, i.position));
        let m = Metrics::measure_points(pts, &s);
        assert!((m.lag_ms - 20.0).abs() < 1.0, "{m:?}");
    }
}
