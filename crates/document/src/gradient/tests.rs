//! Gradients: the geometry and extend rules on numbers that can be checked
//! by hand, and the document operation (history, preview == commit,
//! selection, alpha lock, masks, depths, refusals, sparsity).

use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, Tile, TileCoord};
use tela_color::gradient::{GradientStop, GradientStops, Mixing, ease};
use tela_color::{Color, PixelDepth, PremulRgba, StoragePixel, with_pixel_type};

use super::*;
use crate::{
    BlendMode, Document, FillTarget, HistoryAction, LayerError, LayerId, LayerPart, SelectionOp,
    SelectionShape,
};

fn bw() -> GradientStops {
    GradientStops::two(Color::BLACK, Color::WHITE)
}

fn geometry(shape: GradientShape, start: [f64; 2], end: [f64; 2]) -> GradientGeometry {
    GradientGeometry { shape, start, end }
}

fn gradient(
    shape: GradientShape,
    start: [f64; 2],
    end: [f64; 2],
    extend: GradientExtend,
) -> Gradient {
    Gradient::new(
        geometry(shape, start, end),
        &bw(),
        Mixing::Standard,
        extend,
        false,
    )
}

/// The red channel (grey level) of the paint at pixel `(x, y)`, `0..=1`.
fn grey(g: &Gradient, x: i32, y: i32) -> f32 {
    g.sample(x, y).r
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

fn pixels(id: LayerId) -> FillTarget {
    FillTarget {
        layer: id,
        part: LayerPart::Pixels,
    }
}

fn mask(id: LayerId) -> FillTarget {
    FillTarget {
        layer: id,
        part: LayerPart::Mask,
    }
}

fn paint_rect(d: &mut Document, id: LayerId, r: IRect, p: [u8; 4]) {
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(r);
    with_pixel_type!(d.depth(), P => {
        d.modify_tiles(id, &tiles, |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    let (lx, ly) = (x - ox, y - oy);
                    if (0..TILE_SIZE as i32).contains(&lx) && (0..TILE_SIZE as i32).contains(&ly) {
                        *t.get_mut(lx as u32, ly as u32) = P::from_rgba8(p);
                    }
                }
            }
        })
        .unwrap();
    });
}

// --- Geometry -------------------------------------------------------------

#[test]
fn linear_endpoints_middle_and_quarter() {
    // Pixel centres: pixel x sits at t = (x + ½ − 100.5) / 200 = (x − 100) / 200.
    let g = gradient(
        GradientShape::Linear,
        [100.5, 7.0],
        [300.5, 7.0],
        GradientExtend::Clamp,
    );
    for y in [0, 7, 99] {
        assert_eq!(grey(&g, 100, y), 0.0, "start");
        assert!(close(grey(&g, 300, y), 1.0), "end");
        assert!(close(grey(&g, 200, y), 0.5), "middle");
        assert!(close(grey(&g, 150, y), ease(0.25)), "quarter: eased");
        assert!(close(grey(&g, 250, y), ease(0.75)));
        assert_eq!(grey(&g, -50, y), 0.0, "clamped before the start");
        assert!(close(grey(&g, 1000, y), 1.0), "clamped after the end");
    }
    // The reverse drag mirrors it (GD: the start always gets the first stop).
    let r = gradient(
        GradientShape::Linear,
        [300.5, 7.0],
        [100.5, 7.0],
        GradientExtend::Clamp,
    );
    for x in (100..=300).step_by(17) {
        assert!(close(grey(&r, x, 3), 1.0 - grey(&g, x, 3)), "{x}");
    }
}

#[test]
fn linear_projects_on_a_diagonal_drag() {
    // GD17: constant along lines perpendicular to the drag.
    let g = gradient(
        GradientShape::Linear,
        [100.5, 50.5],
        [300.5, 250.5],
        GradientExtend::Clamp,
    );
    let mid = grey(&g, 200, 150);
    assert!(close(mid, 0.5));
    for k in [-60, -10, 10, 45] {
        assert!(close(grey(&g, 200 + k, 150 - k), mid), "{k}");
    }
    assert!(close(grey(&g, 100, 50), 0.0));
    assert!(close(grey(&g, 300, 250), 1.0));
    // Negative coordinates and non-square directions work the same way.
    let n = gradient(
        GradientShape::Linear,
        [-40.5, -10.5],
        [-0.5, -10.5],
        GradientExtend::Clamp,
    );
    assert!(close(grey(&n, -21, 0), 0.5));
}

