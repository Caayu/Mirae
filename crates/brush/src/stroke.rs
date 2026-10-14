use tela_canvas::{DirtyTiles, TILE_SIZE, Tile, TileCoord, TiledSurface};
use tela_color::{PremulRgba, Rgba};

use crate::Dab;
use crate::preset::PaintMode;
use crate::texture::CanvasTexture;
use crate::tip::{DabShape, PreparedTip};

/// Receives the dabs of a stroke. Dabs are either *committed* (final) or part
/// of the provisional *tail*, which is re-emitted from scratch every time it
/// changes (between `begin_tail` and the next `begin_tail`/`commit_tail`).
pub trait DabSink {
    fn dab(&mut self, dab: &Dab);
    fn begin_tail(&mut self);
    fn tail_dab(&mut self, dab: &Dab);
    /// Makes the current tail final.
    fn commit_tail(&mut self);
}

/// Collects dabs in memory; the final stroke is `committed` followed by
/// `tail`. Used by tests and analysis.
#[derive(Clone, Debug, Default)]
pub struct DabLog {
    pub committed: Vec<Dab>,
    pub tail: Vec<Dab>,
}

impl DabLog {
    pub fn all(&self) -> impl Iterator<Item = &Dab> {
        self.committed.iter().chain(&self.tail)
    }
}

impl DabSink for DabLog {
    fn dab(&mut self, dab: &Dab) {
        self.committed.push(*dab);
    }
    fn begin_tail(&mut self) {
        self.tail.clear();
    }
    fn tail_dab(&mut self, dab: &Dab) {
        self.tail.push(*dab);
    }
    fn commit_tail(&mut self) {
        self.committed.append(&mut self.tail);
    }
}

/// Alpha of the stroke in progress, kept apart from the layer.
///
/// Each dab pulls covered pixels toward its own opacity ceiling:
/// `if op > m { m += (op - m) * flow * coverage }`. Alpha therefore never
/// exceeds the largest opacity that touched a pixel, however many dabs
/// overlap, and flow sets how fast overlapping dabs approach it. With a
/// constant opacity this is exactly `m/op += flow*cov*(1 - m/op)`.
///
/// The provisional tail (see [`DabSink`]) lives in copy-on-write tiles on top
/// of the committed mask, so it can be rebuilt cheaply every frame.
#[derive(Clone, Debug)]
pub struct StrokeBuffer {
    committed: TiledSurface<f32>,
    tail: TiledSurface<f32>,
    color: Rgba,
    ctx: StampContext,
    /// Tiles changed since the last [`StrokeBuffer::take_dirty`].
    dirty: DirtyTiles,
    /// Every tile the stroke has touched.
    touched: DirtyTiles,
}

/// Everything constant across the dabs of one stroke.
#[derive(Clone, Debug, Default)]
pub struct StampContext {
    pub tip: PreparedTip,
    pub texture: Option<CanvasTexture>,
    pub anti_aliasing: bool,
    pub mode: PaintMode,
}

fn stamp_into(
    mask: &mut TiledSurface<f32>,
    seed: Option<&TiledSurface<f32>>,
    dab: &Dab,
    ctx: &StampContext,
    mut on_tile: impl FnMut(TileCoord),
) {
    let flow = dab.flow.clamp(0.0, 1.0);
    let ceiling = dab.opacity.clamp(0.0, 1.0);
    if flow <= 0.0 || ceiling <= 0.0 {
        return;
    }
    let shape = DabShape::new(&ctx.tip, dab, ctx.anti_aliasing);
    let texture = ctx.texture.as_ref().filter(|_| dab.texture > 0.0);
    let bounds = shape.bounds();
    for coord in bounds.tiles() {
        let area = bounds.intersect(&coord.rect());
        let (ox, oy) = coord.origin();
        if mask.tile(coord).is_none() {
            let base = seed
                .and_then(|s| s.tile(coord))
                .cloned()
                .unwrap_or_else(Tile::new);
            mask.insert_tile(coord, base);
        }
        let tile = mask.tile_mut(coord).expect("inserted above");
        for y in area.y0..area.y1 {
            let py = y as f32 + 0.5;
            let row = tile.row_mut((y - oy) as u32);
            for x in area.x0..area.x1 {
                let m = &mut row[(x - ox) as usize];
                if *m >= ceiling {
                    continue;
                }
                let px = x as f32 + 0.5;
                let cov = shape.coverage(px, py);
                if cov <= 0.0 {
                    continue;
                }
                let target = match texture {
                    Some(t) => t.modulate(ceiling, cov, t.height(px, py), dab.texture),
                    None => ceiling,
                };
                if target > *m {
                    *m += (target - *m) * flow * cov;
                }
            }
        }
        on_tile(coord);
    }
}

