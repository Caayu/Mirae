//! Time-based deposition (airbrush): dabs emitted at a fixed rate while the
//! pen is down, whether or not it moves.
//!
//! Emission instants are `t0 + k / rate`, fixed from the stroke start, so
//! the set of dabs never depends on how often the host calls in (30 or 144
//! frames per second give identical results). Each instant takes its
//! position by interpolating the processed path *in time*.
//!
//! When no samples arrive (a resting mouse sends none), the host advances
//! time explicitly. Deposition then continues at the last known position up
//! to `now - HOLD_DELAY`: the delay leaves room for late device samples, so
//! an instant is only ever resolved once its true position is known or the
//! pen has demonstrably stayed put.

use std::collections::VecDeque;

use crate::pipeline::StrokePoint;

#[derive(Clone, Debug, Default)]
pub struct TimeDeposit {
    t0: Option<f64>,
    next: u64,
    /// Recent path points with their arc length, oldest first.
    path: VecDeque<(StrokePoint, f32)>,
    /// Latest instant already resolved.
    horizon: f64,
}

impl TimeDeposit {
    /// How far behind "now" a resting pen's deposition runs, seconds.
    pub const HOLD_DELAY: f64 = 0.030;
    /// Bound on dabs per call, so a clock jump cannot stall the UI.
    pub const MAX_PER_CALL: u64 = 4000;

    pub fn reset(&mut self) {
        self.t0 = None;
        self.next = 0;
        self.path.clear();
        self.horizon = f64::NEG_INFINITY;
    }

    pub fn emitted(&self) -> u64 {
        self.next
    }

    /// Records a processed path point.
    pub fn record(&mut self, p: StrokePoint, distance: f32) {
        self.t0.get_or_insert(p.time);
        if self.path.back().is_some_and(|(b, _)| p.time < b.time) {
            return;
        }
        self.path.push_back((p, distance));
    }

    pub fn last_time(&self) -> Option<f64> {
        self.path.back().map(|(p, _)| p.time)
    }

    /// Horizon for a host clock reading `now`: real path data, or the hold
    /// position up to `now - HOLD_DELAY`, whichever reaches further.
    pub fn horizon_at(&self, now: f64) -> f64 {
        self.last_time()
            .unwrap_or(f64::NEG_INFINITY)
            .max(now - Self::HOLD_DELAY)
    }

    /// Emits every instant up to `horizon` through `emit(k, point, distance)`.
    pub fn emit(&mut self, rate: f32, horizon: f64, mut emit: impl FnMut(u64, &StrokePoint, f32)) {
        let (Some(t0), true) = (self.t0, rate > 0.0 && rate.is_finite()) else {
            return;
        };
        if self.path.is_empty() {
            return;
        }
        self.horizon = self.horizon.max(horizon);
        for _ in 0..Self::MAX_PER_CALL {
            let t = t0 + self.next as f64 / rate as f64;
            if t > self.horizon {
                break;
            }
            while self.path.len() >= 2 && self.path[1].0.time <= t {
                self.path.pop_front();
            }
            let (point, distance) = match (self.path.front(), self.path.get(1)) {
                (Some(&(a, da)), Some(&(b, db))) if t >= a.time => {
                    let span = b.time - a.time;
                    let f = if span > 0.0 {
                        ((t - a.time) / span) as f32
                    } else {
                        0.0
                    };
                    let mut p = a.lerp(&b, f);
                    p.time = t;
                    (p, da + (db - da) * f)
                }
                (Some(&(a, da)), _) => (StrokePoint { time: t, ..a }, da),
                (None, _) => return,
            };
            emit(self.next, &point, distance);
            self.next += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;
    use crate::BrushInput;

    fn point(x: f32, t: f64) -> StrokePoint {
        BrushInput::from_position(Vec2::new(x, 0.0), t).into()
    }

    fn hold(fps: f64, seconds: f64) -> Vec<(u64, f32)> {
        let mut d = TimeDeposit::default();
        d.reset();
        d.record(point(5.0, 0.0), 0.0);
        let mut out = vec![];
        let mut now = 0.0;
        while now <= seconds {
            let h = d.horizon_at(now);
            d.emit(100.0, h, |k, p, _| out.push((k, p.pos.x)));
            now += 1.0 / fps;
        }
        out
    }

    #[test]
    fn resting_pen_deposits_identically_at_any_frame_rate() {
        let a = hold(30.0, 1.0);
        let b = hold(144.0, 1.0);
        let c = hold(60.0, 1.0);
        // Equal up to the slowest host's last frame.
        let n = a.len().min(b.len()).min(c.len());
        assert!(n >= 95, "{n}");
        assert_eq!(a[..n], b[..n]);
        assert_eq!(a[..n], c[..n]);
        assert!(a.iter().all(|&(_, x)| x == 5.0));
    }

    #[test]
    fn moving_pen_uses_positions_interpolated_in_time() {
        let mut d = TimeDeposit::default();
        d.reset();
        for i in 0..=10 {
            d.record(point(i as f32 * 10.0, i as f64 * 0.01), i as f32 * 10.0);
        }
        let mut out = vec![];
        d.emit(400.0, 0.1, |k, p, dist| out.push((k, p.pos.x, dist)));
        assert_eq!(out.len(), 41);
        for (k, x, dist) in out {
            let expected = k as f32 / 400.0 * 1000.0;
            assert!((x - expected).abs() < 1e-3 && (dist - expected).abs() < 1e-3);
        }
    }

    #[test]
    fn zero_rate_and_clock_jumps_are_safe() {
        let mut d = TimeDeposit::default();
        d.reset();
        d.record(point(0.0, 0.0), 0.0);
        let mut n = 0;
        d.emit(0.0, 10.0, |_, _, _| n += 1);
        assert_eq!(n, 0);
        d.emit(1000.0, 1e9, |_, _, _| n += 1);
        assert_eq!(n as u64, TimeDeposit::MAX_PER_CALL);
    }
}
