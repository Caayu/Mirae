//! Stored pixel formats: premultiplied, sRGB-encoded RGBA at 8 or 16 bits
//! per channel (docs/adr/0018).
//!
//! Both formats hold the **same** values as [`PremulRgba`] — the same
//! premultiplication, the same encoding — only quantized more or less
//! finely. Converting between them changes precision, never appearance:
//!
//! - 8 → 16 is exact: `v16 = v8 · 257` (so `v16 / 65535 == v8 / 255`), and
//!   decoding a widened value gives bit-identical `f32` channels.
//! - 16 → 8 rounds to the nearest level: `v8 = (v16 + 128) / 257` (no ties
//!   exist), the same result as decoding to `f32` and quantizing with
//!   [`unorm8`](crate::unorm8). 8 → 16 → 8 is the identity.

use std::fmt::Debug;

use crate::PremulRgba;

/// Bits per channel of a document's stored pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PixelDepth {
    /// `[u8; 4]`, 4 bytes per pixel.
    #[default]
    Rgba8,
    /// `[u16; 4]`, 8 bytes per pixel.
    Rgba16,
}

impl PixelDepth {
    /// Both depths.
    pub const ALL: [PixelDepth; 2] = [PixelDepth::Rgba8, PixelDepth::Rgba16];

    /// Bits per channel.
    pub const fn bits(self) -> u32 {
        match self {
            PixelDepth::Rgba8 => 8,
            PixelDepth::Rgba16 => 16,
        }
    }

    /// Bytes per stored pixel.
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            PixelDepth::Rgba8 => 4,
            PixelDepth::Rgba16 => 8,
        }
    }
}

/// A stored pixel format: premultiplied sRGB-encoded RGBA with integer
/// channels. Implemented for `[u8; 4]` and `[u16; 4]` only; code generic
/// over it is monomorphized once per depth (no dynamic dispatch per
/// pixel).
pub trait StoragePixel: Copy + Default + PartialEq + Debug + Send + Sync + 'static {
    /// The depth this format stores.
    const DEPTH: PixelDepth;

    /// Decodes to the compositing representation (channels in `0..=1`).
    fn to_premul(self) -> PremulRgba;

    /// Quantizes from the compositing representation (round to nearest,
    /// channels clamped to `0..=1`).
    fn from_premul(c: PremulRgba) -> Self;

    /// The exact equivalent of an 8-bit pixel (see the module docs).
    fn from_rgba8(p: [u8; 4]) -> Self;

    /// The nearest 8-bit pixel (see the module docs).
    fn to_rgba8(self) -> [u8; 4];

    /// Quantizes with a dither threshold in `0..1`: every channel becomes
    /// `⌊v · max + threshold⌋` (channels clamped to `0..=1`). A threshold
    /// of ½ is [`StoragePixel::from_premul`] exactly; thresholds spread
    /// evenly over `0..1` keep the mean (see [`dither_threshold`]). All
    /// four channels use the same threshold, so `rgb <= a` survives.
    fn from_premul_dithered(c: PremulRgba, threshold: f32) -> Self;
}

/// An 8 × 8 ordered-dither (Bayer) matrix: each value `0..64` once.
const BAYER8: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// The dither threshold of document pixel `(x, y)` for
/// [`StoragePixel::from_premul_dithered`]: `(B + ½) / 64` with `B` the
/// 8 × 8 Bayer matrix, anchored to document coordinates (the same pixel
/// always gets the same threshold, whatever the tile or the drag). The 64
/// thresholds average ½, so dithering keeps the mean (phase 5F; Mirae's
/// own pattern, not the reference's, D-5F-4).
#[inline]
pub fn dither_threshold(x: i32, y: i32) -> f32 {
    let b = BAYER8[(y & 7) as usize][(x & 7) as usize];
    (f32::from(b) + 0.5) / 64.0
}

#[inline]
fn dithered(v: f32, max: f32, threshold: f32) -> f32 {
    // `clamp` keeps NaN; the casts below turn it into 0.
    v.clamp(0.0, 1.0) * max + threshold
}

impl StoragePixel for [u8; 4] {
    const DEPTH: PixelDepth = PixelDepth::Rgba8;

    #[inline]
    fn to_premul(self) -> PremulRgba {
        PremulRgba::from_rgba8(self)
    }

    #[inline]
    fn from_premul(c: PremulRgba) -> Self {
        c.to_rgba8()
    }

    #[inline]
    fn from_rgba8(p: [u8; 4]) -> Self {
        p
    }

    #[inline]
    fn to_rgba8(self) -> [u8; 4] {
        self
    }

