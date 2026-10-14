# Tela

Professional digital illustration editor written from scratch in Rust
(wgpu + winit). Developed clean-room: behavior may be studied from the
outside, implementation is original. See *Clean-room rules* below.

## Running

```sh
cargo run --release -p tela-app
```

Pen input (Windows Ink: pressure, tilt, rotation) and mouse share one path.

| Input | Action |
|---|---|
| Left drag / pen | Paint |
| Middle drag, or Space + drag | Pan |
| Wheel | Zoom at cursor |
| Shift + wheel | Rotate 15° at cursor |
| `[` / `]` | Brush size |
| `-` / `=` | Stabilization −/+ 10 (0–100) |
| S | Cycle stabilizer (none, moving average, exponential, One Euro) |
| I | Cycle interpolation (linear, Catmull–Rom, Bézier) |
| T | Toggle start/end tapers |
| O | Toggle pressure → opacity |
| C | Clear layer |
| F3 | Latency summary in the title bar |
| Home | Fit document |

Diagnostics: `RUST_LOG=info,tela::latency=debug,tela::stroke=debug`.
Without tablet hardware, `cargo run --release -p tela-tablet --example inject_pen`
drives the running app with a synthetic Windows Ink pen.

## Architecture

Each module is its own crate so dependency rules are enforced by the compiler.

```
color     <- canvas <- brush      (no dependency on tablet, UI or GPU)
                   \<- document   (does not depend on brush)
                   \<- renderer   (sees tiles and a view, never the document)
tablet: device acquisition + normalization (no dependency on brush)
app: wires everything together; the only crate that knows about winit
```

| Crate | Responsibility |
|---|---|
| `tela-color` | Straight/premultiplied RGBA, Porter-Duff over, sRGB transfer |
| `tela-canvas` | Sparse 256×256 tiles, dirty-tile sets, pan/zoom/rotate view |
| `tela-tablet` | `TabletSample`, unit/timestamp normalization, Windows Ink backend |
| `tela-brush` | Dynamics/effectors, velocity, jitter filter, stabilizers, interpolators, dab spacing, tapers, stroke buffer, synthetic analysis corpus |
| `tela-document` | Raster layers and per-tile composition |
| `tela-renderer` | wgpu presentation of composed tiles |
| `tela-app` | Window, event loop, interaction |

Planned crates: `vector`, `tools`, `history`, `ui`, `io`.

### Key decisions

- **Premultiplied alpha everywhere in compositing.** `over` becomes
  `src + dst·(1−αs)`: associative, no division, no dark fringes.
- **Stroke buffer.** A stroke accumulates alpha in a separate f32 mask; each
  dab pulls pixels toward its own opacity ceiling, `m += (op − m)·flow·cov`.
  Opacity is a ceiling, flow the per-dab build-up, the preview equals the
  committed result, and touched tiles are known exactly for undo.
- **Data-driven dynamics.** Each property is `base × Π curve_i(input_i)` over
  effectors (pressure, speed, tilt, direction, rotation, distance, random).
  Curves are monotone cubic (Fritsch–Carlson): no overshoot between points.
  Inputs a device lacks make their effectors neutral.
- **Stabilization is not interpolation.** Stabilizers (time-based, measured in
  screen pixels) move samples; interpolators connect them. Defaults chosen by
  benchmark: One Euro filter and centripetal Catmull–Rom (see `docs/phase2`).
- **Sample-rate-invariant dab placement.** Arc length carries across segments
  and attributes follow arc-length fraction: 2 or 1000 samples of the same
  stroke give the same dabs within 1e-3 px.
- **Live end taper.** The last `taper.end` px are a re-renderable tail drawn as
  if the stroke ended now; older dabs are final.
- **CPU composition of dirty tiles, GPU presentation.** Deterministic and unit
  tested; can move to compute shaders later without changing interfaces.
- **sRGB-encoded storage, non-sRGB swapchain.** Values pass through unchanged.

### Known limitations

- Linear filtering at zoom < 1 can show faint seams at tile borders, and
  nearest filtering at zoom > 1 shows canvas pixels as blocks (needs gutter
  pixels or a mip-mapped atlas and a better magnification filter).
- Curved interpolation runs one sample behind the input (~4 ms at 240 Hz).
- Stroke ends use a mirrored phantom neighbour, so the first/last segment of
  an open stroke has slightly less curvature than its interior.
- Mouse samples carry no device timestamp (stamped on arrival).
- The pen eraser end is detected but paints like the tip until an eraser exists.
- Pixels painted outside the document bounds are kept in the layer (clipped
  only at composition).

## Roadmap

- [x] Phase 1 — window, wgpu, tiled canvas, mouse drawing, pan/zoom/rotate
- [x] Phase 2 — Windows Ink input, pressure, interpolation, stabilization ([report](docs/phase2/README.md))
- [ ] Phase 3 — brush presets, effectors, textures
- [ ] Phase 4 — layers, blend modes, undo/redo
- [ ] Phase 5 — vector layers and editing
- [ ] Phase 6 — selections, transform, fill, gradients
- [ ] Phase 7 — rulers, symmetry, perspective
- [ ] Phase 8 — wet media, color mixing, advanced brushes

## Clean-room rules

No decompiled code, third-party assembly, decompiler pseudocode, extracted
code, proprietary assets/icons/shaders, or undocumented proprietary formats
are used as a basis for implementation. Algorithms are designed from public
mathematics and literature (e.g. Fritsch–Carlson 1980, Casiez et al. 2012,
Yuksel et al. 2011) and this project's own behavioral requirements.
