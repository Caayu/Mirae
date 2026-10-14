//! Scatter: random displacement (and optional rotation/scale jitter) of
//! dabs around their place on the path.
//!
//! Spacing is decided before scattering, from the unscattered dab, so the
//! number of dabs per unit of path never depends on the scatter amount.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::Dab;
use crate::rng::{StrokeRng, stream};

/// Direction in which dabs are displaced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScatterMode {
    /// Along the path's normal, like a wobbling hand.
    #[default]
    Perpendicular,
    /// Uniformly inside a disc, like a spray.
    Radial,
    /// Mode from a newer version; behaves as `Perpendicular`.
    #[serde(other)]
    Unknown,
}

/// Scatter settings of a preset. The displacement amount itself is the
/// `scatter` brush property (a fraction of the dab diameter), so it can be
/// driven by effectors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScatterStyle {
    /// Direction of displacement.
    pub mode: ScatterMode,
    /// Dabs per placement, `1..=16`.
    pub count: u32,
    /// Maximum random rotation, degrees.
    pub rotation_jitter: f32,
    /// Maximum random shrink, `0..=1` (0.3 = up to 30% smaller).
    pub scale_jitter: f32,
}

impl Default for ScatterStyle {
    fn default() -> Self {
        Self {
            mode: ScatterMode::Perpendicular,
            count: 1,
            rotation_jitter: 0.0,
            scale_jitter: 0.0,
        }
    }
}

impl ScatterStyle {
    /// Upper bound of [`ScatterStyle::count`]; validation clamps to it.
    pub const MAX_COUNT: u32 = 16;

    /// Emits the scattered copies of `base`. `amount` is the maximum
    /// displacement as a fraction of the dab's diameter; `direction` is the
    /// unit direction of travel; `index` the dab's placement index.
    pub fn apply(
        &self,
        base: &Dab,
        amount: f32,
        direction: Vec2,
        rng: &StrokeRng,
        index: u64,
        out: &mut Vec<Dab>,
    ) {
        let count = self.count.clamp(1, Self::MAX_COUNT);
        let amount = if amount.is_finite() {
            amount.max(0.0)
        } else {
            0.0
        };
        if count == 1 && amount == 0.0 && self.rotation_jitter == 0.0 && self.scale_jitter == 0.0 {
            out.push(*base);
            return;
        }
        let reach = amount * 2.0 * base.radius;
        for k in 0..count as u64 {
            let i = index * count as u64 + k;
            let offset = match self.mode {
                ScatterMode::Perpendicular | ScatterMode::Unknown => {
                    direction.perp() * reach * rng.signed(stream::SCATTER_A, i)
                }
                ScatterMode::Radial => {
                    let angle = std::f32::consts::TAU * rng.uniform(stream::SCATTER_A, i);
                    // sqrt keeps the density uniform over the disc's area.
                    Vec2::from_angle(angle) * reach * rng.uniform(stream::SCATTER_B, i).sqrt()
                }
            };
            let mut dab = *base;
            dab.center += offset;
            dab.angle +=
                self.rotation_jitter.to_radians() * rng.signed(stream::SCATTER_ROTATION, i);
            dab.radius *=
                1.0 - self.scale_jitter.clamp(0.0, 1.0) * rng.uniform(stream::SCATTER_SCALE, i);
            out.push(dab);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Dab {
        Dab {
            center: Vec2::new(100.0, 100.0),
            radius: 10.0,
            ..Dab::default()
        }
    }

    fn run(style: &ScatterStyle, amount: f32, n: u64) -> Vec<Dab> {
        let rng = StrokeRng::new(3);
        let mut out = vec![];
        for i in 0..n {
            style.apply(&base(), amount, Vec2::X, &rng, i, &mut out);
        }
        out
    }

    #[test]
    fn zero_scatter_is_identity() {
        let out = run(&ScatterStyle::default(), 0.0, 5);
        assert!(out.iter().all(|d| *d == base()));
    }

    #[test]
    fn perpendicular_stays_on_the_normal_within_reach() {
        let out = run(&ScatterStyle::default(), 0.5, 500);
        for d in &out {
            assert_eq!(d.center.x, 100.0);
            assert!((d.center.y - 100.0).abs() <= 10.0 + 1e-4);
        }
        let spread = out
            .iter()
            .map(|d| (d.center.y - 100.0).abs())
            .fold(0.0, f32::max);
        assert!(spread > 9.0, "uses the full reach");
    }

    #[test]
    fn radial_is_uniform_over_the_disc() {
        let style = ScatterStyle {
            mode: ScatterMode::Radial,
            ..Default::default()
        };
        let out = run(&style, 1.0, 20_000);
        let inner = out
            .iter()
            .filter(|d| (d.center - base().center).length() < 20.0 / 2f32.sqrt())
            .count();
        // Half the area lies inside radius R/sqrt(2).
        let frac = inner as f32 / out.len() as f32;
        assert!((frac - 0.5).abs() < 0.02, "{frac}");
        assert!(
            out.iter()
                .all(|d| (d.center - base().center).length() <= 20.0 + 1e-3)
        );
    }

    #[test]
    fn count_jitter_and_determinism() {
        let style = ScatterStyle {
            mode: ScatterMode::Radial,
            count: 3,
            rotation_jitter: 30.0,
            scale_jitter: 0.5,
        };
        let a = run(&style, 0.3, 10);
        assert_eq!(a.len(), 30);
        assert_eq!(a, run(&style, 0.3, 10));
        assert!(a.iter().all(|d| d.radius <= 10.0 && d.radius >= 5.0));
        assert!(a.iter().all(|d| d.angle.abs() <= 30f32.to_radians() + 1e-5));
        assert!(a.windows(2).any(|w| w[0].angle != w[1].angle));
    }

    #[test]
    fn unknown_mode_and_bad_values_are_safe() {
        let s: ScatterStyle = serde_json::from_str(r#"{"mode":"vortex","count":999}"#).unwrap();
        assert_eq!(s.mode, ScatterMode::Unknown);
        let out = run(&s, f32::NAN, 1);
        assert_eq!(out.len(), ScatterStyle::MAX_COUNT as usize);
        assert!(out.iter().all(|d| d.center.is_finite()));
    }
}
