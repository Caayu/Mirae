//! End-to-end document scenarios (phase 4G): real brush strokes, layers,
//! history, blend modes, masks, clipping, alpha lock, both depths and
//! `.mirae` files, combined the way a drawing session combines them.
//! Every stroke also checks preview == commit (P2/P3) and every save/load
//! checks content, composite and byte determinism (IO1, IO2).

mod support;

use support::*;
use tela_canvas::TileCoord;
use tela_color::{Color, PixelDepth};
use tela_document::{BlendMode, Document, LayerPart, Placement};

/// Paints and asserts preview == commit within 1/255.
fn paint(
    d: &mut Document,
    preset: &tela_brush::BrushPreset,
    c: Color,
    path: &[(f32, f32, f32)],
    seed: u64,
) {
    let done = stroke(d, preset, c, path, seed).expect("the target takes paint");
    assert!(
        done.preview_vs_commit <= 1,
        "preview == commit: {}",
        done.preview_vs_commit
    );
}

/// Applies `f` as one history step, remembering the content before it.
fn step(d: &mut Document, walk: &mut Vec<u64>, f: impl FnOnce(&mut Document)) {
    let before = history_digest(d);
    let len = d.history_len();
    f(d);
    assert_eq!(d.history_len(), len + 1, "one step");
    walk.push(before);
}

/// Undoes every step, checking each intermediate content, then redoes
/// them all and checks the final content is back.
fn walk_back_and_forth(d: &mut Document, walk: &[u64]) {
    let end = history_digest(d);
    let end_picture = composite_digest(d);
    for before in walk.iter().rev() {
        d.undo().expect("undoable");
        assert_eq!(
            history_digest(d),
            *before,
            "undo restores the earlier state"
        );
        check(d);
    }
    for _ in walk {
        d.redo().expect("redoable");
    }
    assert_eq!(history_digest(d), end, "redo restores the end state");
    assert_eq!(composite_digest(d), end_picture);
}

#[test]
fn scenario_a_basic_raster_document() {
    let mut d = Document::new(700, 500);
    let mut walk = Vec::new();
    let ink = preset(tela_brush::preset::builtin::ids::INK_PEN);
    let l1 = d.active_layer();
    step(&mut d, &mut walk, |d| {
        paint(
            d,
            &ink,
            colour(4),
            &line((40.0, 40.0), (650.0, 450.0), 80),
            1,
        )
    });
    step(&mut d, &mut walk, |d| {
        d.add_layer(Placement::Top).unwrap();
    });
    let l2 = d.active_layer();
    step(&mut d, &mut walk, |d| {
        paint(
            d,
            &round(40.0, 0.8),
            colour(0),
            &line((600.0, 60.0), (80.0, 420.0), 60),
            2,
        )
    });
    step(&mut d, &mut walk, |d| {
        d.set_opacity(l2, 0.55).unwrap();
    });
    step(&mut d, &mut walk, |d| {
        d.add_layer(Placement::Bottom).unwrap();
    });
    let l3 = d.active_layer();
    step(&mut d, &mut walk, |d| {
        paint(
            d,
            &round(90.0, 1.0),
            colour(2),
            &line((300.0, 30.0), (320.0, 480.0), 40),
            3,
        )
    });
    step(&mut d, &mut walk, |d| {
        d.move_layer(l3, Placement::Above(l2)).unwrap();
    });
    step(&mut d, &mut walk, |d| {
        d.set_name(l1, "Lines").unwrap();
    });
    check(&d);
    let order: Vec<_> = d.layers().map(|l| l.id()).collect();
    assert_eq!(order, [l1, l2, l3]);
    walk_back_and_forth(&mut d, &walk);
    // Visibility is not a step but is saved.
    d.set_visible(l2, false).unwrap();
    let back = round_trip(&d);
    assert!(!back.layer(l2).unwrap().visible());
    assert_eq!(back.layer(l1).unwrap().name(), "Lines");
}