impl StrokeBuffer {
    pub fn new(color: Rgba, ctx: StampContext) -> Self {
        Self {
            committed: TiledSurface::new(),
            tail: TiledSurface::new(),
            color,
            ctx,
            dirty: DirtyTiles::new(),
            touched: DirtyTiles::new(),
        }
    }

    /// Alpha mask tile as currently displayed (tail over committed).
    pub fn mask_tile(&self, coord: TileCoord) -> Option<&Tile<f32>> {
        self.tail.tile(coord).or_else(|| self.committed.tile(coord))
    }

    /// Alpha at one pixel as currently displayed.
    pub fn alpha(&self, x: i32, y: i32) -> f32 {
        let c = TileCoord::containing(x, y);
        let (ox, oy) = c.origin();
        self.mask_tile(c)
            .map_or(0.0, |t| t.get((x - ox) as u32, (y - oy) as u32))
    }

    pub fn touched(&self) -> &DirtyTiles {
        &self.touched
    }

    pub fn take_dirty(&mut self) -> DirtyTiles {
        std::mem::take(&mut self.dirty)
    }

    /// Premultiplied paint color at full alpha.
    pub fn paint(&self) -> PremulRgba {
        self.color.premultiply()
    }

    /// Composites the stroke over one tile of premultiplied pixels.
    pub fn paint_over_tile(&self, coord: TileCoord, pixels: &mut [PremulRgba]) {
        let Some(mask) = self.mask_tile(coord) else {
            return;
        };
        match self.ctx.mode {
            // Destination-out: the stroke's alpha removes layer alpha.
            PaintMode::Erase => {
                for (dst, &m) in pixels.iter_mut().zip(mask.pixels()) {
                    if m > 0.0 {
                        *dst = dst.scale(1.0 - m);
                    }
                }
            }
            PaintMode::Paint | PaintMode::Unknown => {
                let paint = self.paint();
                for (dst, &m) in pixels.iter_mut().zip(mask.pixels()) {
                    if m > 0.0 {
                        *dst = paint.scale(m).over(*dst);
                    }
                }
            }
        }
    }

