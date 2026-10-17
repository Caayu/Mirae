//! Property test of selections: random sequences of shapes combined by
//! every operator, select all, invert, deselect, feather, undo and redo,
//! checked after every step against a model that is a plain array of
//! coverages (`None` = no selection) and a stack of its past states. The
//! operators are the measured 8-bit rules (parity record 5C, S14–S18); the
//! feather is checked against a direct 2D evaluation of the measured
//! kernel (S19–S21).

use proptest::prelude::*;
use tela_canvas::{IRect, TileCoverage};

use crate::history_properties::selection_op;
use crate::selection::rasterize;
use crate::{Document, SelectionOp};

/// Spans 2 × 2 tiles with partial border tiles.
const W: i32 = 300;
const H: i32 = 280;

#[derive(Clone, Debug)]
enum Op {
    Shape(u8, u8, u8, bool),
    SelectAll,
    Deselect,
    Invert,
    Feather(u8),
    Undo,
    Redo,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => (0u8..3, 0u8..4, any::<u8>(), any::<bool>()).prop_map(|(k, o, p, a)| Op::Shape(k, o, p, a)),
        1 => Just(Op::SelectAll),
        1 => Just(Op::Deselect),
        2 => Just(Op::Invert),
        4 => (1u8..6).prop_map(Op::Feather),
        2 => Just(Op::Undo),
        2 => Just(Op::Redo),
    ]
}

/// A shape placed anywhere on (and partly off) the 300 × 280 canvas.
fn selection_shape(kind: u8, p: u8) -> crate::SelectionShape {
    let x = f32::from(p % 32) * 9.3 - 30.0;
    let y = f32::from(p / 32) * 36.1 - 30.0;
    match kind {
        0 => crate::SelectionShape::Rect([x, y], [x + 120.3, y + 90.6]),
        1 => crate::SelectionShape::Ellipse {
            center: [x + 40.0, y + 40.0],
            radii: [50.0 + f32::from(p % 7) * 9.0, 35.0],
            rotation: f32::from(p) * 0.1,
        },
        _ => crate::SelectionShape::Polygon(vec![
            [x, y],
            [x + 150.0, y + 30.0],
            [x + 40.0, y + 140.0],
        ]),
    }
}

type Model = Option<Vec<u8>>;

fn index(x: i32, y: i32) -> usize {
    (y * W + x) as usize
}

/// The document's coverage, read pixel by pixel.
fn read(d: &Document) -> Model {
    let s = d.selection()?;
    Some(
        (0..H)
            .flat_map(|y| (0..W).map(move |x| (x, y)))
            .map(|(x, y)| s.coverage(x, y))
            .collect(),
    )
}

/// The measured kernel evaluated directly in 2D (no separability, no
/// shortcuts), renormalized over the canvas.
fn naive_feather(c: &[u8], strength: f32) -> Vec<u8> {
    let sigma = 0.3 * strength;
    let r = (3.0 * sigma).ceil().max(1.0) as i32;
    let w = |k: i32| (-((k * k) as f32) / (2.0 * sigma * sigma)).exp();
    let mut out = vec![0u8; c.len()];
    for y in 0..H {
        for x in 0..W {
            let (mut sum, mut norm) = (0.0f64, 0.0f64);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (sx, sy) = (x + dx, y + dy);
                    if (0..W).contains(&sx) && (0..H).contains(&sy) {
                        let wt = f64::from(w(dx) * w(dy));
                        sum += wt * f64::from(c[index(sx, sy)]);
                        norm += wt;
                    }
                }
            }
            out[index(x, y)] = (sum / norm).round() as u8;
        }
    }
    out
}

/// The measured rules, written independently of `SelectionOp::combine`.
fn rule(op: SelectionOp, a: u8, b: u8) -> u8 {
    let (a, b) = (i32::from(a), i32::from(b));
    (match op {
        SelectionOp::Replace => b,
        SelectionOp::Add => (a + b).min(255),
        SelectionOp::Subtract => (a - b).max(0),
        SelectionOp::Intersect => a * b / 255,
    }) as u8
}

