//! The reference's measured fills (parity record 5E), reproduced by the
//! document. Data: `tests/reference/fill/` (see its README): the probe
//! documents' ink layers (straight RGBA), which islands each fill reached
//! at each tolerance (`islands.csv`), and decoded captures (premultiplied
//! RGBA) of fills with anti-aliasing, a feathered selection, alpha lock
//! and Edit › Fill.

use tela_canvas::TiledSurface;
use tela_color::{Color, PixelDepth, Rgba, StoragePixel};

use super::*;
use crate::{Document, LayerId, LayerPart, LayerPixels, Placement, SelectionOp, SelectionShape};

const W: u32 = 400;
const H: u32 = 300;

fn path(name: &str) -> String {
    format!("{}/tests/reference/fill/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn png(name: &str) -> Vec<[u8; 4]> {
    let path = path(&format!("{name}.png"));
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

/// A document over the reference's black and white layers is not needed:
/// fills of the ink layer read and write the ink layer only. The probe's
/// straight colours are premultiplied into Mirae's storage at `depth`.
fn document(input: &str, depth: PixelDepth) -> (Document, LayerId) {
    let mut d = Document::with_depth(W, H, depth);
    let pixels = tela_color::with_pixel_type!(depth, P => {
        let mut s = TiledSurface::<P>::new();
        for (i, p) in png(input).into_iter().enumerate() {
            if p[3] > 0 {
                let c = Rgba::new(
                    f32::from(p[0]) / 255.0,
                    f32::from(p[1]) / 255.0,
                    f32::from(p[2]) / 255.0,
                    f32::from(p[3]) / 255.0,
                );
                s.set_pixel((i as u32 % W) as i32, (i as u32 / W) as i32, P::from_premul(c.premultiply()));
            }
        }
        LayerPixels::from_surface(s)
    });
    let (id, _) = d
        .add_layer_with_pixels(Placement::Top, "ink", pixels)
        .unwrap();
    (d, id)
}

fn layer8(d: &Document, id: LayerId) -> Vec<[u8; 4]> {
    let l = d.layer(id).unwrap();
    (0..H as i32)
        .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
        .map(|(x, y)| l.pixels().pixel_rgba8(x, y))
        .collect()
}

fn magenta() -> FillPaint {
    FillPaint::Color(Color::new(1.0, 0.0, 1.0))
}

fn target(id: LayerId) -> FillTarget {
    FillTarget {
        layer: id,
        part: LayerPart::Pixels,
    }
}

fn bucket(seed: (i32, i32), tenths: u16, anti_alias: bool) -> ConnectedFill {
    ConnectedFill {
        seed,
        reference: FillReference::Target,
        tolerance: Tolerance::from_tenths(tenths),
        anti_alias,
    }
}

/// Largest channel difference and pixels more than `tol` apart.
fn compare(ours: &[[u8; 4]], theirs: &[[u8; 4]], tol: u8) -> (u8, usize) {
    let (mut max, mut over) = (0u8, 0);
    for (a, b) in ours.iter().zip(theirs) {
        let e = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap();
        max = max.max(e);
        over += usize::from(e > tol);
    }
    (max, over)
}

/// Island corners of the probe documents, in the probe's order
/// (`fill_probe`: rows above the chain for `tol`; per zone for `alpha`).
fn islands(kind: &str) -> Vec<(u32, u32)> {
    let mut s = Vec::new();
    if kind == "tol" {
        for y in (4..260).step_by(8) {
            for x in (4..W - 4).step_by(8) {
                s.push((x, y));
            }
        }
        return s;
    }
    for ((x0, x1), count) in [
        ((0, 132), 6 + 3 * 111),
        ((136, 266), 3 * 111),
        ((266, 400), 3 * 111),
    ] {
        let mut zone = Vec::new();
        for y in (4..H - 4).step_by(8) {
            for x in (x0 + 4..x1 - 4 - 2).step_by(8) {
                zone.push((x, y));
            }
        }
        s.extend(zone.into_iter().take(count));
    }
    s
}

fn reached(px: &[[u8; 4]], (x, y): (u32, u32)) -> bool {
    (1..3).all(|dy| (1..3).all(|dx| px[((y + dy) * W + x + dx) as usize] == [255, 0, 255, 255]))
}

/// One fill of `islands.csv`.
struct Measured {
    kind: String,
    zone: usize,
    tenths: u16,
    seed: (i32, i32),
    chain_last: i32,
    bits: Vec<u8>,
}

fn measured() -> Vec<Measured> {
    let text = std::fs::read_to_string(path("islands.csv")).unwrap();
    text.lines()
        .skip(1)
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            Measured {
                kind: f[0].into(),
                zone: f[1].parse().unwrap(),
                tenths: f[2].parse().unwrap(),
                seed: (f[3].parse().unwrap(), f[4].parse().unwrap()),
                chain_last: f[5].parse().unwrap(),
                bits: (0..f[6].len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&f[6][i..i + 2], 16).unwrap())
                    .collect(),
            }
        })
        .collect()
}