    /// Writes the finished stroke into an 8-bit premultiplied surface.
    pub fn commit_into(&self, target: &mut TiledSurface<[u8; 4]>) -> DirtyTiles {
        let mut scratch = vec![PremulRgba::TRANSPARENT; (TILE_SIZE * TILE_SIZE) as usize];
        for coord in self.touched.iter() {
            if self.mask_tile(coord).is_none() {
                continue;
            }
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

impl DabSink for StrokeBuffer {
    fn dab(&mut self, dab: &Dab) {
        let (dirty, touched) = (&mut self.dirty, &mut self.touched);
        stamp_into(&mut self.committed, None, dab, &self.ctx, |c| {
            dirty.mark(c);
            touched.mark(c);
        });
        // Tail tiles were seeded from the old committed state; drop them so the
        // owner rebuilds the tail on top of the new one.
        if self.tail.tile_count() > 0 {
            self.begin_tail();
        }
    }

    fn begin_tail(&mut self) {
        for c in self.tail.tile_coords() {
            self.dirty.mark(c);
        }
        self.tail.clear();
    }

    fn tail_dab(&mut self, dab: &Dab) {
        let (dirty, touched) = (&mut self.dirty, &mut self.touched);
        stamp_into(&mut self.tail, Some(&self.committed), dab, &self.ctx, |c| {
            dirty.mark(c);
            touched.mark(c);
        });
    }

    fn commit_tail(&mut self) {
        let coords: Vec<TileCoord> = self.tail.tile_coords().collect();
        for c in coords {
            if let Some(t) = self.tail.remove_tile(c) {
                self.committed.insert_tile(c, t);
                self.dirty.mark(c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;

    fn dab(x: f32, y: f32, radius: f32, opacity: f32, flow: f32) -> Dab {
        Dab {
            center: Vec2::new(x, y),
            radius,
            opacity,
            flow,
            ..Dab::default()
        }
    }

    fn buffer() -> StrokeBuffer {
        StrokeBuffer::new(
            Rgba::BLACK,
            StampContext {
                anti_aliasing: true,
                ..Default::default()
            },
        )
    }

    #[test]
    fn opacity_caps_overlapping_dabs() {
        let mut s = buffer();
        for _ in 0..50 {
            s.dab(&dab(10.0, 10.0, 5.0, 0.5, 0.3));
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
        let mut s = buffer();
        s.dab(&dab(10.0, 10.0, 5.0, 1.0, 0.25));
        let one = s.alpha(10, 10);
        s.dab(&dab(10.0, 10.0, 5.0, 1.0, 0.25));
        let two = s.alpha(10, 10);
        assert!((one - 0.25).abs() < 1e-6);
        assert!((two - (0.25 + 0.25 * 0.75)).abs() < 1e-6);
    }

    #[test]
    fn lighter_dab_never_lightens_existing_paint() {
        let mut s = buffer();
        s.dab(&dab(10.0, 10.0, 5.0, 0.9, 1.0));
        s.dab(&dab(10.0, 10.0, 5.0, 0.2, 1.0));
        assert!((s.alpha(10, 10) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn dab_crossing_tile_corner_touches_four_tiles() {
        let mut s = buffer();
        s.dab(&dab(256.0, 256.0, 4.0, 1.0, 1.0));
        assert_eq!(s.touched().len(), 4);
        assert_eq!(s.take_dirty().len(), 4);
        assert!(s.take_dirty().is_empty());
    }

    #[test]
    fn commit_composites_over_existing_pixels() {
        let mut target = TiledSurface::new();
        target.set_pixel(10, 10, [0, 0, 255, 255]);
        let mut s = StrokeBuffer::new(
            Rgba::new(1.0, 0.0, 0.0, 1.0),
            StampContext {
                anti_aliasing: true,
                ..Default::default()
            },
        );
        s.dab(&dab(10.5, 10.5, 3.0, 0.5, 1.0));
        s.commit_into(&mut target);
        assert_eq!(target.pixel(10, 10), [128, 0, 128, 255]);
    }

    #[test]
    fn erase_mode_removes_alpha_through_the_same_engine() {
        let mut target = TiledSurface::new();
        target.set_pixel(10, 10, [200, 100, 0, 255]);
        target.set_pixel(40, 10, [200, 100, 0, 255]);
        let ctx = StampContext {
            anti_aliasing: true,
            mode: PaintMode::Erase,
            ..Default::default()
        };
        let mut s = StrokeBuffer::new(Rgba::BLACK, ctx);
        s.dab(&dab(10.5, 10.5, 3.0, 0.5, 1.0));
        s.commit_into(&mut target);
        assert_eq!(
            target.pixel(10, 10),
            [100, 50, 0, 128],
            "half erased, color kept"
        );
        assert_eq!(target.pixel(40, 10), [200, 100, 0, 255], "untouched");
    }

    #[test]
    fn tail_is_rebuilt_from_committed_and_can_be_committed() {
        let mut s = buffer();
        s.dab(&dab(10.0, 10.0, 3.0, 0.5, 1.0));
        s.begin_tail();
        s.tail_dab(&dab(10.0, 10.0, 3.0, 1.0, 1.0));
        s.tail_dab(&dab(40.0, 10.0, 3.0, 1.0, 1.0));
        assert!((s.alpha(10, 10) - 1.0).abs() < 1e-6);
        assert!(s.alpha(40, 10) > 0.99);

        // Rebuilding the tail elsewhere restores the committed state.
        s.take_dirty();
        s.begin_tail();
        s.tail_dab(&dab(80.0, 10.0, 3.0, 1.0, 1.0));
        assert!((s.alpha(10, 10) - 0.5).abs() < 1e-6);
        assert_eq!(s.alpha(40, 10), 0.0);
        assert!(s.take_dirty().contains(TileCoord { x: 0, y: 0 }));

        s.commit_tail();
        s.begin_tail();
        assert!(s.alpha(80, 10) > 0.99, "committed tail survives a new tail");
    }

    #[test]
    fn dab_log_orders_committed_then_tail() {
        let mut log = DabLog::default();
        log.dab(&dab(0.0, 0.0, 1.0, 1.0, 1.0));
        log.begin_tail();
        log.tail_dab(&dab(1.0, 0.0, 1.0, 1.0, 1.0));
        log.begin_tail();
        log.tail_dab(&dab(2.0, 0.0, 1.0, 1.0, 1.0));
        assert_eq!(log.all().count(), 2);
        log.commit_tail();
        assert_eq!(log.committed.len(), 2);
    }
}
