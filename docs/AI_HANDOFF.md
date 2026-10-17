# READ THIS FIRST WHEN STARTING A NEW AI SESSION.

This file is enough to continue the project without any previous
conversation. Read it fully, then these, all **mandatory**:

1. [architecture/ARCHITECTURE.md](architecture/ARCHITECTURE.md)
2. [architecture/invariants.md](architecture/invariants.md)
3. [PARITY.md](PARITY.md) — parity goal, states, the parity gate every
   subphase must pass, and the current matrix
4. [STATUS.md](STATUS.md), [ROADMAP.md](ROADMAP.md), the report of the
   last phase ([phase4/README.md](phase4/README.md)), the phase-5 plan
   ([phase5/PLAN.md](phase5/PLAN.md)) and its proposal
   ([phase5/PROPOSAL.md](phase5/PROPOSAL.md))

## 1. Mission

Build **Mirae** (formerly Tela: crates, binary `tela.exe` and some
identifiers keep the old name), a professional raster illustration editor
in Rust (wgpu + winit). The long-term goal is very high functional, behavioural,
qualitative and performance **parity with Clip Studio Paint** for
professional illustration workflows ([PARITY.md](PARITY.md)), developed
strictly **clean-room** ([CLEAN_ROOM.md](CLEAN_ROOM.md)): the reference is
only observed as a black box. Quality bar: a Rust codebase that looks
maintained by an experienced team — small commits, tests for math,
measured performance, documented decisions.

## 2. Current state

**Phase 4 (layers and document) is complete**: 4A (layer API), 4B
(undo/redo), 4C (blend modes), 4D (masks, clipping, alpha lock), 4E
(16-bit storage), 4F (document I/O: the `.mirae` format, PNG, the file
workflow) and 4G (integration, benchmarks, hardening), each with a
**parity gate** recorded in [PARITY.md](PARITY.md) (which ends with a
phase-4 parity summary). Report: [phase4/README.md](phase4/README.md).
Facts: [STATUS.md](STATUS.md).

**Phase 5A (colour) is complete** (2026-10-17): a paint colour captured by
each stroke, main/sub colours, a colour panel, the eyedropper, recent
colours and swatches, with a parity gate ([parity/5A-color.md](parity/5A-color.md),
ADR 0021). **5B–5G are proposed, not approved, not started**
([phase5/PLAN.md](phase5/PLAN.md)). A new session asks the owner before
starting 5B or anything else; it never begins a subphase on its own.

## 3. Repository

`C:\Users\kaio-\rea\paint` (git, branch `main`, no remote). The parent
folder `C:\Users\kaio-\rea` holds unrelated material; work only inside
`paint/`.

## 4. Build

Rust via rustup (MSVC toolchain), installed in `%USERPROFILE%\.cargo\bin`.
In Git Bash that directory may be missing from PATH:
`export PATH="$HOME/.cargo/bin:$PATH"`. Verified with Rust 1.99.0;
minimum 1.95.

```sh
cargo build --workspace --release
```

## 5. Run

```sh
cargo run --release -p tela-app        # binary: target/release/tela.exe
```

