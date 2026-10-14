//! Path interpolation: connects consecutive (already stabilized) samples with
//! a curve and flattens it into a dense polyline for dab placement.
//!
//! Interpolators never move the samples themselves; that is the stabilizer's
//! job. Curved interpolators need the *next* sample to finish a segment, so
//! they run one sample behind the input.
//!
//! Non-positional attributes (pressure, tilt, time, ...) are interpolated by
//! *arc-length fraction* within each segment, not by curve parameter. On a
//! straight path this makes the attribute a function of distance alone, so
//! the result does not depend on how densely the device sampled it.

use glam::Vec2;

use crate::pipeline::StrokePoint;

pub trait PathInterpolator {
    fn reset(&mut self);
    /// Adds a sample. Appends the dense points of every segment that became
    /// fully determined, never repeating a point already emitted. `step` is
    /// the longest chord allowed in the output, canvas pixels.
    fn push(&mut self, p: StrokePoint, step: f32, out: &mut Vec<StrokePoint>);
    /// Emits whatever is still pending at the end of the stroke.
    fn finish(&mut self, step: f32, out: &mut Vec<StrokePoint>);
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum InterpolationKind {
    Linear,
    /// Default: chosen from the comparative tests (see `tests/interpolation_comparison.rs`).
    #[default]
    CatmullRom,
    Bezier,
}

impl InterpolationKind {
    pub const ALL: [Self; 3] = [Self::Linear, Self::CatmullRom, Self::Bezier];

    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::CatmullRom => "catmull-rom",
            Self::Bezier => "bezier",
        }
    }
}

/// Enum dispatch over the interpolators.
#[derive(Clone, Debug)]
pub struct Interpolator {
    kind: InterpolationKind,
    /// The last four control points, oldest first; `len` of them are valid.
    ctrl: [StrokePoint; 4],
    len: usize,
    /// Scratch for flattening, reused to avoid per-sample allocation.
    scratch: Vec<(Vec2, f32)>,
}

impl Interpolator {
    pub fn new(kind: InterpolationKind) -> Self {
        let zero = StrokePoint::from(crate::BrushInput::from_position(Vec2::ZERO, 0.0));
        Self {
            kind,
            ctrl: [zero; 4],
            len: 0,
            scratch: Vec::with_capacity(512),
        }
    }

    pub fn kind(&self) -> InterpolationKind {
        self.kind
    }

    fn remember(&mut self, p: StrokePoint) {
        if self.len == 4 {
            self.ctrl.rotate_left(1);
            self.ctrl[3] = p;
        } else {
            self.ctrl[self.len] = p;
            self.len += 1;
        }
    }

    /// Emits the segment `b -> c` (exclusive of `b`), with neighbours `a`, `d`.
    fn segment(
        &mut self,
        a: Vec2,
        b: StrokePoint,
        c: StrokePoint,
        d: Vec2,
        step: f32,
        out: &mut Vec<StrokePoint>,
    ) {
        let chord = (c.pos - b.pos).length();
        if chord <= f32::EPSILON {
            out.push(c);
            return;
        }
        let n = ((chord / step.max(1e-3)).ceil() as usize).clamp(1, 1024);
        self.scratch.clear();
        self.scratch.push((b.pos, 0.0));
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let pos = match self.kind {
                InterpolationKind::Linear => b.pos.lerp(c.pos, t),
                InterpolationKind::CatmullRom => centripetal_catmull_rom(a, b.pos, c.pos, d, t),
                InterpolationKind::Bezier => smoothed_bezier(a, b.pos, c.pos, d, t),
            };
            let prev = self.scratch[i - 1];
            self.scratch.push((pos, prev.1 + (pos - prev.0).length()));
        }
        let total = self.scratch[n].1.max(f32::EPSILON);
        for &(pos, s) in &self.scratch[1..] {
            let mut p = b.lerp(&c, s / total);
            p.pos = pos;
            out.push(p);
        }
        if let Some(last) = out.last_mut() {
            last.pos = c.pos;
        }
    }
}

