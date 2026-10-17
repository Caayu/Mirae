//! Drawing a gradient into a layer's pixels or mask (phase 5F, ADR 0027).
//! The preview ([`GradientPreview`]) and the commit
//! ([`Document::apply_gradient`]) run the same per-tile function on the
//! same stored values, so the committed layer is exactly what the
//! preview showed (invariant GR5).

use std::cell::RefCell;

use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, Tile, TileCoord, TileCoverage};
use tela_color::{PremulRgba, StoragePixel, dither_threshold, with_pixel_type};

use super::history::HistoryAction;
use super::{Document, LayerPart};
use crate::LayerError;
use crate::fill::FillTarget;
use crate::gradient::{self, Gradient, GradientPaint};

/// Pixels per tile.
const N: usize = (TILE_SIZE * TILE_SIZE) as usize;
const W: usize = TILE_SIZE as usize;

/// Everything one tile's paint depends on, fixed for the operation.
#[derive(Clone, Copy)]
struct Job<'a> {
    gradient: &'a Gradient,
    paint: GradientPaint,
    /// The pixels' alpha is locked (source-atop).
    locked: bool,
    canvas: IRect,
}

impl Job<'_> {
    fn threshold(&self, x: i32, y: i32) -> f32 {
        if self.gradient.dither() {
            dither_threshold(x, y)
        } else {
            0.5
        }
    }

    /// Calls `f(i, x, y, paint, selection)` for every pixel of tile `c`
    /// on the canvas whose paint and selection coverage are both above 0
    /// (the only pixels that can change, invariant GR6).
    fn each(
        &self,
        c: TileCoord,
        within: &TileCoverage,
        src: &mut [PremulRgba],
        mut f: impl FnMut(usize, i32, i32, PremulRgba, f32),
    ) {
        let Some([x0, y0, x1, y1]) = gradient::canvas_clip(self.canvas, c) else {
            return;
        };
        if matches!(within, TileCoverage::Empty) {
            return;
        }
        self.gradient.render_tile(c, src);
        let (ox, oy) = c.origin();
        let mut row = [255u8; W];
        for y in y0..y1 {
            if !matches!(within, TileCoverage::Full) {
                within.row(y, &mut row);
            }
            for x in x0..x1 {
                let i = y as usize * W + x as usize;
                let s = row[x as usize];
                if s == 0 || src[i].a <= 0.0 {
                    continue;
                }
                f(
                    i,
                    ox + x as i32,
                    oy + y as i32,
                    src[i],
                    f32::from(s) / 255.0,
                );
            }
        }
    }

    /// The commit on a stored tile of pixels.
    fn pixels_tile<P: StoragePixel>(
        &self,
        c: TileCoord,
        tile: &mut Tile<P>,
        within: &TileCoverage,
        src: &mut [PremulRgba],
    ) {
        let px = tile.pixels_mut();
        self.each(c, within, src, |i, x, y, s, sel| {
            if let Some(o) =
                gradient::blend_pixel(self.paint, self.locked, px[i].to_premul(), s, sel)
            {
                px[i] = gradient::quantize::<P>(o, self.threshold(x, y));
            }
        });
    }

    /// The preview on the decoded tile the compositor holds: the same
    /// rule, quantized the same way and decoded back.
    fn pixels_preview<P: StoragePixel>(
        &self,
        c: TileCoord,
        px: &mut [PremulRgba],
        within: &TileCoverage,
        src: &mut [PremulRgba],
    ) {
        self.each(c, within, src, |i, x, y, s, sel| {
            if let Some(o) = gradient::blend_pixel(self.paint, self.locked, px[i], s, sel) {
                px[i] = gradient::quantize::<P>(o, self.threshold(x, y)).to_premul();
            }
        });
    }

    /// A mask's reveal value `v` (`0..=255`) after the gradient, or `None`.
    fn mask_value(&self, v: u8, x: i32, y: i32, s: PremulRgba, sel: f32) -> Option<u8> {
        let r = gradient::blend_mask(self.paint, f32::from(v) / 255.0, s.a * sel)?;
        Some((r.clamp(0.0, 1.0) * 255.0 + self.threshold(x, y)) as u8)
    }
}

/// A gradient being previewed on its target (the compositor's overlay):
/// it borrows the document read-only, so it cannot change anything
/// (invariant GR2).
pub struct GradientPreview<'a> {
    doc: &'a Document,
    target: FillTarget,
    job: Job<'a>,
    domain: DirtyTiles,
    /// One tile of paint, reused by every call (composition allocates
    /// nothing per tile).
    src: RefCell<Vec<PremulRgba>>,
}

impl GradientPreview<'_> {
    pub fn target(&self) -> FillTarget {
        self.target
    }

    /// Whether the gradient may change tile `c` (the overlay's `covers`).
    pub fn covers(&self, c: TileCoord) -> bool {
        self.domain.contains(c)
    }

    /// Every tile the gradient may change.
    pub fn tiles(&self) -> &DirtyTiles {
        &self.domain
    }

    /// Draws tile `c` of the target's pixels as the commit will store them
    /// (for `OverlayPaint::Pixels`).
    pub fn pixels(&self, c: TileCoord, px: &mut [PremulRgba]) {
        if !self.covers(c) {
            return;
        }
        let mut src = self.src.borrow_mut();
        let within = self.doc.selection_tile(c);
        with_pixel_type!(self.doc.depth(), P => self.job.pixels_preview::<P>(c, px, &within, &mut src));
    }

    /// Draws tile `c` of the target's mask (reveal values in `0..=1`) as
    /// the commit will store them (for `OverlayPaint::Mask`).
    pub fn mask(&self, c: TileCoord, values: &mut [f32]) {
        if !self.covers(c) {
            return;
        }
        let mut src = self.src.borrow_mut();
        let within = self.doc.selection_tile(c);
        self.job.each(c, &within, &mut src, |i, x, y, s, sel| {
            let v = (values[i].clamp(0.0, 1.0) * 255.0).round() as u8;
            if let Some(r) = self.job.mask_value(v, x, y, s, sel) {
                values[i] = f32::from(r) / 255.0;
            }
        });
    }
}

