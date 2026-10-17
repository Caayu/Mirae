use tela_canvas::{IRect, TileCoord, TileCoverage};

use super::*;

const CANVAS: IRect = IRect::new(0, 0, 700, 500);

fn shape(s: &SelectionShape, aa: bool) -> Selection {
    Selection::from_coverage(CANVAS, rasterize(s, aa, CANVAS))
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> SelectionShape {
    SelectionShape::Rect([x0, y0], [x1, y1])
}

fn area(s: &Selection) -> f64 {
    let mut sum = 0.0;
    for y in CANVAS.y0..CANVAS.y1 {
        for x in CANVAS.x0..CANVAS.x1 {
            sum += f64::from(s.coverage(x, y)) / 255.0;
        }
    }
    sum
}

#[test]
fn operators_follow_the_measured_8_bit_rules() {
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            assert_eq!(SelectionOp::Replace.combine(a, b), b);
            assert_eq!(
                SelectionOp::Add.combine(a, b),
                (u16::from(a) + u16::from(b)).min(255) as u8
            );
            assert_eq!(SelectionOp::Subtract.combine(a, b), a.saturating_sub(b));
            let product = (f64::from(a) * f64::from(b) / 255.0).floor() as u8;
            assert_eq!(SelectionOp::Intersect.combine(a, b), product);
        }
        // Identities the property tests rely on.
        assert_eq!(SelectionOp::Intersect.combine(a, 255), a);
        assert_eq!(SelectionOp::Intersect.combine(a, 0), 0);
        assert_eq!(SelectionOp::Subtract.combine(a, 0), a);
        assert_eq!(SelectionOp::Add.combine(a, 0), a);
    }
}

#[test]
fn rectangles_snap_to_whole_pixels_and_are_never_partial() {
    // Reference S7, S8: corners round to the nearest pixel corner (half
    // down), anti-aliasing or not.
    for aa in [false, true] {
        let s = shape(&rect(50.25, 40.25, 150.5, 120.75), aa);
        assert_eq!(s.bounds(), IRect::new(50, 40, 150, 121));
        assert_eq!(area(&s), 100.0 * 81.0);
        // Any corner order.
        assert_eq!(shape(&rect(150.5, 120.75, 50.25, 40.25), aa), s);
    }
}

#[test]
fn shapes_are_clipped_to_the_canvas() {
    // Reference S12.
    let s = shape(&rect(-40.0, -30.0, 100.0, 90.0), true);
    assert_eq!(s.bounds(), IRect::new(0, 0, 100, 90));
    assert_eq!(s.coverage(-1, 10), 0);
    let off = shape(&rect(-50.0, -50.0, -10.0, -10.0), true);
    assert!(off.is_empty());
    s.check();
}

#[test]
fn anti_aliased_ellipses_have_the_analytic_area_and_a_one_pixel_edge() {
    let e = SelectionShape::Ellipse {
        center: [120.5, 90.5],
        radii: [60.5, 40.5],
        rotation: 0.0,
    };
    let aa = shape(&e, true);
    let exact = std::f64::consts::PI * 60.5 * 40.5;
    assert!((area(&aa) - exact).abs() < 2.0, "{} vs {exact}", area(&aa));
    assert_eq!(aa.bounds(), IRect::new(60, 50, 181, 131));
    // Partial pixels form a one-pixel band (about the perimeter).
    let partial = (50..131)
        .flat_map(|y| (60..181).map(move |x| (x, y)))
        .filter(|&(x, y)| (1..255).contains(&aa.coverage(x, y)))
        .count();
    assert!((300..450).contains(&partial), "{partial}");
    // Hard: only full or empty pixels, centre sampling.
    let hard = shape(&e, false);
    assert!((50..131).all(|y| (60..181).all(|x| matches!(hard.coverage(x, y), 0 | 255))));
    assert!((area(&hard) - exact).abs() < 40.0);
}

#[test]
fn rotated_ellipses_and_polygons_rasterize_like_their_unrotated_selves() {
    let e = |rotation| SelectionShape::Ellipse {
        center: [300.0, 250.0],
        radii: [120.0, 60.0],
        rotation,
    };
    let (a, b) = (
        shape(&e(0.0), true),
        shape(&e(std::f32::consts::FRAC_PI_2), true),
    );
    assert!((area(&a) - area(&b)).abs() < 1.0);
    assert_eq!(b.bounds(), IRect::new(240, 130, 360, 370));
}

#[test]
fn self_intersecting_lassos_fill_even_odd() {
    // Reference S11: the pentagram's centre is not selected.
    let points: Vec<[f32; 2]> = (0..5)
        .map(|k| {
            let a = -std::f32::consts::FRAC_PI_2 + k as f32 * 4.0 * std::f32::consts::PI / 5.0;
            [200.0 + 100.0 * a.cos(), 150.0 + 100.0 * a.sin()]
        })
        .collect();
    let star = shape(&SelectionShape::Polygon(points), true);
    assert_eq!(star.coverage(200, 150), 0, "centre");
    assert_eq!(star.coverage(200, 70), 255, "a point");
    assert_eq!(star.coverage(200, 30), 0, "outside");
}

