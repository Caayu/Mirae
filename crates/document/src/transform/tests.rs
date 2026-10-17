use tela_canvas::IRect;

use super::*;

fn close(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
}

#[test]
fn the_matrix_keeps_the_pivot_and_turns_clockwise_on_screen() {
    let mut p = TransformParams::identity([10.0, 20.0]);
    assert!(close(p.matrix().apply([3.0, 4.0]), [3.0, 4.0]));
    p.scale = [2.0, 3.0];
    p.rotation = std::f64::consts::FRAC_PI_2;
    let m = p.matrix();
    assert!(
        close(m.apply([10.0, 20.0]), [10.0, 20.0]),
        "the pivot stays"
    );
    // y points down: +x turns to +y (clockwise on screen), scaled by 2.
    assert!(close(m.apply([11.0, 20.0]), [10.0, 22.0]));
    p.translate = [5.0, -1.0];
    assert!(close(p.matrix().apply([10.0, 20.0]), [15.0, 19.0]));
}

#[test]
fn inverse_and_composition_round_trip() {
    let p = TransformParams {
        translate: [3.5, -7.25],
        scale: [0.7, -1.9],
        rotation: 0.4,
        pivot: [100.0, 50.0],
    };
    let m = p.matrix();
    let inv = m.inverse().unwrap();
    for q in [[0.0, 0.0], [123.4, -56.7], [1e4, 3e3]] {
        assert!(close(inv.apply(m.apply(q)), q));
        assert!(close(inv.after(&m).apply(q), q));
    }
    let singular = Affine {
        a: 1.0,
        b: 2.0,
        c: 2.0,
        d: 4.0,
        e: 0.0,
        f: 0.0,
    };
    assert!(singular.inverse().is_none());
}

#[test]
fn parameters_must_be_finite_and_scales_bounded() {
    let ok = TransformParams::identity([0.0; 2]);
    assert!(ok.is_valid());
    for bad in [
        TransformParams {
            scale: [0.0, 1.0],
            ..ok
        },
        TransformParams {
            scale: [1.0, MAX_SCALE * 2.0],
            ..ok
        },
        TransformParams {
            rotation: f64::NAN,
            ..ok
        },
        TransformParams {
            translate: [f64::INFINITY, 0.0],
            ..ok
        },
        TransformParams {
            pivot: [0.0, f64::NEG_INFINITY],
            ..ok
        },
    ] {
        assert!(!bad.is_valid(), "{bad:?}");
        assert!(Geometry::new(IRect::new(0, 0, 10, 10), &bad).is_none());
    }
    // Flips are negative scales, and valid.
    assert!(
        TransformParams {
            scale: [-1.0, -MIN_SCALE],
            ..ok
        }
        .is_valid()
    );
}

#[test]
fn the_cubic_is_keys_with_a_minus_one_and_sums_to_one() {
    assert_eq!(keys(0.0), 1.0);
    assert_eq!(keys(1.0), 0.0);
    assert_eq!(keys(2.0), 0.0);
    assert_eq!(keys(-1.5), keys(1.5));
    // a = −1: k(1.5) = −1.5³ + 5·1.5² − 8·1.5 + 4.
    assert!((keys(1.5) - (-3.375 + 11.25 - 12.0 + 4.0)).abs() < 1e-6);
    for k in 0..=20 {
        let t = k as f32 / 20.0;
        let (_, w) = taps(3.0 + t);
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-6, "{t}");
    }
    // At a pixel centre the cubic picks that pixel alone.
    let (i0, w) = taps(5.5);
    assert_eq!((i0, w), (4, [0.0, 1.0, 0.0, 0.0]));
}

