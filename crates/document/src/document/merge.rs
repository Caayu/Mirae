//! Merge down (phase 5B, ADR 0023): a layer and the one below become one.
//!
//! Semantics measured against the reference
//! (docs/parity/5B-transparent-compositing.md, part 2): the pair is
//! composed **alone**, over transparency, exactly as the compositor would
//! compose those two layers (masks, opacities, blend modes, clipping and
//! the translucent-backdrop rule of ADR 0022); the result is stored in the
//! lower layer, which keeps its id, name, position, blend mode, clipping,
//! alpha lock and (if the caller keeps it) its mask, with its opacity
//! baked in (100 %). The upper layer is removed. One history step.

use tela_canvas::{DirtyTiles, TILE_SIZE, Tile, TileCoord};
use tela_color::{PremulRgba, StoragePixel, with_pixel_type};

use super::Document;
use super::history::{Change, HistoryAction, layer_bytes, mask_bytes};
use crate::compose::apply_mask;
use crate::pixels::{TileBackup, tile_bytes};
use crate::{LayerError, LayerId, LayerPart, RasterLayer};

/// What [`Document::merge_down`] does with an **enabled** mask on the
/// lower layer. The reference asks ("apply the mask before merging?",
/// Apply / Keep / Cancel); the caller decides. A disabled mask is kept as
/// it is either way, and the upper layer's mask is always applied when
/// enabled (dropped otherwise).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LowerMask {
    /// Bake the mask into the merged pixels and delete it: the merged
    /// layer looks exactly like the pair did.
    #[default]
    Apply,
    /// Keep the mask on the merged layer; the lower pixels are merged
    /// unmasked, so the mask now also hides the upper layer's content.
    Keep,
}

impl Document {
    /// Merges layer `id` into the layer directly below it.
    ///
    /// The two layers are composed alone over transparency (their masks,
    /// opacities, blend modes and clipping, as on screen without anything
    /// below them) and the result replaces the lower layer's pixels at the
    /// document's depth, quantized once. The lower layer keeps its id,
    /// name, position, blend mode, clip flag and alpha lock; its opacity
    /// becomes 100 % (baked in); its mask follows `lower_mask`. The upper
    /// layer is removed (its id is never reused). The merged layer becomes
    /// active, with its pixels as the edit target.
    ///
    /// Refused, changing nothing, when `id` is the bottom layer
    /// ([`LayerError::NoLayerBelow`]), or either layer is locked or hidden
    /// (the reference disables the command then).
    ///
    /// One history step; undo restores both layers (ids, pixels, masks,
    /// properties, order) and the active layer bit for bit. The step holds
    /// the upper layer and the previous content of the lower layer's
    /// changed tiles only: the tiles under the upper layer's pixels, or
    /// every tile of the lower layer when its opacity or mask is baked in.
    pub fn merge_down(
        &mut self,
        id: LayerId,
        lower_mask: LowerMask,
    ) -> Result<DirtyTiles, LayerError> {
        let ti = self.require(id)?;
        let bi = ti.checked_sub(1).ok_or(LayerError::NoLayerBelow(id))?;
        for layer in [&self.layers[ti], &self.layers[bi]] {
            if layer.locked {
                return Err(LayerError::Locked(layer.id()));
            }
            if !layer.visible {
                return Err(LayerError::Hidden(layer.id()));
            }
        }
        let bottom_id = self.layers[bi].id();
        let apply_mask = lower_mask == LowerMask::Apply
            && self.layers[bi].mask.as_ref().is_some_and(|m| m.enabled);
        let bake = apply_mask || self.layers[bi].opacity < 1.0;

        let mut tiles = DirtyTiles::new();
        for c in self.layers[ti].pixels.tile_coords() {
            tiles.mark(c);
        }
        if bake {
            for c in self.layers[bi].pixels.tile_coords() {
                tiles.mark(c);
            }
        }
        let mut dirty = self.group_tiles(bi);
        dirty.extend(&self.group_tiles(ti));

        let (backup, before, after, entries) = with_pixel_type!(self.depth, P => {
            let (b, before, after) = merge_tiles::<P>(&mut self.layers, bi, ti, &tiles, apply_mask);
            let entries = b.len() * std::mem::size_of::<(TileCoord, Option<Tile<P>>)>();
            (TileBackup::new(b), before, after, entries)
        });
        // As for a stroke (`modify_tiles`): the larger direction's tiles.
        let mut cost = before.max(after) * tile_bytes(self.depth) + entries;
        let mut changes = vec![Change::Tiles {
            layer: bottom_id,
            tiles: backup,
        }];
        let old_opacity = std::mem::replace(&mut self.layers[bi].opacity, 1.0);
        if old_opacity != 1.0 {
            changes.push(Change::Opacity {
                layer: bottom_id,
                other: old_opacity,
            });
        }
        if apply_mask {
            let mask = self.layers[bi].mask.take().expect("checked above");
            cost += mask_bytes(&mask);
            changes.push(Change::MaskPresence {
                layer: bottom_id,
                mask: Some(Box::new(mask)),
            });
        }
        let top = self.layers.remove(ti);
        cost += layer_bytes(&top);
        changes.push(Change::Presence {
            index: ti,
            layer: Some(Box::new(top)),
        });

        let before_active = std::mem::replace(&mut self.active, bottom_id);
        self.active_part = LayerPart::Pixels;
        self.record(HistoryAction::MergeDown, changes, before_active, cost);
        dirty.extend(&self.group_tiles(bi));
        Ok(dirty)
    }
}