fn empty_to_none(m: Vec<u8>) -> Model {
    m.iter().any(|&v| v > 0).then_some(m)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn selections_match_a_plain_coverage_model(ops in proptest::collection::vec(op(), 1..24)) {
        let canvas = IRect::new(0, 0, W, H);
        let mut d = Document::new(W as u32, H as u32);
        let mut model: Model = None;
        let (mut past, mut future): (Vec<Model>, Vec<Model>) = (Vec::new(), Vec::new());
        for op in &ops {
            let before = model.clone();
            let steps = d.history_len();
            match *op {
                Op::Shape(kind, o, p, aa) => {
                    let shape = rasterize(&selection_shape(kind, p), aa, canvas);
                    let s = |x: i32, y: i32| shape.pixel(x, y);
                    let operator = selection_op(o);
                    let new = match (&model, operator) {
                        (_, SelectionOp::Replace) | (None, SelectionOp::Add) => {
                            Some((0..H).flat_map(|y| (0..W).map(move |x| (x, y))).map(|(x, y)| s(x, y)).collect())
                        }
                        (None, _) => None,
                        (Some(m), _) => Some(
                            (0..H)
                                .flat_map(|y| (0..W).map(move |x| (x, y)))
                                .map(|(x, y)| rule(operator, m[index(x, y)], s(x, y)))
                                .collect(),
                        ),
                    };
                    model = new.and_then(empty_to_none);
                    d.select(&selection_shape(kind, p), aa, operator);
                }
                Op::SelectAll => {
                    model = Some(vec![255; (W * H) as usize]);
                    d.select_all();
                }
                Op::Deselect => {
                    model = None;
                    d.deselect();
                }
                Op::Invert => {
                    if let Some(m) = &model {
                        model = empty_to_none(m.iter().map(|v| 255 - v).collect());
                    }
                    d.invert_selection();
                }
                Op::Feather(px) => {
                    let changed = d.feather_selection(f32::from(px));
                    if let Some(m) = &model {
                        let expected = naive_feather(m, f32::from(px));
                        match read(&d) {
                            Some(got) => {
                                for (g, e) in got.iter().zip(&expected) {
                                    prop_assert!(g.abs_diff(*e) <= 1, "feather {}: {} vs {}", px, g, e);
                                }
                                model = Some(got);
                            }
                            None => {
                                prop_assert!(expected.iter().all(|&v| v <= 1));
                                model = None;
                            }
                        }
                    }
                    prop_assert_eq!(changed, model != before);
                }
                Op::Undo => {
                    if d.undo().is_some() {
                        future.push(model.take());
                        model = past.pop().expect("model has the step");
                    }
                    prop_assert_eq!(read(&d), model.clone(), "undo");
                    d.check_invariants();
                    continue;
                }
                Op::Redo => {
                    if d.redo().is_some() {
                        past.push(model.take());
                        model = future.pop().expect("model has the step");
                    }
                    prop_assert_eq!(read(&d), model.clone(), "redo");
                    d.check_invariants();
                    continue;
                }
            }
            // A change is exactly one step; no change records nothing.
            if model != before {
                prop_assert_eq!(d.history_len(), steps + 1, "{:?}", op);
                past.push(before);
                future.clear();
            } else {
                prop_assert_eq!(d.history_len(), steps, "no-op {:?}", op);
            }
            d.check_invariants();
            prop_assert_eq!(read(&d), model.clone(), "after {:?}", op);
            if let Some(s) = d.selection() {
                // SEL7: bounds hold every covered pixel and are tight.
                let b = s.bounds();
                let m = model.as_ref().unwrap();
                let mut tight = IRect::default();
                for y in 0..H {
                    for x in 0..W {
                        if m[index(x, y)] > 0 {
                            tight = tight.union(&IRect::new(x, y, x + 1, y + 1));
                        }
                    }
                }
                prop_assert_eq!(b, tight);
                // The per-tile view agrees with the per-pixel one.
                for c in canvas.tiles() {
                    let t = s.tile(c);
                    let (ox, oy) = c.origin();
                    for (lx, ly) in [(0u32, 0u32), (17, 200), (255, 255), (128, 23)] {
                        let (x, y) = (ox + lx as i32, oy + ly as i32);
                        prop_assert_eq!(t.get(lx, ly), s.coverage(x, y));
                    }
                    if matches!(t, TileCoverage::Full) {
                        prop_assert!(c.rect().intersect(&canvas) == c.rect());
                    }
                }
            }
            prop_assert!(d.history_bytes() <= d.history_reserved_bytes());
        }
    }

    /// Algebraic properties of the measured operators on whole selections.
    #[test]
    fn operators_keep_their_identities(kind in 0u8..3, p in any::<u8>(), aa in any::<bool>()) {
        let shape = selection_shape(kind, p);
        let mut d = Document::new(W as u32, H as u32);
        if !d.select(&shape, aa, SelectionOp::Replace) {
            return Ok(()); // the shape fell off the canvas
        }
        let a = read(&d);
        // Replace(A, A) = A, recorded as nothing.
        let steps = d.history_len();
        prop_assert!(!d.select(&shape, aa, SelectionOp::Replace));
        // Intersect with a full shape and subtract nothing keep A.
        let full = crate::SelectionShape::Rect([-10.0, -10.0], [W as f32 + 10.0, H as f32 + 10.0]);
        prop_assert!(!d.select(&full, false, SelectionOp::Intersect));
        prop_assert!(!d.select(&crate::SelectionShape::Rect([5.0, 5.0], [5.0, 50.0]), false, SelectionOp::Subtract));
        prop_assert_eq!(d.history_len(), steps);
        // invert ∘ invert = identity.
        d.invert_selection();
        d.invert_selection();
        prop_assert_eq!(read(&d), a.clone());
        // Intersect with nothing leaves no selection; undo brings A back.
        prop_assert!(d.select(&crate::SelectionShape::Rect([-50.0, -50.0], [-1.0, -1.0]), false, SelectionOp::Intersect)
            || a.is_none());
        prop_assert!(!d.has_selection());
        d.undo();
        prop_assert_eq!(read(&d), a.clone());
        // Add is idempotent on hard selections (2a saturates at 255 = a
        // only where a is 0 or 255).
        if !aa {
            prop_assert!(!d.select(&shape, false, SelectionOp::Add));
        }
        // Select all covers the canvas and nothing else; deselect is the
        // neutral state.
        d.select_all();
        prop_assert!(read(&d).unwrap().iter().all(|&v| v == 255));
        prop_assert_eq!(d.selection().unwrap().coverage(-1, 0), 0);
        d.deselect();
        prop_assert!(!d.has_selection());
        prop_assert_eq!(d.selection_coverage(3, 3), 255);
    }
}
