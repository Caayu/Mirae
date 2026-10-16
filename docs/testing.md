# Testing

All automated tests run with:

```sh
cargo test --workspace --release
```

(`--release` because pixel loops are slow unoptimized; the dev profile is
already `opt-level = 1`, so plain `cargo test` also works, slower.) The full
gate, including fmt, clippy, docs and the release build, is
`scripts/check.ps1` (see [CONTRIBUTING.md](../CONTRIBUTING.md)).

Counts at the end of phase 4: **401 tests**, all passing (phase 3.5
ended with 207, phase 3 with 188). GPU tests skip themselves when no
adapter is available.

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
| `io/tests/round_trip.rs` | `.mirae` save → load bit-exact at both depths, determinism, compression levels, atomic save on disk, the identification bytes. |
| `io/tests/corrupt.rs` | Malformed, truncated, flipped, hostile and oversized files: each a specific `IoError`, never a panic. |
| `io/tests/fixture.rs` | The v1 compatibility fixtures (never regenerated) load as pinned. |
| `io/tests/png.rs` | PNG export against the compositor, import precision for every (colour, alpha) pair, 16-bit images, centring, colour types, interlaced images, limits. |
| `io/tests/png_parity.rs` | PNG import/export against the reference's own exports (below). |
| `io/tests/properties.rs` | Random documents round-trip exactly; random bytes and random damage never panic or load as a different document. |
| `corpus/tests/scenarios.rs` | End-to-end scenarios A–G (phase 4G): real strokes on combined layers, modes, masks, clipping, alpha lock, both depths, conversion, history walks and save/load (below). |
| `corpus/tests/pipeline.rs` | One whole drawing session through every phase-4 feature, its history, conversion and files; its composite against a golden image. |
| `corpus/tests/random_sessions.rs` | 16 seeded random sessions of the same operations; invariants after every step. |
| `corpus/tests/stress.rs` | Hundreds of layers, many sparse 16-bit layers, histories past the step limit, repeated save/load cycles, fully tiled 4096² layers. |

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
restored, an extra tile copy). Phase 4E adds depth to both (wrong-depth
edits, conversions among random edits) and `depth_properties.rs`
(widened 8-bit stacks compose identically, conversion keeps structure
and undoes exactly, narrowing error, valid 16-bit composites, refused
edits change nothing).

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

`crates/io/tests/png_parity.rs` (phase 4F) imports the `png_probe` test
images (`crates/io/tests/reference/`) into a 1600 × 1200 document,
exports, and compares every pixel with the reference's exports of the
same steps (`docs/parity/4F/csp_*.png`; method in
docs/parity/4F-file-workflow.md).

Both parity tests also run in **16-bit documents** (phase 4E): the probe
colours stored at 16-bit precision against the reference's 8-bit
measurements (the reference has no 16-bit documents), within 2/255.

### End-to-end tests (phase 4G)

`crates/corpus/tests/` drives the whole stack through public APIs as the
app does: `BrushEngine` strokes committed into the active target,
`Document` commands, the history and `tela-io`. The helpers in
`support/mod.rs` check after every step:

- **preview == commit** for every stroke (composites compared; when one
  side is empty, what is shown over the paper — see invariant I2);
- the document's public invariants and premultiplied pixels (`check`);
- digests of the content, of what undo restores (`history_digest`, which
  leaves out selection, visibility and the next id, as the history does)
  and of composites;
- save → load → identical content, and identical bytes on a second save
  (`round_trip`).

Random sessions use a fixed SplitMix generator; a failing seed is printed
and can be rerun alone with `MIRAE_SEED=<seed>`. The random sessions were
checked against a planted bug (all-revealing mask tiles no longer
released, breaking invariant M3) and caught it.

Which test protects what (regression map):

