use tela_canvas::{DirtyTiles, TileCoord};

use super::*;
use crate::{LayerError, LayerPart, Placement};

const A: TileCoord = TileCoord { x: 0, y: 0 };
const B: TileCoord = TileCoord { x: 1, y: 0 };

fn tiles(coords: &[TileCoord]) -> DirtyTiles {
    let mut t = DirtyTiles::new();
    for &c in coords {
        t.mark(c);
    }
    t
}

/// One "stroke": sets pixel (k, k) of every declared tile to `v`.
fn stroke(d: &mut Document, id: LayerId, coords: &[TileCoord], k: u32, v: u8) {
    d.modify_tiles(id, &tiles(coords), |_, t| *t.get_mut(k, k) = [v; 4])
        .unwrap();
}

fn tile_ptr(d: &Document, id: LayerId, c: TileCoord) -> Option<*const [u8; 4]> {
    d.layer(id)?.pixels().tile(c).map(|t| t.pixels().as_ptr())
}

#[test]
fn stroke_undo_redo_is_bit_exact_and_restores_absent_tiles() {
    let mut d = Document::new(1024, 1024);
    let l = d.active_layer();
    stroke(&mut d, l, &[A], 3, 10);
    let before = d.digest();
    stroke(&mut d, l, &[A, B], 5, 200); // A existed, B did not
    let after = d.digest();
    assert_eq!(d.history_len(), 2, "H7: one call, one step");

    let dirty = d.undo().unwrap();
    assert_eq!(d.digest(), before, "H1/H2");
    assert!(
        d.layer(l).unwrap().pixels().tile(B).is_none(),
        "H9: B absent again"
    );
    assert!(dirty.contains(A) && dirty.contains(B));

    d.redo().unwrap();
    assert_eq!(d.digest(), after, "H1");
    d.undo().unwrap();
    d.undo().unwrap();
    assert_eq!(d.layer(l).unwrap().pixels().tile_count(), 0, "H9");
    assert!(!d.can_undo());
    assert!(d.undo().is_none());
}

#[test]
fn undo_and_redo_swap_tiles_instead_of_copying_them() {
    let mut d = Document::new(1024, 1024);
    let l = d.active_layer();
    stroke(&mut d, l, &[A], 1, 50);
    stroke(&mut d, l, &[A], 2, 90);
    let live = tile_ptr(&d, l, A).unwrap();
    d.undo().unwrap();
    let restored = tile_ptr(&d, l, A).unwrap();
    assert_ne!(live, restored);
    d.redo().unwrap();
    assert_eq!(
        tile_ptr(&d, l, A),
        Some(live),
        "H5: the same buffer came back"
    );
    d.undo().unwrap();
    assert_eq!(tile_ptr(&d, l, A), Some(restored), "H5: and again");
}

#[test]
fn new_edits_discard_redo() {
    let mut d = Document::new(512, 512);
    let l = d.active_layer();
    stroke(&mut d, l, &[A], 1, 1);
    stroke(&mut d, l, &[A], 2, 2);
    d.undo().unwrap();
    assert!(d.can_redo());
    stroke(&mut d, l, &[A], 3, 3);
    assert!(!d.can_redo(), "H3");
    assert!(d.redo().is_none());
    assert_eq!(d.history_len(), 2);
}

