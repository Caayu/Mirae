use tela_canvas::{DirtyTiles, IRect, Tile, TileCoord, TileCoverage};

use crate::{Document, HistoryAction, Placement, SelectionOp, SelectionShape};

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> SelectionShape {
    SelectionShape::Rect([x0, y0], [x1, y1])
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> SelectionShape {
    SelectionShape::Ellipse {
        center: [cx, cy],
        radii: [rx, ry],
        rotation: 0.0,
    }
}

#[test]
fn a_new_document_has_no_selection_and_edits_are_unrestricted() {
    let d = Document::new(600, 400);
    assert!(!d.has_selection());
    assert_eq!(d.selection_coverage(10, 10), 255);
    assert!(matches!(
        d.selection_tile(TileCoord { x: 0, y: 0 }),
        TileCoverage::Full
    ));
}

#[test]
fn every_selection_change_is_one_step_and_undoes_bit_exactly() {
    // Reference S28, S29.
    let mut d = Document::new(600, 400);
    let mut states = vec![d.digest()];
    let mut revisions = vec![d.revision()];
    let ops: [&dyn Fn(&mut Document) -> bool; 7] = [
        &|d| d.select(&rect(10.0, 10.0, 200.0, 150.0), false, SelectionOp::Replace),
        &|d| d.select(&ellipse(200.0, 150.0, 80.0, 60.0), true, SelectionOp::Add),
        &|d| {
            d.select(
                &rect(150.0, 100.0, 180.0, 300.0),
                true,
                SelectionOp::Subtract,
            )
        },
        &|d| d.feather_selection(12.0),
        &|d| d.invert_selection(),
        &|d| {
            d.select(
                &ellipse(300.0, 200.0, 250.0, 150.0),
                true,
                SelectionOp::Intersect,
            )
        },
        &|d| d.select_all(),
    ];
    let actions = [
        HistoryAction::Select,
        HistoryAction::Select,
        HistoryAction::Select,
        HistoryAction::FeatherSelection,
        HistoryAction::InvertSelection,
        HistoryAction::Select,
        HistoryAction::SelectAll,
    ];
    for (op, action) in ops.iter().zip(actions) {
        let before = d.history_len();
        let rev = d.selection_revision();
        assert!(op(&mut d));
        assert_eq!(d.history_len(), before + 1);
        assert_eq!(d.next_undo(), Some(action));
        assert_ne!(d.selection_revision(), rev);
        d.check_invariants();
        states.push(d.digest());
        revisions.push(d.revision());
    }
    assert!(d.deselect());
    states.push(d.digest());
    revisions.push(d.revision());
    // Undo walks back through every state exactly, redo forward again.
    for i in (0..states.len() - 1).rev() {
        let dirty = d.undo().unwrap();
        assert!(
            dirty.is_empty(),
            "selection changes never touch the composite"
        );
        assert_eq!(d.digest(), states[i]);
        assert_eq!(d.revision(), revisions[i]);
        d.check_invariants();
    }
    for state in states.iter().skip(1) {
        d.redo().unwrap();
        assert_eq!(&d.digest(), state);
        d.check_invariants();
    }
}

#[test]
fn an_empty_result_is_no_selection() {
    // Reference S5, S17, S18.
    let mut d = Document::new(600, 400);
    assert!(d.select_all());
    assert!(d.invert_selection());
    assert!(!d.has_selection(), "inverting everything");
    assert_eq!(d.next_undo(), Some(HistoryAction::InvertSelection));
    d.undo().unwrap();
    assert!(d.has_selection());

    d.select(&rect(10.0, 10.0, 100.0, 100.0), false, SelectionOp::Replace);
    assert!(d.select(&rect(0.0, 0.0, 300.0, 300.0), false, SelectionOp::Subtract));
    assert!(!d.has_selection(), "subtracting everything");
    d.undo().unwrap();
    assert!(d.select(
        &rect(400.0, 300.0, 500.0, 350.0),
        false,
        SelectionOp::Intersect
    ));
    assert!(!d.has_selection(), "an empty intersection");
    d.undo().unwrap();
    // A click (no area) in Replace mode deselects (S5).
    assert!(d.select(&rect(50.0, 50.0, 50.0, 50.0), true, SelectionOp::Replace));
    assert!(!d.has_selection());
    d.check_invariants();
}

#[test]
fn no_ops_and_refusals_record_nothing() {
    let mut d = Document::new(600, 400);
    let steps = d.history_len();
    assert!(!d.deselect());
    assert!(!d.invert_selection());
    assert!(!d.feather_selection(10.0));
    assert!(!d.select(&rect(1.0, 1.0, 50.0, 50.0), false, SelectionOp::Subtract));
    assert!(!d.select(&rect(1.0, 1.0, 50.0, 50.0), false, SelectionOp::Intersect));
    assert!(!d.select(&rect(1.0, 1.0, 1.0, 50.0), false, SelectionOp::Replace));
    assert_eq!(d.history_len(), steps);
    assert!(d.select(&rect(1.0, 1.0, 50.0, 50.0), false, SelectionOp::Replace));
    let steps = d.history_len();
    let digest = d.digest();
    // Replacing a selection with itself, adding a part of it, feathering by
    // nothing or garbage: no change, no step.
    assert!(!d.select(&rect(1.0, 1.0, 50.0, 50.0), false, SelectionOp::Replace));
    assert!(!d.select(&rect(10.0, 10.0, 20.0, 20.0), false, SelectionOp::Add));
    for bad in [0.0, -1.0, f32::NAN] {
        assert!(!d.feather_selection(bad));
    }
    assert!(d.select_all());
    assert!(!d.select_all());
    d.undo().unwrap();
    assert_eq!(d.history_len(), steps);
    assert_eq!(d.digest(), digest);
}

#[test]
fn without_a_selection_add_replaces() {
    let mut d = Document::new(600, 400);
    assert!(d.select(&rect(5.0, 5.0, 60.0, 60.0), false, SelectionOp::Add));
    assert_eq!(d.selection().unwrap().bounds(), IRect::new(5, 5, 60, 60));
}

#[test]
fn select_all_invert_and_deselect_stay_cheap_on_large_canvases() {
    let mut d = Document::new(8000, 6000);
    d.select_all();
    assert_eq!(d.selection().unwrap().stored_tile_count(), 0);
    d.select(
        &rect(100.0, 100.0, 400.0, 300.0),
        false,
        SelectionOp::Subtract,
    );
    assert_eq!(d.selection().unwrap().stored_tile_count(), 4);
    d.invert_selection();
    assert_eq!(d.selection().unwrap().stored_tile_count(), 4);
    assert_eq!(
        d.selection().unwrap().bounds(),
        IRect::new(100, 100, 400, 300)
    );
    assert!(d.history_bytes() <= d.history_reserved_bytes());
    assert!(
        d.history_reserved_bytes() < 2 * 4 * 65536 + 4096,
        "history holds tiles, not canvases"
    );
    d.deselect();
    assert!(!d.has_selection());
}

#[test]
fn the_selection_belongs_to_the_document_not_to_a_layer() {
    // Reference S13: switching, hiding, adding, removing layers keep it.
    let mut d = Document::new(600, 400);
    d.select(
        &ellipse(200.0, 150.0, 90.0, 70.0),
        true,
        SelectionOp::Replace,
    );
    let digest = d.selection().cloned();
    let l1 = d.active_layer();
    let l2 = d.add_layer(Placement::Top).unwrap();
    d.set_visible(l1, false).unwrap();
    d.set_active_layer(l1).unwrap();
    d.remove_layer(l2).unwrap();
    d.undo().unwrap();
    assert_eq!(d.selection().cloned(), digest);
    // And layer history steps do not move it.
    d.undo().unwrap();
    assert_eq!(d.selection().cloned(), digest);
}

#[test]
fn pixel_edits_limited_by_the_selection_leave_uncovered_pixels_alone() {
    // A whole-tile edit through the selection's coverage: what the app
    // does when it commits a stroke (SEL4).
    let mut d = Document::new(600, 400);
    d.select(&rect(10.0, 10.0, 100.0, 50.0), false, SelectionOp::Replace);
    let l = d.active_layer();
    let mut tiles = DirtyTiles::new();
    tiles.mark(TileCoord { x: 0, y: 0 });
    let coverage: Vec<u8> = (0..256 * 256)
        .map(|i| d.selection_coverage(i % 256, i / 256))
        .collect();
    d.modify_tiles(l, &tiles, |_, t: &mut Tile<[u8; 4]>| {
        for (p, &c) in t.pixels_mut().iter_mut().zip(&coverage) {
            if c > 0 {
                *p = [c; 4];
            }
        }
    })
    .unwrap();
    let px = d.layer(l).unwrap().pixels();
    assert_eq!(px.pixel(5, 5).a, 0.0);
    assert_eq!(px.pixel(50, 20).a, 1.0);
    assert_eq!(px.pixel(150, 20).a, 0.0);
}

#[test]
fn feathering_too_small_a_selection_away_leaves_no_selection() {
    let mut d = Document::new(600, 400);
    d.select(
        &rect(100.0, 100.0, 101.0, 101.0),
        false,
        SelectionOp::Replace,
    );
    assert!(d.feather_selection(200.0));
    assert!(!d.has_selection());
    d.undo().unwrap();
    assert_eq!(
        d.selection().unwrap().bounds(),
        IRect::new(100, 100, 101, 101)
    );
}
