# Roadmap

No dates. A phase starts only when the previous one is done and the owner
explicitly approves it; phases are never started automatically.

| Phase | Scope | Status |
|---|---|---|
| 1 | Window, wgpu renderer, sparse tiled canvas, mouse drawing, pan/zoom/rotate | **Complete** |
| 2 | Windows Ink input (pressure, tilt, rotation, history, QPC timestamps), dynamics with curves, stabilizers, interpolation, sample-rate-invariant spacing, live tapers | **Complete** ([report](phase2/README.md)) |
| 3 | Professional brush engine: versioned presets, generic effectors, procedural/image tips, canvas-space textures, scatter, time-based airbrush, eraser, egui panel and curve editor, golden corpus, parallel stamping | **Complete** ([report](phase3/README.md)) |
| 3.5 | Architecture audit, Rust idioms, boundaries enforced, documentation, ADRs, handoff | **Complete** (see STATUS.md) |
| 4 | Layers and document | **Planned** — not started |
| 5 | Vector layers and editing | Planned |
| 6 | Selections, transform, fill, gradients | Planned |
| 7 | Rulers, symmetry, perspective | Planned |
| 8 | Wet media, color mixing, advanced brushes | Planned |

## Phase 4 direction (not a commitment, not started)

Proposed at the end of phase 3; to be refined and approved before any work:

- **Layers**: a layer stack API with stable ids, groups, visibility,
  opacity, locking; composition still per dirty tile; group caches
  invalidated per tile.
- **Blend modes**: normal, multiply, screen, overlay, darken, lighten,
  color dodge/burn, add, soft/hard light, difference — separable functions
  on premultiplied alpha derived from public definitions (W3C Compositing
  and Blending); one reference test per mode.
- **Undo/redo**: copy-on-write tile snapshots of exactly the tiles a stroke
  touched (`StrokeBuffer::touched`), memory-bounded history; layer
  operations as commands in the same history.
- **Masks, clipping, alpha lock**: sparse 8-bit layer masks; clipping to the
  layer below; alpha lock as an opacity ceiling equal to destination alpha.
- **16-bit storage**: u16 tiles (optional per document) to remove banding;
  memory and composition benchmarks against 8-bit.
- **Document I/O**: an open, versioned container (zip + JSON manifest +
  compressed tiles + used presets); PNG export; PNG import as a layer;
  bit-exact round-trip tests.

## Later goals (unordered)

GPU composition for many layers; display paging for huge documents;
movement prediction; user preset persistence and management; real-tablet
validation matrix; color management; selections and transforms; text;
animation is out of scope until decided otherwise.
