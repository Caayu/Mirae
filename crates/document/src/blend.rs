//! Layer blend modes (ADR 0016).
//!
//! Every mode except `Normal` uses the general formula of W3C *Compositing
//! and Blending Level 1* (§5.3, source-over with a blend function):
//!
//! ```text
//! Cs' = (1 − αb)·Cs + αb·B(Cb, Cs)
//! co  = αs·Cs' + (1 − αs)·αb·Cb        (premultiplied result)
//! αo  = αs + αb·(1 − αs)
//! ```
//!
//! where `Cb`, `Cs` are un-premultiplied colours, `αs` includes the layer
//! opacity, and `B` is the mode's blend function (§9 separable, §10
//! non-separable). `Add` uses the additive blend function
//! `B = min(1, Cb + Cs)` ("linear dodge") in the same formula.
//!
//! Colours are blended as stored: **sRGB-encoded** values, not linear
//! light. That is what the reference application does: measured against
//! it, this formula matches every one of 3 400 probed combinations within
//! 1/255 (2/255 for `Color` and `Luminosity`), while blending in linear
//! light misses by up to 196/255 (docs/parity/4C-blend.md).
//!
//! `Normal` keeps its original code path (`over`), bit-identical to the
//! compositor before blend modes existed. A fully transparent source
//! pixel leaves the destination untouched in every mode.

use tela_color::PremulRgba;

/// How a layer combines with what is below it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    /// Porter-Duff source-over.
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    /// Additive: `B = min(1, Cb + Cs)`.
    Add,
}

impl BlendMode {
    /// Every mode, in menu order.
    pub const ALL: [BlendMode; 17] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::Add,
    ];

    /// Stable lowercase name (W3C keyword where one exists), for UI labels
    /// and future file formats.
    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "normal",
            BlendMode::Multiply => "multiply",
            BlendMode::Screen => "screen",
            BlendMode::Overlay => "overlay",
            BlendMode::Darken => "darken",
            BlendMode::Lighten => "lighten",
            BlendMode::ColorDodge => "color-dodge",
            BlendMode::ColorBurn => "color-burn",
            BlendMode::HardLight => "hard-light",
            BlendMode::SoftLight => "soft-light",
            BlendMode::Difference => "difference",
            BlendMode::Exclusion => "exclusion",
            BlendMode::Hue => "hue",
            BlendMode::Saturation => "saturation",
            BlendMode::Color => "color",
            BlendMode::Luminosity => "luminosity",
            BlendMode::Add => "add",
        }
    }

    /// Blends a **clipped** layer onto its clipping group in place:
    /// source-atop (the group keeps its alpha) with the blend function's
    /// result replacing the source colour as if the group were opaque,
    /// `co = αs·αb·B(Cb, Cs) + (1 − αs)·cb`, `αo = αb`. This is what the
    /// reference application does (docs/parity/4D-masks-clipping-alpha-lock.md,
    /// K1; ADR 0017); the W3C source-atop mix does not match it.
    ///
    /// Same blend functions as [`BlendMode::composite`], in loops of its
    /// own: sharing the loops through a generic made `composite` up to
    /// 16 % slower in several modes (measured with `blend_bench`).
    pub fn clip_atop(self, src: &[PremulRgba], opacity: f32, dst: &mut [PremulRgba]) {
        match self {
            BlendMode::Normal => {
                for (d, s) in dst.iter_mut().zip(src) {
                    if s.a > 0.0 && d.a > 0.0 {
                        let s = s.scale(opacity);
                        let keep = 1.0 - s.a;
                        *d = PremulRgba::new(
                            s.r * d.a + d.r * keep,
                            s.g * d.a + d.g * keep,
                            s.b * d.a + d.b * keep,
                            d.a,
                        );
                    }
                }
            }
            BlendMode::Multiply => separable_atop(src, opacity, dst, |b, s| b * s),
            BlendMode::Screen => separable_atop(src, opacity, dst, screen),
            BlendMode::Overlay => separable_atop(src, opacity, dst, |b, s| hard_light(s, b)),
            BlendMode::Darken => separable_atop(src, opacity, dst, f32::min),
            BlendMode::Lighten => separable_atop(src, opacity, dst, f32::max),
            BlendMode::ColorDodge => separable_atop(src, opacity, dst, color_dodge),
            BlendMode::ColorBurn => separable_atop(src, opacity, dst, color_burn),
            BlendMode::HardLight => separable_atop(src, opacity, dst, hard_light),
            BlendMode::SoftLight => separable_atop(src, opacity, dst, soft_light),
            BlendMode::Difference => separable_atop(src, opacity, dst, |b, s| (b - s).abs()),
            BlendMode::Exclusion => {
                separable_atop(src, opacity, dst, |b, s| b + s - 2.0 * b * s);
            }
            BlendMode::Add => separable_atop(src, opacity, dst, |b, s| (b + s).min(1.0)),
            BlendMode::Hue => {
                non_separable_atop(src, opacity, dst, |b, s| {
                    set_lum(set_sat(s, sat(b)), lum(b))
                });
            }
            BlendMode::Saturation => {
                non_separable_atop(src, opacity, dst, |b, s| {
                    set_lum(set_sat(b, sat(s)), lum(b))
                });
            }
            BlendMode::Color => non_separable_atop(src, opacity, dst, |b, s| set_lum(s, lum(b))),
            BlendMode::Luminosity => {
                non_separable_atop(src, opacity, dst, |b, s| set_lum(b, lum(s)));
            }
        }
    }

    /// Blends `src` (scaled by `opacity`) onto `dst` in place. The mode is
    /// matched once; each arm runs its own monomorphized pixel loop.
    pub fn composite(self, src: &[PremulRgba], opacity: f32, dst: &mut [PremulRgba]) {
        match self {
            BlendMode::Normal => {
                for (d, s) in dst.iter_mut().zip(src) {
                    if s.a > 0.0 {
                        *d = s.scale(opacity).over(*d);
                    }
                }
            }
            BlendMode::Multiply => separable(src, opacity, dst, |b, s| b * s),
            BlendMode::Screen => separable(src, opacity, dst, screen),
            BlendMode::Overlay => separable(src, opacity, dst, |b, s| hard_light(s, b)),
            BlendMode::Darken => separable(src, opacity, dst, f32::min),
            BlendMode::Lighten => separable(src, opacity, dst, f32::max),
            BlendMode::ColorDodge => separable(src, opacity, dst, color_dodge),
            BlendMode::ColorBurn => separable(src, opacity, dst, color_burn),
            BlendMode::HardLight => separable(src, opacity, dst, hard_light),
            BlendMode::SoftLight => separable(src, opacity, dst, soft_light),
            BlendMode::Difference => separable(src, opacity, dst, |b, s| (b - s).abs()),
            BlendMode::Exclusion => separable(src, opacity, dst, |b, s| b + s - 2.0 * b * s),
            BlendMode::Add => separable(src, opacity, dst, |b, s| (b + s).min(1.0)),
            BlendMode::Hue => {
                non_separable(src, opacity, dst, |b, s| {
                    set_lum(set_sat(s, sat(b)), lum(b))
                });
            }
            BlendMode::Saturation => {
                non_separable(src, opacity, dst, |b, s| {
                    set_lum(set_sat(b, sat(s)), lum(b))
                });
            }
            BlendMode::Color => non_separable(src, opacity, dst, |b, s| set_lum(s, lum(b))),
            BlendMode::Luminosity => non_separable(src, opacity, dst, |b, s| set_lum(b, lum(s))),
        }
    }
}

