//! Preset (de)serialization, versioning and migration.
//!
//! Loading is a pipeline over untyped JSON:
//!
//! ```text
//! parse -> read format_version -> migrate step by step to the current
//! version -> deserialize (missing fields take defaults) -> validate
//! ```
//!
//! Nothing in it panics: malformed JSON is an error, anything that parses
//! yields a usable preset plus warnings describing what was adapted.

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json};

use super::{BrushPreset, FORMAT_VERSION};

#[derive(Debug)]
pub enum PresetError {
    Json(serde_json::Error),
    NotAnObject,
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "invalid preset JSON: {e}"),
            Self::NotAnObject => write!(f, "a preset must be a JSON object"),
        }
    }
}

impl std::error::Error for PresetError {}

impl From<serde_json::Error> for PresetError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// A preset plus what had to be changed to load it.
#[derive(Debug)]
pub struct Loaded {
    pub preset: BrushPreset,
    pub warnings: Vec<String>,
}

pub fn to_json(preset: &BrushPreset) -> String {
    serde_json::to_string_pretty(preset).expect("presets always serialize")
}

pub fn from_json(text: &str) -> Result<Loaded, PresetError> {
    let mut value: Value = serde_json::from_str(text)?;
    let Value::Object(obj) = &mut value else {
        return Err(PresetError::NotAnObject);
    };
    let mut warnings = Vec::new();
    let version = obj
        .get("format_version")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    if version > FORMAT_VERSION {
        warnings.push(format!(
            "preset format {version} is newer than {FORMAT_VERSION}; unknown settings are ignored"
        ));
    }
    let mut v = version;
    while v < FORMAT_VERSION {
        MIGRATIONS[v as usize](obj, &mut warnings);
        v += 1;
    }
    obj.insert("format_version".into(), json!(FORMAT_VERSION));
    // A file without an id must not inherit the built-in default's id;
    // validation derives one from the name instead.
    obj.entry("id").or_insert(json!(""));
    let mut preset = BrushPreset::deserialize(value)?;
    warnings.extend(preset.validate());
    Ok(Loaded { preset, warnings })
}

/// `MIGRATIONS[v]` upgrades version `v` to `v + 1` in place.
type Migration = fn(&mut Map<String, Value>, &mut Vec<String>);

const MIGRATIONS: [Migration; FORMAT_VERSION as usize] = [migrate_0_to_1];

/// Version 0 is the flat, unversioned draft layout (top-level `size`,
/// `opacity`, `flow`, `hardness`, `spacing` numbers and
/// `pressure_size`/`pressure_opacity` flags). Version 1 nests parameters
/// under `properties` as base value + effectors.
fn migrate_0_to_1(obj: &mut Map<String, Value>, warnings: &mut Vec<String>) {
    let mut props = Map::new();
    for key in ["size", "opacity", "flow", "hardness", "spacing"] {
        if let Some(base) = obj.remove(key).and_then(|v| v.as_f64()) {
            props.insert(key.into(), json!({ "base": base }));
        }
    }
    for (flag, key) in [("pressure_size", "size"), ("pressure_opacity", "opacity")] {
        let on = obj.remove(flag).and_then(|v| v.as_bool()).unwrap_or(false);
        if on && let Some(Value::Object(p)) = props.get_mut(key) {
            p.insert(
                "effectors".into(),
                json!([{ "source": { "kind": "pressure" }, "curve": [[0.0, 0.0], [1.0, 1.0]] }]),
            );
        }
    }
    if !props.is_empty() {
        obj.insert("properties".into(), Value::Object(props));
    }
    warnings.push("migrated preset from format 0".into());
}