    #[inline]
    fn from_premul_dithered(c: PremulRgba, threshold: f32) -> Self {
        let q = |v: f32| dithered(v, 255.0, threshold) as u8;
        [q(c.r), q(c.g), q(c.b), q(c.a)]
    }
}

impl StoragePixel for [u16; 4] {
    const DEPTH: PixelDepth = PixelDepth::Rgba16;

    #[inline]
    fn to_premul(self) -> PremulRgba {
        PremulRgba::from_rgba16(self)
    }

    #[inline]
    fn from_premul(c: PremulRgba) -> Self {
        c.to_rgba16()
    }

    #[inline]
    fn from_rgba8(p: [u8; 4]) -> Self {
        p.map(widen8)
    }

    #[inline]
    fn to_rgba8(self) -> [u8; 4] {
        self.map(narrow16)
    }

    #[inline]
    fn from_premul_dithered(c: PremulRgba, threshold: f32) -> Self {
        let q = |v: f32| dithered(v, 65535.0, threshold) as u16;
        [q(c.r), q(c.g), q(c.b), q(c.a)]
    }
}

/// Evaluates `$body` with `$P` naming the [`StoragePixel`] type of a
/// [`PixelDepth`] known only at run time: the one place where a depth
/// value becomes a type, so generic code (`fn f<P: StoragePixel>`) is
/// monomorphized once per depth and chosen once per call, not per pixel.
///
/// ```
/// use tela_color::{PixelDepth, StoragePixel, with_pixel_type};
/// fn bytes<P: StoragePixel>() -> usize { size_of::<P>() }
/// let depth = PixelDepth::Rgba16;
/// assert_eq!(with_pixel_type!(depth, P => bytes::<P>()), 8);
/// ```
#[macro_export]
macro_rules! with_pixel_type {
    ($depth:expr, $P:ident => $body:expr) => {
        match $depth {
            $crate::PixelDepth::Rgba8 => {
                #[allow(dead_code)]
                type $P = [u8; 4];
                $body
            }
            $crate::PixelDepth::Rgba16 => {
                #[allow(dead_code)]
                type $P = [u16; 4];
                $body
            }
        }
    };
}

/// 8-bit to 16-bit channel, exact: `v · 257`.
#[inline]
pub const fn widen8(v: u8) -> u16 {
    v as u16 * 257
}

/// 16-bit to 8-bit channel, rounded to the nearest level:
/// `(v + 128) / 257`. Exact inverse of [`widen8`].
#[inline]
pub const fn narrow16(v: u16) -> u8 {
    ((v as u32 + 128) / 257) as u8
}

/// Quantizes `0.0..=1.0` to 16 bits with round-to-nearest.
#[inline]
pub fn unorm16(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
}

