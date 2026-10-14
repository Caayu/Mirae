# READ THIS FIRST WHEN STARTING A NEW AI SESSION.

This file is enough to continue the project without any previous
conversation. Read it fully, then `docs/architecture/ARCHITECTURE.md`.

## 1. Mission

Build **Tela**, a professional raster illustration editor in Rust (wgpu +
winit), aiming over many phases at behaviour comparable to established
illustration software, developed strictly **clean-room**
([CLEAN_ROOM.md](CLEAN_ROOM.md)). Quality bar: a Rust codebase that looks
maintained by an experienced team — small commits, tests for math,
measured performance, documented decisions.

## 2. Current state

Phase 3.5 (architecture audit/handoff) is complete. **Phase 4 has not
started and must not start without the owner's explicit approval.** 207
tests pass; fmt/clippy/doc are clean. Facts: [STATUS.md](STATUS.md).

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
cargo test --workspace --release        # 207 tests, ~5 s after build
```

## 7. Clippy and the full gate

```sh
cargo clippy --workspace --all-targets --release -- -D warnings
powershell -ExecutionPolicy Bypass -File scripts\check.ps1   # fmt, clippy, test, strict doc, build
```

## 8. Benchmarks

```sh
cargo run --release -p tela-corpus --example stamp_bench        # ~1 min
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
TELA_INJECT_REGION="0.26,0.02,0.99,0.98" target/release/examples/inject_pen.exe "Tela" all
```

PowerShell: `$env:TELA_INJECT_REGION = "0.26,0.02,0.99,0.98"; target\release\examples\inject_pen.exe "Tela" all`
(full sequence in docs/testing.md#hardware-tests).

Patterns: `all`, `pressure`, `fast`, `detail`, `eraser`,
`drag:x0,y0,x1,y1[,pressure]`, `hold:u,v,seconds[,pressure]` (fractions
of the client area). `cargo run -p tela-tablet --example pointer_devices`
lists the pen devices Windows sees. Pitfalls on this machine: PowerShell swallows a bare
`--` argument; the pt-BR locale formats decimals with commas (use
`[string]::Format([cultureinfo]::InvariantCulture, ...)` when generating
numbers in PowerShell); kill a running `tela.exe` before rebuilding or the
link step fails with "Acesso negado".

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

`color` (premultiplied color), `canvas` (sparse tiles, view), `brush`
(engine), `document` (layers, composition), `renderer` (wgpu), `tablet`
(input, Windows Ink), `ui` (egui), `corpus` (goldens, benchmark), `app`
(binary `tela`).

## 12. Allowed dependencies

Exact table and reasons: [architecture/crate-dependencies.md](architecture/crate-dependencies.md).
Enforced by `crates/app/tests/crate_boundaries.rs`. Do not add a
dependency for convenience.

## 13. Critical invariants

Full list with tests: [architecture/invariants.md](architecture/invariants.md). The ones most
easily broken:

1. Same preset + seed + samples ⇒ bit-identical dabs (counter-based RNG).
2. Dab placement independent of sample density.
3. Preview == committed result (shared `paint_over_tile`, live tail).
4. Parallel stamping bit-identical to sequential.
5. Airbrush independent of frame rate.
6. Invalid presets never panic; unknown values are neutral.
7. 0 allocations per sample/dab on the sequential path (tested); the
   parallel path only has rayon's amortized queue blocks; composition is
   allocation-free.
8. Crate boundaries (brush ⟂ tablet/UI/GPU/document; renderer ⟂ document).

## 14. Architectural decisions

[adr/README.md](adr/README.md): premultiplied alpha, sparse 256 px tiles,
separate stroke buffer, CPU composition + GPU display texture, One Euro,
centripetal Catmull–Rom, counter-based RNG, versioned JSON presets,
canvas-space textures, bit-identical parallel stamping, egui isolation,
golden corpus, time-instant airbrush.

## 15. Completed features

Phases 1–3 + 3.5; see [STATUS.md](STATUS.md#features) and
[CHANGELOG.md](../CHANGELOG.md). Phase reports: [phase2](phase2/README.md),
[phase3](phase3/README.md) (phase 1 has no separate report; it is
summarized in the changelog).

## 16. Limitations

[TECHNICAL_DEBT.md](TECHNICAL_DEBT.md): no physical tablet validation,
8-bit storage, GPU texture size limit, no undo, no I/O, UI edits not
persisted, goldens verified on one machine.

## 17. Risks

- Real pens may behave differently from synthetic injection (pressure
  curves, batching, timestamps, tilt).
- Golden tolerances vs. other CPUs/compilers are unverified.
- Phase 4 touches storage (16-bit) and the document API at once; keep
  those changes in separate, measured steps.
- The UI crate will grow; keep it free of authoritative state.

## 18. Hardware validated

GPU: AMD Radeon RX 7700 XT (Vulkan). CPU: Ryzen 7 5800X. Pen: **only** the
virtual HID "Microsoft HID RID 000D_0002" with synthetic injection. Never
claim physical-tablet validation that has not happened.

## 19. Golden tests

`crates/corpus/golden/*.png`, compared by `cargo test`. Update only
deliberately: `TELA_UPDATE_GOLDEN=1 cargo test --release -p tela-corpus --test golden`
(exact value `1`), after inspecting the images. [golden-tests.md](golden-tests.md).

## 20. Current performance

0 allocations per dab (sequential path; parallel: rayon queue blocks only); ~375 k dabs/s at 5 px, ~490 dabs/s at 500 px hard
(single thread), ~3× parallel at 500 px; 0.107 ms per composed tile.
[performance.md](performance.md).

## 21. Next steps

1. Wait for the owner to approve phase 4 (do not start it on your own).
2. When approved, refine the phase 4 proposal in [ROADMAP.md](ROADMAP.md)
   into a plan (layer API and undo first, then blend modes, masks/clipping/
   alpha lock, then 16-bit storage, then I/O), with ADRs for the storage and
   file-format decisions.
3. Opportunistically: validate on a real tablet if one becomes available.

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
