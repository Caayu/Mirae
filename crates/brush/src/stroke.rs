use tela_canvas::{DirtyTiles, TILE_SIZE, Tile, TileCoord, TiledSurface};
use tela_color::{Color, PremulRgba, StoragePixel, unorm8};

use crate::Dab;
use crate::preset::PaintMode;
use crate::texture::CanvasTexture;
use crate::tip::{DabShape, PreparedTip};

/// Receives the dabs of a stroke. Dabs are either *committed* (final) or part
/// of the provisional *tail*, which is re-emitted from scratch every time it
/// changes (between `begin_tail` and the next `begin_tail`/`commit_tail`).
pub trait DabSink {
    /// One committed (final) dab.
    fn dab(&mut self, dab: &Dab);
    /// Several committed dabs, in order. Sinks may process them in bulk.
    fn dabs(&mut self, dabs: &[Dab]) {
        for d in dabs {
            self.dab(d);
        }
    }
    /// Discards the previous tail; the dabs that follow replace it.
    fn begin_tail(&mut self);
    /// One provisional dab of the current tail.
    fn tail_dab(&mut self, dab: &Dab);
    /// Several tail dabs, in order.
    fn tail_dabs(&mut self, dabs: &[Dab]) {
        for d in dabs {
            self.tail_dab(d);
        }
    }
    /// Makes the current tail final.
    fn commit_tail(&mut self);
}

/// Collects dabs in memory; the final stroke is `committed` followed by
/// `tail`. Used by tests and analysis.
#[derive(Clone, Debug, Default)]
pub struct DabLog {
    /// Final dabs, in order.
    pub committed: Vec<Dab>,
    /// Current provisional tail.
    pub tail: Vec<Dab>,
}

impl DabLog {
    /// The stroke as currently displayed: committed then tail.
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
    color: Color,
    ctx: StampContext,
    /// Tiles changed since the last [`StrokeBuffer::take_dirty`].
    dirty: DirtyTiles,
    /// Every tile the stroke has touched.
    touched: DirtyTiles,
    scratch: BatchScratch,
}

/// Whether a stroke may change the alpha of the pixels it paints on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PixelAlpha {
    /// Painting adds alpha, erasing removes it.
    #[default]
    Free,
    /// "Lock transparent pixels": painting is source-atop (colour changes
    /// only where there is alpha, alpha never changes); erasing still
    /// erases, as in the reference application (parity record 4D A1–A3).
    Preserved,
}

/// Everything constant across the dabs of one stroke.
#[derive(Clone, Debug, Default)]
pub struct StampContext {
    /// Tip, prepared once per stroke.
    pub tip: PreparedTip,
    /// Paper texture, if the preset has one.
    pub texture: Option<CanvasTexture>,
    /// Analytic one-pixel edge.
    pub anti_aliasing: bool,
    /// Paint or erase when composited.
    pub mode: PaintMode,
    /// Allow large batches to be stamped on several threads.
    pub parallel: bool,
}

fn visible(dab: &Dab) -> bool {
    dab.flow > 0.0 && dab.opacity > 0.0
}

/// Stamps the part of `dab` that falls inside tile `coord`.
fn stamp_tile(tile: &mut Tile<f32>, coord: TileCoord, dab: &Dab, ctx: &StampContext) {
    let flow = dab.flow.clamp(0.0, 1.0);
    let ceiling = dab.opacity.clamp(0.0, 1.0);
    let shape = DabShape::new(&ctx.tip, dab, ctx.anti_aliasing);
    let texture = ctx.texture.as_ref().filter(|_| dab.texture > 0.0);
    let area = shape.bounds().intersect(&coord.rect());
    let (ox, oy) = coord.origin();
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
}

fn take_tile(
    mask: &mut TiledSurface<f32>,
    seed: Option<&TiledSurface<f32>>,
    coord: TileCoord,
) -> Tile<f32> {
    mask.remove_tile(coord).unwrap_or_else(|| {
        seed.and_then(|s| s.tile(coord))
            .cloned()
            .unwrap_or_else(Tile::new)
    })
}

