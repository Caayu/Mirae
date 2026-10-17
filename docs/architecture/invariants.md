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
| P2 | Preview composition and commit use the same math for paint, erase and alpha-preserving paint (`paint_over`, shared by `paint_over_tile` and `commit_tile`, selected by `PixelAlpha`) and for mask strokes (`alpha_over`, shared by `paint_alpha_tile` and `commit_alpha_tile`). At both storage depths the commit is the preview quantized once to the storage (phase 4E). | `brush::stroke::tests::commit_is_the_preview_quantized_once_at_both_depths`, `document::compose::tests::preview_matches_committed_result`, `…::eraser_preview_matches_committed_result`, `…::preview_matches_commit_across_masks_clipping_lock_and_modes`, `brush::stroke::tests::alpha_channel_preview_matches_commit`, golden sheets (rendered through `commit_tile`) |
| P3 | A stroke previews on its own target (layer by id, its pixels or its mask, fixed when the stroke begins), under that layer's mask, opacity, clipping group **and blend mode**, exactly as after commit (within 1/255); a hidden target shows no preview, as a commit would show nothing. | `…::preview_matches_committed_result`, `…::preview_targets_its_layer_by_id_not_by_position`, `…::preview_matches_commit_in_every_mode`, `…::preview_matches_commit_across_masks_clipping_lock_and_modes` (126 cases), `…::preview_matches_commit_in_16_bit_documents` (the same 126 cases at 16 bits) |

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
| L9 | Pixels change only through `modify_tiles` (exactly the declared tiles; newly allocated tiles left blank are released), `clear_layer`, `convert_depth` and `add_layer_with_pixels` (a new layer). | Privacy of `RasterLayer::pixels`; `…::modify_tiles_touches_only_declared_tiles_and_stays_sparse` |
| L10 | `Document::from_parts` refuses parts that would break L1–L5, DP1, M3 or the edit-target rule (zero size, non-opaque paper, stale next id, out-of-range tiles), and keeps accepted parts exactly (nothing repaired or clamped); the result has an empty history. | `document::document::parts::tests::*` (every refusal, round trip at both depths) |

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
| H6 | Every public mutation is recorded, except the documented non-historical `set_active_layer`, `set_active_part` and `set_visible`; no-ops and refused calls record nothing. | Privacy (history is a private submodule of `document`); `every_recorded_mutation_is_one_step_and_the_others_none` |
| H7 | One `modify_tiles` call (one stroke) is one step. | `stroke_undo_redo_is_bit_exact_and_restores_absent_tiles`, property test |
| H8 | Consecutive opacity edits of one layer form one step until something else is recorded, undone/redone, or the active layer changes (reference behaviour); since 5B consecutive renames of one layer do too (reference RN4, ADR 0023). | `opacity_edits_merge_until_something_interrupts_them`, `consecutive_renames_of_one_layer_are_one_step`, property test |
| H9 | A tile absent before a step is absent again after undoing it. | `stroke_undo_redo_is_bit_exact_and_restores_absent_tiles`, property test (digests list tiles) |
| H10 | A removed layer restored by undo has exactly its old `LayerId`, position, properties and pixels. | `removed_layers_come_back_with_their_id_position_and_pixels`, property test |
| H11 | Undo/redo of add/remove (and, since 5B, merge-down) steps restores the active layer recorded with the step; other undo/redo keep the current selection. | `active_layer_after_undo_follows_the_reference`, property test |
| H12 | `Document::revision` identifies the persistent content: equal revisions mean equal content; undo or redo back to a state returns its revision (serials are never reused; dropped steps move the base); merged opacity edits and non-recorded persistent changes (visibility) give new revisions; selecting a layer or the edit target does not (reference F2, F3). | `…::history::tests::undo_and_redo_return_to_the_saved_revision`, `…::merged_opacity_edits_are_new_revisions`, `…::visibility_is_a_modification_and_selection_is_not`, `…::dropped_steps_never_make_an_older_revision_reappear`, `history_matches_a_stack_of_snapshots` (equal revisions ⇒ equal content) |

## Storage and compositing