#[test]
fn axis_aligned_results_round_their_edges_to_whole_pixels() {
    // T17: 30 % of 168..232 about 200 is 190.4..209.6, rounded 190..210.
    let b = IRect::new(168, 118, 232, 182);
    let mut p = TransformParams::identity([200.0, 150.0]);
    p.scale = [0.3, 0.3];
    let g = Geometry::new(b, &p).unwrap();
    assert!(close(g.corners[0], [190.0, 140.0]) && close(g.corners[2], [210.0, 160.0]));
    assert_eq!(g.level, 1, "0.3125 is at most ½: one halving");
    // 33 %: 189.44..210.56 → 189..211 (half up).
    p.scale = [0.33, 0.33];
    let g = Geometry::new(b, &p).unwrap();
    assert!(close(g.corners[0], [189.0, 139.0]) && close(g.corners[2], [211.0, 161.0]));
    // A translation by half a pixel moves by a whole one (T8) and copies.
    let mut t = TransformParams::identity([200.0, 150.0]);
    t.translate = [0.5, -0.5];
    let g = Geometry::new(b, &t).unwrap();
    assert!(close(g.corners[0], [169.0, 118.0]));
    assert!(g.copy && g.axis_aligned);
    // A quarter turn is axis-aligned too; a flip keeps the box.
    t = TransformParams::identity([200.0, 150.0]);
    t.rotation = std::f64::consts::FRAC_PI_2;
    assert!(Geometry::new(b, &t).unwrap().copy);
    t.rotation = 0.0;
    t.scale = [-1.0, 1.0];
    let g = Geometry::new(b, &t).unwrap();
    assert!(g.copy && close(g.corners[1], [168.0, 118.0]));
}

#[test]
fn the_pyramid_halves_while_both_scales_are_at_most_a_half() {
    let b = IRect::new(0, 0, 64, 64);
    let level = |sx, sy| {
        let mut p = TransformParams::identity([32.0, 32.0]);
        p.scale = [sx, sy];
        Geometry::new(b, &p).unwrap().level
    };
    assert_eq!(level(1.0, 1.0), 0);
    assert_eq!(level(0.5, 0.5), 1);
    assert_eq!(level(0.25, 0.25), 2);
    assert_eq!(level(0.2, 0.2), 2);
    assert_eq!(level(0.2, 0.6), 0, "both axes must be reduced");
}

#[test]
fn rotated_sides_are_anti_aliased_by_distance() {
    // T21: along a row crossing a side at 15°, coverage steps by sin 15°.
    let b = IRect::new(0, 0, 100, 100);
    let mut p = TransformParams::identity([50.0, 50.0]);
    p.rotation = 15f64.to_radians();
    let g = Geometry::new(b, &p).unwrap();
    assert!(!g.axis_aligned);
    let centre = g.edge_coverage([50.5, 50.5], false, b);
    assert_eq!(centre, 1.0);
    let far = g.edge_coverage([500.5, 50.5], false, b);
    assert_eq!(far, 0.0);
    // Walk along the top side's normal direction: d grows by 1 per px.
    let (n, _) = g.sides[0];
    let mid = [
        (g.corners[0][0] + g.corners[1][0]) / 2.0,
        (g.corners[0][1] + g.corners[1][1]) / 2.0,
    ];
    let at = |d: f64| [mid[0] + n[0] * d, mid[1] + n[1] * d];
    for (d, want) in [(-0.5, 0.0), (0.0, 0.5), (0.25, 0.75), (0.5, 1.0)] {
        let c = g.edge_coverage(at(d), false, b);
        assert!((f64::from(c) - want).abs() < 1e-6, "{d}: {c}");
    }
    // Nearest neighbour keeps hard edges.
    assert!(matches!(g.edge_coverage(at(0.1), true, b), 0.0 | 1.0));
}

/// A source made of `w × h` premultiplied pixels at the origin.
fn source(w: i32, h: i32, px: impl Fn(i32, i32) -> [f32; 4]) -> TransformSource {
    let mut v = Vec::new();
    for y in 0..h {
        for x in 0..w {
            v.push(px(x, y));
        }
    }
    let d = crate::Document::new(8, 8);
    TransformSource::new(
        d.active_layer(),
        PixelDepth::Rgba8,
        (d.revision(), 0),
        IRect::new(0, 0, w, h),
        v,
        None,
        None,
    )
}