#[test]
fn radial_elliptical_and_reflected_distances() {
    let r = gradient(
        GradientShape::Radial,
        [150.5, 100.5],
        [250.5, 100.5],
        GradientExtend::Clamp,
    );
    assert_eq!(grey(&r, 150, 100), 0.0, "centre");
    // Distance 50 in every direction: the same value.
    for (x, y) in [(200, 100), (100, 100), (150, 150), (150, 50)] {
        assert!(close(grey(&r, x, y), 0.5), "{x} {y}");
    }
    assert!(close(grey(&r, 250, 100), 1.0));
    // Ellipse with radii 100 × 50 (GD19): half the distance vertically.
    let e = gradient(
        GradientShape::Elliptical,
        [200.5, 150.5],
        [300.5, 200.5],
        GradientExtend::Clamp,
    );
    assert!(close(grey(&e, 250, 150), grey(&e, 200, 175)));
    assert!(close(grey(&e, 250, 150), 0.5));
    assert!(close(grey(&e, 300, 150), 1.0));
    assert!(close(grey(&e, 200, 200), 1.0));
    // Reflected: the same on both sides of the start.
    let m = gradient(
        GradientShape::Reflected,
        [200.5, 0.0],
        [300.5, 0.0],
        GradientExtend::Clamp,
    );
    for k in [0, 10, 33, 50, 99, 140] {
        assert!(close(grey(&m, 200 + k, 9), grey(&m, 200 - k, 9)), "{k}");
    }
    assert!(close(grey(&m, 250, 0), 0.5));
}

#[test]
fn angular_goes_round_clockwise_with_an_anti_aliased_seam() {
    // Start at the centre, drag to the right: 0 to the right, ¼ down
    // (clockwise on screen), ½ left, ¾ up.
    let g = gradient(
        GradientShape::Angular,
        [100.5, 100.5],
        [200.5, 100.5],
        GradientExtend::Clamp,
    );
    assert!(
        close(grey(&g, 100, 150), ease(0.25)),
        "{}",
        grey(&g, 100, 150)
    );
    assert!(close(grey(&g, 50, 100), 0.5));
    assert!(close(grey(&g, 100, 50), ease(0.75)));
    // Just above and below the seam (to the right, far from the centre):
    // the pixel the seam crosses blends both sides.
    let below = grey(&g, 180, 101);
    let above = grey(&g, 180, 99);
    assert!(below < 0.05 && above > 0.95, "{below} {above}");
    let seam = grey(&g, 180, 100);
    assert!((0.3..0.7).contains(&seam), "seam {seam}");
    // Reflect makes it symmetric (no seam).
    let s = gradient(
        GradientShape::Angular,
        [100.5, 100.5],
        [200.5, 100.5],
        GradientExtend::Reflect,
    );
    assert!(close(grey(&s, 180, 101), grey(&s, 180, 99)));
    assert!(close(grey(&s, 50, 100), 1.0));
}

#[test]
fn extend_modes_repeat_reflect_and_empty() {
    // GD20, GD21, GD14 on a 100-pixel drag from 150.5 to 250.5.
    let at = |extend, x| {
        grey(
            &gradient(GradientShape::Linear, [150.5, 0.0], [250.5, 0.0], extend),
            x,
            0,
        )
    };
    // Repeat: the period is the drag; the wrap pixel blends the two sides.
    assert!(close(
        at(GradientExtend::Repeat, 175),
        at(GradientExtend::Repeat, 275)
    ));
    assert!(close(
        at(GradientExtend::Repeat, 175),
        at(GradientExtend::Repeat, 75)
    ));
    let wrap = at(GradientExtend::Repeat, 250);
    assert!(
        (0.45..0.55).contains(&wrap),
        "the wrap pixel is half each side: {wrap}"
    );
    // Reflect: mirrored at both ends, no wrap.
    assert!(close(
        at(GradientExtend::Reflect, 260),
        at(GradientExtend::Reflect, 240)
    ));
    assert!(close(
        at(GradientExtend::Reflect, 140),
        at(GradientExtend::Reflect, 160)
    ));
    assert!(close(at(GradientExtend::Reflect, 350), 0.0));
    // Empty: nothing beyond, and the end pixels half covered (GD14).
    let g = gradient(
        GradientShape::Linear,
        [100.5, 0.0],
        [300.5, 0.0],
        GradientExtend::Empty,
    );
    assert_eq!(g.sample(99, 0).a, 0.0);
    assert!(close(g.sample(100, 0).a, 0.5));
    assert!(close(g.sample(200, 0).a, 1.0));
    assert!(close(g.sample(300, 0).a, 0.5));
    assert_eq!(g.sample(301, 0).a, 0.0);
    // At a sub-pixel start (GD15): 27 % of pixel 181.
    let z = gradient(
        GradientShape::Linear,
        [181.73, 0.0],
        [221.73, 0.0],
        GradientExtend::Empty,
    );
    assert!((z.sample(181, 0).a - 0.27).abs() < 1e-4);
    assert!((z.sample(221, 0).a - 0.73).abs() < 1e-4);
}

