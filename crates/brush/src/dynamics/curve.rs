/// Editable response curve on `[0, 1] -> [0, 1]`, defined by control points.
///
/// Evaluation uses monotone cubic Hermite interpolation (Fritsch–Carlson):
/// the curve passes exactly through every control point, is C1-smooth, and
/// never overshoots between neighbouring points. Overshoot would be a real bug
/// for pressure response: raising one point must never make the output dip
/// somewhere else.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    xs: Vec<f32>,
    ys: Vec<f32>,
    /// Tangent at each control point.
    ms: Vec<f32>,
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
    fn degenerate_inputs() {
        assert_eq!(Curve::new(&[]), Curve::linear());
        let single = Curve::new(&[(0.4, 0.7)]);
        assert_eq!(single.eval(0.0), 0.7);
        assert_eq!(single.eval(1.0), 0.7);
        assert_eq!(Curve::constant(0.3).eval(0.77), 0.3);
    }
}
