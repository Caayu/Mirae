# Contributing

## Setup and toolchain

- Rust via [rustup](https://rustup.rs), stable, **≥ 1.95** (egui's
  minimum). Verified with 1.99.0. On Windows use the MSVC toolchain.
- Windows 10/11 for pen input (Windows Ink). The engine crates are
  platform-independent; the app builds elsewhere without pen support.
- A GPU supported by wgpu (Vulkan, DX12 or Metal). GPU tests skip without
  one.
- If `cargo` is not found in Git Bash: `export PATH="$HOME/.cargo/bin:$PATH"`.

## Everyday commands

| Task | Command |
|---|---|
| Build | `cargo build --workspace --release` |
| Run | `cargo run --release -p tela-app` |
| Format | `cargo fmt --all` (check: `cargo fmt --all -- --check`) |
| Lint | `cargo clippy --workspace --all-targets --release -- -D warnings` |
| Test | `cargo test --workspace --release` |
| Docs | `cargo doc --workspace --no-deps` (CI-strict: `RUSTDOCFLAGS="-D warnings"`) |
| Everything | `powershell -ExecutionPolicy Bypass -File scripts\check.ps1` (`-Quick`, `-Bench`) |
| Benchmarks | `cargo run --release -p tela-corpus --example stamp_bench` (and `tela-brush` examples `stabilizer_bench`, `interpolation_bench`) |
| Golden update | `TELA_UPDATE_GOLDEN=1 cargo test --release -p tela-corpus --test golden` (deliberately; see docs/golden-tests.md) |
| Synthetic pen | `target/release/examples/inject_pen.exe "Tela" all` with the app running |

## Architecture rules

- Respect the crate graph in `docs/architecture/crate-dependencies.md`; the
  boundary test enforces it. New dependencies need a row, a reason and the
  test table updated — never add one for convenience.
- Keep the invariants in `docs/architecture/invariants.md`. A change to an
  invariant needs an ADR.
- Brush engine: no device, UI, GPU or document knowledge; no allocation per
  dab/sample; deterministic (no hidden RNG state).
- Renderer: never sees the document.
- UI: no authoritative state; domain rules belong in domain crates.
- `unsafe`: only in FFI modules that opt in, every block with `// SAFETY:`
  (`docs/architecture/unsafe.md`).
- Errors: no panics on external input (`docs/architecture/error-handling.md`).
- Rust style: clear ownership, enums for exclusive states, newtypes where
  they prevent real unit bugs, typed errors in library crates, no
  `Arc<Mutex<_>>` by default, no `Manager/Helper/Utils` names, no magic
  numbers (name and document tuned constants with their unit).

## Tests

See `docs/testing.md`. In short: unit tests for math, garbage-input tests
for external input, property tests where invariants are general, goldens
for visual output, the running app for interaction, and an honest note when
something could only be tested synthetically.

## Commits

- Small and focused; refactors separate from behaviour changes.
- Message: imperative subject (≤ 72 chars), body explaining *why*, numbers
  for performance claims, and what was verified (tests, app, benchmark).
- End with the repository's `Co-Authored-By:` line when applicable.
- Never rewrite or squash published history.

## ADRs

Write one in `docs/adr/` (template in its README) when a change alters an
invariant, a crate boundary, a file format, a benchmark-chosen default, or
anything a future contributor might otherwise revert. Do not edit old ADRs'
decisions; supersede them.

## Definition of Done

- [ ] `scripts/check.ps1` passes (fmt, clippy `-D warnings`, all tests,
      strict rustdoc, release build).
- [ ] No existing test was weakened without an explicit reason.
- [ ] New behaviour has tests; external input has failure tests.
- [ ] Hot-path changes benchmarked; allocations per dab still 0.000.
- [ ] Visual changes checked in the running app; goldens regenerated
      deliberately if needed.
- [ ] Docs/ADRs updated if architecture, formats or invariants changed.
- [ ] Clean-room policy respected.
- [ ] No phase started without the owner's approval.
