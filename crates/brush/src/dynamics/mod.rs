//! Data-driven brush dynamics.
//!
//! Every brush property is a base value transformed by an ordered list of
//! effectors. An effector reads one stroke input, maps it through an editable
//! [`Curve`] onto `[min, max]`, and combines the result with the running
//! value:
//!
//! ```text
//! value = base
//! for each effector: value = combine(value, min + (max - min) * curve(input))
//! ```
//!
//! An effector whose input the device cannot produce (pressure on a mouse,
//! tilt on a basic pen) is skipped, so every brush works on every device.

mod curve;

pub use curve::Curve;
use serde::{Deserialize, Serialize};

use crate::rng::{StrokeRng, stream};

/// Stable numeric identity of each property. It seeds the property's random
/// streams, so it must never change once presets exist.
pub mod slot {
    pub const SIZE: u32 = 0;
    pub const OPACITY: u32 = 1;
    pub const FLOW: u32 = 2;
    pub const SPACING: u32 = 3;
    pub const HARDNESS: u32 = 4;
    pub const ANGLE: u32 = 5;
    pub const ASPECT: u32 = 6;
    pub const SCATTER: u32 = 7;
    pub const TEXTURE: u32 = 8;
}

/// A device's global pressure calibration curve (applied before any brush
/// effector). Same representation as every other curve.
pub type PressureCurve = Curve;

/// Which inputs the device actually reported.
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

/// Stroke input an effector can read. Every value handed to a curve is
/// normalized to `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DynamicsSource {
    Pressure,
    /// Screen-space speed, `1.0` at [`DynamicsInput::SPEED_FOR_FULL`].
    Speed,
    /// Pen inclination, `0` upright to `1` lying flat.
    TiltMagnitude,
    /// Azimuth of the tilt, one turn = `1`.
    TiltDirection,
    /// Barrel rotation, one turn = `1`.
    Rotation,
    /// Direction of travel, one turn = `1`.
    Direction,
    /// Distance travelled since the stroke start, `1.0` after `length`
    /// canvas pixels (stroke progress over a known length).
    Distance {
        length: f32,
    },
    /// Dab number modulo `period`, as a fraction of the period.
    DabIndex {
        period: u32,
    },
    /// Independent uniform value per dab.
    Random,
    /// Smooth value noise along the travelled distance; `frequency` is in
    /// lattice points per canvas pixel.
    Noise {
        frequency: f32,
    },
    /// Placeholder for sources added by newer versions; always neutral.
    #[serde(other)]
    Unknown,
}

impl DynamicsSource {
    /// Sources whose natural value is an angle; they can drive
    /// [`CombineMode::Follow`].
    pub fn is_angular(self) -> bool {
        matches!(self, Self::TiltDirection | Self::Rotation | Self::Direction)
    }
}

/// How an effector's value merges into the running value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombineMode {
    #[default]
    Multiply,
    Add,
    Replace,
    Min,
    Max,
    /// Angles only: rotate the running angle toward the source's angle by
    /// the effector value (`0` = ignore, `1` = follow exactly), along the
    /// shortest arc. Linear blending of angles would jump at the 0°/360° wrap.
    Follow,
    #[serde(other)]
    Unknown,
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
    pub dab_index: u64,
    pub rng: StrokeRng,
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
            dab_index: 0,
            rng: StrokeRng::default(),
        }
    }
}

fn turns(radians: f32) -> f32 {
    (radians / std::f32::consts::TAU).rem_euclid(1.0)
}

impl DynamicsInput {
    /// Speed mapped to `1.0` by the `Speed` source.
    pub const SPEED_FOR_FULL: f32 = 3000.0;
    /// Screen pixels per second below which direction is undefined.
    pub const MIN_DIRECTION_SPEED: f32 = 1.0;

    /// Whether the pen is moving fast enough for its direction to mean
    /// anything; at rest `direction` is undefined and direction-driven
    /// effectors stay neutral.
    pub fn has_direction(&self) -> bool {
        self.speed > Self::MIN_DIRECTION_SPEED
    }

    fn tilt_azimuth(&self) -> f32 {
        self.tilt_y.atan2(self.tilt_x)
    }

