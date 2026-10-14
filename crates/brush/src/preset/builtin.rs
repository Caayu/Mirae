//! Presets shipped with the engine, stored as ordinary preset files so the
//! format is exercised by its own first users.

use super::BrushPreset;
use super::io::from_json;

const FILES: [(&str, &str); 8] = [
    ("ink_pen", include_str!("../../presets/ink_pen.json")),
    ("pencil", include_str!("../../presets/pencil.json")),
    ("soft_round", include_str!("../../presets/soft_round.json")),
    ("marker", include_str!("../../presets/marker.json")),
    ("airbrush", include_str!("../../presets/airbrush.json")),
    ("chalk", include_str!("../../presets/chalk.json")),
    (
        "calligraphy",
        include_str!("../../presets/calligraphy.json"),
    ),
    ("eraser", include_str!("../../presets/eraser.json")),
];

/// The built-in presets, in display order. A file that fails to load is
/// skipped (and logged by the caller's tests); it can never crash the app.
pub fn builtin_presets() -> Vec<BrushPreset> {
    FILES
        .iter()
        .filter_map(|(_, text)| from_json(text).ok().map(|l| l.preset))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_builtin_loads_cleanly() {
        for (name, text) in FILES {
            let loaded = from_json(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(loaded.warnings.is_empty(), "{name}: {:?}", loaded.warnings);
        }
        assert_eq!(builtin_presets().len(), FILES.len());
    }

    #[test]
    fn ids_are_unique_and_namespaced() {
        let presets = builtin_presets();
        let ids: HashSet<_> = presets.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids.len(), presets.len());
        assert!(presets.iter().all(|p| p.id.starts_with("tela.")));
    }
}
