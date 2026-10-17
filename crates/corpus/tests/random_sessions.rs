//! Seeded random sessions (phase 4G): reproducible sequences of valid
//! operations across every phase-4 feature — real strokes on pixels and
//! masks, layer edits, clipping, alpha lock, blend modes, undo/redo,
//! depth conversion, selections (5C), transforms (5D), fills (5E), gradients (5F) — with checkpoints that validate the invariants and
//! premultiplied values and save/load the document. A failure prints its
//! seed; rerun one with `MIRAE_SEED=<n>`.

mod support;

use support::*;
use tela_color::PixelDepth;
use tela_document::{
    BlendMode, ConnectedFill, Document, FillPaint, FillReference, FillTarget, Gradient,
    GradientExtend, GradientGeometry, GradientPaint, GradientShape, GradientStop, GradientStops,
    Interpolation, LayerPart, Mixing, Placement, SelectionOp, SelectionShape, Tolerance, Transform,
    TransformParams,
};

const OPS: usize = 60;
const CHECK_EVERY: usize = 15;

fn random_path(r: &mut Rng) -> Vec<(f32, f32, f32)> {
    let a = (r.unit() * 560.0 - 20.0, r.unit() * 400.0 - 20.0);
    let b = (r.unit() * 560.0 - 20.0, r.unit() * 400.0 - 20.0);
    line(a, b, 8 + r.below(30))
}

fn random_preset(r: &mut Rng) -> tela_brush::BrushPreset {
    match r.below(10) {
        0 => eraser(10.0 + r.unit() * 80.0),
        1 => airbrush(),
        2 => preset(tela_brush::preset::builtin::ids::SOFT_ROUND),
        _ => round(4.0 + r.unit() * 120.0, 0.05 + r.unit() * 0.95),
    }
}

fn pick(d: &Document, r: &mut Rng) -> tela_document::LayerId {
    d.layers().nth(r.below(d.layer_count())).unwrap().id()
}

