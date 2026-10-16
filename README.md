# Mirae

A professional raster illustration editor written from scratch in Rust
(wgpu + winit), with a deterministic, tested brush engine. Developed
clean-room: behaviour may be studied from the outside, the implementation
is original ([docs/CLEAN_ROOM.md](docs/CLEAN_ROOM.md)).

**Status:** phases 1–4 complete (canvas, tablet input, brush engine,
architecture audit, and layers and document: layer stack, undo/redo,
blend modes, masks, clipping, alpha lock, 16-bit storage, `.mirae`
files and PNG, integration). Phase 5 is proposed, not started. 401 tests
passing. Windows is the
primary platform (Windows Ink); no physical tablet has been validated yet.
Details: [docs/STATUS.md](docs/STATUS.md).

![Brush panel and strokes](docs/phase3/ui_2.png)

## Quick start

Requires Rust ≥ 1.95 (rustup, MSVC toolchain on Windows).

```sh
cargo run --release -p tela-app          # the editor (binary: tela)
cargo test --workspace --release         # all tests, including golden images
powershell -ExecutionPolicy Bypass -File scripts\check.ps1   # full local gate
```

## Controls

Pen (Windows Ink: pressure, tilt, rotation, eraser end) and mouse share one
input path. The panel on the left edits the active preset, its effectors
and the pressure curve; the layer panel on the right creates, selects,
hides, locks, reorders and deletes layers, sets their opacity and blend
mode, clips a layer to the one below, locks transparent pixels, and adds,
enables, edits (Pixels / Mask) and deletes layer masks. Above it, the
document lines show the file name (`*` while modified), Open, Save,
Save As, Import PNG (a new centred layer) and Export PNG (with the paper
or transparent, 8 or 16 bits), the storage depth with New 8-bit, New
16-bit and conversion to the other depth (undoable). Documents are saved
as `.mirae` files (docs/formats/mirae-v1.md); dropping a `.mirae` on the
window opens it, dropping a `.png` imports it. Before anything replaces
or closes a modified document, Mirae asks Save / Don't Save / Cancel.
The pen works on the panels too.

| Input | Action |
|---|---|
| Left drag / pen | Paint (pen eraser end erases) |
| 1–8 | Presets: ink pen, pencil, soft round, marker, airbrush, chalk, calligraphy, eraser |
| E | Toggle eraser mode |
| Middle drag, or Space + drag | Pan |
| Wheel | Zoom at cursor |
| Shift + wheel | Rotate 15° at cursor |
| `[` / `]` | Brush size |
| `-` / `=` | Stabilization −/+ 10 (0–100) |
| S | Cycle stabilizer (none, moving average, exponential, One Euro) |
| I | Cycle interpolation (linear, Catmull–Rom, Bézier) |
| T | Toggle start/end tapers |
| O | Toggle pressure → opacity |
| C | Clear the active layer |
| Ctrl+N | New document (8-bit, same size) |
| Ctrl+O | Open a document |
| Ctrl+S | Save |
| Ctrl+Shift+S, Shift+Alt+S | Save As |
| Ctrl+Shift+N | New layer above the active one |
| Ctrl+Z | Undo |
| Ctrl+Y, Ctrl+Shift+Z | Redo |
| F3 | Latency summary in the title bar |
| Home | Fit document |

