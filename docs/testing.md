# Testing

All automated tests run with:

```sh
cargo test --workspace --release
```

(`--release` because pixel loops are slow unoptimized; the dev profile is
already `opt-level = 1`, so plain `cargo test` also works, slower.) The full
gate, including fmt, clippy, docs and the release build, is
`scripts/check.ps1` (see [CONTRIBUTING.md](../CONTRIBUTING.md)).

Counts at the end of phase 3.5: **207 tests**, all passing (phase 3 ended
with 188). GPU tests skip themselves when no adapter is available.

## Kinds of tests

### Unit tests

Inside each module (`#[cfg(test)] mod tests`). They pin down the math of
one component: compositing (`color`), tile addressing and view transform
(`canvas`), curves, effectors, follow, spacing, tips, textures, scatter,
stabilizers, interpolators, stroke buffer, preset I/O and validation
(`brush`), composition (`document`), mips (`renderer`), normalization and
clock (`tablet`), curve-editor geometry (`ui`), tools selection (`app`).

### Integration tests

`crates/<crate>/tests/*.rs`, using only the public API.

| File | What |
|---|---|
| `brush/tests/dab_placement.rs` | Sample-rate invariance of dab placement (2 vs 1000 samples). |
| `brush/tests/stabilizer_comparison.rs` | Stabilizer jitter/lag trade-offs on the synthetic corpus; One Euro not dominated. |
| `brush/tests/interpolation_comparison.rs` | Interpolators against the true path; no kinks, no cusps. |
| `brush/tests/airbrush.rs` | Frame-rate independence, rest deposition, no beads under late delivery. |
| `brush/tests/scatter_dynamics.rs` | Scatter driven by effectors; reproducibility. |
| `brush/tests/texture_canvas_space.rs` | Grain fixed in canvas space. |
| `brush/tests/properties.rs` | Property tests (below). |
| `brush/tests/allocation.rs` | Zero allocation per sample/dab (thread-local counting allocator). |
| `canvas/tests/properties.rs` | Property tests for tiles and view. |
| `renderer/tests/display_quality.rs` | Headless GPU: seams, 100% identity, magnified texels. |
| `corpus/tests/golden.rs` | Golden sheets (below). |
| `app/tests/crate_boundaries.rs` | Crate dependency boundaries. |

### Invariant tests

Every invariant in [architecture/invariants.md](architecture/invariants.md)
names the tests that protect it. When you change behaviour, find the
invariant first; if a test must change, the invariant probably does too
(write an ADR).

### Property tests

`proptest` (dev-dependency) generates inputs; failures shrink to a minimal
case and are recorded under `proptest-regressions/` (commit those files).
Covered: curve range/monotonicity/edit validity, `angle_delta`/`follow`,
`StrokeRng` purity and ranges, preset validation idempotence (incl.
oversized effector lists and non-finite values), spacing independence of
sample density, tile addressing, `IRect::tiles`, view round trip and
anchors; the layer stack against a reference model (512 sequences); undo/
redo against a stack of full-content digests (300 sequences of edits,
undo and redo) and under tiny limits. Case counts are kept modest
(48–512) so the suite stays fast. Each history test has been checked
against planted bugs (undo ignoring the active layer, absent tiles not
restored, an extra tile copy).

### Parity tests (measured reference data)

`crates/document/tests/blend_parity.rs` composes the 3 400 probe
combinations through `Document` and compares them with the reference's
measured results in `crates/document/tests/reference/` (provenance in its
README, method in docs/parity/4C-blend.md). To re-measure:
`blend_probe psd`, open and export in the reference as described in the
parity record, `blend_probe read`.

