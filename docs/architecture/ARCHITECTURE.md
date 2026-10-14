# Tela architecture

Tela is a raster illustration editor written from scratch in Rust (wgpu +
winit), developed clean-room ([CLEAN_ROOM.md](../CLEAN_ROOM.md)). This is the
main technical document: what the parts are, how data flows, and which
boundaries and invariants hold. Companion documents:

| Topic | Document |
|---|---|
| Crate graph, allowed/forbidden dependencies | [crate-dependencies.md](crate-dependencies.md) |
| Invariants and the tests that protect them | [invariants.md](invariants.md) |
| Threads and parallelism | [concurrency.md](concurrency.md) |
| `unsafe` audit | [unsafe.md](unsafe.md) |
| Error policy | [error-handling.md](error-handling.md) |
| UI boundary | [ui.md](ui.md) |
| Decisions and their reasons | [../adr/](../adr/README.md) |
| Hot paths and numbers | [../performance.md](../performance.md) |
| Tests, goldens | [../testing.md](../testing.md), [../golden-tests.md](../golden-tests.md) |

## 1. Workspace

```
paint/
  Cargo.toml            workspace: shared versions, lints, rust-version
  crates/
    color/              color types, premultiplied alpha, sRGB
    canvas/             sparse tiles, dirty sets, rectangles, view transform
    brush/              brush engine (presets, dynamics, pipeline, tips,
                        textures, scatter, airbrush, stroke buffer)
      presets/*.json    built-in presets (embedded at build time)
    document/           layers and per-tile composition
    renderer/           wgpu display texture, mips, overlay hook
    tablet/             input samples, Windows Ink backend, input clock
    ui/                 egui panel and curve editor
    corpus/             trajectories, golden sheets, benchmark (test support)
      golden/           golden reference PNGs
    app/                the `tela` binary: composition layer
  docs/                 architecture, ADRs, phase reports, status
  scripts/check.ps1     local verification gate
```

## 2. Crates and responsibilities

| Crate | Owns | Must not |
|---|---|---|
| `tela-color` | `Rgba` (straight), `PremulRgba`, `over`, RGBA8 quantization, sRGB curves | know about space or devices |
| `tela-canvas` | `TiledSurface<P>`, `Tile<P>`, `TileCoord`, `DirtyTiles`, `IRect`, `ViewTransform` | know about layers, colors or brushes |
| `tela-brush` | the whole stroke engine up to the stroke mask | read devices, own documents, know UI/GPU |
| `tela-document` | authoritative pixels and the layer stack: `Document` (layer ids, rules, the only pixel-mutation path), `RasterLayer`, composition, `Overlay` | know brushes, input, view, GPU |
| `tela-renderer` | GPU device/surface, display texture + mips, canvas shader, `FrameOverlay` | see the document |
| `tela-tablet` | `TabletSample`/`TabletEvent`, unit normalization, input clock, Windows Ink | know brushes or canvas space |
| `tela-ui` | brush panel, curve editor, egui plumbing | own authoritative state |
| `tela-corpus` | standard trajectories, `render_sheet`, image diff, goldens, `stamp_bench` | ship in the app |
| `tela-app` | window/event loop, wiring, application state | implement engine logic |

The boundaries are enforced by a test (see crate-dependencies.md).

## 3. Ownership of state

All authoritative state lives in `tela-app`'s `App`:

| State | Owner | Notes |
|---|---|---|
| `Document` (layers, pixels) | `App.document` | Changed only through `Document` methods (ADR 0014): layer commands from the panel and shortcuts, `modify_tiles` + `StrokeBuffer::commit_tile` at the end of a stroke, `clear_layer`. Every call returns the tiles it invalidated. |
| View (`ViewTransform`) | `App.view` | Screen↔canvas mapping. |
| Presets being edited, selection, eraser mode | `App.tools` (`Tools`) | The UI edits the active preset in place, lent for one frame. |
| User stroke settings, pressure curve, paint color | `App.engine` (`BrushEngine` public fields) | Read when a stroke begins. |
| Stroke in progress | `BrushEngine` (its `StrokeBuffer`) | Separate from the layer until committed. |
| GPU resources | `Renderer`, `UiLayer` | Derived; rebuilt from the document. |
| Pen queue | `InkBackend` (`Rc<Shared>`) | Drained by the app each event. |

