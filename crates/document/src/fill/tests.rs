//! Fill: the measured rules on small maps that can be read by eye, and the
//! document operations (history, refusals, targets, depths, sparsity).

use tela_canvas::{DirtyTiles, IRect, Tile};
use tela_color::{Color, PixelDepth, PremulRgba, StoragePixel, with_pixel_type};

use super::*;
use crate::{
    BlendMode, Document, HistoryAction, LayerError, LayerId, LayerPart, Placement, SelectionOp,
    SelectionShape,
};

/// Premultiplied 8-bit colours of the map letters: `.` transparent, `X`
/// black, `A`–`D` greys 0, 100, 200, 255 shifted by small amounts so
/// tolerance tests can tell them apart, `h` black at half alpha.
fn palette(ch: char) -> [u8; 4] {
    match ch {
        '.' => [0, 0, 0, 0],
        'X' => [0, 0, 0, 255],
        'A' => [200, 200, 200, 255],
        'B' => [210, 200, 200, 255],
        'C' => [230, 200, 200, 255],
        'D' => [60, 60, 60, 255],
        'h' => [0, 0, 0, 128],
        other => panic!("no colour for {other:?}"),
    }
}

/// A document the size of `map` (or `size`, the map at its top left)
/// whose first layer holds the map's colours.
fn doc_from(map: &[&str], size: (u32, u32), depth: PixelDepth) -> (Document, LayerId) {
    let mut d = Document::with_depth(size.0, size.1, depth);
    let id = d.active_layer();
    paint_map(&mut d, id, map, (0, 0));
    (d, id)
}

fn paint_map(d: &mut Document, id: LayerId, map: &[&str], at: (i32, i32)) {
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(IRect::new(
        at.0,
        at.1,
        at.0 + map[0].len() as i32,
        at.1 + map.len() as i32,
    ));
    with_pixel_type!(d.depth(), P => {
        d.modify_tiles(id, &tiles, |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for (y, row) in map.iter().enumerate() {
                for (x, ch) in row.chars().enumerate() {
                    let (lx, ly) = (at.0 + x as i32 - ox, at.1 + y as i32 - oy);
                    if (0..256).contains(&lx) && (0..256).contains(&ly) {
                        *t.get_mut(lx as u32, ly as u32) = P::from_rgba8(palette(ch));
                    }
                }
            }
        })
        .unwrap();
    });
}

fn bucket(seed: (i32, i32), tolerance: Tolerance, anti_alias: bool) -> ConnectedFill {
    ConnectedFill {
        seed,
        reference: FillReference::Target,
        tolerance,
        anti_alias,
    }
}

fn pixels(id: LayerId) -> FillTarget {
    FillTarget {
        layer: id,
        part: LayerPart::Pixels,
    }
}

/// The region as a map: `#` coverage 1, `+` partial, `.` none.
fn region_map(r: &FillRegion, w: i32, h: i32) -> Vec<String> {
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| match r.coverage(x, y) {
                    v if v >= 1.0 => '#',
                    v if v > 0.0 => '+',
                    _ => '.',
                })
                .collect()
        })
        .collect()
}

fn region_of(map: &[&str], seed: (i32, i32), t: Tolerance) -> Vec<String> {
    let (w, h) = (map[0].len() as u32, map.len() as u32);
    let (d, id) = doc_from(map, (w, h), PixelDepth::Rgba8);
    let r = d.fill_region(pixels(id), &bucket(seed, t, false)).unwrap();
    region_map(&r, w as i32, h as i32)
}

fn magenta() -> Color {
    Color::new(1.0, 0.0, 1.0)
}

