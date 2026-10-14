use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, TileCoord, TiledSurface};
use tela_color::{PremulRgba, Rgba};

use crate::{BrushSettings, Dab, round_tip_coverage};

/// Coverage of the stroke in progress, kept apart from the layer.
///
/// Dabs accumulate into `mask` with `m += flow * coverage * (1 - m)`, which
/// saturates at 1. The layer only receives `color * opacity * m`, so opacity is
/// a hard ceiling for the whole stroke while flow controls how quickly
/// overlapping dabs build up toward it.
#[derive(Clone, Debug)]
pub struct StrokeBuffer {
    mask: TiledSurface<f32>,
    color: Rgba,
    opacity: f32,
    hardness: f32,
    anti_aliasing: bool,
    /// Tiles changed since the last [`StrokeBuffer::take_dirty`].
    dirty: DirtyTiles,
    /// Every tile the stroke has touched.
    touched: DirtyTiles,
}

impl StrokeBuffer {
    pub fn new(settings: &BrushSettings) -> Self {
        Self {
            mask: TiledSurface::new(),
            color: settings.color,
            opacity: settings.opacity.clamp(0.0, 1.0),
            hardness: settings.hardness,
            anti_aliasing: settings.anti_aliasing,
            dirty: DirtyTiles::new(),
            touched: DirtyTiles::new(),
        }
    }

    pub fn mask(&self) -> &TiledSurface<f32> {
        &self.mask
    }

    pub fn touched(&self) -> &DirtyTiles {
        &self.touched
    }

    pub fn take_dirty(&mut self) -> DirtyTiles {
        std::mem::take(&mut self.dirty)
    }

    /// Premultiplied paint color at full coverage.
    pub fn paint(&self) -> PremulRgba {
        self.color.premultiply().scale(self.opacity)
    }

    pub fn stamp(&mut self, dab: &Dab) {
        let reach = dab.radius + 1.0;
        let bounds = IRect::enclosing(
            dab.center.x - reach,
            dab.center.y - reach,
            dab.center.x + reach,
            dab.center.y + reach,
        );
        let flow = dab.flow.clamp(0.0, 1.0);
        for coord in bounds.tiles() {
            let area = bounds.intersect(&coord.rect());
            let (ox, oy) = coord.origin();
            let tile = self.mask.tile_or_insert(coord);
            for y in area.y0..area.y1 {
                let py = y as f32 + 0.5 - dab.center.y;
                let row = tile.row_mut((y - oy) as u32);
                for x in area.x0..area.x1 {
                    let px = x as f32 + 0.5 - dab.center.x;
                    let cov = round_tip_coverage(
                        (px * px + py * py).sqrt(),
                        dab.radius,
                        self.hardness,
                        self.anti_aliasing,
                    );
                    if cov > 0.0 {
                        let m = &mut row[(x - ox) as usize];
                        *m += flow * cov * (1.0 - *m);
                    }
                }
            }
            self.dirty.mark(coord);
            self.touched.mark(coord);
        }
    }

    /// Composites the stroke over one tile of premultiplied pixels.
    pub fn paint_over_tile(&self, coord: TileCoord, pixels: &mut [PremulRgba]) {
        let Some(mask) = self.mask.tile(coord) else {
            return;
        };
        let paint = self.paint();
        for (dst, &m) in pixels.iter_mut().zip(mask.pixels()) {
            if m > 0.0 {
                *dst = paint.scale(m).over(*dst);
            }
        }
    }

    /// Writes the finished stroke into an 8-bit premultiplied surface.
    pub fn commit_into(&self, target: &mut TiledSurface<[u8; 4]>) -> DirtyTiles {
        let mut scratch = vec![PremulRgba::TRANSPARENT; (TILE_SIZE * TILE_SIZE) as usize];
        for coord in self.touched.iter() {
            let tile = target.tile_or_insert(coord);
            for (s, p) in scratch.iter_mut().zip(tile.pixels()) {
                *s = PremulRgba::from_rgba8(*p);
            }
            self.paint_over_tile(coord, &mut scratch);
            for (p, s) in tile.pixels_mut().iter_mut().zip(&scratch) {
                *p = s.to_rgba8();
            }
        }
        self.touched.clone()
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;

    fn dab(x: f32, y: f32, radius: f32, flow: f32) -> Dab {
        Dab {
            center: Vec2::new(x, y),
            radius,
            flow,
            pressure: 1.0,
        }
    }

    fn settings(opacity: f32) -> BrushSettings {
        BrushSettings {
            opacity,
            hardness: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn opacity_caps_overlapping_dabs() {
        let mut s = StrokeBuffer::new(&settings(0.5));
        for _ in 0..50 {
            s.stamp(&dab(10.0, 10.0, 5.0, 0.3));
        }
        let mut target = TiledSurface::new();
        s.commit_into(&mut target);
        let a = target.pixel(10, 10)[3];
        assert!(
            (127..=128).contains(&a),
            "alpha {a} should stop at 50% opacity"
        );
    }

    #[test]
    fn flow_builds_up_gradually() {
        let mut s = StrokeBuffer::new(&settings(1.0));
        s.stamp(&dab(10.0, 10.0, 5.0, 0.25));
        let one = s.mask().pixel(10, 10);
        s.stamp(&dab(10.0, 10.0, 5.0, 0.25));
        let two = s.mask().pixel(10, 10);
        assert!((one - 0.25).abs() < 1e-6);
        assert!((two - (0.25 + 0.25 * 0.75)).abs() < 1e-6);
    }

    #[test]
    fn dab_crossing_tile_corner_touches_four_tiles() {
        let mut s = StrokeBuffer::new(&settings(1.0));
        s.stamp(&dab(256.0, 256.0, 4.0, 1.0));
        assert_eq!(s.touched().len(), 4);
        assert_eq!(s.take_dirty().len(), 4);
        assert!(s.take_dirty().is_empty());
    }

    #[test]
    fn commit_composites_over_existing_pixels() {
        let mut target = TiledSurface::new();
        target.set_pixel(10, 10, [0, 0, 255, 255]);
        let mut st = settings(0.5);
        st.color = Rgba::new(1.0, 0.0, 0.0, 1.0);
        let mut s = StrokeBuffer::new(&st);
        s.stamp(&dab(10.5, 10.5, 3.0, 1.0));
        s.commit_into(&mut target);
        assert_eq!(target.pixel(10, 10), [128, 0, 128, 255]);
    }
}