/// Deserializes an optional value, turning anything this version cannot
/// understand (an unknown enum name, a wrong type) into `None` instead of
/// failing the whole preset.
pub fn lenient<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let v = Value::deserialize(d)?;
    Ok(serde_json::from_value(v).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamics::{CombineMode, DynamicsSource};
    use crate::preset::{PaintMode, builtin_presets};
    use crate::tip::TipShape;

    #[test]
    fn round_trip_is_exact() {
        for preset in builtin_presets() {
            let json = to_json(&preset);
            let back = from_json(&json).unwrap();
            assert!(
                back.warnings.is_empty(),
                "{}: {:?}",
                preset.id,
                back.warnings
            );
            assert_eq!(back.preset, preset, "{}", preset.id);
            assert_eq!(to_json(&back.preset), json, "{}: stable text", preset.id);
        }
    }

    #[test]
    fn missing_fields_take_defaults() {
        let loaded = from_json(r#"{"format_version":1,"id":"x","name":"X"}"#).unwrap();
        assert!(loaded.warnings.is_empty());
        let default = BrushPreset::default();
        assert_eq!(loaded.preset.properties, default.properties);
        assert_eq!(loaded.preset.tip, default.tip);
    }

    #[test]
    fn version_zero_is_migrated() {
        let v0 =
            r#"{"name":"Old pen","size":30,"opacity":0.5,"hardness":0.2,"pressure_size":true}"#;
        let loaded = from_json(v0).unwrap();
        let p = &loaded.preset;
        assert_eq!(p.format_version, FORMAT_VERSION);
        assert_eq!(p.properties.size.base, 30.0);
        assert_eq!(p.properties.opacity.base, 0.5);
        assert_eq!(p.properties.size.effectors.len(), 1);
        assert_eq!(
            p.properties.size.effectors[0].source,
            DynamicsSource::Pressure
        );
        assert!(p.properties.opacity.effectors.is_empty());
        assert!(loaded.warnings.iter().any(|w| w.contains("format 0")));
        assert_eq!(p.id, "user.old-pen");
    }

    #[test]
    fn newer_versions_load_what_they_can() {
        let future = r#"{
            "format_version": 7,
            "id": "future.brush",
            "name": "From the future",
            "hologram": {"x": 1},
            "tip": {"shape": {"kind": "quantum_foam"}},
            "compositing": {"mode": "glaze"},
            "stroke": {"stabilizer": "neural", "stabilization": 55},
            "properties": {"size": {"base": 40, "effectors": [
                {"source": {"kind": "heart_rate"}, "mode": "resonate"},
                {"source": {"kind": "pressure"}, "mode": "multiply"}
            ]}}
        }"#;
        let loaded = from_json(future).unwrap();
        let p = &loaded.preset;
        assert!(loaded.warnings.iter().any(|w| w.contains("newer")));
        assert_eq!(p.tip.shape, TipShape::Unknown);
        assert_eq!(p.compositing.mode, PaintMode::Unknown);
        assert_eq!(p.stroke.stabilizer, None);
        assert_eq!(p.stroke.stabilization, Some(55.0));
        assert_eq!(
            p.properties.size.effectors[0].source,
            DynamicsSource::Unknown
        );
        assert_eq!(p.properties.size.effectors[0].mode, CombineMode::Unknown);
        assert_eq!(p.properties.size.base, 40.0);
    }

    #[test]
    fn broken_input_is_an_error_not_a_panic() {
        for bad in [
            "",
            "[]",
            "{",
            "42",
            r#"{"properties":{"size":"big"}}"#,
            r#"{"tip":7}"#,
        ] {
            assert!(from_json(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn out_of_range_values_are_repaired_with_warnings() {
        let loaded = from_json(
            r#"{"format_version":1,"id":"x","properties":{"opacity":{"base":7},"size":{"base":-3}},"scatter":{"count":500}}"#,
        )
        .unwrap();
        assert_eq!(loaded.preset.properties.opacity.base, 1.0);
        assert_eq!(loaded.preset.properties.size.base, 0.5);
        assert_eq!(loaded.preset.scatter.count, 16);
        assert_eq!(loaded.warnings.len(), 3, "{:?}", loaded.warnings);
    }
}
