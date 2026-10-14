use glam::Vec2;

use crate::pipeline::StrokePoint;

/// One brush-tip impression, fully resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub center: Vec2,
    /// Half the tip's major diameter, canvas pixels.
    pub radius: f32,
    /// Minor/major diameter ratio, `0.01..=1`.
    pub aspect: f32,
    /// Rotation of the tip's major axis, radians.
    pub angle: f32,
    /// Alpha ceiling this dab pulls pixels toward, `0..=1`.
    pub opacity: f32,
    /// Fraction of the remaining distance to the ceiling covered at full
    /// tip coverage, `0..=1`.
    pub flow: f32,
    pub hardness: f32,
    /// How strongly the paper texture modulates this dab, `0..=1`.
    pub texture: f32,
}

impl Default for Dab {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            radius: 1.0,
            aspect: 1.0,
            angle: 0.0,
            opacity: 1.0,
            flow: 1.0,
            hardness: 1.0,
            texture: 0.0,
        }
    }
}

impl Dab {
    /// Length of the tip's support along `direction` (a unit vector):
    /// the extent of the ellipse with radii `radius`, `radius * aspect`
    /// projected on that direction, times two.
    pub fn extent_along(&self, direction: Vec2) -> f32 {
        let along = Vec2::from_angle(self.angle);
        let c = direction.dot(along);
        let s = direction.perp_dot(along);
        let (a, b) = (self.radius, self.radius * self.aspect);
        2.0 * ((a * c).powi(2) + (b * s).powi(2)).sqrt()
    }
}

/// Where along the path a dab falls, before dynamics are applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DabSite {
    pub point: StrokePoint,
    /// Canvas pixels from the stroke start along the path.
    pub distance: f32,
    /// Sequential number of the dab within the stroke.
    pub index: u32,
}

/// Places dabs along a dense polyline at a spacing that may vary per dab.
///
/// The arc length still owed to the next dab is carried across segments, so
/// placement depends only on the path and on the step function, never on
/// how densely the path was sampled. `Clone` is cheap so a provisional run
/// (the stroke's tail) can start from a snapshot of the committed state.
#[derive(Clone, Copy, Debug, Default)]
pub struct DabSpacer {
    last: Option<StrokePoint>,
    until_next: f32,
    distance: f32,
    next_index: u32,
}

impl DabSpacer {
    /// Never step less than this, so a 1 px brush with tiny spacing cannot
    /// produce an unbounded number of dabs.
    pub const MIN_STEP: f32 = 0.5;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Arc length processed so far, canvas pixels.
    pub fn distance(&self) -> f32 {
        self.distance
    }

    /// Feeds the next path point. `step` returns the distance to the next
    /// dab after a dab placed at the given site. The first point of a stroke
    /// always produces a dab.
    pub fn push(
        &mut self,
        p: StrokePoint,
        mut step: impl FnMut(&DabSite) -> f32,
        out: &mut Vec<DabSite>,
    ) {
        let mut emit = |this: &mut Self, point: StrokePoint, distance: f32| {
            let site = DabSite {
                point,
                distance,
                index: this.next_index,
            };
            this.next_index += 1;
            this.until_next = step(&site).max(Self::MIN_STEP);
            out.push(site);
        };

        let Some(prev) = self.last.replace(p) else {
            emit(self, p, 0.0);
            return;
        };
        let len = (p.pos - prev.pos).length();
        if len <= f32::EPSILON {
            return;
        }
        let mut walked = 0.0;
        while walked + self.until_next <= len {
            walked += self.until_next;
            let point = prev.lerp(&p, walked / len);
            emit(self, point, self.distance + walked);
        }
        self.until_next -= len - walked;
        self.distance += len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrushInput;

    fn point(x: f32, y: f32) -> StrokePoint {
        BrushInput::from_position(Vec2::new(x, y), 0.0).into()
    }

    #[test]
    fn first_point_emits_one_dab() {
        let mut s = DabSpacer::new();
        let mut out = vec![];
        s.push(point(3.0, 4.0), |_| 5.0, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].point.pos, Vec2::new(3.0, 4.0));
        assert_eq!(out[0].distance, 0.0);
    }

    #[test]
    fn spacing_is_uniform_regardless_of_point_density() {
        for n in [2usize, 7, 33, 101] {
            let mut s = DabSpacer::new();
            let mut out = vec![];
            for i in 0..n {
                let x = 100.0 * i as f32 / (n - 1) as f32;
                s.push(point(x, 0.0), |_| 5.0, &mut out);
            }
            assert_eq!(out.len(), 21, "n={n}");
            for (i, w) in out.windows(2).enumerate() {
                assert!((w[1].point.pos.x - w[0].point.pos.x - 5.0).abs() < 1e-3);
                assert_eq!(w[1].index, i as u32 + 1);
            }
            assert!((s.distance() - 100.0).abs() < 1e-3);
        }
    }

    #[test]
    fn variable_step_follows_the_step_function() {
        // Step grows with distance: dab gaps must match it exactly.
        let mut s = DabSpacer::new();
        let mut out = vec![];
        let step = |site: &DabSite| 1.0 + site.distance * 0.1;
        for i in 0..=200 {
            s.push(point(i as f32, 0.0), step, &mut out);
        }
        for w in out.windows(2) {
            let expected = 1.0 + w[0].distance * 0.1;
            assert!((w[1].distance - w[0].distance - expected).abs() < 1e-3);
        }
    }

    #[test]
    fn snapshot_resumes_identically() {
        let mut a = DabSpacer::new();
        let mut out_a = vec![];
        for i in 0..50 {
            a.push(point(i as f32 * 3.0, 0.0), |_| 4.0, &mut out_a);
        }
        let mut b = a;
        let (mut tail_a, mut tail_b) = (vec![], vec![]);
        for i in 50..80 {
            a.push(point(i as f32 * 3.0, 0.0), |_| 4.0, &mut tail_a);
            b.push(point(i as f32 * 3.0, 0.0), |_| 4.0, &mut tail_b);
        }
        assert_eq!(tail_a, tail_b);
    }

    #[test]
    fn minimum_step_bounds_dab_count() {
        let mut s = DabSpacer::new();
        let mut out = vec![];
        s.push(point(0.0, 0.0), |_| 0.0, &mut out);
        s.push(point(10.0, 0.0), |_| 0.0, &mut out);
        assert_eq!(out.len(), 1 + (10.0 / DabSpacer::MIN_STEP) as usize);
    }
}
