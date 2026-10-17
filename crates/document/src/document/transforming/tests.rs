use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};

use crate::{
    BlendMode, ComposeScratch, Document, HistoryAction, Interpolation, LayerError, LayerId,
    Overlay, OverlayPaint, Placement, SelectionOp, SelectionShape, Transform, TransformParams,
};

/// A document with a layer "art" holding coloured, partly transparent
/// content over (40..120, 30..90), and a layer below it.
fn document(depth: PixelDepth) -> (Document, LayerId) {
    let mut d = Document::with_depth(300, 200, depth);
    let below = d.active_layer();
    let id = d.add_layer(Placement::Above(below)).unwrap();
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(IRect::new(40, 30, 120, 90));
    with_pixel_type!(depth, P => {
        d.modify_tiles(id, &tiles, |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for y in 30..90 {
                for x in 40..120 {
                    let (lx, ly) = (x - ox, y - oy);
                    if !(0..256).contains(&lx) || !(0..256).contains(&ly) {
                        continue;
                    }
                    // Colour with independent channels, alpha from 0.2 to 1.
                    let a = 0.2 + 0.8 * ((x * 3 + y * 7) % 50) as f32 / 49.0;
                    let p = PremulRgba::new(0.9 * a, 0.3 * a, ((x % 7) as f32 / 6.0) * a, a);
                    *t.get_mut(lx as u32, ly as u32) = P::from_premul(p);
                }
            }
        })
        .unwrap();
    });
    (d, id)
}

fn params(t: [f64; 2], s: [f64; 2], degrees: f64, pivot: [f64; 2]) -> TransformParams {
    TransformParams {
        translate: t,
        scale: s,
        rotation: degrees.to_radians(),
        pivot,
    }
}

fn bicubic(p: TransformParams) -> Transform {
    Transform {
        params: p,
        interpolation: Interpolation::Bicubic,
    }
}

#[test]
fn a_commit_is_one_step_that_undoes_and_redoes_bit_exactly() {
    for depth in PixelDepth::ALL {
        let (mut d, id) = document(depth);
        let before = d.digest();
        let rev = d.revision();
        let steps = d.history_len();
        let source = d.transform_source(id).unwrap();
        assert_eq!(source.bounds(), IRect::new(40, 30, 120, 90));
        assert_eq!(
            d.digest(),
            before,
            "taking the source changes nothing (TR1)"
        );
        assert_eq!(d.revision(), rev);
        let t = bicubic(params([13.0, -4.0], [1.6, 0.7], 23.0, source.centre()));
        let dirty = d.apply_transform(&source, &t).unwrap();
        assert!(!dirty.is_empty());
        assert_eq!(d.history_len(), steps + 1, "one step (TR3)");
        assert_eq!(d.next_undo(), Some(HistoryAction::Transform));
        let after = d.digest();
        assert_ne!(after, before);
        d.check_invariants();
        d.undo().unwrap();
        assert_eq!(d.digest(), before, "undo restores the source exactly");
        assert_eq!(d.revision(), rev);
        d.redo().unwrap();
        assert_eq!(d.digest(), after, "redo restores the result exactly");
        // Ids and properties do not change (TR7).
        let l = d.layer(id).unwrap();
        assert_eq!(
            (
                l.opacity(),
                l.blend_mode(),
                l.clipped(),
                l.locked(),
                l.visible()
            ),
            (1.0, BlendMode::Normal, false, false, true)
        );
        assert_eq!(d.active_layer(), id);
    }
}