The engine copies the preset and pressure curve when a stroke begins
(`StrokeProcessor::begin_with`), so edits during a stroke apply to the next
one. The renderer's texture is a cache of the composed document.

## 4. Input flow

```
Windows Ink (WM_POINTER*, window subclass)      winit mouse events
  GetPointerPenInfoHistory (full 200+ Hz)         CursorMoved / MouseInput
  HIMETRIC -> sub-pixel screen position           TabletSample::mouse(pos, clock::now())
  QPC timestamps, pressure/tilt/rotation masks
          │ queue (RefCell<Vec>) + waker -> AppEvent::Tablet
          ▼                                              │
     App::drain_tablet ──────────────► App::on_pointer ◄─┘    (one path for both)
                                          │ UI passthrough: pen contacts that start
                                          │ over the panel go to Windows as mouse
                                          ▼
     TimestampNormalizer (strictly increasing per stroke)
     ViewTransform::to_canvas (screen -> canvas px), tilt rotated into canvas axes
          ▼
     BrushInput { position (canvas px), pressure 0..1, tilt (rad), rotation (rad),
                  timestamp (s), caps }
          ▼
     BrushEngine::begin_stroke / add_input_at(input, now) / end_stroke
```

Units: tablet positions are window client **physical pixels**; the brush
sees **canvas pixels**; distances that model the hand (jitter threshold,
stabilizer, speed) are measured in **screen pixels** by passing
`scale = view.zoom` into each stage. Times are **seconds** (f64) on
`tela_tablet::clock::now` (QPC on Windows). Angles are **radians**
everywhere at runtime; presets store **degrees** for human-edited angles
(tip angle, texture rotation, rotation jitter). Pressure and other
normalized values are `0..=1`.

## 5. Brush pipeline

```
BrushInput
  -> device pressure curve                     (Curve, global calibration)
  -> VelocityEstimator                         (canvas px/s, time-constant EMA)
  -> JitterFilter                              (drops sub-threshold motion, screen px)
  -> Stabilizer (none | moving average | exponential | One Euro*)
  -> Interpolator (linear | centripetal Catmull-Rom* | smoothed Bézier)
       flattened to chords of ~1.5 screen px (0.1..1 canvas px)
  -> DabSpacer: arc length carried across segments;
       next dab at spacing × tip extent along the direction of travel
  -> per dab: DynamicsInput -> BrushProperty::eval (base ∘ effectors)
       taper factor, resolve_dab -> Dab
  -> ScatterStyle::apply (after spacing; count, displacement, jitter)
  -> airbrush time instants (if enabled)      (t0 + k / rate)
  -> DabSink (StrokeBuffer)                    (* = default)
```

All stages are small state machines that write into caller-owned buffers;
`StrokeProcessor` owns and reuses them, so no allocation happens per sample
or per dab once buffers have grown (measured, see performance.md).

### Stroke lifecycle

1. `begin_stroke(input, scale, seed)`: copies preset and pressure curve,
   resolves stroke settings (preset overrides over user settings), resets all
   stages, seeds `StrokeRng`, creates a `StrokeBuffer` with a `StampContext`
   (prepared tip, texture, anti-aliasing, paint/erase, parallel).
2. `add_input_at(input, now)`: processes the sample; dabs further than
   `taper.end` px behind the head are **committed**; the rest form the
   provisional **tail**. `now` measures delivery latency for the airbrush.
3. Every frame: `advance_time(now)` (airbrush rest deposition) and
   `update_preview()` (rebuild the tail as if the pen lifted now).
4. `end_stroke()`: flushes filters, lets the stabilizer catch up to the last
   real sample, emits the final tail and commits it; returns the
   `StrokeBuffer`.
