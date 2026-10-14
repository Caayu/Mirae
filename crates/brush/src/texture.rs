//! Paper textures in canvas space.
//!
//! A texture is a periodic height field sampled at *canvas* coordinates, so
//! the paper stays still while the brush moves across it. It modulates each
//! dab's opacity ceiling rather than its coverage, so the grain survives any
//! amount of dab overlap and does not depend on spacing.
//!
//! The built-in height fields are generated here from periodic gradient
//! noise; every octave's lattice period divides the texture size, which
//! makes them seamless by construction. No external assets are involved.

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use crate::rng::mix64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TexturePattern {
    /// Fine, even tooth.
    #[default]
    PaperFine,
    /// Coarse, fibrous tooth.
    PaperRough,
    /// Speckled, high-contrast grain.
    Chalk,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureMode {
    /// Scale the ceiling down in the paper's valleys.
    #[default]
    Multiply,
    /// Remove a fixed amount of paint in the valleys.
    Subtract,
    /// Paint reaches lower into the paper the harder the dab presses
    /// (opacity x coverage): light touches only mark the peaks.
    Height,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureSettings {
    pub pattern: TexturePattern,
    pub mode: TextureMode,
    /// Texture magnification (1 = one texel per canvas pixel).
    pub scale: f32,
    /// Degrees.
    pub rotation: f32,
    /// Added to the height, `-1..=1`.
    pub brightness: f32,
    /// Multiplies the height's deviation from 0.5, `0..=4`.
    pub contrast: f32,
}

impl Default for TextureSettings {
    fn default() -> Self {
        Self {
            pattern: TexturePattern::PaperFine,
            mode: TextureMode::Multiply,
            scale: 1.0,
            rotation: 0.0,
            brightness: 0.0,
            contrast: 1.0,
        }
    }
}

/// Square, periodic height field in `[0, 1]`.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightMap {
    size: u32,
    data: Vec<f32>,
}

impl HeightMap {
    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn at(&self, x: u32, y: u32) -> f32 {
        self.data[((y % self.size) * self.size + x % self.size) as usize]
    }

    /// Bilinear, wrapping at the edges.
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let s = self.size as f32;
        let (x, y) = (x.rem_euclid(s) - 0.5, y.rem_euclid(s) - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let n = self.size as i64;
        let at = |xi: i64, yi: i64| self.data[(yi.rem_euclid(n) * n + xi.rem_euclid(n)) as usize];
        let (xi, yi) = (x0 as i64, y0 as i64);
        let top = at(xi, yi) + (at(xi + 1, yi) - at(xi, yi)) * fx;
        let bottom = at(xi, yi + 1) + (at(xi + 1, yi + 1) - at(xi, yi + 1)) * fx;
        top + (bottom - top) * fy
    }
}

const TEXTURE_SIZE: u32 = 512;

/// Periodic 2D gradient noise in roughly `[-1, 1]`; `period` lattice cells
/// span the texture.
fn gradient_noise(x: f32, y: f32, period: u32, seed: u64) -> f32 {
    let cell = TEXTURE_SIZE as f32 / period as f32;
    let (x, y) = (x / cell, y / cell);
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let grad = |gx: i64, gy: i64, dx: f32, dy: f32| {
        let p = period as i64;
        let h = mix64(seed ^ ((gx.rem_euclid(p) as u64) << 32) ^ gy.rem_euclid(p) as u64);
        let a = (h >> 40) as f32 / (1u64 << 24) as f32 * std::f32::consts::TAU;
        a.cos() * dx + a.sin() * dy
    };
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (gx, gy) = (ix as i64, iy as i64);
    let n00 = grad(gx, gy, fx, fy);
    let n10 = grad(gx + 1, gy, fx - 1.0, fy);
    let n01 = grad(gx, gy + 1, fx, fy - 1.0);
    let n11 = grad(gx + 1, gy + 1, fx - 1.0, fy - 1.0);
    let (u, v) = (fade(fx), fade(fy));
    let top = n00 + (n10 - n00) * u;
    let bottom = n01 + (n11 - n01) * u;
    (top + (bottom - top) * v) * std::f32::consts::SQRT_2
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl TexturePattern {
    pub fn height_map(self) -> Arc<HeightMap> {
        static CACHE: [OnceLock<Arc<HeightMap>>; 4] = [const { OnceLock::new() }; 4];
        let i = match self {
            Self::PaperFine => 0,
            Self::PaperRough => 1,
            Self::Chalk => 2,
            Self::Unknown => 3,
        };
        CACHE[i].get_or_init(|| Arc::new(self.generate())).clone()
    }

    fn generate(self) -> HeightMap {
        let n = TEXTURE_SIZE;
        let seed = 0x9A9E_0000 + self as u64;
        // (period, weight, ridged) per octave; every period divides `n`.
        let octaves: &[(u32, f32, bool)] = match self {
            Self::PaperFine | Self::Unknown => {
                &[(64, 0.45, false), (128, 0.35, false), (256, 0.2, false)]
            }
            Self::PaperRough => &[
                (16, 0.4, true),
                (32, 0.3, false),
                (64, 0.2, true),
                (128, 0.1, false),
            ],
            Self::Chalk => &[(32, 0.25, false), (128, 0.45, false), (256, 0.3, false)],
        };
        let mut data: Vec<f32> = (0..n * n)
            .map(|i| {
                let (x, y) = ((i % n) as f32 + 0.5, (i / n) as f32 + 0.5);
                octaves
                    .iter()
                    .enumerate()
                    .map(|(k, &(period, w, ridged))| {
                        let v = gradient_noise(x, y, period, seed + k as u64);
                        w * if ridged { 1.0 - 2.0 * v.abs() } else { v }
                    })
                    .sum::<f32>()
            })
            .collect();
        let (lo, hi) = data
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        for v in &mut data {
            let t = (*v - lo) / (hi - lo).max(1e-6);
            *v = match self {
                Self::Chalk => smoothstep(0.35, 0.7, t),
                _ => t,
            };
        }
        HeightMap { size: n, data }
    }
}

/// A texture ready for sampling during one stroke.
#[derive(Clone, Debug)]
pub struct CanvasTexture {
    map: Arc<HeightMap>,
    mode: TextureMode,
    cos: f32,
    sin: f32,
    inv_scale: f32,
    brightness: f32,
    contrast: f32,
}

impl CanvasTexture {
    pub fn new(settings: &TextureSettings) -> Self {
        let (sin, cos) = settings.rotation.to_radians().sin_cos();
        Self {
            map: settings.pattern.height_map(),
            mode: settings.mode,
            cos,
            sin,
            inv_scale: 1.0 / settings.scale.clamp(0.05, 64.0),
            brightness: settings.brightness.clamp(-1.0, 1.0),
            contrast: settings.contrast.clamp(0.0, 4.0),
        }
    }

    /// Paper height at a canvas point, after brightness and contrast.
    #[inline]
    pub fn height(&self, x: f32, y: f32) -> f32 {
        let u = (x * self.cos + y * self.sin) * self.inv_scale;
        let v = (y * self.cos - x * self.sin) * self.inv_scale;
        let h = self.map.sample(u, v);
        ((h - 0.5) * self.contrast + 0.5 + self.brightness).clamp(0.0, 1.0)
    }

    /// Opacity ceiling after texturing. `strength` is the dab's texture
    /// strength, `coverage` the tip coverage at this pixel.
    #[inline]
    pub fn modulate(&self, ceiling: f32, coverage: f32, height: f32, strength: f32) -> f32 {
        let s = strength.clamp(0.0, 1.0);
        match self.mode {
            TextureMode::Multiply | TextureMode::Unknown => ceiling * (1.0 - s * (1.0 - height)),
            TextureMode::Subtract => (ceiling - s * (1.0 - height)).max(0.0),
            TextureMode::Height => {
                let threshold = s * (1.0 - coverage * ceiling);
                ceiling * smoothstep(threshold - 0.1, threshold + 0.1, height)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(m: &HeightMap) -> (f32, f32) {
        let n = m.data.len() as f32;
        let mean = m.data.iter().sum::<f32>() / n;
        let var = m.data.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n;
        (mean, var.sqrt())
    }

    #[test]
    fn builtin_textures_are_deterministic_and_varied() {
        for p in [
            TexturePattern::PaperFine,
            TexturePattern::PaperRough,
            TexturePattern::Chalk,
        ] {
            let a = p.generate();
            assert_eq!(a, *p.height_map());
            let (mean, std) = stats(&a);
            assert!((0.2..0.8).contains(&mean), "{p:?} mean {mean}");
            assert!(std > 0.08, "{p:?} std {std}");
            assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        }
    }

    #[test]
    fn textures_are_seamless() {
        for p in [
            TexturePattern::PaperFine,
            TexturePattern::PaperRough,
            TexturePattern::Chalk,
        ] {
            let m = p.height_map();
            let n = m.size();
            let mean_step = |pairs: &mut dyn Iterator<Item = (f32, f32)>| {
                let v: Vec<f32> = pairs.map(|(a, b)| (a - b).abs()).collect();
                v.iter().sum::<f32>() / v.len() as f32
            };
            let interior = mean_step(&mut (0..n).map(|y| (m.at(n / 2, y), m.at(n / 2 + 1, y))));
            let across = mean_step(&mut (0..n).map(|y| (m.at(n - 1, y), m.at(0, y))));
            let vertical = mean_step(&mut (0..n).map(|x| (m.at(x, n - 1), m.at(x, 0))));
            assert!(
                across < interior * 1.5 + 1e-3,
                "{p:?}: {across} vs {interior}"
            );
            assert!(
                vertical < interior * 1.5 + 1e-3,
                "{p:?}: {vertical} vs {interior}"
            );
        }
    }

    #[test]
    fn sampling_wraps_and_respects_scale_and_rotation() {
        let t = CanvasTexture::new(&TextureSettings::default());
        assert!((t.height(10.3, 20.7) - t.height(10.3 + 512.0, 20.7 - 1024.0)).abs() < 1e-4);
        let scaled = CanvasTexture::new(&TextureSettings {
            scale: 2.0,
            ..Default::default()
        });
        assert!((scaled.height(40.0, 60.0) - t.height(20.0, 30.0)).abs() < 1e-4);
        let rotated = CanvasTexture::new(&TextureSettings {
            rotation: 90.0,
            ..Default::default()
        });
        // Rotating the paper by 90° maps canvas (x, y) to texture (y, -x).
        assert!((rotated.height(30.0, 70.0) - t.height(70.0, -30.0)).abs() < 1e-3);
    }

    #[test]
    fn modes_shape_the_ceiling() {
        let mk = |mode| {
            CanvasTexture::new(&TextureSettings {
                mode,
                ..Default::default()
            })
        };
        let m = mk(TextureMode::Multiply);
        assert_eq!(m.modulate(0.8, 1.0, 1.0, 1.0), 0.8, "peaks keep full paint");
        assert!((m.modulate(0.8, 1.0, 0.25, 1.0) - 0.2).abs() < 1e-6);
        assert_eq!(
            m.modulate(0.8, 1.0, 0.25, 0.0),
            0.8,
            "no strength, no texture"
        );
        let s = mk(TextureMode::Subtract);
        assert_eq!(s.modulate(0.3, 1.0, 0.0, 1.0), 0.0);
        let h = mk(TextureMode::Height);
        // A light touch only reaches the peaks; a firm one fills the valleys.
        assert!(h.modulate(0.3, 1.0, 0.4, 1.0) < 0.05);
        assert!(h.modulate(1.0, 1.0, 0.4, 1.0) > 0.25);
    }

    #[test]
    fn brightness_and_contrast() {
        let base = CanvasTexture::new(&TextureSettings::default());
        let flat = CanvasTexture::new(&TextureSettings {
            contrast: 0.0,
            ..Default::default()
        });
        assert_eq!(flat.height(13.0, 17.0), 0.5);
        let bright = CanvasTexture::new(&TextureSettings {
            brightness: 1.0,
            ..Default::default()
        });
        assert_eq!(bright.height(13.0, 17.0), 1.0);
        assert!(base.height(13.0, 17.0) < 1.0);
    }
}
