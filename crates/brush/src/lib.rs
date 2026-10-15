//! Brush engine. Independent of windowing, UI, GPU and input devices: it
//! consumes [`BrushInput`] samples in canvas coordinates, evaluates a
//! [`BrushPreset`], and emits dabs into a [`DabSink`] (normally a
//! [`StrokeBuffer`], later committed onto a layer).
//!
//! **Responsibility:** presets (versioned JSON, [`preset`]), effectors and
//! curves ([`dynamics`]), per-sample filtering ([`pipeline`],
//! [`stabilizer`]), path interpolation ([`interpolate`]), dab spacing,
//! tips ([`tip`]), paper textures ([`texture`]), [`scatter`], airbrush
//! timing, deterministic randomness ([`rng`]) and rasterization into the
//! stroke mask ([`StrokeBuffer`]).
//! **Does not:** read devices, know about windows/UI/GPU, own layers or
//! history. It never sees the document: the caller commits the finished
//! buffer.
//! **Depends on:** color, canvas, glam, serde/serde_json (presets), rayon
//! (per-tile parallel stamping).
//!
//! Key invariants (docs/architecture/invariants.md): same preset + seed +
//! samples give bit-identical dabs; dab placement does not depend on sample
//! density; preview equals the committed result; parallel stamping is
//! bit-identical to sequential; airbrush output does not depend on frame
//! rate; invalid presets never panic; inputs a device lacks are neutral; no
//! allocation per dab on the hot path.
//!
//! ```text
//! BrushInput -> pressure curve -> velocity -> jitter filter -> stabilizer
//!   -> interpolator -> dab spacing -> dynamics, taper & scatter
//!   -> tip x texture rasterization -> stroke buffer -> dirty tiles
//! ```
//!
//! Acquisition, device normalization and timestamp normalization happen
//! before this crate (see `tela-tablet`).

mod airbrush;
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

pub use engine::{BrushEngine, StrokeParams, StrokeProcessor, StrokeStats};
pub use input::BrushInput;
pub use preset::{BrushPreset, Param, Properties, StrokeSettings, Taper};
pub use spacing::Dab;
pub use stroke::{DabLog, DabSink, PixelAlpha, StampContext, StrokeBuffer};