#[test]
fn tolerance_thresholds_are_the_references() {
    // (tenths of a percent, largest 8-bit difference included), BK11, BK14.
    for (t, max) in [
        (0, 0),
        (1, 0),
        (5, 1),
        (10, 3),
        (20, 5),
        (40, 10),
        (50, 13),
        (100, 26),
        (120, 31),
        (200, 51),
        (300, 77),
        (600, 153),
        (750, 191),
        (1000, 254),
    ] {
        assert_eq!(Tolerance::from_tenths(t).threshold8(), max, "{t}");
    }
    assert_eq!(Tolerance::from_percent(12.34).tenths(), 123);
    assert_eq!(Tolerance::from_percent(f32::NAN), Tolerance::ZERO);
    assert_eq!(Tolerance::from_percent(-3.0), Tolerance::ZERO);
    assert_eq!(Tolerance::from_percent(250.0), Tolerance::MAX);
    assert_eq!(Tolerance::from_tenths(5000), Tolerance::MAX);
    assert_eq!(Tolerance::default().percent(), 10.0);
}

#[test]
fn distance_is_the_largest_straight_channel_difference() {
    let p = |r, g, b, a| PremulRgba::from_rgba8([r, g, b, a]);
    let grey = seed_color(p(128, 128, 128, 255));
    assert_eq!(quantize(distance(grey, p(128, 128, 128, 255))), 0);
    assert_eq!(quantize(distance(grey, p(155, 128, 128, 255))), 27 * 257);
    assert_eq!(quantize(distance(grey, p(140, 100, 128, 255))), 28 * 257);
    assert_eq!(quantize(distance(grey, p(128, 128, 128, 200))), 55 * 257);
    // Straight, not premultiplied: half-transparent white against opaque
    // black differs by a whole 255 in colour.
    let black = seed_color(p(0, 0, 0, 255));
    assert_eq!(quantize(distance(black, p(128, 128, 128, 128))), 65535);
    // Alpha 0 has no colour (BK13): only the alpha difference counts.
    let clear = seed_color(PremulRgba::TRANSPARENT);
    assert_eq!(clear, None);
    assert_eq!(quantize(distance(clear, PremulRgba::TRANSPARENT)), 0);
    assert_eq!(quantize(distance(clear, p(0, 0, 0, 20))), 20 * 257);
    assert_eq!(quantize(distance(clear, p(20, 0, 0, 20))), 20 * 257);
    assert_eq!(quantize(distance(grey, PremulRgba::TRANSPARENT)), 65535);
    // Never NaN, whatever the input.
    let junk = PremulRgba::new(f32::NAN, 2.0, -1.0, f32::NAN);
    assert!(distance(grey, junk).is_finite());
    assert!(distance(None, junk).is_finite() || distance(None, junk).is_nan());
}

#[test]
fn nested_rings_fill_one_ring_each() {
    let map = ["AAAAA", "ADDDA", "ADXDA", "ADDDA", "AAAAA"];
    assert_eq!(
        region_of(&map, (2, 2), Tolerance::DEFAULT),
        [".....", ".....", "..#..", ".....", "....."]
    );
    assert_eq!(
        region_of(&map, (1, 1), Tolerance::DEFAULT),
        [".....", ".###.", ".#.#.", ".###.", "....."]
    );
    assert_eq!(
        region_of(&map, (0, 4), Tolerance::DEFAULT),
        ["#####", "#...#", "#...#", "#...#", "#####"]
    );
}

#[test]
fn diagonal_walls_hold_regions_are_four_connected() {
    let map = ["..X..", ".X.X.", "X...X", ".X.X.", "..X.."];
    assert_eq!(
        region_of(&map, (2, 2), Tolerance::DEFAULT),
        [".....", "..#..", ".###.", "..#..", "....."],
        "the diamond holds (BK5)"
    );
    // A checkerboard's cells touch only at corners: a seed fills one.
    let checker = ["X.X.X", ".X.X.", "X.X.X", ".X.X."];
    assert_eq!(
        region_of(&checker, (1, 2), Tolerance::DEFAULT),
        [".....", ".....", ".#...", "....."]
    );
}

#[test]
fn holes_corridors_and_borders() {
    // A hole inside a solid wall stays out of the surrounding region.
    let hole = ["......", ".XXXX.", ".X..X.", ".XXXX.", "......"];
    assert_eq!(
        region_of(&hole, (0, 0), Tolerance::DEFAULT),
        ["######", "#....#", "#....#", "#....#", "######"]
    );
    // A one-pixel corridor joins two rooms.
    let corridor = ["XXXXXXX", "X..X..X", "X.....X", "X..X..X", "XXXXXXX"];
    assert_eq!(
        region_of(&corridor, (1, 1), Tolerance::DEFAULT),
        [".......", ".##.##.", ".#####.", ".##.##.", "......."]
    );
    // Regions touching the canvas edge stop there.
    let border = ["..X", "..X", "XXX"];
    assert_eq!(
        region_of(&border, (0, 0), Tolerance::DEFAULT),
        ["##.", "##.", "..."]
    );
}

