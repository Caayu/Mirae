//! Blending over a translucent backdrop without paper (phase 5B gate,
//! D-4C-1, ADR 0022).
//!
//! `tests/reference/backdrop_without_paper.csv` holds the reference's
//! results for 45 900 combinations: 17 modes × 3 layer opacities × 36
//! backdrops (six colours at alpha 0, 1, 64, 128, 191, 255) × 25 tops
//! (eight colours at alpha 255, 160, 64, and transparent), composed with
//! no paper and exported with transparency (see the README there). These
//! tests build each stack through `Document` and compose it over
//! transparency, as a transparent export and merge down do.
//!
//! Compared premultiplied (straight colour is meaningless at alpha 0 and
//! amplifies rounding at low alpha): every channel within 2/255.

use std::collections::BTreeMap;

use tela_canvas::{DirtyTiles, TILE_SIZE, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, Rgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, ComposeScratch, Document, LayerId, Placement};

struct Row {
    bottom: [u8; 4],
    top: [u8; 4],
    measured: [u8; 4],
}

/// Rows grouped by (mode, opacity), in file order.
fn groups() -> BTreeMap<(&'static str, u8), Vec<Row>> {
    let path = format!(
        "{}/tests/reference/backdrop_without_paper.csv",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).expect("reference CSV");
    let mut out: BTreeMap<(&'static str, u8), Vec<Row>> = BTreeMap::new();
    for line in text.lines().skip(1) {
        let v: Vec<&str> = line.split(',').collect();
        let n = |i: usize| v[i].parse::<u8>().unwrap();
        let mode = BlendMode::from_name(v[0]).expect("known mode");
        assert_eq!(n(16), 0, "the export has no dithering");
        out.entry((mode.name(), n(1))).or_default().push(Row {
            bottom: [n(4), n(5), n(6), n(7)],
            top: [n(8), n(9), n(10), n(11)],
            measured: [n(12), n(13), n(14), n(15)],
        });
    }
    out
}

fn stored<P: StoragePixel>(c: [u8; 4]) -> P {
    P::from_premul(
        Rgba::new(
            f32::from(c[0]) / 255.0,
            f32::from(c[1]) / 255.0,
            f32::from(c[2]) / 255.0,
            f32::from(c[3]) / 255.0,
        )
        .premultiply(),
    )
}

/// Writes one pixel per row into tile (0, 0) of `layer`.
fn fill(d: &mut Document, layer: LayerId, colors: &[[u8; 4]]) {
    let mut t = DirtyTiles::new();
    t.mark(TileCoord { x: 0, y: 0 });
    with_pixel_type!(d.depth(), P => d.modify_tiles(layer, &t, |_, tile: &mut Tile<P>| {
        for (i, c) in colors.iter().enumerate() {
            *tile.get_mut(i as u32 % TILE_SIZE, i as u32 / TILE_SIZE) = stored::<P>(*c);
        }
    }))
    .unwrap();
}

fn premultiplied(c: [u8; 4]) -> [u8; 4] {
    Rgba::new(
        f32::from(c[0]) / 255.0,
        f32::from(c[1]) / 255.0,
        f32::from(c[2]) / 255.0,
        f32::from(c[3]) / 255.0,
    )
    .premultiply()
    .to_rgba8()
}

/// 2/255 everywhere, except Color Dodge and Color Burn in 8-bit documents:
/// they divide by `1 − Cs` and `Cs`, and an 8-bit premultiplied pixel at
/// low alpha (a 64/255 top) stores its colour a few levels off (the root
/// of D-5A-3), which the division amplifies to 5/255 in 22 of 5 400 rows.
/// The same rows are within 2/255 in 16-bit documents, so the rule itself
/// is not what differs.
fn tolerance(mode: BlendMode, depth: PixelDepth) -> u8 {
    match (mode, depth) {
        (BlendMode::ColorDodge | BlendMode::ColorBurn, PixelDepth::Rgba8) => 5,
        _ => 2,
    }
}

fn without_paper(depth: PixelDepth) {
    let mut scratch = ComposeScratch::new();
    let mut worst: BTreeMap<&'static str, u8> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut count = 0;
    for ((mode, opacity), rows) in groups() {
        let mode = BlendMode::from_name(mode).unwrap();
        // The reference's document as opened: a base layer under the
        // probed layer, no paper.
        let mut d = Document::with_depth(TILE_SIZE, TILE_SIZE, depth);
        let base = d.active_layer();
        fill(
            &mut d,
            base,
            &rows.iter().map(|r| r.bottom).collect::<Vec<_>>(),
        );
        let top = d.add_layer(Placement::Top).unwrap();
        fill(&mut d, top, &rows.iter().map(|r| r.top).collect::<Vec<_>>());
        d.set_opacity(top, f32::from(opacity) / 255.0).unwrap();
        d.set_blend_mode(top, mode).unwrap();
        assert!(d.compose_tile_transparent_into(TileCoord { x: 0, y: 0 }, &mut scratch));
        for (i, r) in rows.iter().enumerate() {
            count += 1;
            let got = scratch.premultiplied()[i].to_rgba8();
            let want = premultiplied(r.measured);
            let err = (0..4).map(|k| got[k].abs_diff(want[k])).max().unwrap();
            let e = worst.entry(mode.name()).or_default();
            *e = (*e).max(err);
            if err > tolerance(mode, depth) {
                failures.push(format!(
                    "{mode:?} op {opacity} bottom {:?} top {:?}: mirae {got:?} reference {want:?} (premultiplied)",
                    r.bottom, r.top
                ));
            }
        }
    }
    assert_eq!(count, 45_900);
    assert!(
        failures.is_empty(),
        "{} mismatches (worst per mode: {:?}):\n{}",
        failures.len(),
        worst,
        failures[..failures.len().min(20)].join("\n")
    );
}

#[test]
fn every_mode_matches_the_reference_over_translucent_backdrops() {
    without_paper(PixelDepth::Rgba8);
}

/// The reference has no 16-bit documents; its 8-bit measurements are the
/// target at the same tolerance (as in 4E).
#[test]
fn every_mode_matches_the_reference_over_translucent_backdrops_in_16_bit_documents() {
    without_paper(PixelDepth::Rgba16);
}

/// The rule the reference does **not** follow, kept as a planted bug: the
/// W3C formula for every mode misses the data by far more than the
/// tolerance, so the test above can tell the rules apart.
#[test]
fn the_w3c_formula_does_not_explain_the_data() {
    let w3c = |b: PremulRgba, t: PremulRgba, opacity: f32, f: fn(f32, f32) -> f32| {
        let a_s = t.a * opacity;
        let ao = a_s + b.a * (1.0 - a_s);
        let un = |p: PremulRgba, c: f32| if p.a > 0.0 { c / p.a } else { 0.0 };
        let ch = |cb: f32, cs: f32| {
            let (cb, cs) = (un(b, cb), un(t, cs));
            a_s * ((1.0 - b.a) * cs + b.a * f(cb, cs)) + (1.0 - a_s) * b.a * cb
        };
        PremulRgba::new(ch(b.r, t.r), ch(b.g, t.g), ch(b.b, t.b), ao).to_rgba8()
    };
    let mut worst = 0u8;
    for ((mode, opacity), rows) in groups() {
        if mode != "lighten" {
            continue;
        }
        for r in rows.iter().filter(|r| r.top[3] > 0) {
            let p = |c| PremulRgba::from_rgba8(premultiplied(c));
            let got = w3c(p(r.bottom), p(r.top), f32::from(opacity) / 255.0, f32::max);
            let want = premultiplied(r.measured);
            worst = worst.max((0..3).map(|k| got[k].abs_diff(want[k])).max().unwrap());
        }
    }
    assert!(worst > 20, "W3C lighten is off by {worst}/255 only");
}