#[test]
fn scenario_b_advanced_composition_survives_save_and_load() {
    for depth in PixelDepth::ALL {
        let mut d = Document::with_depth(600, 400, depth);
        let base = d.active_layer();
        paint(
            &mut d,
            &round(120.0, 1.0),
            colour(1),
            &line((60.0, 200.0), (540.0, 200.0), 50),
            10,
        );
        let modes = [
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::SoftLight,
            BlendMode::Difference,
            BlendMode::Color,
            BlendMode::Add,
        ];
        let mut walk = Vec::new();
        for (i, mode) in modes.into_iter().enumerate() {
            step(&mut d, &mut walk, |d| {
                d.add_layer(Placement::Top).unwrap();
            });
            let l = d.active_layer();
            // Partial alpha: a soft, translucent brush.
            let soft = preset(tela_brush::preset::builtin::ids::SOFT_ROUND);
            let y = 40.0 + 45.0 * i as f32;
            step(&mut d, &mut walk, |d| {
                paint(
                    d,
                    &soft,
                    colour(i),
                    &line((30.0, y), (570.0, 380.0 - y), 40),
                    20 + i as u64,
                );
            });
            step(&mut d, &mut walk, |d| {
                d.set_blend_mode(l, mode).unwrap();
            });
            step(&mut d, &mut walk, |d| {
                d.set_opacity(l, 0.35 + 0.08 * i as f32).unwrap();
            });
            // The opacity chain must not swallow the next layer's edit.
            d.set_active_layer(base).unwrap();
        }
        // Blend tiles are composed over the paper (ADR 0016).
        assert!(d.paper_backdrop(TileCoord { x: 0, y: 0 }));
        check(&d);
        walk_back_and_forth(&mut d, &walk);
        let before = composite_digest(&d);
        let back = round_trip(&d);
        assert_eq!(composite_digest(&back), before, "{depth:?}");
    }
}

#[test]
fn scenario_c_masks_paint_erase_enable_undo_and_files() {
    let mut d = Document::new(640, 480);
    let l = d.active_layer();
    paint(
        &mut d,
        &round(150.0, 1.0),
        colour(0),
        &line((50.0, 240.0), (590.0, 240.0), 50),
        30,
    );
    let no_mask = composite_digest(&d);
    let mut walk = Vec::new();
    step(&mut d, &mut walk, |d| {
        d.add_mask(l).unwrap();
    });
    assert_eq!(
        d.active_part(),
        LayerPart::Mask,
        "a new mask is the target (M1)"
    );
    assert_eq!(
        composite_digest(&d),
        no_mask,
        "a new mask changes nothing (M1)"
    );
    // Erasing on the mask hides, painting reveals again partially.
    step(&mut d, &mut walk, |d| {
        paint(
            d,
            &eraser(80.0),
            colour(5),
            &line((100.0, 100.0), (540.0, 380.0), 50),
            31,
        )
    });
    let hidden = composite_digest(&d);
    assert_ne!(hidden, no_mask);
    step(&mut d, &mut walk, |d| {
        paint(
            d,
            &round(40.0, 0.6),
            colour(3),
            &line((300.0, 100.0), (320.0, 380.0), 30),
            32,
        )
    });
    // Disabled mask == no mask (M2); enabling brings it back.
    step(&mut d, &mut walk, |d| {
        d.set_mask_enabled(l, false).unwrap();
    });
    assert_eq!(composite_digest(&d), no_mask, "M2");
    assert!(
        stroke(
            &mut d,
            &round(10.0, 1.0),
            colour(0),
            &line((10.0, 10.0), (60.0, 60.0), 5),
            33
        )
        .is_none(),
        "disabled masks take no paint"
    );
    step(&mut d, &mut walk, |d| {
        d.set_mask_enabled(l, true).unwrap();
    });
    check(&d);
    walk_back_and_forth(&mut d, &walk);
    let shown = composite_digest(&d);
    let back = round_trip(&d);
    assert_eq!(composite_digest(&back), shown);
    assert_eq!(
        back.active_part(),
        d.active_part(),
        "the edit target is kept"
    );
}

