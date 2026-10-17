//! The reference's measured transforms (parity record 5D), reproduced by
//! the document. Images: `tests/reference/transform/*.png`, the ink
//! layer's premultiplied RGBA decoded from 400 × 300 captures (see that
//! folder's README).

use tela_canvas::{IRect, TiledSurface};
use tela_color::{PixelDepth, StoragePixel};

use super::*;
use crate::{Document, LayerId, LayerPixels, Placement, SelectionOp, SelectionShape};

const W: u32 = 400;
const H: u32 = 300;

fn png(name: &str) -> Vec<[u8; 4]> {
    let path = format!(
        "{}/tests/reference/transform/{name}.png",
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
        .map(|p| [p[0], p[1], p[2], if channels == 4 { p[3] } else { 255 }])
        .collect()
}

/// A document holding the probe pattern (the reference's own capture of
/// it, premultiplied) in a layer "ink", at `depth`.
fn document(depth: PixelDepth) -> (Document, LayerId) {
    let mut d = Document::with_depth(W, H, depth);
    let pixels = tela_color::with_pixel_type!(depth, P => {
        let mut s = TiledSurface::<P>::new();
        for (i, p) in png("source").into_iter().enumerate() {
            if p[3] > 0 {
                s.set_pixel((i as u32 % W) as i32, (i as u32 / W) as i32, P::from_rgba8(p));
            }
        }
        LayerPixels::from_surface(s)
    });
    let (id, _) = d
        .add_layer_with_pixels(Placement::Top, "ink", pixels)
        .unwrap();
    (d, id)
}

/// The ink layer's premultiplied pixels at 8 bits.
fn layer8(d: &Document, id: LayerId) -> Vec<[u8; 4]> {
    let l = d.layer(id).unwrap();
    (0..H as i32)
        .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
        .map(|(x, y)| l.pixels().pixel_rgba8(x, y))
        .collect()
}

/// Largest channel difference, pixels more than 2 apart, and the mean,
/// over pixels where either side has content.
fn compare(ours: &[[u8; 4]], theirs: &[[u8; 4]]) -> (u8, usize, f64) {
    let (mut max, mut over, mut sum, mut n) = (0u8, 0, 0u64, 0u64);
    for (a, b) in ours.iter().zip(theirs) {
        if a[3] == 0 && b[3] == 0 {
            continue;
        }
        let e = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap();
        max = max.max(e);
        over += usize::from(e > 2);
        sum += u64::from(e);
        n += 1;
    }
    (max, over, sum as f64 / n as f64)
}

fn params(scale: [f64; 2], degrees: f64, pivot: [f64; 2]) -> TransformParams {
    TransformParams {
        translate: [0.0; 2],
        scale,
        rotation: degrees.to_radians(),
        pivot,
    }
}

/// Transforms the ink layer (no selection) and compares with `name`.
fn check(name: &str, p: TransformParams, interpolation: Interpolation) -> (u8, usize, f64) {
    let mut worst = (0, 0, 0.0f64);
    for depth in PixelDepth::ALL {
        let (mut d, id) = document(depth);
        let source = d.transform_source(id).unwrap();
        assert_eq!(source.bounds(), IRect::new(168, 118, 232, 182), "T2");
        d.apply_transform(
            &source,
            &Transform {
                params: p,
                interpolation,
            },
        )
        .unwrap();
        let r = compare(&layer8(&d, id), &png(name));
        worst = (worst.0.max(r.0), worst.1.max(r.1), worst.2.max(r.2));
    }
    worst
}

const CENTRE: [f64; 2] = [200.0, 150.0];

#[test]
fn bicubic_scales_match_the_reference() {
    // T17–T20: rounded edges, Keys a = −1 premultiplied, the pyramid.
    for (name, sx, sy) in [
        ("s200", 2.0, 2.0),
        ("s110", 1.1, 1.1),
        ("s075", 0.75, 0.75),
        ("s050", 0.5, 0.5),
        ("s045", 0.45, 0.45),
        ("s033", 0.33, 0.33),
        ("s030", 0.3, 0.3),
        ("np", 1.5, 0.6),
    ] {
        let (max, over, mean) = check(name, params([sx, sy], 0.0, CENTRE), Interpolation::Bicubic);
        // The reference stores straight colour at 8 bits after the
        // transform; Mirae stores the premultiplied result quantized once
        // (ADR 0001): a few pixels land one level further apart.
        assert!(
            max <= 3 && over <= 2 && mean < 0.7,
            "{name}: max {max}, {over} over 2, mean {mean:.3}"
        );
    }
}

#[test]
fn bicubic_rotations_match_the_reference_inside_the_edge_ring() {
    // T21: the interior within 2/255; the outermost, partly covered ring
    // differs in colour (D-5D-1).
    for (name, degrees, pivot) in [
        ("r15", 15.0, CENTRE),
        ("r45", 45.0, CENTRE),
        ("r_m30", -30.0, CENTRE),
        ("pivot30", 30.0, [220.0, 170.0]),
    ] {
        let (max, over, mean) = check(
            name,
            params([1.0; 2], degrees, pivot),
            Interpolation::Bicubic,
        );
        // Measured: max 25–31, 44–51 pixels over 2 of ~3 300 (the ring).
        assert!(
            max <= 32 && over <= 60 && mean < 0.6,
            "{name}: max {max}, {over} over 2, mean {mean:.3}"
        );
    }
}

#[test]
fn nearest_neighbour_matches_the_reference_exactly() {
    // T22: ties to the lower index, no pyramid, hard edges.
    for (name, s, degrees) in [
        ("nearest_s200", 2.0, 0.0),
        ("nearest_s030", 0.3, 0.0),
        ("nearest_r15", 1.0, 15.0),
    ] {
        let (max, over, mean) = check(
            name,
            params([s, s], degrees, CENTRE),
            Interpolation::Nearest,
        );
        assert_eq!(max, 0, "{name}: {over} over 2, mean {mean:.3}");
    }
}

#[test]
fn flipping_is_an_exact_mirror() {
    // T15: a width of −100 %.
    let (max, ..) = check(
        "flipH",
        params([-1.0, 1.0], 0.0, CENTRE),
        Interpolation::Bicubic,
    );
    assert_eq!(max, 0);
}

#[test]
fn the_alpha_lock_does_not_matter() {
    // T37.
    let (mut d, id) = document(PixelDepth::Rgba8);
    d.set_alpha_locked(id, true).unwrap();
    let source = d.transform_source(id).unwrap();
    let t = Transform {
        params: params([1.5; 2], 0.0, CENTRE),
        interpolation: Interpolation::Bicubic,
    };
    d.apply_transform(&source, &t).unwrap();
    let (max, over, _) = compare(&layer8(&d, id), &png("alock150"));
    assert!(max <= 3 && over <= 3, "max {max}, {over} over 2");
}

/// The ink layer moved by `dx` through a rectangle selection, feathered
/// by `feather` px.
fn moved_through_selection(feather: Option<f32>, dx: f64) -> Vec<[u8; 4]> {
    let (mut d, id) = document(PixelDepth::Rgba8);
    d.select(
        &SelectionShape::Rect([150.0, 100.0], [200.0, 160.0]),
        false,
        SelectionOp::Replace,
    );
    if let Some(f) = feather {
        d.feather_selection(f);
    }
    let source = d.transform_source(id).unwrap();
    assert!(source.is_selection());
    let mut p = TransformParams::identity(source.centre());
    p.translate = [dx, 0.0];
    d.apply_transform(
        &source,
        &Transform {
            params: p,
            interpolation: Interpolation::Bicubic,
        },
    )
    .unwrap();
    layer8(&d, id)
}

#[test]
fn moving_selected_content_splits_it_by_the_coverage() {
    // T30: a hard selection leaves a hole and moves an exact copy.
    let (max, ..) = compare(&moved_through_selection(None, 100.0), &png("moveHard"));
    assert_eq!(max, 0);
    // T31: what moves is p·s, what stays p·(1 − s), composed over it.
    let (max, over, mean) = compare(
        &moved_through_selection(Some(10.0), 100.0),
        &png("moveFeather"),
    );
    assert!(
        max <= 3 && mean < 0.5,
        "max {max}, {over} over 2, mean {mean:.3}"
    );
}

#[test]
fn a_masked_layer_moves_with_its_mask() {
    // T33: the box is the union of pixels and mask; both scale together
    // with the same cubic.
    let (mut d, id) = document(PixelDepth::Rgba8);
    d.add_mask(id).unwrap();
    let mut tiles = tela_canvas::DirtyTiles::new();
    tiles.mark_rect(IRect::new(184, 100, 216, 200));
    d.modify_mask(id, &tiles, |c, t| {
        let (ox, oy) = c.origin();
        for y in 100..200 {
            for x in 184..216 {
                if (x - ox) as u32 >= 256 || (y - oy) as u32 >= 256 {
                    continue;
                }
                *t.get_mut((x - ox) as u32, (y - oy) as u32) = 0;
            }
        }
    })
    .unwrap();
    let shown = |d: &Document| -> Vec<[u8; 4]> {
        let l = d.layer(id).unwrap();
        let m = l.mask().unwrap();
        (0..H as i32)
            .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
            .map(|(x, y)| {
                let p = l
                    .pixels()
                    .pixel(x, y)
                    .scale(f32::from(m.value(x, y)) / 255.0);
                p.to_rgba8()
            })
            .collect()
    };
    let (max, ..) = compare(&shown(&d), &png("mask0"));
    assert!(max <= 1, "the masked source: {max}");
    let source = d.transform_source(id).unwrap();
    assert_eq!(source.bounds(), IRect::new(168, 100, 232, 200));
    assert!(source.moves_mask());
    d.apply_transform(
        &source,
        &Transform {
            params: params([1.5; 2], 0.0, source.centre()),
            interpolation: Interpolation::Bicubic,
        },
    )
    .unwrap();
    let (max, over, mean) = compare(&shown(&d), &png("pix150"));
    assert!(
        max <= 3 && over <= 10,
        "max {max}, {over} over 2, mean {mean:.3}"
    );
}

#[test]
fn the_selection_follows_a_rotation_with_the_same_edges() {
    // T32: a rectangle selection rotated 15°, filled black on white by the
    // reference: coverage = 255 − red.
    let (mut d, id) = document(PixelDepth::Rgba8);
    d.select(
        &SelectionShape::Rect([150.0, 100.0], [200.0, 160.0]),
        false,
        SelectionOp::Replace,
    );
    let source = d.transform_source(id).unwrap();
    d.apply_transform(
        &source,
        &Transform {
            params: params([1.0; 2], 15.0, source.centre()),
            interpolation: Interpolation::Bicubic,
        },
    )
    .unwrap();
    let s = d.selection().unwrap();
    let theirs = png("selection_rot15_fill");
    let (mut max, mut over) = (0u8, 0);
    let mut area = 0u64;
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let ours = s.coverage(x, y);
            area += u64::from(ours);
            let e = ours.abs_diff(255 - theirs[(y as u32 * W + x as u32) as usize][0]);
            max = max.max(e);
            over += usize::from(e > 2);
        }
    }
    assert!(
        (area as f64 / 255.0 - 3000.0).abs() < 2.0,
        "area {}",
        area as f64 / 255.0
    );
    // Only the corners differ: two pixels at each of the four (D-5D-2).
    assert!(max <= 24 && over <= 8, "max {max}, {over} over 2");
}

#[test]
fn pixels_moved_off_the_canvas_are_kept() {
    // T39: moved to x = −12 and back is the identity.
    let (mut d, id) = document(PixelDepth::Rgba8);
    for dx in [-180.0, 180.0] {
        let source = d.transform_source(id).unwrap();
        let mut p = TransformParams::identity(source.centre());
        p.translate = [dx, 0.0];
        d.apply_transform(
            &source,
            &Transform {
                params: p,
                interpolation: Interpolation::Bicubic,
            },
        )
        .unwrap();
    }
    assert_eq!(compare(&layer8(&d, id), &png("outback")).0, 0);
    assert_eq!(compare(&layer8(&d, id), &png("source")).0, 0);
}