fn stamp_into(
    mask: &mut TiledSurface<f32>,
    seed: Option<&TiledSurface<f32>>,
    dab: &Dab,
    ctx: &StampContext,
    mut on_tile: impl FnMut(TileCoord),
) {
    if !visible(dab) {
        return;
    }
    let bounds = DabShape::new(&ctx.tip, dab, ctx.anti_aliasing).bounds();
    for coord in bounds.tiles() {
        if mask.tile(coord).is_none() {
            let base = take_tile(mask, seed, coord);
            mask.insert_tile(coord, base);
        }
        let tile = mask.tile_mut(coord).expect("inserted above");
        stamp_tile(tile, coord, dab, ctx);
        on_tile(coord);
    }
}

/// Estimated pixel work below which a batch is stamped on one thread:
/// spreading a few small dabs over threads costs more than it saves.
/// Chosen from `stamp_bench` (see docs/phase3).
pub const PARALLEL_MIN_WORK: f64 = 60_000.0;

/// Reusable buffers for batched stamping.
#[derive(Clone, Debug, Default)]
struct BatchScratch {
    /// `(tile, dab index)`, sorted so each tile's dabs keep stroke order.
    pairs: Vec<(TileCoord, u32)>,
    jobs: Vec<TileJob>,
}

#[derive(Clone, Debug)]
struct TileJob {
    coord: TileCoord,
    tile: Tile<f32>,
    pairs: std::ops::Range<usize>,
}

/// Stamps a batch of dabs. Large batches are split by tile and run in
/// parallel: each tile is moved out of the surface, owned by exactly one
/// worker, receives its dabs in stroke order, and is moved back. Per-pixel
/// operation order is the same as sequential stamping, so the result is
/// bit-identical; no locks are taken.
fn stamp_batch(
    mask: &mut TiledSurface<f32>,
    seed: Option<&TiledSurface<f32>>,
    batch: &[Dab],
    ctx: &StampContext,
    scratch: &mut BatchScratch,
    mut on_tile: impl FnMut(TileCoord),
) {
    let work: f64 = batch
        .iter()
        .filter(|d| visible(d))
        .map(|d| {
            let b = DabShape::new(&ctx.tip, d, ctx.anti_aliasing).bounds();
            b.width() as f64 * b.height() as f64
        })
        .sum();
    if !ctx.parallel || work < PARALLEL_MIN_WORK {
        for d in batch {
            stamp_into(mask, seed, d, ctx, &mut on_tile);
        }
        return;
    }
    let BatchScratch { pairs, jobs } = scratch;
    pairs.clear();
    for (i, d) in batch.iter().enumerate().filter(|(_, d)| visible(d)) {
        let bounds = DabShape::new(&ctx.tip, d, ctx.anti_aliasing).bounds();
        pairs.extend(bounds.tiles().map(|c| (c, i as u32)));
    }
    pairs.sort_unstable();
    jobs.clear();
    let mut start = 0;
    while start < pairs.len() {
        let coord = pairs[start].0;
        let end = start + pairs[start..].iter().take_while(|p| p.0 == coord).count();
        jobs.push(TileJob {
            coord,
            tile: take_tile(mask, seed, coord),
            pairs: start..end,
        });
        start = end;
    }
    let pairs = &*pairs;
    rayon::iter::ParallelIterator::for_each(
        rayon::iter::IntoParallelRefMutIterator::par_iter_mut(jobs),
        |job| {
            for &(_, i) in &pairs[job.pairs.clone()] {
                stamp_tile(&mut job.tile, job.coord, &batch[i as usize], ctx);
            }
        },
    );
    for job in jobs.drain(..) {
        on_tile(job.coord);
        mask.insert_tile(job.coord, job.tile);
    }
}

