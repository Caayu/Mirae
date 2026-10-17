//! Gradient colours (phase 5F, ADR 0027): the stops of a gradient and the
//! colour at a position between them, with the reference application's
//! measured rules (docs/parity/5F-gradients.md):
//!
//! - every segment between two neighbouring stops is **eased** by
//!   [`ease`], `E(u) = 0.38·u + 0.62·q(u)` with `q` the quadratic
//!   ease-in-out (GD6, GD10): the reference never mixes linearly;
//! - colour and alpha are interpolated **straight** and independently
//!   (GD12), except that a stop of opacity 0 has no colour: its segment
//!   takes the other end's colour (GD11, GD13);
//! - colours mix in one of three spaces ([`Mixing`]): the sRGB-encoded
//!   values (the reference's "Padrão", GD6), linear light ("Linear",
//!   GD7) or OKLab ("Perceptiva (compatível com Photoshop)", GD8; Björn
//!   Ottosson, *A perceptual color space for image processing*, 2020).
//!
//! The result is a straight [`Rgba`] in the document's encoding (sRGB,
//! ADR 0016), `f32` to the single quantization at a document's depth.

use crate::{Color, Rgba, srgb};

/// The reference's easing of a segment (GD6): `0.38·u + 0.62·q(u)`, with
/// `q(u) = 2u²` below one half and `1 − 2(1 − u)²` above. Monotone, `E(0)
/// = 0`, `E(½) = ½`, `E(1) = 1`, symmetric (`E(1 − u) = 1 − E(u)`);
/// slope 0.38 at the ends and 1.62 in the middle. `u` is clamped to
/// `0..=1`; not a number is 0.
pub fn ease(u: f32) -> f32 {
    let u = if u.is_nan() { 0.0 } else { u.clamp(0.0, 1.0) };
    let q = if u < 0.5 {
        2.0 * u * u
    } else {
        let v = 1.0 - u;
        1.0 - 2.0 * v * v
    };
    0.38 * u + 0.62 * q
}

/// The space colours are mixed in between two stops.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mixing {
    /// The sRGB-encoded channels, as stored (the reference's "Padrão").
    Standard,
    /// Linear light ("Linear (compatível com Photoshop)").
    Linear,
    /// OKLab ("Perceptiva (compatível com Photoshop)"); the nearest of the
    /// three to the reference's default "Perceptiva" (D-5F-1).
    #[default]
    Perceptual,
}

/// One stop of a gradient: where it is, its colour and its opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// `0..=1` along the gradient.
    pub position: f32,
    pub color: Color,
    /// `0..=1`; 0 is a transparent stop, which has no colour (GD13).
    pub opacity: f32,
}

impl GradientStop {
    /// A stop with its values made valid: position and opacity clamped
    /// to `0..=1`, not a number 0.
    pub fn new(position: f32, color: Color, opacity: f32) -> Self {
        Self {
            position: unit(position),
            color,
            opacity: unit(opacity),
        }
    }
}

fn unit(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

/// The stops of a gradient: at least one, valid (see
/// [`GradientStop::new`]), ordered by position. Stops at the same
/// position keep their order and make a hard edge there: the later one
/// starts the next segment.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientStops(Vec<GradientStop>);

/// At most this many stops (the panel's editor never needs more).
pub const MAX_STOPS: usize = 64;

impl GradientStops {
    /// Stops in any order, made valid and sorted (stably) by position;
    /// `None` when there is none. Beyond [`MAX_STOPS`] the rest is
    /// dropped.
    pub fn new(stops: impl IntoIterator<Item = GradientStop>) -> Option<Self> {
        let mut v: Vec<GradientStop> = stops
            .into_iter()
            .take(MAX_STOPS)
            .map(|s| GradientStop::new(s.position, s.color, s.opacity))
            .collect();
        if v.is_empty() {
            return None;
        }
        v.sort_by(|a, b| a.position.total_cmp(&b.position));
        Some(Self(v))
    }

    /// `a` at 0 to `b` at 1, both opaque (main to sub colour).
    pub fn two(a: Color, b: Color) -> Self {
        Self(vec![
            GradientStop::new(0.0, a, 1.0),
            GradientStop::new(1.0, b, 1.0),
        ])
    }

    /// `c` at 0, opaque, to transparent at 1 (main to transparent).
    pub fn to_transparent(c: Color) -> Self {
        Self(vec![
            GradientStop::new(0.0, c, 1.0),
            GradientStop::new(1.0, c, 0.0),
        ])
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.0
    }
}

/// A gradient's colours prepared for evaluation: each stop converted once
/// into the mixing space.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorRamp {
    mixing: Mixing,
    positions: Vec<f32>,
    /// Per stop: the colour in the mixing space, and the opacity.
    values: Vec<([f32; 3], f32)>,
}

