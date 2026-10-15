//! Blend-mode parity with the reference application (phase 4C gate).
//!
//! The CSV files in `tests/reference/` are black-box measurements of the
//! reference for 3 400 combinations each (see the README there). These
//! tests put the same inputs through Tela and require every result to be
//! within 1/255 of the reference (2/255 for `Color` and `Luminosity`,
//! whose luma weights amplify rounding).

use std::collections::BTreeMap;

use tela_canvas::{DirtyTiles, TileCoord};
use tela_color::{PremulRgba, Rgba};
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

/// Straight 8-bit RGBA as Tela stores it: premultiplied, 8-bit.
fn stored(c: [u8; 4]) -> [u8; 4] {
    Rgba::new(
        f32::from(c[0]) / 255.0,
        f32::from(c[1]) / 255.0,
        f32::from(c[2]) / 255.0,
        f32::from(c[3]) / 255.0,
    )
    .premultiply()
    .to_rgba8()
}

fn set(d: &mut Document, layer: tela_document::LayerId, c: [u8; 4]) {
    let mut t = DirtyTiles::new();
    t.mark(TileCoord { x: 0, y: 0 });
    d.modify_tiles(layer, &t, |_, tile| *tile.get_mut(0, 0) = stored(c))
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
    let mut worst: BTreeMap<&'static str, u8> = BTreeMap::new();
    let mut failures = Vec::new();
    for r in rows("blend_with_paper.csv") {
        // The reference's document: white paper, a Normal base layer and
        // the probed layer on top.
        let mut d = Document::new(1, 1);
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
        if err > tolerance(r.mode) {
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
fn formula_matches_the_reference_without_paper_except_over_translucent_backdrops() {
    // Without a paper layer the backdrop can be translucent. Over opaque
    // or fully transparent backdrops the reference follows the same
    // formula; over translucent ones it does not (a known, recorded
    // difference — docs/parity/4C-blend.md, D-4C-1), so those rows are
    // only counted here.
    let mut translucent = 0;
    for r in rows("blend_without_paper.csv") {
        if r.bottom[3] != 0 && r.bottom[3] != 255 {
            translucent += 1;
            continue;
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