/// Previous content of changed tiles (`None` = absent).
type Backup<P> = Vec<(TileCoord, Option<Tile<P>>)>;

/// Composes layer `ti` onto layer `bi` in each of `tiles` and stores the
/// result in `bi` at depth `P`. Returns the previous content of the tiles
/// that changed (`None` = absent) and how many of them were stored before
/// and after.
fn merge_tiles<P: StoragePixel>(
    layers: &mut [RasterLayer],
    bi: usize,
    ti: usize,
    tiles: &DirtyTiles,
    apply_lower_mask: bool,
) -> (Backup<P>, usize, usize) {
    let n = (TILE_SIZE * TILE_SIZE) as usize;
    let mut lower = vec![PremulRgba::TRANSPARENT; n];
    let mut upper = vec![PremulRgba::TRANSPARENT; n];
    let (below, above) = layers.split_at_mut(ti);
    let (bottom, top) = (&mut below[bi], &above[0]);
    // A clipped upper layer whose base is the lower layer applies
    // source-atop (ADR 0017); otherwise the pair blends like any two
    // layers (ADR 0022). Measured: md_clip_into_base, md_clip_into_clip.
    let atop = top.clipped && !bottom.clipped;
    let opacity = bottom.opacity;
    let mut previous = Vec::new();
    let (mut before, mut after) = (0, 0);
    for coord in tiles.iter() {
        bottom.pixels.decode_tile(coord, &mut lower);
        if apply_lower_mask && let Some(m) = &bottom.mask {
            apply_mask(m, coord, &mut lower);
        }
        if opacity < 1.0 {
            for p in &mut lower {
                *p = p.scale(opacity);
            }
        }
        top.pixels.decode_tile(coord, &mut upper);
        if let Some(m) = top.mask.as_ref().filter(|m| m.enabled) {
            apply_mask(m, coord, &mut upper);
        }
        if atop {
            top.blend_mode.clip_atop(&upper, top.opacity, &mut lower);
        } else {
            top.blend_mode.composite(&upper, top.opacity, &mut lower);
        }

        let surface = bottom
            .pixels
            .surface_mut::<P>()
            .expect("every layer stores the document's depth");
        let old = surface.tile(coord).cloned();
        let tile = surface.tile_or_insert(coord);
        for (d, s) in tile.pixels_mut().iter_mut().zip(&lower) {
            *d = P::from_premul(*s);
        }
        if old.is_none() && tile.is_blank() {
            surface.remove_tile(coord);
        }
        let new = surface.tile(coord);
        if old.as_ref() != new {
            before += usize::from(old.is_some());
            after += usize::from(new.is_some());
            previous.push((coord, old));
        }
    }
    (previous, before, after)
}