/// One random operation; refused ones (locked layers, …) are fine: they
/// must change nothing, which the checkpoints and the final walk verify.
fn op(d: &mut Document, r: &mut Rng, seed: u64, i: usize) {
    match r.below(28) {
        0..=5 => {
            let p = random_preset(r);
            let path = random_path(r);
            let c = colour(r.below(6));
            let layer = d.layer(d.active_layer()).map(|l| {
                format!(
                    "{:?} {:?} clipped {} alpha lock {} opacity {} mask {:?} depth {:?} layers {}",
                    l.blend_mode(),
                    d.active_part(),
                    l.clipped(),
                    l.alpha_locked(),
                    l.opacity(),
                    l.mask().map(|m| m.enabled()),
                    d.depth(),
                    d.layer_count()
                )
            });
            let stack: Vec<String> = d
                .layers()
                .map(|l| {
                    format!(
                        "{}:{:?}/{}/{}{}",
                        l.id(),
                        l.blend_mode(),
                        l.opacity(),
                        if l.visible() { "shown" } else { "hidden" },
                        if l.clipped() { "/clipped" } else { "" }
                    )
                })
                .collect();
            if let Some(done) = stroke(d, &p, c, &path, seed * 1000 + i as u64) {
                assert!(
                    done.preview_vs_commit <= 1,
                    "preview == commit: {} at op {i}, {:?}, preset {} mode {:?}, stack {stack:?}",
                    done.preview_vs_commit,
                    layer,
                    p.name,
                    p.compositing.mode
                );
            }
        }
        6 => {
            let at = match r.below(3) {
                0 => Placement::Top,
                1 => Placement::Bottom,
                _ => Placement::Above(d.active_layer()),
            };
            let _ = d.add_layer(at);
        }
        7 => {
            let _ = d.remove_layer(pick(d, r));
        }
        8 => {
            let (l, other) = (pick(d, r), pick(d, r));
            let _ = d.move_layer(
                l,
                if r.below(2) == 0 {
                    Placement::Above(other)
                } else {
                    Placement::Below(other)
                },
            );
        }
        9 => {
            let _ = d.set_opacity(pick(d, r), r.unit());
        }
        10 => {
            let mode = BlendMode::ALL[r.below(BlendMode::ALL.len())];
            let _ = d.set_blend_mode(pick(d, r), mode);
        }
        11 => {
            let l = d.active_layer();
            if d.layer(l).unwrap().mask().is_none() {
                let _ = d.add_mask(l);
            } else if r.below(4) == 0 {
                let _ = d.remove_mask(l);
            } else {
                let on = !d.layer(l).unwrap().mask().unwrap().enabled();
                let _ = d.set_mask_enabled(l, on);
            }
        }
        12 => {
            let l = pick(d, r);
            let _ = d.set_clipped(l, !d.layer(l).unwrap().clipped());
        }
        13 => {
            let l = pick(d, r);
            let _ = d.set_alpha_locked(l, !d.layer(l).unwrap().alpha_locked());
        }
        14 => {
            // Rare, and undone soon after most of the time.
            let l = pick(d, r);
            let _ = d.set_locked(l, !d.layer(l).unwrap().locked());
        }
        15 => {
            let l = pick(d, r);
            let _ = d.set_active_layer(l);
            if r.below(2) == 0 {
                let _ = d.set_active_part(LayerPart::Mask);
            }
        }
        16 | 17 => {
            d.undo();
        }
        // Selections (5C): shapes combined by every operator, invert,
        // feather, deselect; later strokes are limited by them.
        20 | 21 => {
            let (x, y) = (r.unit() * 600.0 - 40.0, r.unit() * 440.0 - 30.0);
            let (w, h) = (20.0 + r.unit() * 300.0, 20.0 + r.unit() * 250.0);
            let shape = match r.below(3) {
                0 => SelectionShape::Rect([x, y], [x + w, y + h]),
                1 => SelectionShape::Ellipse {
                    center: [x, y],
                    radii: [w * 0.5, h * 0.5],
                    rotation: r.unit() * 3.0,
                },
                _ => SelectionShape::Polygon(vec![
                    [x, y],
                    [x + w, y + h * 0.3],
                    [x + w * 0.2, y + h],
                ]),
            };
            let op = [
                SelectionOp::Replace,
                SelectionOp::Add,
                SelectionOp::Subtract,
                SelectionOp::Intersect,
            ][r.below(4)];
            d.select(&shape, r.below(2) == 0, op);
        }
        22 => match r.below(4) {
            0 => drop(d.invert_selection()),
            1 => drop(d.deselect()),
            2 => drop(d.select_all()),
            _ => drop(d.feather_selection(1.0 + r.unit() * 20.0)),
        },
        18 => {
            d.redo();
        }
        // Transforms (5D) of the active layer or its selected part: moves
        // by whole and fractional pixels, flips, scales, rotations, both
        // interpolations; or a session cancelled (nothing changes).
        19 => {
            let Ok(source) = d.transform_source(d.active_layer()) else {
                return;
            };
            let before = content_digest(d);
            let mut p = TransformParams::identity(source.centre());
            match r.below(6) {
                0 => p.translate = [r.below(80) as f64 - 40.0, r.below(60) as f64 - 30.0],
                1 => p.translate = [r.unit() as f64 * 30.0, r.unit() as f64 * -20.0],
                2 => p.scale[r.below(2)] = -1.0,
                3 => p.scale = [0.2 + r.unit() as f64 * 2.0, 0.2 + r.unit() as f64 * 2.0],
                4 => p.rotation = r.unit() as f64 * 6.3,
                _ => {
                    drop(source);
                    assert_eq!(
                        content_digest(d),
                        before,
                        "a cancelled transform changes nothing"
                    );
                    return;
                }
            }
            let interpolation = if r.below(4) == 0 {
                Interpolation::Nearest
            } else {
                Interpolation::Bicubic
            };
            let t = Transform {
                params: p,
                interpolation,
            };
            let steps = d.history_len();
            match d.apply_transform(&source, &t) {
                Ok(dirty) => assert!(
                    d.history_len() <= steps + 1
                        && (dirty.is_empty() || d.history_len() == steps + 1),
                    "one step at most (seed {seed}, op {i})"
                ),
                Err(e) => panic!("a fresh source applies: {e} (seed {seed}, op {i})"),
            }
        }
        // Fills (5E) of the edit target: a bucket from a random seed on the
        // target or the composite, any tolerance, anti-aliased or not, a
        // colour or erasing; or Edit › Fill. Refusals (hidden, locked)
        // change nothing; a fill is one step at most.
        24 | 25 => {
            let target = FillTarget {
                layer: d.active_layer(),
                part: d.active_part(),
            };
            let paint = if r.below(5) == 0 {
                FillPaint::Erase
            } else {
                FillPaint::Color(tela_color::Color::new(r.unit(), r.unit(), r.unit()))
            };
            let steps = d.history_len();
            let result = if r.below(4) == 0 {
                d.fill_selection(target, paint)
            } else {
                let fill = ConnectedFill {
                    seed: (r.below(560) as i32 - 20, r.below(420) as i32 - 20),
                    reference: if r.below(2) == 0 {
                        FillReference::Target
                    } else {
                        FillReference::Composite
                    },
                    tolerance: Tolerance::from_tenths(r.below(1001) as u16),
                    anti_alias: r.below(2) == 0,
                };
                d.fill_connected(target, &fill, paint)
            };
            if let Ok(dirty) = result {
                assert!(
                    d.history_len() <= steps + 1
                        && (dirty.is_empty() || d.history_len() == steps + 1),
                    "one step at most (seed {seed}, op {i})"
                );
            }
        }
        // Gradients (5F) into the edit target: any shape, ends, mixing,
        // dithering, one to four stops (some transparent), drawn or
        // erasing; refusals change nothing; one step at most.
        26 | 27 => {
            let target = FillTarget {
                layer: d.active_layer(),
                part: d.active_part(),
            };
            let shapes = [
                GradientShape::Linear,
                GradientShape::Radial,
                GradientShape::Elliptical,
                GradientShape::Angular,
                GradientShape::Reflected,
            ];
            let extends = [
                GradientExtend::Clamp,
                GradientExtend::Repeat,
                GradientExtend::Reflect,
                GradientExtend::Empty,
            ];
            let mixings = [Mixing::Standard, Mixing::Linear, Mixing::Perceptual];
            let n = 1 + r.below(4);
            let stops = GradientStops::new((0..n).map(|_| {
                GradientStop::new(
                    r.unit(),
                    tela_color::Color::new(r.unit(), r.unit(), r.unit()),
                    if r.below(4) == 0 { 0.0 } else { r.unit() },
                )
            }))
            .expect("at least one stop");
            let point = |r: &mut Rng| {
                [
                    r.unit() as f64 * 560.0 - 20.0,
                    r.unit() as f64 * 420.0 - 20.0,
                ]
            };
            let geometry = GradientGeometry {
                shape: shapes[r.below(shapes.len())],
                start: point(r),
                end: point(r),
            };
            let g = Gradient::new(
                geometry,
                &stops,
                mixings[r.below(mixings.len())],
                extends[r.below(extends.len())],
                r.below(2) == 0,
            );
            let paint = if r.below(5) == 0 {
                GradientPaint::Erase
            } else {
                GradientPaint::Draw
            };
            let steps = d.history_len();
            if let Ok(dirty) = d.apply_gradient(target, &g, paint) {
                assert!(
                    d.history_len() <= steps + 1
                        && (dirty.is_empty() || d.history_len() == steps + 1),
                    "one step at most (seed {seed}, op {i})"
                );
            }
        }
        _ => {
            if r.below(3) == 0 {
                let to = match d.depth() {
                    PixelDepth::Rgba8 => PixelDepth::Rgba16,
                    PixelDepth::Rgba16 => PixelDepth::Rgba8,
                };
                d.convert_depth(to);
            }
        }
    }
}

