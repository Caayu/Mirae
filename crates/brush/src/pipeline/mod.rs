//! Per-sample processing stages between raw input and path interpolation.
//!
//! Every stage is a small state machine that works on [`StrokePoint`]s, can be
//! reset between strokes, writes into caller-owned buffers (no allocation per
//! sample) and is unit tested on its own.
//!
//! Distances and speeds that express *hand or device* behaviour are measured
//! in screen pixels: each stage receives `scale` (screen pixels per canvas
//! pixel, i.e. the view zoom), so the filters feel identical at any zoom.

mod jitter;
mod velocity;

pub use jitter::JitterFilter;
pub use velocity::VelocityEstimator;

use glam::Vec2;

use crate::BrushInput;
use crate::dynamics::InputCaps;

/// A sample travelling through the pipeline, in canvas space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokePoint {
    pub pos: Vec2,
    pub pressure: f32,
    pub tilt: Vec2,
    pub rotation: f32,
    /// Seconds.
    pub time: f64,
    /// Canvas pixels per second; filled in by [`VelocityEstimator`].
    pub velocity: Vec2,
    pub caps: InputCaps,
}

impl From<BrushInput> for StrokePoint {
    fn from(i: BrushInput) -> Self {
        Self {
            pos: i.position,
            pressure: i.pressure.clamp(0.0, 1.0),
            tilt: i.tilt,
            rotation: i.rotation,
            time: i.timestamp,
            velocity: Vec2::ZERO,
            caps: i.caps,
        }
    }
}

impl StrokePoint {
    /// Linear blend of every continuous attribute. Rotation is blended along
    /// the shortest arc.
    pub fn lerp(&self, o: &Self, t: f32) -> Self {
        let mut d_rot = (o.rotation - self.rotation).rem_euclid(std::f32::consts::TAU);
        if d_rot > std::f32::consts::PI {
            d_rot -= std::f32::consts::TAU;
        }
        Self {
            pos: self.pos.lerp(o.pos, t),
            pressure: self.pressure + (o.pressure - self.pressure) * t,
            tilt: self.tilt.lerp(o.tilt, t),
            rotation: self.rotation + d_rot * t,
            time: self.time + (o.time - self.time) * t as f64,
            velocity: self.velocity.lerp(o.velocity, t),
            caps: self.caps,
        }
    }
}