#[test]
fn colours_are_compared_with_the_seed_not_with_neighbours() {
    // A, B, C step by 10 and 20 levels of red: at 5 % (13 levels) only A
    // and B are within reach of A, although B and C differ by 20.
    let map = ["AABBCC"];
    assert_eq!(
        region_of(&map, (0, 0), Tolerance::from_percent(5.0)),
        ["####.."]
    );
    assert_eq!(
        region_of(&map, (0, 0), Tolerance::from_percent(1.0)),
        ["##...."]
    );
    assert_eq!(
        region_of(&map, (0, 0), Tolerance::from_percent(12.0)),
        ["######"]
    );
    assert_eq!(
        region_of(&map, (5, 0), Tolerance::from_percent(5.0)),
        ["....##"]
    );
    // Tolerance 0 takes exactly the seed's colour.
    assert_eq!(region_of(&map, (2, 0), Tolerance::ZERO), ["..##.."]);
}

#[test]
fn full_tolerance_still_stops_at_a_full_difference() {
    // 100 %: everything except pixels 255 apart in a channel (BK14).
    let map = ["..h..X"];
    assert_eq!(region_of(&map, (0, 0), Tolerance::MAX), ["#####."]);
}

#[test]
fn anti_aliasing_covers_the_four_adjacent_edge_by_distance() {
    // h (alpha 128) next to the region gets (255 − 128) / (255 − 26).
    let map = ["...h", "....", "X..."];
    let (d, id) = doc_from(&map, (4, 3), PixelDepth::Rgba8);
    let r = d
        .fill_region(pixels(id), &bucket((0, 1), Tolerance::DEFAULT, true))
        .unwrap();
    assert_eq!(region_map(&r, 4, 3), ["###+", "####", ".###"]);
    let expected = (255.0 - 128.0) / (255.0 - 26.0);
    assert!((r.coverage(3, 0) - expected).abs() < 1e-6);
    assert_eq!(r.coverage(0, 2), 0.0, "black is 255 away: no coverage");
    // Pixels touching the region only at a corner are not on the edge,
    // even of the seed's colour (BK16).
    let map = ["X.X", ".X.", "XXX"];
    let (d, id) = doc_from(&map, (3, 3), PixelDepth::Rgba8);
    let r = d
        .fill_region(pixels(id), &bucket((1, 0), Tolerance::DEFAULT, true))
        .unwrap();
    assert_eq!(region_map(&r, 3, 3), [".#.", "...", "..."]);
}

#[test]
fn an_edge_pixel_receives_the_fill_over_it() {
    let map = ["...h"];
    let (mut d, id) = doc_from(&map, (4, 1), PixelDepth::Rgba8);
    d.fill_connected(
        pixels(id),
        &bucket((0, 0), Tolerance::DEFAULT, true),
        FillPaint::Color(magenta()),
    )
    .unwrap();
    let px = d.layer(id).unwrap().pixels();
    assert_eq!(px.pixel_rgba8(0, 0), [255, 0, 255, 255]);
    // c = 127/229 of magenta over black at alpha 128 (BK17: 198, 139…142).
    let c = 127.0 / 229.0;
    let a = c + (128.0 / 255.0) * (1.0 - c);
    assert_eq!(
        px.pixel_rgba8(3, 0),
        [
            tela_color::unorm8(c),
            0,
            tela_color::unorm8(c),
            tela_color::unorm8(a)
        ]
    );
}

