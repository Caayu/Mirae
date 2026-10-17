//! The colour a user paints with (phase 5A, ADR 0021).
//!
//! [`Color`] is an **opaque, straight** sRGB-encoded colour with `f32`
//! channels in `0..=1`: what the colour panel shows and edits, what the
//! eyedropper returns and what a stroke captures when it begins. It is not
//! premultiplied (it has no alpha to premultiply by) and not quantized: a
//! colour picked in a 16-bit document or set by dragging in the HSV square
//! keeps its precision until a stroke is committed, which quantizes once
//! to the document's storage. 8-bit values (`0..=255`) and hex are views
//! of it, rounded to the nearest level.
//!
//! The colour has **no alpha**, like the reference application's colour
//! (its colour UI has no alpha field and its hex field takes six digits;
//! parity record 5A, O7): how much paint lands is the brush's business
//! (opacity, flow, coverage), and transparency is painted with the eraser.
//!
//! [`Hsv`] is hue/saturation/value computed on the encoded channels (the
//! reference's HSV, O5); [`Hsv::follow`] keeps the hue and saturation of a
//! previous value where a colour leaves them undefined, so an HSV control
//! does not jump when it passes through grey or black.

use std::fmt;

use crate::{PremulRgba, Rgba};

/// An opaque sRGB-encoded colour, channels in `0..=1` (never NaN).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Color {
    r: f32,
    g: f32,
    b: f32,
}

fn channel(v: f32) -> f32 {
    // `clamp` keeps NaN; a colour never holds one (COL1).
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

impl Color {
    /// Black, the default main colour.
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };
    /// White, the default sub colour.
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };

    /// A colour from channels in `0..=1`; values outside are clamped and
    /// NaN becomes 0, so every `Color` is valid (invariant COL1).
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self {
            r: channel(r),
            g: channel(g),
            b: channel(b),
        }
    }

    /// Exactly `v / 255` per channel.
    pub fn from_rgb8([r, g, b]: [u8; 3]) -> Self {
        const INV: f32 = 1.0 / 255.0;
        Self::new(f32::from(r) * INV, f32::from(g) * INV, f32::from(b) * INV)
    }

    /// The nearest 8-bit levels (what the panel's 0–255 fields show).
    pub fn to_rgb8(self) -> [u8; 3] {
        [
            crate::unorm8(self.r),
            crate::unorm8(self.g),
            crate::unorm8(self.b),
        ]
    }

    /// Red channel.
    pub fn r(self) -> f32 {
        self.r
    }

    /// Green channel.
    pub fn g(self) -> f32 {
        self.g
    }

    /// Blue channel.
    pub fn b(self) -> f32 {
        self.b
    }

    /// The colour at full alpha (straight).
    pub fn opaque(self) -> Rgba {
        Rgba::new(self.r, self.g, self.b, 1.0)
    }

    /// The colour as premultiplied paint covering `coverage` (`0..=1`) of a
    /// pixel: `(rgb · coverage, coverage)`.
    pub fn premultiplied(self, coverage: f32) -> PremulRgba {
        PremulRgba::new(
            self.r * coverage,
            self.g * coverage,
            self.b * coverage,
            coverage,
        )
    }

    /// The visible colour of a stored (premultiplied) pixel, or `None`
    /// where it is fully transparent (alpha 0 has no colour) or not a
    /// number. Channels are divided by alpha and clamped to `0..=1`, so
    /// rounding in the stored values can never amplify past a valid
    /// colour; at very low alpha the result is as coarse as the storage
    /// (an 8-bit pixel at alpha 1/255 can only be 0 or 1 per channel).
    pub fn from_premultiplied(p: PremulRgba) -> Option<Self> {
        if p.a.is_nan() || p.a <= 0.0 || !p.a.is_finite() {
            return None;
        }
        let inv = 1.0 / p.a;
        let c = |v: f32| if v.is_finite() { v * inv } else { 0.0 };
        Some(Self::new(c(p.r), c(p.g), c(p.b)))
    }

    /// `#RRGGBB`, upper case, from the nearest 8-bit levels.
    pub fn to_hex(self) -> String {
        let [r, g, b] = self.to_rgb8();
        format!("#{r:02X}{g:02X}{b:02X}")
    }

    /// Parses `RRGGBB` or `#RRGGBB` (either case, surrounding spaces
    /// ignored). Anything else — including an alpha pair, which a colour
    /// does not have — is an error.
    pub fn from_hex(text: &str) -> Result<Self, HexError> {
        let t = text.trim();
        let digits = t.strip_prefix('#').unwrap_or(t);
        let count = digits.chars().count();
        if count != 6 {
            return Err(HexError::Length(count));
        }
        // Checked here because `from_str_radix` also accepts a sign.
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(HexError::Digit);
        }
        let mut rgb = [0u8; 3];
        for (i, v) in rgb.iter_mut().enumerate() {
            *v = u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).map_err(|_| HexError::Digit)?;
        }
        Ok(Self::from_rgb8(rgb))
    }

    /// Hue, saturation and value of the encoded channels.
    pub fn to_hsv(self) -> Hsv {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let d = max - min;
        let s = if max > 0.0 { d / max } else { 0.0 };
        let h = if d <= 0.0 {
            0.0
        } else if max == self.r {
            60.0 * ((self.g - self.b) / d).rem_euclid(6.0)
        } else if max == self.g {
            60.0 * ((self.b - self.r) / d + 2.0)
        } else {
            60.0 * ((self.r - self.g) / d + 4.0)
        };
        Hsv::new(h, s, max)
    }
}