| # | Invariant | Protected by |
|---|---|---|
| T1 | Tiles never written are not allocated; absent tiles read as default. | `canvas::tile::tests::surface_is_sparse_and_reads_default`, `brush::engine::tests::stroke_lifecycle` |
| T2 | Tile addressing is exact for any coordinate, including negative. | `properties.rs::containing_tile_holds_the_pixel`, `properties.rs::rect_tiles_cover_exactly` (canvas) |
| T3 | Compositing uses premultiplied alpha; `over` is associative; RGBA8 and RGBA16 round trip exactly through f32 (phase 4E). | `color::tests::over_is_associative`, `color::tests::rgba8_round_trip_is_exact`, `color::storage::tests::every_16_bit_value_round_trips_through_f32` |
| T4 | A stroke's alpha never exceeds the largest opacity that touched a pixel. | `brush::stroke::tests::opacity_caps_overlapping_dabs`, `…::lighter_dab_never_lightens_existing_paint`, `…::flow_builds_up_gradually` |
| T5 | The view transform round-trips and zoom/rotate keep the anchor fixed. | `canvas::view::tests::*`, `properties.rs::view_round_trips_and_pins_the_anchor` |
| T6 | The display is seam-free when minified and rotated; magnified texels are flat squares. | `display_quality.rs::no_seams_when_minified_and_rotated`, `display_quality.rs::magnified_texels_are_flat_squares` |

## Blend modes (phase 4C, ADR 0016; translucent backdrops 5B, ADR 0022)

| # | Invariant | Protected by |
|---|---|---|
| C1 | `Normal` composition is bit-identical to the pre-4C compositor (same code path); tiles where only Normal layers draw keep a transparent backdrop. | `document::compose::tests::normal_only_tiles_keep_a_transparent_backdrop`, existing compose tests, brush goldens |
| C2 | A fully transparent source pixel leaves the destination unchanged in every mode. | `document::blend::tests::transparent_source_is_the_identity_in_every_mode`, property test `transparent_source_never_changes_the_destination` |
| C3 | Every mode outputs finite, valid premultiplied pixels (components in [0,1], colour ≤ alpha) with the W3C alpha `αs + αb(1 − αs)`, except Screen, Color Dodge, Hard Light and Soft Light over a translucent backdrop, whose alpha is `min(1, αs + αb)` (ADR 0022, 5B; was the W3C alpha for every mode until 5B). | property test `every_mode_yields_valid_premultiplied_pixels` |
| C4 | Blending uses the published W3C formula and blend functions on sRGB-encoded values; a tile in which a non-Normal layer blends with the stack (a group base or a base-less layer; clipped layers blend inside their group) has the paper as opaque bottom backdrop. | `opaque_results_follow_the_published_blend_functions`, `non_separable_modes_keep_the_published_components`, `blend_layers_see_the_paper` |
| C5 | Over its paper, Tela matches the reference's measured blend results within 1/255 (2/255 Color, Luminosity) for every probed combination. | `crates/document/tests/blend_parity.rs` (3 400 combinations) |
| C6 | The blend sheet renders deterministically and equal to its reviewed golden (± 1/255). | `crates/document/tests/blend_sheet.rs` |
| C7 | Composition stays allocation-free in every mode. | `blend_bench` (allocations column), `stamp_bench` |
| C8 | Over a translucent backdrop (no paper: transparent export, merge down) every mode follows the rule measured in the reference (ADR 0022): Multiply the W3C formula, the others the blended colour composed over the backdrop first; over an opaque backdrop the code takes the pre-5B W3C path, so composites over the paper are unchanged bit for bit. Tela matches the reference within 2/255 premultiplied on 45 900 combinations (Color Dodge/Burn 5/255 in 8-bit documents, a storage-precision effect). | `crates/document/tests/backdrop_parity.rs` (both depths), `backdrop_parity.rs::the_w3c_formula_does_not_explain_the_data` (planted alternative), `blend_parity.rs::every_mode_matches_the_reference_without_paper` |

T3 (`over` is associative) holds for Normal only; the other modes are not
associative, so the stack order matters.

## Masks, clipping, alpha lock (phase 4D, ADR 0017)

Also checked by the extended model test `document::properties::layer_stack_matches_the_reference_model`
and history test `document::history_properties::history_matches_a_stack_of_snapshots`.