impl PathInterpolator for Interpolator {
    fn reset(&mut self) {
        self.len = 0;
    }

    fn push(&mut self, p: StrokePoint, step: f32, out: &mut Vec<StrokePoint>) {
        if self.len > 0 && (p.pos - self.ctrl[self.len - 1].pos).length() <= f32::EPSILON {
            // Repeated position: keep the newest attributes, no new segment.
            self.ctrl[self.len - 1] = p;
            return;
        }
        self.remember(p);
        if self.kind == InterpolationKind::Linear {
            if self.len == 1 {
                out.push(p);
            } else {
                let (b, c) = (self.ctrl[self.len - 2], self.ctrl[self.len - 1]);
                self.segment(b.pos, b, c, c.pos, step, out);
            }
            return;
        }
        match self.len {
            1 => out.push(p),
            2 => {}
            3 => {
                // First segment: mirror the start to get a phantom neighbour.
                let [p0, p1, p2, _] = self.ctrl;
                self.segment(2.0 * p0.pos - p1.pos, p0, p1, p2.pos, step, out);
            }
            _ => {
                let [p0, p1, p2, p3] = self.ctrl;
                self.segment(p0.pos, p1, p2, p3.pos, step, out);
            }
        }
    }

    fn finish(&mut self, step: f32, out: &mut Vec<StrokePoint>) {
        if self.kind == InterpolationKind::Linear || self.len < 2 {
            self.len = 0;
            return;
        }
        let n = self.len;
        let (b, c) = (self.ctrl[n - 2], self.ctrl[n - 1]);
        let a = if n >= 3 {
            self.ctrl[n - 3].pos
        } else {
            2.0 * b.pos - c.pos
        };
        // Mirror the end for the phantom neighbour after the last sample.
        self.segment(a, b, c, 2.0 * c.pos - b.pos, step, out);
        self.len = 0;
    }
}

/// Centripetal Catmull–Rom (alpha = 1/2) segment between `p1` and `p2`,
/// evaluated with the Barry–Goldman pyramid. Knot spacing grows with the
/// square root of chord length, which provably prevents cusps and
/// self-intersections within a segment, unlike the uniform variant.
pub fn centripetal_catmull_rom(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, u: f32) -> Vec2 {
    let knot = |a: Vec2, b: Vec2| (a - b).length().sqrt().max(1e-4);
    let t0 = 0.0;
    let t1 = t0 + knot(p0, p1);
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let t = t1 + (t2 - t1) * u;
    let lerp = |a: Vec2, b: Vec2, ta: f32, tb: f32| {
        a * ((tb - t) / (tb - ta)) + b * ((t - ta) / (tb - ta))
    };
    let a1 = lerp(p0, p1, t0, t1);
    let a2 = lerp(p1, p2, t1, t2);
    let a3 = lerp(p2, p3, t2, t3);
    let b1 = lerp(a1, a2, t0, t2);
    let b2 = lerp(a2, a3, t1, t3);
    lerp(b1, b2, t1, t2)
}

