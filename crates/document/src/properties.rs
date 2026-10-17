//! Property tests of the layer stack: random operation sequences (including
//! stale ids) run against the document and against a deliberately simple
//! reference model; after every step both must agree and the document's
//! invariants (L1–L9, M3) must hold. The model includes masks, clipping,
//! alpha lock and the edit target (phase 4D), and the document depth: pixel
//! edits for either depth (refused when it is not the document's) and
//! depth conversion (phase 4E). Lives inside the crate to reach
//! `Document::check_invariants`.

use std::collections::HashMap;

use proptest::prelude::*;
use tela_canvas::{DirtyTiles, Tile, TileCoord};
use tela_color::{PixelDepth, StoragePixel};

use crate::{Document, LayerError, LayerId, LayerPart, Placement};

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
    /// A pixel edit written for 16 bits (`true`) or 8 bits.
    Paint(usize, bool),
    Clear(usize),
    AddMask(usize),
    RemoveMask(usize),
    MaskEnabled(usize, bool),
    PaintMask(usize),
    Clip(usize, bool),
    AlphaLock(usize, bool),
    Target(bool),
    /// Convert to 16 bits (`true`) or 8 bits.
    Convert(bool),
    /// Merge down; `true` keeps an enabled lower mask.
    Merge(usize, bool),
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
        2 => (any::<usize>(), any::<bool>()).prop_map(|(i, s)| Op::Paint(i, s)),
        1 => any::<usize>().prop_map(Op::Clear),
        1 => any::<usize>().prop_map(Op::AddMask),
        1 => any::<usize>().prop_map(Op::RemoveMask),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Op::MaskEnabled(i, v)),
        1 => any::<usize>().prop_map(Op::PaintMask),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Op::Clip(i, v)),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(i, v)| Op::AlphaLock(i, v)),
        1 => any::<bool>().prop_map(Op::Target),
        1 => any::<bool>().prop_map(Op::Convert),
        2 => (any::<usize>(), any::<bool>()).prop_map(|(i, k)| Op::Merge(i, k)),
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
    /// The layer stores a tile (`Paint` allocates the origin tile).
    painted: bool,
    /// The painted pixel is visible: a merge can leave a stored tile
    /// transparent (a lower mask applied over it, opacity 0 baked in).
    ink: bool,
    alpha_locked: bool,
    clipped: bool,
    /// `(enabled, hides something)`.
    mask: Option<(bool, bool)>,
}

impl ModelLayer {
    fn new(id: u32, name: String, clipped: bool) -> Self {
        Self {
            id,
            name,
            visible: true,
            opacity: 1.0,
            locked: false,
            painted: false,
            ink: false,
            alpha_locked: false,
            clipped,
            mask: None,
        }
    }
}

#[derive(Clone, Debug)]
struct Model {
    layers: Vec<ModelLayer>,
    active: u32,
    /// The edit target is the active layer's mask.
    on_mask: bool,
    next: u32,
    depth: PixelDepth,
}

impl Model {
    fn new() -> Self {
        Self {
            layers: vec![ModelLayer::new(1, "Layer 1".into(), false)],
            active: 1,
            on_mask: false,
            next: 2,
            depth: PixelDepth::Rgba8,
        }
    }

