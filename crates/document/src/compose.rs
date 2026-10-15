use tela_canvas::{TILE_SIZE, TileCoord};
use tela_color::PremulRgba;

use crate::{BlendMode, Document, LayerId, LayerPart, RasterLayer};

/// Content drawn into one layer while compositing, e.g. the preview of a
/// stroke that has not been committed yet.
pub struct Overlay<'a> {
    /// The layer the overlay is drawn on. Nothing is drawn if the layer
    /// does not exist or is hidden.
    pub target: LayerId,
    /// What the overlay draws into.
    pub paint: OverlayPaint<'a>,
    /// Tiles the overlay may draw into.
    pub covers: &'a dyn Fn(TileCoord) -> bool,
}

/// The overlay's drawing function, by the part of the layer it edits.
/// Both are applied exactly where the committed edit acts, so the preview
/// equals the result after commit.
#[derive(Clone, Copy)]
pub enum OverlayPaint<'a> {
    /// Draws into one tile of the layer's premultiplied pixels, before the
    /// layer's mask, opacity, clipping and blend mode.
    Pixels(&'a dyn Fn(TileCoord, &mut [PremulRgba])),
    /// Draws into one tile of the layer's mask, as reveal values in
    /// `0..=1` (1 shows the layer). Shows only while the mask is enabled,
    /// as a commit would.
    Mask(&'a dyn Fn(TileCoord, &mut [f32])),
}

impl Overlay<'_> {
    fn part(&self) -> LayerPart {
        match self.paint {
            OverlayPaint::Pixels(_) => LayerPart::Pixels,
            OverlayPaint::Mask(_) => LayerPart::Mask,
        }
    }

    /// Whether this overlay draws pixels of `l`.
    fn on_pixels(&self, l: &RasterLayer) -> bool {
        self.target == l.id() && self.part() == LayerPart::Pixels
    }
}

/// Index one past the clipping group that starts at `i`: an unclipped
/// layer and the clipped layers directly above it. A clipped layer with no
/// unclipped layer anywhere below it (`based` false) has no base and is a
/// group of its own, drawn like an ordinary layer (reference K10).
fn group_end(layers: &[RasterLayer], i: usize, based: bool) -> usize {
    if layers[i].clipped() && !based {
        return i + 1;
    }
    i + 1 + layers[i + 1..].iter().take_while(|l| l.clipped()).count()
}

impl Document {
    /// Composites every visible layer (and the overlay) for one tile.
    /// Returns `None` when nothing is there, so callers can skip or free
    /// the tile. Pixels outside the document are cleared.
    ///
    /// Each layer's mask (if enabled) scales its alpha first. A clipping
    /// group — an unclipped base and the clipped layers directly above it
    /// — is composed apart: the base, then each visible clipped layer
    /// source-atop with its own mask, opacity and blend mode
    /// ([`BlendMode::clip_atop`]); the group then meets the stack through
    /// the base's opacity and blend mode. A hidden base hides its group.
    /// This reproduces the reference application's measured results
    /// (ADR 0017).
    ///
    /// The backdrop is transparency when every layer blending onto the
    /// stack in this tile is `Normal` (the display puts the paper
    /// underneath; this path is bit-identical to the compositor before
    /// blend modes). When any of them is non-Normal, the paper colour is
    /// the opaque bottom backdrop, as in the reference application, where
    /// the paper is a layer that blend modes see (Screen over empty paper
    /// is white; parity record 4C) — the tile then comes out opaque.
    /// Clipped layers blend inside their group and never need the paper.
    ///
    /// Allocates; interactive callers should use [`Document::compose_tile_into`].
    pub fn compose_tile(
        &self,
        coord: TileCoord,
        overlay: Option<&Overlay>,
    ) -> Option<Vec<[u8; 4]>> {
        let mut scratch = ComposeScratch::new();
        self.compose_tile_into(coord, overlay, &mut scratch)
            .then(|| scratch.out.clone())
    }

