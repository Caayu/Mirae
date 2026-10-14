use serde::{Deserialize, Serialize};

/// Editable response curve on `[0, 1] -> [0, 1]`, defined by control points.
///
/// Evaluation uses monotone cubic Hermite interpolation (Fritsch–Carlson):
/// the curve passes exactly through every control point, is C1-smooth, and
/// never overshoots between neighbouring points. Overshoot would be a real bug
/// for pressure response: raising one point must never make the output dip
/// somewhere else.
///
/// Serialized as a list of `[x, y]` points; loading always goes through
/// [`Curve::new`], so malformed data cannot produce NaN or an invalid curve.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "Vec<[f32; 2]>", into = "Vec<[f32; 2]>")]
pub struct Curve {
    xs: Vec<f32>,
    ys: Vec<f32>,
    /// Tangent at each control point.
    ms: Vec<f32>,
}

impl From<Vec<[f32; 2]>> for Curve {
    fn from(points: Vec<[f32; 2]>) -> Self {
        let pts: Vec<(f32, f32)> = points.into_iter().map(|[x, y]| (x, y)).collect();
        Self::new(&pts)
    }
}

impl From<Curve> for Vec<[f32; 2]> {
    fn from(c: Curve) -> Self {
        c.points().map(|(x, y)| [x, y]).collect()
    }
}

impl Default for Curve {
    fn default() -> Self {
        Self::linear()
    }
}

impl Curve {
    /// Builds a curve from control points. Points are clamped to the unit
    /// square and sorted; points sharing an x keep the last one given.
    /// An empty list yields the identity curve.
    pub fn new(points: &[(f32, f32)]) -> Self {
        let mut pts: Vec<(f32, f32)> = points
            .iter()
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .map(|&(x, y)| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
            .collect();
        if pts.is_empty() {
            return Self::linear();
        }
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut dedup: Vec<(f32, f32)> = Vec::with_capacity(pts.len());
        for p in pts {
            match dedup.last_mut() {
                Some(last) if (last.0 - p.0).abs() < 1e-6 => *last = p,
                _ => dedup.push(p),
            }
        }
        let (xs, ys): (Vec<f32>, Vec<f32>) = dedup.into_iter().unzip();
        let ms = monotone_tangents(&xs, &ys);
        Self { xs, ys, ms }
    }

    /// `y = x`.
    pub fn linear() -> Self {
        Self::new(&[(0.0, 0.0), (1.0, 1.0)])
    }

    /// `y = v` everywhere.
    pub fn constant(v: f32) -> Self {
        Self::new(&[(0.0, v), (1.0, v)])
    }

    pub fn points(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        self.xs.iter().copied().zip(self.ys.iter().copied())
    }

    pub fn len(&self) -> usize {
        self.xs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.xs.is_empty()
    }

    /// Same points with every y raised to at least the previous one, for
    /// parameters that must respond monotonically (e.g. pressure).
    pub fn monotone(&self) -> Self {
        let mut floor = 0.0f32;
        let pts: Vec<(f32, f32)> = self
            .points()
            .map(|(x, y)| {
                floor = floor.max(y);
                (x, floor)
            })
            .collect();
        Self::new(&pts)
    }

    pub fn is_monotone(&self) -> bool {
        self.ys.windows(2).all(|w| w[1] >= w[0])
    }

    /// Editing: inserts a point on the curve at `x`. Returns its index.
    pub fn insert_point(&mut self, x: f32) -> usize {
        let x = if x.is_finite() {
            x.clamp(0.0, 1.0)
        } else {
            0.5
        };
        let y = self.eval(x);
        let mut pts: Vec<(f32, f32)> = self.points().collect();
        pts.push((x, y));
        *self = Self::new(&pts);
        self.xs
            .iter()
            .position(|&px| (px - x).abs() < 1e-6)
            .unwrap_or(0)
    }

    /// Editing: moves point `i`. Endpoints keep their x (0 and 1) so the
    /// whole input range stays defined; interior points stay strictly
    /// between their neighbours so the order never changes.
    pub fn move_point(&mut self, i: usize, x: f32, y: f32) {
        let n = self.xs.len();
        if i >= n || !x.is_finite() || !y.is_finite() {
            return;
        }
        let mut pts: Vec<(f32, f32)> = self.points().collect();
        let x = if i == 0 || i == n - 1 {
            pts[i].0
        } else {
            const GAP: f32 = 1e-3;
            x.clamp(pts[i - 1].0 + GAP, pts[i + 1].0 - GAP)
        };
        pts[i] = (x, y.clamp(0.0, 1.0));
        *self = Self::new(&pts);
    }

    /// Editing: removes an interior point (endpoints cannot be removed).
    pub fn remove_point(&mut self, i: usize) -> bool {
        let n = self.xs.len();
        if i == 0 || i + 1 >= n {
            return false;
        }
        let mut pts: Vec<(f32, f32)> = self.points().collect();
        pts.remove(i);
        *self = Self::new(&pts);
        true
    }

    pub fn eval(&self, x: f32) -> f32 {
        let n = self.xs.len();
        if n == 1 {
            return self.ys[0];
        }
        let x = if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) };
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[n - 1] {
            return self.ys[n - 1];
        }
        // Index of the interval [xs[k], xs[k+1]] containing x.
        let k = self.xs.partition_point(|&xi| xi <= x) - 1;
        let h = self.xs[k + 1] - self.xs[k];
        let t = (x - self.xs[k]) / h;
        let (t2, t3) = (t * t, t * t * t);
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        let y = h00 * self.ys[k]
            + h10 * h * self.ms[k]
            + h01 * self.ys[k + 1]
            + h11 * h * self.ms[k + 1];
        y.clamp(0.0, 1.0)
    }
}

