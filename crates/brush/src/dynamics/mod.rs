//! Data-driven brush dynamics: every brush property is a base value modulated
//! by any number of effectors, each mapping one stroke input through an
//! editable [`Curve`].
//!
//! ```text
//! value = base * curve_1(input_1) * curve_2(input_2) * ...
//! ```
//!
//! Multiplication keeps effectors independent and order-free, and a neutral
//! effector (curve = 1 everywhere, or an input the device cannot produce)
//! leaves the value untouched.

mod curve;

pub use curve::Curve;

/// A device's global pressure calibration curve (applied before any brush
/// effector). Same representation as every other curve.
pub type PressureCurve = Curve;

/// Which inputs the device actually reported. Effectors fed by a missing
/// input are neutral, so a mouse or a pen without tilt keeps working with
/// any brush.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputCaps {
    pub pressure: bool,
    pub tilt: bool,
    pub rotation: bool,
}

impl InputCaps {
    pub const NONE: Self = Self {
        pressure: false,
        tilt: false,
        rotation: false,
    };
    pub const ALL: Self = Self {
        pressure: true,
        tilt: true,
        rotation: true,
    };
}

/// Stroke input a property can respond to. All values handed to curves are
/// normalized to `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DynamicsSource {
    Pressure,
    /// Screen-space speed, `1.0` at [`DynamicsInput::SPEED_FOR_FULL`].
    Speed,
    /// Pen inclination, `0` upright to `1` lying flat.
    TiltMagnitude,
    /// Azimuth of the tilt, one turn = `1`.
    TiltDirection,
    /// Direction of travel, one turn = `1`.
    Direction,
    /// Barrel rotation, one turn = `1`.
    Rotation,
    /// Distance travelled since the stroke start, `1.0` after `length`
    /// canvas pixels.
    Distance {
        length: f32,
    },
    /// Deterministic per-dab random value.
    Random,
}

/// Inputs available when evaluating dynamics for one dab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicsInput {
    pub caps: InputCaps,
    pub pressure: f32,
    /// Screen pixels per second.
    pub speed: f32,
    /// Radians from vertical along x/y.
    pub tilt_x: f32,
    pub tilt_y: f32,
    /// Radians.
    pub rotation: f32,
    /// Direction of travel, radians.
    pub direction: f32,
    /// Canvas pixels travelled since the stroke started.
    pub distance: f32,
    /// Uniform in `0..1`.
    pub random: f32,
}

impl Default for DynamicsInput {
    fn default() -> Self {
        Self {
            caps: InputCaps::NONE,
            pressure: 1.0,
            speed: 0.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            rotation: 0.0,
            direction: 0.0,
            distance: 0.0,
            random: 0.5,
        }
    }
}

fn turns(radians: f32) -> f32 {
    (radians / std::f32::consts::TAU).rem_euclid(1.0)
}

impl DynamicsInput {
    /// Speed mapped to `1.0` by the `Speed` source.
    pub const SPEED_FOR_FULL: f32 = 3000.0;

    /// Normalized value of `source`, or `None` if the device did not report it.
    pub fn value(&self, source: DynamicsSource) -> Option<f32> {
        let tilt = || {
            // Inclination from the two per-axis tilt angles.
            let (tx, ty) = (self.tilt_x.tan(), self.tilt_y.tan());
            (tx * tx + ty * ty).sqrt().atan()
        };
        Some(
            match source {
                DynamicsSource::Pressure if self.caps.pressure => self.pressure,
                DynamicsSource::Pressure => return None,
                DynamicsSource::Speed => self.speed / Self::SPEED_FOR_FULL,
                DynamicsSource::TiltMagnitude if self.caps.tilt => {
                    tilt() / std::f32::consts::FRAC_PI_2
                }
                DynamicsSource::TiltDirection if self.caps.tilt => {
                    turns(self.tilt_y.atan2(self.tilt_x))
                }
                DynamicsSource::TiltMagnitude | DynamicsSource::TiltDirection => return None,
                DynamicsSource::Direction => turns(self.direction),
                DynamicsSource::Rotation if self.caps.rotation => turns(self.rotation),
                DynamicsSource::Rotation => return None,
                DynamicsSource::Distance { length } => {
                    if length > 0.0 {
                        self.distance / length
                    } else {
                        1.0
                    }
                }
                DynamicsSource::Random => self.random,
            }
            .clamp(0.0, 1.0),
        )
    }
}

