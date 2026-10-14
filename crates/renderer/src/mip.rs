//! CPU mip generation for a single tile.
//!
//! Tiles are power-of-two sized and aligned, so mip level `k` of a tile is
//! exactly the `256 >> k` square of the document's level `k` that the tile
//! covers: tiles can be refreshed independently without touching neighbours.

use tela_canvas::TILE_SIZE;

/// Number of mip levels of one tile (256 -> 1).
pub const TILE_LEVELS: u32 = TILE_SIZE.trailing_zeros() + 1;

/// Halves a square premultiplied RGBA8 image with a 2x2 box filter.
/// Box filtering premultiplied values is correct for alpha: transparent
/// texels contribute no color.
pub fn downsample(src: &[[u8; 4]], src_size: u32, dst: &mut Vec<[u8; 4]>) {
    let n = (src_size / 2).max(1);
    dst.clear();
    dst.reserve((n * n) as usize);
    let s = src_size as usize;
    for y in 0..n as usize {
        for x in 0..n as usize {
            let (x0, y0) = (2 * x, 2 * y);
            let (x1, y1) = ((x0 + 1).min(s - 1), (y0 + 1).min(s - 1));
            let px = [
                src[y0 * s + x0],
                src[y0 * s + x1],
                src[y1 * s + x0],
                src[y1 * s + x1],
            ];
            let mut out = [0u8; 4];
            for (c, o) in out.iter_mut().enumerate() {
                let sum: u32 = px.iter().map(|p| p[c] as u32).sum();
                *o = ((sum + 2) / 4) as u8;
            }
            dst.push(out);
        }
    }
}

/// Reusable buffers holding a tile's levels 1.. (level 0 is the input).
#[derive(Default)]
pub struct MipChain {
    levels: Vec<Vec<[u8; 4]>>,
}

impl MipChain {
    pub fn build(&mut self, level0: &[[u8; 4]]) {
        self.levels.resize_with(TILE_LEVELS as usize - 1, Vec::new);
        let mut size = TILE_SIZE;
        for k in 0..self.levels.len() {
            let (done, rest) = self.levels.split_at_mut(k);
            let src: &[[u8; 4]] = if k == 0 { level0 } else { &done[k - 1] };
            downsample(src, size, &mut rest[0]);
            size /= 2;
        }
    }

    /// Level `k >= 1`.
    pub fn level(&self, k: u32) -> &[[u8; 4]] {
        &self.levels[k as usize - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_filter_averages_and_rounds() {
        let src = vec![
            [0, 0, 0, 0],
            [255, 0, 0, 255],
            [255, 0, 0, 255],
            [255, 0, 0, 255],
        ];
        let mut dst = vec![];
        downsample(&src, 2, &mut dst);
        assert_eq!(dst, vec![[191, 0, 0, 191]]);
    }

    #[test]
    fn chain_reaches_one_pixel_and_preserves_mean() {
        let level0: Vec<[u8; 4]> = (0..TILE_SIZE * TILE_SIZE)
            .map(|i| {
                if (i / TILE_SIZE + i % TILE_SIZE).is_multiple_of(2) {
                    [200, 100, 50, 255]
                } else {
                    [0; 4]
                }
            })
            .collect();
        let mut chain = MipChain::default();
        chain.build(&level0);
        assert_eq!(TILE_LEVELS, 9);
        assert_eq!(chain.level(8).len(), 1);
        // A checkerboard averages to half everywhere.
        for p in chain.level(1) {
            assert_eq!(*p, [100, 50, 25, 128]);
        }
        let top = chain.level(8)[0];
        assert!((top[3] as i32 - 128).abs() <= 1, "{top:?}");
    }
}
