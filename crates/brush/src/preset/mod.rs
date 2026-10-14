//! Brush presets: the complete, serializable, versioned description of a
//! brush. The engine reads presets directly; nothing here depends on UI.
//!
//! Format: JSON (see [`io`]). Every struct uses `#[serde(default)]`, so new
//! fields can be added without breaking older files, unknown fields are
//! ignored, and unknown enum variants load as neutral `Unknown` values.

pub mod builtin;
pub mod io;

pub use builtin::{builtin_preset, builtin_presets};
use serde::{Deserialize, Serialize};

use crate::dynamics::{BrushProperty, Curve, DynamicsInput, DynamicsSource, Effector, slot};
use crate::interpolate::InterpolationKind;
use crate::scatter::ScatterStyle;
use crate::stabilizer::StabilizerKind;
use crate::texture::TextureSettings;
use crate::tip::{Tip, TipImage, TipImageSource, TipShape};

/// Current preset format version.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// Every modulatable brush parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Properties {
    /// Major diameter, canvas pixels.
    pub size: BrushProperty,
    /// Alpha ceiling, `0..=1`.
    pub opacity: BrushProperty,
    /// Per-dab approach to the ceiling, `0..=1`.
    pub flow: BrushProperty,
    /// Step between dabs as a fraction of the tip's extent along the path.
    pub spacing: BrushProperty,
    pub hardness: BrushProperty,
    /// Tip rotation, degrees.
    pub angle: BrushProperty,
    /// Minor/major diameter ratio.
    pub aspect: BrushProperty,
    /// Maximum displacement as a fraction of the diameter.
    pub scatter: BrushProperty,
    pub texture_strength: BrushProperty,
}

impl Default for Properties {
    fn default() -> Self {
        Self {
            size: BrushProperty::constant(12.0),
            opacity: BrushProperty::constant(1.0),
            flow: BrushProperty::constant(1.0),
            spacing: BrushProperty::constant(0.1),
            hardness: BrushProperty::constant(0.9),
            angle: BrushProperty::constant(0.0),
            aspect: BrushProperty::constant(1.0),
            scatter: BrushProperty::constant(0.0),
            texture_strength: BrushProperty::constant(1.0),
        }
    }
}

/// Identity, valid range and storage of each parameter. UIs and validation
/// iterate over this instead of hard-coding fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Param {
    Size,
    Opacity,
    Flow,
    Spacing,
    Hardness,
    Angle,
    Aspect,
    Scatter,
    TextureStrength,
}

impl Param {
    pub const ALL: [Self; 9] = [
        Self::Size,
        Self::Opacity,
        Self::Flow,
        Self::Spacing,
        Self::Hardness,
        Self::Angle,
        Self::Aspect,
        Self::Scatter,
        Self::TextureStrength,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Opacity => "opacity",
            Self::Flow => "flow",
            Self::Spacing => "spacing",
            Self::Hardness => "hardness",
            Self::Angle => "angle",
            Self::Aspect => "aspect",
            Self::Scatter => "scatter",
            Self::TextureStrength => "texture strength",
        }
    }

    /// Valid values `(min, max)`.
    pub fn range(self) -> (f32, f32) {
        match self {
            Self::Size => (0.5, 5000.0),
            Self::Spacing => (0.01, 10.0),
            Self::Angle => (-360.0, 360.0),
            Self::Aspect => (0.01, 1.0),
            Self::Scatter => (0.0, 10.0),
            Self::Opacity | Self::Flow | Self::Hardness | Self::TextureStrength => (0.0, 1.0),
        }
    }

    pub fn slot(self) -> u32 {
        match self {
            Self::Size => slot::SIZE,
            Self::Opacity => slot::OPACITY,
            Self::Flow => slot::FLOW,
            Self::Spacing => slot::SPACING,
            Self::Hardness => slot::HARDNESS,
            Self::Angle => slot::ANGLE,
            Self::Aspect => slot::ASPECT,
            Self::Scatter => slot::SCATTER,
            Self::TextureStrength => slot::TEXTURE,
        }
    }

    pub fn get(self, p: &Properties) -> &BrushProperty {
        match self {
            Self::Size => &p.size,
            Self::Opacity => &p.opacity,
            Self::Flow => &p.flow,
            Self::Spacing => &p.spacing,
            Self::Hardness => &p.hardness,
            Self::Angle => &p.angle,
            Self::Aspect => &p.aspect,
            Self::Scatter => &p.scatter,
            Self::TextureStrength => &p.texture_strength,
        }
    }

    pub fn get_mut(self, p: &mut Properties) -> &mut BrushProperty {
        match self {
            Self::Size => &mut p.size,
            Self::Opacity => &mut p.opacity,
            Self::Flow => &mut p.flow,
            Self::Spacing => &mut p.spacing,
            Self::Hardness => &mut p.hardness,
            Self::Angle => &mut p.angle,
            Self::Aspect => &mut p.aspect,
            Self::Scatter => &mut p.scatter,
            Self::TextureStrength => &mut p.texture_strength,
        }
    }
}