#[test]
fn refusals_change_nothing() {
    let (mut d, id) = document(PixelDepth::Rgba8);
    let below = d.layer_below(id).unwrap();
    // Nothing to transform on an empty layer without a selection (T36).
    assert_eq!(
        d.transform_source(below).err(),
        Some(LayerError::NothingToTransform(below))
    );
    let before = d.digest();
    let steps = d.history_len();
    let source = d.transform_source(id).unwrap();
    // Invalid parameters.
    let mut bad = bicubic(params([0.0; 2], [0.0, 1.0], 0.0, [0.0; 2]));
    assert_eq!(
        d.apply_transform(&source, &bad).err(),
        Some(LayerError::InvalidTransform)
    );
    bad.params = params([f64::NAN, 0.0], [1.0; 2], 0.0, [0.0; 2]);
    assert_eq!(
        d.apply_transform(&source, &bad).err(),
        Some(LayerError::InvalidTransform)
    );
    // Beyond the tiles a layer can address.
    let far = bicubic(params([0.0; 2], [100.0; 2], 0.0, [-9e8, 0.0]));
    assert_eq!(
        d.apply_transform(&source, &far).err(),
        Some(LayerError::InvalidTransform)
    );
    assert_eq!(d.digest(), before, "TR9");
    assert_eq!(d.history_len(), steps);
    // Hidden and locked layers (T34, T35).
    d.set_visible(id, false).unwrap();
    assert_eq!(d.transform_source(id).err(), Some(LayerError::Hidden(id)));
    d.set_visible(id, true).unwrap();
    d.set_locked(id, true).unwrap();
    assert_eq!(d.transform_source(id).err(), Some(LayerError::Locked(id)));
    let ok = bicubic(params([5.0, 0.0], [1.0; 2], 0.0, [0.0; 2]));
    assert_eq!(
        d.apply_transform(&source, &ok).err(),
        Some(LayerError::Locked(id))
    );
    d.set_locked(id, false).unwrap();
    // The document changed since the source was taken.
    assert_eq!(
        d.apply_transform(&source, &ok).err(),
        Some(LayerError::StaleTransform)
    );
    d.check_invariants();
}

#[test]
fn an_identity_records_nothing_and_a_cancel_is_simply_no_apply() {
    for depth in PixelDepth::ALL {
        let (mut d, id) = document(depth);
        let before = d.digest();
        let steps = d.history_len();
        let source = d.transform_source(id).unwrap();
        let identity = bicubic(TransformParams::identity(source.centre()));
        assert!(d.apply_transform(&source, &identity).unwrap().is_empty());
        assert_eq!(d.digest(), before);
        assert_eq!(d.history_len(), steps, "no-ops record nothing");
        // A session dropped without applying (cancel) leaves the document
        // as it was, bit for bit (TR2).
        drop(d.transform_source(id).unwrap());
        assert_eq!(d.digest(), before);
    }
}

/// The preview of `t`: the layer's tiles through the transform overlay,
/// exactly as the app shows them, quantized at `P`.
fn preview_tiles<P: StoragePixel>(
    d: &Document,
    source: &crate::TransformSource,
    t: &Transform,
) -> Vec<(TileCoord, Option<Tile<P>>)> {
    let id = source.layer();
    let mut out = Vec::new();
    let surface = d.layer(id).unwrap().pixels().surface::<P>().unwrap();
    let mut px = vec![PremulRgba::TRANSPARENT; (TILE_SIZE * TILE_SIZE) as usize];
    for c in source.affected_tiles(&t.params).iter() {
        match surface.tile(c) {
            Some(tile) => {
                for (d, p) in px.iter_mut().zip(tile.pixels()) {
                    *d = p.to_premul();
                }
            }
            None => px.fill(PremulRgba::TRANSPARENT),
        }
        assert!(source.transform_pixels_tile(t, c, &d.selection_tile(c), &mut px));
        let mut tile = Tile::<P>::new();
        for (d, s) in tile.pixels_mut().iter_mut().zip(&px) {
            *d = P::from_premul(*s);
        }
        out.push((c, (!tile.is_blank()).then_some(tile)));
    }
    out
}