#[test]
fn undoing_everything_and_redoing_it_walks_the_same_states() {
    let mut d = Document::new(1024, 1024);
    let l1 = d.active_layer();
    let mut states = vec![d.digest()];
    stroke(&mut d, l1, &[A], 1, 1);
    states.push(d.digest());
    let l2 = d.add_layer(Placement::Top).unwrap();
    states.push(d.digest());
    stroke(&mut d, l2, &[B], 2, 2);
    states.push(d.digest());
    d.set_opacity(l2, 0.25).unwrap();
    states.push(d.digest());
    d.move_layer(l2, Placement::Bottom).unwrap();
    states.push(d.digest());
    d.set_name(l1, "Inks").unwrap();
    states.push(d.digest());
    d.set_locked(l1, true).unwrap();
    states.push(d.digest());
    d.remove_layer(l2).unwrap();
    states.push(d.digest());
    d.set_locked(l1, false).unwrap();
    states.push(d.digest());
    d.clear_layer(l1).unwrap();
    states.push(d.digest());
    assert_eq!(d.history_len(), states.len() - 1);
    // H2: each undo lands on the previous state. Active layers can differ
    // where the reference keeps the current selection, so compare the
    // rest; the stack's active layer is checked by its own test.
    for expected in states.iter().rev().skip(1) {
        d.undo().unwrap();
        let mut got = d.digest();
        got.active = expected.active;
        assert_eq!(&got, expected);
        d.check_invariants();
    }
    for expected in states.iter().skip(1) {
        d.redo().unwrap();
        let mut got = d.digest();
        got.active = expected.active;
        assert_eq!(&got, expected, "H1");
    }
}

#[test]
fn removed_layers_come_back_with_their_id_position_and_pixels() {
    let mut d = Document::new(1024, 1024);
    let l1 = d.active_layer();
    let l2 = d.add_layer(Placement::Top).unwrap();
    let l3 = d.add_layer(Placement::Top).unwrap();
    stroke(&mut d, l2, &[A, B], 4, 77);
    d.set_opacity(l2, 0.5).unwrap();
    d.set_name(l2, "Shading").unwrap();
    d.set_active_layer(l2).unwrap();
    let before = d.digest();
    let ptr = tile_ptr(&d, l2, A);
    d.remove_layer(l2).unwrap();
    assert_eq!(d.active_layer(), l1, "the layer below");
    assert!(
        d.history_bytes() >= 2 * TILE_BYTES,
        "the layer's tiles are held"
    );
    d.set_active_layer(l3).unwrap();
    d.undo().unwrap();
    assert_eq!(
        d.digest(),
        before,
        "H10: same id, position, props, pixels, active"
    );
    assert_eq!(d.find_layer(l2.get()), Some(l2));
    assert_eq!(tile_ptr(&d, l2, A), ptr, "the layer moved back, not copied");
    d.redo().unwrap();
    assert!(d.layer(l2).is_none());
    assert_eq!(
        d.active_layer(),
        l1,
        "redo restores the active layer after removal"
    );
}

#[test]
fn active_layer_after_undo_follows_the_reference() {
    let mut d = Document::new(512, 512);
    let l1 = d.active_layer();
    let l2 = d.add_layer(Placement::Top).unwrap();
    // U5: undoing a creation restores the layer active before it, even if
    // the selection changed since.
    let l3 = d.add_layer(Placement::Top).unwrap();
    d.set_active_layer(l1).unwrap();
    d.undo().unwrap();
    assert_eq!(d.active_layer(), l2);
    d.redo().unwrap();
    assert_eq!(
        d.active_layer(),
        l3,
        "redo restores the active layer after creation"
    );
    // U7: undoing a stroke keeps the current selection.
    stroke(&mut d, l3, &[A], 1, 1);
    d.set_active_layer(l1).unwrap();
    d.undo().unwrap();
    assert_eq!(d.active_layer(), l1);
    // U8: so does undoing a reorder.
    d.move_layer(l3, Placement::Bottom).unwrap();
    d.set_active_layer(l2).unwrap();
    d.undo().unwrap();
    assert_eq!(d.active_layer(), l2);
    assert_eq!(d.index_of(l3), Some(2));
    d.check_invariants();
}