Without tablet hardware, `inject_pen` drives the running app with a
synthetic Windows Ink pen (see [docs/testing.md](docs/testing.md#hardware-tests)).

## Architecture

```
tablet (input, FFI) ──► app ──► brush (presets, dynamics, pipeline, dabs, stroke buffer)
                         │ ──► document (layers, per-tile composition; stroke as overlay)
                         │ ──► renderer (wgpu display texture)  ◄── ui (egui overlay)
                         │ ──► io (.mirae files, PNG) ──► document
color, canvas: shared foundations          corpus: goldens + benchmarks
```

The app owns all state. The brush engine never sees the document; the
renderer never sees the document; the UI owns nothing. Boundaries are
enforced by a test. Read [docs/architecture/ARCHITECTURE.md](docs/architecture/ARCHITECTURE.md).

| Crate | Responsibility |
|---|---|
| `tela-color` | Straight/premultiplied RGBA, Porter-Duff over, sRGB |
| `tela-canvas` | Sparse 256×256 tiles, dirty sets, view transform |
| `tela-brush` | Presets (versioned JSON), effectors, stabilizers, interpolation, spacing, tips, textures, scatter, airbrush, stroke buffer |
| `tela-document` | Layer stack, masks, clipping, blend modes, 8/16-bit storage, undo/redo, per-tile composition |
| `tela-io` | `.mirae` documents (save atomically, load hostile input safely), PNG import/export |
| `tela-renderer` | wgpu presentation: mipmapped display texture, overlay hook |
| `tela-tablet` | Input samples, Windows Ink backend, input clock |
| `tela-ui` | egui panels (brush, layers, document, history), curve editor, prompts |
| `tela-corpus` | Golden brush sheets, image diff, stamping benchmark |
| `tela-app` | Window, event loop, wiring (binary `tela`) |

## Tests

401 tests: unit, integration, property (`proptest`), golden images of every
built-in preset, GPU display tests (skipped without a GPU) and crate
boundary checks, file-format corruption and compatibility tests, parity
tests against data measured in the reference, and end-to-end sessions
(scenarios, randomized sessions, stress). Benchmarks are separate
examples (`scripts\check.ps1 -Bench` runs the gating ones). See
[docs/testing.md](docs/testing.md) and [docs/golden-tests.md](docs/golden-tests.md).

## Documentation

| Document | Purpose |
|---|---|
| [docs/AI_HANDOFF.md](docs/AI_HANDOFF.md) | Start here when continuing the project |
| [docs/architecture/](docs/architecture/ARCHITECTURE.md) | Architecture, crate graph, invariants, concurrency, unsafe, errors, UI |
| [docs/adr/](docs/adr/README.md) | Architecture decision records |
| [docs/formats/mirae-v1.md](docs/formats/mirae-v1.md) | The `.mirae` document format |
| [docs/PARITY.md](docs/PARITY.md) | Parity with the reference, per subsystem |
| [docs/performance.md](docs/performance.md), [docs/memory.md](docs/memory.md) | Hot paths and current numbers; memory model |
| [docs/STATUS.md](docs/STATUS.md), [docs/ROADMAP.md](docs/ROADMAP.md), [docs/TECHNICAL_DEBT.md](docs/TECHNICAL_DEBT.md) | Where the project stands and goes |
| [docs/phase5/PROPOSAL.md](docs/phase5/PROPOSAL.md) | Proposal for the next phase (not approved) |
| [CONTRIBUTING.md](CONTRIBUTING.md), [AGENTS.md](AGENTS.md), [CHANGELOG.md](CHANGELOG.md) | How to work on it; history |
| [docs/phase2/](docs/phase2/README.md), [docs/phase3/](docs/phase3/README.md), [docs/phase4/](docs/phase4/README.md) | Phase reports with evidence |

## Roadmap

- [x] Phase 1 — window, wgpu, tiled canvas, mouse drawing, pan/zoom/rotate
- [x] Phase 2 — Windows Ink input, pressure, interpolation, stabilization ([report](docs/phase2/README.md))
- [x] Phase 3 — brush presets, effectors, tips, textures, airbrush, eraser ([report](docs/phase3/README.md))
- [x] Phase 3.5 — architecture audit, boundaries, documentation, handoff
- [x] Phase 4 — layers, blend modes, undo/redo, masks, 16-bit storage, document I/O, integration ([report](docs/phase4/README.md))
- [ ] Phase 5 — proposed: colour, layer workflow, selections, transform, fill, gradients ([proposal](docs/phase5/PROPOSAL.md), awaiting approval)
- [ ] Later — layer folders, vector layers, rulers/symmetry/perspective, advanced brushes, comic tools, colour management ([roadmap](docs/ROADMAP.md))

## Clean-room notice

No decompiled code, third-party assembly, decompiler pseudocode, extracted
code, proprietary assets/icons/shaders/presets, or undocumented proprietary
formats are used as a basis for implementation. Algorithms come from public
mathematics and literature (e.g. Porter–Duff 1984, Fritsch–Carlson 1980,
Casiez et al. 2012, Yuksel et al. 2011) and this project's own measured
requirements. All brush tips and paper textures are generated by code.

License: MIT OR Apache-2.0 (workspace metadata).