5. The app commits into the layer it fixed when the stroke began:
   `document.modify_tiles(target, stroke.touched(), |c, t| stroke.commit_tile(c, t))`,
   and marks the returned tiles stale. Strokes do not start on hidden or
   locked layers (`Document::paint_target`), and layer commands are ignored
   while a stroke is in progress.

### Stroke buffer

A stroke is an f32 alpha mask in its own sparse `TiledSurface`, separate
from the layer (ADR 0003). A dab pulls each covered pixel toward its opacity
ceiling: `if target > m { m += (target − m) · flow · coverage }`, where
`target` is the dab opacity, modulated by the paper texture if any. Alpha
never exceeds the largest opacity that touched a pixel; flow controls the
build-up. The tail lives in copy-on-write tiles seeded from the committed
mask, so it can be rebuilt every frame. Paint composites as premultiplied
`color·m over dst`; erase as destination-out `dst·(1 − m)`. Preview and commit
call the same `paint_over_tile` (ADR 0003, invariants).

## 6. Presets, effectors, tips, textures, scatter, airbrush

**Presets** (`brush::preset`, ADR 0008): plain data, versioned JSON.
Loading: parse → read `format_version` → migrate step by step → deserialize
(missing fields default) → validate (clamp, repair, warn). Never panics.
Built-ins live in `crates/brush/presets/*.json` with stable ids in
`preset::builtin::ids`.

**Effectors** (`brush::dynamics`): each property is
`v = base; for e in effectors: v = e.mode.apply(v, min + (max − min)·curve(input))`.
Sources: pressure, speed, tilt, tilt direction, barrel rotation, stroke
direction, distance, dab index, random, noise. Modes: multiply, add,
replace, min, max, follow (angles; blended in multiplied-angle space for
symmetric tips). A source the device lacks is skipped (neutral). Curves are
monotone cubic Hermite (Fritsch–Carlson). At most 64 effectors per property
(each owns a random stream).

**Tips** (`brush::tip`): evaluated per pixel in dab-local coordinates
(rotation, radii from size × aspect, flips). Procedural round/rectangle
with hardness falloff and analytic AA; image tips are grayscale with a
box-filtered mip pyramid sampled trilinearly. Built-in images (speckle,
bristle, charcoal) are generated by code.

**Textures** (`brush::texture`, ADR 0009): periodic height fields generated
from periodic gradient noise, sampled in canvas space, modulating the
opacity ceiling (multiply / subtract / height).

**Scatter** (`brush::scatter`): applied after spacing is decided, so the
number of dabs per length never depends on the scatter amount.

**Airbrush** (`brush::airbrush`, crate-private): dabs at instants
`t0 + k/rate`, positioned by interpolating the processed path in time; the
dab list is identical for any frame rate. Resting is inferred from input
motion (beyond the rest threshold) plus a 30 ms hold delay plus the measured
delivery lag. Moving dabs scale flow by dwell time
`flow' = 1 − (1 − flow)^min(step/speed · rate, 1)`. Details in
`docs/phase3/README.md`.

**Randomness** (`brush::rng`, ADR 0007): `StrokeRng` is counter-based:
`value = f(seed, stream, index)`, no hidden state. Streams separate
consumers (scatter, each effector of each property, noise); airbrush dabs
use a separate index range (`1 << 40`).

## 7. Tiles and dirty tracking

`TiledSurface<P>` is a `HashMap<TileCoord, Tile<P>>` of 256×256 tiles,
allocated on first write (ADR 0002). `DirtyTiles` is a `BTreeSet` (ordered,
deterministic). The stroke buffer keeps two sets: `dirty` (changed since the
display last took them) and `touched` (every tile of the stroke, for commit
and future undo). The app keeps `stale` (tiles whose composite must be
rebuilt) and recomposes only those.

## 8. Composition and rendering

