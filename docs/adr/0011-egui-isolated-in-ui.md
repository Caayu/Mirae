# ADR 0011: egui isolated in the `tela-ui` crate

Status: accepted (phase 3). Recorded in phase 3.5.

# Context

Phase 3 needed a panel to edit presets, effectors and curves. A UI toolkit
tends to spread through a codebase (types in signatures, state in models),
which would make replacing it or testing the engine headless harder.

# Decision

- Only `tela-ui` depends on `egui`, `egui-wgpu`, `egui-winit`.
- The UI edits plain engine data lent for one frame (`PanelModel`) and
  returns `PanelAction`s; it holds no authoritative state.
- It draws through the renderer's generic `FrameOverlay` trait; the
  renderer does not know egui exists.
- Domain rules used by the UI live in the domain crates
  (e.g. `StrokeOverrides::edit_resolved`, `Curve` editing operations).

# Consequences

- Engine, document and renderer build and test without any UI.
- Replacing egui touches one crate plus the app's wiring.
- The UI is functional but not designed; that is acceptable for now.
- Enforced by `crates/app/tests/crate_boundaries.rs`.

# Alternatives considered

- Native Win32 UI: platform-bound.
- Custom immediate-mode UI: too much work before the product needs it.
- egui across crates: faster to write, but couples the engine to a UI.
