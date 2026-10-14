//! Property tests of the layer stack: random operation sequences (including
//! stale ids) run against the document and against a deliberately simple
//! reference model; after every step both must agree and the document's
//! invariants (L1–L9) must hold. Lives inside the crate to reach
//! `Document::check_invariants`.

use std::collections::HashMap;

use proptest::prelude::*;
use tela_canvas::{DirtyTiles, TileCoord};

use crate::{Document, LayerError, LayerId, Placement};

#[derive(Clone, Debug)]
enum Op {
    Add(u8, usize),
    Remove(usize),
    Move(usize, u8, usize),
    Activate(usize),
    Show(usize, bool),
    Opacity(usize, f32),
    Lock(usize, bool),
    Rename(usize, u8),
    Paint(usize),
    Clear(usize),
}

fn op() -> impl Strategy<Value = Op> {
    let opacity = prop_oneof![
        8 => -0.5f32..1.5,
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
    ];
    prop_oneof![
        3 => (0u8..4, any::<usize>()).prop_map(|(k, a)| Op::Add(k, a)),
        2 => any::<usize>().prop_map(Op::Remove),
        2 => (any::<usize>(), 0u8..4, any::<usize>()).prop_map(|(i, k, a)| Op::Move(i, k, a)),
        1 => any::<usize>().prop_map(Op::Activate),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Op::Show(i, v)),
        1 => (any::<usize>(), opacity).prop_map(|(i, o)| Op::Opacity(i, o)),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Op::Lock(i, v)),
        1 => (any::<usize>(), 0u8..4).prop_map(|(i, n)| Op::Rename(i, n)),
        2 => any::<usize>().prop_map(Op::Paint),
        1 => any::<usize>().prop_map(Op::Clear),
    ]
}

/// The reference model: the rules written as plainly as possible.
#[derive(Clone, Debug, PartialEq)]
struct ModelLayer {
    id: u32,
    name: String,
    visible: bool,
    opacity: f32,
    locked: bool,
    painted: bool,
}

#[derive(Clone, Debug)]
struct Model {
    layers: Vec<ModelLayer>,
    active: u32,
    next: u32,
}

impl Model {
    fn new() -> Self {
        Self {
            layers: vec![ModelLayer {
                id: 1,
                name: "Layer 1".into(),
                visible: true,
                opacity: 1.0,
                locked: false,
                painted: false,
            }],
            active: 1,
            next: 2,
        }
    }

    fn find(&self, id: LayerId) -> Result<usize, LayerError> {
        self.layers
            .iter()
            .position(|l| l.id == id.get())
            .ok_or(LayerError::NotFound(id))
    }

    fn unlocked(&self, id: LayerId) -> Result<usize, LayerError> {
        let i = self.find(id)?;
        if self.layers[i].locked {
            return Err(LayerError::Locked(id));
        }
        Ok(i)
    }

    fn index_for(&self, kind: u8, anchor: LayerId) -> Result<usize, LayerError> {
        Ok(match kind {
            0 => self.find(anchor)? + 1,
            1 => self.find(anchor)?,
            2 => self.layers.len(),
            _ => 0,
        })
    }

    fn free_name(&self) -> String {
        (1..)
            .map(|n| format!("Layer {n}"))
            .find(|name| self.layers.iter().all(|l| &l.name != name))
            .unwrap()
    }
}

fn placement(kind: u8, anchor: LayerId) -> Placement {
    match kind {
        0 => Placement::Above(anchor),
        1 => Placement::Below(anchor),
        2 => Placement::Top,
        _ => Placement::Bottom,
    }
}

/// Live ids plus every removed id (stale handles a UI may still hold).
fn pick(d: &Document, removed: &[LayerId], i: usize) -> LayerId {
    let mut all: Vec<LayerId> = d.layers().map(|l| l.id()).collect();
    all.extend_from_slice(removed);
    all[i % all.len()]
}

fn origin_tile() -> DirtyTiles {
    let mut t = DirtyTiles::new();
    t.mark(TileCoord { x: 0, y: 0 });
    t
}

/// Drops the success value, keeping the outcome.
fn ok<T>(r: Result<T, LayerError>) -> Result<(), LayerError> {
    r.map(|_| ())
}