    /// Like [`Document::compose_tile`], but writes into reusable buffers
    /// (`scratch.out`) and returns whether the tile has content.
    pub fn compose_tile_into(
        &self,
        coord: TileCoord,
        overlay: Option<&Overlay>,
        scratch: &mut ComposeScratch,
    ) -> bool {
        let in_doc = coord.rect().intersect(&self.bounds());
        if in_doc.is_empty() {
            return false;
        }
        let overlay = overlay
            .filter(|o| (o.covers)(coord) && self.layer(o.target).is_some_and(|l| l.visible()));
        let draws = |l: &RasterLayer| {
            l.visible() && (l.pixels().has_tile(coord) || overlay.is_some_and(|o| o.on_pixels(l)))
        };
        let layers = self.layer_slice();
        if !layers.iter().any(draws) {
            return false;
        }

        // Only group bases (and base-less clipped layers) blend with the
        // stack; their modes decide whether the paper is the backdrop.
        let mut blends_here = false;
        let (mut i, mut based) = (0, false);
        while i < layers.len() {
            let end = group_end(layers, i, based);
            based |= !layers[i].clipped();
            let base = &layers[i];
            blends_here |= base.blend_mode() != BlendMode::Normal && draws(base);
            i = end;
        }
        let backdrop = if blends_here {
            self.background().premultiply()
        } else {
            PremulRgba::TRANSPARENT
        };

        let ComposeScratch {
            acc,
            layer,
            group,
            mask,
            out,
        } = scratch;
        acc.fill(backdrop);
        let mut drew = false;
        let (mut i, mut based) = (0, false);
        while i < layers.len() {
            let end = group_end(layers, i, based);
            based |= !layers[i].clipped();
            let (base, clipped) = (&layers[i], &layers[i + 1..end]);
            i = end;
            if !draws(base) {
                // Hidden, or no pixels here: nothing of the group shows.
                continue;
            }
            drew = true;
            load_layer(base, coord, overlay, layer, mask);
            if !clipped.iter().any(draws) {
                base.blend_mode().composite(layer, base.opacity(), acc);
                continue;
            }
            std::mem::swap(layer, group);
            for c in clipped.iter().filter(|c| draws(c)) {
                load_layer(c, coord, overlay, layer, mask);
                c.blend_mode().clip_atop(layer, c.opacity(), group);
            }
            base.blend_mode().composite(group, base.opacity(), acc);
        }
        if !drew {
            return false;
        }

        for (o, a) in out.iter_mut().zip(acc.iter()) {
            *o = a.to_rgba8();
        }
        if in_doc != coord.rect() {
            let (ox, oy) = coord.origin();
            for y in 0..TILE_SIZE as i32 {
                for x in 0..TILE_SIZE as i32 {
                    let (px, py) = (ox + x, oy + y);
                    if px < in_doc.x0 || px >= in_doc.x1 || py < in_doc.y0 || py >= in_doc.y1 {
                        out[(y * TILE_SIZE as i32 + x) as usize] = [0; 4];
                    }
                }
            }
        }
        true
    }
}

/// One layer's tile as it enters blending: its pixels, the overlay drawn
/// on them, then its mask (with the overlay drawn on the mask) if enabled.
fn load_layer(
    l: &RasterLayer,
    coord: TileCoord,
    overlay: Option<&Overlay>,
    layer: &mut [PremulRgba],
    reveal: &mut [f32],
) {
    // Decoded at the document's depth; composition itself is f32 at
    // every depth, so storage precision is all that differs.
    l.pixels().decode_tile(coord, layer);
    let mine = overlay.filter(|o| o.target == l.id());
    if let Some(OverlayPaint::Pixels(paint)) = mine.map(|o| o.paint) {
        paint(coord, layer);
    }
    let Some(m) = l.mask().filter(|m| m.enabled()) else {
        return;
    };
    let hidden = m.hidden_tile(coord);
    if let Some(OverlayPaint::Mask(paint)) = mine.map(|o| o.paint) {
        match hidden {
            Some(t) => {
                for (r, h) in reveal.iter_mut().zip(t.pixels()) {
                    *r = f32::from(255 - h) / 255.0;
                }
            }
            None => reveal.fill(1.0),
        }
        paint(coord, reveal);
        for (p, r) in layer.iter_mut().zip(reveal.iter()) {
            if *r < 1.0 {
                *p = p.scale(*r);
            }
        }
    } else if let Some(t) = hidden {
        for (p, h) in layer.iter_mut().zip(t.pixels()) {
            if *h != 0 {
                *p = p.scale(f32::from(255 - h) / 255.0);
            }
        }
    }
}

/// Reusable buffers for [`Document::compose_tile_into`] (about 3.6 MB).
pub struct ComposeScratch {
    acc: Vec<PremulRgba>,
    layer: Vec<PremulRgba>,
    /// A clipping group being composed.
    group: Vec<PremulRgba>,
    /// Mask reveal values while a mask stroke is previewed.
    mask: Vec<f32>,
    /// The composed tile, premultiplied RGBA8 for display at every storage
    /// depth (storage precision is not display precision, ADR 0018).
    pub out: Vec<[u8; 4]>,
}

impl Default for ComposeScratch {
    fn default() -> Self {
        Self::new()
    }
}

