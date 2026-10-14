use tela_color::Rgba;

use crate::dynamics::{BrushProperty, Curve, DynamicsInput, DynamicsSource, Effector};
use crate::interpolate::InterpolationKind;
use crate::stabilizer::StabilizerKind;

/// Narrowing of the stroke ends.
#[derive(Clone, Debug, PartialEq)]
pub struct Taper {
    /// Canvas pixels over which the stroke grows to full size; 0 disables.
    pub start: f32,
    /// Canvas pixels over which the stroke shrinks at its end; 0 disables.
    pub end: f32,
    /// Scale at the very tip, `0..=1`.
    pub tip_scale: f32,
    pub affects_size: bool,
    pub affects_opacity: bool,
}

impl Default for Taper {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 0.0,
            tip_scale: 0.05,
            affects_size: true,
            affects_opacity: false,
        }
    }
}

impl Taper {
    /// Multiplier at `from_start` canvas pixels after the start and
    /// `to_end` before the end (`f32::INFINITY` if the end is unknown).
    ///
    /// The ramp is ease-out `t(2 - t)`: the width grows quickly off the tip
    /// and then settles, which reads as a natural pen taper rather than the
    /// cone a linear ramp produces.
    pub fn factor(&self, from_start: f32, to_end: f32) -> f32 {
        let ramp = |d: f32, len: f32| {
            if len <= 0.0 {
                return 1.0;
            }
            let t = (d / len).clamp(0.0, 1.0);
            let e = t * (2.0 - t);
            self.tip_scale + (1.0 - self.tip_scale) * e
        };
        ramp(from_start, self.start).min(ramp(to_end, self.end))
    }
}

/// Every parameter of a brush. Properties are data-driven: a base value
/// plus effectors (see [`crate::dynamics`]).
#[derive(Clone, Debug, PartialEq)]
pub struct BrushSettings {
    pub color: Rgba,
    /// Diameter in canvas pixels.
    pub size: BrushProperty,
    /// Ceiling of the stroke's alpha, `0..=1`.
    pub opacity: BrushProperty,
    /// Alpha each dab contributes toward the ceiling, `0..=1`.
    pub flow: BrushProperty,
    /// Fraction of the radius that is fully opaque, `0..=1`.
    pub hardness: BrushProperty,
    /// Distance between dabs as a fraction of the current diameter.
    pub spacing: f32,
    pub anti_aliasing: bool,
    pub taper: Taper,
    pub stabilizer: StabilizerKind,
    /// User-facing `0..=100`.
    pub stabilization: f32,
    pub interpolation: InterpolationKind,
    /// Screen pixels of movement ignored as sensor noise.
    pub jitter_threshold: f32,
}

impl Default for BrushSettings {
    fn default() -> Self {
        let pressure = |curve| Effector::new(DynamicsSource::Pressure, curve);
        let mut opacity_pressure = pressure(Curve::new(&[(0.0, 0.25), (1.0, 1.0)]));
        opacity_pressure.enabled = false;
        Self {
            color: Rgba::BLACK,
            size: BrushProperty::constant(12.0)
                .with(pressure(Curve::new(&[(0.0, 0.08), (1.0, 1.0)]))),
            opacity: BrushProperty::constant(1.0).with(opacity_pressure),
            flow: BrushProperty::constant(1.0),
            hardness: BrushProperty::constant(0.9),
            spacing: 0.1,
            anti_aliasing: true,
            taper: Taper::default(),
            stabilizer: StabilizerKind::default(),
            stabilization: 20.0,
            interpolation: InterpolationKind::default(),
            jitter_threshold: 0.35,
        }
    }
}

impl BrushSettings {
    /// Smallest radius a dab may have; thinner dabs alias into dotted lines.
    pub const MIN_RADIUS: f32 = 0.5;

    /// Constant-everything brush, handy for tests and tools.
    pub fn plain(size: f32) -> Self {
        Self {
            size: BrushProperty::constant(size),
            opacity: BrushProperty::constant(1.0),
            stabilization: 0.0,
            jitter_threshold: 0.0,
            ..Default::default()
        }
    }

    pub fn radius(&self, input: &DynamicsInput, taper: f32) -> f32 {
        let t = if self.taper.affects_size { taper } else { 1.0 };
        (self.size.eval(input) * 0.5 * t).max(Self::MIN_RADIUS)
    }

    pub fn opacity(&self, input: &DynamicsInput, taper: f32) -> f32 {
        let t = if self.taper.affects_opacity {
            taper
        } else {
            1.0
        };
        (self.opacity.eval(input) * t).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taper_ramps_from_tip_to_full() {
        let t = Taper {
            start: 100.0,
            end: 50.0,
            tip_scale: 0.0,
            ..Default::default()
        };
        assert_eq!(t.factor(0.0, f32::INFINITY), 0.0);
        assert_eq!(t.factor(100.0, f32::INFINITY), 1.0);
        assert_eq!(t.factor(500.0, 0.0), 0.0);
        assert!((t.factor(500.0, 25.0) - 0.75).abs() < 1e-6);
        // Monotone along the ramp.
        let mut prev = 0.0;
        for i in 0..=100 {
            let f = t.factor(i as f32, f32::INFINITY);
            assert!(f >= prev);
            prev = f;
        }
    }

    #[test]
    fn disabled_taper_is_neutral() {
        assert_eq!(Taper::default().factor(0.0, 0.0), 1.0);
    }

    #[test]
    fn default_brush_responds_to_pressure_in_size_only() {
        let s = BrushSettings::default();
        let pen = |p| DynamicsInput {
            caps: crate::dynamics::InputCaps::ALL,
            pressure: p,
            ..Default::default()
        };
        assert!(s.radius(&pen(0.2), 1.0) < s.radius(&pen(1.0), 1.0));
        assert_eq!(s.opacity(&pen(0.2), 1.0), 1.0);
        // A mouse gets the full size.
        assert_eq!(s.radius(&DynamicsInput::default(), 1.0), 6.0);
    }
}