impl Document {
    /// The tiles a gradient may change on `target`: the canvas, inside
    /// the selection's bounds, where the gradient paints; where the paint
    /// cannot create content (erase, alpha lock, revealing a mask) only
    /// tiles that hold something.
    fn gradient_domain(
        &self,
        target: FillTarget,
        gradient: &Gradient,
        paint: GradientPaint,
    ) -> DirtyTiles {
        let mut tiles = DirtyTiles::new();
        if gradient.is_empty() {
            return tiles;
        }
        let Some(layer) = self.layer(target.layer) else {
            return tiles;
        };
        let rect = match &self.selection {
            Some(s) => s.bounds().intersect(&self.bounds()),
            None => self.bounds(),
        };
        let locked = layer.alpha_locked();
        for c in rect.tiles() {
            if gradient.misses_tile(c) || matches!(self.selection_tile(c), TileCoverage::Empty) {
                continue;
            }
            let keep = match target.part {
                LayerPart::Pixels => {
                    !(locked || paint == GradientPaint::Erase) || layer.pixels().has_tile(c)
                }
                // An absent mask tile reveals everything: revealing more
                // cannot change it.
                LayerPart::Mask => {
                    paint == GradientPaint::Erase
                        || layer.mask().is_some_and(|m| m.hidden_tile(c).is_some())
                }
            };
            if keep {
                tiles.mark(c);
            }
        }
        tiles
    }

    /// A preview of `gradient` drawn into `target` with `paint`, for the
    /// compositor's overlay. Refused like the commit: hidden or locked
    /// layer, missing or disabled mask ([`Document::paint_target`]).
    pub fn gradient_preview<'a>(
        &'a self,
        target: FillTarget,
        gradient: &'a Gradient,
        paint: GradientPaint,
    ) -> Result<GradientPreview<'a>, LayerError> {
        self.paint_target(target.layer, target.part)?;
        let locked = self.layer(target.layer).is_some_and(|l| l.alpha_locked());
        Ok(GradientPreview {
            doc: self,
            target,
            job: Job {
                gradient,
                paint,
                locked,
                canvas: self.bounds(),
            },
            domain: self.gradient_domain(target, gradient, paint),
            src: RefCell::new(vec![PremulRgba::TRANSPARENT; N]),
        })
    }

    /// Draws `gradient` into `target` (a layer's pixels or its mask) as
    /// **one** history step ([`HistoryAction::Gradient`]): over the pixels
    /// (recolouring them keeping alpha when the layer's alpha is locked,
    /// GD30), erasing by the gradient's alpha with
    /// [`GradientPaint::Erase`], or on a mask revealing (hiding) by its
    /// alpha whatever its colours (GD33). The selection limits what
    /// changes and its coverage scales it (GD31, GD32); the gradient's
    /// geometry does not depend on it. Each changed pixel is quantized
    /// once at the document's depth (dithered if the gradient dithers);
    /// the layer's opacity, blend mode and clipping are not involved
    /// (GD34, GD35).
    ///
    /// Refused, changing nothing, on a hidden or locked layer or a missing
    /// or disabled mask (as strokes and fills). A degenerate gradient
    /// (GD24) or one that changes nothing records nothing. Returns the
    /// tiles that changed.
    pub fn apply_gradient(
        &mut self,
        target: FillTarget,
        gradient: &Gradient,
        paint: GradientPaint,
    ) -> Result<DirtyTiles, LayerError> {
        self.paint_target(target.layer, target.part)?;
        let tiles = self.gradient_domain(target, gradient, paint);
        if tiles.is_empty() {
            return Ok(DirtyTiles::new());
        }
        let i = self.require_unlocked(target.layer)?;
        let job = Job {
            gradient,
            paint,
            locked: self.layers[i].alpha_locked,
            canvas: self.bounds(),
        };
        let mut src = vec![PremulRgba::TRANSPARENT; N];
        let edit = match target.part {
            LayerPart::Pixels => with_pixel_type!(self.depth, P => {
                self.tile_edit::<P>(target.layer, &tiles, true, |c, tile, within| {
                    job.pixels_tile::<P>(c, tile, &within, &mut src);
                })?
            }),
            LayerPart::Mask => self.mask_edit(target.layer, &tiles, true, |c, tile, within| {
                let values = tile.pixels_mut();
                job.each(c, &within, &mut src, |i, x, y, s, sel| {
                    if let Some(v) = job.mask_value(values[i], x, y, s, sel) {
                        values[i] = v;
                    }
                });
            })?,
        };
        if edit.changed.is_empty() {
            return Ok(DirtyTiles::new());
        }
        self.record(
            HistoryAction::Gradient,
            vec![edit.change],
            self.active,
            edit.cost,
        );
        Ok(if edit.shown {
            edit.changed
        } else {
            DirtyTiles::new()
        })
    }
}