/// Why a hex colour did not parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HexError {
    /// Not six hex digits (the number found, after an optional `#`).
    Length(usize),
    /// A character that is not a hex digit.
    Digit,
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexError::Length(n) => write!(f, "expected 6 hex digits (#RRGGBB), found {n}"),
            HexError::Digit => f.write_str("not a hex digit"),
        }
    }
}

impl std::error::Error for HexError {}

/// Hue (degrees, `0..360`), saturation and value (`0..=1`) of an
/// sRGB-encoded colour.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Hsv {
    /// Degrees in `0..360`.
    pub h: f32,
    /// `0..=1`.
    pub s: f32,
    /// `0..=1`.
    pub v: f32,
}

impl Hsv {
    /// Wraps the hue into `0..360` and clamps the rest (NaN becomes 0).
    pub fn new(h: f32, s: f32, v: f32) -> Self {
        let h = if h.is_finite() {
            h.rem_euclid(360.0)
        } else {
            0.0
        };
        // rem_euclid can round up to exactly 360 for tiny negative input.
        let h = if h >= 360.0 { 0.0 } else { h };
        Self {
            h,
            s: channel(s),
            v: channel(v),
        }
    }

    /// The colour (standard HSV → RGB on encoded values).
    pub fn to_color(self) -> Color {
        let Self { h, s, v } = Self::new(self.h, self.s, self.v);
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
        let (r, g, b) = match (h / 60.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = v - c;
        Color::new(r + m, g + m, b + m)
    }

    /// `color` in HSV, keeping `previous`'s hue where `color` has none
    /// (grey, black) and its saturation where `color` has none (black), so
    /// a control that follows the colour does not jump. Converting the
    /// result back gives `color` (within `f32` rounding).
    pub fn follow(color: Color, previous: Hsv) -> Hsv {
        let hsv = color.to_hsv();
        let grey = color.r == color.g && color.g == color.b;
        Hsv::new(
            if grey { previous.h } else { hsv.h },
            if hsv.v <= 0.0 { previous.s } else { hsv.s },
            hsv.v,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Color, b: Color, tol: f32) -> bool {
        (a.r - b.r).abs() <= tol && (a.g - b.g).abs() <= tol && (a.b - b.b).abs() <= tol
    }

    #[test]
    fn colors_are_always_valid() {
        let c = Color::new(f32::NAN, 2.0, -1.0);
        assert_eq!((c.r(), c.g(), c.b()), (0.0, 1.0, 0.0));
        let c = Color::new(f32::INFINITY, f32::NEG_INFINITY, 0.25);
        assert_eq!((c.r(), c.g(), c.b()), (1.0, 0.0, 0.25));
    }

    #[test]
    fn eight_bit_values_round_trip_exactly() {
        for v in 0..=255u8 {
            let rgb = [v, 255 - v, v / 3];
            assert_eq!(Color::from_rgb8(rgb).to_rgb8(), rgb);
        }
    }

    #[test]
    fn hex_parses_and_formats() {
        assert_eq!(
            Color::from_hex("#2595D5").unwrap().to_rgb8(),
            [37, 149, 213]
        );
        assert_eq!(
            Color::from_hex(" 2595d5 ").unwrap().to_rgb8(),
            [37, 149, 213]
        );
        assert_eq!(Color::from_rgb8([37, 149, 213]).to_hex(), "#2595D5");
        assert_eq!(Color::from_rgb8([255, 128, 0]).to_hex(), "#FF8000");
        assert_eq!(Color::BLACK.to_hex(), "#000000");
    }

    #[test]
    fn malformed_hex_is_an_error_not_a_panic() {
        for bad in [
            "",
            "#",
            "#FFF",
            "#FF800080",
            "FF80",
            "GG0000",
            "#12345G",
            "##123456",
            "+12345",
            "-12345",
            "ééé",
            "#ééé",
            "12 456",
            "0x1234",
        ] {
            assert!(Color::from_hex(bad).is_err(), "{bad:?}");
        }
        assert_eq!(Color::from_hex("#FF800080"), Err(HexError::Length(8)));
    }

    #[test]
    fn hsv_matches_the_reference_readout() {
        // The reference shows 37,149,213 as h 202°, S 83 %, V 84 % (O5).
        let hsv = Color::from_rgb8([37, 149, 213]).to_hsv();
        assert_eq!(hsv.h.round(), 202.0);
        assert_eq!((hsv.s * 100.0).round(), 83.0);
        assert_eq!((hsv.v * 100.0).round(), 84.0);
        // Primaries and secondaries.
        for (rgb, h) in [
            ([255, 0, 0], 0.0),
            ([255, 255, 0], 60.0),
            ([0, 255, 0], 120.0),
            ([0, 255, 255], 180.0),
            ([0, 0, 255], 240.0),
            ([255, 0, 255], 300.0),
        ] {
            let c = Color::from_rgb8(rgb);
            let hsv = c.to_hsv();
            assert_eq!((hsv.h, hsv.s, hsv.v), (h, 1.0, 1.0), "{rgb:?}");
            assert_eq!(hsv.to_color(), c);
        }
    }

    #[test]
    fn hsv_inputs_are_exact_not_quantized() {
        // h 120, S 50 %, V 50 %: (0.25, 0.5, 0.25), shown as 64,128,64.
        let c = Hsv::new(120.0, 0.5, 0.5).to_color();
        assert!(close(c, Color::new(0.25, 0.5, 0.25), 1e-6));
        assert_eq!(c.to_rgb8(), [64, 128, 64]);
    }

    #[test]
    fn hsv_wraps_and_clamps() {
        assert_eq!(Hsv::new(-30.0, 2.0, f32::NAN), Hsv::new(330.0, 1.0, 0.0));
        assert_eq!(Hsv::new(720.0, 0.5, 0.5).h, 0.0);
        assert_eq!(Hsv::new(f32::NAN, 0.5, 0.5).h, 0.0);
        assert!(Hsv::new(-1e-9, 0.5, 0.5).h < 360.0);
    }

    #[test]
    fn following_keeps_undefined_hue_and_saturation() {
        let previous = Hsv::new(200.0, 0.7, 0.9);
        let grey = Hsv::follow(Color::new(0.5, 0.5, 0.5), previous);
        assert_eq!((grey.h, grey.s, grey.v), (200.0, 0.0, 0.5));
        let black = Hsv::follow(Color::BLACK, previous);
        assert_eq!((black.h, black.s, black.v), (200.0, 0.7, 0.0));
        assert_eq!(black.to_color(), Color::BLACK);
        let red = Hsv::follow(Color::new(1.0, 0.0, 0.0), previous);
        assert_eq!((red.h, red.s, red.v), (0.0, 1.0, 1.0));
    }

    #[test]
    fn picking_unpremultiplies_safely() {
        // The four alphas of the 5A brief, at 8 bits.
        let straight = [220u8, 80, 40];
        let at = |a: u8| {
            let p = Rgba::new(
                f32::from(straight[0]) / 255.0,
                f32::from(straight[1]) / 255.0,
                f32::from(straight[2]) / 255.0,
                f32::from(a) / 255.0,
            )
            .premultiply()
            .to_rgba8();
            Color::from_premultiplied(PremulRgba::from_rgba8(p))
        };
        assert_eq!(at(255).unwrap().to_rgb8(), straight);
        let half = at(128).unwrap().to_rgb8();
        for (h, s) in half.iter().zip(straight) {
            assert!(h.abs_diff(s) <= 1, "{half:?}");
        }
        // Alpha 1/255: premultiplied channels are 0 or 1/255, so the colour
        // is as coarse as the storage, but valid.
        let faint = at(1).unwrap();
        assert_eq!(faint.to_rgb8(), [255, 0, 0]);
        assert_eq!(at(0), None, "alpha 0 has no colour");
    }

    #[test]
    fn picking_garbage_never_gives_nan() {
        for p in [
            PremulRgba::new(f32::NAN, 0.5, 0.5, 0.5),
            PremulRgba::new(0.5, f32::INFINITY, 0.5, 0.5),
            PremulRgba::new(0.9, 0.9, 0.9, 1e-30),
            PremulRgba::new(2.0, -1.0, 0.5, 0.25),
        ] {
            let c = Color::from_premultiplied(p).unwrap();
            for v in [c.r(), c.g(), c.b()] {
                assert!((0.0..=1.0).contains(&v), "{p:?} -> {c:?}");
            }
        }
        for a in [0.0, -0.5, f32::NAN, f32::INFINITY] {
            assert_eq!(
                Color::from_premultiplied(PremulRgba::new(0.1, 0.1, 0.1, a)),
                None
            );
        }
    }

    #[test]
    fn premultiplied_paint_is_valid() {
        let c = Color::from_rgb8([37, 149, 213]);
        for k in [0.0, 0.25, 0.5, 1.0] {
            let p = c.premultiplied(k);
            assert!(p.r <= p.a && p.g <= p.a && p.b <= p.a && p.a == k);
        }
        assert_eq!(c.opaque().premultiply(), c.premultiplied(1.0));
    }
}