#[test]
fn every_recorded_mutation_is_one_step_and_the_others_none() {
    let mut d = Document::new(512, 512);
    let l1 = d.active_layer();
    let mut n = 0;
    let mut step = |d: &Document, recorded: bool, what: &str| {
        n += usize::from(recorded);
        assert_eq!(d.history_len(), n, "H6: {what}");
    };
    let l2 = d.add_layer(Placement::Top).unwrap();
    step(&d, true, "add");
    stroke(&mut d, l2, &[A], 1, 1);
    step(&d, true, "paint");
    d.move_layer(l2, Placement::Bottom).unwrap();
    step(&d, true, "move");
    d.set_blend_mode(l2, BlendMode::Normal).unwrap();
    step(&d, false, "same blend mode");
    d.set_name(l2, "A").unwrap();
    step(&d, true, "rename");
    d.set_name(l2, "A").unwrap();
    step(&d, false, "same name");
    d.set_opacity(l2, 0.5).unwrap();
    step(&d, true, "opacity");
    d.set_locked(l2, true).unwrap();
    step(&d, true, "lock");
    d.set_locked(l2, true).unwrap();
    step(&d, false, "same lock");
    assert_eq!(d.set_opacity(l2, 0.1).err(), Some(LayerError::Locked(l2)));
    assert_eq!(d.clear_layer(l2).err(), Some(LayerError::Locked(l2)));
    step(&d, false, "refused operations");
    d.set_visible(l2, false).unwrap();
    d.set_active_layer(l2).unwrap();
    step(&d, false, "visibility and selection are not recorded (U9)");
    d.set_locked(l2, false).unwrap();
    step(&d, true, "unlock");
    d.clear_layer(l2).unwrap();
    step(&d, true, "clear");
    d.clear_layer(l2).unwrap();
    step(&d, false, "clearing an empty layer");
    d.move_layer(l2, Placement::Bottom).unwrap();
    step(&d, false, "moving to where it is");
    d.remove_layer(l1).unwrap();
    step(&d, true, "remove");
    assert_eq!(d.next_undo(), Some(HistoryAction::RemoveLayer));
}

#[test]
fn visibility_is_outside_history() {
    let mut d = Document::new(512, 512);
    let l = d.active_layer();
    stroke(&mut d, l, &[A], 1, 1);
    d.set_visible(l, false).unwrap();
    d.undo().unwrap();
    assert!(
        !d.layer(l).unwrap().visible(),
        "undo skips the visibility change"
    );
    assert_eq!(
        d.layer(l).unwrap().pixels().tile_count(),
        0,
        "and undid the stroke"
    );
}

#[test]
fn opacity_edits_merge_until_something_interrupts_them() {
    let mut d = Document::new(512, 512);
    let l1 = d.active_layer();
    let l2 = d.add_layer(Placement::Top).unwrap();
    let base = d.history_len();
    // A drag: many values, one step (H8); a second drag right after merges
    // too (U14, U15).
    for v in [0.9, 0.8, 0.7, 0.6] {
        d.set_opacity(l2, v).unwrap();
    }
    d.set_opacity(l2, 0.4).unwrap();
    assert_eq!(d.history_len(), base + 1);
    d.undo().unwrap();
    assert_eq!(
        d.layer(l2).unwrap().opacity(),
        1.0,
        "one undo, back to the start"
    );
    d.redo().unwrap();
    // Undo/redo ended the chain: a new drag is a new step.
    d.set_opacity(l2, 0.3).unwrap();
    assert_eq!(d.history_len(), base + 2);
    // A selection change ends it (U16).
    d.set_active_layer(l1).unwrap();
    d.set_active_layer(l2).unwrap();
    d.set_opacity(l2, 0.2).unwrap();
    assert_eq!(d.history_len(), base + 3);
    // Another layer is another step; so is anything recorded between (U17).
    d.set_opacity(l1, 0.5).unwrap();
    assert_eq!(d.history_len(), base + 4);
    d.set_name(l1, "x").unwrap();
    d.set_opacity(l1, 0.6).unwrap();
    assert_eq!(d.history_len(), base + 6);
    // Visibility is not recorded and does not interrupt.
    d.set_visible(l1, false).unwrap();
    d.set_opacity(l1, 0.7).unwrap();
    assert_eq!(d.history_len(), base + 6);
    d.undo().unwrap();
    assert_eq!(d.layer(l1).unwrap().opacity(), 0.5);
}