/// Cubic Bézier between `p1` and `p2` whose tangents point along the
/// neighbour chords (`p2 - p0`, `p3 - p1`) and whose handles are a third of
/// the local chord. Short handles keep the curve inside the region spanned by
/// the samples even when neighbouring chords differ a lot in length.
pub fn smoothed_bezier(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, u: f32) -> Vec2 {
    let h = (p2 - p1).length() / 3.0;
    let c1 = p1 + (p2 - p0).normalize_or_zero() * h;
    let c2 = p2 - (p3 - p1).normalize_or_zero() * h;
    let v = 1.0 - u;
    p1 * (v * v * v) + c1 * (3.0 * v * v * u) + c2 * (3.0 * v * u * u) + p2 * (u * u * u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrushInput;

    fn pt(x: f32, y: f32) -> StrokePoint {
        BrushInput::from_position(Vec2::new(x, y), 0.0).into()
    }

    fn run(kind: InterpolationKind, pts: &[StrokePoint]) -> Vec<StrokePoint> {
        let mut it = Interpolator::new(kind);
        let mut out = vec![];
        for p in pts {
            it.push(*p, 1.0, &mut out);
        }
        it.finish(1.0, &mut out);
        out
    }

    #[test]
    fn curves_pass_through_every_sample_in_order() {
        let pts = [
            pt(0.0, 0.0),
            pt(30.0, 10.0),
            pt(60.0, -5.0),
            pt(90.0, 20.0),
            pt(100.0, 60.0),
        ];
        for kind in InterpolationKind::ALL {
            let out = run(kind, &pts);
            let mut cursor = 0;
            for p in &pts {
                let found = out[cursor..]
                    .iter()
                    .position(|o| (o.pos - p.pos).length() < 1e-4);
                cursor += found.unwrap_or_else(|| panic!("{kind:?} misses {:?}", p.pos));
            }
            assert_eq!(out.last().unwrap().pos, pts[4].pos, "{kind:?}");
        }
    }

    #[test]
    fn chords_respect_step() {
        let pts = [pt(0.0, 0.0), pt(100.0, 0.0), pt(100.0, 100.0)];
        for kind in InterpolationKind::ALL {
            let mut it = Interpolator::new(kind);
            let mut out = vec![];
            for p in pts {
                it.push(p, 2.0, &mut out);
            }
            it.finish(2.0, &mut out);
            for w in out.windows(2) {
                assert!((w[1].pos - w[0].pos).length() <= 2.5, "{kind:?}");
            }
        }
    }

    #[test]
    fn collinear_samples_stay_on_the_line() {
        let pts = [
            pt(0.0, 0.0),
            pt(3.0, 0.0),
            pt(50.0, 0.0),
            pt(51.0, 0.0),
            pt(200.0, 0.0),
        ];
        for kind in InterpolationKind::ALL {
            for o in run(kind, &pts) {
                assert!(o.pos.y.abs() < 1e-3, "{kind:?}");
                assert!(
                    (-1e-3..=200.0 + 1e-3).contains(&o.pos.x),
                    "{kind:?} {:?}",
                    o.pos
                );
            }
        }
    }

    #[test]
    fn attributes_follow_arc_length() {
        let mut a = pt(0.0, 0.0);
        a.pressure = 0.0;
        let mut b = pt(100.0, 0.0);
        b.pressure = 1.0;
        let mut c = pt(200.0, 0.0);
        c.pressure = 1.0;
        let out = run(InterpolationKind::CatmullRom, &[a, b, c]);
        for o in out.iter().filter(|o| o.pos.x <= 100.0) {
            assert!((o.pressure - o.pos.x / 100.0).abs() < 1e-3, "{:?}", o);
        }
    }

    #[test]
    fn single_and_repeated_samples() {
        for kind in InterpolationKind::ALL {
            assert_eq!(run(kind, &[pt(5.0, 5.0)]).len(), 1);
            assert_eq!(
                run(kind, &[pt(5.0, 5.0), pt(5.0, 5.0), pt(5.0, 5.0)]).len(),
                1
            );
            let two = run(kind, &[pt(0.0, 0.0), pt(10.0, 0.0)]);
            assert_eq!(two.last().unwrap().pos, Vec2::new(10.0, 0.0));
        }
    }

    #[test]
    fn centripetal_has_no_cusp_on_uneven_spacing() {
        // Very uneven chords: the uniform variant would loop here.
        let (p0, p1, p2, p3) = (
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.5, 50.0),
            Vec2::new(2.0, 50.5),
        );
        let mut prev = p1;
        let mut length = 0.0;
        for i in 1..=100 {
            let q = centripetal_catmull_rom(p0, p1, p2, p3, i as f32 / 100.0);
            length += (q - prev).length();
            prev = q;
        }
        // A loop would make the segment much longer than its chord.
        assert!(length < (p2 - p1).length() * 1.3, "{length}");
    }
}
