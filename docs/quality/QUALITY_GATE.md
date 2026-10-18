# Quality gate

Permanent criteria for approving a subphase (and any change a user can
see). Adopted on 2026-10-17 after the owner revoked the approval of
phase 5F: 618 green engine tests had not shown that the application
worked for a person using it (gradient previews lagging and left on
screen, controls out of reach, panel clicks that painted;
[UI_AUDIT.md](UI_AUDIT.md)).

**A tool or feature is not functionally done because its engine tests
pass.** It is done when every applicable layer below is validated, and
only the owner approves a subphase.

## The layers

| Layer | What it proves | Where | Run |
|---|---|---|---|
| **L1 Unit** | Algorithms and invariants (math, rules, edge cases, garbage input) | `#[cfg(test)]` in each crate | `cargo test --workspace --release` |
| **L2 Integration** | Subsystems together (brush → document → history → files; property and randomized sessions; parity data) | `crates/*/tests/` | same |
| **L3 Desktop E2E** | The real executable, driven by OS input (mouse, keyboard, synthetic pen, native file dialogs), checked through its state, its document's pixels and the screen | `crates/e2e/tests/` ([testing.md](../testing.md#desktop-end-to-end-tests)) | `powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1` |
| **L4 UX/UI acceptance** | The interface is coherent and usable: reachable controls, no overlaps, a stable layout, visible state (active tool, active layer, pixels or mask), feedback, focus, at reference window sizes | `crates/e2e/tests/ui_layout.rs` (automated part) + a recorded review with screenshots ([UI_AUDIT.md](UI_AUDIT.md)) | same, plus the review |
| **Performance** | Engine benchmarks (0 allocations per dab, composition) and interface response times | `scripts\check.ps1 -Bench`; `crates/e2e/tests/performance.rs` | both |
| **Parity** | Behaviour the reference defines, observed first and compared | [PARITY.md](../PARITY.md), `docs/parity/` | the subphase's parity gate |
| **Real hardware** | Pen feel, pressure, tilt on a physical tablet | [STATUS.md](../STATUS.md#hardware-validation) | when a tablet exists; never claimed otherwise |

A headless test never stands in for L3, and a test of a model or view
model is never reported as an end-to-end test. A defect that appears
only through the interface gets a test that goes through the interface
(a direct `Document` test may be added, never instead).

## Result states

Every item of a gate is reported in exactly one of these states:

| State | Meaning |
|---|---|
| **automated pass** | A test of the stated layer ran and passed on the stated build and machine. |
| **manually verified** | A person (or the agent, said which) checked it by hand in the running app, with the steps and evidence recorded. |
| **not tested** | Nobody checked it. Never counted as passing. |
| **failed** | Checked and wrong; a defect with an ID. |
| **blocked** | Cannot be checked here (no hardware, no display, automation cannot reach it); the reason is recorded. |

Nobody records "approved" for the owner. The agent's report ends at
"ready for the owner's review".

## Required for a subphase

1. **L1/L2**: `scripts\check.ps1 -Bench` green (fmt, clippy `-D warnings`,
   every test, strict rustdoc, build, `stamp_bench` with 0.000
   allocations per dab, `doc_bench --quick` without composition
   allocations). Earlier tests unchanged unless an invariant changed
   deliberately; goldens never regenerated to hide a defect; parity
   tolerances never relaxed.
2. **L3**: `scripts\e2e.ps1` green. Every feature the subphase adds or
   changes has E2E coverage in [E2E_MATRIX.md](E2E_MATRIX.md) at least for
   its main path, its cancel path, undo/redo, and 8- and 16-bit documents
   where pixels are involved, with the result checked three ways:
   application state (tool, layer, history, modified), pixels (document
   queries, exported files, digests) and the screen (captures).
3. **L4**: the interface gate below passes, and the subphase's new or
   changed panels are reviewed in [UI_AUDIT.md](UI_AUDIT.md) with
   screenshots taken in Mirae (never copied from the reference).
4. **Performance**: no engine benchmark regresses beyond its noise
   ([performance.md](../performance.md)); the interface response times
   stay within their limits; new long operations are measured.
5. **Parity**: the subphase's parity gate is recorded.
6. **Hardware**: anything about pen feel says "synthetic pen only" until a
   physical tablet has been used.
7. **Defects**: every confirmed P0 and P1 fixed (with a regression test of
   the layer that found it); P2 that affect basic workflows fixed; the
   rest listed with an ID in [TECHNICAL_DEBT.md](../TECHNICAL_DEBT.md) or
   [UI_AUDIT.md](UI_AUDIT.md).

Severities: **P0** data loss, a corrupted save, a reproducible crash, an
unusable tool, an action that changes the wrong document or layer; **P1**
functionally wrong results, a misleading preview, essential controls not
working, an operation that cannot be completed, an interface that
prevents normal use; **P2** poor layout, confusing feedback, misalignment,
inconsistent focus, local slowness; **P3** visual refinement, small
inconsistencies, style.

## Interface gate (automated, `ui_layout.rs`)

| Check | How |
|---|---|
| No essential control outside the window | Every essential control (file, history, document, layers, colour, every tool and each tool's main options) is reached inside the window — scrolling its panel with the wheel as a user would — at 1280×800 (the default window), 1920×1080, 1024×700, 800×600 and maximized. |
| No overlap | No two interactive widgets or labels overlap in what is shown of them (clipped rectangles), for every tool's options, at those sizes. |
| Critical controls accessible | They are found by role and label in egui's accessibility tree (the test mode writes it), enabled where the state allows it. |
| Focus restored | After a text or number field, a canvas click returns the keyboard: tool shortcuts act; Escape leaves a field. |
| No unexpected layout change | Painting (a recent colour, an undo step), adding layers and changing the document name do not move the panels' controls. |
| The active tool is visible | Each tool key marks exactly its own strip button and heads the options with its name. |
| Feedback of apply / cancel | Escape and Apply/Cancel of gradients, selections and transforms leave the expected state and screen (gradient, transform and scenario tests). |
| Browsing never edits | Switching tools and scrolling through every panel leaves the document without a history step; a panel click right after the window moved never paints. |

Reference screenshots may be added to a test only after a person has
checked that they show the correct interface, and are never regenerated
automatically.

## Not covered by automation (stated, not hidden)

- Display scaling other than 100 % and screens larger than 2560×1440: the
  development machine has one 2560×1440 display at 100 %; changing the
  system scaling of a shared machine was not done. **Not tested.**
- Windows UI Automation: egui exposes no UIA provider (the AccessKit
  adapter is not enabled); the test mode publishes egui's own AccessKit
  description instead. Screen readers are **not tested**.
- Dropping files on the window (OLE drag and drop from another process).
  **Blocked** for automation; check by hand.
- A physical pen. **Blocked** (no hardware).
