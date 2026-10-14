//! Stroke stabilizers: causal filters that trade latency for smoothness.
//!
//! Stabilization *moves* samples to remove hand tremor and sensor noise. It is
//! a different job from interpolation, which only *connects* the (already
//! filtered) samples with a curve; see [`crate::interpolate`].
//!
//! All filters are time-based rather than sample-count based, so a 125 Hz
//! mouse and a 240 Hz pen behave the same. Position, pressure and tilt go
//! through the same weights, keeping pressure aligned with the (lagging)
//! position it belongs to.

use std::collections::VecDeque;

use glam::Vec2;

use crate::pipeline::StrokePoint;

/// A causal one-sample-in, one-sample-out filter.
pub trait StrokeStabilizer {
    fn reset(&mut self);
    /// `scale`: screen pixels per canvas pixel.
    fn push(&mut self, p: StrokePoint, scale: f32) -> StrokePoint;
}

/// Available algorithms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StabilizerKind {
    None,
    MovingAverage,
    Exponential,
    /// Default: chosen from the comparative benchmark (see `analysis`).
    #[default]
    OneEuro,
}

impl StabilizerKind {
    pub const ALL: [Self; 4] = [
        Self::None,
        Self::MovingAverage,
        Self::Exponential,
        Self::OneEuro,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MovingAverage => "moving-average",
            Self::Exponential => "exponential",
            Self::OneEuro => "one-euro",
        }
    }
}

/// User-facing strength `0..=100` mapped onto each algorithm's parameters.
///
/// The mappings are deliberately non-linear: the low end must only remove
/// micro-jitter (sub-pixel tremor) with no perceptible lag, the middle is tuned
/// for line art, and the top end trades a lot of lag for very smooth curves.
/// Both ends of every mapping are geometric because perceived smoothing is
/// roughly logarithmic in the filter's time constant.
pub fn stabilizer(kind: StabilizerKind, strength: f32) -> Stabilizer {
    let s = (strength / 100.0).clamp(0.0, 1.0);
    if s <= 0.0 {
        return Stabilizer::None(NoStabilizer);
    }
    // Geometric interpolation between `a` (at s≈0) and `b` (at s=1).
    let geo = |a: f32, b: f32| a * (b / a).powf(s);
    match kind {
        StabilizerKind::None => Stabilizer::None(NoStabilizer),
        StabilizerKind::MovingAverage => {
            Stabilizer::MovingAverage(MovingAverage::new(geo(0.004, 0.350) as f64))
        }
        StabilizerKind::Exponential => {
            Stabilizer::Exponential(Exponential::new(geo(0.0015, 0.140) as f64))
        }
        StabilizerKind::OneEuro => {
            Stabilizer::OneEuro(OneEuro::new(geo(40.0, 0.35), geo(0.05, 0.000_25), 8.0))
        }
    }
}

/// Enum dispatch over the algorithms: no boxing, no virtual call per sample.
#[derive(Clone, Debug)]
pub enum Stabilizer {
    None(NoStabilizer),
    MovingAverage(MovingAverage),
    Exponential(Exponential),
    OneEuro(OneEuro),
}

impl StrokeStabilizer for Stabilizer {
    fn reset(&mut self) {
        match self {
            Self::None(f) => f.reset(),
            Self::MovingAverage(f) => f.reset(),
            Self::Exponential(f) => f.reset(),
            Self::OneEuro(f) => f.reset(),
        }
    }

    fn push(&mut self, p: StrokePoint, scale: f32) -> StrokePoint {
        match self {
            Self::None(f) => f.push(p, scale),
            Self::MovingAverage(f) => f.push(p, scale),
            Self::Exponential(f) => f.push(p, scale),
            Self::OneEuro(f) => f.push(p, scale),
        }
    }
}