| # | Invariant | Protected by |
|---|---|---|
| M1 | A layer without a mask, or with a new one, composes exactly as before masks existed. | `document::compose::tests::a_new_mask_changes_nothing_and_allocates_nothing`, existing compose tests and goldens |
| M2 | A disabled mask is exactly equivalent to no mask. | `…::a_disabled_mask_is_the_same_as_no_mask`, property test `mask_clip_properties.rs::disabled_masks_equal_absent_masks` |
| M3 | An absent mask tile is exactly the default state (reveals everything): a new mask stores no tile, and no stored tile reveals everything. | `document::document::tests::masks_start_revealing_everything_and_become_the_target`, `…::modify_mask_shows_reveal_values_and_keeps_the_mask_sparse`, `check_invariants` after every model step |
| M4 | A mask stroke previews exactly as it commits (within 1/255). | `brush::stroke::tests::alpha_channel_preview_matches_commit`, `document::compose::tests::preview_matches_commit_across_masks_clipping_lock_and_modes` |
| M5 | Undo/redo of mask edits (add, remove, enable, strokes) is bit-exact and swaps tiles (no copies). | `document::document::history::tests::mask_edits_undo_and_redo_bit_exactly`, `…::mask_undo_swaps_tiles_and_restores_absent_ones`, history property test |
| A1 | With alpha lock, a paint stroke never changes any pixel's alpha. | `brush::stroke::tests::preserved_alpha_paints_atop_and_never_changes_alpha`, property test `brush/tests/properties.rs::preserved_alpha_never_changes_alpha` |
| A2 | With alpha lock, no pixel appears where alpha was 0. | same |
| A3 | Preview with alpha lock equals the commit; the eraser erases with or without the lock (reference A3). | `…::preview_matches_commit_across_masks_clipping_lock_and_modes`, `brush::stroke::tests::preserved_alpha_does_not_stop_the_eraser` |
| CL1 | Clipped content shows only inside its base's (masked) alpha: hiding clipped layers never changes the composite's coverage; a hidden base hides its group. | `document::compose::tests::clipped_layers_show_only_inside_the_base_alpha`, property test `clipped_layers_never_change_coverage`, `blend::tests::clip_atop_keeps_the_group_alpha_in_every_mode` |
| CL2 | Clipped layers apply in stack order, deterministically; a clipped layer with no base is ordinary. | `clip_parity.rs` (`two_clips`, `three_clips`, `clips_swapped`, `orphan_clip`), `…::a_clipped_layer_without_a_base_is_ordinary` |
| CL3 | Turning clipping off restores exactly the ordinary composition. | `…::unclipping_restores_the_ordinary_composition` |
| CL4 | Undo/redo restore clip flags and therefore groups; structural edits invalidate every layer whose group changes. | `…::mask_edits_undo_and_redo_bit_exactly`, history property test, `document::document::tests::group_changes_invalidate_the_clipped_layers_too` |
| CL5 | Over and without its paper, Tela matches the reference's measured mask and clipping results within 2/255 for every probed swatch. | `crates/document/tests/clip_parity.rs` (2 × 2 176 swatches) |
| CL6 | Composites with masks and clipping stay valid premultiplied pixels and allocation-free. | property test `composites_stay_valid_premultiplied`, `mask_clip_bench` (allocations column) |

## Storage depth (phase 4E, ADR 0018)

Also checked by the layer-stack model test (depth, wrong-depth edits,
conversions) and the history property test (conversions among random
edits, undo and redo).

| # | Invariant | Protected by |
|---|---|---|
| DP1 | A document has one depth; every raster layer stores its pixels at it (new layers included). | `check_invariants` after every model step, `document::document::tests::documents_store_one_depth_and_refuse_edits_for_the_other` |
| DP2 | 8 → 16 is exact (`v · 257`) and widened content composes **bit-identically** (every mode, masks, clipping, opacity, paper). | `color::storage::tests::widening_every_8_bit_value_is_exact`, `…::widened_values_decode_to_the_same_f32_as_8_bit`, `document::compose::tests::widened_8_bit_content_composes_identically_in_every_mode`, `depth_properties.rs::widened_8_bit_stacks_compose_identically` |
| DP3 | 16 → 8 moves each channel to the nearest level (≤ ½ level); 8 → 16 → 8 is the identity; premultiplied order (`rgb ≤ a`) survives both. | `color::storage::tests::narrowing_rounds_to_the_nearest_level_like_unorm8`, `…::premultiplied_order_survives_both_conversions`, `depth_properties.rs::narrowing_is_the_nearest_level_and_undoable` |
| DP4 | `convert_depth` is one history step; undo and redo restore each depth bit-exactly (moving surfaces, no copy); ids, order, properties, masks and the sparse layout do not change; converting to the current depth records nothing. | `document::document::tests::widening_keeps_structure_composite_and_masks_and_is_one_step`, `…::history::tests::depth_conversion_undoes_and_redoes_through_later_steps`, `depth_properties.rs::conversion_keeps_structure_and_undoes_exactly` |
| DP5 | A pixel edit is written for one depth; an edit for the other depth is refused (`WrongDepth`) and changes nothing, history included. | `…::documents_store_one_depth_and_refuse_edits_for_the_other`, `depth_properties.rs::edits_for_the_other_depth_change_nothing`, model test |
| DP6 | Masks are 8-bit at every depth; conversion never touches them. | Type (`TiledSurface<u8>`), `…::widening_keeps_structure_composite_and_masks_and_is_one_step` |
| DP7 | Composition outputs RGBA8 for display at every depth and stays allocation-free; history counts tiles at their real size (512 KiB at 16 bits). | `depth_bench` (allocations column), `…::history::tests::sixteen_bit_tiles_cost_twice_as_much_history`, `…::the_byte_budget_counts_16_bit_tiles_at_their_size` |

