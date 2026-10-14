use tela_color::Rgba;

/// Phase-1 brush parameters. These become data-driven properties with
/// effector curves in a later phase.
#[derive(Clone, Debug, PartialEq)]
pub struct BrushSettings {
    pub color: Rgba,
    /// Diameter in canvas pixels at full pressure.
    pub size: f32,
    /// Ceiling of the whole stroke's alpha, `0..=1`.
    pub opacity: f32,
    /// Alpha each dab contributes, `0..=1`.
    pub flow: f32,
    /// Distance between dabs as a fraction of the current diameter.
    pub spacing: f32,
    /// Fraction of the radius that is fully opaque, `0..=1`.
    pub hardness: f32,
    pub anti_aliasing: bool,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            color: Rgba::BLACK,
            size: 12.0,
            opacity: 1.0,
            flow: 1.0,
            spacing: 0.1,
            hardness: 0.9,
            anti_aliasing: true,
        }
    }
}

impl BrushSettings {
    /// Smallest radius a dab may have; thinner dabs alias into dotted lines.
    pub const MIN_RADIUS: f32 = 0.5;

    pub fn radius_at(&self, pressure: f32) -> f32 {
        (self.size * 0.5 * pressure.clamp(0.0, 1.0)).max(Self::MIN_RADIUS)
    }
}
