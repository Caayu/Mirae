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
| 4 | Layers and document: 4A layer API · 4B undo/redo · 4C blend modes · 4D masks, clipping, alpha lock · 4E 16-bit · 4F document I/O · 4G integration | **Complete** ([report](phase4/README.md), [plan](phase4/PLAN.md)) |
| 5 | Editing fundamentals — **5A colour (complete)** · **5B layer workflow + translucent-backdrop probe, merge down (complete)** · 5C selections · 5D transform · 5E fill · 5F gradients · 5G integration | 5A, 5B done ([plan](phase5/PLAN.md)); 5C–5G proposed, awaiting the owner ([proposal](phase5/PROPOSAL.md)) |
| later | In the proposal's recommended order: layer folders + GPU compositing decision · vector layers · rulers, symmetry, perspective · advanced brushes (colour mixing, wet media, preset management) · comic tools, colour management | Planned |

The table before phase 4G planned phase 5 as vector layers and phase 6 as
selections/transform/fill/gradients; the proposal reorders them by
dependency (selections and colour first, vector layers after transform).
The owner decides.

## Phase 4

Complete. Plan, decisions and per-subphase parity gates in
[phase4/PLAN.md](phase4/PLAN.md); the phase report (what was built,
benchmarks, invariants, parity, limitations, remaining debt) in
[phase4/README.md](phase4/README.md).

## Later goals (unordered)

GPU composition for many layers; display paging for huge documents;
movement prediction; user preset persistence and management; real-tablet
validation matrix; color management; selections and transforms; text;
animation is out of scope until decided otherwise.