fn session(seed: u64) {
    let mut r = Rng::new(seed);
    let depth = if seed.is_multiple_of(2) {
        PixelDepth::Rgba8
    } else {
        PixelDepth::Rgba16
    };
    let mut d = Document::with_depth(520, 380, depth);
    let empty = (history_digest(&d), selection_digest(&d));
    for i in 0..OPS {
        op(&mut d, &mut r, seed, i);
        if (i + 1) % CHECK_EVERY == 0 {
            check(&d);
            round_trip(&d);
        }
    }
    // Walk the whole history back to the empty document and forward again.
    // Visibility is not recorded, so the walk compares everything else.
    // The session may end with redoable steps: redo exactly what is undone.
    let end = (history_digest(&d), selection_digest(&d));
    let picture_end = content_digest(&d);
    let steps = d.history_len();
    while d.undo().is_some() {}
    assert_eq!(
        (history_digest(&d), selection_digest(&d)),
        empty,
        "seed {seed}: undo all → empty"
    );
    for _ in 0..steps {
        d.redo().expect("redoable");
    }
    assert_eq!(
        (history_digest(&d), selection_digest(&d)),
        end,
        "seed {seed}: redo all → end"
    );
    assert_eq!(content_digest(&d), picture_end);
    check(&d);
}

#[test]
fn seeded_sessions_keep_every_invariant() {
    let seeds: Vec<u64> = match std::env::var("MIRAE_SEED") {
        Ok(s) => vec![s.parse().expect("MIRAE_SEED=<number>")],
        Err(_) => (1..=16).collect(),
    };
    for seed in seeds {
        let result = std::panic::catch_unwind(|| session(seed));
        assert!(
            result.is_ok(),
            "session failed for seed {seed} (rerun with MIRAE_SEED={seed})"
        );
    }
}