/// Un-premultiplied colour, clamped to the unit range (8-bit rounding can
/// leave a channel a hair above its alpha).
#[inline]
fn straight(p: PremulRgba) -> [f32; 3] {
    if p.a <= 0.0 {
        return [0.0; 3];
    }
    let inv = 1.0 / p.a;
    [
        (p.r * inv).min(1.0),
        (p.g * inv).min(1.0),
        (p.b * inv).min(1.0),
    ]
}

/// The W3C general formula given the blended colour `blend` = B(Cb, Cs).
#[inline]
fn general(
    s: PremulRgba,
    opacity: f32,
    d: PremulRgba,
    cs: [f32; 3],
    blend: [f32; 3],
) -> PremulRgba {
    let a_s = s.a * opacity;
    let a_b = d.a;
    let mix = |cs: f32, b: f32, cb_premul: f32| {
        let cs_prime = (1.0 - a_b) * cs + a_b * b;
        a_s * cs_prime + (1.0 - a_s) * cb_premul
    };
    PremulRgba::new(
        mix(cs[0], blend[0], d.r),
        mix(cs[1], blend[1], d.g),
        mix(cs[2], blend[2], d.b),
        a_s + a_b * (1.0 - a_s),
    )
}

#[inline]
fn separable(
    src: &[PremulRgba],
    opacity: f32,
    dst: &mut [PremulRgba],
    f: impl Fn(f32, f32) -> f32,
) {
    for (d, s) in dst.iter_mut().zip(src) {
        if s.a > 0.0 {
            let (cs, cb) = (straight(*s), straight(*d));
            let blend = [f(cb[0], cs[0]), f(cb[1], cs[1]), f(cb[2], cs[2])];
            *d = general(*s, opacity, *d, cs, blend);
        }
    }
}