/// Islands classified differently from the reference, per fill.
fn island_mismatches(depth: PixelDepth, kind: &str) -> Vec<(u16, usize, usize, Vec<usize>)> {
    let corners = islands(kind);
    let zones = [(0u32, 132u32), (136, 266), (266, 400)];
    let mut out = Vec::new();
    for m in measured().into_iter().filter(|m| m.kind == kind) {
        let (mut d, id) = document(kind, depth);
        d.fill_connected(target(id), &bucket(m.seed, m.tenths, false), magenta())
            .unwrap();
        let px = layer8(&d, id);
        let mut wrong = Vec::new();
        for (k, &c) in corners.iter().enumerate() {
            if kind == "alpha" && !(zones[m.zone].0..zones[m.zone].1).contains(&c.0) {
                continue;
            }
            let theirs = m.bits[k / 8] >> (k % 8) & 1 == 1;
            if reached(&px, c) != theirs {
                wrong.push(k);
            }
        }
        if kind == "tol" {
            let y = 281;
            let last = (0..98u32)
                .rev()
                .find(|k| px[(y * W + 6 + 4 * k) as usize] == [255, 0, 255, 255])
                .map_or(-1, |k| k as i32);
            assert_eq!(
                last, m.chain_last,
                "chain at {} tenths ({depth:?})",
                m.tenths
            );
        }
        out.push((m.tenths, m.zone, corners.len(), wrong));
    }
    out
}

/// Islands an 8-bit document classifies differently from the reference:
/// translucent colours exactly on the 30 % threshold (77 levels), whose
/// straight colour 8-bit premultiplied storage cannot keep —
/// `(190, 205, 55, 241)` comes back with green 205.27 and
/// `(185, 205, 73, 186)` with 205.65 (at alpha 186, 205 and 206 store the
/// same value). Parity record D-5E-1; 16-bit documents have none.
const EIGHT_BIT_THRESHOLD_ISLANDS: [usize; 2] = [1013, 1475];

#[test]
fn tolerance_classifies_every_island_as_the_reference() {
    // 16 tolerances × 1 568 islands, opaque and translucent (BK10–BK14).
    for depth in PixelDepth::ALL {
        for (tenths, _, _, wrong) in island_mismatches(depth, "tol") {
            let known: &[usize] = if depth == PixelDepth::Rgba8 && tenths == 300 {
                &EIGHT_BIT_THRESHOLD_ISLANDS
            } else {
                &[]
            };
            assert_eq!(wrong, known, "{depth:?} at {tenths} tenths");
        }
    }
}

#[test]
fn transparency_classifies_every_island_as_the_reference() {
    // Three seeds (transparent, black, white) × 5 tolerances; hidden
    // colours under alpha 0, low alphas, alpha-only differences (BK13).
    for depth in PixelDepth::ALL {
        for (tenths, zone, _, wrong) in island_mismatches(depth, "alpha") {
            assert!(
                wrong.is_empty(),
                "{depth:?} zone {zone} at {tenths} tenths: islands {wrong:?}"
            );
        }
    }
}

