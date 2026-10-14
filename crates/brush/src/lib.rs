//! Brush engine. Independent of windowing, UI, GPU and input devices: it
//! consumes [`BrushInput`] samples in canvas coordinates, evaluates a
//! [`BrushPreset`], and emits dabs into a [`DabSink`] (normally a
//! [`StrokeBuffer`], later committed onto a layer).
//!
//! ```text
//! BrushInput -> pressure curve -> velocity -> jitter filter -> stabilizer
//!   -> interpolator -> dab spacing -> dynamics, taper & scatter
//!   -> tip x texture rasterization -> stroke buffer -> dirty tiles
//! ```
//!
//! Acquisition, device normalization and timestamp normalization happen
//! before this crate (see `tela-tablet`).

pub mod analysis;
pub mod dynamics;
mod engine;
mod input;
pub mod interpolate;
pub mod pipeline;
pub mod preset;
pub mod rng;
pub mod scatter;
mod spacing;
pub mod stabilizer;
mod stroke;
pub mod texture;
pub mod tip;

pub use engine::{BrushEngine, StrokeParams, StrokeProcessor, StrokeStats, resolve_dab};
pub use input::BrushInput;
pub use preset::{BrushPreset, Param, Properties, StrokeSettings, Taper};
pub use spacing::{Dab, DabSite, DabSpacer};
pub use stroke::{DabLog, DabSink, StampContext, StrokeBuffer};