#[test]
fn degenerate_drags_draw_nothing() {
    for shape in [
        GradientShape::Linear,
        GradientShape::Radial,
        GradientShape::Angular,
        GradientShape::Reflected,
    ] {
        assert!(geometry(shape, [10.0, 10.0], [10.0, 10.0]).is_degenerate());
        assert!(!geometry(shape, [10.0, 10.0], [10.0, 11.0]).is_degenerate());
    }
    // An ellipse needs both radii (GD18's horizontal circle drag).
    assert!(geometry(GradientShape::Elliptical, [0.0, 0.0], [50.0, 0.0]).is_degenerate());
    assert!(geometry(GradientShape::Linear, [f64::NAN, 0.0], [5.0, 0.0]).is_degenerate());
    assert!(geometry(GradientShape::Linear, [0.0, 0.0], [f64::INFINITY, 0.0]).is_degenerate());
    let g = gradient(
        GradientShape::Linear,
        [10.0, 10.0],
        [10.0, 10.0],
        GradientExtend::Clamp,
    );
    assert!(g.is_empty());
    let mut d = Document::new(64, 64);
    let id = d.active_layer();
    let before = d.digest();
    assert!(
        d.apply_gradient(pixels(id), &g, GradientPaint::Draw)
            .unwrap()
            .is_empty()
    );
    assert_eq!(d.digest(), before);
    assert_eq!(d.history_len(), 0);
}

#[test]
fn render_tile_shortcuts_agree_with_per_pixel_samples() {
    // Uniform tiles (clamped beyond the stops) and tiles the empty extend
    // misses must equal sampling every pixel.
    let cases = [
        gradient(
            GradientShape::Linear,
            [300.5, 40.0],
            [340.5, 90.0],
            GradientExtend::Clamp,
        ),
        gradient(
            GradientShape::Reflected,
            [300.5, 40.0],
            [340.5, 90.0],
            GradientExtend::Clamp,
        ),
        gradient(
            GradientShape::Radial,
            [300.5, 300.5],
            [330.5, 310.5],
            GradientExtend::Clamp,
        ),
        gradient(
            GradientShape::Elliptical,
            [300.5, 300.5],
            [380.5, 320.5],
            GradientExtend::Empty,
        ),
        gradient(
            GradientShape::Linear,
            [300.5, 40.0],
            [340.5, 90.0],
            GradientExtend::Empty,
        ),
    ];
    let mut out = vec![PremulRgba::TRANSPARENT; (TILE_SIZE * TILE_SIZE) as usize];
    for g in &cases {
        for c in IRect::new(-256, -256, 1024, 1024).tiles() {
            g.render_tile(c, &mut out);
            let (ox, oy) = c.origin();
            for (i, p) in out.iter().enumerate().step_by(97) {
                let (x, y) = (ox + (i % 256) as i32, oy + (i / 256) as i32);
                assert_eq!(*p, g.sample(x, y), "{g:?} {c:?} {x} {y}");
            }
        }
    }
}

// --- The document operation ------------------------------------------------

fn linear_bw(extend: GradientExtend, dither: bool) -> Gradient {
    Gradient::new(
        geometry(GradientShape::Linear, [20.5, 0.0], [280.5, 0.0]),
        &bw(),
        Mixing::Perceptual,
        extend,
        dither,
    )
}

