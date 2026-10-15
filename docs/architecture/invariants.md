# Invariants

Properties the code must keep. Each one lists what protects it. Breaking
one is a design change: it needs an ADR, not just an updated test.

Test names are `crate::module::tests::name` for unit tests and
`file.rs::name` for integration tests under `crates/<crate>/tests/`.

## Determinism

| # | Invariant | Protected by |
|---|---|---|
| D1 | Same preset + same seed + same input samples ⇒ bit-identical dabs. | `brush::engine::tests::seed_controls_randomness`, `scatter_dynamics.rs::scattered_strokes_are_reproducible`, `golden.rs::rendering_is_deterministic` |
| D2 | Randomness is a pure function of `(seed, stream, index)`; evaluation order never matters. | `brush::rng::tests::streams_and_seeds_are_independent`, `properties.rs::stroke_rng_is_pure_and_bounded`, `brush::dynamics::tests::random_effectors_are_deterministic_and_per_property` |
| D3 | Each effector of each property has its own random stream (≤ 64 effectors per property). | `brush::preset::io::tests::effector_lists_are_capped`, `properties.rs::repaired_presets_round_trip_cleanly` |
| D4 | Parallel stamping is bit-identical to sequential stamping. | `brush::stroke::tests::parallel_stamping_is_bit_identical_to_sequential` |
| D5 | Golden sheets render identically run to run. | `golden.rs::rendering_is_deterministic`, `golden.rs::presets_match_their_golden_images` |

## Sampling and time independence

| # | Invariant | Protected by |
|---|---|---|
| S1 | Dab placement depends on the path, not on how many samples describe it (spacing carried across segments). | `dab_placement.rs::*` (2 vs 1000 samples), `brush::spacing::tests::spacing_is_uniform_regardless_of_point_density`, `properties.rs::spacing_is_independent_of_sample_density` |
| S2 | Velocity and filters behave the same at any device sample rate. | `brush::pipeline::velocity::tests::converges_to_constant_velocity_at_any_rate`, `stabilizer_comparison.rs::behaviour_is_independent_of_sample_rate` |
| S3 | Airbrush output does not depend on the host frame rate. | `airbrush.rs::same_result_at_30_and_144_fps`, `airbrush.rs::moving_strokes_are_frame_rate_independent`, `brush::airbrush::tests::resting_pen_deposits_identically_at_any_frame_rate` |
| S4 | Late or batched input delivery does not create airbrush beads; a resting pen keeps depositing. | `airbrush.rs::delayed_input_delivery_does_not_create_beads`, `airbrush.rs::rest_after_late_delivery_still_builds_up`, `airbrush.rs::fast_strokes_deposit_evenly_without_beads` |
| S5 | A stroke ends on the last real sample, whatever the stabilization. | `brush::engine::tests::stroke_ends_on_the_last_sample_even_with_heavy_stabilization`, `brush::stabilizer::tests::catch_up_ends_on_the_last_raw_sample` |

## Preview equals result

| # | Invariant | Protected by |
|---|---|---|
| P1 | The live preview (including the end-taper tail) equals the committed stroke. | `brush::engine::tests::end_taper_preview_matches_final_result`, `brush::stroke::tests::tail_is_rebuilt_from_committed_and_can_be_committed` |
| P2 | Preview composition and commit use the same math (`paint_over`, shared by `paint_over_tile` and `commit_tile`), for paint and erase. | `document::compose::tests::preview_matches_committed_result`, `document::compose::tests::eraser_preview_matches_committed_result`, golden sheets (rendered through `commit_tile`) |
| P3 | A stroke previews on its own layer (named by id), under that layer's opacity **and blend mode**, exactly as after commit; a hidden target shows no preview, as a commit would show nothing. | `document::compose::tests::preview_matches_committed_result`, `…::preview_targets_its_layer_by_id_not_by_position`, `…::preview_matches_commit_in_every_mode` |

## Document (layer stack)

Added in phase 4A (ADR 0014). The structural ones (L1–L7, L9) are also
checked after every step of `document::properties::layer_stack_matches_the_reference_model`,
which runs random operation sequences (live and stale ids) against a
plain reference model.