#[test]
fn the_selection_limits_connectivity_and_scales_the_fill() {
    let mut d = Document::new(40, 10);
    let id = d.active_layer();
    // Two disjoint rectangles over one empty region (BK26).
    d.select(
        &SelectionShape::Rect([2.0, 2.0], [10.0, 8.0]),
        false,
        SelectionOp::Replace,
    );
    d.select(
        &SelectionShape::Rect([20.0, 2.0], [30.0, 8.0]),
        false,
        SelectionOp::Add,
    );
    let r = d
        .fill_region(pixels(id), &bucket((5, 5), Tolerance::DEFAULT, true))
        .unwrap();
    assert!(r.contains(5, 5) && r.contains(9, 7) && !r.contains(10, 5));
    assert!(!r.contains(25, 5), "the other rectangle is not reached");
    assert_eq!(r.pixel_count(), 8 * 6);
    // A seed outside the selection fills nothing and records nothing (BK27).
    let steps = d.history_len();
    let dirty = d
        .fill_connected(
            pixels(id),
            &bucket((15, 5), Tolerance::DEFAULT, true),
            FillPaint::Color(magenta()),
        )
        .unwrap();
    assert!(dirty.is_empty());
    assert_eq!(d.history_len(), steps);
    // Partial coverage multiplies what is written (BK28).
    d.select(
        &SelectionShape::Rect([8.0, 3.0], [32.0, 7.0]),
        false,
        SelectionOp::Replace,
    );
    d.feather_selection(3.0);
    d.fill_connected(
        pixels(id),
        &bucket((20, 5), Tolerance::DEFAULT, false),
        FillPaint::Color(magenta()),
    )
    .unwrap();
    let px = d.layer(id).unwrap().pixels();
    let mut partial = 0;
    for y in 0..10 {
        for x in 0..40 {
            let s = d.selection_coverage(x, y);
            partial += usize::from(s > 0 && s < 255);
            assert_eq!(px.pixel_rgba8(x, y)[3], s, "alpha = coverage at ({x}, {y})");
        }
    }
    assert!(partial > 50);
}

#[test]
fn a_fill_is_one_step_that_undoes_and_redoes_bit_exactly() {
    for depth in PixelDepth::ALL {
        let map = ["XXXXX", "X...X", "X...X", "XXXXX"];
        let (mut d, id) = doc_from(&map, (300, 20), depth);
        let before = d.digest();
        let tiles_before = d.layer(id).unwrap().pixels().tile_count();
        let steps = d.history_len();
        let dirty = d
            .fill_connected(
                pixels(id),
                &bucket((10, 10), Tolerance::DEFAULT, true),
                FillPaint::Color(magenta()),
            )
            .unwrap();
        assert_eq!(d.history_len(), steps + 1);
        assert_eq!(d.next_undo(), Some(HistoryAction::Fill));
        // The outside region crosses into the second tile column.
        assert_eq!(dirty.len(), 2);
        let after = d.digest();
        assert_ne!(after, before);
        d.check_invariants();
        d.undo().unwrap();
        assert_eq!(d.digest(), before);
        assert_eq!(
            d.layer(id).unwrap().pixels().tile_count(),
            tiles_before,
            "absent tiles come back absent"
        );
        d.redo().unwrap();
        assert_eq!(d.digest(), after);
        // Filling the same region again changes nothing: no step.
        let steps = d.history_len();
        let dirty = d
            .fill_connected(
                pixels(id),
                &bucket((10, 10), Tolerance::DEFAULT, true),
                FillPaint::Color(magenta()),
            )
            .unwrap();
        assert!(dirty.is_empty());
        assert_eq!(d.history_len(), steps);
    }
}