    /// Normalized value of `source`, or `None` if the device did not
    /// report it. `stream` selects the random sequence for random sources.
    pub fn value(&self, source: DynamicsSource, stream: u32) -> Option<f32> {
        let v = match source {
            DynamicsSource::Pressure => self.caps.pressure.then_some(self.pressure)?,
            DynamicsSource::Speed => self.speed / Self::SPEED_FOR_FULL,
            DynamicsSource::TiltMagnitude => {
                if !self.caps.tilt {
                    return None;
                }
                let (tx, ty) = (self.tilt_x.tan(), self.tilt_y.tan());
                (tx * tx + ty * ty).sqrt().atan() / std::f32::consts::FRAC_PI_2
            }
            DynamicsSource::TiltDirection => self.caps.tilt.then(|| turns(self.tilt_azimuth()))?,
            DynamicsSource::Rotation => self.caps.rotation.then(|| turns(self.rotation))?,
            DynamicsSource::Direction => self.has_direction().then(|| turns(self.direction))?,
            DynamicsSource::Distance { length } => {
                if length > 0.0 {
                    self.distance / length
                } else {
                    1.0
                }
            }
            DynamicsSource::DabIndex { period } => {
                let p = period.max(1) as u64;
                (self.dab_index % p) as f32 / p as f32
            }
            DynamicsSource::Random => self.rng.uniform(stream, self.dab_index),
            DynamicsSource::Noise { frequency } => {
                self.rng.noise(stream, self.distance * frequency.max(0.0))
            }
            DynamicsSource::Unknown => return None,
        };
        Some(if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        })
    }

    /// The angle (radians) an angular source points to.
    pub fn angle(&self, source: DynamicsSource) -> Option<f32> {
        match source {
            DynamicsSource::Direction => self.has_direction().then_some(self.direction),
            DynamicsSource::TiltDirection => self.caps.tilt.then(|| self.tilt_azimuth()),
            DynamicsSource::Rotation => self.caps.rotation.then_some(self.rotation),
            _ => None,
        }
    }
}

/// One input -> curve -> `[min, max]` -> combine step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effector {
    pub enabled: bool,
    pub source: DynamicsSource,
    pub curve: Curve,
    pub mode: CombineMode,
    pub min: f32,
    pub max: f32,
}

impl Default for Effector {
    fn default() -> Self {
        Self {
            enabled: true,
            source: DynamicsSource::Pressure,
            curve: Curve::linear(),
            mode: CombineMode::Multiply,
            min: 0.0,
            max: 1.0,
        }
    }
}

impl Effector {
    /// Multiplier in `[0, 1]` driven by `source` through `curve`.
    pub fn new(source: DynamicsSource, curve: Curve) -> Self {
        Self {
            source,
            curve,
            ..Default::default()
        }
    }

    pub fn with_mode(mut self, mode: CombineMode, min: f32, max: f32) -> Self {
        self.mode = mode;
        self.min = min;
        self.max = max;
        self
    }

    fn output(&self, input: &DynamicsInput, stream: u32) -> Option<f32> {
        let x = input.value(self.source, stream)?;
        Some(self.min + (self.max - self.min) * self.curve.eval(x))
    }
}

/// Rotates angle `from` toward `to` (degrees) by `amount` in `0..=1`,
/// for angles with the given symmetry `period`.
///
/// Blends unit vectors in the "multiplied angle" space (`angle * 360 /
/// period`), where the symmetry becomes a full turn, then normalizes. This
/// is continuous in both angles everywhere except the degenerate case of
/// exactly opposite angles at `amount = 0.5`. Scaling the shortest
/// difference instead would jump by `amount * period` whenever the target
/// crosses the point opposite to `from`.
pub fn follow(from: f32, to: f32, amount: f32, period: f32) -> f32 {
    let k = std::f32::consts::TAU / period;
    let (a, b) = (from * k, to * k);
    let x = (1.0 - amount) * a.cos() + amount * b.cos();
    let y = (1.0 - amount) * a.sin() + amount * b.sin();
    if x * x + y * y < 1e-12 {
        return from;
    }
    from + angle_delta(from, y.atan2(x) / k, period)
}

/// Shortest signed difference `b - a` for angles of the given period.
pub fn angle_delta(a: f32, b: f32, period: f32) -> f32 {
    let d = (b - a).rem_euclid(period);
    if d > period * 0.5 { d - period } else { d }
}

