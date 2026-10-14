//! RGBA8 images, PNG I/O and tolerant comparison.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl Image {
    /// White, opaque.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![[255; 4]; (width * height) as usize],
        }
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        self.pixels[(y * self.width + x) as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, p: [u8; 4]) {
        self.pixels[(y * self.width + x) as usize] = p;
    }

    pub fn save_png(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let w = BufWriter::new(File::create(path)?);
        let mut enc = png::Encoder::new(w, self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(std::io::Error::other)?;
        let bytes: Vec<u8> = self.pixels.iter().flatten().copied().collect();
        writer
            .write_image_data(&bytes)
            .map_err(std::io::Error::other)?;
        Ok(())
    }

    pub fn load_png(path: &Path) -> std::io::Result<Self> {
        let dec = png::Decoder::new(BufReader::new(File::open(path)?));
        let mut reader = dec.read_info().map_err(std::io::Error::other)?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| std::io::Error::other("PNG too large"))?;
        let mut buf = vec![0; size];
        let info = reader.next_frame(&mut buf).map_err(std::io::Error::other)?;
        if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
            return Err(std::io::Error::other("golden images must be RGBA8"));
        }
        let pixels = buf[..info.buffer_size()].as_chunks::<4>().0.to_vec();
        Ok(Self {
            width: info.width,
            height: info.height,
            pixels,
        })
    }
}

/// How different two images are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Diff {
    /// Mean absolute channel difference, 0..255.
    pub mean: f64,
    /// Largest channel difference, 0..255.
    pub max: u8,
    /// Fraction of pixels with any channel differing by more than the
    /// per-pixel tolerance.
    pub differing: f64,
}

/// Golden-test thresholds. Rendering is deterministic on one machine, so
/// identical output is the norm; the tolerances absorb floating-point
/// rounding differences across compilers/CPUs while still failing on any
/// visible change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thresholds {
    /// Channel difference at or below which a pixel counts as equal.
    pub pixel_tolerance: u8,
    pub max_mean: f64,
    pub max_error: u8,
    pub max_differing: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            pixel_tolerance: 2,
            max_mean: 0.5,
            max_error: 64,
            max_differing: 0.001,
        }
    }
}

impl Thresholds {
    pub fn accepts(&self, d: &Diff) -> bool {
        d.mean <= self.max_mean && d.max <= self.max_error && d.differing <= self.max_differing
    }
}

impl Image {
    /// `None` if the sizes differ.
    pub fn diff(&self, other: &Self, pixel_tolerance: u8) -> Option<Diff> {
        if self.width != other.width || self.height != other.height {
            return None;
        }
        let (mut sum, mut max, mut differing) = (0u64, 0u8, 0usize);
        for (a, b) in self.pixels.iter().zip(&other.pixels) {
            let mut worst = 0;
            for c in 0..4 {
                let d = a[c].abs_diff(b[c]);
                sum += d as u64;
                worst = worst.max(d);
            }
            max = max.max(worst);
            if worst > pixel_tolerance {
                differing += 1;
            }
        }
        let n = self.pixels.len().max(1);
        Some(Diff {
            mean: sum as f64 / (n * 4) as f64,
            max,
            differing: differing as f64 / n as f64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip() {
        let mut img = Image::new(7, 5);
        img.set(3, 2, [10, 20, 30, 40]);
        let dir = std::env::temp_dir().join(format!("tela-corpus-{}", std::process::id()));
        let path = dir.join("rt.png");
        img.save_png(&path).unwrap();
        assert_eq!(Image::load_png(&path).unwrap(), img);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn diff_metrics_and_thresholds() {
        let a = Image::new(100, 100);
        let mut b = a.clone();
        assert_eq!(a.diff(&b, 2).unwrap(), Diff::default());
        b.set(0, 0, [254, 255, 255, 255]);
        let small = a.diff(&b, 2).unwrap();
        assert_eq!(small.differing, 0.0);
        assert!(Thresholds::default().accepts(&small));
        // A visible change: a 20x20 black square (4% of pixels).
        for y in 0..20 {
            for x in 0..20 {
                b.set(x, y, [0, 0, 0, 255]);
            }
        }
        let big = a.diff(&b, 2).unwrap();
        assert!((big.differing - 0.04).abs() < 1e-9);
        assert!(!Thresholds::default().accepts(&big));
        assert!(a.diff(&Image::new(1, 1), 2).is_none());
    }
}
