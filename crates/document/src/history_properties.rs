//! Property test of undo/redo: random sequences of edits interleaved with
//! undo and redo, checked after every step against a model that is just a
//! stack of document snapshots (digests). The model encodes the reference
//! behaviour recorded in docs/parity/4B-undo-redo.md: visibility and
//! selection are outside history, consecutive opacity edits of one layer
//! merge, and only add/remove steps restore the active layer; and in
//! docs/parity/4D-masks-clipping-alpha-lock.md: mask add/remove/enable,
//! mask strokes, clipping and alpha lock are steps (M5, CL4).

use proptest::prelude::*;
use tela_canvas::{DirtyTiles, TileCoord};

use crate::document::Digest;
use crate::{Document, LayerError, LayerId, Placement};

#[derive(Clone, Debug)]
enum Edit {
    Add(u8, usize),
    Remove(usize),
    Move(usize, u8, usize),
    Activate(usize),
    Show(usize, bool),
    Opacity(usize, u8),
    Lock(usize, bool),
    Rename(usize, u8),
    Paint(usize, u8, u8),
    Clear(usize),
    AddMask(usize),
    RemoveMask(usize),
    MaskEnabled(usize, bool),
    PaintMask(usize, u8, u8),
    Clip(usize, bool),
    AlphaLock(usize, bool),
}

#[derive(Clone, Debug)]
enum HOp {
    Edit(Edit),
    Undo,
    Redo,
}

fn hop() -> impl Strategy<Value = HOp> {
    let edit = prop_oneof![
        2 => (0u8..4, any::<usize>()).prop_map(|(k, a)| Edit::Add(k, a)),
        1 => any::<usize>().prop_map(Edit::Remove),
        1 => (any::<usize>(), 0u8..4, any::<usize>()).prop_map(|(i, k, a)| Edit::Move(i, k, a)),
        1 => any::<usize>().prop_map(Edit::Activate),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Edit::Show(i, v)),
        // Few distinct values, so repeated (no-op) values happen too.
        2 => (any::<usize>(), 0u8..5).prop_map(|(i, o)| Edit::Opacity(i, o)),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Edit::Lock(i, v)),
        1 => (any::<usize>(), 0u8..3).prop_map(|(i, n)| Edit::Rename(i, n)),
        3 => (any::<usize>(), 0u8..3, 0u8..3).prop_map(|(i, t, v)| Edit::Paint(i, t, v)),
        1 => any::<usize>().prop_map(Edit::Clear),
        1 => any::<usize>().prop_map(Edit::AddMask),
        1 => any::<usize>().prop_map(Edit::RemoveMask),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Edit::MaskEnabled(i, v)),
        2 => (any::<usize>(), 0u8..3, 0u8..3).prop_map(|(i, t, v)| Edit::PaintMask(i, t, v)),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Edit::Clip(i, v)),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Edit::AlphaLock(i, v)),
    ];
    prop_oneof![
        6 => edit.prop_map(HOp::Edit),
        2 => Just(HOp::Undo),
        2 => Just(HOp::Redo),
    ]
}

fn pick(d: &Document, removed: &[LayerId], i: usize) -> LayerId {
    let mut all: Vec<LayerId> = d.layers().map(|l| l.id()).collect();
    all.extend_from_slice(removed);
    all[i % all.len()]
}

fn placement(kind: u8, anchor: LayerId) -> Placement {
    match kind {
        0 => Placement::Above(anchor),
        1 => Placement::Below(anchor),
        2 => Placement::Top,
        _ => Placement::Bottom,
    }
}

fn ok<T>(r: Result<T, LayerError>) -> Result<(), LayerError> {
    r.map(|_| ())
}

/// Applies an edit; returns whether it succeeded.
fn edit(d: &mut Document, removed: &mut Vec<LayerId>, e: &Edit) -> Result<(), LayerError> {
    match *e {
        Edit::Add(k, a) => {
            let anchor = pick(d, removed, a);
            ok(d.add_layer(placement(k, anchor)))
        }
        Edit::Remove(i) => {
            let id = pick(d, removed, i);
            let r = ok(d.remove_layer(id));
            if r.is_ok() {
                removed.push(id);
            }
            r
        }
        Edit::Move(i, k, a) => {
            let (id, anchor) = (pick(d, removed, i), pick(d, removed, a));
            ok(d.move_layer(id, placement(k, anchor)))
        }
        Edit::Activate(i) => d.set_active_layer(pick(d, removed, i)),
        Edit::Show(i, v) => ok(d.set_visible(pick(d, removed, i), v)),
        Edit::Opacity(i, o) => ok(d.set_opacity(pick(d, removed, i), f32::from(o) / 4.0)),
        Edit::Lock(i, v) => d.set_locked(pick(d, removed, i), v),
        Edit::Rename(i, n) => d.set_name(pick(d, removed, i), ["a", "b", "Layer 9"][n as usize]),
        Edit::Paint(i, t, v) => {
            let mut tiles = DirtyTiles::new();
            tiles.mark(TileCoord {
                x: i32::from(t),
                y: 0,
            });
            let px = [60 * (v + 1); 4];
            ok(d.modify_tiles(pick(d, removed, i), &tiles, |_, tile| {
                *tile.get_mut(u32::from(v), 1) = px;
            }))
        }
        Edit::Clear(i) => ok(d.clear_layer(pick(d, removed, i))),
        Edit::AddMask(i) => ok(d.add_mask(pick(d, removed, i))),
        Edit::RemoveMask(i) => ok(d.remove_mask(pick(d, removed, i))),
        Edit::MaskEnabled(i, v) => ok(d.set_mask_enabled(pick(d, removed, i), v)),
        Edit::PaintMask(i, t, v) => {
            let mut tiles = DirtyTiles::new();
            tiles.mark(TileCoord {
                x: i32::from(t),
                y: 0,
            });
            // 255 reveals: some strokes give tiles back (released).
            let value = [0, 120, 255][v as usize];
            ok(d.modify_mask(pick(d, removed, i), &tiles, |_, tile| {
                *tile.get_mut(u32::from(t), 2) = value;
            }))
        }
        Edit::Clip(i, v) => ok(d.set_clipped(pick(d, removed, i), v)),
        Edit::AlphaLock(i, v) => d.set_alpha_locked(pick(d, removed, i), v),
    }
}

