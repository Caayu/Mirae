# Changelog

Organized by project phase (no releases yet). Commit hashes point to the
main change; see `git log` for everything.

## Phase 4F — document I/O (the product is now named Mirae)

### Added
- `png_probe` (PNG test images of our own, reads the reference's exports
  against models) and `docs/parity/4F-file-workflow.md` part 1: the
  reference's file workflow, prompts, shortcuts, PNG export and import
  observed before implementing (9f21130).
- `Document::revision` (save point from history serials; visibility
  counts, selection does not), `Document::from_parts` with
  `DocumentParts`/`LayerParts`/`MaskParts`/`PartsError` (validated
  construction), `next_layer_id`, `add_layer_with_pixels`
  (`HistoryAction::ImportLayer`, one step), `LayerPixels::from_surface`,
  `LayerMask::reveal_tile`, `paper_backdrop`,
  `ComposeScratch::premultiplied`, `BlendMode::from_name`,
  `LayerError::TileOutOfRange`, `MAX_TILE_COORD` (7ef2b93).
- New crate `tela-io`: the `.mirae` v1 format (ZIP, JSON manifest, raw
  tiles), deterministic atomic save, hostile-input loading with explicit
  `Limits`, `IoError` and `Warning`, PNG export (8/16-bit, paper or
  transparent) and import (any colour type, 16 bits kept or rounded,
  centred); tests for round trips, corruption, properties, PNG fidelity
  (c9baa3a). v1 compatibility fixtures, never regenerated (17df807).
  `io_bench` (2965e01). `png_parity` against the reference's exports and
  the format specification `docs/formats/mirae-v1.md` (5645f7b).
- App: Open / Save / Save As / Import PNG / Export PNG in the document
  bar, Ctrl+N/O/S, Ctrl+Shift+S and Shift+Alt+S, native file dialogs
  (`rfd`), drag-and-drop of `.mirae` and `.png`, `Mirae — <name>*` title,
  Save / Don't Save / Cancel before anything replaces or closes a
  modified document, error notices; egui `prompt` module in `tela-ui`
  (6484d27).
- ADR 0019 (format) and 0020 (crate, dependencies, workflow); invariants
  L10, H12, IO1–IO10.

### Changed
- Window title and `inject_pen`'s default target are "Mirae".
- The composition's paper-backdrop decision is a shared function (output
  unchanged).

### Decided
- Deflate level 2 by default (measured). 16-bit PNG import keeps 16 bits
  or rounds to the nearest level (the reference truncates; D-4F-3).
  Newer format versions and unknown blend modes or depths are errors.

### Parity
- 4F gate recorded: PNG export with the paper equals the reference's own
  export pixel for pixel at both depths; opaque imports and translucent
  imports in 16-bit documents equal too; workflow compared in the running
  app. Remaining differences D-4F-1 … D-4F-11.

## Phase 4E — 16-bit storage