| # | Invariant | Protected by |
|---|---|---|
| L1 | Layer ids are unique within a document. | Privacy (no public `LayerId` constructor, ids allocated by `Document`); property test |
| L2 | Ids are never reused, not even after removal; allocation fails (`IdsExhausted`) instead of wrapping. | `document::document::tests::ids_are_never_reused`, `…::ids_run_out_instead_of_wrapping`, property test |
| L3 | A document always has at least one layer. | `…::removing_the_active_layer_activates_the_one_below_else_above`, property test |
| L4 | The active layer always exists; removing it activates the layer below, else the one above. | same, `…::removing_another_layer_keeps_the_selection`, property test |
| L5 | Layer opacity is finite and in `0..=1`. | `…::opacity_is_clamped_and_must_be_finite`, property test |
| L6 | An operation that returns an error leaves the document unchanged. | `…::unknown_ids_are_errors_and_change_nothing`, `…::lock_blocks_pixels_opacity_blend_and_removal_only`, property test (model unchanged on error) |
| L7 | Moving, removing or re-inserting layers never copies tile buffers. | `…::moving_keeps_identity_selection_and_tile_memory`, property test (buffer addresses) |
| L8 | Composition does not depend on id values, only on order and properties. | `document::compose::tests::composition_does_not_depend_on_id_values` |
| L9 | Pixels change only through `modify_tiles` (exactly the declared tiles; newly allocated tiles left blank are released) and `clear_layer`. | Privacy of `RasterLayer::pixels`; `…::modify_tiles_touches_only_declared_tiles_and_stays_sparse` |

## History (undo/redo)

Added in phase 4B (ADR 0015). Unit tests in
`document::document::history::tests`; the property tests
`document::history_properties::history_matches_a_stack_of_snapshots`
(random edits, undo and redo against a stack of full-content digests,
300 sequences) and `…::history_stays_within_small_limits`.

| # | Invariant | Protected by |
|---|---|---|
| H1 | Undo followed by redo restores the document bit-exactly (ids, order, properties, every tile). | `stroke_undo_redo_is_bit_exact_and_restores_absent_tiles`, `undoing_everything_and_redoing_it_walks_the_same_states`, property test |
| H2 | N undos restore exactly the state N steps ago (active layer per H11). | same, property test |
| H3 | Recording a step after an undo discards the redoable steps. | `new_edits_discard_redo`, property test |
| H4 | The history respects `HistoryLimits`: at most `max_steps` steps; reserved bytes ≤ `max_bytes` unless a single step is kept alone; held bytes ≤ reserved bytes. | `step_limit_drops_the_oldest_steps`, `byte_limit_keeps_an_oversized_step_alone`, `history_stays_within_small_limits` |
| H5 | Undo and redo copy no tile: buffers, surfaces and layers are swapped. | `undo_and_redo_swap_tiles_instead_of_copying_them`, `clearing_moves_the_surface_into_history`, `removed_layers_come_back_with_their_id_position_and_pixels` (buffer addresses) |
| H6 | Every public mutation is recorded, except the documented non-historical `set_active_layer` and `set_visible`; no-ops and refused calls record nothing. | Privacy (history is a private submodule of `document`); `every_recorded_mutation_is_one_step_and_the_others_none` |
| H7 | One `modify_tiles` call (one stroke) is one step. | `stroke_undo_redo_is_bit_exact_and_restores_absent_tiles`, property test |
| H8 | Consecutive opacity edits of one layer form one step until something else is recorded, undone/redone, or the active layer changes (reference behaviour). | `opacity_edits_merge_until_something_interrupts_them`, property test |
| H9 | A tile absent before a step is absent again after undoing it. | `stroke_undo_redo_is_bit_exact_and_restores_absent_tiles`, property test (digests list tiles) |
| H10 | A removed layer restored by undo has exactly its old `LayerId`, position, properties and pixels. | `removed_layers_come_back_with_their_id_position_and_pixels`, property test |
| H11 | Undo/redo of add/remove steps restores the active layer recorded with the step; other undo/redo keep the current selection. | `active_layer_after_undo_follows_the_reference`, property test |

## Storage and compositing

| # | Invariant | Protected by |
|---|---|---|
| T1 | Tiles never written are not allocated; absent tiles read as default. | `canvas::tile::tests::surface_is_sparse_and_reads_default`, `brush::engine::tests::stroke_lifecycle` |
| T2 | Tile addressing is exact for any coordinate, including negative. | `properties.rs::containing_tile_holds_the_pixel`, `properties.rs::rect_tiles_cover_exactly` (canvas) |
| T3 | Compositing uses premultiplied alpha; `over` is associative; RGBA8 round trips exactly. | `color::tests::over_is_associative`, `color::tests::rgba8_round_trip_is_exact` |
| T4 | A stroke's alpha never exceeds the largest opacity that touched a pixel. | `brush::stroke::tests::opacity_caps_overlapping_dabs`, `…::lighter_dab_never_lightens_existing_paint`, `…::flow_builds_up_gradually` |
| T5 | The view transform round-trips and zoom/rotate keep the anchor fixed. | `canvas::view::tests::*`, `properties.rs::view_round_trips_and_pins_the_anchor` |
| T6 | The display is seam-free when minified and rotated; magnified texels are flat squares. | `display_quality.rs::no_seams_when_minified_and_rotated`, `display_quality.rs::magnified_texels_are_flat_squares` |

## Blend modes (phase 4C, ADR 0016)