`crates/document/tests/clip_parity.rs` does the same for masks and
clipping: the 34 `clip_probe` scenarios (2 × 2 176 swatches, with and
without the reference's paper) built through the public `Document` API,
within 2/255 (method in docs/parity/4D-masks-clipping-alpha-lock.md;
`clip_probe psd`, export, `clip_probe read`; `clip_probe fit` compares
the candidate clipping models). `mask_clip_properties.rs` property-tests
mask and clipping composition over random stacks.

### Visual / golden tests

`crates/corpus/tests/golden.rs` compares every built-in preset's sheet
with `crates/corpus/golden/*.png`. Details, tolerances and the update
procedure: [golden-tests.md](golden-tests.md).

### Benchmarks

Not part of `cargo test`; run them when touching hot paths and compare with
[performance.md](performance.md):

```sh
cargo run --release -p tela-corpus --example stamp_bench        # stamping, allocations, composition
cargo run --release -p tela-corpus --example history_bench      # commit/undo/redo time, history memory per stroke
cargo run --release -p tela-corpus --example blend_bench        # composition ms/tile per blend mode, 2 and 10 layers
cargo run --release -p tela-corpus --example blend_fit          # sRGB-encoded vs linear blending against the reference data
cargo run --release -p tela-corpus --example mask_clip_bench    # masks, clipping groups, alpha lock: compose, preview, commit, mask history
cargo run --release -p tela-brush --example stabilizer_bench     # phase 2 stabilizer table
cargo run --release -p tela-brush --example interpolation_bench  # phase 2 interpolator table
```

### Manual tests (running app)

Visual or interaction changes must be checked in the real app, not only in
tests:

```sh
cargo run --release -p tela-app
```

Checklist used in phases 2–3 (screenshots in `docs/phase2/`, `docs/phase3/`):
line art (circle, S, spiral, V, diagonals); pencil shading and hatching;
marker overlap; airbrush fast drags (no beads) and rests (build-up); chalk;
sizes from 5 px (zoomed in) to 500+ px; rotated canvas; zoom far out (no
seams) and in (crisp texels); curve editor (double-click adds, drag moves,
right-click removes); pen over the panel moves sliders; pen eraser end
erases.

Phase 4 additions (4A/4B): create/select/hide/lock/reorder/delete layers
and opacity; Ctrl+Shift+N during a stroke is ignored; stroke undo/redo
(Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z, buttons); ten strokes undone and redone
with identical pixel counts; create/delete/reorder undo and the active
layer afterwards; two opacity drags = one undo, split by a selection
change; visibility skipped by undo; lock and clear undo; Ctrl+Z during a
stroke ignored; undo after synthetic-pen strokes. Pixel counts are read
from screen captures (see docs/parity/ for the method).

### Hardware tests

**No physical tablet has been tested.** The only pen device on the
development machine is a virtual HID ("Microsoft HID RID 000D_0002"). Pen
input is exercised end to end through the real `WM_POINTER` path with
synthetic Windows Ink injection:

Bash:

```sh
cargo build --release -p tela-app -p tela-tablet --examples
target/release/tela.exe &          # start the app, wait for its window
TELA_INJECT_REGION="0.26,0.02,0.88,0.98" target/release/examples/inject_pen.exe "Tela" all
```

PowerShell:

```powershell
cargo build --release -p tela-app -p tela-tablet --examples
$app = Start-Process target\release\tela.exe -PassThru; Start-Sleep 3
$env:TELA_INJECT_REGION = "0.26,0.02,0.88,0.98"
target\release\examples\inject_pen.exe "Tela" all
Stop-Process -Id $app.Id
```

Patterns: `all`, `pressure`, `fast`, `detail`, `eraser`,
`drag:x0,y0,x1,y1[,pressure]`, `hold:u,v,seconds[,pressure]` (client-area
fractions). Set `TELA_INJECT_REGION="0.26,0.02,0.88,0.98"` to keep
patterns off the side panels. Limits: injection sends integer pixel
positions (HIMETRIC sub-pixel mapping untested), and tilt/rotation were not
validated against a real pen. When real hardware is available, record the
device, driver and results in docs/STATUS.md.

`cargo run --release -p tela-tablet --example pointer_devices` lists the
pointer devices Windows reports (type, name, rectangles) — use it to see
whether a real tablet is visible.

Useful logging: `RUST_LOG=info,tela::ink=debug,tela::latency=debug,tela::stroke=debug`.

## Writing tests

- Prefer testing a property ("does not depend on X") over a magic output.
- Floating-point comparisons use explicit tolerances; bit-exactness is
  asserted only where it is an invariant (determinism, parallel stamping).
- External-input code gets a garbage-input test (no panic).
- New visual behaviour in a built-in preset updates its golden in the same
  commit, after looking at the image.
