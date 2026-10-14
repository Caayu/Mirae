//! Presets shipped with the engine, stored as ordinary preset files
//! (`crates/brush/presets/*.json`, embedded at build time) so the format is
//! exercised by its own first users.
//!
//! Adding one: write the JSON file, add its id to [`ids`] and a row to
//! `FILES`, then generate its golden image (see `docs/golden-tests.md`).

use super::BrushPreset;
use super::io::from_json;

/// Stable ids of the built-in presets. An id is never reused for a
/// different brush; documents and settings may refer to it.
pub mod ids {
    pub const INK_PEN: &str = "tela.ink-pen";
    pub const PENCIL: &str = "tela.pencil";
    pub const SOFT_ROUND: &str = "tela.soft-round";
    pub const MARKER: &str = "tela.marker";
    pub const AIRBRUSH: &str = "tela.airbrush";
    pub const CHALK: &str = "tela.chalk";
    pub const CALLIGRAPHY: &str = "tela.calligraphy";
    pub const ERASER: &str = "tela.eraser";
}

/// `(id, file)` in display order; the id must match the file's `id`.
const FILES: [(&str, &str); 8] = [
    (ids::INK_PEN, include_str!("../../presets/ink_pen.json")),
    (ids::PENCIL, include_str!("../../presets/pencil.json")),
    (
        ids::SOFT_ROUND,
        include_str!("../../presets/soft_round.json"),
    ),
    (ids::MARKER, include_str!("../../presets/marker.json")),
    (ids::AIRBRUSH, include_str!("../../presets/airbrush.json")),
    (ids::CHALK, include_str!("../../presets/chalk.json")),
    (
        ids::CALLIGRAPHY,
        include_str!("../../presets/calligraphy.json"),
    ),
    (ids::ERASER, include_str!("../../presets/eraser.json")),
];

/// The built-in presets, in display order. A file that fails to load is
/// skipped (and logged by the caller's tests); it can never crash the app.
pub fn builtin_presets() -> Vec<BrushPreset> {
    FILES
        .iter()
        .filter_map(|(_, text)| from_json(text).ok().map(|l| l.preset))
        .collect()
}

/// One built-in preset by id.
pub fn builtin_preset(id: &str) -> Option<BrushPreset> {
    FILES
        .iter()
        .find(|(file_id, _)| *file_id == id)
        .and_then(|(_, text)| from_json(text).ok())
        .map(|l| l.preset)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_builtin_loads_cleanly() {
        for (id, text) in FILES {
            let loaded = from_json(text).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(loaded.warnings.is_empty(), "{id}: {:?}", loaded.warnings);
            assert_eq!(loaded.preset.id, id, "declared id matches the file");
            assert_eq!(builtin_preset(id).map(|p| p.id), Some(id.to_string()));
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
