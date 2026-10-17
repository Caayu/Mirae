//! The reference's measured selections (parity record 5C), reproduced.
//! Images: `tests/reference/selection/*.png`, 400 × 300 captures of the
//! reference's canvas, coverage = `255 − red` (see that folder's README).

use tela_brush::{Dab, DabSink, PixelAlpha, StampContext, StrokeBuffer};
use tela_canvas::{IRect, TILE_SIZE, Tile, TileCoord, TiledSurface};
use tela_color::{Color, StoragePixel};

use super::*;

const W: u32 = 400;
const H: u32 = 300;
const CANVAS: IRect = IRect::new(0, 0, W as i32, H as i32);

/// Coverage per pixel of a reference image.
fn reference(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/reference/selection/{name}.png",
        env!("CARGO_MANIFEST_DIR")
    );
    let file = std::fs::File::open(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut buf).unwrap();
    assert_eq!((frame.width, frame.height), (W, H));
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("{other:?}"),
    };
    buf[..frame.buffer_size()]
        .chunks_exact(channels)
        .map(|p| 255 - p[0])
        .collect()
}

/// A selection with exactly a reference image's coverage.
fn measured(name: &str) -> Selection {
    let values = reference(name);
    let mut tiles = TiledSurface::new();
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let v = values[(y as u32 * W + x as u32) as usize];
            if v > 0 {
                tiles.set_pixel(x, y, v);
            }
        }
    }
    Selection::from_coverage(CANVAS, tiles)
}

fn coverage(s: &Selection) -> Vec<u8> {
    (0..H as i32)
        .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
        .map(|(x, y)| s.coverage(x, y))
        .collect()
}

/// Largest difference and the number of pixels more than `tol` apart.
fn compare(ours: &[u8], theirs: &[u8], tol: u8) -> (u8, usize) {
    let max = ours
        .iter()
        .zip(theirs)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    let over = ours
        .iter()
        .zip(theirs)
        .filter(|(a, b)| a.abs_diff(**b) > tol)
        .count();
    (max, over)
}

fn select(shape: &SelectionShape, aa: bool) -> Selection {
    Selection::from_coverage(CANVAS, rasterize(shape, aa, CANVAS))
}

fn apply(sel: &mut Selection, mut tiles: Vec<(TileCoord, Option<Tile<u8>>)>) {
    sel.swap_tiles(&mut tiles);
}

#[test]
fn rectangles_match_the_reference_exactly() {
    let r = select(&SelectionShape::Rect([50.0, 40.0], [150.0, 120.0]), false);
    assert_eq!(
        compare(&coverage(&r), &reference("rect_50_40_150_120"), 0),
        (0, 0)
    );
    // S8: the 400 % drag at quarter pixels, with anti-aliasing on.
    let r = select(&SelectionShape::Rect([50.25, 40.25], [150.5, 120.75]), true);
    assert_eq!(
        compare(&coverage(&r), &reference("rect_subpixel_400pct"), 0),
        (0, 0)
    );
}

#[test]
fn ellipses_match_the_reference_in_area_and_differ_only_on_the_edge() {
    let e = SelectionShape::Ellipse {
        center: [120.5, 90.5],
        radii: [60.5, 40.5],
        rotation: 0.0,
    };
    let area = |v: &[u8]| v.iter().map(|&c| f64::from(c) / 255.0).sum::<f64>();
    let (ours, theirs) = (
        coverage(&select(&e, true)),
        reference("ellipse_60_50_181_131_aa"),
    );
    let (max, over) = compare(&ours, &theirs, 2);
    // D-5C-1: the reference flattens its ellipse its own way; single edge
    // pixels differ (measured here: max and count), the area within 0.5 %.
    assert!(max <= 70 && over <= 350, "max {max}, {over} pixels");
    assert!((area(&ours) - area(&theirs)).abs() / area(&theirs) < 0.005);
    // Interior and exterior agree everywhere: only partial pixels differ.
    for (a, b) in ours.iter().zip(&theirs) {
        if *b == 0 || *b == 255 {
            assert!(a.abs_diff(*b) <= 70);
        }
    }
    let (ours, theirs) = (
        coverage(&select(&e, false)),
        reference("ellipse_60_50_181_131_hard"),
    );
    let (_, over) = compare(&ours, &theirs, 0);
    assert!(over <= 40, "hard ellipse: {over} pixels differ");
}

#[test]
fn the_lasso_fills_even_odd_like_the_reference() {
    let points: Vec<[f32; 2]> = (0..5)
        .map(|k| {
            let a = -std::f32::consts::FRAC_PI_2 + k as f32 * 4.0 * std::f32::consts::PI / 5.0;
            [
                (200.0 + 100.0 * a.cos()).round(),
                (150.0 + 100.0 * a.sin()).round(),
            ]
        })
        .collect();
    let ours = coverage(&select(&SelectionShape::Polygon(points), true));
    let theirs = reference("lasso_pentagram");
    // Same inside/outside everywhere except near the tips the reference
    // rounds (D-5C-2): measured 185 of 7 691 covered pixels (2.4 %).
    let covered = theirs.iter().filter(|&&c| c > 127).count();
    let differ = ours
        .iter()
        .zip(&theirs)
        .filter(|(a, b)| (**a > 127) != (**b > 127))
        .count();
    assert!(differ * 100 < covered * 3, "{differ} of {covered}");
    assert_eq!(ours[(150 * W + 200) as usize], 0, "even-odd centre");
}

