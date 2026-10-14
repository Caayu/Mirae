//! Per-stroke deterministic randomness.
//!
//! Values are a pure function of `(seed, stream, index)`: there is no hidden
//! state, so evaluation order does not matter. That is what lets the
//! provisional stroke tail be rebuilt any number of times, and what makes a
//! stroke replayable (undo, tests, recordings): same preset + seed + input
//! samples gives bit-identical dabs.

/// Mixes 64 bits (SplitMix64 finalizer).
pub(crate) fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Independent random sequences used by the engine. Each consumer has its
/// own stream so values never correlate across uses.
#[allow(missing_docs)]
pub mod stream {
    pub const SCATTER_A: u32 = 1;
    pub const SCATTER_B: u32 = 2;
    pub const SCATTER_ROTATION: u32 = 3;
    pub const SCATTER_SCALE: u32 = 4;
    /// Effector streams start here; see [`crate::dynamics`].
    pub const EFFECTORS: u32 = 1 << 16;
    pub const NOISE: u32 = 1 << 24;
}

/// Random source of one stroke: a seed, nothing else. Every value is
/// derived from `(seed, stream, index)` (counter-based, SplitMix64 mixing),
/// so values do not depend on how many were drawn before or in what order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct StrokeRng {
    seed: u64,
}

impl StrokeRng {
    /// Generator for the stroke with this seed.
    pub fn new(seed: u64) -> Self {
        Self {
            seed: mix64(seed ^ 0x7E1A_5EED_0000_0001),
        }
    }

    /// The mixed seed (not the value passed to `new`).
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// 64 random bits for `(stream, index)`.
    pub fn u64(&self, stream: u32, index: u64) -> u64 {
        mix64(
            self.seed
                ^ mix64(
                    (stream as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                        ^ index.wrapping_mul(0xD1B5_4A32_D192_ED03),
                ),
        )
    }

    /// Uniform in `[0, 1)`.
    pub fn uniform(&self, stream: u32, index: u64) -> f32 {
        (self.u64(stream, index) >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `[-1, 1)`.
    pub fn signed(&self, stream: u32, index: u64) -> f32 {
        self.uniform(stream, index) * 2.0 - 1.0
    }

    /// Smooth value noise in `[0, 1]` along `x` (one lattice point per unit),
    /// C1-continuous thanks to the quintic fade.
    pub fn noise(&self, stream: u32, x: f32) -> f32 {
        let x = x.max(0.0);
        let i = x.floor();
        let t = x - i;
        let fade = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        let a = self.uniform(stream ^ stream::NOISE, i as u64);
        let b = self.uniform(stream ^ stream::NOISE, i as u64 + 1);
        a + (b - a) * fade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_inputs_same_values_in_any_order() {
        let r = StrokeRng::new(42);
        let forward: Vec<f32> = (0..100).map(|i| r.uniform(7, i)).collect();
        let backward: Vec<f32> = (0..100).rev().map(|i| r.uniform(7, i)).collect();
        assert!(forward.iter().eq(backward.iter().rev()));
        assert_eq!(StrokeRng::new(42).uniform(7, 5), r.uniform(7, 5));
    }

    #[test]
    fn streams_and_seeds_are_independent() {
        let r = StrokeRng::new(1);
        let a: Vec<f32> = (0..2000).map(|i| r.uniform(1, i)).collect();
        let b: Vec<f32> = (0..2000).map(|i| r.uniform(2, i)).collect();
        let c: Vec<f32> = (0..2000).map(|i| StrokeRng::new(2).uniform(1, i)).collect();
        let corr = |x: &[f32], y: &[f32]| {
            let n = x.len() as f32;
            let (mx, my) = (x.iter().sum::<f32>() / n, y.iter().sum::<f32>() / n);
            let cov: f32 = x
                .iter()
                .zip(y)
                .map(|(a, b)| (a - mx) * (b - my))
                .sum::<f32>()
                / n;
            let sx = (x.iter().map(|a| (a - mx).powi(2)).sum::<f32>() / n).sqrt();
            let sy = (y.iter().map(|b| (b - my).powi(2)).sum::<f32>() / n).sqrt();
            cov / (sx * sy)
        };
        assert!(corr(&a, &b).abs() < 0.06);
        assert!(corr(&a, &c).abs() < 0.06);
    }

    #[test]
    fn uniform_is_uniform() {
        let r = StrokeRng::new(9);
        let mut bins = [0u32; 10];
        for i in 0..50_000 {
            let v = r.uniform(3, i);
            assert!((0.0..1.0).contains(&v));
            bins[(v * 10.0) as usize] += 1;
        }
        assert!(bins.iter().all(|&b| (4500..5500).contains(&b)), "{bins:?}");
    }

    #[test]
    fn noise_is_continuous_and_bounded() {
        let r = StrokeRng::new(5);
        let mut prev = r.noise(1, 0.0);
        for i in 1..10_000 {
            let v = r.noise(1, i as f32 * 0.001);
            assert!((0.0..=1.0).contains(&v));
            assert!((v - prev).abs() < 0.01, "jump at {i}");
            prev = v;
        }
    }
}