impl ColorRamp {
    pub fn new(stops: &GradientStops, mixing: Mixing) -> Self {
        let positions = stops.0.iter().map(|s| s.position).collect();
        let values = stops
            .0
            .iter()
            .map(|s| (to_space(s.color, mixing), s.opacity))
            .collect();
        Self {
            mixing,
            positions,
            values,
        }
    }

    pub fn mixing(&self) -> Mixing {
        self.mixing
    }

    /// The first and last stops' positions: the colour is constant
    /// before the first and after the last.
    pub fn bounds(&self) -> (f32, f32) {
        (self.positions[0], self.positions[self.positions.len() - 1])
    }

    /// The straight colour at `t` (clamped to `0..=1`; not a number is
    /// 0): the first stop's before it, the last one's after it, eased and
    /// mixed between neighbours. Channels are in `0..=1`.
    pub fn eval(&self, t: f32) -> Rgba {
        let t = unit(t);
        // The segment starts at the last stop at or before `t`, so equal
        // positions switch to the later stop exactly there.
        let i = self.positions.partition_point(|&p| p <= t);
        if i == 0 {
            return self.stop(0);
        }
        if i == self.positions.len() {
            return self.stop(i - 1);
        }
        let (p0, p1) = (self.positions[i - 1], self.positions[i]);
        let e = ease((t - p0) / (p1 - p0));
        let ((c0, a0), (c1, a1)) = (self.values[i - 1], self.values[i]);
        let alpha = a0 + (a1 - a0) * e;
        // A transparent stop has no colour: the other end's is used
        // (GD11, GD13).
        let c = if a1 <= 0.0 {
            c0
        } else if a0 <= 0.0 {
            c1
        } else {
            [
                c0[0] + (c1[0] - c0[0]) * e,
                c0[1] + (c1[1] - c0[1]) * e,
                c0[2] + (c1[2] - c0[2]) * e,
            ]
        };
        let [r, g, b] = from_space(c, self.mixing);
        Rgba::new(r, g, b, unit(alpha))
    }

    fn stop(&self, i: usize) -> Rgba {
        let (c, a) = self.values[i];
        let [r, g, b] = from_space(c, self.mixing);
        Rgba::new(r, g, b, a)
    }
}

fn to_space(c: Color, mixing: Mixing) -> [f32; 3] {
    let encoded = [c.r(), c.g(), c.b()];
    match mixing {
        Mixing::Standard => encoded,
        Mixing::Linear => encoded.map(srgb::to_linear),
        Mixing::Perceptual => linear_to_oklab(encoded.map(srgb::to_linear)),
    }
}

fn from_space(c: [f32; 3], mixing: Mixing) -> [f32; 3] {
    match mixing {
        Mixing::Standard => c.map(unit),
        Mixing::Linear => c.map(|v| srgb::from_linear(unit(v))),
        // OKLab mixes can leave the sRGB gamut (as the reference's do):
        // clamped in linear light before encoding.
        Mixing::Perceptual => oklab_to_linear(c).map(|v| srgb::from_linear(unit(v))),
    }
}