impl Properties {
    /// Evaluates a scalar parameter and clamps it to its valid range.
    pub fn eval(&self, param: Param, input: &DynamicsInput) -> f32 {
        let (lo, hi) = param.range();
        param.get(self).eval(input, param.slot()).clamp(lo, hi)
    }
}

/// Narrowing of the stroke ends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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

/// Continuous deposition while the pen is down, independent of movement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Airbrush {
    pub enabled: bool,
    /// Dabs per second at the pen position.
    pub rate: f32,
}

impl Default for Airbrush {
    fn default() -> Self {
        Self {
            enabled: false,
            rate: 60.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaintMode {
    /// Paint color over the layer.
    #[default]
    Paint,
    /// Remove alpha from the layer (destination-out).
    Erase,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Compositing {
    pub mode: PaintMode,
    pub anti_aliasing: bool,
}

impl Default for Compositing {
    fn default() -> Self {
        Self {
            mode: PaintMode::Paint,
            anti_aliasing: true,
        }
    }
}

/// Stroke processing settings, resolved from user defaults and preset
/// overrides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrokeSettings {
    pub stabilizer: StabilizerKind,
    /// `0..=100`.
    pub stabilization: f32,
    pub interpolation: InterpolationKind,
    /// Screen pixels of movement ignored as sensor noise.
    pub jitter_threshold: f32,
}

impl Default for StrokeSettings {
    fn default() -> Self {
        Self {
            stabilizer: StabilizerKind::default(),
            stabilization: 20.0,
            interpolation: InterpolationKind::default(),
            jitter_threshold: 0.35,
        }
    }
}

/// Per-preset overrides of the user's stroke settings. `None` keeps the
/// user's value. Values from newer versions that this version does not
/// understand load as `None`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrokeOverrides {
    #[serde(
        deserialize_with = "io::lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub stabilizer: Option<StabilizerKind>,
    #[serde(
        deserialize_with = "io::lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub stabilization: Option<f32>,
    #[serde(
        deserialize_with = "io::lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub interpolation: Option<InterpolationKind>,
    #[serde(
        deserialize_with = "io::lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub jitter_threshold: Option<f32>,
}

impl StrokeOverrides {
    /// Applies a user edit to the stroke settings this preset resolves to.
    ///
    /// `edit` receives the resolved settings, i.e. what the user sees. Each
    /// field it changes becomes the user's own setting and the preset stops
    /// overriding it, so the next stroke uses exactly the edited value.
    /// Fields it leaves alone keep their override.
    pub fn edit_resolved(
        &mut self,
        user: &mut StrokeSettings,
        edit: impl FnOnce(&mut StrokeSettings),
    ) {
        let before = self.resolve(user);
        let mut after = before.clone();
        edit(&mut after);
        if after.stabilizer != before.stabilizer {
            user.stabilizer = after.stabilizer;
            self.stabilizer = None;
        }
        if after.stabilization != before.stabilization {
            user.stabilization = after.stabilization;
            self.stabilization = None;
        }
        if after.interpolation != before.interpolation {
            user.interpolation = after.interpolation;
            self.interpolation = None;
        }
        if after.jitter_threshold != before.jitter_threshold {
            user.jitter_threshold = after.jitter_threshold;
            self.jitter_threshold = None;
        }
    }

    pub fn resolve(&self, user: &StrokeSettings) -> StrokeSettings {
        StrokeSettings {
            stabilizer: self.stabilizer.unwrap_or(user.stabilizer),
            stabilization: self.stabilization.unwrap_or(user.stabilization),
            interpolation: self.interpolation.unwrap_or(user.interpolation),
            jitter_threshold: self.jitter_threshold.unwrap_or(user.jitter_threshold),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrushPreset {
    pub format_version: u32,
    /// Stable identifier, e.g. `tela.ink-pen`. Never reused for a
    /// different brush.
    pub id: String,
    pub name: String,
    pub metadata: Metadata,
    pub tip: Tip,
    pub properties: Properties,
    pub texture: Option<TextureSettings>,
    pub scatter: ScatterStyle,
    pub taper: Taper,
    pub airbrush: Airbrush,
    pub compositing: Compositing,
    pub stroke: StrokeOverrides,
}

impl Default for BrushPreset {
    fn default() -> Self {
        let pressure = |curve| Effector::new(DynamicsSource::Pressure, curve);
        let mut opacity_pressure = pressure(Curve::new(&[(0.0, 0.25), (1.0, 1.0)]));
        opacity_pressure.enabled = false;
        Self {
            format_version: FORMAT_VERSION,
            id: "tela.default".into(),
            name: "Default".into(),
            metadata: Metadata::default(),
            tip: Tip::default(),
            properties: Properties {
                size: BrushProperty::constant(12.0)
                    .with(pressure(Curve::new(&[(0.0, 0.08), (1.0, 1.0)]))),
                opacity: BrushProperty::constant(1.0).with(opacity_pressure),
                ..Properties::default()
            },
            texture: None,
            scatter: ScatterStyle::default(),
            taper: Taper::default(),
            airbrush: Airbrush::default(),
            compositing: Compositing::default(),
            stroke: StrokeOverrides::default(),
        }
    }
}

impl BrushPreset {
    /// Smallest radius a dab may have; thinner dabs alias into dotted lines.
    pub const MIN_RADIUS: f32 = 0.5;

    /// Constant-everything brush with no stroke filtering, for tests and
    /// tools.
    pub fn plain(size: f32) -> Self {
        Self {
            id: "tela.plain".into(),
            name: "Plain".into(),
            properties: Properties {
                size: BrushProperty::constant(size),
                ..Properties::default()
            },
            stroke: StrokeOverrides {
                stabilization: Some(0.0),
                jitter_threshold: Some(0.0),
                ..StrokeOverrides::default()
            },
            ..Self::default()
        }
    }

    pub fn radius(&self, input: &DynamicsInput, taper: f32) -> f32 {
        let t = if self.taper.affects_size { taper } else { 1.0 };
        (self.properties.eval(Param::Size, input) * 0.5 * t).max(Self::MIN_RADIUS)
    }

    pub fn opacity(&self, input: &DynamicsInput, taper: f32) -> f32 {
        let t = if self.taper.affects_opacity {
            taper
        } else {
            1.0
        };
        (self.properties.eval(Param::Opacity, input) * t).clamp(0.0, 1.0)
    }

    /// Brings every value into its valid range and replaces non-finite
    /// numbers. Returns a description of each change made.
    pub fn validate(&mut self) -> Vec<String> {
        let mut notes = Vec::new();
        if self.id.trim().is_empty() {
            self.id = format!("user.{}", self.name.trim().to_lowercase().replace(' ', "-"));
            notes.push(format!("missing id, using {:?}", self.id));
        }
        for param in Param::ALL {
            let (lo, hi) = param.range();
            let prop = param.get_mut(&mut self.properties);
            let fixed = if prop.base.is_finite() {
                prop.base.clamp(lo, hi)
            } else {
                param.get(&Properties::default()).base
            };
            if fixed != prop.base {
                notes.push(format!("{} base {} -> {}", param.name(), prop.base, fixed));
                prop.base = fixed;
            }
            if prop.effectors.len() > BrushProperty::MAX_EFFECTORS {
                notes.push(format!(
                    "{} has {} effectors; keeping the first {}",
                    param.name(),
                    prop.effectors.len(),
                    BrushProperty::MAX_EFFECTORS
                ));
                prop.effectors.truncate(BrushProperty::MAX_EFFECTORS);
            }
            for e in &mut prop.effectors {
                if !e.min.is_finite() || !e.max.is_finite() {
                    notes.push(format!("{} effector range was not finite", param.name()));
                    e.min = if e.min.is_finite() { e.min } else { 0.0 };
                    e.max = if e.max.is_finite() { e.max } else { 1.0 };
                }
            }
        }
        let mut finite = |v: &mut f32, lo: f32, hi: f32, default: f32, what: &str| {
            let fixed = if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            };
            if fixed != *v {
                notes.push(format!("{what} {v} -> {fixed}"));
                *v = fixed;
            }
        };
        finite(&mut self.taper.start, 0.0, 10_000.0, 0.0, "taper.start");
        finite(&mut self.taper.end, 0.0, 10_000.0, 0.0, "taper.end");
        finite(&mut self.taper.tip_scale, 0.0, 1.0, 0.05, "taper.tip_scale");
        finite(&mut self.airbrush.rate, 0.0, 2000.0, 60.0, "airbrush.rate");
        finite(
            &mut self.scatter.rotation_jitter,
            0.0,
            360.0,
            0.0,
            "scatter.rotation_jitter",
        );
        finite(
            &mut self.scatter.scale_jitter,
            0.0,
            1.0,
            0.0,
            "scatter.scale_jitter",
        );
        if let Some(t) = &mut self.texture {
            finite(&mut t.scale, 0.05, 64.0, 1.0, "texture.scale");
            finite(&mut t.rotation, -360.0, 360.0, 0.0, "texture.rotation");
            finite(&mut t.brightness, -1.0, 1.0, 0.0, "texture.brightness");
            finite(&mut t.contrast, 0.0, 4.0, 1.0, "texture.contrast");
        }
        if let Some(s) = &mut self.stroke.stabilization {
            finite(s, 0.0, 100.0, 20.0, "stroke.stabilization");
        }
        if let Some(j) = &mut self.stroke.jitter_threshold {
            finite(j, 0.0, 10.0, 0.35, "stroke.jitter_threshold");
        }
        let count = self.scatter.count.clamp(1, ScatterStyle::MAX_COUNT);
        if count != self.scatter.count {
            notes.push(format!("scatter.count {} -> {count}", self.scatter.count));
            self.scatter.count = count;
        }
        if let TipShape::Image {
            source: TipImageSource::Embedded { size, pixels },
        } = &self.tip.shape
            && TipImage::from_gray(*size, pixels).is_none()
        {
            notes.push("embedded tip image is invalid; using a round tip".into());
            self.tip.shape = TipShape::Round;
        }
        notes
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
        let s = BrushPreset::default();
        let pen = |p| DynamicsInput {
            caps: crate::dynamics::InputCaps::ALL,
            pressure: p,
            ..Default::default()
        };
        assert!(s.radius(&pen(0.2), 1.0) < s.radius(&pen(1.0), 1.0));
        assert_eq!(s.opacity(&pen(0.2), 1.0), 1.0);
        assert_eq!(s.radius(&DynamicsInput::default(), 1.0), 6.0);
    }

    #[test]
    fn evaluation_is_clamped_to_the_parameter_range() {
        let mut p = BrushPreset::plain(10.0);
        p.properties.opacity = BrushProperty::constant(3.0);
        assert_eq!(
            p.properties.eval(Param::Opacity, &DynamicsInput::default()),
            1.0
        );
    }

    #[test]
    fn validation_repairs_bad_values() {
        let mut p = BrushPreset::plain(10.0);
        p.id.clear();
        p.properties.size.base = f32::NAN;
        p.properties.spacing.base = -1.0;
        p.scatter.count = 0;
        p.taper.end = f32::INFINITY;
        p.tip.shape = TipShape::Image {
            source: TipImageSource::Embedded {
                size: 4,
                pixels: vec![1, 2],
            },
        };
        let notes = p.validate();
        assert_eq!(notes.len(), 6, "{notes:?}");
        assert_eq!(p.properties.size.base, 12.0);
        assert_eq!(p.properties.spacing.base, 0.01);
        assert_eq!(p.scatter.count, 1);
        assert_eq!(p.taper.end, 0.0);
        assert_eq!(p.tip.shape, TipShape::Round);
        assert!(p.id.starts_with("user."));
        assert!(p.validate().is_empty(), "validation is idempotent");
    }

    #[test]
    fn overrides_resolve_over_user_settings() {
        let user = StrokeSettings {
            stabilization: 40.0,
            ..Default::default()
        };
        let o = StrokeOverrides {
            interpolation: Some(InterpolationKind::Linear),
            ..Default::default()
        };
        let r = o.resolve(&user);
        assert_eq!(r.stabilization, 40.0);
        assert_eq!(r.interpolation, InterpolationKind::Linear);
    }

    #[test]
    fn editing_resolved_settings_drops_only_the_edited_overrides() {
        let mut user = StrokeSettings::default();
        let mut o = StrokeOverrides {
            stabilization: Some(0.0),
            interpolation: Some(InterpolationKind::Linear),
            ..StrokeOverrides::default()
        };
        // Edits start from what the user sees (the override), not from the
        // hidden user value.
        o.edit_resolved(&mut user, |s| s.stabilization += 10.0);
        assert_eq!(o.stabilization, None);
        assert_eq!(user.stabilization, 10.0);
        assert_eq!(o.resolve(&user).stabilization, 10.0);
        // Untouched overrides survive; an edit that changes nothing is a no-op.
        assert_eq!(o.interpolation, Some(InterpolationKind::Linear));
        let before = (o.clone(), user.clone());
        o.edit_resolved(&mut user, |_| {});
        assert_eq!((o, user), before);
    }
}
