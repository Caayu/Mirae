use glam::Vec2;

use crate::dynamics::InputCaps;

/// One pointer sample, already mapped to canvas pixel coordinates.
///
/// Backends (mouse, Windows Ink, ...) translate their native events into this
/// type; nothing downstream knows which device produced it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushInput {
    /// Canvas pixels, sub-pixel precise.
    pub position: Vec2,
    /// Normalized `0.0..=1.0`. Devices without pressure report 1.0.
    pub pressure: f32,
    /// Pen tilt from vertical along the canvas x/y axes, radians.
    pub tilt: Vec2,
    /// Barrel rotation, radians.
    pub rotation: f32,
    /// Seconds on a monotonic clock shared by all samples of a stroke.
    pub timestamp: f64,
    /// Which of the fields above the device really measured.
    pub caps: InputCaps,
}

impl BrushInput {
    /// Sample from a device that only knows position (e.g. a mouse).
    pub fn from_position(position: Vec2, timestamp: f64) -> Self {
        Self {
            position,
            pressure: 1.0,
            tilt: Vec2::ZERO,
            rotation: 0.0,
            timestamp,
            caps: InputCaps::NONE,
        }
    }

    pub fn with_pressure(mut self, pressure: f32) -> Self {
        self.pressure = pressure;
        self.caps.pressure = true;
        self
    }
}
