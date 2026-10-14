use glam::Vec2;

use crate::{BrushInput, BrushSettings};

/// One brush-tip impression.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub center: Vec2,
    pub radius: f32,
    /// Alpha contributed at full tip coverage.
    pub flow: f32,
    pub pressure: f32,
}

/// Turns a polyline of samples into evenly spaced dabs.
///
/// The distance still owed to the next dab is carried across segments, so
/// spacing does not depend on how densely the device reports samples.
#[derive(Clone, Debug, Default)]
pub struct DabSpacer {
    last: Option<BrushInput>,
    /// Arc length remaining on the path before the next dab is due.
    until_next: f32,
}

impl DabSpacer {
    /// Never step less than this, so a 1 px brush with tiny spacing cannot
    /// produce an unbounded number of dabs.
    pub const MIN_STEP: f32 = 0.5;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn step(settings: &BrushSettings, radius: f32) -> f32 {
        (2.0 * radius * settings.spacing).max(Self::MIN_STEP)
    }

    /// Feeds one sample and appends the dabs it produces to `out`.
    /// The first sample of a stroke always produces a dab.
    pub fn push(&mut self, sample: BrushInput, settings: &BrushSettings, out: &mut Vec<Dab>) {
        let make = |center: Vec2, pressure: f32| Dab {
            center,
            radius: settings.radius_at(pressure),
            flow: settings.flow,
            pressure,
        };

        let Some(prev) = self.last.replace(sample) else {
            let dab = make(sample.position, sample.pressure);
            self.until_next = Self::step(settings, dab.radius);
            out.push(dab);
            return;
        };

        let seg = sample.position - prev.position;
        let len = seg.length();
        if len <= f32::EPSILON {
            return;
        }

        let mut walked = 0.0;
        while walked + self.until_next <= len {
            walked += self.until_next;
            let t = walked / len;
            let pressure = prev.pressure + (sample.pressure - prev.pressure) * t;
            let dab = make(prev.position + seg * t, pressure);
            self.until_next = Self::step(settings, dab.radius);
            out.push(dab);
        }
        self.until_next -= len - walked;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(x: f32, y: f32) -> BrushInput {
        BrushInput::from_position(Vec2::new(x, y), 0.0)
    }

    fn settings(size: f32, spacing: f32) -> BrushSettings {
        BrushSettings {
            size,
            spacing,
            ..Default::default()
        }
    }

    #[test]
    fn first_sample_emits_one_dab() {
        let mut s = DabSpacer::new();
        let mut out = vec![];
        s.push(sample(3.0, 4.0), &settings(10.0, 0.5), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].center, Vec2::new(3.0, 4.0));
    }

    #[test]
    fn spacing_is_uniform_regardless_of_sample_density() {
        // 10 px brush, 50% spacing => a dab every 5 px along 100 px.
        let st = settings(10.0, 0.5);
        for samples in [2usize, 7, 33, 101] {
            let mut s = DabSpacer::new();
            let mut out = vec![];
            for i in 0..samples {
                let x = 100.0 * i as f32 / (samples - 1) as f32;
                s.push(sample(x, 0.0), &st, &mut out);
            }
            assert_eq!(out.len(), 21, "samples={samples}");
            for w in out.windows(2) {
                let d = w[1].center.x - w[0].center.x;
                assert!((d - 5.0).abs() < 1e-3, "gap {d} with {samples} samples");
            }
        }
    }

    #[test]
    fn repeated_position_emits_nothing() {
        let st = settings(10.0, 0.5);
        let mut s = DabSpacer::new();
        let mut out = vec![];
        s.push(sample(1.0, 1.0), &st, &mut out);
        s.push(sample(1.0, 1.0), &st, &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn pressure_is_interpolated_along_segment() {
        let st = settings(10.0, 0.5);
        let mut s = DabSpacer::new();
        let mut out = vec![];
        let mut a = sample(0.0, 0.0);
        a.pressure = 1.0;
        let mut b = sample(100.0, 0.0);
        b.pressure = 1.0;
        s.push(a, &st, &mut out);
        b.pressure = 0.0;
        s.push(b, &st, &mut out);
        // Pressure falls monotonically as the dabs advance.
        assert!(out.windows(2).all(|w| w[1].pressure <= w[0].pressure));
        assert!(out.len() > 2);
    }

    #[test]
    fn minimum_step_bounds_dab_count() {
        let st = settings(1.0, 0.01);
        let mut s = DabSpacer::new();
        let mut out = vec![];
        s.push(sample(0.0, 0.0), &st, &mut out);
        s.push(sample(10.0, 0.0), &st, &mut out);
        assert_eq!(out.len(), 1 + (10.0 / DabSpacer::MIN_STEP) as usize);
    }
}
