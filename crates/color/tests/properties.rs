//! Property tests of the paint colour model (phase 5A, invariants COL1,
//! COL6).

use proptest::prelude::*;
use tela_color::{Color, Hsv, PremulRgba};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 2000,
        ..ProptestConfig::default()
    }
}

fn unit() -> impl Strategy<Value = f32> {
    0.0f32..=1.0
}

/// Any `f32`, including NaN, infinities and values far outside `0..=1`.
fn any_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        Just(f32::NAN),
        Just(f32::INFINITY),
        Just(f32::NEG_INFINITY),
        Just(0.0),
        Just(1.0),
        -10.0f32..10.0,
        any::<f32>(),
    ]
}

fn valid(c: Color) -> bool {
    [c.r(), c.g(), c.b()]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

proptest! {
    #![proptest_config(config())]

    /// COL1: every constructor yields a valid colour.
    #[test]
    fn colors_are_always_valid(r in any_f32(), g in any_f32(), b in any_f32()) {
        prop_assert!(valid(Color::new(r, g, b)));
        let hsv = Hsv::new(r, g, b);
        prop_assert!(hsv.h.is_finite() && (0.0..360.0).contains(&hsv.h));
        prop_assert!(valid(hsv.to_color()));
    }

    /// RGB → HSV → RGB is the identity within f32 rounding.
    #[test]
    fn rgb_hsv_round_trip(r in unit(), g in unit(), b in unit()) {
        let c = Color::new(r, g, b);
        let back = c.to_hsv().to_color();
        for (x, y) in [(c.r(), back.r()), (c.g(), back.g()), (c.b(), back.b())] {
            prop_assert!((x - y).abs() <= 2e-6, "{c:?} -> {back:?}");
        }
        // Following any previous value converts back to the same colour.
        let followed = Hsv::follow(c, Hsv::new(r * 360.0, g, b)).to_color();
        for (x, y) in [(c.r(), followed.r()), (c.g(), followed.g()), (c.b(), followed.b())] {
            prop_assert!((x - y).abs() <= 2e-6, "{c:?} -> {followed:?}");
        }
    }

    /// HSV → RGB → HSV keeps hue (where defined), saturation and value.
    #[test]
    fn hsv_rgb_round_trip(h in 0.0f32..360.0, s in 0.05f32..=1.0, v in 0.05f32..=1.0) {
        let hsv = Hsv::new(h, s, v);
        let back = hsv.to_color().to_hsv();
        let dh = (back.h - hsv.h).abs();
        prop_assert!(dh.min(360.0 - dh) <= 0.01, "{hsv:?} -> {back:?}");
        prop_assert!((back.s - hsv.s).abs() <= 1e-5 && (back.v - hsv.v).abs() <= 1e-5);
    }

    /// Hex formatting and parsing are inverse on 8-bit colours.
    #[test]
    fn hex_round_trips(rgb in any::<[u8; 3]>(), lower in any::<bool>(), hash in any::<bool>()) {
        let c = Color::from_rgb8(rgb);
        let mut text = c.to_hex();
        if lower {
            text = text.to_lowercase();
        }
        if !hash {
            text.remove(0);
        }
        prop_assert_eq!(Color::from_hex(&text), Ok(c));
    }

    /// Any text gives a colour or an error, never a panic; whatever parses
    /// is six hex digits.
    #[test]
    fn hex_parsing_never_panics(text in ".{0,12}") {
        if let Ok(c) = Color::from_hex(&text) {
            prop_assert!(valid(c));
            let digits = text.trim().trim_start_matches('#');
            prop_assert_eq!(digits.len(), 6);
        }
    }

    /// COL6: picking never produces NaN or out-of-range channels, whatever
    /// the stored values; alpha 0 (or not a number) has no colour.
    #[test]
    fn picking_never_yields_nan(r in any_f32(), g in any_f32(), b in any_f32(), a in any_f32()) {
        match Color::from_premultiplied(PremulRgba::new(r, g, b, a)) {
            Some(c) => prop_assert!(valid(c) && a > 0.0),
            None => prop_assert!(a.is_nan() || a <= 0.0 || !a.is_finite()),
        }
    }

    /// Premultiply → unpremultiply returns the colour (exactly enough to
    /// round to the same 8-bit levels) whenever alpha is not tiny.
    #[test]
    fn premultiply_then_pick_returns_the_colour(rgb in any::<[u8; 3]>(), a in 0.02f32..=1.0) {
        let c = Color::from_rgb8(rgb);
        let p = c.premultiplied(a);
        prop_assert!(p.r <= p.a && p.g <= p.a && p.b <= p.a);
        prop_assert_eq!(Color::from_premultiplied(p).unwrap().to_rgb8(), rgb);
    }
}