/// One input -> curve -> multiplier mapping.
#[derive(Clone, Debug, PartialEq)]
pub struct Effector {
    pub enabled: bool,
    pub source: DynamicsSource,
    pub curve: Curve,
}

impl Effector {
    pub fn new(source: DynamicsSource, curve: Curve) -> Self {
        Self {
            enabled: true,
            source,
            curve,
        }
    }

    /// Multiplier contributed for `input`; `1.0` when disabled or the source
    /// is unavailable.
    pub fn factor(&self, input: &DynamicsInput) -> f32 {
        if !self.enabled {
            return 1.0;
        }
        input.value(self.source).map_or(1.0, |v| self.curve.eval(v))
    }
}

/// A brush parameter: base value plus effectors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrushProperty {
    pub base: f32,
    pub effectors: Vec<Effector>,
}

impl BrushProperty {
    pub fn constant(base: f32) -> Self {
        Self {
            base,
            effectors: Vec::new(),
        }
    }

    pub fn with(mut self, effector: Effector) -> Self {
        self.effectors.push(effector);
        self
    }

    pub fn eval(&self, input: &DynamicsInput) -> f32 {
        self.effectors
            .iter()
            .fold(self.base, |v, e| v * e.factor(input))
    }

    /// The effector for `source`, if any.
    pub fn effector_mut(&mut self, source: DynamicsSource) -> Option<&mut Effector> {
        self.effectors.iter_mut().find(|e| e.source == source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pen(pressure: f32) -> DynamicsInput {
        DynamicsInput {
            caps: InputCaps::ALL,
            pressure,
            ..Default::default()
        }
    }

    #[test]
    fn pressure_effector_scales_base() {
        let size = BrushProperty::constant(20.0)
            .with(Effector::new(DynamicsSource::Pressure, Curve::linear()));
        assert!((size.eval(&pen(0.5)) - 10.0).abs() < 1e-5);
        assert!((size.eval(&pen(1.0)) - 20.0).abs() < 1e-5);
    }

    #[test]
    fn missing_input_is_neutral() {
        let size = BrushProperty::constant(20.0).with(Effector::new(
            DynamicsSource::Pressure,
            Curve::constant(0.1),
        ));
        let mouse = DynamicsInput::default();
        assert_eq!(size.eval(&mouse), 20.0);
        let tilt = BrushProperty::constant(1.0).with(Effector::new(
            DynamicsSource::TiltMagnitude,
            Curve::constant(0.0),
        ));
        let no_tilt = DynamicsInput {
            caps: InputCaps {
                tilt: false,
                ..InputCaps::ALL
            },
            ..pen(1.0)
        };
        assert_eq!(tilt.eval(&no_tilt), 1.0);
    }

    #[test]
    fn effectors_multiply_and_can_be_disabled() {
        let mut p = BrushProperty::constant(1.0)
            .with(Effector::new(DynamicsSource::Pressure, Curve::linear()))
            .with(Effector::new(DynamicsSource::Random, Curve::linear()));
        let mut input = pen(0.5);
        input.random = 0.5;
        assert!((p.eval(&input) - 0.25).abs() < 1e-6);
        p.effector_mut(DynamicsSource::Random).unwrap().enabled = false;
        assert!((p.eval(&input) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn normalized_sources() {
        let input = DynamicsInput {
            caps: InputCaps::ALL,
            speed: 1500.0,
            direction: std::f32::consts::PI,
            tilt_x: std::f32::consts::FRAC_PI_4,
            distance: 50.0,
            ..Default::default()
        };
        assert!((input.value(DynamicsSource::Speed).unwrap() - 0.5).abs() < 1e-6);
        assert!((input.value(DynamicsSource::Direction).unwrap() - 0.5).abs() < 1e-6);
        assert!((input.value(DynamicsSource::TiltMagnitude).unwrap() - 0.5).abs() < 1e-5);
        assert!(
            (input
                .value(DynamicsSource::Distance { length: 200.0 })
                .unwrap()
                - 0.25)
                .abs()
                < 1e-6
        );
        let neg = DynamicsInput {
            direction: -std::f32::consts::FRAC_PI_2,
            ..input
        };
        assert!((neg.value(DynamicsSource::Direction).unwrap() - 0.75).abs() < 1e-6);
    }
}
