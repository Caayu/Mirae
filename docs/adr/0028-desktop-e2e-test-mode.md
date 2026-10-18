# 0028 — A desktop end-to-end test mode in the executable

Quality gate after 5F (2026-10-17;
[../quality/QUALITY_GATE.md](../quality/QUALITY_GATE.md)).

# Context

The owner revoked the approval of phase 5F: 618 engine tests passed
while the application showed lagging and ghost gradient previews,
controls out of reach and panel clicks that painted
([../quality/UI_AUDIT.md](../quality/UI_AUDIT.md)). Nothing tested the
executable a person runs. The new quality gate requires desktop tests
(L3) and interface acceptance (L4) that drive the real window with real
input and check the result as the user would see it.

Mirae draws its interface with egui, which paints into a GPU surface:
Windows UI Automation sees one window and no controls (egui's AccessKit
adapter, `accesskit_winit`, is not built in), and pixel coordinates of
controls change with every layout change.

# Decision

1. **A test mode in the executable**, off unless the environment variable
   `TELA_E2E_DIR` names a folder (`crates/app/src/app/e2e.rs`). In that
   mode the app, after every frame (and when an input event needs no
   frame), atomically writes `state.json`: the tool, the drag, the view
   mapping, the document's structure and history, the colours, the
   gradient and fill settings, open prompts, the panels' areas, when it
   last handled input, and **egui's own AccessKit description of every
   widget** (role, label, value, state, rectangle and visible part in
   window pixels). It answers read-only queries written to `request.txt`
   (`pixel x y`, `digest`). It never changes the document, the tools or
   the interface; production runs never enter it.
2. **Inspection in `tela-ui`**: `UiLayer::set_inspect` turns on egui's
   AccessKit tree and keeps `WidgetInfo`s; off by default. Reading a
   widget's clipped rectangle needs `egui::Id::from_high_entropy_bits`,
   which is `unsafe` only in name (no memory-safety contract); one
   documented block ([../architecture/unsafe.md](../architecture/unsafe.md)).
3. **A driver crate, `tela-e2e`** (test support, depends on no workspace
   crate; dev-dependencies on the engine as an oracle): it starts
   `tela.exe` in test mode, sends OS input (`SendInput` mouse and
   keyboard, `InjectSyntheticPointerInput` for the pen, a window of its
   own to take the focus), finds widgets by role and label — scrolling
   their panel as a user would — waits for its input to be handled
   (`input_ms`) and the app to become idle, captures the screen, answers
   native file dialogs, writes a report per test into
   `target/e2e/<test>/`, and stops only the process it started. Its tests
   are `#[ignore]`d (they take over the desktop) and run with
   `scripts\e2e.ps1`.

# Consequences

- Defects that only the interface shows are reproducible and stay fixed:
  each fix in the audit has a failing-then-passing E2E test.
- `cargo test --workspace` lists the E2E tests as ignored; the gate runs
  them separately. They need an unlocked desktop and the window in
  front; another program taking the focus fails a test rather than
  sending it input.
- The state file is the app's own view: a check of it is complemented by
  pixel queries, exported files and screen captures, so a correct engine
  behind a broken interface is still caught.
- The test mode costs nothing when off (one `Option` check per frame and
  per event).

# Alternatives considered

- **Windows UI Automation through AccessKit** (`egui-winit`'s
  `accesskit` feature): would also serve screen readers, but adds
  `accesskit_winit` and its platform adapters as dependencies; recorded
  as a later accessibility decision. The test mode reads the same
  AccessKit tree without them.
- **Fixed coordinates** of controls: break with every layout change.
- **Driving the app in-process** (calling its handlers): would skip the
  window, winit, egui's input routing and the OS — exactly where the 5F
  defects were.
