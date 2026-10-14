# Changelog

Organized by project phase (no releases yet). Commit hashes point to the
main change; see `git log` for everything.

## Phase 4A — document model, layer API, stable ids

### Added
- `docs/PARITY.md` and `docs/parity/`: parity matrix with the reference
  application, black-box comparison rules, parity gate per subphase;
  approved phase 4 plan in `docs/phase4/PLAN.md` (615fea5).
- Layer stack API with stable `LayerId`s, relative `Placement`,
  `LayerError`, lock, `paint_target`, default "Layer N" names (19e3b90);
  ADR 0014.
- `StrokeBuffer::commit_tile` (allocation-free per-tile commit) (b3837e9).
- Property test of the stack against a reference model (9213da8).
- Layer panel (right side) and Ctrl+Shift+N (06ccba9).

### Changed
- `Document` fields are private; pixels change only through
  `modify_tiles`/`clear_layer`; every mutation returns the tiles it
  invalidated (19e3b90).
- `Overlay::above_layer` (index) became `Overlay::target` (`LayerId`); a
  hidden target shows no preview (19e3b90).
- The app fixes a stroke's target layer when it begins, refuses strokes on
  hidden or locked layers and ignores layer commands during a stroke
  (19e3b90, 06ccba9).
- `commit_into` no longer allocates a 1 MiB scratch buffer (b3837e9).

### Parity
- 4A gate recorded: 14 reference observations compared; remaining
  differences in `docs/parity/4A-layers.md`.

## Phase 3.5 — architecture audit and handoff

### Architecture
- Crate boundaries are enforced by `crates/app/tests/crate_boundaries.rs`
  (exact allowed dependency set per crate) and documented in
  `docs/architecture/crate-dependencies.md` (9e5c3da).
- The stroke-override edit rule moved from the app and the UI into
  `StrokeOverrides::edit_resolved` (dbb7d05).
- Golden references moved from `docs/phase3/corpus` to
  `crates/corpus/golden`; `tela-corpus` owns their directory, naming and
  update switch (660f328).

### Changed
- `InkBackend::attach` takes `&impl HasWindowHandle` instead of a raw HWND;
  new `InkError::UnsupportedWindow` (630f607).
- `normalize::tilt_degrees`/`rotation_degrees` (which returned radians)
  renamed `ink_tilt`/`ink_rotation` (630f607).
- Brush API reduced: `airbrush` module, `resolve_dab`, `DabSpacer`,
  `DabSite`, `mix64` are crate-private; `PressureCurve` and
  `TimeDeposit::emitted` removed; `analysis::Rng` renamed `SyntheticRng`
  (6878d9e).
- Built-in preset ids are constants (`preset::builtin::ids`) with
  `builtin_preset(id)`; `StabilizerKind::next`, `InterpolationKind::next`
  (dbb7d05).
- Workspace declares `rust-version = 1.95`, centralizes `windows-sys`,
  requires `// SAFETY:` on every unsafe block, denies
  `unsafe_op_in_unsafe_fn`; `tela-document` is a dev-dependency of
  `tela-corpus` (da4eee8).

### Fixed
- Preset `format_version` above `u32::MAX` wrapped to 0 and ran the v0
  migration; it now counts as newer (2429345).
- More than 64 effectors on a property shared random streams with the next
  property; validation now truncates and evaluation never reads past 64
  (2429345).
- `-`/`=` and `S`/`I` keys edited the hidden user value instead of the
  value shown when a preset overrode it (Ink Pen: 30 → 40 instead of 40 → 50)
  (dbb7d05).
- `TELA_UPDATE_GOLDEN` updated goldens for any value, including `0`; now
  only `1` (660f328).
- `cargo run -p tela-corpus` without arguments silently overwrote the golden
  references; it now requires an output directory and refuses the golden
  directory.
- The "0 allocations per dab" claim was only measured on the sequential
  path; a new test pins it for sequential stamping and per-sample
  processing, and documents the parallel path's amortized rayon
  allocations.

### Added
- `brush/tests/allocation.rs`: zero allocation per sample/dab, tested.
- Property tests (`proptest`): curves, angles, RNG, preset validation
  idempotence, spacing vs sample density, tiles, view transform (12c97c2).
- Crate-level and API documentation with units and domains (9a96e2c).
- `docs/architecture/` (ARCHITECTURE, invariants, unsafe, error handling,
  concurrency, UI), 13 ADRs, testing, golden tests, performance, status,
  roadmap, technical debt, clean-room, AI handoff, AGENTS.md,
  CONTRIBUTING.md, this changelog; `scripts/check.ps1`; broader
  `.gitignore`.

### Performance
- Unchanged (re-measured: within ±2% of phase 3; 0 allocations per dab).

## Phase 3 — professional brush engine

### Added
- Single mipmapped display texture with AA-nearest magnification (6376365).
- Generalized effectors (10 sources, 6 combine modes) and per-stroke
  deterministic RNG (3d1eb95).
- Procedural and image tips with angle, aspect, flips (457ccae).
- Deterministic scatter driven by effectors (89a6362).
- Canvas-space paper textures (d194b63).
- Versioned JSON presets with migration and 8 built-ins (f2cfb21).
- Frame-rate independent airbrush (f0350e7), dwell-time flow (f8e2fff),
  rest detection with delivery-lag compensation (afad6b4).
- Eraser compositing and automatic stylus-eraser switching (164a097).
- Golden corpus, continuous angle follow (67ea1b0).
- Pen passthrough to UI regions (e76c4d2); egui brush panel and curve
  editor (6b06667).

### Performance
- Stamping benchmark; texture sampling with bit-mask wrapping (c4f9fd0).
- Per-tile parallel stamping, bit-identical (8b212a4): ~1.7× at 250 px,
  ~3× at 500 px on 16 threads.

### Fixed
- Airbrush beads on fast strokes under late input delivery (afad6b4).

## Phase 2 — tablet input and stroke quality

### Added
- Brush dynamics: monotone curves, effectors, brush properties (295385f).
- Input stages: velocity, jitter filter, four stabilizers (799ffaf).
- Synthetic stroke corpus, metrics, stabilizer comparison (3b9183d).
- Interpolators: linear, centripetal Catmull–Rom, smoothed Bézier (88afab8).
- Integrated stroke pipeline with opacity ceiling and tapers (00e25f4).
- Tablet crate: neutral samples, normalization, Windows Ink backend with
  history and QPC timestamps (03201c9); one pointer path for mouse and pen
  (2f6c30e); synthetic pen injector (6befe87).

### Performance
- Composition into reusable scratch buffers (438dc7f).

### Architecture
- One Euro and centripetal Catmull–Rom chosen as defaults by benchmark
  (ADRs 0005, 0006).

## Phase 1 — foundation

### Added
- Workspace and color crate with premultiplied compositing (b8fb5d8).
- Canvas crate: sparse 256 px tiles, dirty set, view transform (715992b).
- Brush crate: input, dab spacing, round tip, stroke buffer (b756743).
- Document crate: raster layers and per-tile composition (764714d).
- wgpu renderer (90f7623).
- Desktop app: mouse painting, pan, zoom, rotation (e70a5c9).