/// Applies `op` to both sides and returns both outcomes.
fn step(
    d: &mut Document,
    m: &mut Model,
    removed: &mut Vec<LayerId>,
    op: &Op,
) -> (Result<(), LayerError>, Result<(), LayerError>) {
    match *op {
        Op::Add(kind, a) => {
            let anchor = pick(d, removed, a);
            let model = m.index_for(kind, anchor).map(|i| {
                let id = m.next;
                m.next += 1;
                let name = m.free_name();
                m.layers.insert(
                    i,
                    ModelLayer {
                        id,
                        name,
                        visible: true,
                        opacity: 1.0,
                        locked: false,
                        painted: false,
                    },
                );
                m.active = id;
            });
            (ok(d.add_layer(placement(kind, anchor))), model)
        }
        Op::Remove(i) => {
            let id = pick(d, removed, i);
            let model = m.unlocked(id).and_then(|i| {
                if m.layers.len() == 1 {
                    return Err(LayerError::LastLayer);
                }
                m.layers.remove(i);
                if m.active == id.get() {
                    m.active = m.layers[i.saturating_sub(1)].id;
                }
                Ok(())
            });
            let doc = ok(d.remove_layer(id));
            if doc.is_ok() {
                removed.push(id);
            }
            (doc, model)
        }
        Op::Move(i, kind, a) => {
            let id = pick(d, removed, i);
            let anchor = pick(d, removed, a);
            let model = m.find(id).and_then(|from| {
                if kind < 2 {
                    m.find(anchor)?;
                    if anchor == id {
                        return Ok(());
                    }
                }
                let layer = m.layers.remove(from);
                let to = m.index_for(kind, anchor).unwrap();
                m.layers.insert(to, layer);
                Ok(())
            });
            (ok(d.move_layer(id, placement(kind, anchor))), model)
        }
        Op::Activate(i) => {
            let id = pick(d, removed, i);
            let model = m.find(id).map(|_| m.active = id.get());
            (d.set_active_layer(id), model)
        }
        Op::Show(i, v) => {
            let id = pick(d, removed, i);
            let model = m.find(id).map(|i| m.layers[i].visible = v);
            (ok(d.set_visible(id, v)), model)
        }
        Op::Opacity(i, o) => {
            let id = pick(d, removed, i);
            let model = m.unlocked(id).and_then(|i| {
                if !o.is_finite() {
                    return Err(LayerError::InvalidOpacity);
                }
                m.layers[i].opacity = o.clamp(0.0, 1.0);
                Ok(())
            });
            (ok(d.set_opacity(id, o)), model)
        }
        Op::Lock(i, v) => {
            let id = pick(d, removed, i);
            let model = m.find(id).map(|i| m.layers[i].locked = v);
            (d.set_locked(id, v), model)
        }
        Op::Rename(i, n) => {
            let id = pick(d, removed, i);
            // Includes default-looking names, which reserve their number.
            let name = ["Sketch", "Layer 1", "Layer 2", "Layer 7"][n as usize].to_string();
            let model = m.find(id).map(|i| m.layers[i].name = name.clone());
            (d.set_name(id, name), model)
        }
        Op::Paint(i) => {
            let id = pick(d, removed, i);
            let model = m.unlocked(id).map(|i| m.layers[i].painted = true);
            let doc = d.modify_tiles(id, &origin_tile(), |_, t| *t.get_mut(1, 1) = [255; 4]);
            (ok(doc), model)
        }
        Op::Clear(i) => {
            let id = pick(d, removed, i);
            let model = m.unlocked(id).map(|i| m.layers[i].painted = false);
            (ok(d.clear_layer(id)), model)
        }
    }
}

fn snapshot(d: &Document) -> Vec<ModelLayer> {
    d.layers()
        .map(|l| ModelLayer {
            id: l.id().get(),
            name: l.name().to_string(),
            visible: l.visible(),
            opacity: l.opacity(),
            locked: l.locked(),
            painted: l.pixels().tile_count() > 0,
        })
        .collect()
}

/// Address of each layer's tile buffer, to prove layers are not copied.
fn buffers(d: &Document) -> HashMap<LayerId, *const [u8; 4]> {
    d.layers()
        .filter_map(|l| {
            let t = l.pixels().tile(TileCoord { x: 0, y: 0 })?;
            Some((l.id(), t.pixels().as_ptr()))
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn layer_stack_matches_the_reference_model(ops in proptest::collection::vec(op(), 1..60)) {
        let mut d = Document::new(512, 512);
        let mut m = Model::new();
        let mut removed = Vec::new();
        let mut ever: Vec<u32> = vec![1];
        for op in &ops {
            let before = buffers(&d);
            let (doc, model) = step(&mut d, &mut m, &mut removed, op);
            prop_assert_eq!(doc, model, "outcome of {:?}", op);
            d.check_invariants();
            prop_assert_eq!(snapshot(&d), m.layers.clone(), "after {:?}", op);
            prop_assert_eq!(d.active_layer().get(), m.active);
            // L2: a new id was never used before.
            for l in d.layers() {
                if !ever.contains(&l.id().get()) {
                    prop_assert!(l.id().get() > *ever.iter().max().unwrap());
                    ever.push(l.id().get());
                }
            }
            // L7: tile buffers of surviving layers stay where they were,
            // unless this very operation repainted or cleared them.
            let touched = match *op {
                Op::Paint(i) | Op::Clear(i) => Some(pick(&d, &removed, i)),
                _ => None,
            };
            for (id, ptr) in buffers(&d) {
                if Some(id) != touched && let Some(old) = before.get(&id) {
                    prop_assert_eq!(*old, ptr);
                }
            }
        }
    }
}
