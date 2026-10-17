//! Blend-mode parity with the reference application (phase 4C gate).
//!
//! The CSV files in `tests/reference/` are black-box measurements of the
//! reference for 3 400 combinations each (see the README there). These
//! tests put the same inputs through Tela and require every result to be
//! within 1/255 of the reference (2/255 for `Color` and `Luminosity`,
//! whose luma weights amplify rounding).

use std::collections::BTreeMap;

use tela_canvas::{DirtyTiles, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, Rgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, Document, Placement};

struct Row {
    mode: BlendMode,
    opacity: f32,
    bottom: [u8; 4],
    top: [u8; 4],
    measured: [u8; 4],
}

fn rows(file: &str) -> Vec<Row> {
    let path = format!("{}/tests/reference/{file}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("reference CSV");
    text.lines()
        .skip(1)
        .map(|line| {
            let v: Vec<&str> = line.split(',').collect();
            let n = |i: usize| v[i].parse::<u8>().unwrap();
            Row {
                mode: *BlendMode::ALL
                    .iter()
                    .find(|m| m.name() == v[0])
                    .unwrap_or_else(|| panic!("unknown mode {}", v[0])),
                opacity: f32::from(n(1)) / 255.0,
                bottom: [n(4), n(5), n(6), n(7)],
                top: [n(8), n(9), n(10), n(11)],
                measured: [n(12), n(13), n(14), n(15)],
            }
        })
        .collect()
}

fn tolerance(mode: BlendMode) -> u8 {
    match mode {
        BlendMode::Color | BlendMode::Luminosity => 2,
        _ => 1,
    }
}

/// In 16-bit documents the stored colours carry less rounding than the
/// reference's 8-bit ones, and its measurements include its own 8-bit
/// rounding: over the translucent teal (α = 128) Multiply and Color Burn
/// land 2/255 from the reference (3 of 3 400 rows), as the unrounded
/// formula does (`blend_fit`, docs/parity/4C-blend.md). Parity record 4E.
fn tolerance_at(mode: BlendMode, depth: PixelDepth) -> u8 {
    match depth {
        PixelDepth::Rgba8 => tolerance(mode),
        PixelDepth::Rgba16 => 2,
    }
}

/// Straight 8-bit RGBA as Tela stores it: premultiplied, at the
/// storage's precision (8-bit: rounded to 8 bits after premultiplying,
/// as the reference's 8-bit documents hold it; 16-bit: to 16 bits).
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

fn set(d: &mut Document, layer: tela_document::LayerId, c: [u8; 4]) {
    let mut t = DirtyTiles::new();
    t.mark(TileCoord { x: 0, y: 0 });
    with_pixel_type!(d.depth(), P => d.modify_tiles(layer, &t, |_, tile: &mut Tile<P>| {
        *tile.get_mut(0, 0) = stored::<P>(c)
    }))
    .unwrap();
}

/// Worst error per mode, for the failure message.
fn summary(errors: &BTreeMap<&'static str, u8>) -> String {
    errors
        .iter()
        .map(|(m, e)| format!("{m}={e}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn every_mode_matches_the_reference_over_its_paper() {
    over_paper(PixelDepth::Rgba8);
}

/// Phase 4E: the same probe in a 16-bit document, the colours stored at
/// 16-bit precision. The reference has no 16-bit documents (parity record
/// 4E, E1), so its 8-bit measurements are the target; the tolerance is
/// unchanged.
#[test]
fn every_mode_matches_the_reference_over_its_paper_in_16_bit_documents() {
    over_paper(PixelDepth::Rgba16);
}

fn over_paper(depth: PixelDepth) {
    let mut worst: BTreeMap<&'static str, u8> = BTreeMap::new();
    let mut failures = Vec::new();
    for r in rows("blend_with_paper.csv") {
        // The reference's document: white paper, a Normal base layer and
        // the probed layer on top.
        let mut d = Document::with_depth(1, 1, depth);
        let base = d.active_layer();
        set(&mut d, base, r.bottom);
        let top = d.add_layer(Placement::Top).unwrap();
        set(&mut d, top, r.top);
        d.set_opacity(top, r.opacity).unwrap();
        d.set_blend_mode(top, r.mode).unwrap();
        let tile = d.compose_tile(TileCoord { x: 0, y: 0 }, None);
        // Tiles without any content compose to nothing (the display shows
        // the paper): equivalent to white.
        let px = tile.map_or([255; 4], |t| t[0]);
        // Normal-only tiles are composed over transparency; the display
        // puts the paper under them. Do the same here.
        let shown = PremulRgba::from_rgba8(px)
            .over(Rgba::WHITE.premultiply())
            .to_rgba8();
        let err = (0..3)
            .map(|i| shown[i].abs_diff(r.measured[i]))
            .max()
            .unwrap();
        let e = worst.entry(r.mode.name()).or_default();
        *e = (*e).max(err);
        if err > tolerance_at(r.mode, depth) {
            failures.push(format!(
                "{:?} op {:.2} bottom {:?} top {:?}: tela {:?} reference {:?}",
                r.mode, r.opacity, r.bottom, r.top, shown, r.measured
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches (worst per mode: {}):\n{}",
        failures.len(),
        summary(&worst),
        failures[..failures.len().min(20)].join("\n")
    );
}

#[test]
fn every_mode_matches_the_reference_without_paper() {
    // Without a paper layer the backdrop can be translucent. Until 5B the
    // translucent rows (the teal swatch at alpha 128) were only counted:
    // the reference does not follow the W3C formula there (D-4C-1). The
    // rule measured in 5B (ADR 0022, `backdrop_parity.rs`) reproduces
    // them too.
    let mut translucent = 0;
    for r in rows("blend_without_paper.csv") {
        if r.bottom[3] != 0 && r.bottom[3] != 255 {
            translucent += 1;
        }
        let premul = |c: [u8; 4]| PremulRgba::from_rgba8(stored(c));
        let mut d = [premul(r.bottom)];
        r.mode.composite(&[premul(r.top)], r.opacity, &mut d);
        let got = d[0].to_rgba8();
        if got[3] == 0 {
            assert_eq!(r.measured[3], 0);
            continue;
        }
        let s = d[0].unpremultiply();
        let straight = [s.r, s.g, s.b].map(|v| (v * 255.0).round() as u8);
        for i in 0..3 {
            assert!(
                straight[i].abs_diff(r.measured[i]) <= tolerance(r.mode) + 1,
                "{:?} op {} bottom {:?} top {:?}: tela {:?} reference {:?}",
                r.mode,
                r.opacity,
                r.bottom,
                r.top,
                straight,
                r.measured
            );
        }
        assert!(got[3].abs_diff(r.measured[3]) <= 1, "alpha");
    }
    assert_eq!(translucent, 17 * 2 * 10, "one translucent bottom swatch");
}