#[test]
fn the_preview_quantized_once_is_the_commit() {
    // TR4, over the matrix: translate, scale, rotate × both depths × no
    // selection, a hard and a feathered selection × colour and partial
    // alpha (the content) × both interpolations.
    let transforms = [
        params([17.0, 9.0], [1.0; 2], 0.0, [80.0, 60.0]),
        params([0.0; 2], [2.3, 1.7], 0.0, [80.0, 60.0]),
        params([0.0; 2], [0.35, 0.4], 0.0, [80.0, 60.0]),
        params([3.0, 2.0], [1.2, 0.9], 37.0, [70.0, 50.0]),
        params([0.0; 2], [-1.0, 1.0], 0.0, [80.0, 60.0]),
    ];
    let selections: [Option<(f32, bool)>; 3] = [None, Some((0.0, false)), Some((8.0, true))];
    for depth in PixelDepth::ALL {
        for sel in selections {
            for p in transforms {
                for interpolation in [Interpolation::Bicubic, Interpolation::Nearest] {
                    let (mut d, id) = document(depth);
                    if let Some((feather, aa)) = sel {
                        d.select(
                            &SelectionShape::Ellipse {
                                center: [70.0, 55.0],
                                radii: [40.0, 25.0],
                                rotation: 0.3,
                            },
                            aa,
                            SelectionOp::Replace,
                        );
                        if feather > 0.0 {
                            d.feather_selection(feather);
                        }
                    }
                    let source = d.transform_source(id).unwrap();
                    let t = Transform {
                        params: p,
                        interpolation,
                    };
                    let preview = with_pixel_type!(depth, P => {
                        let tiles = preview_tiles::<P>(&d, &source, &t);
                        tiles.into_iter().map(|(c, t)| (c, t.map(|t| t.pixels().iter().map(|p| p.to_premul()).collect::<Vec<_>>()))).collect::<Vec<_>>()
                    });
                    d.apply_transform(&source, &t).unwrap();
                    let l = d.layer(id).unwrap();
                    for (c, want) in preview {
                        let got = l.pixels().has_tile(c).then(|| {
                            let (ox, oy) = c.origin();
                            (0..TILE_SIZE as i32)
                                .flat_map(|y| (0..TILE_SIZE as i32).map(move |x| (x, y)))
                                .map(|(x, y)| l.pixels().pixel(ox + x, oy + y))
                                .collect::<Vec<_>>()
                        });
                        assert_eq!(got, want, "{depth:?} {sel:?} {p:?} {interpolation:?} {c:?}");
                    }
                    d.check_invariants();
                }
            }
        }
    }
}

#[test]
fn the_composed_preview_equals_the_composed_commit() {
    // TR4 through the compositor, as the app previews (the overlay paints
    // the layer's pixels and mask).
    for depth in PixelDepth::ALL {
        let (mut d, id) = document(depth);
        d.add_mask(id).unwrap();
        let mut tiles = DirtyTiles::new();
        tiles.mark_rect(IRect::new(60, 40, 90, 70));
        d.modify_mask(id, &tiles, |c, t| {
            let (ox, oy) = c.origin();
            for y in 40..70 {
                for x in 60..90 {
                    *t.get_mut((x - ox) as u32, (y - oy) as u32) = ((x * 5) % 200) as u8;
                }
            }
        })
        .unwrap();
        let source = d.transform_source(id).unwrap();
        assert!(source.moves_mask());
        let t = bicubic(params([10.0, 5.0], [1.3, 1.1], 12.0, source.centre()));
        let pixels = |c: TileCoord, px: &mut [PremulRgba]| {
            source.transform_pixels_tile(&t, c, &d.selection_tile(c), px);
        };
        let mask = |c: TileCoord, r: &mut [f32]| {
            source.transform_mask_tile(&t, c, r);
        };
        let affected = source.affected_tiles(&t.params);
        let covers = |c: TileCoord| affected.contains(c);
        let overlay = Overlay {
            target: id,
            paint: OverlayPaint::Layer {
                pixels: &pixels,
                mask: &mask,
            },
            covers: &covers,
        };
        let mut scratch = ComposeScratch::new();
        let previews: Vec<_> = affected
            .iter()
            .map(|c| {
                let has = d.compose_tile_into(c, Some(&overlay), &mut scratch);
                (c, has.then(|| scratch.out.clone()))
            })
            .collect();
        d.apply_transform(&source, &t).unwrap();
        for (c, want) in previews {
            let got = d.compose_tile(c, None);
            match (got, want) {
                (Some(a), Some(b)) => {
                    let e = a
                        .iter()
                        .zip(&b)
                        .flat_map(|(p, q)| (0..4).map(move |k| p[k].abs_diff(q[k])))
                        .max()
                        .unwrap();
                    assert!(e <= 1, "{depth:?} {c:?}: {e}");
                }
                (None, None) => {}
                (a, b) => panic!("{depth:?} {c:?}: {} vs {}", a.is_some(), b.is_some()),
            }
        }
    }
}

