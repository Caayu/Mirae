# Agent instructions

1. Read `docs/AI_HANDOFF.md` first, then `docs/architecture/ARCHITECTURE.md`.
2. Run the baseline before changing anything:
   `powershell -ExecutionPolicy Bypass -File scripts\check.ps1` (or
   `cargo test --workspace --release` + clippy `-D warnings`). Note the test
   count.
3. Preserve crate boundaries (`docs/architecture/crate-dependencies.md`,
   enforced by `crates/app/tests/crate_boundaries.rs`).
4. Preserve the invariants in `docs/architecture/invariants.md`. Changing
   one requires an ADR in `docs/adr/`.
5. No large rewrites. Small, reviewable steps; refactor only for a concrete
   gain.
6. Write tests: unit tests for math, garbage-input tests for external
   input, property tests where they add value.
7. For visual or interaction changes, run the real app
   (`cargo run --release -p tela-app`; synthetic pen: `inject_pen`, see the
   handoff) — tests alone are not enough. Never claim physical-tablet
   validation that did not happen.
8. Hot-path changes: re-run `stamp_bench`; allocations per dab must stay
   0.000; never trade determinism for speed.
9. Update docs when architecture, formats or invariants change; update
   `docs/STATUS.md` and `CHANGELOG.md` at the end of a phase.
10. Small commits with explanatory messages ending with the repository's
    `Co-Authored-By:` line. Never rewrite history.
11. Clean-room: no decompiled code, proprietary assets, shaders, presets or
    formats (`docs/CLEAN_ROOM.md`).
12. Never advance to the next phase automatically; wait for the owner.