### Added
- `depth_probe` (16-bit test images in public formats: PSD in both layer
  layouts, PNG; reads the reference's exports against three models) and
  `docs/parity/4E-16bit.md` part 1: the reference observed before
  implementing; `psd` writer gains 16-bit samples (04ba25e).
- `tela_color::{StoragePixel, PixelDepth, widen8, narrow16, unorm16,
  with_pixel_type!}`, `PremulRgba::{from_rgba16, to_rgba16}`: exact
  8 → 16, nearest-level 16 → 8, 16-bit decode that keeps widened values
  bit-identical (57b66f0, 32967fa).
- Per-document depth: `Document::{with_depth, depth, convert_depth}`,
  `LayerPixels` (read-only, typed views), `LayerError::WrongDepth`,
  `HistoryAction::ConvertDepth`; tests for both depths, the 4D preview ==
  commit matrix and the 4C/4D parity data in 16-bit documents,
  `depth_properties.rs` (32967fa); ADR 0018.
- Document bar in the right panel: depth, New 8-bit, New 16-bit,
  To 8/16-bit; depth in the window title (41d5632).
- `tela_corpus::banding`, the `banding` example and test (371c9a2);
  `depth_bench` (1df2a26).

### Changed
- `StrokeBuffer::commit_tile` / `commit_into` are generic over
  `StoragePixel`; `Document::modify_tiles` is generic and refuses an edit
  for the other depth; `RasterLayer::pixels()` returns `&LayerPixels`.
- History keeps tile backups at their recorded depth and counts their
  real size (512 KiB per 16-bit tile).
- Composition decodes each layer tile in its own loop: 8-bit composition
  4–23 % faster, output unchanged.

### Decided
- D12: 8-bit stays the default for new documents (measured; ADR 0018).

### Parity
- 4E gate recorded: the reference has no 16-bit documents, no depth
  conversion and 8-bit exports; Tela adds 16-bit storage beyond it,
  keeps 8-bit content exact, and measures banding objectively (16-bit
  within 0.75 level of exact where 8-bit is off by 25–136 levels).

## Phase 4D — layer masks, clipping, alpha lock

### Added
- `clip_probe` (PSD test documents with clipping, masks, hidden layers;
  reads the reference's exports; `fit` compares clipping models) and the
  reference's measured results with and without its paper; `tela-corpus`
  gains a `psd` writer module (0fbc792).
- `docs/parity/4D-masks-clipping-alpha-lock.md`: composition measured and
  UI behaviour observed in the reference before implementing (8319a06).
- `PixelAlpha` (alpha lock as source-atop; the eraser unaffected) and
  alpha-channel strokes for masks (`paint_alpha_tile`,
  `commit_alpha_tile`) in `StrokeBuffer` (01c388e).
- Layer masks (`LayerMask`, `add_mask`, `remove_mask`, `set_mask_enabled`,
  `modify_mask`), clipping (`set_clipped`, `BlendMode::clip_atop`, group
  composition), alpha lock (`set_alpha_locked`), edit target
  (`LayerPart`, `active_part`, `set_active_part`), all recorded in history;
  `OverlayPaint::{Pixels, Mask}`; `clip_parity` (2 × 2 176 measured
  swatches) and the preview == commit matrix (0fd0ab2); ADR 0017.
- Property tests for masks, clipping and alpha lock (c898e0e).
- Panel controls: Clip, Lock alpha, Add/Delete mask, Pixels/Mask target,
  mask on/off; the app captures the stroke target at stroke start
  (27499af).
- `mask_clip_bench` (59eaa53).

### Changed
- `paint_over_tile` / `commit_tile` take a `PixelAlpha`;
  `Document::paint_target` takes a `LayerPart`; `Overlay::paint` is an
  `OverlayPaint`.
- Structural edits (move, remove, clip toggle and their undo) invalidate
  the clipped layers whose group changes.
- Only group bases decide whether a tile is composed over the paper.

### Parity
- 4D gate recorded: clipping composed in isolation with the blend result
  replacing the colour (measured; W3C atop and non-isolated rejected),
  masks and alpha lock as observed; every probed swatch within 2/255.

## Phase 4C — blend modes and compositor

### Added
- `blend_probe` (PSD test document from the public spec; reads the
  reference's PNG export) and the reference's measured results with and
  without its paper, `crates/document/tests/reference/` (04491cd).
- 16 blend modes besides Normal, W3C formulas on sRGB-encoded values;
  paper as blending backdrop for blend tiles; `blend_parity` (3 400
  measured combinations) and the blend-sheet golden (4573db1); ADR 0016.
- Blend-mode selector in the layer panel (b3f9a60).
- `blend_bench` (a4d5afa, 45b37c0) and `blend_fit` (e18bc71).

### Changed
- `BlendMode` moved to its own module and gained `ALL` and `name()`;
  `BlendMode::composite` dispatches once per mode (4573db1).

### Deferred
- Merge down (TECHNICAL_DEBT #21), behind the translucent-backdrop
  difference D-4C-1 (#20).

### Parity
- 4C gate recorded: compositing space and paper behaviour chosen by
  measurement; 17 modes within 1–2/255 of the reference over its paper.

## Phase 4B — undo/redo

### Added
- `docs/parity/4B-undo-redo.md`: 20 black-box observations of the
  reference's undo/redo, recorded before implementing (d0aabed).
- Linear undo/redo inside `Document`: `undo`, `redo`, `can_undo`,
  `can_redo`, `history_len`, `redo_len`, `next_undo`/`next_redo`
  (`HistoryAction`), `history_bytes`, `history_reserved_bytes`,
  `HistoryLimits` (200 steps / 1 GiB) (7d61c3e); ADR 0015.
- Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z and Undo/Redo buttons (9b7358e).
- `history_bench` (2f22cc7).

### Changed
- Every `Document` mutation records a step, except `set_active_layer` and
  `set_visible` (as in the reference); no-op calls (same name, same lock,
  clearing an empty layer) record nothing (7d61c3e).
- Consecutive opacity edits of one layer merge into one step (7d61c3e).
- Ctrl chords no longer fall through to single-letter shortcuts (9b7358e).

### Parity
- 4B gate recorded; the brief changed in two points to follow the
  reference (visibility not recorded; opacity merging by chain).

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