/// A model stack entry: the content on the other side of one step.
#[derive(Clone, Debug)]
struct Entry {
    state: Digest,
    /// Add/remove steps restore the active layer (before for undo, after
    /// for redo); others keep the current selection.
    restores: bool,
    /// Fixed when the step is recorded, like the document does.
    active_before: u32,
    active_after: u32,
}

/// Visibility is outside history: a change applies to every snapshot.
fn set_visible_everywhere(stacks: [&mut Vec<Entry>; 2], id: u32, visible: bool) {
    for stack in stacks {
        for e in stack.iter_mut() {
            for l in e.state.layers.iter_mut().filter(|l| l.id == id) {
                l.visible = visible;
            }
        }
    }
}

fn same_content(mut got: Digest, expected: &Digest) -> bool {
    got.active = expected.active;
    &got == expected
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn history_matches_a_stack_of_snapshots(ops in proptest::collection::vec(hop(), 1..50)) {
        let mut d = Document::new(1024, 256);
        let mut removed = Vec::new();
        let (mut past, mut future): (Vec<Entry>, Vec<Entry>) = (Vec::new(), Vec::new());
        let mut chain: Option<u32> = None;
        for op in &ops {
            let before = d.digest();
            let len_before = d.history_len();
            match op {
                HOp::Undo => {
                    let undone = d.undo();
                    prop_assert_eq!(undone.is_some(), !past.is_empty());
                    if let Some(e) = past.pop() {
                        prop_assert!(same_content(d.digest(), &e.state), "undo content");
                        let want = if e.restores { e.active_before } else { before.active };
                        prop_assert_eq!(d.active_layer().get(), want, "undo active");
                        future.push(Entry { state: before, ..e });
                        chain = None;
                    }
                }
                HOp::Redo => {
                    let redone = d.redo();
                    prop_assert_eq!(redone.is_some(), !future.is_empty());
                    if let Some(e) = future.pop() {
                        prop_assert!(same_content(d.digest(), &e.state), "redo content");
                        let want = if e.restores { e.active_after } else { before.active };
                        prop_assert_eq!(d.active_layer().get(), want, "redo active");
                        past.push(Entry { state: before, ..e });
                        chain = None;
                    }
                }
                HOp::Edit(e) => {
                    let result = edit(&mut d, &mut removed, e);
                    let after = d.digest();
                    let changed = after != before;
                    match (e, result) {
                        (_, Err(_)) => prop_assert_eq!(&after, &before, "refused edits change nothing"),
                        (Edit::Activate(_), Ok(())) => {
                            if after.active != before.active {
                                chain = None;
                            }
                        }
                        (Edit::Show(i, v), Ok(())) => {
                            let id = pick(&d, &removed, *i).get();
                            set_visible_everywhere([&mut past, &mut future], id, *v);
                        }
                        (Edit::Opacity(i, _), Ok(())) if changed => {
                            let id = pick(&d, &removed, *i).get();
                            if chain != Some(id) {
                                past.push(Entry { active_before: before.active, active_after: after.active, state: before, restores: false });
                                future.clear();
                            }
                            chain = Some(id);
                        }
                        // Paint is always a step (one call = one stroke);
                        // the rest only when they changed something.
                        (Edit::Paint(..) | Edit::PaintMask(..), Ok(())) | (_, Ok(())) if changed || matches!(e, Edit::Paint(..) | Edit::PaintMask(..)) => {
                            let restores = matches!(e, Edit::Add(..) | Edit::Remove(_));
                            past.push(Entry { active_before: before.active, active_after: after.active, state: before, restores });
                            future.clear();
                            chain = None;
                        }
                        _ => {}
                    }
                }
            }
            d.check_invariants();
            prop_assert_eq!(d.history_len(), past.len(), "after {:?} (was {})", op, len_before);
            prop_assert_eq!(d.redo_len(), future.len());
            prop_assert!(d.history_bytes() <= d.history_reserved_bytes());
            prop_assert_eq!(d.next_undo().is_some(), !past.is_empty());
        }
    }

    #[test]
    fn history_stays_within_small_limits(ops in proptest::collection::vec(hop(), 1..60), steps in 0usize..4, tiles in 1usize..4) {
        let mut d = Document::new(1024, 256);
        let max_bytes = tiles * 256 * 256 * 4;
        d.set_history_limits(crate::HistoryLimits { max_steps: steps, max_bytes });
        let mut removed = Vec::new();
        for op in &ops {
            match op {
                HOp::Undo => { d.undo(); }
                HOp::Redo => { d.redo(); }
                HOp::Edit(e) => { let _ = edit(&mut d, &mut removed, e); }
            }
            d.check_invariants();
            let kept = d.history_len() + d.redo_len();
            prop_assert!(kept <= steps, "H4: {} steps kept, limit {}", kept, steps);
            prop_assert!(kept <= 1 || d.history_reserved_bytes() <= max_bytes, "H4: bytes");
            prop_assert!(d.history_bytes() <= d.history_reserved_bytes());
        }
    }
}