#[test]
fn scenario_d_clipping_groups_with_opacity_modes_and_masks() {
    let mut d = Document::new(600, 400);
    let base = d.active_layer();
    let soft = preset(tela_brush::preset::builtin::ids::SOFT_ROUND);
    paint(
        &mut d,
        &round(140.0, 0.9),
        colour(1),
        &line((100.0, 200.0), (500.0, 200.0), 40),
        40,
    );
    let mut walk = Vec::new();
    let mut clipped = Vec::new();
    for (i, mode) in [BlendMode::Darken, BlendMode::Multiply, BlendMode::Screen]
        .into_iter()
        .enumerate()
    {
        step(&mut d, &mut walk, |d| {
            d.add_layer(Placement::Top).unwrap();
        });
        let l = d.active_layer();
        let path = line((20.0, 50.0 + 100.0 * i as f32), (580.0, 350.0), 40);
        step(&mut d, &mut walk, |d| {
            paint(d, &soft, colour(i + 2), &path, 41 + i as u64)
        });
        step(&mut d, &mut walk, |d| {
            d.set_clipped(l, true).unwrap();
        });
        step(&mut d, &mut walk, |d| {
            d.set_blend_mode(l, mode).unwrap();
        });
        d.set_active_layer(base).unwrap();
        clipped.push(l);
    }
    let last = *clipped.last().unwrap();
    step(&mut d, &mut walk, |d| {
        d.set_opacity(last, 0.6).unwrap();
    });
    step(&mut d, &mut walk, |d| {
        d.add_mask(clipped[0]).unwrap();
    });
    d.set_active_layer(clipped[0]).unwrap();
    d.set_active_part(LayerPart::Mask).unwrap();
    step(&mut d, &mut walk, |d| {
        paint(
            d,
            &eraser(60.0),
            colour(5),
            &line((300.0, 20.0), (300.0, 380.0), 30),
            45,
        )
    });
    // CL1: clipped content never changes where the group has alpha.
    let alpha = |d: &Document| -> Vec<u8> {
        tiles_of_interest(d)
            .into_iter()
            .flat_map(|c| compose(d, c, None).unwrap_or_else(|| vec![[0; 4]; 65_536]))
            .map(|p| p[3])
            .collect()
    };
    let with = alpha(&d);
    let mut bare = d.clone();
    for &l in &clipped {
        bare.set_visible(l, false).unwrap();
    }
    assert_eq!(alpha(&bare), with, "CL1: coverage is the base's");
    check(&d);
    walk_back_and_forth(&mut d, &walk);
    let picture = composite_digest(&d);
    let back = round_trip(&d);
    assert!(back.layers().skip(1).all(|l| l.clipped()));
    assert_eq!(composite_digest(&back), picture);
}

#[test]
fn scenario_e_alpha_lock_on_anti_aliased_edges_at_both_depths() {
    for depth in PixelDepth::ALL {
        let mut d = Document::with_depth(512, 512, depth);
        let l = d.active_layer();
        let soft = preset(tela_brush::preset::builtin::ids::SOFT_ROUND);
        paint(
            &mut d,
            &soft,
            colour(1),
            &line((60.0, 256.0), (450.0, 256.0), 40),
            50,
        );
        let alphas = |d: &Document| -> Vec<f32> {
            (0..512)
                .flat_map(|y| (0..512).map(move |x| (x, y)))
                .step_by(7)
                .map(|(x, y)| d.layer(l).unwrap().pixels().pixel(x, y).a)
                .collect()
        };
        let before = alphas(&d);
        assert!(
            before.iter().any(|&a| a > 0.0 && a < 1.0),
            "anti-aliased edges"
        );
        let mut walk = Vec::new();
        step(&mut d, &mut walk, |d| {
            d.set_alpha_locked(l, true).unwrap();
        });
        // Paint over the whole layer: colour changes, alpha does not (A1),
        // nothing appears where alpha was 0 (A2).
        step(&mut d, &mut walk, |d| {
            paint(
                d,
                &round(200.0, 1.0),
                colour(0),
                &line((0.0, 256.0), (512.0, 256.0), 40),
                51,
            )
        });
        assert_eq!(alphas(&d), before, "A1, A2 ({depth:?})");
        let red = d.layer(l).unwrap().pixels().pixel(256, 256);
        assert!(red.r > red.b, "the colour changed");
        // The eraser still erases (A3).
        step(&mut d, &mut walk, |d| {
            paint(
                d,
                &eraser(60.0),
                colour(0),
                &line((200.0, 200.0), (300.0, 300.0), 20),
                52,
            )
        });
        assert!(d.layer(l).unwrap().pixels().pixel(250, 250).a < 0.5, "A3");
        check(&d);
        walk_back_and_forth(&mut d, &walk);
        let back = round_trip(&d);
        assert!(back.layer(l).unwrap().alpha_locked());
    }
}

