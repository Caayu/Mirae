//! Golden-image regression tests for every built-in preset.
//!
//! References live in `crates/corpus/golden/<preset>.png`. Regenerate them
//! deliberately (only the exact value `1` is honoured) with:
//!
//! ```sh
//! TELA_UPDATE_GOLDEN=1 cargo test -p tela-corpus --test golden
//! ```
//!
//! See docs/golden-tests.md.
//!
//! Thresholds (see `Thresholds::default`): a pixel differs when any channel
//! moves by more than 2/255; a sheet passes when at most 0.1% of pixels
//! differ, the mean channel error is at most 0.5/255 and no channel moves by
//! more than 64/255.

use std::collections::BTreeSet;

use tela_brush::dynamics::BrushProperty;
use tela_brush::preset::builtin::ids;
use tela_brush::preset::{builtin_preset, builtin_presets};
use tela_corpus::{
    Image, Thresholds, golden_dir, golden_file_name, render_sheet, update_golden_requested,
};

#[test]
fn presets_match_their_golden_images() {
    let update = update_golden_requested();
    let thresholds = Thresholds::default();
    let mut failures = vec![];
    for preset in builtin_presets() {
        let file = golden_file_name(&preset);
        let name = file.trim_end_matches(".png");
        let path = golden_dir().join(&file);
        let actual = render_sheet(&preset);
        if update {
            actual.save_png(&path).unwrap();
            eprintln!("updated golden {}", path.display());
            continue;
        }
        let golden = Image::load_png(&path).unwrap_or_else(|e| {
            panic!(
                "missing golden {}: {e} (generate it with TELA_UPDATE_GOLDEN=1)",
                path.display()
            )
        });
        match actual.diff(&golden, thresholds.pixel_tolerance) {
            Some(d) if thresholds.accepts(&d) => {}
            Some(d) => {
                let out = std::env::temp_dir().join(format!("tela-golden-actual-{name}.png"));
                let _ = actual.save_png(&out);
                failures.push(format!(
                    "{name}: {d:?} (actual written to {})",
                    out.display()
                ));
            }
            None => failures.push(format!("{name}: size changed")),
        }
    }
    assert!(
        failures.is_empty(),
        "golden mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn rendering_is_deterministic() {
    let preset = builtin_preset(ids::CHALK).unwrap();
    assert_eq!(render_sheet(&preset), render_sheet(&preset));
}

#[test]
fn a_visible_change_fails_the_thresholds() {
    let preset = builtin_preset(ids::INK_PEN).unwrap();
    let base = render_sheet(&preset);
    let mut bigger = preset.clone();
    let size = &mut bigger.properties.size;
    *size = BrushProperty {
        base: size.base * 1.1,
        ..size.clone()
    };
    let d = render_sheet(&bigger).diff(&base, 2).unwrap();
    assert!(
        !Thresholds::default().accepts(&d),
        "10% larger brush must fail: {d:?}"
    );
}

#[test]
fn golden_directory_matches_the_builtin_presets() {
    // An orphaned reference (renamed or removed preset) would otherwise sit
    // in the repository unchecked forever.
    let expected: BTreeSet<String> = builtin_presets().iter().map(golden_file_name).collect();
    let present: BTreeSet<String> = std::fs::read_dir(golden_dir())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".png"))
        .collect();
    assert_eq!(present, expected);
}
