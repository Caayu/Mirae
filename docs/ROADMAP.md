# Roadmap

No dates. A phase (and, from phase 4 on, each subphase) starts only when
the previous one is done and the owner explicitly approves it; nothing is
started automatically. The long-term goal is very high parity with Clip
Studio Paint for professional illustration workflows, clean-room; progress
is tracked in [PARITY.md](PARITY.md), and every subphase ends with a parity
gate.

| Phase | Scope | Status |
|---|---|---|
| 1 | Window, wgpu renderer, sparse tiled canvas, mouse drawing, pan/zoom/rotate | **Complete** |
| 2 | Windows Ink input (pressure, tilt, rotation, history, QPC timestamps), dynamics with curves, stabilizers, interpolation, sample-rate-invariant spacing, live tapers | **Complete** ([report](phase2/README.md)) |
| 3 | Professional brush engine: versioned presets, generic effectors, procedural/image tips, canvas-space textures, scatter, time-based airbrush, eraser, egui panel and curve editor, golden corpus, parallel stamping | **Complete** ([report](phase3/README.md)) |
| 3.5 | Architecture audit, Rust idioms, boundaries enforced, documentation, ADRs, handoff | **Complete** (see STATUS.md) |
| 4 | Layers and document: 4A layer API · 4B undo/redo · 4C blend modes · 4D masks, clipping, alpha lock · 4E 16-bit · 4F document I/O · 4G integration | **In progress** — plan approved ([phase4/PLAN.md](phase4/PLAN.md)); 4A authorized |
| 5 | Vector layers and editing | Planned |
| 6 | Selections, transform, fill, gradients | Planned |
| 7 | Rulers, symmetry, perspective | Planned |
| 8 | Wet media, color mixing, advanced brushes | Planned |

## Phase 4

The approved plan, decisions and per-subphase parity gates are in
[phase4/PLAN.md](phase4/PLAN.md). Sequence: 4A → 4B → 4C → 4D → 4E → 4F →
4G, each subphase approved separately.

## Later goals (unordered)

GPU composition for many layers; display paging for huge documents;
movement prediction; user preset persistence and management; real-tablet
validation matrix; color management; selections and transforms; text;
animation is out of scope until decided otherwise.