#[test]
fn scenario_f_16_bit_faint_strokes_airbrush_mask_clip_blend_and_conversion() {
    let mut d = Document::with_depth(600, 400, PixelDepth::Rgba16);
    let base = d.active_layer();
    // Glazing: 40 faint strokes (the case 16 bits is for, ADR 0018).
    for i in 0..40 {
        let y = 150.0 + (i % 5) as f32 * 3.0;
        paint(
            &mut d,
            &round(160.0, 0.02),
            colour(4),
            &line((80.0, y), (520.0, y + 60.0), 30),
            60 + i,
        );
    }
    let between_levels = |d: &Document| {
        let s = d.layer(base).unwrap().pixels().rgba16().unwrap();
        s.tile_coords()
            .flat_map(|c| s.tile(c).unwrap().pixels().to_vec())
            .any(|p| p.iter().any(|v| v % 257 != 0))
    };
    assert!(between_levels(&d), "16-bit values between 8-bit levels");
    d.add_layer(Placement::Top).unwrap();
    let air = d.active_layer();
    paint(
        &mut d,
        &airbrush(),
        colour(0),
        &line((100.0, 100.0), (500.0, 300.0), 60),
        70,
    );
    d.set_clipped(air, true).unwrap();
    d.set_blend_mode(air, BlendMode::Multiply).unwrap();
    d.add_mask(air).unwrap();
    paint(
        &mut d,
        &eraser(50.0),
        colour(5),
        &line((300.0, 50.0), (300.0, 350.0), 30),
        71,
    );
    check(&d);
    let sixteen = content_digest(&d);
    let picture = composite_digest(&d);
    // The file keeps the 16 bits.
    let back = round_trip(&d);
    assert!(between_levels(&back));
    // 16 → 8 rounds (one step, undoable exactly); 8 → 16 is exact.
    d.convert_depth(PixelDepth::Rgba8);
    assert_eq!(d.depth(), PixelDepth::Rgba8);
    check(&d);
    let eight = content_digest(&d);
    let eight_picture = composite_digest(&d);
    round_trip(&d);
    d.convert_depth(PixelDepth::Rgba16);
    assert_eq!(
        composite_digest(&d),
        eight_picture,
        "8 → 16 composes identically (DP2)"
    );
    assert!(!between_levels(&d), "widened content is on 8-bit levels");
    d.undo().unwrap();
    assert_eq!(content_digest(&d), eight);
    d.undo().unwrap();
    assert_eq!(
        content_digest(&d),
        sixteen,
        "undo brings the 16 bits back (DP4)"
    );
    assert_eq!(composite_digest(&d), picture);
}

#[test]
fn scenario_g_full_file_workflow_with_save_points() {
    let dir = std::env::temp_dir().join(format!("mirae-g-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (first, second) = (dir.join("draft.mirae"), dir.join("final.mirae"));
    let save = |d: &Document, p: &std::path::Path| {
        tela_io::save_document(d, p, &tela_io::SaveOptions::default()).unwrap();
    };
    let load =
        |p: &std::path::Path| tela_io::load_document(p, &tela_io::Limits::default()).unwrap();

    let mut d = Document::new(500, 400);
    let clean = d.revision();
    paint(
        &mut d,
        &round(30.0, 1.0),
        colour(4),
        &line((50.0, 50.0), (450.0, 350.0), 40),
        80,
    );
    d.add_layer(Placement::Top).unwrap();
    paint(
        &mut d,
        &round(60.0, 0.5),
        colour(0),
        &line((450.0, 50.0), (50.0, 350.0), 40),
        81,
    );
    assert_ne!(d.revision(), clean);
    save(&d, &first);
    let saved = d.revision();
    let at_first_save = digest(&d);
    // Edit again; undo back to the save point is clean (D-4F-2), redo is
    // modified again.
    paint(
        &mut d,
        &round(20.0, 1.0),
        colour(2),
        &line((250.0, 20.0), (250.0, 380.0), 30),
        82,
    );
    let l = d.active_layer();
    d.set_opacity(l, 0.3).unwrap();
    assert_ne!(d.revision(), saved);
    d.undo().unwrap();
    d.undo().unwrap();
    assert_eq!(d.revision(), saved, "undo to the save point is clean");
    assert_eq!(content_digest(&d), {
        let mut h = load(&first).document;
        h.set_active_layer(d.active_layer()).unwrap();
        content_digest(&h)
    });
    d.redo().unwrap();
    assert_ne!(d.revision(), saved);
    d.redo().unwrap();
    // Save As.
    save(&d, &second);
    let at_second_save = digest(&d);
    drop(d);
    // Close, open both.
    let a = load(&first);
    assert!(a.warnings.is_empty());
    assert_eq!(digest(&a.document), at_first_save);
    let b = load(&second);
    assert_eq!(digest(&b.document), at_second_save);
    // Saving the opened document over its own file gives the same bytes.
    let bytes = std::fs::read(&second).unwrap();
    save(&b.document, &second);
    assert_eq!(std::fs::read(&second).unwrap(), bytes);
    std::fs::remove_dir_all(&dir).unwrap();
}