#[test]
fn a_gradient_is_one_step_that_undoes_and_redoes_bit_exactly() {
    for depth in PixelDepth::ALL {
        let mut d = Document::with_depth(300, 40, depth);
        let id = d.active_layer();
        paint_rect(&mut d, id, IRect::new(0, 0, 10, 10), [0, 0, 255, 255]);
        let before = d.digest();
        let steps = d.history_len();
        let revision = d.revision();
        let dirty = d
            .apply_gradient(
                pixels(id),
                &linear_bw(GradientExtend::Clamp, true),
                GradientPaint::Draw,
            )
            .unwrap();
        assert_eq!(d.history_len(), steps + 1);
        assert_eq!(d.next_undo(), Some(HistoryAction::Gradient));
        assert_ne!(d.revision(), revision);
        assert_eq!(dirty.len(), 2, "both tile columns of the 300-pixel canvas");
        let after = d.digest();
        d.check_invariants();
        d.undo().unwrap();
        assert_eq!(d.digest(), before);
        assert_eq!(
            d.layer(id).unwrap().pixels().tile_count(),
            1,
            "absent tiles come back absent"
        );
        d.redo().unwrap();
        assert_eq!(d.digest(), after);
        // The same gradient again changes nothing: no step.
        let steps = d.history_len();
        let dirty = d
            .apply_gradient(
                pixels(id),
                &linear_bw(GradientExtend::Clamp, true),
                GradientPaint::Draw,
            )
            .unwrap();
        assert!(dirty.is_empty());
        assert_eq!(d.history_len(), steps);
    }
}

/// The decoded stored tile `c` of `id`'s pixels.
fn stored(d: &Document, id: LayerId, c: TileCoord) -> Vec<PremulRgba> {
    let p = d.layer(id).unwrap().pixels();
    let (ox, oy) = c.origin();
    (0..(TILE_SIZE * TILE_SIZE) as i32)
        .map(|i| p.pixel(ox + i % 256, oy + i / 256))
        .collect()
}

#[test]
fn the_preview_is_the_commit_bit_for_bit() {
    // GR5 on pixels and masks, both depths, with and without dither,
    // alpha lock, selection and every shape.
    let shapes = [
        (GradientShape::Linear, GradientExtend::Clamp),
        (GradientShape::Radial, GradientExtend::Repeat),
        (GradientShape::Elliptical, GradientExtend::Empty),
        (GradientShape::Angular, GradientExtend::Clamp),
        (GradientShape::Reflected, GradientExtend::Reflect),
    ];
    for depth in PixelDepth::ALL {
        for (k, (shape, extend)) in shapes.into_iter().enumerate() {
            for locked in [false, true] {
                let mut d = Document::with_depth(300, 280, depth);
                let id = d.active_layer();
                paint_rect(&mut d, id, IRect::new(30, 30, 200, 120), [40, 100, 30, 160]);
                paint_rect(&mut d, id, IRect::new(10, 150, 290, 260), [200, 10, 0, 255]);
                d.set_alpha_locked(id, locked).unwrap();
                if k % 2 == 1 {
                    let sel = SelectionShape::Rect([15.0, 20.0], [240.0, 250.0]);
                    assert!(d.select(&sel, false, SelectionOp::Replace));
                    assert!(d.feather_selection(6.0));
                }
                let stops = GradientStops::new([
                    GradientStop::new(0.0, Color::new(1.0, 0.2, 0.1), 1.0),
                    GradientStop::new(0.4, Color::new(0.1, 0.9, 0.3), 0.6),
                    GradientStop::new(1.0, Color::WHITE, 0.0),
                ])
                .unwrap();
                let g = Gradient::new(
                    geometry(shape, [120.3, 130.7], [230.1, 190.2]),
                    &stops,
                    Mixing::Perceptual,
                    extend,
                    k % 2 == 0,
                );
                let target = pixels(id);
                let preview: Vec<(TileCoord, Vec<PremulRgba>)> = {
                    let p = d.gradient_preview(target, &g, GradientPaint::Draw).unwrap();
                    p.tiles()
                        .iter()
                        .map(|c| {
                            let mut px = stored(&d, id, c);
                            p.pixels(c, &mut px);
                            (c, px)
                        })
                        .collect()
                };
                d.apply_gradient(target, &g, GradientPaint::Draw).unwrap();
                for (c, px) in preview {
                    let committed = stored(&d, id, c);
                    for (i, (a, b)) in px.iter().zip(&committed).enumerate() {
                        assert_eq!(a, b, "{depth:?} {shape:?} locked {locked} {c:?} pixel {i}");
                    }
                }
                d.check_invariants();
            }
        }
    }
}