| # | Invariant | Protected by |
|---|---|---|
| C1 | `Normal` composition is bit-identical to the pre-4C compositor (same code path); tiles where only Normal layers draw keep a transparent backdrop. | `document::compose::tests::normal_only_tiles_keep_a_transparent_backdrop`, existing compose tests, brush goldens |
| C2 | A fully transparent source pixel leaves the destination unchanged in every mode. | `document::blend::tests::transparent_source_is_the_identity_in_every_mode`, property test `transparent_source_never_changes_the_destination` |
| C3 | Every mode outputs finite, valid premultiplied pixels (components in [0,1], colour ≤ alpha) with the W3C alpha `αs + αb(1 − αs)`. | property test `every_mode_yields_valid_premultiplied_pixels` |
| C4 | Blending uses the published W3C formula and blend functions on sRGB-encoded values; a tile drawing a non-Normal layer has the paper as opaque bottom backdrop. | `opaque_results_follow_the_published_blend_functions`, `non_separable_modes_keep_the_published_components`, `blend_layers_see_the_paper` |
| C5 | Over its paper, Tela matches the reference's measured blend results within 1/255 (2/255 Color, Luminosity) for every probed combination. | `crates/document/tests/blend_parity.rs` (3 400 combinations) |
| C6 | The blend sheet renders deterministically and equal to its reviewed golden (± 1/255). | `crates/document/tests/blend_sheet.rs` |
| C7 | Composition stays allocation-free in every mode. | `blend_bench` (allocations column), `stamp_bench` |

T3 (`over` is associative) holds for Normal only; the other modes are not
associative, so the stack order matters.

## Robustness

| # | Invariant | Protected by |
|---|---|---|
| R1 | Invalid presets never panic: malformed JSON is an error; anything that parses loads (repaired, with warnings). | `brush::preset::io::tests::broken_input_is_an_error_not_a_panic`, `…::out_of_range_values_are_repaired_with_warnings`, `properties.rs::repaired_presets_round_trip_cleanly` |
| R2 | Unknown fields are ignored; unknown enum variants load as neutral `Unknown`; newer versions load what they can. | `brush::preset::io::tests::newer_versions_load_what_they_can`, `brush::dynamics::tests::unknown_sources_and_modes_deserialize`, `brush::tip::tests::unknown_shape_deserializes_as_round` |
| R3 | Older formats migrate step by step; huge versions never wrap to 0. | `brush::preset::io::tests::version_zero_is_migrated`, `…::huge_versions_are_newer_not_version_zero` |
| R4 | Validation is idempotent; saved presets reload without warnings and unchanged. | `brush::preset::io::tests::round_trip_is_exact`, `properties.rs::repaired_presets_round_trip_cleanly` |
| R5 | Curves never produce NaN or values outside `[0, 1]`; pressure curves are monotone; edits keep curves valid. | `properties.rs::curves_stay_in_the_unit_range`, `…::monotone_curves_never_decrease`, `…::curve_edits_preserve_validity` |
| R6 | Inputs a device lacks (pressure on a mouse, tilt on a basic pen, direction at rest) are neutral. | `brush::dynamics::tests::missing_input_is_neutral`, `…::direction_is_undefined_at_rest` |

## Boundaries

| # | Invariant | Protected by |
|---|---|---|
| B1 | The brush engine does not depend on input backends, windowing, UI, GPU or the document. | `app/tests/crate_boundaries.rs` (compile-time graph + test) |
| B2 | The renderer cannot read or modify the document. | same (renderer has no `tela-document` dependency) |
| B3 | Only `tela-ui` knows egui; only `tela-app`/`tela-ui` know winit; only `tela-tablet` has FFI. | same |
| B4 | The UI holds no authoritative document or preset state. | Code review: `PanelModel` and `LayerRow` lend data per frame (docs/architecture/ui.md) |
| B5 | Golden images change only on explicit request (`TELA_UPDATE_GOLDEN=1` exactly); no orphan references. | `corpus::update_golden_requested`, `golden.rs::golden_directory_matches_the_builtin_presets` |

## Performance

| # | Invariant | Protected by |
|---|---|---|
| F1 | No allocation per dab on the sequential stamping path (after tiles exist); the parallel path allocates only rayon's amortized injector blocks (≤ 1 per 100 samples). | `allocation.rs::processing_and_stamping_do_not_allocate_per_sample`, `stamp_bench` (allocs/dab = 0.000) |
| F2 | No allocation per sample in the stroke processor (owned, reused buffers). | `allocation.rs::processing_and_stamping_do_not_allocate_per_sample` |
| F3 | Composition of a dirty tile does not allocate. | `stamp_bench` composition line (0 allocations) |

F1 and F2 are tested by `cargo test` (thread-local counting allocator);
F3 and throughput are measured by the benchmark; run it after touching hot
paths (docs/performance.md). B4 is the only invariant protected by review
alone.

## Hardware

No physical tablet has validated any pen invariant. Pen behaviour is tested
with synthetic Windows Ink injection (integer pixel positions only); see
docs/STATUS.md.