impl ComposeScratch {
    pub fn new() -> Self {
        let n = (TILE_SIZE * TILE_SIZE) as usize;
        Self {
            acc: vec![PremulRgba::TRANSPARENT; n],
            layer: vec![PremulRgba::TRANSPARENT; n],
            group: vec![PremulRgba::TRANSPARENT; n],
            mask: vec![1.0; n],
            out: vec![[0; 4]; n],
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use tela_brush::dynamics::BrushProperty;
    use tela_brush::{BrushInput, BrushPreset, PixelAlpha, StrokeBuffer};
    use tela_canvas::{DirtyTiles, Tile};
    use tela_color::{StoragePixel, with_pixel_type};

    use super::*;
    use crate::Placement;

    fn stroke_at(x: f32, y: f32, opacity: f32) -> StrokeBuffer {
        let mut settings = BrushPreset::plain(12.0);
        settings.properties.opacity = BrushProperty::constant(opacity);
        let mut e = tela_brush::BrushEngine::new(settings);
        e.begin_stroke(BrushInput::from_position(Vec2::new(x, y), 0.0), 1.0, 0);
        e.end_stroke().unwrap()
    }

    fn set_pixel(d: &mut Document, id: LayerId, x: i32, y: i32, p: [u8; 4]) {
        let coord = TileCoord::containing(x, y);
        let mut tiles = DirtyTiles::new();
        tiles.mark(coord);
        let (ox, oy) = coord.origin();
        with_pixel_type!(d.depth(), P => d.modify_tiles(id, &tiles, |_, t: &mut Tile<P>| {
            *t.get_mut((x - ox) as u32, (y - oy) as u32) = P::from_rgba8(p);
        }))
        .unwrap();
    }

    fn commit(d: &mut Document, id: LayerId, stroke: &StrokeBuffer) {
        with_pixel_type!(d.depth(), P => d.modify_tiles(id, stroke.touched(), |c, t: &mut Tile<P>| {
            stroke.commit_tile(c, t, PixelAlpha::Free)
        }))
        .unwrap();
    }

    fn max_diff(a: &[[u8; 4]], b: &[[u8; 4]]) -> u8 {
        a.iter()
            .zip(b)
            .flat_map(|(a, b)| a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)))
            .max()
            .unwrap()
    }

    #[test]
    fn empty_tile_composes_to_none() {
        let d = Document::new(512, 512);
        assert!(d.compose_tile(TileCoord { x: 0, y: 0 }, None).is_none());
    }

    #[test]
    fn tile_outside_document_is_none() {
        let mut d = Document::new(100, 100);
        let l = d.active_layer();
        set_pixel(&mut d, l, 300, 300, [255; 4]);
        assert!(d.compose_tile(TileCoord { x: 1, y: 1 }, None).is_none());
    }

    #[test]
    fn pixels_beyond_document_edge_are_cleared() {
        let mut d = Document::new(100, 100);
        let l = d.active_layer();
        set_pixel(&mut d, l, 50, 50, [255; 4]);
        set_pixel(&mut d, l, 150, 50, [255; 4]);
        let t = d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap();
        assert_eq!(t[50 * 256 + 50], [255; 4]);
        assert_eq!(t[50 * 256 + 150], [0; 4]);
    }

    #[test]
    fn hidden_layer_is_skipped_and_opacity_applies() {
        let mut d = Document::new(256, 256);
        let l = d.active_layer();
        set_pixel(&mut d, l, 0, 0, [255, 0, 0, 255]);
        d.set_opacity(l, 0.5).unwrap();
        let t = d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap();
        assert_eq!(t[0], [128, 0, 0, 128]);
        d.set_visible(l, false).unwrap();
        assert!(d.compose_tile(TileCoord { x: 0, y: 0 }, None).is_none());
    }

    #[test]
    fn layer_order_decides_what_is_on_top() {
        let mut d = Document::new(256, 256);
        let red = d.active_layer();
        set_pixel(&mut d, red, 0, 0, [255, 0, 0, 255]);
        let blue = d.add_layer(Placement::Top).unwrap();
        set_pixel(&mut d, blue, 0, 0, [0, 0, 255, 255]);
        let top = |d: &Document| d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap()[0];
        assert_eq!(top(&d), [0, 0, 255, 255]);
        d.move_layer(red, Placement::Top).unwrap();
        assert_eq!(top(&d), [255, 0, 0, 255]);
    }