    fn with_mask(&self, i: usize, id: LayerId) -> Result<usize, LayerError> {
        self.layers[i].mask.ok_or(LayerError::NoMask(id))?;
        Ok(i)
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

/// Paints one opaque pixel with an edit written for `P`.
fn paint<P: StoragePixel>(d: &mut Document, id: LayerId) -> Result<(), LayerError> {
    ok(d.modify_tiles(id, &origin_tile(), |_, t: &mut Tile<P>| {
        *t.get_mut(1, 1) = P::from_rgba8([255; 4]);
    }))
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
                // Clipped when the layer above is (reference C4, C5).
                let clipped = m.layers.get(i).is_some_and(|l| l.clipped);
                m.layers.insert(i, ModelLayer::new(id, name, clipped));
                m.active = id;
                m.on_mask = false;
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
                    m.on_mask = false;
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
            let model = m.find(id).map(|_| {
                m.active = id.get();
                m.on_mask = false;
            });
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
        Op::Paint(i, sixteen) => {
            let id = pick(d, removed, i);
            let edit = if sixteen {
                PixelDepth::Rgba16
            } else {
                PixelDepth::Rgba8
            };
            let model = m.unlocked(id).and_then(|i| {
                if edit != m.depth {
                    return Err(LayerError::WrongDepth {
                        document: m.depth,
                        edit,
                    });
                }
                m.layers[i].painted = true;
                m.layers[i].ink = true;
                Ok(())
            });
            let doc = if sixteen {
                paint::<[u16; 4]>(d, id)
            } else {
                paint::<[u8; 4]>(d, id)
            };
            (doc, model)
        }
        Op::Convert(sixteen) => {
            let depth = if sixteen {
                PixelDepth::Rgba16
            } else {
                PixelDepth::Rgba8
            };
            m.depth = depth;
            d.convert_depth(depth);
            (Ok(()), Ok(()))
        }
        Op::Clear(i) => {
            let id = pick(d, removed, i);
            let model = m.unlocked(id).map(|i| {
                m.layers[i].painted = false;
                m.layers[i].ink = false;
            });
            (ok(d.clear_layer(id)), model)
        }
        Op::AddMask(i) => {
            let id = pick(d, removed, i);
            let model = m.unlocked(id).and_then(|i| {
                if m.layers[i].mask.is_some() {
                    return Err(LayerError::HasMask(id));
                }
                m.layers[i].mask = Some((true, false));
                if m.active == id.get() {
                    m.on_mask = true;
                }
                Ok(())
            });
            (ok(d.add_mask(id)), model)
        }
        Op::RemoveMask(i) => {
            let id = pick(d, removed, i);
            let model = m.unlocked(id).and_then(|i| m.with_mask(i, id)).map(|i| {
                m.layers[i].mask = None;
                if m.active == id.get() {
                    m.on_mask = false;
                }
            });
            (ok(d.remove_mask(id)), model)
        }
        Op::MaskEnabled(i, v) => {
            let id = pick(d, removed, i);
            // Allowed on locked layers (reference A6).
            let model = m
                .find(id)
                .and_then(|i| m.with_mask(i, id))
                .map(|i| m.layers[i].mask.as_mut().unwrap().0 = v);
            (ok(d.set_mask_enabled(id, v)), model)
        }
        Op::PaintMask(i) => {
            let id = pick(d, removed, i);
            let model = m
                .unlocked(id)
                .and_then(|i| m.with_mask(i, id))
                .map(|i| m.layers[i].mask.as_mut().unwrap().1 = true);
            let doc = d.modify_mask(id, &origin_tile(), |_, t| *t.get_mut(1, 1) = 0);
            (ok(doc), model)
        }
        Op::Clip(i, v) => {
            let id = pick(d, removed, i);
            let model = m.unlocked(id).map(|i| m.layers[i].clipped = v);
            (ok(d.set_clipped(id, v)), model)
        }
        Op::AlphaLock(i, v) => {
            let id = pick(d, removed, i);
            let model = m.unlocked(id).map(|i| m.layers[i].alpha_locked = v);
            (d.set_alpha_locked(id, v), model)
        }
        Op::Merge(i, keep) => {
            let id = pick(d, removed, i);
            let model = m.find(id).and_then(|ti| {
                let bi = ti.checked_sub(1).ok_or(LayerError::NoLayerBelow(id))?;
                for l in [&m.layers[ti], &m.layers[bi]] {
                    let lid = d.find_layer(l.id).unwrap();
                    if l.locked {
                        return Err(LayerError::Locked(lid));
                    }
                    if !l.visible {
                        return Err(LayerError::Hidden(lid));
                    }
                }
                // An opacity survives quantization at the document depth.
                let levels = match m.depth {
                    PixelDepth::Rgba8 => 255.0,
                    PixelDepth::Rgba16 => 65535.0,
                };
                let shows = |opacity: f32| opacity * levels > 0.5;
                let top = m.layers.remove(ti);
                let bottom = &mut m.layers[bi];
                // The lower layer's own pixel: its opacity is baked in and an
                // applied mask (`PaintMask` hides exactly the painted pixel)
                // erases it; its tile stays either way.
                let applied = !keep && bottom.mask.is_some_and(|(enabled, _)| enabled);
                bottom.ink &=
                    shows(bottom.opacity) && !(applied && bottom.mask == Some((true, true)));
                // The upper pixel lands unless it is invisible, its opacity
                // rounds to 0, its mask hides it, or it is clipped to a base
                // that is empty there.
                let lands = top.ink
                    && shows(top.opacity)
                    && top.mask != Some((true, true))
                    && !(top.clipped && !bottom.clipped && !bottom.ink);
                bottom.painted |= lands;
                bottom.ink |= lands;
                bottom.opacity = 1.0;
                if applied {
                    bottom.mask = None;
                }
                m.active = bottom.id;
                m.on_mask = false;
                Ok(())
            });
            let mask = if keep {
                crate::LowerMask::Keep
            } else {
                crate::LowerMask::Apply
            };
            let doc = ok(d.merge_down(id, mask));
            if doc.is_ok() {
                removed.push(id);
            }
            (doc, model)
        }
        Op::Target(mask) => {
            let active = d.active_layer();
            let model = if !mask {
                m.on_mask = false;
                Ok(())
            } else {
                let i = m.find(active).unwrap();
                m.with_mask(i, active).map(|_| m.on_mask = true)
            };
            let part = if mask {
                LayerPart::Mask
            } else {
                LayerPart::Pixels
            };
            (d.set_active_part(part), model)
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
            ink: origin_alpha(l) > 0,
            alpha_locked: l.alpha_locked(),
            clipped: l.clipped(),
            mask: l.mask().map(|m| (m.enabled(), m.tile_count() > 0)),
        })
        .collect()
}

/// Alpha of the pixel `Paint` paints (0 when its tile is absent).
fn origin_alpha(l: &crate::RasterLayer) -> u16 {
    let origin = TileCoord { x: 0, y: 0 };
    let p = l.pixels();
    match (p.rgba8(), p.rgba16()) {
        (Some(s), _) => s.tile(origin).map_or(0, |t| u16::from(t.get(1, 1)[3])),
        (_, Some(s)) => s.tile(origin).map_or(0, |t| t.get(1, 1)[3]),
        _ => unreachable!("one of the two depths"),
    }
}

/// Address of each layer's tile buffer, to prove layers are not copied.
fn buffers(d: &Document) -> HashMap<LayerId, *const u8> {
    let origin = TileCoord { x: 0, y: 0 };
    d.layers()
        .filter_map(|l| {
            let p = l.pixels();
            let ptr = match (p.rgba8(), p.rgba16()) {
                (Some(s), _) => s.tile(origin)?.pixels().as_ptr().cast(),
                (_, Some(s)) => s.tile(origin)?.pixels().as_ptr().cast(),
                _ => unreachable!("one of the two depths"),
            };
            Some((l.id(), ptr))
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
            prop_assert_eq!(d.active_part() == LayerPart::Mask, m.on_mask);
            prop_assert_eq!(d.depth(), m.depth);
            // L2: a new id was never used before.
            for l in d.layers() {
                if !ever.contains(&l.id().get()) {
                    prop_assert!(l.id().get() > *ever.iter().max().unwrap());
                    ever.push(l.id().get());
                }
            }
            // L7: tile buffers of surviving layers stay where they were,
            // unless this very operation repainted, cleared or converted them.
            if let Op::Convert(_) = op {
                continue;
            }
            let touched = match *op {
                Op::Paint(i, _) | Op::Clear(i) => Some(pick(&d, &removed, i)),
                Op::Merge(..) => Some(d.active_layer()),
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