#[test]
fn sixteen_bit_content_never_passes_through_eight_bits() {
    // TR6: a 16-bit level between two 8-bit levels survives a copy (a
    // whole-pixel move) exactly, and a resampled result keeps values off
    // the 8-bit grid.
    let mut d = Document::with_depth(64, 64, PixelDepth::Rgba16);
    let id = d.active_layer();
    let mut tiles = DirtyTiles::new();
    tiles.mark(TileCoord { x: 0, y: 0 });
    d.modify_tiles(id, &tiles, |_, t: &mut Tile<[u16; 4]>| {
        for y in 10..30 {
            for x in 10..30 {
                let v = 30_000 + (x * 13 + y * 7) as u16;
                *t.get_mut(x, y) = [v / 3, v / 2, v, 60_001];
            }
        }
    })
    .unwrap();
    let source = d.transform_source(id).unwrap();
    d.apply_transform(
        &source,
        &bicubic(params([7.0, 3.0], [1.0; 2], 0.0, [0.0; 2])),
    )
    .unwrap();
    let s = d.layer(id).unwrap().pixels().rgba16().unwrap();
    for y in 10..30 {
        for x in 10..30 {
            let v = 30_000 + (x * 13 + y * 7) as u16;
            assert_eq!(s.pixel(x + 7, y + 3), [v / 3, v / 2, v, 60_001]);
        }
    }
    let source = d.transform_source(id).unwrap();
    d.apply_transform(
        &source,
        &bicubic(params([0.0; 2], [1.37, 1.37], 0.0, source.centre())),
    )
    .unwrap();
    let s = d.layer(id).unwrap().pixels().rgba16().unwrap();
    let off_grid = (0..64)
        .flat_map(|y| (0..64).map(move |x| (x, y)))
        .filter(|&(x, y)| s.pixel(x, y).iter().any(|v| v % 257 != 0))
        .count();
    assert!(off_grid > 300, "{off_grid}");
}

#[test]
fn pixels_outside_the_selection_change_only_where_content_lands() {
    // TR8 (T29–T31).
    let (mut d, id) = document(PixelDepth::Rgba8);
    d.select(
        &SelectionShape::Rect([50.0, 40.0], [90.0, 70.0]),
        false,
        SelectionOp::Replace,
    );
    d.feather_selection(4.0);
    let before = d.clone();
    let source = d.transform_source(id).unwrap();
    assert_eq!(source.bounds(), d.selection().unwrap().bounds(), "T28");
    let t = bicubic(params([60.0, 50.0], [1.2, 1.2], 10.0, source.centre()));
    let landing = source.dest_bounds(&t.params).unwrap();
    d.apply_transform(&source, &t).unwrap();
    let (b, a) = (
        before.layer(id).unwrap().pixels(),
        d.layer(id).unwrap().pixels(),
    );
    let mut checked = 0;
    for y in 0..200 {
        for x in 0..300 {
            let inside = x >= landing.x0 && x < landing.x1 && y >= landing.y0 && y < landing.y1;
            if before.selection_coverage(x, y) == 0 && !inside {
                assert_eq!(a.pixel_rgba8(x, y), b.pixel_rgba8(x, y), "({x}, {y})");
                checked += 1;
            }
        }
    }
    assert!(checked > 40_000);
    // The selection followed the content (T32) and undo brings it back.
    assert!(d.selection().unwrap().bounds().x0 > 90);
    d.undo().unwrap();
    assert_eq!(d.selection(), before.selection());
}

#[test]
fn the_selection_bounds_are_the_box_even_over_empty_pixels() {
    // T28: a selection over nothing still opens a box; moving it moves
    // only the selection.
    let (mut d, id) = document(PixelDepth::Rgba8);
    d.select(
        &SelectionShape::Rect([200.0, 120.0], [250.0, 160.0]),
        false,
        SelectionOp::Replace,
    );
    let pixels = d.layer(id).unwrap().pixels().tile_count();
    let source = d.transform_source(id).unwrap();
    let t = bicubic(params([-30.0, 10.0], [1.0; 2], 0.0, source.centre()));
    d.apply_transform(&source, &t).unwrap();
    assert_eq!(d.layer(id).unwrap().pixels().tile_count(), pixels);
    assert_eq!(
        d.selection().unwrap().bounds(),
        IRect::new(170, 130, 220, 170)
    );
    assert_eq!(d.next_undo(), Some(HistoryAction::Transform));
}