H1–H11, M1–M5, A1–A3 and CL1–CL6 hold at both depths: the history unit
and property tests, `brush/tests/properties.rs::preserved_alpha_never_changes_alpha_at_16_bits`,
`clip_parity.rs::masks_and_clipping_match_the_reference_in_16_bit_documents`
and `depth_properties.rs::sixteen_bit_composites_stay_valid_premultiplied`
run them in 16-bit documents.

## Document files (phase 4F, ADR 0019, 0020)

Tests in `crates/io` (unit tests and `tests/*.rs`); the format is
specified in [../formats/mirae-v1.md](../formats/mirae-v1.md).

| # | Invariant | Protected by |
|---|---|---|
| IO1 | Save → load is the identity on everything the format stores: depth, size, paper bits, ids and order, next id, active layer and part, every property (opacity bits), every stored tile bit for bit at both depths, masks, and the sparse layout (absent tiles absent, present blank tiles present, negative and off-canvas tiles kept). History is not stored. | `round_trip.rs::save_then_load_is_bit_exact_at_both_depths`, `…::the_composite_is_unchanged_by_a_round_trip`, `properties.rs::random_documents_round_trip_exactly` |
| IO2 | Saving is deterministic: the same document gives the same bytes; save ∘ load ∘ save = save. | `round_trip.rs::saving_is_deterministic_and_idempotent`, property test |
| IO3 | Any input gives a valid document or an error, never a panic; documents are built only through `Document::from_parts` (L10). | `corrupt.rs::*`, `properties.rs::random_bytes_are_errors`, `…::damaged_documents_never_load_as_something_else` (the panic guard's `Internal` error fails these tests) |
| IO4 | A damaged file never loads as a different document: every entry's checksum is verified, every listed tile must exist with its exact size, every entry under `layers/` must be listed, duplicate and unsafe names are refused. | `corrupt.rs::missing_extra_and_damaged_tiles_are_errors`, `…::checksums_sizes_and_methods_are_enforced`, `…::duplicate_and_hostile_entry_names_are_refused`, `properties.rs::damaged_documents_never_load_as_something_else` |
| IO5 | `Limits` are checked before the data they govern is read (ZIP end records before opening, declared sizes before decompressing, no entry read past its expected size); saving refuses what loading would refuse. | `corrupt.rs::limits_are_checked_before_the_data_is_read`, `…::a_million_layers_or_absurd_sizes_are_refused_cheaply`, `zipcheck::tests::*`, `round_trip.rs::files_on_disk_round_trip_and_failed_saves_keep_the_old_file` |
| IO6 | A newer format version is refused as a whole, checked before any other field; unknown blend modes and depths are errors, never a fallback; unknown fields and entries load with a warning each. | `corrupt.rs::versions_newer_than_this_build_are_refused_whole`, `…::values_that_would_change_the_picture_are_errors_not_fallbacks`, `…::unknown_fields_and_entries_load_with_warnings` |
| IO7 | A failed save leaves the previous file byte-identical and no temporary file behind. | `atomic::tests::*`, `round_trip.rs::files_on_disk_round_trip_and_failed_saves_keep_the_old_file` |
| IO8 | The committed v1 fixtures load, forever, exactly as pinned (they are never regenerated). | `fixture.rs::*` |
| IO9 | PNG export is the document's composite quantized once: over the paper (opaque) or un-premultiplied (transparent), within 1/255 of the screen; the transparent export composes without the paper, blend layers included (ADR 0022; refused before 5B, D-4F-5). | `png.rs::export_with_paper_is_the_composite_over_the_paper`, `…::transparent_export_is_the_unpremultiplied_composite`, `…::sixteen_bit_export_keeps_a_16_bit_composite`, `…::transparent_export_of_blend_layers_is_the_composite_without_paper` |
| IO10 | PNG import premultiplies once, at the document's depth: 16 bits kept in 16-bit documents, nearest level in 8-bit ones; an 8-bit image survives import and transparent export exactly in a 16-bit document; fully transparent tiles are not stored. | `png.rs::import_premultiplies_once_at_the_document_depth`, `…::sixteen_bit_images_keep_16_bits_or_round_to_the_nearest_level`, `…::eight_bit_images_survive_import_and_export_exactly_in_16_bit_documents` |

## Colour (phase 5A, ADR 0021)

| # | Invariant | Protected by |
|---|---|---|
| COL1 | Every `Color` (main, sub, recent, swatches, picks) is valid: finite channels in `0..=1`; every `Hsv` has a hue in `0..360`. Constructors clamp and map NaN to 0. | Private fields; `color::paint::tests::colors_are_always_valid`, `color/tests/properties.rs::colors_are_always_valid` |
| COL2 | A stroke paints with the colour captured when it began; changing the colour during a stroke changes the next stroke only (preview and commit alike). | `brush::engine::tests::a_stroke_keeps_the_colour_it_began_with`; the app copies `ColorState::active` into `BrushEngine::color` only at pen-down |
| COL3 | Erasing does not depend on the colour (free or preserved alpha, both depths). | `brush::stroke::tests::colour_never_changes_what_the_eraser_does`, `brush/tests/color_properties.rs::erasing_ignores_the_colour` |
| COL4 | A stroke on a mask changes it by its alpha only: any hue with the same coverage gives the same mask (ADR 0017). | `brush::stroke::tests::alpha_channel_ignores_the_paint_colour`, `color_properties.rs::mask_strokes_ignore_the_colour` |
| COL5 | Alpha lock with coloured paint never changes alpha and recolours source-atop (the reference's measured pixels within 1/255). | `properties.rs::preserved_alpha_never_changes_alpha` (8 and 16 bits, random colours), `brush::stroke::tests::preserved_alpha_recolours_like_the_reference` |
| COL6 | The eyedropper never yields NaN, infinity or out-of-range channels; alpha 0 (or a non-number) has no colour and leaves the active colour unchanged. | `color::paint::tests::picking_unpremultiplies_safely`, `…::picking_garbage_never_gives_nan`, `color/tests/properties.rs::picking_never_yields_nan` |
| COL7 | Stored pixels stay valid premultiplied (colour ≤ alpha) after coloured paint, at both depths. | `color_properties.rs::colored_paint_stays_valid_premultiplied`, `document::compose::tests::colored_preview_matches_commit_at_both_depths` |
| COL8 | A colour representable at 8 bits paints the same in 8- and 16-bit documents within one 8-bit level (exactly at full coverage). | `document::compose::tests::colored_strokes_store_their_colour_at_both_depths` |

P1–P3 also hold for coloured paint: `document::compose::tests::colored_preview_matches_commit_at_both_depths`
(seven colours with independent channels × hard and soft/low-flow
strokes × free and preserved alpha × Normal/Multiply × unclipped,
clipped, on the base × 8 and 16 bits: 336 cases within 1/255). Colour
state is not document state: nothing in COL1–COL8 touches the history
(H6) or the file format (IO1).

## Merge down and layer workflow (phase 5B, ADR 0023)

Also checked by the layer-stack model test and the history property
test, which include random merges (succeeding and refused).

| # | Invariant | Protected by |
|---|---|---|
| MD1 | Merging composes the pair alone over transparency, as the reference does: the merged layer shows exactly what the two layers showed **alone** (within 1/255 after the single quantization); with the merged layer in the stack the picture is therefore unchanged where nothing visible lies under the pair's translucent pixels or both layers blend Normal, and changes exactly as in the reference where a non-Normal pair met layers below or the paper, a lower mask is kept, or a clipping base is merged down. | `merge_parity.rs` (22 measured scenarios, both depths), `…::merging_keeps_the_picture_where_the_reference_does` |
| MD2 | A successful merge removes exactly one layer (the upper one, whose id is never reused) and leaves the lower one active. | `merge::tests::the_lower_layer_keeps_its_identity_and_the_upper_one_goes`, both property models (`Merge` ops) |
| MD3 | A merge is one history step; undo restores both layers (ids, order, pixels, masks, properties) and the active layer bit for bit, redo the merged state. | `merge::tests::merging_is_one_step_and_undo_restores_both_layers_exactly`, `history_matches_a_stack_of_snapshots` |
| MD4 | A refused merge (no layer below, a locked or hidden layer) changes nothing, history included. | `merge::tests::refusals_change_nothing`, both property models |
| MD5 | Merging keeps the document's depth and quantizes once at it: 16-bit documents never pass through 8 bits. | `merge::tests::sixteen_bit_precision_survives_the_merge`, `merge_parity.rs` at 16 bits |
| MD6 | The paper never enters a merged layer: pixels transparent in both layers stay transparent, and blend layers merge over transparency. | `merge::tests::the_paper_is_never_merged_in`, `merge_parity.rs` (`md_top_screen_op60_paper`) |
| MD7 | A merge's history step holds the upper layer and only the lower tiles that changed. | `merge::tests::history_holds_only_the_changed_tiles_and_the_upper_layer` |
| LR1 | Reordering (drag and drop, Up/Down) keeps every layer's `LayerId`; the panel sends keys and relative placements, never indices. | `reorder.rs::reordering_keeps_ids_and_content_and_regroups_clipping_by_position`, L7 (`moving_keeps_identity_selection_and_tile_memory`), property model |
| LR2 | Reordering keeps each layer's pixels, mask and properties (clip flag included); dropping a layer where it already is records nothing; locked layers move. | same, `…::dropping_a_layer_where_it_already_is_records_nothing`, `…::moving_a_locked_layer_is_allowed` |
| LR3 | After a reorder, clipping groups are what the new positions make them (the document's rule, 4D C2; no rule in the UI): the result composes exactly like the same stack built in that order, and one undo restores the old order and picture. | `reorder.rs` (5 cases × 2 depths: a new base inside a group, a clipped layer moved to the bottom or onto another base, a base moved above its clipped layer) |
| RN1 | Renaming changes only the name, is allowed on locked layers, and is recorded; refusing nothing else. | `document::tests::*`, `consecutive_renames_of_one_layer_are_one_step`, property models |
| RN2 | Consecutive renames of one layer are one step (H8). | `consecutive_renames_of_one_layer_are_one_step`, `history_matches_a_stack_of_snapshots` |

## Integration (phase 4G)

No new invariant: these rows record where the invariants above are
checked **together**, with real brush strokes committed as the app commits
them, across layers, modes, masks, clipping, alpha lock, both depths,
history and files (`crates/corpus/tests/`, helpers in `support/mod.rs`).

| # | Invariant | Protected by |
|---|---|---|
| I1 | After every step of a combined session the document's public invariants hold (L1–L9, premultiplied pixels, a valid active layer and edit target). | `scenarios.rs::scenario_a…` – `scenario_g…`, `random_sessions.rs::seeded_sessions_keep_every_invariant` (16 seeds; rerun one with `MIRAE_SEED=`), `stress.rs::*` |
| I2 | P1–P3 hold in combination: every stroke's preview composite equals its committed composite. | `support::stroke` (checked on every stroke of every end-to-end test) |
| I3 | Undoing a combined session to its start and redoing it to its end restores every intermediate state exactly (H1–H4 together). | `scenarios.rs` (walk back and forth), `random_sessions.rs`, `stress.rs::long_histories_respect_the_step_limit_and_stay_exact` |
| I4 | Saving and loading any reachable document restores its content exactly, and saving is byte-deterministic (IO1–IO3 together). | `support::round_trip` in every scenario, `stress.rs::repeated_save_and_load_cycles_are_stable`, `pipeline.rs::a_whole_drawing_session_survives_history_conversion_and_files` |
| I5 | The appearance of a combined session does not change silently. | `pipeline.rs::the_session_document_matches_its_golden_image` |

Note on I2: a stroke on an **empty**, alpha-locked layer in a non-Normal
mode previews the tile composed over the paper while the commit stores
nothing; both show the same pixels on screen (the backdrop), so the
helper compares what is shown over the paper when one side is empty. Not
a defect; recorded so the comparison rule is not mistaken for one.

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
| F3 | Composition of a dirty tile does not allocate. | `stamp_bench` composition line (0 allocations), `doc_bench` (every mode, mask, clipping, 1–100 layers, both depths; exits non-zero on any allocation and runs in `check.ps1 -Bench`) |

F1 and F2 are tested by `cargo test` (thread-local counting allocator);
F3 and throughput are measured by the benchmark; run it after touching hot
paths (docs/performance.md). B4 is the only invariant protected by review
alone.

## Hardware

No physical tablet has validated any pen invariant. Pen behaviour is tested
with synthetic Windows Ink injection (integer pixel positions only); see
docs/STATUS.md.
