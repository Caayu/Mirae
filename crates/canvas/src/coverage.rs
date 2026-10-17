use crate::tile::{TILE_SIZE, Tile};

/// How much an edit may change each pixel of one tile, `0..=255` (a
/// selection's coverage, seen through one tile). Borrowed, never
/// allocated: producing one per tile per frame costs nothing.
#[derive(Clone, Copy, Debug)]
pub enum TileCoverage<'a> {
    /// Every pixel may change fully (no selection, or a fully selected
    /// tile). Edits take their unrestricted path.
    Full,
    /// No pixel may change.
    Empty,
    /// Per-pixel values: `stored ^ xor` inside `clip`, 0 outside it. An
    /// absent `stored` tile reads as 0 (so `xor` alone).
    Partial {
        stored: Option<&'a Tile<u8>>,
        /// 0 or 255: stored values are kept inverted when 255.
        xor: u8,
        /// Tile-local half-open bounds `[x0, x1) × [y0, y1)` of the pixels
        /// that can be covered at all (the canvas, clipped to the tile).
        clip: [u32; 4],
    },
}

impl TileCoverage<'_> {
    /// Coverage of tile-local pixel `(x, y)`.
    pub fn get(&self, x: u32, y: u32) -> u8 {
        match *self {
            TileCoverage::Full => 255,
            TileCoverage::Empty => 0,
            TileCoverage::Partial { stored, xor, clip } => {
                if x < clip[0] || x >= clip[2] || y < clip[1] || y >= clip[3] {
                    0
                } else {
                    stored.map_or(0, |t| t.get(x, y)) ^ xor
                }
            }
        }
    }

    /// The coverage of tile row `y`, all 256 values.
    pub fn row(&self, y: u32, out: &mut [u8; TILE_SIZE as usize]) {
        match *self {
            TileCoverage::Full => out.fill(255),
            TileCoverage::Empty => out.fill(0),
            TileCoverage::Partial { stored, xor, clip } => {
                out.fill(0);
                if y < clip[1] || y >= clip[3] {
                    return;
                }
                let (x0, x1) = (clip[0] as usize, clip[2] as usize);
                match stored {
                    Some(t) => {
                        for (o, &s) in out[x0..x1].iter_mut().zip(&t.row(y)[x0..x1]) {
                            *o = s ^ xor;
                        }
                    }
                    None => out[x0..x1].fill(xor),
                }
            }
        }
    }

    /// `mask` (one row of a tile) scaled by the coverage of row `y`, into
    /// `out`: `m · c / 255`. Returns whether any value is non-zero. A full
    /// coverage leaves values exactly unchanged (`m · 1.0`).
    pub fn scale_row(&self, y: u32, mask: &[f32], out: &mut [f32; TILE_SIZE as usize]) -> bool {
        let mut coverage = [0u8; TILE_SIZE as usize];
        self.row(y, &mut coverage);
        let mut any = false;
        for ((o, &m), &c) in out.iter_mut().zip(mask).zip(&coverage) {
            *o = m * (f32::from(c) / 255.0);
            any |= *o > 0.0;
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_coverage_reads_inverted_values_inside_the_clip_only() {
        let mut t = Tile::<u8>::new();
        *t.get_mut(3, 4) = 200;
        let c = TileCoverage::Partial {
            stored: Some(&t),
            xor: 255,
            clip: [0, 0, 10, 10],
        };
        assert_eq!(c.get(3, 4), 55);
        assert_eq!(c.get(0, 0), 255);
        assert_eq!(c.get(10, 0), 0, "outside the clip");
        let absent = TileCoverage::Partial {
            stored: None,
            xor: 255,
            clip: [0, 0, 256, 128],
        };
        assert_eq!(absent.get(5, 5), 255);
        assert_eq!(absent.get(5, 200), 0);
    }

    #[test]
    fn rows_agree_with_single_pixels() {
        let mut t = Tile::<u8>::new();
        for (i, v) in t.pixels_mut().iter_mut().enumerate() {
            *v = (i * 7 % 251) as u8;
        }
        for c in [
            TileCoverage::Full,
            TileCoverage::Empty,
            TileCoverage::Partial {
                stored: Some(&t),
                xor: 255,
                clip: [3, 5, 200, 250],
            },
            TileCoverage::Partial {
                stored: None,
                xor: 255,
                clip: [0, 0, 100, 256],
            },
        ] {
            let mut row = [0u8; 256];
            for y in [0, 4, 5, 100, 249, 250, 255] {
                c.row(y, &mut row);
                for (x, &v) in row.iter().enumerate() {
                    assert_eq!(v, c.get(x as u32, y), "{c:?} at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn scaling_by_full_coverage_is_exact() {
        let mask: Vec<f32> = (0..256).map(|i| i as f32 / 255.0).collect();
        let mut out = [0.0; 256];
        let full = TileCoverage::Partial {
            stored: None,
            xor: 255,
            clip: [0, 0, 256, 256],
        };
        assert!(full.scale_row(0, &mask, &mut out));
        assert_eq!(out.as_slice(), mask.as_slice());
        assert!(!TileCoverage::Empty.scale_row(0, &mask, &mut out));
        assert!(out.iter().all(|&v| v == 0.0));
    }
}