/// Blends the filtered attributes of `prev` toward `p` by `alpha`; time and
/// velocity stay those of the newest sample.
fn blend(prev: &StrokePoint, p: &StrokePoint, alpha: f32) -> StrokePoint {
    StrokePoint {
        pos: prev.pos + (p.pos - prev.pos) * alpha,
        pressure: prev.pressure + (p.pressure - prev.pressure) * alpha,
        tilt: prev.tilt + (p.tilt - prev.tilt) * alpha,
        ..*p
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoStabilizer;

impl StrokeStabilizer for NoStabilizer {
    fn reset(&mut self) {}
    fn push(&mut self, p: StrokePoint, _scale: f32) -> StrokePoint {
        p
    }
}

/// Mean of the samples inside a sliding time window.
#[derive(Clone, Debug)]
pub struct MovingAverage {
    /// Seconds.
    pub window: f64,
    buf: VecDeque<StrokePoint>,
}

impl MovingAverage {
    /// Bound on retained samples (a 1 kHz device over a 0.35 s window).
    const CAPACITY: usize = 512;

    pub fn new(window: f64) -> Self {
        Self {
            window,
            buf: VecDeque::with_capacity(Self::CAPACITY),
        }
    }
}

impl StrokeStabilizer for MovingAverage {
    fn reset(&mut self) {
        self.buf.clear();
    }

    fn push(&mut self, p: StrokePoint, _scale: f32) -> StrokePoint {
        if self.buf.len() == Self::CAPACITY {
            self.buf.pop_front();
        }
        self.buf.push_back(p);
        while self
            .buf
            .front()
            .is_some_and(|f| p.time - f.time > self.window)
        {
            self.buf.pop_front();
        }
        let n = self.buf.len() as f32;
        let (pos, pressure, tilt) = self
            .buf
            .iter()
            .fold((Vec2::ZERO, 0.0, Vec2::ZERO), |(a, b, c), s| {
                (a + s.pos, b + s.pressure, c + s.tilt)
            });
        StrokePoint {
            pos: pos / n,
            pressure: pressure / n,
            tilt: tilt / n,
            ..p
        }
    }
}

/// First-order low-pass with a fixed time constant.
#[derive(Clone, Debug)]
pub struct Exponential {
    /// Seconds.
    pub tau: f64,
    state: Option<StrokePoint>,
}

impl Exponential {
    pub fn new(tau: f64) -> Self {
        Self { tau, state: None }
    }
}

impl StrokeStabilizer for Exponential {
    fn reset(&mut self) {
        self.state = None;
    }

    fn push(&mut self, p: StrokePoint, _scale: f32) -> StrokePoint {
        let out = match &self.state {
            None => p,
            Some(prev) => {
                let dt = (p.time - prev.time).max(0.0);
                let alpha = 1.0 - (-dt / self.tau.max(1e-9)).exp();
                blend(prev, &p, alpha as f32)
            }
        };
        self.state = Some(out);
        out
    }
}

/// The "1€ filter" (Casiez, Roussel & Vogel, CHI 2012): a first-order
/// low-pass whose cutoff rises with speed. Slow, precise motion is heavily
/// smoothed (where jitter is visible) while fast motion gets a high cutoff
/// (where lag is what the user would notice).
#[derive(Clone, Debug)]
pub struct OneEuro {
    /// Cutoff at rest, Hz.
    pub min_cutoff: f32,
    /// Cutoff increase per screen pixel per second.
    pub beta: f32,
    /// Cutoff for the speed estimate itself, Hz.
    pub d_cutoff: f32,
    state: Option<StrokePoint>,
    /// Filtered speed, screen pixels per second.
    speed: f32,
}

impl OneEuro {
    pub fn new(min_cutoff: f32, beta: f32, d_cutoff: f32) -> Self {
        Self {
            min_cutoff,
            beta,
            d_cutoff,
            state: None,
            speed: 0.0,
        }
    }

    fn alpha(cutoff: f32, dt: f32) -> f32 {
        let tau = 1.0 / (std::f32::consts::TAU * cutoff);
        1.0 / (1.0 + tau / dt)
    }
}

impl StrokeStabilizer for OneEuro {
    fn reset(&mut self) {
        self.state = None;
        self.speed = 0.0;
    }

    fn push(&mut self, p: StrokePoint, scale: f32) -> StrokePoint {
        let Some(prev) = self.state else {
            self.state = Some(p);
            return p;
        };
        let dt = (p.time - prev.time) as f32;
        if dt <= 0.0 {
            // Same instant: keep the filter state, adopt the newest pressure.
            return StrokePoint { pos: prev.pos, ..p };
        }
        let raw_speed = (p.pos - prev.pos).length() * scale / dt;
        self.speed += (raw_speed - self.speed) * Self::alpha(self.d_cutoff, dt);
        let cutoff = self.min_cutoff + self.beta * self.speed;
        let out = blend(&prev, &p, Self::alpha(cutoff, dt));
        self.state = Some(out);
        out
    }
}

/// Runs `last` (the final raw sample) through the stabilizer repeatedly with
/// advancing time until the output converges on it, so a stroke ends where
/// the pen was lifted instead of where the lagging filter happened to be.
pub fn catch_up(
    stabilizer: &mut impl StrokeStabilizer,
    last: StrokePoint,
    scale: f32,
    out: &mut Vec<StrokePoint>,
) {
    const STEP: f64 = 1.0 / 240.0;
    const MAX_STEPS: usize = 240;
    let tolerance = 0.05 / scale.max(1e-6);
    let mut p = last;
    for _ in 0..MAX_STEPS {
        p.time += STEP;
        let s = stabilizer.push(p, scale);
        out.push(s);
        if (s.pos - last.pos).length() < tolerance {
            break;
        }
    }
    if let Some(end) = out.last_mut() {
        end.pos = last.pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrushInput;

    fn point(x: f32, y: f32, t: f64) -> StrokePoint {
        BrushInput::from_position(Vec2::new(x, y), t).into()
    }

    #[test]
    fn strength_zero_is_identity_for_every_kind() {
        for kind in StabilizerKind::ALL {
            let mut s = stabilizer(kind, 0.0);
            for i in 0..50 {
                let p = point(i as f32 * 3.0, (i as f32).sin(), i as f64 * 0.005);
                assert_eq!(s.push(p, 1.0).pos, p.pos, "{kind:?}");
            }
        }
    }

    #[test]
    fn filters_converge_on_a_resting_pen() {
        for kind in StabilizerKind::ALL {
            let mut s = stabilizer(kind, 100.0);
            s.push(point(0.0, 0.0, 0.0), 1.0);
            let mut last = Vec2::ZERO;
            for i in 1..2000 {
                last = s.push(point(100.0, 50.0, i as f64 * 0.005), 1.0).pos;
            }
            assert!(
                (last - Vec2::new(100.0, 50.0)).length() < 0.01,
                "{kind:?}: {last}"
            );
        }
    }

    #[test]
    fn output_lies_between_input_extremes() {
        // Low-pass filters of this family never overshoot a step.
        for kind in StabilizerKind::ALL {
            let mut s = stabilizer(kind, 60.0);
            s.push(point(0.0, 0.0, 0.0), 1.0);
            for i in 1..400 {
                let x = s.push(point(100.0, 0.0, i as f64 * 0.004), 1.0).pos.x;
                assert!((0.0..=100.0 + 1e-3).contains(&x), "{kind:?} overshoot {x}");
            }
        }
    }

    #[test]
    fn pressure_is_filtered_with_position() {
        let mut s = stabilizer(StabilizerKind::Exponential, 80.0);
        let mut a = point(0.0, 0.0, 0.0);
        a.pressure = 0.0;
        s.push(a, 1.0);
        let mut b = point(10.0, 0.0, 0.004);
        b.pressure = 1.0;
        let out = s.push(b, 1.0);
        let t = out.pos.x / 10.0;
        assert!((out.pressure - t).abs() < 1e-5);
    }

    #[test]
    fn one_euro_follows_fast_motion_more_closely_than_slow() {
        let lag_at = |speed: f32| {
            let mut s = stabilizer(StabilizerKind::OneEuro, 50.0);
            let mut out = Vec2::ZERO;
            for i in 0..200 {
                let t = i as f64 * 0.005;
                out = s.push(point(speed * t as f32, 0.0, t), 1.0).pos;
            }
            (speed * 199.0 * 0.005 - out.x) / speed
        };
        // Lag expressed in seconds shrinks as the pen speeds up.
        assert!(lag_at(2000.0) < lag_at(50.0) * 0.5);
    }

    #[test]
    fn catch_up_ends_on_the_last_raw_sample() {
        for kind in StabilizerKind::ALL {
            let mut s = stabilizer(kind, 100.0);
            let mut last = point(0.0, 0.0, 0.0);
            for i in 0..100 {
                last = point(i as f32 * 5.0, 0.0, i as f64 * 0.005);
                s.push(last, 1.0);
            }
            let mut out = vec![];
            catch_up(&mut s, last, 1.0, &mut out);
            assert_eq!(out.last().unwrap().pos, last.pos, "{kind:?}");
            // Converges monotonically toward the end point.
            assert!(
                out.windows(2).all(|w| w[1].pos.x >= w[0].pos.x - 1e-3),
                "{kind:?}"
            );
        }
    }
}
