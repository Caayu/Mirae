# ADR 0008: Versioned JSON brush presets

Status: accepted (phase 3); reviewed in phase 3.5 (no breaking change).

# Context

Brushes must be data (editable, shareable, testable), not code. The format
will evolve; old files must keep loading, and files from newer versions
should load as well as possible. Presets are external input and must never
crash the app.

# Decision

- A preset is a plain `BrushPreset` struct serialized as JSON with
  `serde`. Top-level `format_version` (current: 1) and a stable `id`
  (`tela.*` for built-ins, constants in `preset::builtin::ids`).
- Loading pipeline (`preset::io::from_json`): parse to untyped JSON → read
  version → apply migrations `v → v+1` in order → deserialize (every struct
  `#[serde(default)]`) → `validate` (clamp/repair, return warnings).
- Forward compatibility: unknown fields ignored; unknown enum variants load
  as `Unknown` (neutral behaviour documented per type); unknown stroke
  overrides load as "no override".
- Built-in presets are ordinary JSON files embedded at build time
  (`crates/brush/presets/`), so the format is exercised by its first users.

# Consequences

- Malformed JSON is an error, anything else loads (invariants R1–R4).
- Saved presets round-trip textually exactly; validation is idempotent.
- Version-0 migration targets an invented draft layout only to exercise
  the migration path (there is no real v0 data).

## Phase 3.5 review

Checked: id stability, defaults, migration, Unknown handling, validation,
extensibility, round-trip, forward/backward compatibility. Findings:

| Finding | Action |
|---|---|
| A `format_version` above `u32::MAX` wrapped to 0 and ran the v0 migration on a "future" file. | Fixed: counts as newer (test `huge_versions_are_newer_not_version_zero`). |
| More than 64 effectors per property would share random streams with the next property. | Fixed: validation truncates with a warning; evaluation never reads past 64. |
| Built-in ids were repeated as strings in several crates. | Fixed: `preset::builtin::ids` + a test that each file declares its id. |
| `PaintMode::Unknown` paints (a future eraser variant would paint). | Kept and documented: safest neutral choice for unknown modes; a future format bump can add an explicit fallback field. |
| Embedded tip images are a JSON array of numbers (~4× larger than base64). | Kept: no user files exist; a v2 migration can switch to base64 without breaking v1 files. |
| Effectors reference properties by field name, sources by `kind` tag. | Kept: readable and extensible. |

No breaking change was justified, so `FORMAT_VERSION` stays 1.

# Alternatives considered

- Code-defined presets: not shareable, no user editing.
- Binary formats: compact but opaque; tooling and diffs suffer.
- TOML/RON: fine for hand editing, but JSON is universally supported and
  `serde_json::Value` makes migrations simple.