/// A brush parameter: base value plus effectors.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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

    fn stream(slot: u32, i: usize) -> u32 {
        stream::EFFECTORS + slot * 64 + i as u32
    }

    /// Evaluates a scalar property. `slot` identifies the property so each
    /// random effector gets an independent stream.
    pub fn eval(&self, input: &DynamicsInput, slot: u32) -> f32 {
        let mut acc = self.base;
        for (i, e) in self.effectors.iter().enumerate().filter(|(_, e)| e.enabled) {
            let Some(v) = e.output(input, Self::stream(slot, i)) else {
                continue;
            };
            acc = match e.mode {
                CombineMode::Multiply => acc * v,
                CombineMode::Add => acc + v,
                CombineMode::Replace | CombineMode::Follow => v,
                CombineMode::Min => acc.min(v),
                CombineMode::Max => acc.max(v),
                CombineMode::Unknown => acc,
            };
        }
        if acc.is_finite() { acc } else { self.base }
    }

    /// Evaluates an angle property in degrees. `period` is the tip's
    /// rotational symmetry (360 for asymmetric tips, 180 for ellipses).
    pub fn eval_angle(&self, input: &DynamicsInput, slot: u32, period: f32) -> f32 {
        let mut acc = self.base;
        for (i, e) in self.effectors.iter().enumerate().filter(|(_, e)| e.enabled) {
            if e.mode == CombineMode::Follow {
                let Some(target) = input.angle(e.source) else {
                    continue;
                };
                acc = follow(acc, target.to_degrees(), e.max.clamp(0.0, 1.0), period);
                continue;
            }
            let Some(v) = e.output(input, Self::stream(slot, i)) else {
                continue;
            };
            acc = match e.mode {
                CombineMode::Multiply => acc * v,
                CombineMode::Add => acc + v,
                CombineMode::Replace => v,
                CombineMode::Min => acc.min(v),
                CombineMode::Max => acc.max(v),
                CombineMode::Follow | CombineMode::Unknown => acc,
            };
        }
        if acc.is_finite() {
            acc.rem_euclid(360.0)
        } else {
            self.base
        }
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
        assert!((size.eval(&pen(0.5), 0) - 10.0).abs() < 1e-5);
        assert!((size.eval(&pen(1.0), 0) - 20.0).abs() < 1e-5);
    }

    #[test]
    fn missing_input_is_neutral() {
        let size = BrushProperty::constant(20.0).with(Effector::new(
            DynamicsSource::Pressure,
            Curve::constant(0.1),
        ));
        assert_eq!(size.eval(&DynamicsInput::default(), 0), 20.0);
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
        assert_eq!(tilt.eval(&no_tilt, 0), 1.0);
    }

    #[test]
    fn combine_modes() {
        let input = pen(0.5);
        let with = |mode, min, max| {
            BrushProperty::constant(10.0)
                .with(
                    Effector::new(DynamicsSource::Pressure, Curve::linear())
                        .with_mode(mode, min, max),
                )
                .eval(&input, 0)
        };
        // Effector value at pressure 0.5 over [0, 4] is 2.
        assert_eq!(with(CombineMode::Multiply, 0.0, 4.0), 20.0);
        assert_eq!(with(CombineMode::Add, 0.0, 4.0), 12.0);
        assert_eq!(with(CombineMode::Replace, 0.0, 4.0), 2.0);
        assert_eq!(with(CombineMode::Min, 0.0, 4.0), 2.0);
        assert_eq!(with(CombineMode::Max, 0.0, 40.0), 20.0);
        assert_eq!(with(CombineMode::Unknown, 0.0, 4.0), 10.0);
    }

    #[test]
    fn effectors_apply_in_order_and_can_be_disabled() {
        let mut p = BrushProperty::constant(1.0)
            .with(Effector::new(DynamicsSource::Pressure, Curve::linear()))
            .with(
                Effector::new(DynamicsSource::Pressure, Curve::linear()).with_mode(
                    CombineMode::Add,
                    0.0,
                    1.0,
                ),
            );
        assert!((p.eval(&pen(0.5), 0) - 1.0).abs() < 1e-6);
        p.effectors[0].enabled = false;
        assert!((p.eval(&pen(0.5), 0) - 1.5).abs() < 1e-6);
    }

    #[test]
    fn follow_rotates_along_the_shortest_arc() {
        let follow = |amount| {
            BrushProperty::constant(350.0).with(
                Effector::new(DynamicsSource::Direction, Curve::linear()).with_mode(
                    CombineMode::Follow,
                    0.0,
                    amount,
                ),
            )
        };
        let input = DynamicsInput {
            direction: 10f32.to_radians(),
            speed: 100.0,
            ..Default::default()
        };
        // Halfway from 350° to 10° is 0°, not 180°.
        let half = follow(0.5).eval_angle(&input, 0, 360.0);
        assert!(half.min(360.0 - half) < 1e-3, "{half}");
        assert!((follow(1.0).eval_angle(&input, 0, 360.0) - 10.0).abs() < 1e-3);
        assert!((follow(0.0).eval_angle(&input, 0, 360.0) - 350.0).abs() < 1e-3);
        // With 180° symmetry, travelling at 190° equals travelling at 10°.
        let back = DynamicsInput {
            direction: 190f32.to_radians(),
            speed: 100.0,
            ..Default::default()
        };
        assert!((follow(1.0).eval_angle(&back, 0, 180.0) - 10.0).abs() < 1e-3);
    }

    #[test]
    fn follow_is_continuous_where_the_target_crosses_the_opposite_angle() {
        // Base 35°, ellipse symmetry 180°: the opposite angle is 125°.
        let mut prev = follow(35.0, 120.0, 0.12, 180.0);
        for i in 1..=100 {
            let target = 120.0 + i as f32 * 0.1;
            let v = follow(35.0, target, 0.12, 180.0);
            assert!(
                angle_delta(prev, v, 180.0).abs() < 0.2,
                "jump at {target}: {prev} -> {v}"
            );
            prev = v;
        }
        assert!((follow(10.0, 50.0, 1.0, 360.0) - 50.0).abs() < 1e-3);
        assert!((follow(10.0, 50.0, 0.0, 360.0) - 10.0).abs() < 1e-3);
    }

    #[test]
    fn direction_is_undefined_at_rest() {
        let p = BrushProperty::constant(30.0).with(
            Effector::new(DynamicsSource::Direction, Curve::linear()).with_mode(
                CombineMode::Follow,
                0.0,
                1.0,
            ),
        );
        let rest = DynamicsInput::default();
        assert_eq!(p.eval_angle(&rest, 0, 360.0), 30.0);
        let moving = DynamicsInput {
            speed: 500.0,
            direction: 1.0,
            ..Default::default()
        };
        assert!((p.eval_angle(&moving, 0, 360.0) - 1f32.to_degrees()).abs() < 1e-3);
    }

    #[test]
    fn random_effectors_are_deterministic_and_per_property() {
        let p = BrushProperty::constant(1.0)
            .with(Effector::new(DynamicsSource::Random, Curve::linear()));
        let input = |i| DynamicsInput {
            dab_index: i,
            rng: StrokeRng::new(77),
            ..Default::default()
        };
        let a: Vec<f32> = (0..50).map(|i| p.eval(&input(i), 0)).collect();
        let b: Vec<f32> = (0..50).map(|i| p.eval(&input(i), 0)).collect();
        let other: Vec<f32> = (0..50).map(|i| p.eval(&input(i), 1)).collect();
        assert_eq!(a, b);
        assert_ne!(a, other);
    }

    #[test]
    fn normalized_sources() {
        let input = DynamicsInput {
            caps: InputCaps::ALL,
            speed: 1500.0,
            direction: std::f32::consts::PI,
            tilt_x: std::f32::consts::FRAC_PI_4,
            distance: 50.0,
            dab_index: 13,
            ..Default::default()
        };
        let v = |s| input.value(s, 0).unwrap();
        assert!((v(DynamicsSource::Speed) - 0.5).abs() < 1e-6);
        assert!((v(DynamicsSource::Direction) - 0.5).abs() < 1e-6);
        assert!((v(DynamicsSource::TiltMagnitude) - 0.5).abs() < 1e-5);
        assert!((v(DynamicsSource::Distance { length: 200.0 }) - 0.25).abs() < 1e-6);
        assert!((v(DynamicsSource::DabIndex { period: 4 }) - 0.25).abs() < 1e-6);
        assert!(input.value(DynamicsSource::Unknown, 0).is_none());
    }

    #[test]
    fn unknown_sources_and_modes_deserialize() {
        let e: Effector = serde_json::from_str(
            r#"{"source":{"kind":"brain_waves"},"mode":"telepathy","curve":[[0,0],[1,1]],"future_field":3}"#,
        )
        .unwrap();
        assert_eq!(e.source, DynamicsSource::Unknown);
        assert_eq!(e.mode, CombineMode::Unknown);
        let p = BrushProperty::constant(5.0).with(e);
        assert_eq!(p.eval(&pen(0.3), 0), 5.0);
    }
}
