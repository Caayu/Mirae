//! The document model. It owns pixel data and layer properties and knows how
//! to flatten itself; it does not know about brushes, input or rendering.

mod layer;

pub use layer::{BlendMode, RasterLayer};

use tela_canvas::{IRect, TILE_SIZE, TileCoord};
use tela_color::{PremulRgba, Rgba};

/// Content drawn directly above one layer while compositing, e.g. the
/// preview of a stroke that has not been committed yet.
pub struct Overlay<'a> {
    pub above_layer: usize,
    pub paint: &'a dyn Fn(TileCoord, &mut [PremulRgba]),
    /// Tiles the overlay may draw into.
    pub covers: &'a dyn Fn(TileCoord) -> bool,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    /// Paper color shown under all layers.
    pub background: Rgba,
    /// Bottom to top.
    pub layers: Vec<RasterLayer>,
    pub active_layer: usize,
}

impl Document {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            background: Rgba::WHITE,
            layers: vec![RasterLayer::new("Layer 1")],
            active_layer: 0,
        }
    }

    pub fn bounds(&self) -> IRect {
        IRect::from_size(self.width, self.height)
    }

    pub fn active_layer_mut(&mut self) -> &mut RasterLayer {
        &mut self.layers[self.active_layer]
    }

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
        let overlay = overlay.filter(|o| (o.covers)(coord));
        let any_layer = self
            .layers
            .iter()
            .any(|l| l.visible && l.pixels.tile(coord).is_some());
        if !any_layer && overlay.is_none() {
            return false;
        }

        let ComposeScratch { acc, layer, out } = scratch;
        acc.fill(PremulRgba::TRANSPARENT);
        for (i, l) in self.layers.iter().enumerate() {
            let tile = l.pixels.tile(coord).filter(|_| l.visible);
            let draws_overlay = overlay.is_some_and(|o| o.above_layer == i);
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
            l.blend_mode.composite(layer, l.opacity, acc);
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

    use super::*;

    fn stroke_at(x: f32, y: f32, opacity: f32) -> StrokeBuffer {
        let mut settings = BrushPreset::plain(12.0);
        settings.properties.opacity = BrushProperty::constant(opacity);
        let mut e = tela_brush::BrushEngine::new(settings);
        e.begin_stroke(BrushInput::from_position(Vec2::new(x, y), 0.0), 1.0, 0);
        e.end_stroke().unwrap()
    }

    #[test]
    fn empty_tile_composes_to_none() {
        let d = Document::new(512, 512);
        assert!(d.compose_tile(TileCoord { x: 0, y: 0 }, None).is_none());
    }

    #[test]
    fn tile_outside_document_is_none() {
        let mut d = Document::new(100, 100);
        d.active_layer_mut().pixels.set_pixel(300, 300, [255; 4]);
        assert!(d.compose_tile(TileCoord { x: 1, y: 1 }, None).is_none());
    }

    #[test]
    fn pixels_beyond_document_edge_are_cleared() {
        let mut d = Document::new(100, 100);
        d.active_layer_mut().pixels.set_pixel(50, 50, [255; 4]);
        d.active_layer_mut().pixels.set_pixel(150, 50, [255; 4]);
        let t = d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap();
        assert_eq!(t[50 * 256 + 50], [255; 4]);
        assert_eq!(t[50 * 256 + 150], [0; 4]);
    }

    #[test]
    fn hidden_layer_is_skipped_and_opacity_applies() {
        let mut d = Document::new(256, 256);
        d.active_layer_mut()
            .pixels
            .set_pixel(0, 0, [255, 0, 0, 255]);
        d.layers[0].opacity = 0.5;
        let t = d.compose_tile(TileCoord { x: 0, y: 0 }, None).unwrap();
        assert_eq!(t[0], [128, 0, 0, 128]);
        d.layers[0].visible = false;
        assert!(d.compose_tile(TileCoord { x: 0, y: 0 }, None).is_none());
    }

    #[test]
    fn preview_matches_committed_result() {
        let mut d = Document::new(256, 256);
        d.layers[0].opacity = 0.6;
        let stroke = stroke_at(40.0, 40.0, 0.7);
        let coord = TileCoord { x: 0, y: 0 };
        let paint = |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px);
        let covers = |c: TileCoord| stroke.touched().contains(c);
        let o = Overlay {
            above_layer: 0,
            paint: &paint,
            covers: &covers,
        };
        let preview = d.compose_tile(coord, Some(&o)).unwrap();
        stroke.commit_into(&mut d.layers[0].pixels);
        let committed = d.compose_tile(coord, None).unwrap();
        let max_diff = preview
            .iter()
            .zip(&committed)
            .flat_map(|(a, b)| a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)))
            .max()
            .unwrap();
        // Committing quantizes the layer to 8 bits once more.
        assert!(max_diff <= 1, "preview differs from commit by {max_diff}");
    }
}