/// Linear sRGB to OKLab (Ottosson 2020; his published matrices).
pub fn linear_to_oklab([r, g, b]: [f32; 3]) -> [f32; 3] {
    let l = 0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// OKLab to linear sRGB (the inverse of [`linear_to_oklab`]); may leave
/// `0..=1` for colours outside the sRGB gamut.
pub fn oklab_to_linear([l, a, b]: [f32; 3]) -> [f32; 3] {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(v: f32) -> Color {
        Color::new(v, v, v)
    }

    fn ramp(stops: GradientStops, mixing: Mixing) -> ColorRamp {
        ColorRamp::new(&stops, mixing)
    }

    #[test]
    fn the_easing_is_the_references() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!((ease(0.5) - 0.5).abs() < 1e-7);
        // Measured slopes (GD6): 0.38 at the ends, 1.62 in the middle.
        let h = 1e-3;
        assert!(((ease(h) - ease(0.0)) / h - 0.38).abs() < 0.01);
        assert!(((ease(0.5 + h) - ease(0.5 - h)) / (2.0 * h) - 1.62).abs() < 0.01);
        // A quarter: 0.38/4 + 0.62/8.
        assert!((ease(0.25) - 0.1725).abs() < 1e-6);
        let mut previous = -1.0;
        for i in 0..=1000 {
            let u = i as f32 / 1000.0;
            let e = ease(u);
            assert!(e >= previous, "monotone at {u}");
            assert!((e + ease(1.0 - u) - 1.0).abs() < 1e-6, "symmetric at {u}");
            previous = e;
        }
        assert_eq!(ease(-3.0), 0.0);
        assert_eq!(ease(7.0), 1.0);
        assert_eq!(ease(f32::NAN), 0.0);
    }

    #[test]
    fn stops_are_made_valid_and_sorted() {
        assert!(GradientStops::new([]).is_none());
        let s = GradientStops::new([
            GradientStop::new(0.9, Color::WHITE, 1.0),
            GradientStop {
                position: f32::NAN,
                color: Color::BLACK,
                opacity: 2.0,
            },
            GradientStop {
                position: -4.0,
                color: grey(0.5),
                opacity: f32::NAN,
            },
        ])
        .unwrap();
        let p: Vec<_> = s.stops().iter().map(|s| (s.position, s.opacity)).collect();
        // NaN and −4 both become 0 and keep their order.
        assert_eq!(p, [(0.0, 1.0), (0.0, 0.0), (0.9, 1.0)]);
        assert_eq!(s.stops()[0].color, Color::BLACK);
        let many = GradientStops::new(
            (0..200).map(|i| GradientStop::new(i as f32 / 200.0, Color::BLACK, 1.0)),
        );
        assert_eq!(many.unwrap().stops().len(), MAX_STOPS);
    }

    #[test]
    fn endpoints_and_midpoints_in_each_space() {
        let bw = GradientStops::two(Color::BLACK, Color::WHITE);
        for mixing in [Mixing::Standard, Mixing::Linear, Mixing::Perceptual] {
            let r = ramp(bw.clone(), mixing);
            assert_eq!(r.eval(0.0), Rgba::new(0.0, 0.0, 0.0, 1.0), "{mixing:?}");
            let w = r.eval(1.0);
            for c in [w.r, w.g, w.b, w.a] {
                assert!((c - 1.0).abs() < 1e-5, "{mixing:?} {w:?}");
            }
        }
        // The middle of black → white (GD6–GD8): encoded ½, linear ½
        // (0.7354 encoded), OKLab L ½ (Y = ⅛, 0.3829 encoded).
        let mid = |m| ramp(bw.clone(), m).eval(0.5).r;
        assert!((mid(Mixing::Standard) - 0.5).abs() < 1e-6);
        assert!((mid(Mixing::Linear) - srgb::from_linear(0.5)).abs() < 1e-5);
        assert!((mid(Mixing::Perceptual) - srgb::from_linear(0.125)).abs() < 1e-4);
    }

    #[test]
    fn red_to_blue_matches_the_references_measured_middles() {
        // GD6–GD8 at the drag's middle, where the eased position is 0.494
        // (the fitted phase, see the parity record).
        let rb = GradientStops::two(Color::new(1.0, 0.0, 0.0), Color::new(0.0, 0.0, 1.0));
        let at = |m: Mixing| {
            let p = ramp(rb.clone(), m);
            // Invert the easing: the position whose eased value is 0.494.
            let mut lo = 0.0;
            let mut hi = 1.0;
            for _ in 0..40 {
                let u = (lo + hi) / 2.0;
                if ease(u) < 0.494 { lo = u } else { hi = u }
            }
            let c = p.eval(lo);
            [c.r, c.g, c.b].map(|v| v * 255.0)
        };
        let close = |a: [f32; 3], b: [f32; 3], tol: f32| {
            a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= tol)
        };
        assert!(
            close(at(Mixing::Standard), [128.1, 0.0, 125.9], 1.5),
            "{:?}",
            at(Mixing::Standard)
        );
        assert!(
            close(at(Mixing::Linear), [187.8, 0.0, 186.5], 1.5),
            "{:?}",
            at(Mixing::Linear)
        );
        assert!(
            close(at(Mixing::Perceptual), [140.8, 82.5, 161.2], 1.5),
            "{:?}",
            at(Mixing::Perceptual)
        );
    }

    #[test]
    fn segments_are_eased_on_their_own_and_equal_positions_make_a_hard_edge() {
        let s = GradientStops::new([
            GradientStop::new(0.0, Color::BLACK, 1.0),
            GradientStop::new(0.5, grey(0.5), 1.0),
            GradientStop::new(0.5, Color::new(1.0, 0.0, 0.0), 1.0),
            GradientStop::new(1.0, Color::WHITE, 1.0),
        ])
        .unwrap();
        let r = ramp(s, Mixing::Standard);
        assert!((r.eval(0.125).r - 0.5 * ease(0.25)).abs() < 1e-6);
        assert!((r.eval(0.4999).r - 0.5).abs() < 1e-3);
        assert_eq!(r.eval(0.5), Rgba::new(1.0, 0.0, 0.0, 1.0));
        assert!((r.eval(0.75).g - ease(0.5)).abs() < 1e-6);
    }

    #[test]
    fn colour_and_alpha_are_straight_and_a_transparent_stop_has_no_colour() {
        let red = Color::new(1.0, 0.0, 0.0);
        // Red → white at opacity ½ (GD12): straight colour and alpha both
        // follow the easing independently.
        let half = ramp(
            GradientStops::new([
                GradientStop::new(0.0, red, 1.0),
                GradientStop::new(1.0, Color::WHITE, 0.5),
            ])
            .unwrap(),
            Mixing::Standard,
        );
        let c = half.eval(0.5);
        assert!(
            (c.g - 0.5).abs() < 1e-6 && (c.a - 0.75).abs() < 1e-6,
            "{c:?}"
        );
        // Red → white at opacity 0 (GD11): stays red.
        let gone = ramp(
            GradientStops::new([
                GradientStop::new(0.0, red, 1.0),
                GradientStop::new(1.0, Color::WHITE, 0.0),
            ])
            .unwrap(),
            Mixing::Perceptual,
        );
        for t in [0.0, 0.3, 0.5, 0.99] {
            let c = gone.eval(t);
            assert!(
                (c.r - 1.0).abs() < 1e-5 && c.g.abs() < 1e-5 && c.b.abs() < 1e-5,
                "{t}: {c:?}"
            );
            assert!((c.a - (1.0 - ease(t))).abs() < 1e-6);
        }
        // And from a transparent stop the colour is the far end's.
        let from = ramp(
            GradientStops::new([
                GradientStop::new(0.0, Color::WHITE, 0.0),
                GradientStop::new(1.0, red, 1.0),
            ])
            .unwrap(),
            Mixing::Linear,
        );
        assert!((from.eval(0.2).g).abs() < 1e-6);
    }

    #[test]
    fn outside_the_stops_and_garbage_positions() {
        let s = GradientStops::new([
            GradientStop::new(0.25, Color::BLACK, 1.0),
            GradientStop::new(0.75, Color::WHITE, 1.0),
        ])
        .unwrap();
        let r = ramp(s, Mixing::Standard);
        assert_eq!(r.eval(0.1).r, 0.0);
        assert_eq!(r.eval(0.9).r, 1.0);
        assert_eq!(r.eval(f32::NAN).r, 0.0);
        assert_eq!(r.eval(f32::INFINITY).r, 1.0);
        assert_eq!(r.eval(-1.0).r, 0.0);
        let one = ramp(
            GradientStops::new([GradientStop::new(0.4, Color::WHITE, 0.3)]).unwrap(),
            Mixing::Perceptual,
        );
        for t in [0.0, 0.4, 1.0] {
            let c = one.eval(t);
            assert!((c.r - 1.0).abs() < 1e-5 && (c.a - 0.3).abs() < 1e-7);
        }
    }

    #[test]
    fn every_result_is_a_valid_colour() {
        let colours = [
            Color::BLACK,
            Color::WHITE,
            Color::new(1.0, 0.0, 0.0),
            Color::new(0.0, 1.0, 0.3),
            Color::new(0.1, 0.2, 1.0),
        ];
        for mixing in [Mixing::Standard, Mixing::Linear, Mixing::Perceptual] {
            for a in colours {
                for b in colours {
                    let r = ramp(GradientStops::two(a, b), mixing);
                    for i in 0..=64 {
                        let c = r.eval(i as f32 / 64.0);
                        for v in [c.r, c.g, c.b, c.a] {
                            assert!((0.0..=1.0).contains(&v), "{mixing:?} {a:?} {b:?} {c:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn oklab_round_trips_and_has_its_published_values() {
        // White: L = 1, a = b = 0; red's published OKLab (0.6279, 0.2249, 0.1258).
        let w = linear_to_oklab([1.0, 1.0, 1.0]);
        assert!(
            (w[0] - 1.0).abs() < 1e-4 && w[1].abs() < 1e-4 && w[2].abs() < 1e-4,
            "{w:?}"
        );
        let r = linear_to_oklab([1.0, 0.0, 0.0]);
        assert!(
            (r[0] - 0.6279).abs() < 1e-3
                && (r[1] - 0.2249).abs() < 1e-3
                && (r[2] - 0.1258).abs() < 1e-3,
            "{r:?}"
        );
        for c in [
            [0.2f32, 0.5, 0.9],
            [0.0, 0.0, 0.0],
            [1.0, 0.3, 0.0],
            [0.01, 0.02, 0.03],
        ] {
            let back = oklab_to_linear(linear_to_oklab(c));
            for (x, y) in c.iter().zip(&back) {
                assert!((x - y).abs() < 1e-4, "{c:?} {back:?}");
            }
        }
    }
}