#[test]
fn clearing_moves_the_surface_into_history() {
    let mut d = Document::new(1024, 1024);
    let l = d.active_layer();
    stroke(&mut d, l, &[A, B], 1, 9);
    let ptr = tile_ptr(&d, l, A);
    let before = d.digest();
    d.clear_layer(l).unwrap();
    assert_eq!(d.layer(l).unwrap().pixels().tile_count(), 0);
    d.undo().unwrap();
    assert_eq!(d.digest(), before);
    assert_eq!(tile_ptr(&d, l, A), ptr, "no copy either way");
}

#[test]
fn step_limit_drops_the_oldest_steps() {
    let mut d = Document::new(512, 512);
    let l = d.active_layer();
    d.set_history_limits(HistoryLimits {
        max_steps: 3,
        ..HistoryLimits::default()
    });
    for k in 0..6 {
        stroke(&mut d, l, &[A], k, 1);
    }
    assert_eq!(d.history_len(), 3, "H4");
    while d.can_undo() {
        d.undo().unwrap();
    }
    let px = d.layer(l).unwrap().pixels();
    assert_eq!(
        px.pixel(2, 2),
        [1; 4],
        "steps 0..=2 were dropped, so they stay"
    );
    assert_eq!(px.pixel(3, 3), [0; 4]);
}

#[test]
fn byte_limit_keeps_an_oversized_step_alone() {
    let mut d = Document::new(4096, 256);
    let l = d.active_layer();
    d.set_history_limits(HistoryLimits {
        max_bytes: 3 * TILE_BYTES,
        ..HistoryLimits::default()
    });
    stroke(&mut d, l, &[A], 1, 1);
    stroke(&mut d, l, &[B], 1, 1);
    assert_eq!(d.history_len(), 2);
    assert!(d.history_reserved_bytes() <= 3 * TILE_BYTES);
    // Eight new tiles: larger than the whole budget.
    let wide: Vec<TileCoord> = (2..10).map(|x| TileCoord { x, y: 0 }).collect();
    stroke(&mut d, l, &wide, 1, 1);
    assert_eq!(d.history_len(), 1, "H4: the oversized step is kept alone");
    d.undo().unwrap();
    assert!(d.layer(l).unwrap().pixels().tile(wide[0]).is_none());
    assert!(d.history_bytes() <= d.history_reserved_bytes());
    // Shrinking the limits applies at once; 0 steps disables history.
    d.set_history_limits(HistoryLimits {
        max_steps: 0,
        ..HistoryLimits::default()
    });
    assert!(!d.can_undo() && !d.can_redo());
    stroke(&mut d, l, &[A], 2, 2);
    assert!(!d.can_undo());
}

#[test]
fn memory_accounting_counts_held_tiles() {
    let mut d = Document::new(1024, 1024);
    let l = d.active_layer();
    stroke(&mut d, l, &[A, B], 1, 1); // two new tiles: nothing held yet
    assert!(d.history_bytes() < TILE_BYTES);
    assert!(
        d.history_reserved_bytes() >= 2 * TILE_BYTES,
        "reserved for the undo"
    );
    d.undo().unwrap(); // now the history holds the two tiles
    assert!(d.history_bytes() >= 2 * TILE_BYTES);
    assert!(d.history_bytes() <= d.history_reserved_bytes());
}

/// One mask "stroke": sets mask value (k, k) of every declared tile to `v`
/// (reveal units).
fn mask_stroke(d: &mut Document, id: LayerId, coords: &[TileCoord], k: u32, v: u8) {
    d.modify_mask(id, &tiles(coords), |_, t| *t.get_mut(k, k) = v)
        .unwrap();
}

fn mask_tile_ptr(d: &Document, id: LayerId, c: TileCoord) -> Option<*const u8> {
    d.layer(id)?
        .mask()?
        .hidden_tile(c)
        .map(|t| t.pixels().as_ptr())
}