#[test]
fn feathering_matches_the_reference_within_2() {
    for px in [1, 3, 10, 20, 50] {
        let mut s = select(&SelectionShape::Rect([100.0, 80.0], [300.0, 220.0]), false);
        let t = feather_tiles(&s, px as f32);
        apply(&mut s, t);
        let (max, _) = compare(
            &coverage(&s),
            &reference(&format!("feather_{px}_rect_100_80_300_220")),
            2,
        );
        assert!(max <= 2, "{px} px: {max}");
    }
    // S21: the kernel is renormalized over the canvas.
    let mut s = select(&SelectionShape::Rect([-5.0, 40.0], [10.0, 90.0]), false);
    let t = feather_tiles(&s, 20.0);
    apply(&mut s, t);
    let (max, _) = compare(&coverage(&s), &reference("feather_20_rect_m5_40_10_90"), 2);
    assert!(max <= 2, "edge: {max}");
}

#[test]
fn combining_reproduces_the_reference_exactly() {
    // S14–S16: the reference's own A and B in, its results out.
    let a = measured("feather_50_rect_100_80_300_220");
    let b = rasterize_measured("op_b_ellipse_150_100_271_181");
    for (op, name) in [
        (SelectionOp::Add, "op_add"),
        (SelectionOp::Subtract, "op_subtract"),
    ] {
        let mut r = a.clone();
        let t = r.combined_tiles(&b, op);
        apply(&mut r, t);
        assert_eq!(
            compare(&coverage(&r), &reference(name), 0),
            (0, 0),
            "{op:?}"
        );
    }
    let r = a.intersected(&b);
    assert_eq!(
        compare(&coverage(&r), &reference("op_intersect"), 0),
        (0, 0)
    );
}

/// A reference image as raw shape coverage.
fn rasterize_measured(name: &str) -> TiledSurface<u8> {
    let s = measured(name);
    let mut out = TiledSurface::new();
    for (c, t) in s.sorted_tiles() {
        out.insert_tile(c, t.clone());
    }
    out
}

/// One stroke covering the canvas fully (`m` = 1 everywhere), as the
/// reference's dense scrub does inside the selection (S22).
fn covering_stroke(mode: tela_brush::preset::PaintMode) -> StrokeBuffer {
    let ctx = StampContext {
        mode,
        ..Default::default()
    };
    let mut s = StrokeBuffer::new(Color::new(0.0, 0.0, 0.0), ctx);
    for y in (0..H).step_by(40) {
        for x in (0..W).step_by(40) {
            s.dab(&Dab {
                center: glam::Vec2::new(x as f32 + 20.0, y as f32 + 20.0),
                radius: 60.0,
                opacity: 1.0,
                flow: 1.0,
                ..Dab::default()
            });
        }
    }
    s
}

/// Black paint (or erase) committed through `sel` onto a layer `base`
/// (white paper underneath is implied: coverage = alpha of black).
fn commit_through(stroke: &StrokeBuffer, sel: &Selection, base: &mut TiledSurface<[u8; 4]>) {
    for c in CANVAS.tiles() {
        let tile = base.tile_or_insert(c);
        stroke.commit_tile_within(c, tile, PixelAlpha::Free, sel.tile(c));
    }
}

fn alphas(layer: &TiledSurface<[u8; 4]>) -> Vec<u8> {
    (0..H as i32)
        .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
        .map(|(x, y)| (layer.pixel(x, y).to_premul().a * 255.0).round() as u8)
        .collect()
}

/// The scrubbed band of the reference probes (rows 125..175, x 70..330).
fn band(v: &[u8]) -> Vec<u8> {
    (125..175)
        .flat_map(|y| (70..330).map(move |x| v[(y * W + x) as usize]))
        .collect()
}

#[test]
fn strokes_through_a_selection_match_the_reference() {
    use tela_brush::preset::PaintMode;
    let sel = measured("feather_50_rect_100_80_300_220");
    let paint = covering_stroke(PaintMode::Paint);
    let mut layer = TiledSurface::new();
    commit_through(&paint, &sel, &mut layer);
    assert_eq!(
        band(&alphas(&layer)),
        band(&reference("paint_one_stroke")),
        "S22"
    );
    commit_through(&paint, &sel, &mut layer);
    let (max, _) = compare(
        &band(&alphas(&layer)),
        &band(&reference("paint_two_strokes")),
        1,
    );
    assert!(max <= 1, "S23: {max}");
    // S24: erasing a full layer through the selection leaves 1 − s.
    let mut full = TiledSurface::new();
    for c in CANVAS.tiles() {
        full.insert_tile(c, Tile::filled(<[u8; 4]>::from_rgba8([0, 0, 0, 255])));
    }
    commit_through(&covering_stroke(PaintMode::Erase), &sel, &mut full);
    let left: Vec<u8> = alphas(&full);
    let s = coverage(&sel);
    assert!(left.iter().zip(&s).all(|(l, s)| l.abs_diff(255 - s) <= 1));
    // S26: a mask revealed through the selection reveals exactly s.
    let mut mask = Tile::<u8>::new();
    let c = TileCoord { x: 0, y: 0 };
    paint.commit_alpha_tile_within(c, &mut mask, sel.tile(c));
    let theirs = reference("mask_reveal_stroke");
    for y in 125..175 {
        for x in 70..TILE_SIZE as i32 {
            assert_eq!(
                mask.get(x as u32, y as u32),
                theirs[(y as u32 * W + x as u32) as usize]
            );
        }
    }
}