impl StrokeBuffer {
    /// Empty stroke painting `color`. The colour is fixed for the whole
    /// stroke (invariant COL2); it has no alpha of its own: how much paint
    /// lands is the mask's business (see [`StrokeBuffer::paint_over_tile`]).
    pub fn new(color: Color, ctx: StampContext) -> Self {
        Self {
            committed: TiledSurface::new(),
            tail: TiledSurface::new(),
            color,
            ctx,
            dirty: DirtyTiles::new(),
            touched: DirtyTiles::new(),
            scratch: BatchScratch::default(),
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

    /// Every tile the stroke has touched (exactly the tiles an undo
    /// snapshot needs).
    pub fn touched(&self) -> &DirtyTiles {
        &self.touched
    }

    /// Tiles changed since the last call; the display recomposes these.
    pub fn take_dirty(&mut self) -> DirtyTiles {
        std::mem::take(&mut self.dirty)
    }

    /// The stroke's colour.
    pub fn color(&self) -> Color {
        self.color
    }

    /// Premultiplied paint colour at full alpha.
    pub fn paint(&self) -> PremulRgba {
        self.color.premultiplied(1.0)
    }

    /// Composites the stroke over one tile of premultiplied pixels.
    /// `alpha` says whether the stroke may change the pixels' alpha.
    ///
    /// The colour math, per pixel (ADR 0021): the mask value `m` already
    /// combines everything the brush decides — dab coverage, flow (how fast
    /// overlapping dabs build up) and opacity (the ceiling `m` never
    /// exceeds), see [`StrokeBuffer`]. The colour `C` (straight, opaque)
    /// only says *what* lands: the source is `(C·m, m)` premultiplied.
    /// Paint is `src over dst`; with alpha preserved it is source-atop,
    /// `rgb = C·m·αd + rgb_d·(1 − m)`, `α = αd`; erase is
    /// `dst·(1 − m)` and ignores `C` (invariant COL3). Layer opacity, mask
    /// and blend mode apply later, at composition.
    pub fn paint_over_tile(&self, coord: TileCoord, pixels: &mut [PremulRgba], alpha: PixelAlpha) {
        if let Some(mask) = self.mask_tile(coord) {
            self.paint_over(pixels, mask.pixels(), alpha);
        }
    }

    /// The one compositing rule shared by preview and commit: `pixels` and
    /// `mask` are matching runs of the same tile (whole tile or one row).
    fn paint_over(&self, pixels: &mut [PremulRgba], mask: &[f32], alpha: PixelAlpha) {
        match (self.ctx.mode, alpha) {
            // Destination-out: the stroke's alpha removes layer alpha. A
            // preserved alpha does not stop the eraser (reference
            // behaviour, parity record 4D A3).
            (PaintMode::Erase, _) => {
                for (dst, &m) in pixels.iter_mut().zip(mask) {
                    if m > 0.0 {
                        *dst = dst.scale(1.0 - m);
                    }
                }
            }
            (PaintMode::Paint | PaintMode::Unknown, PixelAlpha::Free) => {
                let paint = self.paint();
                for (dst, &m) in pixels.iter_mut().zip(mask) {
                    if m > 0.0 {
                        *dst = paint.scale(m).over(*dst);
                    }
                }
            }
            // Source-atop: colour changes where there is alpha, alpha
            // never does (parity record 4D A1, A2).
            (PaintMode::Paint | PaintMode::Unknown, PixelAlpha::Preserved) => {
                let paint = self.paint();
                for (dst, &m) in pixels.iter_mut().zip(mask) {
                    if m > 0.0 && dst.a > 0.0 {
                        let s = paint.scale(m);
                        let keep = 1.0 - s.a;
                        *dst = PremulRgba::new(
                            s.r * dst.a + dst.r * keep,
                            s.g * dst.a + dst.g * keep,
                            s.b * dst.a + dst.b * keep,
                            dst.a,
                        );
                    }
                }
            }
        }
    }

    /// Writes the finished stroke into one tile of a premultiplied layer
    /// of either depth (the tile at `coord`): the same rule as the
    /// preview ([`StrokeBuffer::paint_over_tile`]), quantized once to the
    /// tile's storage. Does nothing where the stroke has no mask. Works
    /// row by row on the stack: no allocation.
    pub fn commit_tile<P: StoragePixel>(
        &self,
        coord: TileCoord,
        tile: &mut Tile<P>,
        alpha: PixelAlpha,
    ) {
        let Some(mask) = self.mask_tile(coord) else {
            return;
        };
        let mut row = [PremulRgba::TRANSPARENT; TILE_SIZE as usize];
        for y in 0..TILE_SIZE {
            let pixels = tile.row_mut(y);
            for (s, p) in row.iter_mut().zip(pixels.iter()) {
                *s = p.to_premul();
            }
            self.paint_over(&mut row, mask.row(y), alpha);
            for (p, s) in pixels.iter_mut().zip(&row) {
                *p = P::from_premul(*s);
            }
        }
    }

    /// Writes the finished stroke into a premultiplied surface
    /// ([`StrokeBuffer::commit_tile`] for every touched tile with paint,
    /// alpha free).
    pub fn commit_into<P: StoragePixel>(&self, target: &mut TiledSurface<P>) -> DirtyTiles {
        for coord in self.touched.iter() {
            if self.mask_tile(coord).is_some() {
                self.commit_tile(coord, target.tile_or_insert(coord), PixelAlpha::Free);
            }
        }
        self.touched.clone()
    }

    /// Applies the stroke to one tile of a single alpha channel (`0..=1`,
    /// e.g. how much a layer mask reveals): painting adds the stroke's
    /// alpha (`a + m·(1 − a)`), erasing removes it (`a·(1 − m)`). The
    /// stroke's colour plays no part (reference behaviour for masks,
    /// parity record 4D M2–M4). Shared by preview and commit.
    pub fn paint_alpha_tile(&self, coord: TileCoord, values: &mut [f32]) {
        if let Some(mask) = self.mask_tile(coord) {
            self.alpha_over(values, mask.pixels());
        }
    }

    fn alpha_over(&self, values: &mut [f32], mask: &[f32]) {
        match self.ctx.mode {
            PaintMode::Erase => {
                for (v, &m) in values.iter_mut().zip(mask) {
                    if m > 0.0 {
                        *v *= 1.0 - m;
                    }
                }
            }
            PaintMode::Paint | PaintMode::Unknown => {
                for (v, &m) in values.iter_mut().zip(mask) {
                    if m > 0.0 {
                        *v = m + *v * (1.0 - m);
                    }
                }
            }
        }
    }

    /// Writes the finished stroke into one tile of an 8-bit alpha channel
    /// (255 = full), with [`StrokeBuffer::paint_alpha_tile`]'s rule. Row by
    /// row on the stack: no allocation.
    pub fn commit_alpha_tile(&self, coord: TileCoord, tile: &mut Tile<u8>) {
        let Some(mask) = self.mask_tile(coord) else {
            return;
        };
        let mut row = [0.0f32; TILE_SIZE as usize];
        for y in 0..TILE_SIZE {
            let values = tile.row_mut(y);
            for (r, v) in row.iter_mut().zip(values.iter()) {
                *r = f32::from(*v) / 255.0;
            }
            self.alpha_over(&mut row, mask.row(y));
            for (v, r) in values.iter_mut().zip(&row) {
                *v = unorm8(*r);
            }
        }
    }
}

impl DabSink for StrokeBuffer {
    fn dabs(&mut self, dabs: &[Dab]) {
        let Self {
            committed,
            ctx,
            scratch,
            dirty,
            touched,
            ..
        } = self;
        stamp_batch(committed, None, dabs, ctx, scratch, |c| {
            dirty.mark(c);
            touched.mark(c);
        });
        if self.tail.tile_count() > 0 {
            self.begin_tail();
        }
    }

    fn tail_dabs(&mut self, dabs: &[Dab]) {
        let Self {
            committed,
            tail,
            ctx,
            scratch,
            dirty,
            touched,
            ..
        } = self;
        stamp_batch(tail, Some(committed), dabs, ctx, scratch, |c| {
            dirty.mark(c);
            touched.mark(c);
        });
    }

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
            Color::BLACK,
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
        let mut target = TiledSurface::<[u8; 4]>::new();
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
        let mut target = TiledSurface::<[u8; 4]>::new();
        target.set_pixel(10, 10, [0, 0, 255, 255]);
        let mut s = StrokeBuffer::new(
            Color::new(1.0, 0.0, 0.0),
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
        let mut target = TiledSurface::<[u8; 4]>::new();
        target.set_pixel(10, 10, [200, 100, 0, 255]);
        target.set_pixel(40, 10, [200, 100, 0, 255]);
        let ctx = StampContext {
            anti_aliasing: true,
            mode: PaintMode::Erase,
            ..Default::default()
        };
        let mut s = StrokeBuffer::new(Color::BLACK, ctx);
        s.dab(&dab(10.5, 10.5, 3.0, 0.5, 1.0));
        s.commit_into(&mut target);
        assert_eq!(
            target.pixel(10, 10),
            [100, 50, 0, 128],
            "half erased, color kept"
        );
        assert_eq!(target.pixel(40, 10), [200, 100, 0, 255], "untouched");
    }

    /// A tile with opaque, half-transparent and empty pixels of one colour
    /// at x = 10, 20, 30 (y = 10).
    fn bands() -> Tile<[u8; 4]> {
        let mut t = Tile::new();
        *t.get_mut(10, 10) = [200, 160, 60, 255];
        *t.get_mut(20, 10) = [100, 80, 30, 128];
        t
    }

    fn stroke_over(xs: &[f32], opacity: f32, mode: PaintMode) -> StrokeBuffer {
        let ctx = StampContext {
            anti_aliasing: true,
            mode,
            ..Default::default()
        };
        let mut s = StrokeBuffer::new(Color::BLACK, ctx);
        for &x in xs {
            s.dab(&dab(x + 0.5, 10.5, 3.0, opacity, 1.0));
        }
        s
    }

    #[test]
    fn preserved_alpha_paints_atop_and_never_changes_alpha() {
        let c = TileCoord { x: 0, y: 0 };
        let s = stroke_over(&[10.0, 20.0, 30.0], 0.5, PaintMode::Paint);
        let mut t = bands();
        s.commit_tile(c, &mut t, PixelAlpha::Preserved);
        // Colour moves halfway to black; alpha stays; empty stays empty.
        assert_eq!(t.get(10, 10), [100, 80, 30, 255]);
        assert_eq!(t.get(20, 10), [50, 40, 15, 128]);
        assert_eq!(t.get(30, 10), [0, 0, 0, 0]);
        // Free alpha paints over everything.
        let mut free = bands();
        s.commit_tile(c, &mut free, PixelAlpha::Free);
        assert_eq!(free.get(30, 10), [0, 0, 0, 128]);
        assert_eq!(free.get(20, 10)[3], 192, "0.5 + 0.502 * 0.5");
    }

    #[test]
    fn preserved_alpha_matches_the_reference_edge_measurement() {
        // Parity record 4D A2: black at coverage 0.47 over 200,160,60 at
        // 50 % alpha shows 180,170,143 over white paper.
        let s = stroke_over(&[], 1.0, PaintMode::Paint);
        let dst = PremulRgba::from_rgba8([100, 80, 30, 128]);
        let mut px = [dst];
        s.paint_over(&mut px, &[0.47], PixelAlpha::Preserved);
        let shown = px[0].over(PremulRgba::new(1.0, 1.0, 1.0, 1.0)).to_rgba8();
        let reference = [180u8, 170, 143, 255];
        let diff = shown
            .iter()
            .zip(reference)
            .map(|(a, b)| a.abs_diff(b))
            .max();
        assert!(diff <= Some(1), "{shown:?} vs {reference:?}");
        assert!((px[0].a - dst.a).abs() < 1e-7);
    }

    #[test]
    fn preserved_alpha_recolours_like_the_reference() {
        // Parity record 5A, O16: blue over 220,80,40 at alpha 128 with the
        // lock on shows 127,127,255 at full coverage and 186,148,198 at
        // the edge pixel (coverage ≈ 0.466) over white paper; alpha never
        // changes (COL5).
        let blue = StrokeBuffer::new(Color::new(0.0, 0.0, 1.0), StampContext::default());
        let dst = PremulRgba::from_rgba8([110, 40, 20, 128]);
        let paper = PremulRgba::new(1.0, 1.0, 1.0, 1.0);
        for (m, reference) in [(1.0, [127u8, 127, 255, 255]), (0.466, [186, 148, 198, 255])] {
            let mut px = [dst];
            blue.paint_over(&mut px, &[m], PixelAlpha::Preserved);
            assert!((px[0].a - dst.a).abs() < 1e-7, "COL5");
            let shown = px[0].over(paper).to_rgba8();
            for (a, b) in shown.iter().zip(reference) {
                assert!(a.abs_diff(b) <= 1, "m {m}: {shown:?} vs {reference:?}");
            }
        }
    }

    #[test]
    fn colour_never_changes_what_the_eraser_does() {
        // COL3, with the exact pixels: red, blue and black erase alike.
        let dst = PremulRgba::from_rgba8([110, 40, 20, 128]);
        let ctx = StampContext {
            mode: PaintMode::Erase,
            ..Default::default()
        };
        let erased: Vec<_> = [
            Color::new(1.0, 0.0, 0.0),
            Color::new(0.0, 0.0, 1.0),
            Color::BLACK,
        ]
        .into_iter()
        .map(|c| {
            let mut px = [dst; 3];
            StrokeBuffer::new(c, ctx.clone()).paint_over(
                &mut px,
                &[0.3, 0.7, 1.0],
                PixelAlpha::Free,
            );
            px
        })
        .collect();
        assert!(erased.windows(2).all(|w| w[0] == w[1]));
        assert_eq!(erased[0][2], PremulRgba::TRANSPARENT);
    }

    #[test]
    fn colored_paint_follows_the_documented_formula() {
        // (C·m, m) over dst, channel by channel (ADR 0021).
        let c = Color::from_rgb8([37, 149, 213]);
        let s = StrokeBuffer::new(c, StampContext::default());
        let dst = PremulRgba::from_rgba8([200, 50, 20, 255]);
        let m = 0.51;
        let mut px = [dst];
        s.paint_over(&mut px, &[m], PixelAlpha::Free);
        let expect = |ci: f32, di: f32| ci * m + di * (1.0 - m);
        assert!((px[0].r - expect(c.r(), dst.r)).abs() < 1e-6);
        assert!((px[0].g - expect(c.g(), dst.g)).abs() < 1e-6);
        assert!((px[0].b - expect(c.b(), dst.b)).abs() < 1e-6);
        assert!((px[0].a - 1.0).abs() < 1e-6);
        // The reference's 51 % stroke of this colour over white paper
        // shows 144,201,234 (parity record 5A, O15).
        let mut px = [PremulRgba::new(1.0, 1.0, 1.0, 1.0)];
        s.paint_over(&mut px, &[m], PixelAlpha::Free);
        assert_eq!(px[0].to_rgba8(), [144, 201, 234, 255]);
    }

    #[test]
    fn preserved_alpha_does_not_stop_the_eraser() {
        let c = TileCoord { x: 0, y: 0 };
        let s = stroke_over(&[10.0, 20.0], 1.0, PaintMode::Erase);
        let mut t = bands();
        s.commit_tile(c, &mut t, PixelAlpha::Preserved);
        assert_eq!(t.get(10, 10), [0, 0, 0, 0]);
        assert_eq!(t.get(20, 10), [0, 0, 0, 0]);
    }

    #[test]
    fn alpha_channel_painting_adds_and_erasing_removes_stroke_alpha() {
        let c = TileCoord { x: 0, y: 0 };
        let mut t: Tile<u8> = Tile::new();
        *t.get_mut(20, 10) = 255;
        *t.get_mut(30, 10) = 128;
        let paint = stroke_over(&[10.0, 30.0], 0.5, PaintMode::Paint);
        paint.commit_alpha_tile(c, &mut t);
        assert_eq!(t.get(10, 10), 128, "half the stroke's alpha added");
        assert_eq!(t.get(30, 10), 192, "a + m(1 - a)");
        let erase = stroke_over(&[20.0, 30.0], 0.5, PaintMode::Erase);
        erase.commit_alpha_tile(c, &mut t);
        assert_eq!(t.get(20, 10), 128, "a(1 - m)");
        assert_eq!(t.get(30, 10), 96);
        assert_eq!(t.get(50, 10), 0, "untouched");
    }

    #[test]
    fn alpha_channel_ignores_the_paint_colour() {
        let c = TileCoord { x: 0, y: 0 };
        let mut results = Vec::new();
        for color in [Color::BLACK, Color::WHITE, Color::new(1.0, 0.2, 0.4)] {
            let mut s = StrokeBuffer::new(
                color,
                StampContext {
                    anti_aliasing: true,
                    ..Default::default()
                },
            );
            s.dab(&dab(10.5, 10.5, 3.0, 1.0, 1.0));
            let mut t: Tile<u8> = Tile::new();
            s.commit_alpha_tile(c, &mut t);
            results.push(t);
        }
        assert!(results.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn alpha_channel_preview_matches_commit() {
        let c = TileCoord { x: 0, y: 0 };
        let mut s = buffer();
        for i in 0..20 {
            s.dab(&dab(5.0 + i as f32 * 3.7, 30.0, 6.0, 0.7, 0.4));
        }
        let mut t: Tile<u8> = Tile::new();
        for (i, v) in t.pixels_mut().iter_mut().enumerate() {
            *v = (i % 251) as u8;
        }
        let mut preview: Vec<f32> = t.pixels().iter().map(|&v| f32::from(v) / 255.0).collect();
        s.paint_alpha_tile(c, &mut preview);
        s.commit_alpha_tile(c, &mut t);
        let diff = preview
            .iter()
            .zip(t.pixels())
            .map(|(p, &v)| unorm8(*p).abs_diff(v))
            .max()
            .unwrap();
        assert_eq!(diff, 0, "commit is the preview rounded once");
    }
    #[test]
    fn parallel_stamping_is_bit_identical_to_sequential() {
        use crate::texture::{CanvasTexture, TextureMode, TextureSettings};
        let batch: Vec<Dab> = (0..60)
            .map(|i| Dab {
                center: Vec2::new(200.0 + i as f32 * 9.0, 300.0 + (i as f32).sin() * 40.0),
                radius: 90.0 + (i % 7) as f32 * 10.0,
                aspect: 0.6,
                angle: i as f32 * 0.2,
                opacity: 0.4 + (i % 3) as f32 * 0.2,
                flow: 0.3,
                hardness: (i % 5) as f32 * 0.25,
                texture: 0.8,
            })
            .collect();
        let run = |parallel| {
            let ctx = StampContext {
                anti_aliasing: true,
                texture: Some(CanvasTexture::new(&TextureSettings {
                    mode: TextureMode::Height,
                    ..Default::default()
                })),
                parallel,
                ..Default::default()
            };
            let mut s = StrokeBuffer::new(Color::BLACK, ctx);
            s.dabs(&batch[..40]);
            s.begin_tail();
            s.tail_dabs(&batch[40..]);
            s
        };
        let (seq, par) = (run(false), run(true));
        assert_eq!(seq.touched().len(), par.touched().len());
        assert!(seq.touched().len() >= 8, "batch must span many tiles");
        for c in seq.touched().iter() {
            let (a, b) = (seq.mask_tile(c).unwrap(), par.mask_tile(c).unwrap());
            assert!(
                a.pixels()
                    .iter()
                    .zip(b.pixels())
                    .all(|(x, y)| x.to_bits() == y.to_bits()),
                "tile {c:?}"
            );
        }
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

    /// A stroke over a tile of varied premultiplied pixels (alpha ramp,
    /// colour below alpha, values between 8-bit levels where the format
    /// allows).
    fn varied_tile<P: StoragePixel>() -> Tile<P> {
        let mut t: Tile<P> = Tile::new();
        for (i, p) in t.pixels_mut().iter_mut().enumerate() {
            let a = (i % 997) as f32 / 996.0;
            *p = P::from_premul(tela_color::Rgba::new(0.9, 0.45, 0.2, a).premultiply());
        }
        t
    }

    fn varied_stroke(mode: PaintMode) -> StrokeBuffer {
        let ctx = StampContext {
            anti_aliasing: true,
            mode,
            ..Default::default()
        };
        let mut s = StrokeBuffer::new(Color::new(0.2, 0.6, 0.9), ctx);
        for i in 0..14 {
            let i = i as f32;
            s.dab(&dab(12.0 + i * 4.3, 20.0 + i * 2.1, 9.0, 0.35, 0.3));
        }
        s
    }

    /// P2 at both depths: the commit is the preview quantized once to the
    /// tile's storage (within half a level of that storage), for paint,
    /// erase and alpha-preserving paint.
    #[test]
    fn commit_is_the_preview_quantized_once_at_both_depths() {
        fn check<P: StoragePixel>(level: f32) {
            let c = TileCoord { x: 0, y: 0 };
            for (mode, alpha) in [
                (PaintMode::Paint, PixelAlpha::Free),
                (PaintMode::Paint, PixelAlpha::Preserved),
                (PaintMode::Erase, PixelAlpha::Free),
            ] {
                let s = varied_stroke(mode);
                let mut tile = varied_tile::<P>();
                let mut preview: Vec<PremulRgba> =
                    tile.pixels().iter().map(|p| p.to_premul()).collect();
                s.paint_over_tile(c, &mut preview, alpha);
                s.commit_tile(c, &mut tile, alpha);
                let worst = tile
                    .pixels()
                    .iter()
                    .zip(&preview)
                    .map(|(p, q)| {
                        let p = p.to_premul();
                        [p.r - q.r, p.g - q.g, p.b - q.b, p.a - q.a]
                            .into_iter()
                            .fold(0.0f32, |m, d| m.max(d.abs()))
                    })
                    .fold(0.0f32, f32::max);
                assert!(
                    worst <= level * 0.5 + 1e-6,
                    "{:?} {mode:?} {alpha:?}: {worst}",
                    P::DEPTH
                );
            }
        }
        check::<[u8; 4]>(1.0 / 255.0);
        check::<[u16; 4]>(1.0 / 65535.0);
    }

    #[test]
    fn sixteen_bits_keep_a_stroke_that_8_bits_round_away() {
        let c = TileCoord { x: 0, y: 0 };
        let mut s = buffer();
        // Opacity 0.001 is a quarter of an 8-bit level.
        s.dab(&dab(10.5, 10.5, 3.0, 0.001, 1.0));
        let mut eight: Tile<[u8; 4]> = Tile::new();
        let mut sixteen: Tile<[u16; 4]> = Tile::new();
        s.commit_tile(c, &mut eight, PixelAlpha::Free);
        s.commit_tile(c, &mut sixteen, PixelAlpha::Free);
        assert_eq!(eight.get(10, 10), [0; 4], "below half a level: lost");
        assert_eq!(sixteen.get(10, 10)[3], 66, "0.001 · 65535 = 65.5");
    }

    #[test]
    fn widened_8_bit_content_commits_like_8_bit_within_one_level() {
        let c = TileCoord { x: 0, y: 0 };
        for mode in [PaintMode::Paint, PaintMode::Erase] {
            let s = varied_stroke(mode);
            let mut eight = varied_tile::<[u8; 4]>();
            let mut sixteen: Tile<[u16; 4]> = Tile::new();
            for (w, p) in sixteen.pixels_mut().iter_mut().zip(eight.pixels()) {
                *w = <[u16; 4]>::from_rgba8(*p);
            }
            s.commit_tile(c, &mut eight, PixelAlpha::Free);
            s.commit_tile(c, &mut sixteen, PixelAlpha::Free);
            // Same f32 result, quantized once (8-bit) or twice (16 → 8).
            for (a, b) in eight.pixels().iter().zip(sixteen.pixels()) {
                for (x, y) in a.iter().zip(b.to_rgba8()) {
                    assert!(x.abs_diff(y) <= 1, "{mode:?}: {a:?} vs {b:?}");
                }
            }
        }
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