/// Fritsch–Carlson tangents: secant averages, zeroed at local extrema and
/// rescaled where they would cause overshoot.
fn monotone_tangents(xs: &[f32], ys: &[f32]) -> Vec<f32> {
    let n = xs.len();
    if n < 2 {
        return vec![0.0; n];
    }
    let d: Vec<f32> = (0..n - 1)
        .map(|k| (ys[k + 1] - ys[k]) / (xs[k + 1] - xs[k]))
        .collect();
    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for k in 1..n - 1 {
        m[k] = if d[k - 1] * d[k] <= 0.0 {
            0.0
        } else {
            (d[k - 1] + d[k]) * 0.5
        };
    }
    for k in 0..n - 1 {
        if d[k] == 0.0 {
            m[k] = 0.0;
            m[k + 1] = 0.0;
            continue;
        }
        let a = m[k] / d[k];
        let b = m[k + 1] / d[k];
        let s = a * a + b * b;
        if s > 9.0 {
            let tau = 3.0 / s.sqrt();
            m[k] = tau * a * d[k];
            m[k + 1] = tau * b * d[k];
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_control_points() {
        let pts = [(0.0, 0.1), (0.3, 0.2), (0.6, 0.85), (1.0, 1.0)];
        let c = Curve::new(&pts);
        for (x, y) in pts {
            assert!(
                (c.eval(x) - y).abs() < 1e-6,
                "f({x}) = {} != {y}",
                c.eval(x)
            );
        }
    }

    #[test]
    fn linear_is_identity() {
        let c = Curve::linear();
        for i in 0..=20 {
            let x = i as f32 / 20.0;
            assert!((c.eval(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn monotone_data_gives_monotone_curve() {
        // Steep step that a plain cubic spline would overshoot.
        let c = Curve::new(&[(0.0, 0.0), (0.45, 0.02), (0.55, 0.98), (1.0, 1.0)]);
        let mut prev = -1.0;
        for i in 0..=1000 {
            let y = c.eval(i as f32 / 1000.0);
            assert!(y >= prev - 1e-6, "dip at {i}");
            assert!((0.0..=1.0).contains(&y));
            prev = y;
        }
    }

    #[test]
    fn no_overshoot_around_extremum() {
        let c = Curve::new(&[(0.0, 0.0), (0.5, 0.8), (1.0, 0.0)]);
        for i in 0..=1000 {
            assert!(c.eval(i as f32 / 1000.0) <= 0.8 + 1e-6);
        }
    }

    #[test]
    fn input_is_clamped_and_sorted() {
        let c = Curve::new(&[(1.5, 1.0), (-1.0, 0.25)]);
        assert_eq!(
            c.points().collect::<Vec<_>>(),
            vec![(0.0, 0.25), (1.0, 1.0)]
        );
        assert_eq!(c.eval(-5.0), 0.25);
        assert_eq!(c.eval(f32::NAN), 0.25);
        assert_eq!(c.eval(9.0), 1.0);
    }

    #[test]
    fn serde_round_trip_and_sanitizing() {
        let c = Curve::new(&[(0.0, 0.1), (0.4, 0.3), (1.0, 0.9)]);
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "[[0.0,0.1],[0.4,0.3],[1.0,0.9]]");
        assert_eq!(serde_json::from_str::<Curve>(&json).unwrap(), c);
        let messy: Curve = serde_json::from_str("[[2,5],[-1,0.5],[0.5,-3]]").unwrap();
        assert_eq!(
            messy.points().collect::<Vec<_>>(),
            vec![(0.0, 0.5), (0.5, 0.0), (1.0, 1.0)]
        );
        assert!((0..=100).all(|i| messy.eval(i as f32 / 100.0).is_finite()));
    }

    #[test]
    fn editing_keeps_endpoints_and_order() {
        let mut c = Curve::linear();
        let i = c.insert_point(0.5);
        assert_eq!((c.len(), i), (3, 1));
        assert!(
            (c.eval(0.5) - 0.5).abs() < 1e-6,
            "insertion keeps the shape"
        );
        c.move_point(1, 2.0, 0.9);
        let pts: Vec<_> = c.points().collect();
        assert!(pts[1].0 < 1.0 && pts[1].1 == 0.9);
        c.move_point(0, 0.7, 0.2);
        assert_eq!(
            c.points().next().unwrap(),
            (0.0, 0.2),
            "endpoint x is fixed"
        );
        c.move_point(1, f32::NAN, 0.5);
        assert_eq!(c.len(), 3, "NaN edits are ignored");
        assert!(!c.remove_point(0) && !c.remove_point(2));
        assert!(c.remove_point(1));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn monotone_projection() {
        let c = Curve::new(&[(0.0, 0.5), (0.5, 0.2), (1.0, 0.8)]);
        assert!(!c.is_monotone());
        let m = c.monotone();
        assert!(m.is_monotone());
        assert_eq!(m.points().nth(1).unwrap().1, 0.5);
    }

    #[test]
    fn degenerate_inputs() {
        assert_eq!(Curve::new(&[]), Curve::linear());
        let single = Curve::new(&[(0.4, 0.7)]);
        assert_eq!(single.eval(0.0), 0.7);
        assert_eq!(single.eval(1.0), 0.7);
        assert_eq!(Curve::constant(0.3).eval(0.77), 0.3);
    }
}