Controls are in the [README](../README.md#controls). Logging:
`RUST_LOG=info,tela::ink=debug,tela::latency=debug,tela::stroke=debug`.

## 6. Test

```sh
cargo test --workspace --release        # 434 tests, ~35 s after build
```

## 7. Clippy and the full gate

```sh
cargo clippy --workspace --all-targets --release -- -D warnings
powershell -ExecutionPolicy Bypass -File scripts\check.ps1   # fmt, clippy, test, strict doc, build
powershell -ExecutionPolicy Bypass -File scripts\check.ps1 -Bench   # + stamp_bench, doc_bench --quick (fails if composition allocates)
```

## 8. Benchmarks

```sh
cargo run --release -p tela-corpus --example stamp_bench        # ~1 min
cargo run --release -p tela-corpus --example doc_bench          # 1-100 layers, 4096² documents, history, memory; ~1 min (--quick ~3 s)
cargo run --release -p tela-corpus --example depth_bench        # 8 vs 16-bit, ~20 s
cargo run --release -p tela-io --example io_bench              # .mirae save/load, ~40 s
cargo run --release -p tela-corpus --example banding            # 8 vs 16-bit banding, ~2 s
cargo run --release -p tela-brush --example stabilizer_bench
cargo run --release -p tela-brush --example interpolation_bench
```

Compare with [performance.md](performance.md).

## 9. Synthetic tablet

No physical tablet exists on this machine. Drive the running app with a
synthetic Windows Ink pen (real WM_POINTER path):

```sh
cargo build --release -p tela-app -p tela-tablet --examples
target/release/tela.exe                                   # start it first
TELA_INJECT_REGION="0.26,0.02,0.88,0.98" target/release/examples/inject_pen.exe "Mirae" all
```

PowerShell: `$env:TELA_INJECT_REGION = "0.26,0.02,0.88,0.98"; target\release\examples\inject_pen.exe "Mirae" all`
(full sequence in docs/testing.md#hardware-tests).

Patterns: `all`, `pressure`, `fast`, `detail`, `eraser`,
`drag:x0,y0,x1,y1[,pressure]`, `hold:u,v,seconds[,pressure]` (fractions
of the client area). `cargo run -p tela-tablet --example pointer_devices`
lists the pen devices Windows sees. Pitfalls on this machine: PowerShell swallows a bare
`--` argument; the pt-BR locale formats decimals with commas (use
`[string]::Format([cultureinfo]::InvariantCulture, ...)` when generating
numbers in PowerShell); kill a running `tela.exe` before rebuilding or the
link step fails with "Acesso negado". Ctrl+Shift+S never reaches the
app on this machine (another program holds it as a global hotkey); use
Shift+Alt+S for Save As.

## 10. Architecture in brief

```
tablet (FFI) -> app -> BrushInput -> brush (pipeline -> dabs -> StrokeBuffer)
                 app -> document (layers; stroke previewed via Overlay, committed by app)
                 app -> renderer (finished tiles -> display texture)   ui -> FrameOverlay
```

App owns all state; the brush engine never sees the document; the renderer
never sees the document; the UI owns nothing. Read
[architecture/ARCHITECTURE.md](architecture/ARCHITECTURE.md).

## 11. Crates

`color` (the paint colour `Color`, HSV, hex; premultiplied color, storage formats), `canvas` (sparse tiles, view), `brush`
(engine), `document` (layer stack with stable ids, composition, history),
`io` (`.mirae` files, PNG), `renderer` (wgpu), `tablet` (input, Windows
Ink), `ui` (egui), `corpus` (goldens, benchmarks), `app` (binary `tela`;
the file workflow).

## 12. Allowed dependencies

Exact table and reasons: [architecture/crate-dependencies.md](architecture/crate-dependencies.md).
Enforced by `crates/app/tests/crate_boundaries.rs`. Do not add a
dependency for convenience.

## 13. Critical invariants

Full list with tests: [architecture/invariants.md](architecture/invariants.md). The ones most
easily broken:

1. Same preset + seed + samples ⇒ bit-identical dabs (counter-based RNG).
2. Dab placement independent of sample density.
3. Preview == committed result (shared `paint_over_tile` / `paint_alpha_tile`,
   live tail; stroke target captured at stroke start).
4. Parallel stamping bit-identical to sequential.
5. Airbrush independent of frame rate.
6. Invalid presets never panic; unknown values are neutral.
7. 0 allocations per sample/dab on the sequential path (tested); the
   parallel path only has rayon's amortized queue blocks; composition is
   allocation-free.
8. Crate boundaries (brush ⟂ tablet/UI/GPU/document; renderer ⟂ document).
9. One depth per document; pixel edits name their depth (`modify_tiles::<P>`,
   `with_pixel_type!`) and the other depth is refused; 8 → 16 is exact and
   composes bit-identically; masks stay 8-bit (ADR 0018, DP1–DP7).
10. Files: save → load is bit-exact (IO1); documents are built only
    through `Document::from_parts` (L10); every file is hostile input
    (limits before data, never a panic); a newer format version is
    refused; the v1 fixtures in `crates/io/tests/fixtures/v1/` are a
    contract and are **never regenerated** (ADR 0019).
11. "Modified" is `document.revision() != saved` (H12): every new
    persistent change must either be a history step or bump the
    revision (as visibility does).
12. Colour (ADR 0021, COL1–COL8): a stroke copies the active `Color` at
    pen-down and never reads colour state again; the colour is opaque and
    `f32` (no alpha, not premultiplied, not quantized before the commit);
    colour state lives in the app, not in the document, the file or the
    history; the eraser and mask strokes ignore the colour.

## 14. Architectural decisions

[adr/README.md](adr/README.md): premultiplied alpha, sparse 256 px tiles,
separate stroke buffer, CPU composition + GPU display texture, One Euro,
centripetal Catmull–Rom, counter-based RNG, versioned JSON presets,
canvas-space textures, bit-identical parallel stamping, egui isolation,
golden corpus, time-instant airbrush, document layer model, swap-based
undo, blend modes measured against the reference, masks/clipping/alpha lock
measured against the reference (0014–0017), 16-bit storage per document
with an 8-bit default (0018), the `.mirae` v1 format (0019), the I/O crate
and file workflow (0020), the paint colour and colour tool state (0021).

## 15. Completed features

Phases 1–4 + 3.5 and subphase 5A; see [STATUS.md](STATUS.md#features) and
[CHANGELOG.md](../CHANGELOG.md). Phase reports: [phase2](phase2/README.md),
[phase3](phase3/README.md), [phase4](phase4/README.md) (phase 1 has no
separate report; it is summarized in the changelog).

## 16. Limitations

[TECHNICAL_DEBT.md](TECHNICAL_DEBT.md): no physical tablet validation,
8-bit default storage and 8-bit masks and display (16-bit documents
optional), GPU texture size limit, undo history uncompressed in memory,
UI edits not persisted, synchronous saving (no autosave), transparent
export refused where D-4C-1 applies, goldens verified on one machine,
no transparent colour and colour palette not persisted (#35, #36), CPU composition single-threaded and linear in
layers (#33), over-budget depth conversions hold both copies (#34).
Memory model: [memory.md](memory.md).

## 17. Risks

- Real pens may behave differently from synthetic injection (pressure
  curves, batching, timestamps, tilt).
- Golden tolerances vs. other CPUs/compilers are unverified.
- 16-bit documents double pixel and history memory.
- `.mirae` v1 is now a compatibility promise: any format change needs a
  version decision (docs/formats/mirae-v1.md, ADR 0019) and must keep
  the v1 fixtures loading.
- The UI crate will grow; keep it free of authoritative state.
- Someone else sometimes uses the development machine while an agent
  drives it (seen in 4G and again in 5A: a new document and an export
  dialog appeared in a running Mirae). Check the foreground window before
  every synthetic click, never answer dialogs you did not open, and build
  with a separate `CARGO_TARGET_DIR` if `tela.exe` is in use.

## 18. Hardware validated

GPU: AMD Radeon RX 7700 XT (Vulkan). CPU: Ryzen 7 5800X. Pen: **only** the
virtual HID "Microsoft HID RID 000D_0002" with synthetic injection. Never
claim physical-tablet validation that has not happened.

## 19. Golden tests

`crates/corpus/golden/*.png`, compared by `cargo test`. Update only
deliberately: `TELA_UPDATE_GOLDEN=1 cargo test --release -p tela-corpus --test golden`
(exact value `1`), after inspecting the images. Also the blend sheet
(`document/tests/blend_sheet.rs`) and the integrated session golden
(`corpus/tests/pipeline.rs`). [golden-tests.md](golden-tests.md).

## 20. Current performance

0 allocations per dab (sequential path; parallel: rayon queue blocks only); ~375 k dabs/s at 5 px, ~490 dabs/s at 500 px hard
(single thread), ~3× parallel at 500 px; 0.107–0.13 ms per composed tile
(2 layers); with every layer drawing in the tile 1.6 ms (10 Normal
layers), 12.4 ms (100), 44 ms (100 Soft Light).
[performance.md](performance.md).

## 21. Next steps

1. Ask the owner whether 5B (or something else) is approved; record the
   decision in [phase5/PLAN.md](phase5/PLAN.md) and follow it one
   subphase at a time; never start a phase or subphase without explicit
   approval.
2. Close every subphase with its parity gate ([PARITY.md](PARITY.md)).
   Where the reference defines a behaviour (clipping, masks, alpha lock,
   blend results), observe it **before** implementing.
3. Opportunistically: validate on a real tablet if one becomes available.

## 21a. Comparing with the reference

Clip Studio Paint EX 5.1.5 (trial) is installed on the development machine
(`C:\Program Files\CELSYS\CLIP STUDIO 1.5\CLIP STUDIO PAINT\CLIPStudioPaint.exe`).
Drive it as a user: start it, maximize it, click/drag with
`SetCursorPos` + `mouse_event`, send keys with `keybd_event`, and read the
result from screen captures (`Graphics.CopyFromScreen`) and pixel
measurements. Its UI is in Portuguese. Bring it to the front by sending an
Alt key event before `SetForegroundWindow`, and use `ShowWindow(h, 3)`;
`SW_RESTORE` un-maximizes it. Layer deletion asks for confirmation (Enter
confirms); never tick "do not show again" or change its settings. Do not
right-click the canvas with a drawing tool: it picks the colour. **Never**
open its install or user-data files (presets, settings, databases); see
[CLEAN_ROOM.md](CLEAN_ROOM.md#black-box-comparison). Record each comparison
in `docs/parity/`.

## 22. Do not change without strong justification (and an ADR)

- Premultiplied alpha; the stroke-buffer accumulation formula; the shared
  preview/commit path.
- `StrokeRng` stream layout and property `slot` numbers (changes every
  preset's random pattern).
- `DabSpacer` arc-length carry; airbrush time-instant scheme.
- Preset format semantics (`FORMAT_VERSION`, migration, lenient loading);
  built-in ids.
- `PARALLEL_MIN_WORK` and other tuned constants (re-benchmark first).
- Crate boundaries and the dependency table.
- Golden references (only via the explicit update procedure).
- The One Euro / Catmull–Rom defaults (re-run the phase 2 benchmarks).

## 23. Clean-room policy

No decompiled code, pseudocode, third-party assembly, proprietary assets,
shaders, presets, or undocumented proprietary formats/schemas — and nothing
from the earlier binary analysis of a commercial app in the developer's
history. Public math/papers and observed behaviour are fine.
[CLEAN_ROOM.md](CLEAN_ROOM.md).

## 24. Definition of Done (any change)

- Builds; `cargo test --workspace --release` passes (old tests unchanged
  unless the invariant changed deliberately).
- `cargo fmt --all -- --check` and clippy `-D warnings` clean; strict
  rustdoc clean (`scripts/check.ps1` passes).
- New math has unit tests; new external input has garbage-input tests.
- Hot-path changes re-run `stamp_bench` (allocs/dab stays 0.000).
- Visual changes verified in the running app (and goldens regenerated
  deliberately if presets change).
- Docs updated when architecture, invariants or formats change (ADR if a
  decision changes); STATUS/CHANGELOG at phase end.
- Small commits, message explains why. AI-authored commits in this
  repository end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
  (use the attribution your own session specifies, if different).
- Phase boundaries respected: never advance a phase automatically.
- At the end of a subphase: its parity gate is recorded in
  [PARITY.md](PARITY.md) and `docs/parity/` (implemented, compared,
  remaining differences).
