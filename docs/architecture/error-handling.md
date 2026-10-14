# Error handling

## Policy

| Situation | Handling |
|---|---|
| Invalid **external** input (preset files, device data, environment) | Never panic. Return a typed error, or repair and report (warnings). |
| Fallible **system** operations (GPU, window, Ink attach, file I/O) | Typed error; the app logs it and degrades (no pen → mouse; no GPU → exit cleanly with a message). |
| Violated **internal** invariant (a bug) | `expect("why this cannot fail")` / indexing panic is acceptable. The message names the invariant. |
| Tests, examples, benchmarks | `unwrap` is fine. |

No library crate uses `anyhow`, `thiserror` or `Box<dyn Error>`: each error
type is a small enum with a hand-written `Display` and `std::error::Error`.
The app's `main` returns `Result<(), Box<dyn Error>>` for event-loop errors.
`thiserror` would only remove boilerplate from four short impls; it is not
worth a dependency today.

## Error types

| Type | Crate | Variants | Raised by |
|---|---|---|---|
| `PresetError` | brush | `Json(serde_json::Error)`, `NotAnObject` | `preset::io::from_json` when the text is not JSON or not an object, or a field has an incompatible type |
| `RendererError` | renderer | `Surface`, `Adapter`, `Device`, `UnsupportedSurface`, `DocumentTooLarge { width, height, max }` | `Renderer::new`, `set_document_size` (`new_headless` returns `None` without a GPU, so GPU tests skip) |
| `InkError` | tablet (Windows) | `UnsupportedWindow`, `Subclass` | `InkBackend::attach` |
| `std::io::Error` | corpus | — | PNG load/save |

## Repair instead of failure

Presets are the main external input. Anything that parses as a JSON object
yields a usable preset plus `warnings: Vec<String>`:

- missing fields take defaults (`#[serde(default)]` everywhere);
- unknown fields are ignored; unknown enum variants load as `Unknown`, which
  every consumer treats as neutral (documented per type);
- out-of-range or non-finite numbers are clamped or replaced
  (`BrushPreset::validate`);
- more than `BrushProperty::MAX_EFFECTORS` effectors are truncated;
- an invalid embedded tip image falls back to a round tip;
- stroke overrides of an unknown kind load as "no override" (`lenient`).

Device input is normalized defensively (`ink_pressure` clamps, tilt is
clamped to ±90°, rotation wraps; timestamps are forced strictly increasing;
HIMETRIC mapping falls back to pixel positions when it disagrees).
Numerical code guards division and non-finite values (curves sanitize,
`BrushProperty::eval` falls back to `base` on non-finite results, airbrush
clamps clock jumps).

## Remaining `expect` in library code (all internal invariants)

| Location | Invariant |
|---|---|
| `brush/src/preset/io.rs` `to_json` | Presets contain only serializable types (non-finite floats serialize as `null`). |
| `brush/src/stroke.rs` `stamp_into` | The tile was inserted on the line above. |
| `brush/src/tip/image.rs` mip build | The level list starts non-empty. |
| `corpus/src/lib.rs` `flatten` | The coordinate came from `tile_coords()`. |
| `document` `active_layer_mut` (indexing) | `active_layer` is in range (public field; phase 4 will encapsulate it). |

## Adding error handling

- New external input → typed error or repair-with-warning, plus a test that
  feeds it garbage (see `broken_input_is_an_error_not_a_panic` and the
  property tests).
- Do not use `unwrap` on anything derived from a file, a device or the user.
- Logging (`log::warn!`) is for degraded-but-working states; errors that
  stop a feature go through `Result`.