/// Decodes a 16-bit channel. Two multiplications instead of one so that
/// widened 8-bit values decode to exactly the `f32` of
/// [`PremulRgba::from_rgba8`] (`v·257 / 257` is exact); one multiplication
/// by `1/65535` differs in the last bit for 134 of the 256 levels.
#[inline]
pub(crate) fn decode16(v: u16) -> f32 {
    const INV257: f32 = 1.0 / 257.0;
    const INV255: f32 = 1.0 / 255.0;
    (f32::from(v) * INV257) * INV255
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unorm8;

    #[test]
    fn widening_every_8_bit_value_is_exact() {
        for v in 0..=255u8 {
            let w = widen8(v);
            assert_eq!(u32::from(w), u32::from(v) * 65535 / 255, "{v}");
            assert_eq!(narrow16(w), v, "8 -> 16 -> 8 is the identity");
        }
        assert_eq!(widen8(255), u16::MAX);
    }

    #[test]
    fn widened_values_decode_to_the_same_f32_as_8_bit() {
        for v in 0..=255u8 {
            let p = [v, v / 2, v / 3, 255];
            let a = PremulRgba::from_rgba8(p);
            let b = <[u16; 4]>::from_rgba8(p).to_premul();
            for (x, y) in [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)] {
                assert_eq!(x.to_bits(), y.to_bits(), "{p:?}");
            }
        }
    }

    #[test]
    fn narrowing_rounds_to_the_nearest_level_like_unorm8() {
        for v in 0..=u16::MAX {
            let n = narrow16(v);
            assert_eq!(n, unorm8(decode16(v)), "{v}");
            let err = (f64::from(v) / 257.0 - f64::from(n)).abs();
            assert!(err <= 0.5, "{v}: error {err}");
        }
    }

    #[test]
    fn every_16_bit_value_round_trips_through_f32() {
        let mut previous = -1.0;
        for v in 0..=u16::MAX {
            let f = decode16(v);
            assert!(f > previous, "decoding is strictly increasing at {v}");
            previous = f;
            assert_eq!(unorm16(f), v);
        }
        assert_eq!(decode16(u16::MAX), 1.0);
        assert_eq!(decode16(0), 0.0);
    }

    #[test]
    fn quantization_clamps_out_of_range_values() {
        assert_eq!(unorm16(-0.5), 0);
        assert_eq!(unorm16(2.0), u16::MAX);
        assert_eq!(unorm16(f32::NAN), 0, "NaN saturates to 0 in the cast");
        let c = PremulRgba::new(1.5, -1.0, 0.25, 1.0);
        assert_eq!(<[u16; 4]>::from_premul(c), [65535, 0, 16384, 65535]);
    }

    #[test]
    fn premultiplied_order_survives_both_conversions() {
        // rgb <= a holds before, so it holds after (both maps are monotone).
        for a in (0..=255u8).step_by(5) {
            for c in (0..=a).step_by(3) {
                let p16 = <[u16; 4]>::from_rgba8([c, c, c, a]);
                assert!(p16[0] <= p16[3]);
            }
        }
        for a in (0..=u16::MAX).step_by(97) {
            for c in (0..=a).step_by(1031) {
                let p8 = [c, c, c, a].to_rgba8();
                assert!(p8[0] <= p8[3], "{c} {a}");
            }
        }
    }

    #[test]
    fn a_half_threshold_is_plain_rounding() {
        for i in 0..=4096 {
            let v = i as f32 / 4096.0;
            let c = PremulRgba::new(v * 0.3, v * 0.6, v * 0.9, v);
            assert_eq!(
                <[u8; 4]>::from_premul_dithered(c, 0.5),
                <[u8; 4]>::from_premul(c)
            );
            assert_eq!(
                <[u16; 4]>::from_premul_dithered(c, 0.5),
                <[u16; 4]>::from_premul(c)
            );
        }
        let odd = PremulRgba::new(f32::NAN, -1.0, 2.0, 1.0);
        assert_eq!(<[u8; 4]>::from_premul_dithered(odd, 0.99), [0, 0, 255, 255]);
        assert_eq!(
            <[u16; 4]>::from_premul_dithered(odd, 0.0),
            [0, 0, 65535, 65535]
        );
    }

    #[test]
    fn the_dither_matrix_is_a_permutation_with_a_mean_of_one_half() {
        let mut seen = [false; 64];
        let mut sum = 0.0;
        for y in 0..8 {
            for x in 0..8 {
                let t = dither_threshold(x, y);
                let b = (t * 64.0 - 0.5).round() as usize;
                assert!(!seen[b], "{x} {y}");
                seen[b] = true;
                sum += t;
                // Anchored to document coordinates, negative ones included.
                assert_eq!(t, dither_threshold(x - 8, y + 16));
            }
        }
        assert!((sum / 64.0 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn dithering_keeps_the_mean_and_premultiplied_order() {
        for i in 0..=200 {
            let v = i as f32 / 200.0 * 0.37;
            let c = PremulRgba::new(v * 0.5, v, v, (v * 1.7).min(1.0));
            let mut mean = [0.0f64; 4];
            for y in 0..8 {
                for x in 0..8 {
                    let q = <[u8; 4]>::from_premul_dithered(c, dither_threshold(x, y));
                    assert!(q[0] <= q[3] && q[1] <= q[3] && q[2] <= q[3], "{c:?}");
                    for k in 0..4 {
                        mean[k] += f64::from(q[k]) / 64.0;
                    }
                }
            }
            for (k, want) in [c.r, c.g, c.b, c.a].iter().enumerate() {
                // Exact to a 64th of a level (the matrix's resolution).
                assert!(
                    (mean[k] - f64::from(*want) * 255.0).abs() <= 1.0 / 64.0 + 1e-4,
                    "{c:?} {mean:?}"
                );
            }
        }
    }

    #[test]
    fn depth_sizes() {
        assert_eq!(PixelDepth::Rgba8.bytes_per_pixel(), size_of::<[u8; 4]>());
        assert_eq!(PixelDepth::Rgba16.bytes_per_pixel(), size_of::<[u16; 4]>());
        assert_eq!(<[u8; 4]>::DEPTH.bits(), 8);
        assert_eq!(<[u16; 4]>::DEPTH.bits(), 16);
        assert_eq!(PixelDepth::default(), PixelDepth::Rgba8);
    }
}
