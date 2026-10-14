use glam::Vec2;

use super::StrokePoint;

/// Estimates pen velocity with a time-constant exponential average of the
/// finite differences.
///
/// Raw differences are noisy (positions are quantized and timestamps jitter
/// by a millisecond), so they are smoothed with `alpha = 1 - exp(-dt/tau)`,
/// which keeps the estimate independent of the device's sample rate.
#[derive(Clone, Debug)]
pub struct VelocityEstimator {
    /// Smoothing time constant, seconds.
    pub tau: f64,
    last: Option<(Vec2, f64)>,
    velocity: Vec2,
}

impl Default for VelocityEstimator {
    fn default() -> Self {
        Self::new(0.012)
    }
}

impl VelocityEstimator {
    /// Shortest interval trusted for a finite difference.
    const MIN_DT: f64 = 0.000_5;

    pub fn new(tau: f64) -> Self {
        Self {
            tau,
            last: None,
            velocity: Vec2::ZERO,
        }
    }

    pub fn reset(&mut self) {
        self.last = None;
        self.velocity = Vec2::ZERO;
    }

    pub fn process(&mut self, p: &mut StrokePoint) {
        if let Some((last_pos, last_t)) = self.last {
            let dt = (p.time - last_t).max(Self::MIN_DT);
            let inst = (p.pos - last_pos) / dt as f32;
            let alpha = if self.tau > 0.0 {
                1.0 - (-dt / self.tau).exp()
            } else {
                1.0
            };
            self.velocity += (inst - self.velocity) * alpha as f32;
        }
        self.last = Some((p.pos, p.time));
        p.velocity = self.velocity;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrushInput;

    fn point(x: f32, t: f64) -> StrokePoint {
        BrushInput::from_position(Vec2::new(x, 0.0), t).into()
    }

    #[test]
    fn converges_to_constant_velocity_at_any_rate() {
        for rate in [60.0, 133.0, 240.0, 1000.0] {
            let mut v = VelocityEstimator::default();
            let mut p = point(0.0, 0.0);
            for i in 0..(rate as usize / 4) {
                let t = i as f64 / rate;
                p = point(500.0 * t as f32, t);
                v.process(&mut p);
            }
            assert!(
                (p.velocity.x - 500.0).abs() < 1.0,
                "rate {rate}: {}",
                p.velocity.x
            );
        }
    }

    #[test]
    fn first_sample_has_zero_velocity_and_reset_forgets() {
        let mut v = VelocityEstimator::default();
        let mut a = point(0.0, 0.0);
        v.process(&mut a);
        assert_eq!(a.velocity, Vec2::ZERO);
        let mut b = point(10.0, 0.01);
        v.process(&mut b);
        assert!(b.velocity.x > 0.0);
        v.reset();
        let mut c = point(50.0, 1.0);
        v.process(&mut c);
        assert_eq!(c.velocity, Vec2::ZERO);
    }

    #[test]
    fn duplicate_timestamps_do_not_explode() {
        let mut v = VelocityEstimator::default();
        let mut a = point(0.0, 0.5);
        v.process(&mut a);
        let mut b = point(1.0, 0.5);
        v.process(&mut b);
        assert!(b.velocity.x.is_finite() && b.velocity.x <= 2000.0);
    }
}
