# Tela

Professional digital illustration editor written from scratch in Rust
(wgpu + winit). Developed clean-room: behavior may be studied from the
outside, implementation is original. See *Clean-room rules* below.

## Running

```sh
cargo run --release -p tela-app
```

| Input | Action |
|---|---|
| Left drag | Paint |
| Middle drag, or Space + left drag | Pan |
| Wheel | Zoom at cursor |
| Shift + wheel | Rotate 15° at cursor |
| `[` / `]` | Brush size |
| Home | Fit document |

## Architecture

Each module is its own crate so dependency rules are enforced by the compiler.

```
color     <- canvas <- brush
                   \<- document   (does not depend on brush)
                   \<- renderer   (sees tiles and a view, never the document)
app: wires everything together; the only crate that knows about winit
```

| Crate | Responsibility |
|---|---|
| `tela-color` | Straight/premultiplied RGBA, Porter-Duff over, sRGB transfer |
| `tela-canvas` | Sparse 256×256 tiles, dirty-tile sets, pan/zoom/rotate view |
| `tela-brush` | `BrushInput`, dab spacing, tip coverage, stroke buffer |
| `tela-document` | Raster layers and per-tile composition |
| `tela-renderer` | wgpu presentation of composed tiles |
| `tela-app` | Window, event loop, interaction |

Planned crates: `vector`, `tools`, `tablet`, `history`, `ui`, `io`.

### Key decisions

- **Premultiplied alpha everywhere in compositing.** `over` becomes
  `src + dst·(1−αs)`: associative, no division, no dark fringes.
- **Stroke buffer.** A stroke accumulates coverage `m += flow·cov·(1−m)` in a
  separate f32 mask; the layer receives `color·opacity·m` on commit. Opacity is
  thus a ceiling for the whole stroke and flow the per-dab build-up, the
  preview equals the committed result, and the touched tiles are known exactly
  for undo.
- **Carried dab spacing.** Remaining arc length carries over between input
  segments, so dab spacing does not depend on the device's sample rate.
- **CPU composition of dirty tiles, GPU presentation.** Deterministic and unit
  tested; can move to compute shaders later without changing interfaces.
- **sRGB-encoded storage, non-sRGB swapchain.** Values pass through unchanged.

### Known limitations

- Linear filtering at zoom < 1 can show faint seams at tile borders (needs
  gutter pixels or a mip-mapped atlas).
- Path interpolation is linear between samples; no stabilization yet.
- Pixels painted outside the document bounds are kept in the layer (clipped
  only at composition).

## Roadmap

- [x] Phase 1 — window, wgpu, tiled canvas, mouse drawing, pan/zoom/rotate
- [ ] Phase 2 — Windows Ink input, pressure, interpolation, stabilization
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
mathematics and this project's own behavioral requirements.