#[test]
fn a_mask_preview_is_its_commit_and_ignores_the_colour() {
    for depth in PixelDepth::ALL {
        let mut d = Document::with_depth(260, 100, depth);
        let id = d.active_layer();
        d.add_mask(id).unwrap();
        // Hide everything first, then reveal by a gradient.
        let hide = Gradient::new(
            geometry(GradientShape::Linear, [0.0, 0.0], [1.0, 0.0]),
            &GradientStops::two(Color::BLACK, Color::BLACK),
            Mixing::Standard,
            GradientExtend::Clamp,
            false,
        );
        d.apply_gradient(mask(id), &hide, GradientPaint::Erase)
            .unwrap();
        let reveal = |c: Color| {
            Gradient::new(
                geometry(GradientShape::Linear, [10.5, 0.0], [250.5, 0.0]),
                &GradientStops::to_transparent(c),
                Mixing::Perceptual,
                GradientExtend::Clamp,
                true,
            )
        };
        let g = reveal(Color::BLACK);
        let c = TileCoord::containing(0, 0);
        let previewed = {
            let p = d
                .gradient_preview(mask(id), &g, GradientPaint::Draw)
                .unwrap();
            // What the compositor hands the overlay: the current reveal values.
            let mut values: Vec<f32> = d
                .layer(id)
                .unwrap()
                .mask()
                .unwrap()
                .reveal_tile(c)
                .unwrap()
                .map(|v| f32::from(v) / 255.0)
                .collect();
            p.mask(c, &mut values);
            values
        };
        let mut other = d.clone();
        d.apply_gradient(mask(id), &g, GradientPaint::Draw).unwrap();
        let m = d.layer(id).unwrap().mask().unwrap();
        let committed: Vec<f32> = m
            .reveal_tile(c)
            .unwrap()
            .map(|v| f32::from(v) / 255.0)
            .collect();
        if let Some(i) = (0..previewed.len()).find(|&i| previewed[i] != committed[i]) {
            panic!(
                "{depth:?} pixel {i}: preview {} commit {}",
                previewed[i], committed[i]
            );
        }
        // Reveal follows the gradient's alpha (GD33): ~1 at the start, 0 at the end.
        assert!(
            committed[10] > 0.99 && committed[251] == 0.0,
            "{} {}",
            committed[10],
            committed[251]
        );
        // Any colour reveals the same (GR8).
        other
            .apply_gradient(
                mask(id),
                &reveal(Color::new(0.9, 0.2, 0.4)),
                GradientPaint::Draw,
            )
            .unwrap();
        assert_eq!(other.digest(), d.digest());
    }
}

#[test]
fn the_selection_limits_and_scales_but_does_not_move_the_gradient() {
    let mut d = Document::new(300, 60);
    let id = d.active_layer();
    let sel = SelectionShape::Rect([150.0, 10.0], [250.0, 50.0]);
    assert!(d.select(&sel, false, SelectionOp::Replace));
    let g = linear_bw(GradientExtend::Clamp, false);
    d.apply_gradient(pixels(id), &g, GradientPaint::Draw)
        .unwrap();
    let p = d.layer(id).unwrap().pixels();
    for x in [0, 149, 250, 299] {
        assert_eq!(p.pixel_rgba8(x, 20), [0; 4], "outside the selection: {x}");
    }
    for y in [0, 9, 50] {
        assert_eq!(p.pixel_rgba8(200, y), [0; 4]);
    }
    // Inside: the values of the unselected gradient (GD31).
    let mut free = Document::new(300, 60);
    let fid = free.active_layer();
    free.apply_gradient(pixels(fid), &g, GradientPaint::Draw)
        .unwrap();
    for x in 150..250 {
        assert_eq!(
            p.pixel_rgba8(x, 30),
            free.layer(fid).unwrap().pixels().pixel_rgba8(x, 30)
        );
    }
}