/// The transformed content, as a map from destination pixel to sample.
fn rendered(s: &TransformSource, t: &Transform) -> Vec<((i32, i32), [f32; 4])> {
    let mut out = Vec::new();
    let g = Geometry::new(s.bounds(), &t.params).unwrap();
    for c in g.dest.tiles() {
        let (ox, oy) = c.origin();
        s.render_tile(t, c, |i, smp| {
            let (x, y) = (ox + (i % 256) as i32, oy + (i / 256) as i32);
            out.push(((x, y), smp.px));
        });
    }
    out
}

#[test]
fn premultiplied_resampling_leaves_no_fringe() {
    // An opaque red pixel block beside transparency: scaled and rotated,
    // no pixel gains another hue, and colour never exceeds alpha (T25).
    let s = source(12, 12, |x, y| {
        if (4..8).contains(&x) && (4..8).contains(&y) {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0; 4]
        }
    });
    for (scale, degrees) in [
        (2.0, 0.0),
        (0.4, 0.0),
        (1.0, 15.0),
        (1.7, 33.0),
        (0.3, 45.0),
    ] {
        for interpolation in [Interpolation::Bicubic, Interpolation::Nearest] {
            let mut p = TransformParams::identity([6.0, 6.0]);
            p.scale = [scale, scale];
            p.rotation = f64::to_radians(degrees);
            let t = Transform {
                params: p,
                interpolation,
            };
            for (at, [r, g, b, a]) in rendered(&s, &t) {
                assert!(g == 0.0 && b == 0.0, "{at:?}: {r} {g} {b} {a}");
                assert!(r <= a + 1e-6 && (0.0..=1.0).contains(&a), "{at:?}: {r} {a}");
                if a > 0.0 {
                    assert!((r / a - 1.0).abs() < 1e-5, "straight colour stays red");
                }
            }
        }
    }
}

#[test]
fn a_full_turn_and_double_flips_restore_the_content() {
    let s = source(20, 16, |x, y| {
        let v = ((x * 7 + y * 13) % 17) as f32 / 16.0;
        [v * 0.5, v * 0.25, v, v]
    });
    let original = rendered(
        &s,
        &Transform {
            params: TransformParams::identity([10.0, 8.0]),
            interpolation: Interpolation::Bicubic,
        },
    );
    let mut turn = TransformParams::identity([10.0, 8.0]);
    turn.rotation = std::f64::consts::TAU;
    let mut flip = TransformParams::identity([10.0, 8.0]);
    flip.scale = [-1.0, -1.0];
    for p in [turn, flip] {
        // A full turn is a quarter-turn multiple: axis-aligned, copied.
        let g = Geometry::new(s.bounds(), &p).unwrap();
        assert!(g.copy);
    }
    let twice = |p: TransformParams| {
        // Flipping twice is the identity map.
        let m = p.matrix().after(&p.matrix());
        [m.a, m.b, m.c, m.d, m.e, m.f]
    };
    let id = twice(flip);
    assert!(close([id[0], id[3]], [1.0, 1.0]) && close([id[4], id[5]], [0.0, 0.0]));
    let turned = rendered(
        &s,
        &Transform {
            params: turn,
            interpolation: Interpolation::Bicubic,
        },
    );
    assert_eq!(turned, original);
    // A flip mirrors exactly.
    let flipped = rendered(
        &s,
        &Transform {
            params: flip,
            interpolation: Interpolation::Bicubic,
        },
    );
    for ((x, y), px) in &flipped {
        let (_, want) = original
            .iter()
            .find(|((ox, oy), _)| *ox == 19 - x && *oy == 15 - y)
            .unwrap();
        assert_eq!(px, want);
    }
}
