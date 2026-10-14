use tela_canvas::{TILE_SIZE, TileCoord};
use tela_color::PremulRgba;

use crate::{Document, LayerId};

/// Content drawn directly above one layer while compositing, e.g. the
/// preview of a stroke that has not been committed yet.
pub struct Overlay<'a> {
    /// The layer the overlay is drawn on (before that layer's opacity is
    /// applied, as the committed result would be). Nothing is drawn if the
    /// layer does not exist or is hidden.
    pub target: LayerId,
    /// Draws the overlay into one tile of premultiplied pixels.
    pub paint: &'a dyn Fn(TileCoord, &mut [PremulRgba]),
    /// Tiles the overlay may draw into.
    pub covers: &'a dyn Fn(TileCoord) -> bool,
}

impl Document {
    /// Composites every visible layer (and the overlay) for one tile, over
    /// transparency. Returns `None` when nothing is there, so callers can
    /// skip or free the tile. Pixels outside the document are cleared.
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
        let any_layer = self
            .layers()
            .any(|l| l.visible() && l.pixels().tile(coord).is_some());
        if !any_layer && overlay.is_none() {
            return false;
        }

        let ComposeScratch { acc, layer, out } = scratch;
        acc.fill(PremulRgba::TRANSPARENT);
        for l in self.layers().filter(|l| l.visible()) {
            let tile = l.pixels().tile(coord);
            let draws_overlay = overlay.is_some_and(|o| o.target == l.id());
            if tile.is_none() && !draws_overlay {
                continue;
            }
            match tile {
                Some(t) => {
                    for (d, s) in layer.iter_mut().zip(t.pixels()) {
                        *d = PremulRgba::from_rgba8(*s);
                    }
                }
                None => layer.fill(PremulRgba::TRANSPARENT),
            }
            // The stroke previews as part of its target layer, so it is
            // affected by that layer's opacity just like after commit.
            if let Some(o) = overlay.filter(|_| draws_overlay) {
                (o.paint)(coord, layer);
            }
            l.blend_mode().composite(layer, l.opacity(), acc);
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

/// Reusable buffers for [`Document::compose_tile_into`] (about 2.3 MB).
pub struct ComposeScratch {
    acc: Vec<PremulRgba>,
    layer: Vec<PremulRgba>,
    /// The composed tile, premultiplied RGBA8.
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
            out: vec![[0; 4]; n],
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use tela_brush::dynamics::BrushProperty;
    use tela_brush::{BrushInput, BrushPreset, StrokeBuffer};
    use tela_canvas::DirtyTiles;

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
        d.modify_tiles(id, &tiles, |_, t| {
            *t.get_mut((x - ox) as u32, (y - oy) as u32) = p;
        })
        .unwrap();
    }

    fn commit(d: &mut Document, id: LayerId, stroke: &StrokeBuffer) {
        d.modify_tiles(id, stroke.touched(), |c, t| stroke.commit_tile(c, t))
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
        let paint = |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px);
        let covers = |c: TileCoord| stroke.touched().contains(c);
        let o = Overlay {
            target: l,
            paint: &paint,
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
        let paint = |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px);
        let covers = |c: TileCoord| stroke.touched().contains(c);
        let o = Overlay {
            target,
            paint: &paint,
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
        d.modify_tiles(l, &square, |_, t| {
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
        let paint = |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px);
        let covers = |c: TileCoord| stroke.touched().contains(c);
        let o = Overlay {
            target: l,
            paint: &paint,
            covers: &covers,
        };
        let preview = d.compose_tile(coord, Some(&o)).unwrap();
        commit(&mut d, l, &stroke);
        let committed = d.compose_tile(coord, None).unwrap();
        assert_eq!(committed[40 * 256 + 40], [54, 18, 9, 77], "70% erased");
        let diff = max_diff(&preview, &committed);
        assert!(diff <= 1, "preview differs from commit by {diff}");
    }
}
