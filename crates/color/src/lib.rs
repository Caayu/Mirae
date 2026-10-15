//! Color representation and compositing math.
//!
//! **Responsibility:** color value types, alpha conventions, Porter-Duff
//! `over`, 8- and 16-bit quantization and the stored pixel formats
//! ([`StoragePixel`], [`PixelDepth`]), sRGB transfer functions.
//! **Does not:** know about pixels in space (tiles, layers) or any device.
//! **Depends on:** nothing. **Used by:** every crate that touches pixels.
//!
//! Colors are sRGB-encoded for now (blending happens in encoded space, which is
//! what most painting software does by default). Linear-light blending can be
//! added later by converting at the compositing boundary with the transfer
//! functions in [`srgb`].
//!
//! Two alpha conventions are kept as distinct types so they cannot be mixed up:
//! [`Rgba`] is straight (unassociated) alpha, used for user-facing colors;
//! [`PremulRgba`] is premultiplied alpha, used for all compositing
//! (see `docs/adr/0001-premultiplied-alpha.md`).
//!
//! Invariants: [`PremulRgba`] holds `rgb <= a` when built through
//! [`Rgba::premultiply`]; `over` is associative; RGBA8 and RGBA16 round
//! trips are exact; 8 → 16 → 8 is the identity.

pub mod srgb;
mod storage;

pub use storage::{PixelDepth, StoragePixel, narrow16, unorm16, widen8};

/// Straight-alpha color, components in `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[allow(missing_docs)] // channels
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn premultiply(self) -> PremulRgba {
        PremulRgba::new(self.r * self.a, self.g * self.a, self.b * self.a, self.a)
    }
}

/// Premultiplied-alpha color, components in `0.0..=1.0` with `rgb <= a`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[allow(missing_docs)] // channels
pub struct PremulRgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl PremulRgba {
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Porter-Duff "source over destination".
    pub fn over(self, dst: Self) -> Self {
        let k = 1.0 - self.a;
        Self::new(
            self.r + dst.r * k,
            self.g + dst.g * k,
            self.b + dst.b * k,
            self.a + dst.a * k,
        )
    }

    /// Multiplies every component, i.e. scales coverage/opacity.
    pub fn scale(self, s: f32) -> Self {
        Self::new(self.r * s, self.g * s, self.b * s, self.a * s)
    }

    /// Removes the alpha from the color channels. Fully transparent input
    /// yields transparent black, since its hue is undefined.
    pub fn unpremultiply(self) -> Rgba {
        if self.a <= 0.0 {
            return Rgba::TRANSPARENT;
        }
        let inv = 1.0 / self.a;
        Rgba::new(self.r * inv, self.g * inv, self.b * inv, self.a)
    }

    pub fn to_rgba8(self) -> [u8; 4] {
        [
            unorm8(self.r),
            unorm8(self.g),
            unorm8(self.b),
            unorm8(self.a),
        ]
    }

    pub fn from_rgba8(p: [u8; 4]) -> Self {
        const INV: f32 = 1.0 / 255.0;
        Self::new(
            p[0] as f32 * INV,
            p[1] as f32 * INV,
            p[2] as f32 * INV,
            p[3] as f32 * INV,
        )
    }

    /// Quantizes to 16 bits per channel (round to nearest, clamped).
    pub fn to_rgba16(self) -> [u16; 4] {
        [
            unorm16(self.r),
            unorm16(self.g),
            unorm16(self.b),
            unorm16(self.a),
        ]
    }

    /// Decodes 16-bit channels. A widened 8-bit pixel (`v·257`) decodes to
    /// exactly the values of [`PremulRgba::from_rgba8`].
    pub fn from_rgba16(p: [u16; 4]) -> Self {
        Self::new(
            storage::decode16(p[0]),
            storage::decode16(p[1]),
            storage::decode16(p[2]),
            storage::decode16(p[3]),
        )
    }
}

/// Quantizes `0.0..=1.0` to a byte with round-to-nearest.
pub fn unorm8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: PremulRgba, b: PremulRgba) -> bool {
        [a.r - b.r, a.g - b.g, a.b - b.b, a.a - b.a]
            .iter()
            .all(|d| d.abs() < 1e-6)
    }

    #[test]
    fn transparent_is_identity_for_over() {
        let c = Rgba::new(0.2, 0.4, 0.6, 0.5).premultiply();
        assert!(approx(PremulRgba::TRANSPARENT.over(c), c));
        assert!(approx(c.over(PremulRgba::TRANSPARENT), c));
    }

    #[test]
    fn opaque_source_replaces_destination() {
        let src = Rgba::new(1.0, 0.0, 0.0, 1.0).premultiply();
        let dst = Rgba::new(0.0, 0.0, 1.0, 1.0).premultiply();
        assert!(approx(src.over(dst), src));
    }

    #[test]
    fn over_is_associative() {
        let a = Rgba::new(0.9, 0.1, 0.3, 0.4).premultiply();
        let b = Rgba::new(0.2, 0.8, 0.5, 0.6).premultiply();
        let c = Rgba::new(0.1, 0.2, 0.9, 0.7).premultiply();
        assert!(approx(a.over(b).over(c), a.over(b.over(c))));
    }

    #[test]
    fn premultiply_round_trip() {
        let c = Rgba::new(0.25, 0.5, 0.75, 0.5);
        let back = c.premultiply().unpremultiply();
        assert!((back.r - c.r).abs() < 1e-6 && (back.b - c.b).abs() < 1e-6);
    }

    #[test]
    fn rgba8_round_trip_is_exact() {
        for v in 0..=255u8 {
            let p = [v, v / 2, v / 3, 255];
            assert_eq!(PremulRgba::from_rgba8(p).to_rgba8(), p);
        }
    }
}