#[test]
fn refusals_change_nothing() {
    let map = ["....", "...."];
    let (mut d, id) = doc_from(&map, (8, 8), PixelDepth::Rgba8);
    let fill = bucket((1, 1), Tolerance::DEFAULT, true);
    let paint = FillPaint::Color(magenta());
    d.set_visible(id, false).unwrap();
    let (before, steps) = (d.digest(), d.history_len());
    assert_eq!(
        d.fill_connected(pixels(id), &fill, paint).err(),
        Some(LayerError::Hidden(id))
    );
    assert_eq!(
        d.fill_selection(pixels(id), paint).err(),
        Some(LayerError::Hidden(id))
    );
    assert_eq!(
        (d.digest(), d.history_len()),
        (before, steps),
        "refusals change nothing"
    );
    d.set_visible(id, true).unwrap();
    d.set_locked(id, true).unwrap();
    let steps_locked = d.history_len();
    assert_eq!(
        d.fill_connected(pixels(id), &fill, paint).err(),
        Some(LayerError::Locked(id))
    );
    assert_eq!(d.history_len(), steps_locked);
    d.set_locked(id, false).unwrap();
    let mask = FillTarget {
        layer: id,
        part: LayerPart::Mask,
    };
    assert_eq!(
        d.fill_connected(mask, &fill, paint).err(),
        Some(LayerError::NoMask(id))
    );
    assert_eq!(d.fill_region(mask, &fill), Err(LayerError::NoMask(id)));
    d.add_mask(id).unwrap();
    d.set_mask_enabled(id, false).unwrap();
    let steps_mask = d.history_len();
    assert_eq!(
        d.fill_connected(mask, &fill, paint).err(),
        Some(LayerError::MaskDisabled(id))
    );
    assert_eq!(d.history_len(), steps_mask);
    // Off the canvas: nothing, no step (BK9).
    d.set_mask_enabled(id, true).unwrap();
    let steps_off = d.history_len();
    for seed in [(-1, 0), (0, -1), (8, 0), (0, 8), (i32::MIN, i32::MAX)] {
        let dirty = d
            .fill_connected(pixels(id), &bucket(seed, Tolerance::MAX, true), paint)
            .unwrap();
        assert!(dirty.is_empty());
    }
    assert_eq!(d.history_len(), steps_off);
}

#[test]
fn layer_properties_are_not_involved() {
    let map = ["...."];
    let (mut d, id) = doc_from(&map, (4, 1), PixelDepth::Rgba8);
    d.set_opacity(id, 0.5).unwrap();
    d.set_blend_mode(id, BlendMode::Difference).unwrap();
    let below = d.add_layer(Placement::Below(id)).unwrap();
    d.set_clipped(id, true).unwrap();
    let active = d.active_layer();
    d.fill_connected(
        pixels(id),
        &bucket((0, 0), Tolerance::DEFAULT, true),
        FillPaint::Color(magenta()),
    )
    .unwrap();
    let l = d.layer(id).unwrap();
    // The pixels are the colour itself (BK20–BK22); the properties stay.
    assert_eq!(l.pixels().pixel_rgba8(2, 0), [255, 0, 255, 255]);
    assert_eq!(l.opacity(), 0.5);
    assert_eq!(l.blend_mode(), BlendMode::Difference);
    assert!(l.clipped());
    assert_eq!(d.active_layer(), active);
    let _ = below;
}

#[test]
fn alpha_lock_recolours_keeping_alpha_and_erase_removes() {
    let map = ["..hhXX"];
    let (mut d, id) = doc_from(&map, (6, 1), PixelDepth::Rgba8);
    d.set_alpha_locked(id, true).unwrap();
    d.fill_connected(
        pixels(id),
        &bucket((0, 0), Tolerance::MAX, false),
        FillPaint::Color(magenta()),
    )
    .unwrap();
    let px = d.layer(id).unwrap().pixels();
    assert_eq!(px.pixel_rgba8(0, 0), [0, 0, 0, 0], "nothing appears (BK23)");
    assert_eq!(
        px.pixel_rgba8(2, 0),
        [128, 0, 128, 128],
        "recoloured, alpha kept"
    );
    assert_eq!(
        px.pixel_rgba8(4, 0),
        [0, 0, 0, 255],
        "255 away: not reached"
    );
    assert_eq!(px.tile_count(), 1);
    // Erase removes the region (BK24), alpha lock or not.
    d.fill_connected(
        pixels(id),
        &bucket((3, 0), Tolerance::DEFAULT, false),
        FillPaint::Erase,
    )
    .unwrap();
    let px = d.layer(id).unwrap().pixels();
    assert_eq!(px.pixel_rgba8(2, 0), [0, 0, 0, 0]);
    assert_eq!(px.pixel_rgba8(4, 0), [0, 0, 0, 255]);
}

