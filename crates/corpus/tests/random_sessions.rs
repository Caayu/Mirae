//! Seeded random sessions (phase 4G): reproducible sequences of valid
//! operations across every phase-4 feature — real strokes on pixels and
//! masks, layer edits, clipping, alpha lock, blend modes, undo/redo,
//! depth conversion — with checkpoints that validate the invariants and
//! premultiplied values and save/load the document. A failure prints its
//! seed; rerun one with `MIRAE_SEED=<n>`.

mod support;

use support::*;
use tela_color::PixelDepth;
use tela_document::{BlendMode, Document, LayerPart, Placement};

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
    match r.below(20) {
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
            if let Some(done) = stroke(d, &p, c, &path, seed * 1000 + i as u64) {
                assert!(
                    done.preview_vs_commit <= 1,
                    "preview == commit: {} at op {i}, {:?}, preset {} mode {:?}",
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
        18 => {
            d.redo();
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
    let empty = history_digest(&d);
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
    let end = history_digest(&d);
    let picture_end = content_digest(&d);
    let steps = d.history_len();
    while d.undo().is_some() {}
    assert_eq!(history_digest(&d), empty, "seed {seed}: undo all → empty");
    for _ in 0..steps {
        d.redo().expect("redoable");
    }
    assert_eq!(history_digest(&d), end, "seed {seed}: redo all → end");
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
