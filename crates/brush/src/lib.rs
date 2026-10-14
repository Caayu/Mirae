//! Brush engine. Independent of windowing, UI and GPU: it consumes
//! [`BrushInput`] samples in canvas coordinates and writes coverage into a
//! [`StrokeBuffer`], which is later committed onto a layer.
//!
//! Pipeline (stages marked * are pass-through for now):
//!
//! ```text
//! raw samples -> normalization* -> velocity* -> jitter filter* -> stabilization*
//!   -> path interpolation -> dab generation -> tip sampling -> color -> compositing
//!   -> dirty tiles
//! ```

pub mod dynamics;
mod engine;
mod input;
pub mod pipeline;
mod settings;
mod spacing;
pub mod stabilizer;
mod stroke;
mod tip;

pub use engine::BrushEngine;
pub use input::BrushInput;
pub use settings::BrushSettings;
pub use spacing::{Dab, DabSpacer};
pub use stroke::StrokeBuffer;
pub use tip::round_tip_coverage;