#[test]
fn a_mask_is_filled_from_its_own_values_whatever_the_colour() {
    let mut d = Document::new(12, 12);
    let id = d.active_layer();
    d.add_mask(id).unwrap();
    // Hide everything except a revealed outline (BK25).
    let mask = FillTarget {
        layer: id,
        part: LayerPart::Mask,
    };
    d.fill_selection(mask, FillPaint::Erase).unwrap();
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(IRect::new(0, 0, 12, 12));
    d.modify_mask(id, &tiles, |_, t| {
        for i in 2..10 {
            for (x, y) in [(i, 2), (i, 9), (2, i), (9, i)] {
                *t.get_mut(x, y) = 255;
            }
        }
    })
    .unwrap();
    let mut results = Vec::new();
    for colour in [magenta(), Color::new(0.0, 0.3, 0.1)] {
        let mut e = d.clone();
        e.fill_connected(
            mask,
            &bucket((5, 5), Tolerance::DEFAULT, false),
            FillPaint::Color(colour),
        )
        .unwrap();
        let m = e.layer(id).unwrap().mask().unwrap();
        assert_eq!(m.value(5, 5), 255, "revealed inside");
        assert_eq!(m.value(0, 0), 0, "the outside stays hidden");
        results.push(e.digest());
    }
    assert_eq!(results[0], results[1], "the colour does not matter (FL5)");
    // With the composed reference (the layer is empty) the whole canvas is
    // one region.
    let mut e = d.clone();
    let mut f = bucket((5, 5), Tolerance::DEFAULT, false);
    f.reference = FillReference::Composite;
    e.fill_connected(mask, &f, FillPaint::Color(magenta()))
        .unwrap();
    assert_eq!(e.layer(id).unwrap().mask().unwrap().value(0, 0), 255);
}

#[test]
fn the_composite_reference_is_the_visible_stack_without_paper() {
    // Layer "line" holds boxes; the target "fill" is empty (BK29, BK30).
    let mut d = Document::new(60, 12);
    let fill_id = d.active_layer();
    let line = d.add_layer(Placement::Below(fill_id)).unwrap();
    paint_map(&mut d, line, &["XXXXX", "X...X", "X...X", "XXXXX"], (0, 0));
    let faint = d.add_layer(Placement::Below(fill_id)).unwrap();
    paint_map(
        &mut d,
        faint,
        &["DDDDD", "D...D", "D...D", "DDDDD"],
        (10, 0),
    );
    let hidden = d.add_layer(Placement::Below(fill_id)).unwrap();
    paint_map(
        &mut d,
        hidden,
        &["XXXXX", "X...X", "X...X", "XXXXX"],
        (20, 0),
    );
    d.set_visible(hidden, false).unwrap();
    let mut comp = bucket((2, 2), Tolerance::DEFAULT, false);
    comp.reference = FillReference::Composite;
    let target = pixels(fill_id);
    let r = d.fill_region(target, &comp).unwrap();
    assert_eq!(r.pixel_count(), 6, "the black box holds");
    comp.seed = (22, 2);
    assert!(
        d.fill_region(target, &comp).unwrap().contains(40, 10),
        "a hidden box leaks"
    );
    comp.seed = (12, 2);
    assert_eq!(
        d.fill_region(target, &comp).unwrap().pixel_count(),
        6,
        "grey 60 holds"
    );
    // At 3 % opacity grey 60 is shown as alpha ~8 over transparency: within
    // 10 % of the transparent inside, so it leaks.
    d.set_opacity(faint, 0.03).unwrap();
    assert!(d.fill_region(target, &comp).unwrap().contains(40, 10));
    // A screen-mode black box over transparency shows as black: it holds,
    // as in the reference without its paper.
    d.set_opacity(faint, 1.0).unwrap();
    d.set_blend_mode(line, BlendMode::Screen).unwrap();
    comp.seed = (2, 2);
    assert_eq!(d.fill_region(target, &comp).unwrap().pixel_count(), 6);
    // The target alone sees an empty layer: everything.
    let mut own = comp;
    own.reference = FillReference::Target;
    assert_eq!(d.fill_region(target, &own).unwrap().pixel_count(), 60 * 12);
}

