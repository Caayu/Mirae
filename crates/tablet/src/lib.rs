//! Input acquisition and normalization.
//!
//! **Responsibility:** device backends (Windows Ink in [`windows`]), unit
//! normalization ([`normalize`]), the shared input clock ([`clock`]) and
//! timestamp repair ([`TimestampNormalizer`]).
//! **Does not:** know about brushes, canvas space, the view or windows
//! beyond the handle it attaches to.
//! **Depends on:** glam, log; on Windows windows-sys and raw-window-handle.
//! This is the only crate (besides one benchmark) with `unsafe`, confined
//! to FFI (docs/architecture/unsafe.md).
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

/// What produced a sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    /// Mouse or anything delivered as mouse input.
    Mouse,
    /// Pen tip.
    Pen,
    /// The pen's eraser end (or the pen inverted).
    Eraser,
    /// Finger (not used for painting yet).
    Touch,
}

/// Which optional measurements a sample really carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SampleCaps {
    /// `pressure` was measured.
    pub pressure: bool,
    /// `tilt_x`/`tilt_y` were measured.
    pub tilt: bool,
    /// `rotation` was measured.
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
    /// Tilt from vertical along screen x, radians in `-π/2..=π/2`; `0` if
    /// absent.
    pub tilt_x: f32,
    /// Tilt from vertical along screen y, radians in `-π/2..=π/2`; `0` if
    /// absent.
    pub tilt_y: f32,
    /// Barrel rotation, radians in `0..2π`; `0` if absent.
    pub rotation: f32,
    /// Seconds on [`clock::now`]'s clock.
    pub timestamp: f64,
    /// Producing device.
    pub device_kind: DeviceKind,
    /// Which optional fields are real measurements.
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
    /// Contact started.
    Down,
    /// Contact continues.
    Move,
    /// Contact ended normally (carries a final sample).
    Up,
    /// The system took the contact away (capture lost, ...); discard or end
    /// the stroke without a final sample.
    Cancel,
    /// Left proximity/the window.
    Leave,
}

/// A sample plus where it sits in its contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabletEvent {
    /// Contact phase.
    pub phase: Phase,
    /// Device pointer id (stable for one contact; 0 for the mouse).
    pub pointer_id: u32,
    /// The normalized sample.
    pub sample: TabletSample,
}