#[test]
fn mask_clip_and_alpha_lock_edits_are_one_step_each() {
    let mut d = Document::new(512, 512);
    let l = d.active_layer();
    let mut n = 0;
    let mut step = |d: &Document, recorded: bool, action: Option<HistoryAction>| {
        n += usize::from(recorded);
        assert_eq!(d.history_len(), n, "H6: {action:?}");
        if recorded {
            assert_eq!(d.next_undo(), action);
        }
    };
    d.add_mask(l).unwrap();
    step(&d, true, Some(HistoryAction::AddMask));
    mask_stroke(&mut d, l, &[A], 1, 0);
    step(&d, true, Some(HistoryAction::PaintMask));
    d.set_mask_enabled(l, false).unwrap();
    step(&d, true, Some(HistoryAction::MaskEnabled));
    d.set_mask_enabled(l, false).unwrap();
    step(&d, false, None);
    d.set_active_part(LayerPart::Pixels).unwrap();
    step(&d, false, None);
    d.set_clipped(l, true).unwrap();
    step(&d, true, Some(HistoryAction::Clip));
    d.set_clipped(l, true).unwrap();
    step(&d, false, None);
    d.set_alpha_locked(l, true).unwrap();
    step(&d, true, Some(HistoryAction::AlphaLock));
    d.set_alpha_locked(l, true).unwrap();
    step(&d, false, None);
    assert_eq!(d.add_mask(l).err(), Some(LayerError::HasMask(l)));
    step(&d, false, None);
    d.remove_mask(l).unwrap();
    step(&d, true, Some(HistoryAction::RemoveMask));
    assert_eq!(d.remove_mask(l).err(), Some(LayerError::NoMask(l)));
    step(&d, false, None);
}

#[test]
fn mask_edits_undo_and_redo_bit_exactly() {
    let mut d = Document::new(1024, 1024);
    let l = d.active_layer();
    stroke(&mut d, l, &[A, B], 2, 200);
    let mut states = vec![d.digest()];
    d.add_mask(l).unwrap();
    states.push(d.digest());
    mask_stroke(&mut d, l, &[A], 2, 10);
    states.push(d.digest());
    mask_stroke(&mut d, l, &[A, B], 3, 90);
    states.push(d.digest());
    d.set_mask_enabled(l, false).unwrap();
    states.push(d.digest());
    d.set_clipped(l, true).unwrap();
    states.push(d.digest());
    d.set_alpha_locked(l, true).unwrap();
    states.push(d.digest());
    d.remove_mask(l).unwrap();
    states.push(d.digest());
    for want in states.iter().rev().skip(1) {
        d.undo().unwrap();
        assert_eq!(&d.digest(), want, "M5 / CL4: undo");
        d.check_invariants();
    }
    for want in states.iter().skip(1) {
        d.redo().unwrap();
        assert_eq!(&d.digest(), want, "M5 / CL4: redo");
        d.check_invariants();
    }
}

#[test]
fn mask_undo_swaps_tiles_and_restores_absent_ones() {
    let mut d = Document::new(1024, 1024);
    let l = d.active_layer();
    d.add_mask(l).unwrap();
    mask_stroke(&mut d, l, &[A], 1, 50);
    mask_stroke(&mut d, l, &[A, B], 2, 90);
    let live = mask_tile_ptr(&d, l, A).unwrap();
    d.undo().unwrap();
    assert!(mask_tile_ptr(&d, l, B).is_none(), "absent again (M3)");
    let restored = mask_tile_ptr(&d, l, A).unwrap();
    assert_ne!(live, restored);
    d.redo().unwrap();
    assert_eq!(mask_tile_ptr(&d, l, A), Some(live), "H5: swapped back");
    // A removed mask comes back with its tiles (no copy).
    d.remove_mask(l).unwrap();
    d.undo().unwrap();
    assert_eq!(mask_tile_ptr(&d, l, A), Some(live));
    assert_eq!(d.layer(l).unwrap().mask().unwrap().value(2, 2), 90);
}

#[test]
fn undoing_a_mask_away_targets_the_pixels() {
    let mut d = Document::new(64, 64);
    let l = d.active_layer();
    d.add_mask(l).unwrap();
    assert_eq!(d.active_part(), LayerPart::Mask);
    d.undo().unwrap();
    assert!(d.layer(l).unwrap().mask().is_none());
    assert_eq!(d.active_part(), LayerPart::Pixels);
    d.redo().unwrap();
    d.check_invariants();
}