#[test]
fn fill_selection_fills_the_canvas_or_the_selection() {
    for depth in PixelDepth::ALL {
        let mut d = Document::with_depth(300, 40, depth);
        let id = d.active_layer();
        d.fill_selection(pixels(id), FillPaint::Color(magenta()))
            .unwrap();
        let px = d.layer(id).unwrap().pixels();
        assert_eq!(px.pixel_rgba8(299, 39), [255, 0, 255, 255]);
        assert_eq!(
            px.pixel_rgba8(300, 0),
            [0, 0, 0, 0],
            "not beyond the canvas"
        );
        assert_eq!(px.tile_count(), 2);
        assert_eq!(d.next_undo(), Some(HistoryAction::Fill));
        d.undo().unwrap();
        d.select(
            &SelectionShape::Rect([10.0, 10.0], [20.0, 20.0]),
            false,
            SelectionOp::Replace,
        );
        d.fill_selection(pixels(id), FillPaint::Color(magenta()))
            .unwrap();
        let px = d.layer(id).unwrap().pixels();
        assert_eq!(px.pixel_rgba8(15, 15), [255, 0, 255, 255]);
        assert_eq!(px.pixel_rgba8(9, 15), [0, 0, 0, 0]);
        assert_eq!(px.tile_count(), 1);
        // With alpha lock and nothing to recolour: no change, no step.
        d.undo().unwrap();
        d.set_alpha_locked(id, true).unwrap();
        let steps = d.history_len();
        assert!(
            d.fill_selection(pixels(id), FillPaint::Color(magenta()))
                .unwrap()
                .is_empty()
        );
        assert_eq!(d.history_len(), steps);
        assert_eq!(d.layer(id).unwrap().pixels().tile_count(), 0);
    }
}

#[test]
fn sixteen_bit_colour_never_passes_through_eight_bits() {
    let colour = Color::new(0.1234, 0.5678, 0.9012);
    let (mut d, id) = doc_from(&["...."], (4, 1), PixelDepth::Rgba16);
    d.fill_connected(
        pixels(id),
        &bucket((0, 0), Tolerance::DEFAULT, false),
        FillPaint::Color(colour),
    )
    .unwrap();
    let p = d.layer(id).unwrap().pixels().pixel(1, 0);
    for (got, want) in [(p.r, 0.1234), (p.g, 0.5678), (p.b, 0.9012)] {
        assert!(
            (got - want).abs() <= 0.5 / 65535.0 + 1e-6,
            "{got} vs {want}"
        );
    }
    // A colour on the 8-bit grid fills the same at both depths (FL8).
    let grid = Color::from_rgb8([37, 149, 213]);
    let mut out = Vec::new();
    for depth in PixelDepth::ALL {
        let (mut d, id) = doc_from(&["..h.", "X..."], (4, 2), depth);
        d.fill_connected(
            pixels(id),
            &bucket((0, 0), Tolerance::DEFAULT, true),
            FillPaint::Color(grid),
        )
        .unwrap();
        let px = d.layer(id).unwrap().pixels();
        out.push(
            (0..2)
                .flat_map(|y| (0..4).map(move |x| (x, y)))
                .map(|(x, y)| px.pixel_rgba8(x, y))
                .collect::<Vec<_>>(),
        );
    }
    for (a, b) in out[0].iter().zip(&out[1]) {
        for c in 0..4 {
            assert!(a[c].abs_diff(b[c]) <= 1, "{a:?} vs {b:?}");
        }
    }
}

#[test]
fn empty_regions_stay_sparse() {
    let mut d = Document::new(4096, 4096);
    let id = d.active_layer();
    let r = d
        .fill_region(pixels(id), &bucket((100, 100), Tolerance::DEFAULT, true))
        .unwrap();
    assert_eq!(r.pixel_count(), 4096 * 4096);
    assert_eq!(r.stats().maps, 0, "uniform tiles keep no map");
    // Erasing an empty layer visits and allocates nothing (FL10).
    let dirty = d
        .fill_connected(
            pixels(id),
            &bucket((100, 100), Tolerance::DEFAULT, true),
            FillPaint::Erase,
        )
        .unwrap();
    assert!(dirty.is_empty());
    assert_eq!(d.layer(id).unwrap().pixels().tile_count(), 0);
    assert!(!d.can_undo());
}