#[inline]
fn non_separable(
    src: &[PremulRgba],
    opacity: f32,
    dst: &mut [PremulRgba],
    f: impl Fn([f32; 3], [f32; 3]) -> [f32; 3],
) {
    for (d, s) in dst.iter_mut().zip(src) {
        if s.a > 0.0 {
            let (cs, cb) = (straight(*s), straight(*d));
            *d = general(*s, opacity, *d, cs, f(cb, cs));
        }
    }
}

// Separable blend functions, W3C Compositing and Blending Level 1 §9.

fn screen(b: f32, s: f32) -> f32 {
    b + s - b * s
}

fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b * 2.0 * s
    } else {
        screen(b, 2.0 * s - 1.0)
    }
}

fn color_dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        (b / (1.0 - s)).min(1.0)
    }
}

fn color_burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 {
        1.0
    } else if s <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - b) / s).min(1.0)
    }
}

fn soft_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 {
            ((16.0 * b - 12.0) * b + 4.0) * b
        } else {
            b.sqrt()
        };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

// Non-separable helpers, W3C Compositing and Blending Level 1 §10.

fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut c = c;
    if n < 0.0 {
        c = c.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        c = c.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    c
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    // Indices of the min, mid and max channels: three comparisons
    // instead of a general sort (Hue/Saturation ~15% faster, blend_bench).
    let (mut min, mut mid, mut max) = (0usize, 1usize, 2usize);
    if c[min] > c[mid] {
        std::mem::swap(&mut min, &mut mid);
    }
    if c[mid] > c[max] {
        std::mem::swap(&mut mid, &mut max);
    }
    if c[min] > c[mid] {
        std::mem::swap(&mut min, &mut mid);
    }
    let mut out = [0.0; 3];
    if c[max] > c[min] {
        out[mid] = (c[mid] - c[min]) * s / (c[max] - c[min]);
        out[max] = s;
    }
    out
}

/// Source-atop given the blended colour `blend` = B(Cb, Cs): the clipping
/// rule of [`BlendMode::clip_atop`].
#[inline]
fn atop(s: PremulRgba, opacity: f32, d: PremulRgba, blend: [f32; 3]) -> PremulRgba {
    let a_s = s.a * opacity;
    let keep = 1.0 - a_s;
    let k = a_s * d.a;
    PremulRgba::new(
        k * blend[0] + keep * d.r,
        k * blend[1] + keep * d.g,
        k * blend[2] + keep * d.b,
        d.a,
    )
}

#[inline]
fn separable_atop(
    src: &[PremulRgba],
    opacity: f32,
    dst: &mut [PremulRgba],
    f: impl Fn(f32, f32) -> f32,
) {
    for (d, s) in dst.iter_mut().zip(src) {
        if s.a > 0.0 && d.a > 0.0 {
            let (cs, cb) = (straight(*s), straight(*d));
            let blend = [f(cb[0], cs[0]), f(cb[1], cs[1]), f(cb[2], cs[2])];
            *d = atop(*s, opacity, *d, blend);
        }
    }
}