#[test]
fn degenerate_and_garbage_shapes_select_nothing() {
    let nan = f32::NAN;
    for s in [
        rect(10.0, 10.0, 10.0, 80.0),
        rect(nan, 0.0, 5.0, 5.0),
        SelectionShape::Polygon(vec![]),
        SelectionShape::Polygon(vec![[1.0, 1.0], [50.0, 50.0]]),
        SelectionShape::Polygon(vec![[1.0, 1.0], [50.0, 50.0], [99.0, 99.0]]),
        SelectionShape::Polygon(vec![[nan, nan], [f32::INFINITY, 3.0], [0.0, 1.0]]),
        SelectionShape::Ellipse {
            center: [10.0, 10.0],
            radii: [0.0, 0.0],
            rotation: 0.0,
        },
        SelectionShape::Ellipse {
            center: [nan, 10.0],
            radii: [5.0, 5.0],
            rotation: 0.0,
        },
    ] {
        assert!(shape(&s, true).is_empty(), "{s:?}");
    }
    // Huge coordinates are clipped, not overflowed.
    let huge = shape(&rect(-1e30, -1e30, 1e30, 1e30), true);
    assert_eq!(huge.bounds(), CANVAS);
    let tiny = shape(
        &SelectionShape::Polygon(vec![[5.0, 5.0], [5.2, 5.0], [5.1, 5.3]]),
        true,
    );
    assert!(area(&tiny) < 0.1);
}

#[test]
fn select_all_and_invert_store_no_tile() {
    let mut all = Selection::all(CANVAS);
    assert_eq!(all.stored_tile_count(), 0);
    assert_eq!(all.bounds(), CANVAS);
    assert!(matches!(
        all.tile(TileCoord { x: 0, y: 0 }),
        TileCoverage::Full
    ));
    // A border tile is partial: the canvas ends inside it.
    assert!(matches!(
        all.tile(TileCoord { x: 2, y: 1 }),
        TileCoverage::Partial { .. }
    ));
    assert!(matches!(
        all.tile(TileCoord { x: 9, y: 9 }),
        TileCoverage::Empty
    ));
    assert_eq!(all.coverage(699, 499), 255);
    assert_eq!(all.coverage(700, 10), 0);
    all.invert();
    assert!(all.is_empty());
    assert_eq!(all.stored_tile_count(), 0);
}

#[test]
fn invert_is_its_own_inverse_and_complements_every_canvas_pixel() {
    let mut s = shape(
        &SelectionShape::Ellipse {
            center: [300.0, 200.0],
            radii: [150.0, 90.0],
            rotation: 0.3,
        },
        true,
    );
    let before = s.clone();
    s.invert();
    for (x, y) in [(300, 200), (0, 0), (450, 200), (299, 111), (699, 499)] {
        assert_eq!(s.coverage(x, y), 255 - before.coverage(x, y));
    }
    s.invert();
    assert_eq!(s, before);
}

#[test]
fn add_and_subtract_touch_only_the_shape_tiles() {
    let a = shape(&rect(10.0, 10.0, 300.0, 300.0), false);
    let b = rasterize(&rect(250.0, 250.0, 600.0, 400.0), false, CANVAS);
    let tiles = a.combined_tiles(&b, SelectionOp::Add);
    assert!(tiles.iter().all(|(c, _)| b.tile(*c).is_some()));
    let mut sum = a.clone();
    let mut tiles = tiles;
    sum.swap_tiles(&mut tiles);
    assert_eq!(sum.coverage(275, 275), 255);
    assert_eq!(sum.coverage(500, 350), 255);
    assert_eq!(sum.coverage(500, 100), 0);
    sum.check();
    // Swapping back restores the original exactly.
    sum.swap_tiles(&mut tiles);
    assert_eq!(sum, a);
}

#[test]
fn bounds_hold_every_covered_pixel() {
    let s = shape(
        &SelectionShape::Ellipse {
            center: [333.3, 222.2],
            radii: [77.7, 55.5],
            rotation: 1.0,
        },
        true,
    );
    let b = s.bounds();
    for y in CANVAS.y0..CANVAS.y1 {
        for x in CANVAS.x0..CANVAS.x1 {
            let inside = x >= b.x0 && x < b.x1 && y >= b.y0 && y < b.y1;
            assert!(inside || s.coverage(x, y) == 0, "{x},{y}");
        }
    }
    // Tight: each edge has a covered pixel.
    assert!((b.y0..b.y1).any(|y| s.coverage(b.x0, y) > 0));
    assert!((b.x0..b.x1).any(|x| s.coverage(x, b.y1 - 1) > 0));
}

#[test]
fn feathering_changes_only_pixels_within_reach_and_keeps_uniform_areas() {
    let s = shape(&rect(100.0, 80.0, 300.0, 220.0), false);
    let mut f = s.clone();
    let mut tiles = feather_tiles(&s, 10.0);
    f.swap_tiles(&mut tiles);
    // σ = 3, radius 9: values change only within 9 px of the edge.
    assert_eq!(f.coverage(200, 150), 255);
    assert_eq!(f.coverage(90, 150), 0);
    assert!(f.coverage(99, 150) > 0 && f.coverage(99, 150) < 128);
    assert!(f.coverage(100, 150) > 128 && f.coverage(100, 150) < 255);
    // Symmetric about the old edge.
    let sum = u16::from(f.coverage(99, 150)) + u16::from(f.coverage(100, 150));
    assert!((254..=256).contains(&sum), "{sum}");
    // Everything selected feathers to itself (renormalized at the canvas
    // edge): nothing changes.
    assert!(feather_tiles(&Selection::all(CANVAS), 50.0).is_empty());
    // Not a positive number: nothing.
    for bad in [0.0, -3.0, f32::NAN, f32::INFINITY * 0.0] {
        assert!(feather_tiles(&s, bad).is_empty());
    }
}
