//! Selections end to end (phase 5C): real brush strokes committed as the
//! app commits them, through the document's selection, at both depths, on
//! pixels and masks, with history and files. Every stroke also checks that
//! pixels with coverage 0 are unchanged (`support::stroke`, SEL4).

mod support;

use support::*;
use tela_color::{Color, PixelDepth};
use tela_document::{Document, LayerPart, Placement, SelectionOp, SelectionShape};

/// The selections of the matrix.
#[derive(Clone, Copy, Debug)]
enum Sel {
    None,
    Full,
    Hard,
    Feathered,
}

fn select(d: &mut Document, sel: Sel) {
    match sel {
        Sel::None => {}
        Sel::Full => drop(d.select_all()),
        Sel::Hard => drop(d.select(
            &SelectionShape::Rect([60.0, 40.0], [300.0, 220.0]),
            false,
            SelectionOp::Replace,
        )),
        Sel::Feathered => {
            d.select(
                &SelectionShape::Ellipse {
                    center: [200.0, 150.0],
                    radii: [140.0, 90.0],
                    rotation: 0.3,
                },
                true,
                SelectionOp::Replace,
            );
            d.feather_selection(25.0);
        }
    }
}

/// A layer with content under the stroke (so erasing has something to
/// remove) and the stroke target chosen.
fn document(depth: PixelDepth, sel: Sel, mask: bool) -> Document {
    let mut d = Document::with_depth(420, 300, depth);
    stroke(
        &mut d,
        &round(160.0, 1.0),
        colour(2),
        &line((0.0, 150.0), (420.0, 150.0), 30),
        7,
    )
    .expect("paints");
    if mask {
        let l = d.active_layer();
        d.add_mask(l).unwrap();
        // Hide part of the layer first, so painting the mask reveals.
        stroke(
            &mut d,
            &eraser(120.0),
            Color::new(0.0, 0.0, 0.0),
            &line((0.0, 120.0), (420.0, 180.0), 30),
            8,
        )
        .expect("mask accepts");
    }
    select(&mut d, sel);
    d
}

#[test]
fn preview_equals_commit_through_every_kind_of_selection() {
    let colour = Color::new(37.0 / 255.0, 149.0 / 255.0, 213.0 / 255.0);
    let path = line((10.0, 60.0), (410.0, 250.0), 40);
    for depth in [PixelDepth::Rgba8, PixelDepth::Rgba16] {
        for sel in [Sel::None, Sel::Full, Sel::Hard, Sel::Feathered] {
            for mask in [false, true] {
                for erase in [false, true] {
                    let mut d = document(depth, sel, mask);
                    let preset = if erase {
                        eraser(70.0)
                    } else {
                        round(70.0, 0.8)
                    };
                    let done = stroke(&mut d, &preset, colour, &path, 11).expect("paints");
                    assert!(
                        done.preview_vs_commit <= 1,
                        "{depth:?} {sel:?} mask {mask} erase {erase}: {}",
                        done.preview_vs_commit
                    );
                    check(&d);
                }
            }
        }
    }
}

/// Every layer's stored pixel and mask value on the canvas (not the
/// pixels a brush may leave beyond its edge, debt #14).
fn canvas_digest(d: &Document) -> u64 {
    let mut h = Hash::new();
    for l in d.layers() {
        for y in 0..d.height() as i32 {
            for x in 0..d.width() as i32 {
                let p = l.pixels().pixel(x, y);
                for v in [p.r, p.g, p.b, p.a] {
                    h.eat(&v.to_bits().to_le_bytes());
                }
            }
        }
        if let Some(m) = l.mask() {
            for c in d.bounds().tiles() {
                if let Some(t) = m.reveal_tile(c) {
                    let (ox, oy) = c.origin();
                    let on_canvas = t.enumerate().filter(|(i, _)| {
                        let (x, y) = (ox + (*i % 256) as i32, oy + (*i / 256) as i32);
                        x < d.width() as i32 && y < d.height() as i32
                    });
                    h.eat(&on_canvas.map(|(_, v)| v).collect::<Vec<u8>>());
                }
            }
        }
    }
    h.finish()
}