#[inline]
fn non_separable_atop(
    src: &[PremulRgba],
    opacity: f32,
    dst: &mut [PremulRgba],
    f: impl Fn([f32; 3], [f32; 3]) -> [f32; 3],
) {
    for (d, s) in dst.iter_mut().zip(src) {
        if s.a > 0.0 && d.a > 0.0 {
            let (cs, cb) = (straight(*s), straight(*d));
            *d = atop(*s, opacity, *d, f(cb, cs));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(r: u8, g: u8, b: u8, a: u8) -> PremulRgba {
        tela_color::Rgba::new(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            f32::from(a) / 255.0,
        )
        .premultiply()
    }

    fn blend(mode: BlendMode, bottom: PremulRgba, top: PremulRgba, opacity: f32) -> [u8; 4] {
        let mut d = [bottom];
        mode.composite(&[top], opacity, &mut d);
        let s = d[0].unpremultiply();
        [s.r, s.g, s.b, s.a].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    /// Reference values computed by hand from the W3C definitions
    /// (opaque layers, so the result is B(Cb, Cs) itself).
    #[test]
    fn opaque_results_follow_the_published_blend_functions() {
        let b = px(200, 100, 50, 255); // Cb = (0.784, 0.392, 0.196)
        let s = px(100, 150, 250, 255); // Cs = (0.392, 0.588, 0.980)
        let cases: [(BlendMode, [u8; 3]); 13] = [
            // Multiply: Cb·Cs → (0.308, 0.231, 0.192)
            (BlendMode::Multiply, [78, 59, 49]),
            // Screen: Cb + Cs − Cb·Cs → (0.869, 0.750, 0.984)
            (BlendMode::Screen, [222, 191, 251]),
            // Overlay = HardLight(Cs, Cb): R 1−2(1−.784)(1−.392)=.737, G 2·.392·.588=.461, B 2·.196·.980=.384
            (BlendMode::Overlay, [188, 118, 98]),
            (BlendMode::Darken, [100, 100, 50]),
            (BlendMode::Lighten, [200, 150, 250]),
            // Dodge: min(1, Cb/(1−Cs)) → R 1, G .392/.412=.952, B 1
            (BlendMode::ColorDodge, [255, 243, 255]),
            // Burn: 1 − min(1, (1−Cb)/Cs) → R .449, G 0 (.608/.588>1), B .180
            (BlendMode::ColorBurn, [114, 0, 46]),
            // Hard light: R 2·.784·.392=.615, G screen(.392,.176)=.499, B screen(.196,.961)=.969
            (BlendMode::HardLight, [157, 127, 247]),
            (BlendMode::Difference, [100, 50, 200]),
            // Exclusion: Cb + Cs − 2CbCs → R .561, G .519, B .792
            (BlendMode::Exclusion, [143, 132, 202]),
            (BlendMode::Add, [255, 250, 255]),
            // Soft light, Cs ≤ .5 (R): .784 − (1−.784)(.784)(.216)=.748;
            // G (Cs>.5, Cb>.25): .392 + .176·(√.392 − .392)=.433;
            // B: .196 + .961·(D(.196) − .196), D=((16·.196−12)·.196+4)·.196=.443 → .433
            (BlendMode::SoftLight, [191, 110, 110]),
            (BlendMode::Normal, [100, 150, 250]),
        ];
        for (mode, want) in cases {
            let got = blend(mode, b, s, 1.0);
            for i in 0..3 {
                assert!(
                    got[i].abs_diff(want[i]) <= 1,
                    "{mode:?} channel {i}: got {got:?}, want {want:?}"
                );
            }
            assert_eq!(got[3], 255, "{mode:?} stays opaque");
        }
    }

    #[test]
    fn non_separable_modes_keep_the_published_components() {
        let b = px(200, 100, 50, 255);
        let s = px(40, 160, 90, 255);
        let lum8 = |c: [u8; 4]| lum([c[0], c[1], c[2]].map(|v| f32::from(v) / 255.0));
        let sat8 = |c: [u8; 4]| sat([c[0], c[1], c[2]].map(|v| f32::from(v) / 255.0));
        let (bb, ss) = ([200, 100, 50, 255], [40, 160, 90, 255]);
        // Luminosity takes Lum(Cs), Color/Hue/Saturation keep Lum(Cb).
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(near(
            lum8(blend(BlendMode::Luminosity, b, s, 1.0)),
            lum8(ss)
        ));
        for mode in [BlendMode::Color, BlendMode::Hue, BlendMode::Saturation] {
            assert!(near(lum8(blend(mode, b, s, 1.0)), lum8(bb)), "{mode:?}");
        }
        // Saturation takes Sat(Cs); Hue keeps Sat(Cb).
        assert!(near(
            sat8(blend(BlendMode::Saturation, b, s, 1.0)),
            sat8(ss)
        ));
        assert!(near(sat8(blend(BlendMode::Hue, b, s, 1.0)), sat8(bb)));
    }

    #[test]
    fn transparent_source_is_the_identity_in_every_mode() {
        let dst = [px(10, 200, 30, 128), px(0, 0, 0, 0), px(255, 255, 255, 255)];
        for mode in BlendMode::ALL {
            let mut d = dst;
            mode.composite(&[PremulRgba::TRANSPARENT; 3], 1.0, &mut d);
            assert_eq!(d, dst, "{mode:?}");
        }
    }

    #[test]
    fn over_a_transparent_backdrop_every_mode_shows_the_source() {
        let s = px(30, 200, 60, 200);
        for mode in BlendMode::ALL {
            let mut d = [PremulRgba::TRANSPARENT];
            mode.composite(&[s], 0.5, &mut d);
            let want = s.scale(0.5);
            let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
            assert!(
                close(d[0].r, want.r) && close(d[0].g, want.g) && close(d[0].a, want.a),
                "{mode:?}: {:?}",
                d[0]
            );
        }
    }

    fn premul() -> impl proptest::strategy::Strategy<Value = PremulRgba> {
        use proptest::prelude::*;
        (0.0f32..=1.0, 0.0f32..=1.0, 0.0f32..=1.0, 0.0f32..=1.0)
            .prop_map(|(r, g, b, a)| tela_color::Rgba::new(r, g, b, a).premultiply())
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(512))]

        #[test]
        fn every_mode_yields_valid_premultiplied_pixels(
            s in premul(),
            d in premul(),
            opacity in 0.0f32..=1.0,
            mode in 0usize..17,
        ) {
            let mode = BlendMode::ALL[mode];
            let mut out = [d];
            mode.composite(&[s], opacity, &mut out);
            let o = out[0];
            let eps = 1e-5;
            for c in [o.r, o.g, o.b, o.a] {
                proptest::prop_assert!(c.is_finite() && (-eps..=1.0 + eps).contains(&c), "{:?} {:?}", mode, o);
            }
            for c in [o.r, o.g, o.b] {
                proptest::prop_assert!(c <= o.a + eps, "{:?}: colour exceeds alpha {:?}", mode, o);
            }
            // Alpha is the W3C union in every mode (no mode adds coverage).
            let a_s = if s.a > 0.0 { s.a * opacity } else { 0.0 };
            let want = if s.a > 0.0 { a_s + d.a * (1.0 - a_s) } else { d.a };
            proptest::prop_assert!((o.a - want).abs() <= eps, "{:?} alpha {} want {}", mode, o.a, want);
        }

        #[test]
        fn transparent_source_never_changes_the_destination(d in premul(), opacity in 0.0f32..=1.0, mode in 0usize..17) {
            let mut out = [d];
            BlendMode::ALL[mode].composite(&[PremulRgba::TRANSPARENT], opacity, &mut out);
            proptest::prop_assert_eq!(out[0], d);
        }
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<_> = BlendMode::ALL.iter().map(|m| m.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BlendMode::ALL.len());
    }

    #[test]
    fn clip_atop_keeps_the_group_alpha_in_every_mode() {
        let group = PremulRgba::new(0.3, 0.2, 0.1, 0.5);
        let src = PremulRgba::new(0.1, 0.4, 0.6, 0.8);
        for mode in BlendMode::ALL {
            let mut d = [group];
            mode.clip_atop(&[src], 0.7, &mut d);
            assert_eq!(d[0].a, group.a, "{mode:?}");
            assert!(d[0].r <= d[0].a && d[0].g <= d[0].a && d[0].b <= d[0].a);
        }
    }

    #[test]
    fn clip_atop_uses_the_blend_result_as_if_the_group_were_opaque() {
        // Reference K1: black Multiply over a 50 % base gives black at 50 %;
        // white Multiply leaves the base colour.
        let base = PremulRgba::new(0.4, 0.3, 0.1, 0.5);
        let mut d = [base];
        BlendMode::Multiply.clip_atop(&[PremulRgba::new(0.0, 0.0, 0.0, 1.0)], 1.0, &mut d);
        assert_eq!(d[0], PremulRgba::new(0.0, 0.0, 0.0, 0.5));
        let mut d = [base];
        BlendMode::Multiply.clip_atop(&[PremulRgba::new(1.0, 1.0, 1.0, 1.0)], 1.0, &mut d);
        assert!((d[0].r - base.r).abs() < 1e-6 && (d[0].g - base.g).abs() < 1e-6);
        // Normal: source-atop.
        let mut d = [base];
        let s = PremulRgba::new(0.5, 0.0, 0.0, 0.5);
        BlendMode::Normal.clip_atop(&[s], 1.0, &mut d);
        let want = PremulRgba::new(0.5 * 0.5 + 0.4 * 0.5, 0.15, 0.05, 0.5);
        assert!((d[0].r - want.r).abs() < 1e-6 && (d[0].g - want.g).abs() < 1e-6);
    }

    #[test]
    fn clip_atop_leaves_transparent_groups_and_sources_alone() {
        for mode in BlendMode::ALL {
            let mut d = [PremulRgba::TRANSPARENT, PremulRgba::new(0.2, 0.2, 0.2, 0.4)];
            let src = [PremulRgba::new(0.9, 0.1, 0.5, 1.0), PremulRgba::TRANSPARENT];
            mode.clip_atop(&src, 1.0, &mut d);
            assert_eq!(d[0], PremulRgba::TRANSPARENT, "{mode:?}");
            assert_eq!(d[1], PremulRgba::new(0.2, 0.2, 0.2, 0.4), "{mode:?}");
        }
    }
}