#[test]
fn anti_aliased_edges_match_the_reference() {
    // Island edges at 10 % and 30 % (BK15) and the ring of line art (BK17).
    for depth in PixelDepth::ALL {
        for (name, input, seed, tenths, aa) in [
            ("tol_aa_100", "tol", (2, 2), 100, true),
            ("tol_aa_300", "tol", (2, 2), 300, true),
            ("ring_aa", "conn", (240, 150), 100, true),
            ("ring_noaa", "conn", (240, 150), 100, false),
        ] {
            let (mut d, id) = document(input, depth);
            d.fill_connected(target(id), &bucket(seed, tenths, aa), magenta())
                .unwrap();
            let (mut ours, theirs) = (layer8(&d, id), png(name));
            if depth == PixelDepth::Rgba8 && name == "tol_aa_300" {
                // The two threshold islands (D-5E-1) are compared above.
                let corners = islands("tol");
                for k in EIGHT_BIT_THRESHOLD_ISLANDS {
                    let (x, y) = corners[k];
                    for i in
                        (0..4).flat_map(|dy| (0..4).map(move |dx| ((y + dy) * W + x + dx) as usize))
                    {
                        ours[i] = theirs[i];
                    }
                }
            }
            let (max, over) = compare(&ours, &theirs, 2);
            assert!(
                max <= 2 && over == 0,
                "{name} ({depth:?}): max {max}, {over} pixels over 2"
            );
        }
    }
}

#[test]
fn a_fill_through_a_feathered_selection_is_scaled_by_its_coverage() {
    // BK28: rectangle (170, 205)–(290, 265) feathered 10 px; Mirae's
    // feather is the reference's within 2/255 (5C S19).
    for depth in PixelDepth::ALL {
        let (mut d, id) = document("conn", depth);
        d.select(
            &SelectionShape::Rect([170.0, 205.0], [290.0, 265.0]),
            false,
            SelectionOp::Replace,
        );
        d.feather_selection(10.0);
        d.fill_connected(target(id), &bucket((230, 235), 100, false), magenta())
            .unwrap();
        let (max, over) = compare(&layer8(&d, id), &png("feather10"), 2);
        assert!(max <= 2 && over == 0, "{depth:?}: max {max}, {over} over 2");
    }
}

#[test]
fn alpha_lock_and_edit_fill_match_the_reference() {
    // BK23, BK32: the lock probe's ramp, block and transparency.
    for depth in PixelDepth::ALL {
        for (name, locked, fill) in [
            ("alock_1000", true, Some(1000)),
            ("alock_100", true, Some(100)),
            ("lock_1000", false, Some(1000)),
            ("fill_all", false, None),
            ("fill_all_alock", true, None),
        ] {
            let (mut d, id) = document("lock", depth);
            d.set_alpha_locked(id, locked).unwrap();
            match fill {
                Some(t) => d.fill_connected(target(id), &bucket((10, 10), t, false), magenta()),
                None => d.fill_selection(target(id), magenta()),
            }
            .unwrap();
            let (max, over) = compare(&layer8(&d, id), &png(name), 1);
            assert!(
                max <= 1 && over == 0,
                "{name} ({depth:?}): max {max}, {over} over 1"
            );
        }
    }
}

#[test]
fn connectivity_matches_the_reference_over_white() {
    // BK5–BK7 (close gap off): the diamond, the checkerboard, the
    // corridors; plain captures over white, anti-aliasing on.
    for depth in PixelDepth::ALL {
        for (name, seed) in [
            ("diamond_white", (150, 50)),
            ("checker_white", (18, 140)),
            ("corridor_white", (225, 50)),
        ] {
            let (mut d, id) = document("conn", depth);
            d.fill_connected(target(id), &bucket(seed, 100, true), magenta())
                .unwrap();
            let over_white: Vec<[u8; 4]> = layer8(&d, id)
                .into_iter()
                .map(|p| {
                    let k = 255 - p[3];
                    [p[0] + k, p[1] + k, p[2] + k, 255]
                })
                .collect();
            let (max, over) = compare(&over_white, &png(name), 2);
            assert!(
                max <= 2 && over == 0,
                "{name} ({depth:?}): max {max}, {over} over 2"
            );
        }
    }
}