#[test]
fn alpha_lock_keeps_every_alpha_and_erase_removes() {
    for depth in PixelDepth::ALL {
        let mut d = Document::with_depth(300, 40, depth);
        let id = d.active_layer();
        for x in 0..256 {
            paint_rect(
                &mut d,
                id,
                IRect::new(x + 20, 0, x + 21, 20),
                [0, 0, x as u8, x as u8],
            );
        }
        let alphas = |d: &Document| -> Vec<f32> {
            (0..300)
                .flat_map(|x| (0..40).map(move |y| (x, y)))
                .map(|(x, y)| d.layer(id).unwrap().pixels().pixel(x, y).a)
                .collect()
        };
        let before = alphas(&d);
        d.set_alpha_locked(id, true).unwrap();
        d.apply_gradient(
            pixels(id),
            &linear_bw(GradientExtend::Clamp, true),
            GradientPaint::Draw,
        )
        .unwrap();
        assert_eq!(alphas(&d), before, "GR7");
        // Recoloured: the opaque blue end is grey now.
        let p = d.layer(id).unwrap().pixels().pixel(275, 5);
        assert!((p.r - p.b).abs() < 0.01 && p.a == 1.0, "{p:?}");
        // Erase is not stopped by the lock (as the eraser).
        d.apply_gradient(
            pixels(id),
            &linear_bw(GradientExtend::Clamp, false),
            GradientPaint::Erase,
        )
        .unwrap();
        assert_eq!(d.layer(id).unwrap().pixels().pixel(275, 5).a, 0.0);
    }
}

#[test]
fn sixteen_bit_documents_never_pass_through_eight_bits() {
    // GR9: a long dark ramp keeps more than 8-bit levels at 16 bits and
    // stays within half a 16-bit level of the exact value.
    let g = Gradient::new(
        geometry(GradientShape::Linear, [0.5, 0.0], [255.5, 0.0]),
        &GradientStops::two(Color::BLACK, Color::new(0.05, 0.05, 0.05)),
        Mixing::Standard,
        GradientExtend::Clamp,
        false,
    );
    let mut d = Document::with_depth(256, 4, PixelDepth::Rgba16);
    let id = d.active_layer();
    d.apply_gradient(pixels(id), &g, GradientPaint::Draw)
        .unwrap();
    let s = d.layer(id).unwrap().pixels().rgba16().unwrap();
    let mut levels: Vec<u16> = (0..256).map(|x| s.pixel(x, 1)[0]).collect();
    for (x, &v) in levels.iter().enumerate() {
        let exact = 0.05 * f64::from(ease(x as f32 / 255.0)) * 65535.0;
        assert!(
            (f64::from(v) - exact).abs() <= 0.5 + 0.01,
            "{x}: {v} vs {exact}"
        );
    }
    levels.dedup();
    assert!(
        levels.len() > 200,
        "{} distinct 16-bit levels",
        levels.len()
    );
    // At 8 bits the same ramp has at most 14 levels.
    let mut e = Document::new(256, 4);
    let eid = e.active_layer();
    e.apply_gradient(pixels(eid), &g, GradientPaint::Draw)
        .unwrap();
    let mut l8: Vec<u8> = (0..256)
        .map(|x| e.layer(eid).unwrap().pixels().pixel_rgba8(x, 1)[0])
        .collect();
    l8.dedup();
    assert!(l8.len() <= 14);
}