#[cfg(test)]
mod tests {
    use tela_canvas::{DirtyTiles, Tile, TileCoord};
    use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};

    use super::LowerMask;
    use crate::{BlendMode, Document, LayerError, LayerId, Placement};

    fn put(d: &mut Document, id: LayerId, x: i32, y: i32, p: PremulRgba) {
        let c = TileCoord::containing(x, y);
        let mut t = DirtyTiles::new();
        t.mark(c);
        let (ox, oy) = c.origin();
        with_pixel_type!(d.depth(), P => d.modify_tiles(id, &t, |_, tile: &mut Tile<P>| {
            *tile.get_mut((x - ox) as u32, (y - oy) as u32) = P::from_premul(p);
        }))
        .unwrap();
    }

    /// Two layers with a pixel each; returns (document, lower, upper).
    fn pair(depth: PixelDepth) -> (Document, LayerId, LayerId) {
        let mut d = Document::with_depth(1024, 1024, depth);
        let lower = d.active_layer();
        put(&mut d, lower, 3, 3, PremulRgba::new(0.4, 0.1, 0.1, 0.5));
        let upper = d.add_layer(Placement::Top).unwrap();
        put(&mut d, upper, 3, 3, PremulRgba::new(0.0, 0.3, 0.0, 0.6));
        (d, lower, upper)
    }

    #[test]
    fn the_lower_layer_keeps_its_identity_and_the_upper_one_goes() {
        let (mut d, lower, upper) = pair(PixelDepth::Rgba8);
        d.set_name(lower, "ink").unwrap();
        d.set_blend_mode(lower, BlendMode::Multiply).unwrap();
        d.set_opacity(lower, 0.5).unwrap();
        d.set_clipped(lower, true).unwrap();
        d.set_alpha_locked(lower, true).unwrap();
        d.merge_down(upper, LowerMask::Apply).unwrap();
        assert_eq!(d.layer_count(), 1);
        assert!(d.layer(upper).is_none());
        let l = d.layer(lower).unwrap();
        assert_eq!(l.name(), "ink");
        assert_eq!(l.blend_mode(), BlendMode::Multiply, "the mode stays");
        assert_eq!(l.opacity(), 1.0, "the opacity is baked in");
        assert!(l.clipped() && l.alpha_locked());
        assert_eq!(d.active_layer(), lower);
    }

    #[test]
    fn refusals_change_nothing() {
        let (mut d, lower, upper) = pair(PixelDepth::Rgba8);
        let before = d.digest();
        let steps = d.history_len();
        assert_eq!(
            d.merge_down(lower, LowerMask::Apply).map(|_| ()),
            Err(LayerError::NoLayerBelow(lower))
        );
        d.set_visible(upper, false).unwrap();
        assert_eq!(
            d.merge_down(upper, LowerMask::Apply).map(|_| ()),
            Err(LayerError::Hidden(upper))
        );
        d.set_visible(upper, true).unwrap();
        d.set_visible(lower, false).unwrap();
        assert_eq!(
            d.merge_down(upper, LowerMask::Apply).map(|_| ()),
            Err(LayerError::Hidden(lower))
        );
        d.set_visible(lower, true).unwrap();
        d.set_locked(upper, true).unwrap();
        assert_eq!(
            d.merge_down(upper, LowerMask::Apply).map(|_| ()),
            Err(LayerError::Locked(upper))
        );
        d.set_locked(upper, false).unwrap();
        d.set_locked(lower, true).unwrap();
        assert_eq!(
            d.merge_down(upper, LowerMask::Apply).map(|_| ()),
            Err(LayerError::Locked(lower))
        );
        d.set_locked(lower, false).unwrap();
        assert_eq!(d.digest(), before, "MD4");
        // Locking and unlocking were the only steps.
        assert_eq!(d.history_len(), steps + 4);
    }

    #[test]
    fn merging_is_one_step_and_undo_restores_both_layers_exactly() {
        for depth in [PixelDepth::Rgba8, PixelDepth::Rgba16] {
            let (mut d, lower, upper) = pair(depth);
            d.add_mask(lower).unwrap();
            let mut t = DirtyTiles::new();
            t.mark(TileCoord { x: 0, y: 0 });
            d.modify_mask(lower, &t, |_, m| *m.get_mut(3, 3) = 100)
                .unwrap();
            d.set_opacity(lower, 0.7).unwrap();
            d.set_active_layer(upper).unwrap();
            let before = d.digest();
            let steps = d.history_len();
            d.merge_down(upper, LowerMask::Apply).unwrap();
            let merged = d.digest();
            assert_eq!(d.history_len(), steps + 1, "MD3: one step");
            assert!(d.layer(lower).unwrap().mask().is_none(), "applied");
            d.undo().unwrap();
            assert_eq!(
                d.digest(),
                before,
                "MD3: undo is exact, active layer included"
            );
            assert_eq!(d.active_layer(), upper);
            d.redo().unwrap();
            assert_eq!(d.digest(), merged, "MD3: redo is exact");
            d.check_invariants();
        }
    }

    #[test]
    fn sixteen_bit_precision_survives_the_merge() {
        // MD5: a value between two 8-bit levels stays in a 16-bit document.
        let mut d = Document::with_depth(256, 256, PixelDepth::Rgba16);
        let lower = d.active_layer();
        let fine = PremulRgba::new(0.30001, 0.5, 0.7, 1.0);
        put(&mut d, lower, 1, 1, fine);
        let upper = d.add_layer(Placement::Top).unwrap();
        put(&mut d, upper, 9, 9, PremulRgba::new(0.0, 0.0, 0.0, 1.0));
        let stored = |d: &Document| {
            d.layer(lower)
                .unwrap()
                .pixels()
                .rgba16()
                .unwrap()
                .pixel(1, 1)
        };
        let want = stored(&d);
        d.merge_down(upper, LowerMask::Apply).unwrap();
        assert_eq!(d.depth(), PixelDepth::Rgba16);
        assert_eq!(stored(&d), want, "untouched pixels keep every bit");
        assert_eq!(<[u16; 4]>::from_premul(fine), want);
    }

    #[test]
    fn the_paper_is_never_merged_in() {
        // MD6: a Screen layer over an empty lower layer composes over
        // transparency (it shows its own colour), not over the white
        // paper, which would give white.
        let mut d = Document::new(256, 256);
        let lower = d.active_layer();
        let upper = d.add_layer(Placement::Top).unwrap();
        put(&mut d, upper, 5, 5, PremulRgba::new(0.2, 0.0, 0.0, 1.0));
        d.set_blend_mode(upper, BlendMode::Screen).unwrap();
        d.merge_down(upper, LowerMask::Apply).unwrap();
        let pixels = d.layer(lower).unwrap().pixels();
        assert_eq!(pixels.pixel_rgba8(5, 5), [51, 0, 0, 255]);
        assert_eq!(pixels.pixel_rgba8(6, 5), [0; 4]);
    }

    #[test]
    fn history_holds_only_the_changed_tiles_and_the_upper_layer() {
        let mut d = Document::new(2048, 2048);
        let lower = d.active_layer();
        for x in 0..4 {
            put(
                &mut d,
                lower,
                x * 256 + 1,
                1,
                PremulRgba::new(0.5, 0.5, 0.5, 1.0),
            );
        }
        let upper = d.add_layer(Placement::Top).unwrap();
        put(&mut d, upper, 1, 1, PremulRgba::new(0.0, 0.0, 0.5, 0.5));
        let held = d.history_bytes();
        d.merge_down(upper, LowerMask::Apply).unwrap();
        let tile = 256 * 256 * 4;
        // The lower tile under the upper pixels and the upper layer's one
        // tile, not all four lower tiles.
        let step = d.history_bytes() - held;
        assert!((2 * tile..3 * tile).contains(&step), "{step}");
        assert!(d.history_bytes() <= d.history_reserved_bytes());
        // Baking the opacity changes every lower tile.
        let (mut d2, l2, u2) = pair(PixelDepth::Rgba8);
        put(&mut d2, l2, 700, 700, PremulRgba::new(0.5, 0.5, 0.5, 1.0));
        d2.set_opacity(l2, 0.5).unwrap();
        let held = d2.history_bytes();
        d2.merge_down(u2, LowerMask::Apply).unwrap();
        assert!(d2.history_bytes() - held >= 3 * tile);
    }

    #[test]
    fn a_kept_lower_mask_stays_and_hides_the_merged_content() {
        let (mut d, lower, upper) = pair(PixelDepth::Rgba8);
        d.add_mask(lower).unwrap();
        let mut t = DirtyTiles::new();
        t.mark(TileCoord { x: 0, y: 0 });
        d.modify_mask(lower, &t, |_, m| *m.get_mut(3, 3) = 0)
            .unwrap();
        d.merge_down(upper, LowerMask::Keep).unwrap();
        let l = d.layer(lower).unwrap();
        assert_eq!(l.mask().unwrap().value(3, 3), 0, "kept");
        // The merged pixel holds both layers unmasked; the mask hides it.
        assert!(l.pixels().pixel_rgba8(3, 3)[3] > 150);
        let shown = d.compose_tile(TileCoord { x: 0, y: 0 }, None);
        assert!(shown.is_none_or(|t| t[3 * 256 + 3] == [0; 4]));
    }
}