#[test]
fn a_full_selection_edits_exactly_like_no_selection() {
    // SEL3, bit for bit on the canvas, for pixels and masks at both
    // depths. Beyond the canvas a selection covers nothing (S12), so the
    // off-canvas pixels a brush leaves without one (debt #14) differ.
    let path = line((0.0, 20.0), (420.0, 280.0), 40);
    for depth in [PixelDepth::Rgba8, PixelDepth::Rgba16] {
        for mask in [false, true] {
            for erase in [false, true] {
                let preset = if erase {
                    eraser(90.0)
                } else {
                    preset(tela_brush::preset::builtin::ids::SOFT_ROUND)
                };
                let mut free = document(depth, Sel::None, mask);
                let mut full = document(depth, Sel::Full, mask);
                stroke(&mut free, &preset, colour(4), &path, 3).unwrap();
                stroke(&mut full, &preset, colour(4), &path, 3).unwrap();
                assert_eq!(
                    canvas_digest(&free),
                    canvas_digest(&full),
                    "{depth:?} mask {mask} erase {erase}"
                );
                for c in free.bounds().tiles() {
                    assert_eq!(
                        compose(&free, c, None),
                        compose(&full, c, None),
                        "composite at {c:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn alpha_lock_and_clipping_work_through_a_selection() {
    for depth in [PixelDepth::Rgba8, PixelDepth::Rgba16] {
        let mut d = document(depth, Sel::Feathered, false);
        let base = d.active_layer();
        d.set_alpha_locked(base, true).unwrap();
        let done = stroke(
            &mut d,
            &round(100.0, 1.0),
            colour(5),
            &line((0.0, 100.0), (420.0, 200.0), 30),
            4,
        )
        .unwrap();
        assert!(done.preview_vs_commit <= 1);
        let clipped = d.add_layer(Placement::Above(base)).unwrap();
        d.set_clipped(clipped, true).unwrap();
        let done = stroke(
            &mut d,
            &round(100.0, 1.0),
            colour(1),
            &line((0.0, 200.0), (420.0, 100.0), 30),
            5,
        )
        .unwrap();
        assert!(done.preview_vs_commit <= 1);
        check(&d);
    }
}

#[test]
fn a_selection_never_changes_the_picture_by_itself() {
    // Selection ≠ layer mask: making, combining, feathering, inverting and
    // removing a selection never touches the composite or the layers.
    let mut d = document(PixelDepth::Rgba8, Sel::None, true);
    let picture = composite_digest(&d);
    let content = history_digest(&d);
    for sel in [Sel::Hard, Sel::Feathered, Sel::Full] {
        select(&mut d, sel);
        d.invert_selection();
        assert_eq!(composite_digest(&d), picture);
        assert_eq!(history_digest(&d), content);
    }
    d.deselect();
    assert_eq!(composite_digest(&d), picture);
}

#[test]
fn undo_walks_selections_and_strokes_back_exactly() {
    let mut d = document(PixelDepth::Rgba16, Sel::None, false);
    let mut states = vec![(history_digest(&d), selection_digest(&d))];
    let mut record = |d: &Document| states.push((history_digest(d), selection_digest(d)));
    select(&mut d, Sel::Hard);
    record(&d);
    stroke(
        &mut d,
        &round(50.0, 1.0),
        colour(3),
        &line((0.0, 0.0), (420.0, 300.0), 30),
        1,
    )
    .unwrap();
    record(&d);
    d.select(
        &SelectionShape::Ellipse {
            center: [300.0, 200.0],
            radii: [80.0, 60.0],
            rotation: 0.0,
        },
        true,
        SelectionOp::Add,
    );
    record(&d);
    d.feather_selection(12.0);
    record(&d);
    stroke(
        &mut d,
        &eraser(60.0),
        colour(0),
        &line((420.0, 0.0), (0.0, 300.0), 30),
        2,
    )
    .unwrap();
    record(&d);
    d.invert_selection();
    record(&d);
    // Undo restores each state; undoing a stroke keeps the selection that
    // was there (S29).
    for i in (0..states.len() - 1).rev() {
        d.undo().unwrap();
        assert_eq!(
            (history_digest(&d), selection_digest(&d)),
            states[i],
            "undo to {i}"
        );
    }
    for (i, state) in states.iter().enumerate().skip(1) {
        d.redo().unwrap();
        assert_eq!(
            (history_digest(&d), selection_digest(&d)),
            *state,
            "redo to {i}"
        );
    }
}

#[test]
fn files_do_not_keep_the_selection_and_their_bytes_do_not_change() {
    // SEL10 (the reference does not save it either, S30): a document with
    // a selection saves to exactly the bytes of the same document without
    // one, so the .mirae v1 format is untouched.
    let mut with = document(PixelDepth::Rgba8, Sel::Feathered, true);
    let without = {
        let mut d = with.clone();
        d.deselect();
        d
    };
    assert!(with.has_selection());
    assert_eq!(save_bytes(&with), save_bytes(&without));
    let back = round_trip(&with);
    assert!(!back.has_selection());
    // The selection is a modification (a history step, S32), like any edit.
    let before = with.revision();
    select(&mut with, Sel::Hard);
    assert_ne!(with.revision(), before);
    assert_eq!(
        with.active_part(),
        LayerPart::Mask,
        "selecting keeps the edit target"
    );
}
