# Technical debt

Known, real debt at the end of phase 3.5. Each item: impact, priority,
why it is not fixed now, and a possible direction. Priorities: **P1** blocks
a planned phase or affects users soon; **P2** visible quality issue;
**P3** latent or cosmetic.

| # | Item | Impact | Priority | Why not now | Direction |
|---|---|---|---|---|---|
| 1 | **No physical tablet validated.** Only a virtual HID pen exists on the dev machine; tests use synthetic Windows Ink injection with integer positions. | Real pens may differ in pressure response, tilt/rotation, timestamps, delivery batching; HIMETRIC sub-pixel mapping is untested. | P1 | No hardware available. | Test on at least one Wacom-class and one N-trig/Surface pen; record device/driver/results in STATUS.md; add Wintab if Ink proves insufficient. |
| 2 | **8-bit layer storage** (premultiplied RGBA8). | Banding in soft, low-flow build-ups and dark low-alpha colors. | P1 (phase 4) | Changes storage, composition and memory budget; belongs with the layer work. | 16-bit (u16 linear or encoded) tiles, at least as an option; benchmark memory and compose cost. |
| 3 | **Document limited by GPU max texture size** (single display texture). | Documents beyond 8192/16384 px cannot be shown. | P2 | Typical illustration sizes fit. | Page the display texture (virtual texture / tiles of display textures) while keeping mip continuity. |
| 4 | **No undo/redo.** | Any mistake is permanent. | P1 (phase 4) | Phase 4 scope. | Copy-on-write tile snapshots of `StrokeBuffer::touched()`; memory-bounded history. |
| 5 | **Airbrush rest position lags one sample** (uses the last processed path point). | Slight offset of rest build-up after a fast stop. | P3 | Visually negligible at 200 Hz. | Use the raw last input position for rest deposition, filtered for jitter. |
| 6 | **`distance` effector uses a fixed period `length`**, not the stroke's final length (unknown while drawing). | "Fade over the whole stroke" is not expressible. | P3 | Needs a design for live strokes (the end is unknown). | Optional "fit to stroke" mode evaluated in the provisional tail, like the end taper. |
| 7 | **No movement prediction.** | Perceived latency ~1 sample + display. | P3 | Correctness first; prediction must be retractable. | Predict into the re-renderable tail only (never committed). |
| 8 | **UI edits are not persisted**; preset changes are lost on exit. | Users cannot keep customized brushes. | P2 | Needs settings/document I/O (phase 4+). | Save user presets as JSON (format exists) in a user directory; "reset to built-in". |
| 9 | **Goldens verified on one machine only.** | Different CPUs/compilers might exceed tolerances via float differences. | P3 | No second platform available. | Run the corpus in CI on another OS/CPU; widen tolerances only with evidence. |
| 10 | **`Document` exposes public fields** (`layers`, `active_layer`); an out-of-range `active_layer` panics. | Easy to break invariants from the app. | P2 (phase 4) | Layer API is phase 4's main design task. | Encapsulate the layer stack behind an API with stable layer ids. |
| 11 | **Embedded tip images are JSON number arrays.** | Presets with image tips are ~4× larger than necessary. | P3 | No user files exist yet. | Base64 in format v2 with a v1→v2 migration. |
| 12 | **`missing_docs` not enforced**; ~197 items (mostly self-describing constructors, enum variants, test-support types) lack doc comments. | New public API may go undocumented. | P3 | Enforcing now would add noise docs. | Enable `missing_docs` per crate as each crate's API stabilizes (color, canvas first). |
| 13 | **Curved interpolation runs one sample behind**; stroke ends use a mirrored phantom neighbour. | ~4 ms extra latency at 240 Hz; slightly flatter last segment. | P3 | Inherent to the chosen spline. | Combine with prediction (item 7). |
| 14 | **Pixels painted outside the document are kept** in layers (clipped at composition). | Memory for off-canvas paint; surprising on canvas resize. | P3 | Harmless today. | Clip at commit, or define canvas-resize semantics in phase 4. |
| 15 | **Mouse samples have no device timestamps** (stamped on arrival). | Velocity-driven effectors are noisier with a mouse. | P3 | winit does not expose them. | Use raw input timestamps on Windows if needed. |
| 16 | **Large modules**: `brush/src/engine.rs` (~800 lines incl. tests) holds `StrokeProcessor` and `BrushEngine`. | Harder to navigate. | P3 | Cohesive (one pipeline); splitting now is cosmetic. | Split `BrushEngine` into its own module if it grows (e.g. with undo hooks). |

Resolved in phase 3.5 (no longer debt): raw HWND in the Ink API; unit
conversion functions named after their input; duplicated stroke-override
rule in app and UI; magic preset id strings; golden updates triggered by
any value of the variable; effector stream overlap beyond 64 effectors;
`format_version` overflow; unenforced crate boundaries.
