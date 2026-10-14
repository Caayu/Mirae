//! Input acquisition and normalization.
//!
//! Backends turn device events into the neutral [`TabletSample`]; nothing
//! downstream knows whether a sample came from a mouse, Windows Ink or a
//! future backend. This crate stops at normalization: positions are window
//! client pixels, and mapping into canvas space is the application's job.
//!
//! ```text
//! raw device event -> normalize units (normalize) -> clock (clock)
//!   -> TimestampNormalizer -> TabletSample
//! ```

pub mod clock;
pub mod normalize;
#[cfg(windows)]
#[allow(unsafe_code)]
pub mod windows;

use glam::Vec2;

pub use clock::TimestampNormalizer;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Mouse,
    Pen,
    /// The pen's eraser end (or the pen inverted).
    Eraser,
    Touch,
}

/// Which optional measurements a sample really carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SampleCaps {
    pub pressure: bool,
    pub tilt: bool,
    pub rotation: bool,
}

/// One normalized input sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletSample {
    /// Window client-area physical pixels, sub-pixel precise when the
    /// device allows.
    pub position: Vec2,
    /// `0.0..=1.0`; `1.0` when the device has no pressure sensor.
    pub pressure: f32,
    /// Tilt from vertical along x/y, radians in `-π/2..=π/2`; `0` if absent.
    pub tilt_x: f32,
    pub tilt_y: f32,
    /// Barrel rotation, radians in `0..2π`; `0` if absent.
    pub rotation: f32,
    /// Seconds on [`clock::now`]'s clock.
    pub timestamp: f64,
    pub device_kind: DeviceKind,
    pub caps: SampleCaps,
}

impl TabletSample {
    /// A position-only sample (mouse).
    pub fn mouse(position: Vec2, timestamp: f64) -> Self {
        Self {
            position,
            pressure: 1.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            rotation: 0.0,
            timestamp,
            device_kind: DeviceKind::Mouse,
            caps: SampleCaps::default(),
        }
    }
}

/// Where a sample sits in a contact's life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// In range but not touching.
    Hover,
    Down,
    Move,
    Up,
    /// The system took the contact away (capture lost, ...); discard or end
    /// the stroke without a final sample.
    Cancel,
    /// Left proximity/the window.
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletEvent {
    pub phase: Phase,
    pub pointer_id: u32,
    pub sample: TabletSample,
}