| Area | Unit / property | Parity data | End-to-end |
|---|---|---|---|
| Layers (4A) | `document` unit tests, `properties.rs` (reference model) | — | scenarios A, G; random sessions |
| History (4B) | `history::tests`, `history_properties.rs` | — | every scenario (walk back and forth); stress |
| Blend modes (4C) | `blend` unit tests | `blend_parity.rs`, `blend_sheet.rs` | scenarios B, D, F |
| Masks, clipping, alpha lock (4D) | unit tests, `mask_clip_properties.rs` | `clip_parity.rs` | scenarios C, D, E |
| 16-bit (4E) | `depth_properties.rs`, `banding.rs` | both parity tests at 16 bits | scenario F, pipeline |
| Files (4F) | `io` unit tests, `corrupt.rs`, `properties.rs`, `fixture.rs` | `png_parity.rs` | scenarios B, C, G; stress (40 cycles) |
| Appearance | brush goldens | — | `pipeline.rs` golden |

### Precision tests (phase 4E)

`crates/corpus/tests/banding.rs` keeps the banding gate: repeated faint
strokes stay within one display level of the exact result at 16 bits and
miss by many levels at 8; a single stroke and a stored ramp show the
same at both depths. The full table: `banding` example.
`docs/parity/4E/` keeps the reference's exports of the depth probe
(`depth_probe read-png`, `read-levels`).

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
cargo run --release -p tela-corpus --example depth_bench        # 8 vs 16-bit: memory, composition, strokes, history, conversion
cargo run --release -p tela-io --example io_bench                # .mirae save/load: time, size, MB/s, peak memory, deflate levels
cargo run --release -p tela-corpus --example doc_bench          # 1-100 layers per mode, realistic 4096² documents, history, memory (--quick: ~3 s)
cargo run --release -p tela-corpus --example banding            # 8 vs 16-bit banding table
cargo run --release -p tela-brush --example stabilizer_bench     # phase 2 stabilizer table
cargo run --release -p tela-brush --example interpolation_bench  # phase 2 interpolator table
```

`scripts/check.ps1 -Bench` runs `stamp_bench` and `doc_bench -- --quick`;
the latter fails the gate if composition allocates (invariant F3).

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

Phase 4E additions: New 8-bit / New 16-bit; 60 identical faint strokes
(Soft Round 200 px, opacity 0.03) at both depths, profile read across the
edge at zoom ≈ 1.09 (8-bit: hard edge and plateau; 16-bit: smooth);
To 8-bit, undo, redo, To 16-bit with the profile unchanged; alpha lock,
clipping, mask erase/reveal and layer opacity in a 16-bit document;
undo back through the conversion; synthetic-pen strokes in a 16-bit
document (`docs/parity/4E-16bit.md`, "Running app").

Phase 4G session (8- and 16-bit, mouse and synthetic pen): layers,
Multiply, clipping, alpha lock, mask erase, undo/redo, Ctrl+S, Save As,
close, reopen, PNG export and re-import (`docs/parity/4G-integration.md`).
On the development machine **Ctrl+Shift+S never reaches Mirae**: another
program registers it as a global hotkey (`RegisterHotKey` fails with
error 1409 for it). Use Shift+Alt+S for Save As there; try another
machine before treating a missing shortcut as a Mirae bug.

### Hardware tests

**No physical tablet has been tested.** The only pen device on the
development machine is a virtual HID ("Microsoft HID RID 000D_0002"). Pen
input is exercised end to end through the real `WM_POINTER` path with
synthetic Windows Ink injection:

Bash:

```sh
cargo build --release -p tela-app -p tela-tablet --examples
target/release/tela.exe &          # start the app, wait for its window
TELA_INJECT_REGION="0.26,0.02,0.88,0.98" target/release/examples/inject_pen.exe "Mirae" all
```

PowerShell:

```powershell
cargo build --release -p tela-app -p tela-tablet --examples
$app = Start-Process target\release\tela.exe -PassThru; Start-Sleep 3
$env:TELA_INJECT_REGION = "0.26,0.02,0.88,0.98"
target\release\examples\inject_pen.exe "Mirae" all
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