```
App::redraw (one frame)
  engine.advance_time(now)           airbrush rest deposition
  engine.update_preview()            rebuild provisional tail
  stale += stroke.take_dirty()
  for tile in stale:
     document.compose_tile_into(tile, overlay = stroke preview, scratch)
        layers bottom->top, premultiplied f32; the stroke is painted onto its
        target layer before that layer's opacity (preview == commit)
     renderer.upload_tile(tile, rgba8)  -> CPU mip chain -> display texture
  ui.build(...)                      egui panel, returns occupied rects
  renderer.render_with(frame, overlay = UiLayer)
```

Composition runs on the CPU per dirty tile into reusable `ComposeScratch`
buffers (no allocation). The renderer keeps the whole document in **one**
texture with mips generated on the CPU per tile (power-of-two aligned, so
tiles update independently). The fragment shader uses trilinear sampling
when minifying and anti-aliased nearest when magnifying, then composites
over the paper color (ADR 0004). The renderer only receives finished tiles
and a `ViewTransform`; it cannot reach the document.

## 9. UI overlay

`tela-ui::UiLayer` runs egui each frame and implements the renderer's
`FrameOverlay`, drawing after the canvas pass. The panel edits data lent
through `PanelModel` and returns a `PanelAction` for things only the app
can do (select preset, toggle eraser, clear layer). The layer panel on the
right receives one `LayerRow` per layer and returns a `LayerAction`; it
never sees the `Document`. The UI reports the
rectangles it covers; the app forwards them to the Ink backend as
passthrough regions so pen contacts starting over the panel reach egui as
mouse input. See [ui.md](ui.md).

## 10. Golden corpus and benchmarks

`tela-corpus` renders each built-in preset over twelve standard
trajectories (deterministic seeds, simulated time) and compares against
`crates/corpus/golden/*.png` with tolerances. Updates are explicit
(`TELA_UPDATE_GOLDEN=1`). `stamp_bench` measures dabs/s, Mpx/s, tiles per
dab, allocations per dab (counting allocator), single vs parallel and
composition cost. See [golden-tests.md](../golden-tests.md) and
[performance.md](../performance.md).

## 11. Extension points

| To add | Where | Notes |
|---|---|---|
| A built-in preset | `crates/brush/presets/*.json`, `preset::builtin::{ids, FILES}` | Then generate its golden. |
| A dynamics source | `DynamicsSource` + `DynamicsInput::value` | Must normalize to `0..=1` and return `None` when the device lacks it. |
| A combine mode | `CombineMode` + `CombineMode::apply` | Keep `Unknown` neutral. |
| A tip shape | `TipShape`, `PreparedTip`, `DabShape::coverage` | Coverage in `0..=1`, bounds must enclose it. |
| A texture pattern | `TexturePattern::generate` | Periods must divide the texture size (seamless). |
| A stabilizer / interpolator | `stabilizer.rs` / `interpolate.rs` + `ALL` | Compare with the phase-2 benchmarks before changing defaults. |
| A preset format change | bump `FORMAT_VERSION`, add a migration, write an ADR | Old files must keep loading. |
| An input backend | new module in `tela-tablet` producing `TabletEvent` | Fill `SampleCaps` honestly. |
| A UI panel | `tela-ui`, lent data + `PanelAction` | No authoritative state in the UI. |
| A layer property | private field on `RasterLayer` + getter + `Document` setter returning `Result<DirtyTiles, LayerError>` | Keep the lock rules in `Document`; extend the property test model. |
| A DabSink | implement `DabSink` | e.g. a logger, a GPU stamper later. |

## 12. Architectural limits (today)

- Flat layer stack (no groups); `BlendMode::Normal` only (4C).
- No undo/redo yet (4B); `Document::modify_tiles` is where tile snapshots will be taken.
- 8-bit layer storage; display texture limited to the GPU's max texture size.
- Document I/O does not exist.
- The UI is functional, not designed; preset edits are not persisted.

See [TECHNICAL_DEBT.md](../TECHNICAL_DEBT.md) and [ROADMAP.md](../ROADMAP.md).