#[test]
fn dithering_is_deterministic_and_keeps_the_mean() {
    let g = linear_bw(GradientExtend::Clamp, true);
    let mut a = Document::new(300, 64);
    let mut b = Document::new(300, 64);
    let (ia, ib) = (a.active_layer(), b.active_layer());
    a.apply_gradient(pixels(ia), &g, GradientPaint::Draw)
        .unwrap();
    b.apply_gradient(pixels(ib), &g, GradientPaint::Draw)
        .unwrap();
    assert_eq!(a.digest(), b.digest(), "GR1");
    let mut plain = Document::new(300, 64);
    let ip = plain.active_layer();
    plain
        .apply_gradient(
            pixels(ip),
            &linear_bw(GradientExtend::Clamp, false),
            GradientPaint::Draw,
        )
        .unwrap();
    // The ordered dither keeps the mean: exactly to a 64th of a level on
    // flat values (tela_color's tests); on a ramp each 8 × 8 block is
    // within a tenth or so and the error has no bias overall.
    let mut bias = 0.0;
    let mut blocks = 0.0;
    for x in (16..280).step_by(8) {
        let block = |f: &dyn Fn(i32, i32) -> f64| -> f64 {
            (0..64).map(|k| f(x + k % 8, 8 + k / 8)).sum::<f64>() / 64.0
        };
        let mean = block(&|x, y| f64::from(a.layer(ia).unwrap().pixels().pixel_rgba8(x, y)[0]));
        let want = block(&|x, _| f64::from(g.sample(x, 0).r) * 255.0);
        assert!((mean - want).abs() <= 0.15, "{x}: {mean} vs {want}");
        bias += mean - want;
        blocks += 1.0;
        let exact = f64::from(g.sample(x, 0).r) * 255.0;
        // Dithered values are one of the two levels around the exact one.
        for y in 0..64 {
            let v = f64::from(a.layer(ia).unwrap().pixels().pixel_rgba8(x, y)[0]);
            assert!((v - exact).abs() < 1.0, "{x} {y}");
        }
        let r = f64::from(plain.layer(ip).unwrap().pixels().pixel_rgba8(x, 5)[0]);
        assert!((r - exact).abs() <= 0.5 + 1e-3);
    }
    assert!((bias / blocks).abs() < 0.01, "bias {}", bias / blocks);
}

#[test]
fn refusals_and_untouched_properties() {
    let g = linear_bw(GradientExtend::Clamp, false);
    let mut d = Document::new(64, 64);
    let id = d.active_layer();
    d.set_visible(id, false).unwrap();
    let before = d.digest();
    assert!(matches!(
        d.apply_gradient(pixels(id), &g, GradientPaint::Draw),
        Err(LayerError::Hidden(e)) if e == id
    ));
    assert!(
        d.gradient_preview(pixels(id), &g, GradientPaint::Draw)
            .is_err()
    );
    d.set_visible(id, true).unwrap();
    d.set_locked(id, true).unwrap();
    assert!(matches!(
        d.apply_gradient(pixels(id), &g, GradientPaint::Draw),
        Err(LayerError::Locked(e)) if e == id
    ));
    d.set_locked(id, false).unwrap();
    assert!(matches!(
        d.apply_gradient(mask(id), &g, GradientPaint::Draw),
        Err(LayerError::NoMask(e)) if e == id
    ));
    d.add_mask(id).unwrap();
    d.set_mask_enabled(id, false).unwrap();
    assert!(matches!(
        d.apply_gradient(mask(id), &g, GradientPaint::Draw),
        Err(LayerError::MaskDisabled(e)) if e == id
    ));
    d.set_mask_enabled(id, true).unwrap();
    // Layer properties are not involved (GD34).
    d.set_opacity(id, 0.5).unwrap();
    d.set_blend_mode(id, BlendMode::Multiply).unwrap();
    let props = |d: &Document| {
        let l = d.layer(id).unwrap();
        (
            l.opacity(),
            l.blend_mode(),
            l.clipped(),
            l.locked(),
            l.visible(),
            l.alpha_locked(),
        )
    };
    let p = props(&d);
    let active = d.active_layer();
    let _ = before;
    d.apply_gradient(pixels(id), &g, GradientPaint::Draw)
        .unwrap();
    assert_eq!(props(&d), p);
    assert_eq!(d.active_layer(), active);
    assert!(!d.has_selection());
}

#[test]
fn empty_extend_and_erase_stay_sparse() {
    let mut d = Document::new(2048, 2048);
    let id = d.active_layer();
    // A small radial gradient that draws nothing beyond its circle.
    let g = gradient(
        GradientShape::Radial,
        [900.5, 900.5],
        [940.5, 900.5],
        GradientExtend::Empty,
    );
    let dirty = d
        .apply_gradient(pixels(id), &g, GradientPaint::Draw)
        .unwrap();
    assert_eq!(dirty.len(), 1, "one tile holds the 40-pixel circle");
    assert_eq!(d.layer(id).unwrap().pixels().tile_count(), 1);
    // Erasing the whole canvas only visits that tile.
    let all = linear_bw(GradientExtend::Clamp, false);
    let dirty = d
        .apply_gradient(pixels(id), &all, GradientPaint::Erase)
        .unwrap();
    assert!(dirty.len() <= 1);
    assert!(d.layer(id).unwrap().pixels().tile_count() <= 1);
}