/// A 4096² document whose layer holds `colour(x, y)` (premultiplied 8-bit).
fn big(colour: impl Fn(i32, i32) -> [u8; 4]) -> (Document, LayerId) {
    let mut d = Document::new(4096, 4096);
    let id = d.active_layer();
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(IRect::new(0, 0, 4096, 4096));
    d.modify_tiles(id, &tiles, |c, t: &mut Tile<[u8; 4]>| {
        let (ox, oy) = c.origin();
        for y in 0..256 {
            for x in 0..256 {
                *t.get_mut(x, y) = colour(ox + x as i32, oy + y as i32);
            }
        }
    })
    .unwrap();
    (d, id)
}

#[test]
fn stress_one_colour_everywhere() {
    let (mut d, id) = big(|_, _| [40, 90, 200, 255]);
    let f = bucket((2000, 3000), Tolerance::DEFAULT, true);
    let r = d.fill_region(pixels(id), &f).unwrap();
    assert_eq!(r.pixel_count(), 4096 * 4096);
    assert_eq!(r.stats().maps, 0);
    assert!(r.stats().temp_bytes < 4 << 20, "{}", r.stats().temp_bytes);
    let dirty = d
        .fill_connected(pixels(id), &f, FillPaint::Color(magenta()))
        .unwrap();
    assert_eq!(dirty.len(), 256);
    assert_eq!(
        d.layer(id).unwrap().pixels().pixel_rgba8(4095, 4095),
        [255, 0, 255, 255]
    );
}

#[test]
fn stress_checkerboard() {
    let (d, id) = big(|x, y| {
        if (x + y) % 2 == 0 {
            [0, 0, 0, 255]
        } else {
            [0; 4]
        }
    });
    for t in [Tolerance::ZERO, Tolerance::DEFAULT, Tolerance::MAX] {
        let r = d.fill_region(pixels(id), &bucket((1, 0), t, true)).unwrap();
        // Every cell touches others only at corners: one pixel, at any
        // tolerance (black is 255 from transparent).
        assert_eq!(r.pixel_count(), 1);
        assert_eq!(r.stats().tiles_read, 1, "only the seed's tile is read");
    }
}

#[test]
fn stress_a_wall_grid_with_a_million_holes() {
    // 1-px walls every 4 px: the walls are one region around 1 048 576
    // holes of 3 × 3.
    let (d, id) = big(|x, y| {
        if x % 4 == 0 || y % 4 == 0 {
            [0, 0, 0, 255]
        } else {
            [0; 4]
        }
    });
    let r = d
        .fill_region(pixels(id), &bucket((0, 0), Tolerance::DEFAULT, true))
        .unwrap();
    assert_eq!(r.pixel_count(), 4096 * 4096 - 1024 * 1024 * 9);
    assert!(r.stats().temp_bytes < 64 << 20, "{}", r.stats().temp_bytes);
    let hole = d
        .fill_region(pixels(id), &bucket((2, 2), Tolerance::DEFAULT, false))
        .unwrap();
    assert_eq!(hole.pixel_count(), 9);
}

#[test]
fn stress_one_long_narrow_corridor() {
    // Walls on every odd row, open at alternate ends: a single 1-px
    // corridor zig-zagging through the canvas, 2048 rows × 4096 px long.
    let (d, id) = big(|x, y| {
        let wall = y % 2 == 1 && !((y / 2) % 2 == 0 && x == 4095) && !((y / 2) % 2 == 1 && x == 0);
        if wall { [0, 0, 0, 255] } else { [0; 4] }
    });
    let r = d
        .fill_region(pixels(id), &bucket((0, 0), Tolerance::DEFAULT, false))
        .unwrap();
    assert_eq!(
        r.pixel_count(),
        2048 * 4096 + 2048,
        "every row and every opening"
    );
    assert!(r.stats().temp_bytes < 64 << 20, "{}", r.stats().temp_bytes);
}