    #[test]
    fn composition_does_not_depend_on_id_values() {
        // Same stack built through different id histories.
        let build = |extra: usize| {
            let mut d = Document::new(256, 256);
            for _ in 0..extra {
                let tmp = d.add_layer(Placement::Top).unwrap();
                d.remove_layer(tmp).unwrap();
            }
            let a = d.active_layer();
            set_pixel(&mut d, a, 3, 3, [10, 20, 30, 200]);
            let b = d.add_layer(Placement::Top).unwrap();
            set_pixel(&mut d, b, 3, 3, [90, 0, 0, 120]);
            d.set_opacity(b, 0.4).unwrap();
            d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap()
        };
        assert_eq!(build(0), build(5));
    }

    #[test]
    fn preview_matches_committed_result() {
        let mut d = Document::new(256, 256);
        let l = d.active_layer();
        d.set_opacity(l, 0.6).unwrap();
        let stroke = stroke_at(40.0, 40.0, 0.7);
        let coord = TileCoord { x: 0, y: 0 };
        let paint =
            |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px, PixelAlpha::Free);
        let covers = |c: TileCoord| stroke.touched().contains(c);
        let o = Overlay {
            target: l,
            paint: OverlayPaint::Pixels(&paint),
            covers: &covers,
        };
        let preview = d.compose_tile(coord, Some(&o)).unwrap();
        commit(&mut d, l, &stroke);
        let committed = d.compose_tile(coord, None).unwrap();
        // Committing quantizes the layer to 8 bits once more.
        let diff = max_diff(&preview, &committed);
        assert!(diff <= 1, "preview differs from commit by {diff}");
    }

    #[test]
    fn preview_targets_its_layer_by_id_not_by_position() {
        let mut d = Document::new(256, 256);
        let target = d.active_layer();
        let above = d.add_layer(Placement::Top).unwrap();
        set_pixel(&mut d, above, 40, 40, [0, 0, 200, 255]);
        let stroke = stroke_at(40.0, 40.0, 1.0);
        let paint =
            |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px, PixelAlpha::Free);
        let covers = |c: TileCoord| stroke.touched().contains(c);
        let o = Overlay {
            target,
            paint: OverlayPaint::Pixels(&paint),
            covers: &covers,
        };
        let coord = TileCoord { x: 0, y: 0 };
        // The opaque blue layer above hides the stroke under it.
        assert_eq!(
            d.compose_tile(coord, Some(&o)).unwrap()[40 * 256 + 40],
            [0, 0, 200, 255]
        );
        // Reordering moves the preview with its layer.
        d.move_layer(target, Placement::Top).unwrap();
        assert_eq!(
            d.compose_tile(coord, Some(&o)).unwrap()[40 * 256 + 40],
            [0, 0, 0, 255]
        );
        // A hidden target shows no preview, as a commit would show nothing.
        d.set_visible(target, false).unwrap();
        assert_eq!(
            d.compose_tile(coord, Some(&o)).unwrap()[40 * 256 + 40],
            [0, 0, 200, 255]
        );
    }

    #[test]
    fn eraser_preview_matches_committed_result() {
        let mut d = Document::new(256, 256);
        let l = d.active_layer();
        let mut square = DirtyTiles::new();
        square.mark(TileCoord { x: 0, y: 0 });
        d.modify_tiles(l, &square, |_, t: &mut Tile<[u8; 4]>| {
            for y in 20..60 {
                for x in 20..60 {
                    *t.get_mut(x, y) = [180, 60, 30, 255];
                }
            }
        })
        .unwrap();
        let mut preset = BrushPreset::plain(20.0);
        preset.compositing.mode = tela_brush::preset::PaintMode::Erase;
        preset.properties.opacity = BrushProperty::constant(0.7);
        let mut e = tela_brush::BrushEngine::new(preset);
        e.begin_stroke(
            BrushInput::from_position(Vec2::new(40.0, 40.0), 0.0),
            1.0,
            0,
        );
        let stroke = e.end_stroke().unwrap();
        let coord = TileCoord { x: 0, y: 0 };
        let paint =
            |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px, PixelAlpha::Free);
        let covers = |c: TileCoord| stroke.touched().contains(c);
        let o = Overlay {
            target: l,
            paint: OverlayPaint::Pixels(&paint),
            covers: &covers,
        };
        let preview = d.compose_tile(coord, Some(&o)).unwrap();
        commit(&mut d, l, &stroke);
        let committed = d.compose_tile(coord, None).unwrap();
        assert_eq!(committed[40 * 256 + 40], [54, 18, 9, 77], "70% erased");
        let diff = max_diff(&preview, &committed);
        assert!(diff <= 1, "preview differs from commit by {diff}");
    }

    /// A stack with a coloured base and a target layer in `mode`.
    fn blend_stack(mode: BlendMode) -> (Document, LayerId) {
        let mut d = Document::new(256, 256);
        let base = d.active_layer();
        let mut tile = DirtyTiles::new();
        tile.mark(TileCoord { x: 0, y: 0 });
        d.modify_tiles(base, &tile, |_, t: &mut Tile<[u8; 4]>| {
            for y in 0..128 {
                for x in 0..256 {
                    *t.get_mut(x, y) = [180, 90, 40, 255];
                }
            }
            for y in 128..192 {
                for x in 0..256 {
                    *t.get_mut(x, y) = [20, 60, 50, 128];
                }
            }
        })
        .unwrap();
        let top = d.add_layer(Placement::Top).unwrap();
        d.set_opacity(top, 0.7).unwrap();
        d.set_blend_mode(top, mode).unwrap();
        (d, top)
    }

    #[test]
    fn preview_matches_commit_in_every_mode() {
        for mode in BlendMode::ALL {
            let (mut d, top) = blend_stack(mode);
            // Crosses the opaque, translucent and empty parts of the base.
            let mut preset = BrushPreset::plain(30.0);
            preset.properties.opacity = BrushProperty::constant(0.8);
            let mut e = tela_brush::BrushEngine::new(preset);
            e.begin_stroke(
                BrushInput::from_position(Vec2::new(60.0, 40.0), 0.0),
                1.0,
                0,
            );
            e.add_input_at(BrushInput::from_position(Vec2::new(60.0, 240.0), 1.0), 1.0);
            let stroke = e.end_stroke().unwrap();
            let coord = TileCoord { x: 0, y: 0 };
            let paint = |c: TileCoord, px: &mut [PremulRgba]| {
                stroke.paint_over_tile(c, px, PixelAlpha::Free)
            };
            let covers = |c: TileCoord| stroke.touched().contains(c);
            let o = Overlay {
                target: top,
                paint: OverlayPaint::Pixels(&paint),
                covers: &covers,
            };
            let preview = d.compose_tile(coord, Some(&o)).unwrap();
            commit(&mut d, top, &stroke);
            let committed = d.compose_tile(coord, None).unwrap();
            let diff = max_diff(&preview, &committed);
            assert!(diff <= 1, "{mode:?}: preview differs from commit by {diff}");
        }
    }

    #[test]
    fn normal_only_tiles_keep_a_transparent_backdrop() {
        let (mut d, top) = blend_stack(BlendMode::Normal);
        set_pixel(&mut d, top, 10, 250, [40, 40, 40, 255]);
        let t = d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap();
        assert_eq!(t[255 * 256 + 255], [0; 4], "empty pixels stay transparent");
        assert_eq!(
            t[150 * 256 + 5],
            [20, 60, 50, 128],
            "translucent base untouched"
        );
    }

    #[test]
    fn blend_layers_see_the_paper() {
        // Screen over empty paper shows the paper (the reference's paper is
        // a layer); Multiply over white shows the layer colour.
        let (mut d, top) = blend_stack(BlendMode::Screen);
        set_pixel(&mut d, top, 10, 250, [40, 40, 40, 255]);
        let t = d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap();
        assert_eq!(t[250 * 256 + 10], [255; 4]);
        assert_eq!(t[255 * 256 + 255], [255; 4], "the whole tile is over paper");
        d.set_blend_mode(top, BlendMode::Multiply).unwrap();
        d.set_opacity(top, 1.0).unwrap();
        let t = d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap();
        assert_eq!(t[250 * 256 + 10], [40, 40, 40, 255]);
        // A blend layer with nothing in this tile leaves it Normal-only.
        d.clear_layer(top).unwrap();
        let t = d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap();
        assert_eq!(t[255 * 256 + 255], [0; 4]);
    }

    #[test]
    fn blend_mode_changes_are_undoable() {
        let (mut d, top) = blend_stack(BlendMode::Normal);
        set_pixel(&mut d, top, 10, 10, [40, 200, 40, 255]);
        let coord = TileCoord { x: 0, y: 0 };
        let normal = d.compose_tile(coord, None).unwrap();
        d.set_blend_mode(top, BlendMode::Difference).unwrap();
        let difference = d.compose_tile(coord, None).unwrap();
        assert_ne!(normal, difference);
        assert_eq!(d.next_undo(), Some(crate::HistoryAction::BlendMode));
        let dirty = d.undo().unwrap();
        assert!(dirty.contains(coord));
        assert_eq!(d.layer(top).unwrap().blend_mode(), BlendMode::Normal);
        assert_eq!(d.compose_tile(coord, None).unwrap(), normal);
        d.redo().unwrap();
        assert_eq!(d.compose_tile(coord, None).unwrap(), difference);
    }

    /// Fills the whole tile (0, 0) of a layer with `f(x, y)` (8-bit values,
    /// stored exactly at either depth).
    fn fill(d: &mut Document, id: LayerId, f: impl Fn(u32, u32) -> [u8; 4]) {
        let mut tile = DirtyTiles::new();
        tile.mark(TileCoord { x: 0, y: 0 });
        with_pixel_type!(d.depth(), P => d.modify_tiles(id, &tile, |_, t: &mut Tile<P>| {
            for y in 0..256 {
                for x in 0..256 {
                    *t.get_mut(x, y) = P::from_rgba8(f(x, y));
                }
            }
        }))
        .unwrap();
    }

    fn fill_mask(d: &mut Document, id: LayerId, f: impl Fn(u32, u32) -> u8) {
        let mut tile = DirtyTiles::new();
        tile.mark(TileCoord { x: 0, y: 0 });
        d.modify_mask(id, &tile, |_, t| {
            for y in 0..256 {
                for x in 0..256 {
                    *t.get_mut(x, y) = f(x, y);
                }
            }
        })
        .unwrap();
    }

    /// A base with an alpha ramp (left to right) and varied colour, and a
    /// second layer above it with its own colour and alpha bands.
    fn two_layers() -> (Document, LayerId, LayerId) {
        two_layers_at(tela_color::PixelDepth::Rgba8)
    }

    /// [`two_layers`] in a document of `depth` (same 8-bit content).
    fn two_layers_at(depth: tela_color::PixelDepth) -> (Document, LayerId, LayerId) {
        let mut d = Document::with_depth(256, 256, depth);
        let base = d.active_layer();
        fill(&mut d, base, |x, y| {
            let a = x as u8;
            let c = |v: u32| ((v * u32::from(a)) / 255) as u8;
            [c(200), c(60 + y / 2), c(40), a]
        });
        let top = d.add_layer(Placement::Top).unwrap();
        fill(&mut d, top, |x, y| {
            let a = [255u8, 180, 90, 0][(y / 64) as usize];
            let k = |v: u32| ((v * u32::from(a)) / 255) as u8;
            [k(30 + x / 2), k(120), k(220), a]
        });
        (d, base, top)
    }

    /// A long stroke crossing the alpha ramp and every band.
    fn crossing_stroke(mode: tela_brush::preset::PaintMode, opacity: f32) -> StrokeBuffer {
        let mut preset = BrushPreset::plain(26.0);
        preset.compositing.mode = mode;
        preset.properties.opacity = BrushProperty::constant(opacity);
        let mut e = tela_brush::BrushEngine::new(preset);
        e.begin_stroke(
            BrushInput::from_position(Vec2::new(10.0, 10.0), 0.0),
            1.0,
            0,
        );
        e.add_input_at(BrushInput::from_position(Vec2::new(240.0, 120.0), 0.5), 0.5);
        e.add_input_at(BrushInput::from_position(Vec2::new(30.0, 245.0), 1.0), 1.0);
        e.end_stroke().unwrap()
    }

    #[test]
    fn a_new_mask_changes_nothing_and_allocates_nothing() {
        let (mut d, _, top) = two_layers();
        d.set_blend_mode(top, BlendMode::Multiply).unwrap();
        let coord = TileCoord { x: 0, y: 0 };
        let before = d.compose_tile(coord, None).unwrap();
        d.add_mask(top).unwrap();
        assert_eq!(d.layer(top).unwrap().mask().unwrap().tile_count(), 0, "M3");
        assert_eq!(d.compose_tile(coord, None).unwrap(), before, "M1");
    }

    #[test]
    fn a_disabled_mask_is_the_same_as_no_mask() {
        let (mut d, base, top) = two_layers();
        d.set_clipped(top, true).unwrap();
        let coord = TileCoord { x: 0, y: 0 };
        let without = d.compose_tile(coord, None).unwrap();
        for id in [base, top] {
            d.add_mask(id).unwrap();
            fill_mask(&mut d, id, |x, y| ((x * 7 + y * 3) % 256) as u8);
        }
        assert_ne!(d.compose_tile(coord, None).unwrap(), without);
        for id in [base, top] {
            d.set_mask_enabled(id, false).unwrap();
        }
        assert_eq!(d.compose_tile(coord, None).unwrap(), without, "M2");
    }

    #[test]
    fn mask_values_scale_layer_alpha_before_opacity() {
        let mut d = Document::new(256, 256);
        let l = d.active_layer();
        set_pixel(&mut d, l, 1, 1, [200, 100, 50, 255]);
        d.add_mask(l).unwrap();
        fill_mask(&mut d, l, |_, _| 128);
        d.set_opacity(l, 0.5).unwrap();
        let t = d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap();
        assert_eq!(t[256 + 1], [50, 25, 13, 64]);
    }

    #[test]
    fn clipped_layers_show_only_inside_the_base_alpha() {
        let (mut d, base, top) = two_layers();
        let coord = TileCoord { x: 0, y: 0 };
        let base_only = {
            let mut c = d.clone();
            c.set_visible(top, false).unwrap();
            c.compose_tile(coord, None).unwrap()
        };
        d.set_clipped(top, true).unwrap();
        for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
            d.set_blend_mode(top, mode).unwrap();
            let t = d.compose_tile(coord, None).unwrap();
            for (got, b) in t.iter().zip(&base_only) {
                assert!(got[3].abs_diff(b[3]) <= 1, "CL1 {mode:?}: {got:?} vs {b:?}");
            }
        }
        // A hidden base hides its clipped layers.
        d.set_visible(base, false).unwrap();
        assert!(d.compose_tile(coord, None).is_none());
    }

    #[test]
    fn unclipping_restores_the_ordinary_composition() {
        let (mut d, _, top) = two_layers();
        d.set_blend_mode(top, BlendMode::Screen).unwrap();
        let coord = TileCoord { x: 0, y: 0 };
        let ordinary = d.compose_tile(coord, None).unwrap();
        d.set_clipped(top, true).unwrap();
        assert_ne!(d.compose_tile(coord, None).unwrap(), ordinary);
        d.set_clipped(top, false).unwrap();
        assert_eq!(d.compose_tile(coord, None).unwrap(), ordinary, "CL3");
    }

    #[test]
    fn a_clipped_layer_without_a_base_is_ordinary() {
        let (mut d, base, top) = two_layers();
        d.move_layer(base, Placement::Top).unwrap();
        let coord = TileCoord { x: 0, y: 0 };
        let ordinary = d.compose_tile(coord, None).unwrap();
        d.set_clipped(top, true).unwrap();
        assert_eq!(d.compose_tile(coord, None).unwrap(), ordinary, "K10");
    }

    /// Preview (overlay) and commit of one stroke, as composed tiles.
    fn preview_and_commit(
        mut d: Document,
        target: LayerId,
        part: LayerPart,
        stroke: &StrokeBuffer,
        alpha: PixelAlpha,
    ) -> (Vec<[u8; 4]>, Vec<[u8; 4]>) {
        let coord = TileCoord { x: 0, y: 0 };
        let pixels = |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px, alpha);
        let mask = |c: TileCoord, v: &mut [f32]| stroke.paint_alpha_tile(c, v);
        let covers = |c: TileCoord| stroke.touched().contains(c);
        let o = Overlay {
            target,
            paint: match part {
                LayerPart::Pixels => OverlayPaint::Pixels(&pixels),
                LayerPart::Mask => OverlayPaint::Mask(&mask),
            },
            covers: &covers,
        };
        let preview = d.compose_tile(coord, Some(&o)).unwrap();
        match part {
            LayerPart::Pixels => {
                with_pixel_type!(d.depth(), P => d.modify_tiles(target, stroke.touched(), |c, t: &mut Tile<P>| {
                    stroke.commit_tile(c, t, alpha)
                }))
                .unwrap();
            }
            LayerPart::Mask => {
                d.modify_mask(target, stroke.touched(), |c, t| {
                    stroke.commit_alpha_tile(c, t)
                })
                .unwrap();
            }
        }
        (preview, d.compose_tile(coord, None).unwrap())
    }

    #[derive(Clone, Copy, Debug)]
    enum MaskState {
        Absent,
        Enabled,
        Disabled,
    }

    /// The preview == commit matrix of phase 4D: target (pixels, mask) ×
    /// paint/erase × alpha lock × Normal/Multiply/Screen × mask absent,
    /// enabled, disabled × unclipped/clipped (stroke on the clipped layer
    /// or on its base). Combinations that cannot happen are skipped: strokes
    /// reach only enabled masks (`paint_target`) and alpha lock concerns
    /// pixels only.
    #[test]
    fn preview_matches_commit_across_masks_clipping_lock_and_modes() {
        let (cases, worst, differing) = preview_commit_matrix(tela_color::PixelDepth::Rgba8);
        assert_eq!(cases, 2 * (2 * 3 * 3 * 3 + 3 * 3));
        assert!(worst <= 1, "{worst}");
        eprintln!(
            "8-bit preview == commit: {cases} cases, worst {worst}/255, {differing} pixel channels differ"
        );
    }

    /// The same matrix in 16-bit documents (phase 4E): the preview is
    /// composed from the f32 stroke, the commit is stored at 16 bits; both
    /// are shown at 8 bits, so they may differ by one display level where
    /// the stored value falls on the other side of a rounding boundary.
    #[test]
    fn preview_matches_commit_in_16_bit_documents() {
        let (cases, worst, differing) = preview_commit_matrix(tela_color::PixelDepth::Rgba16);
        assert_eq!(cases, 2 * (2 * 3 * 3 * 3 + 3 * 3));
        assert!(worst <= 1, "{worst}");
        eprintln!(
            "16-bit preview == commit: {cases} cases, worst {worst}/255, {differing} pixel channels differ"
        );
    }

    /// Runs the matrix at `depth`; returns the cases, the largest
    /// difference (1/255) and how many pixel channels differ in total.
    fn preview_commit_matrix(depth: tela_color::PixelDepth) -> (usize, u8, usize) {
        use tela_brush::preset::PaintMode;
        let (mut cases, mut worst, mut differing) = (0, 0, 0);
        for op in [PaintMode::Paint, PaintMode::Erase] {
            let stroke = crossing_stroke(op, 0.8);
            for part in [LayerPart::Pixels, LayerPart::Mask] {
                for lock in [PixelAlpha::Free, PixelAlpha::Preserved] {
                    if part == LayerPart::Mask && lock == PixelAlpha::Preserved {
                        continue;
                    }
                    for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
                        for mask in [MaskState::Absent, MaskState::Enabled, MaskState::Disabled] {
                            if part == LayerPart::Mask && !matches!(mask, MaskState::Enabled) {
                                continue;
                            }
                            for (clipped, on_base) in [(false, false), (true, false), (true, true)]
                            {
                                let (mut d, base, top) = two_layers_at(depth);
                                d.set_blend_mode(top, mode).unwrap();
                                d.set_opacity(top, 0.7).unwrap();
                                d.set_clipped(top, clipped).unwrap();
                                let target = if on_base { base } else { top };
                                if !matches!(mask, MaskState::Absent) {
                                    d.add_mask(target).unwrap();
                                    fill_mask(&mut d, target, |x, y| {
                                        [255u8, 200, 90, 0][((x + y) / 128) as usize]
                                    });
                                    if matches!(mask, MaskState::Disabled) {
                                        d.set_mask_enabled(target, false).unwrap();
                                    }
                                }
                                let (preview, committed) =
                                    preview_and_commit(d, target, part, &stroke, lock);
                                let diff = max_diff(&preview, &committed);
                                assert!(
                                    diff <= 1,
                                    "{depth:?} {op:?} {part:?} {lock:?} {mode:?} {mask:?} clipped {clipped} base {on_base}: {diff}"
                                );
                                worst = worst.max(diff);
                                differing += preview
                                    .iter()
                                    .zip(&committed)
                                    .flat_map(|(a, b)| a.iter().zip(b))
                                    .filter(|(a, b)| a != b)
                                    .count();
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
        (cases, worst, differing)
    }

    /// 8-bit content widened to 16 bits composes to exactly the same
    /// display pixels in every mode, with masks (enabled and disabled),
    /// clipping, opacity and the paper backdrop (ADR 0018: depth changes
    /// precision, never appearance).
    #[test]
    fn widened_8_bit_content_composes_identically_in_every_mode() {
        let coord = TileCoord { x: 0, y: 0 };
        for mode in BlendMode::ALL {
            for (clipped, masked, enabled) in [
                (false, false, true),
                (true, false, true),
                (false, true, true),
                (true, true, false),
            ] {
                let (mut d, base, top) = two_layers();
                d.set_blend_mode(top, mode).unwrap();
                d.set_blend_mode(base, mode).unwrap();
                d.set_opacity(top, 0.65).unwrap();
                d.set_clipped(top, clipped).unwrap();
                if masked {
                    d.add_mask(top).unwrap();
                    fill_mask(&mut d, top, |x, y| ((x * 5 + y * 3) % 256) as u8);
                    d.set_mask_enabled(top, enabled).unwrap();
                }
                let eight = d.compose_tile(coord, None);
                d.convert_depth(tela_color::PixelDepth::Rgba16);
                assert_eq!(
                    d.compose_tile(coord, None),
                    eight,
                    "{mode:?} {clipped} {masked}"
                );
            }
        }
    }
}
