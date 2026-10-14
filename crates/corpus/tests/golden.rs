//! Golden-image regression tests for every built-in preset.
//!
//! References live in `docs/phase3/corpus/<preset>.png`. Regenerate them
//! deliberately with:
//!
//! ```sh
//! TELA_UPDATE_GOLDEN=1 cargo test -p tela-corpus --test golden
//! ```
//!
//! Thresholds (see `Thresholds::default`): a pixel differs when any channel
//! moves by more than 2/255; a sheet passes when at most 0.1% of pixels
//! differ, the mean channel error is at most 0.5/255 and no channel moves by
//! more than 64/255.

use std::path::PathBuf;

use tela_brush::dynamics::BrushProperty;
use tela_brush::preset::builtin_presets;
use tela_corpus::{Image, Thresholds, render_sheet};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/phase3/corpus")
}

#[test]
fn presets_match_their_golden_images() {
    let update = std::env::var_os("TELA_UPDATE_GOLDEN").is_some();
    let thresholds = Thresholds::default();
    let mut failures = vec![];
    for preset in builtin_presets() {
        let name = preset.id.trim_start_matches("tela.").to_string();
        let path = golden_dir().join(format!("{name}.png"));
        let actual = render_sheet(&preset);
        if update {
            actual.save_png(&path).unwrap();
            continue;
        }
        let golden = Image::load_png(&path).unwrap_or_else(|e| {
            panic!(
                "missing golden {}: {e} (set TELA_UPDATE_GOLDEN=1)",
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
    let preset = builtin_presets()
        .into_iter()
        .find(|p| p.id == "tela.chalk")
        .unwrap();
    assert_eq!(render_sheet(&preset), render_sheet(&preset));
}

#[test]
fn a_visible_change_fails_the_thresholds() {
    let preset = builtin_presets()
        .into_iter()
        .find(|p| p.id == "tela.ink-pen")
        .unwrap();
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
