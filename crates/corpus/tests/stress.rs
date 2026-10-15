//! Correctness under reasonable stress (phase 4G): many empty layers,
//! many sparse layers, long histories past the step limit, repeated
//! save/load cycles, a fully tiled layer. Sizes stay in what a real
//! session reaches; the point is behaviour, not memory exhaustion.

mod support;

use support::*;
use tela_canvas::{DirtyTiles, IRect, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, Document, HistoryLimits, Placement};

/// Fills the given tiles of the active layer with a pattern (no brush:
/// fast enough for hundreds of layers).
fn fill(d: &mut Document, coords: &[TileCoord], k: u32) {
    let mut t = DirtyTiles::new();
    for &c in coords {
        t.mark(c);
    }
    let l = d.active_layer();
    with_pixel_type!(d.depth(), P => {
        d.modify_tiles(l, &t, |c, tile: &mut Tile<P>| {
            for (i, p) in tile.pixels_mut().iter_mut().enumerate() {
                let a = ((i as u32 * 7 + k * 13 + c.x as u32) % 256) as f32 / 255.0;
                *p = P::from_premul(PremulRgba::new(a * 0.8, a * 0.3, a * 0.5, a));
            }
        })
        .unwrap();
    });
}

#[test]
fn hundreds_of_empty_layers() {
    let mut d = Document::new(1024, 768);
    for _ in 0..299 {
        d.add_layer(Placement::Top).unwrap();
    }
    assert_eq!(d.layer_count(), 300);
    assert!(d.content_tiles().is_empty());
    assert!(
        tiles_of_interest(&d)
            .iter()
            .all(|&c| compose(&d, c, None).is_none())
    );
    check(&d);
    round_trip(&d);
    // 200 of the additions are undoable (the step limit), the rest stay.
    while d.undo().is_some() {}
    assert_eq!(d.layer_count(), 100);
}

#[test]
fn many_sparse_layers_compose_save_and_load() {
    let mut d = Document::with_depth(2048, 2048, PixelDepth::Rgba16);
    let mut r = Rng::new(7);
    for k in 0..120 {
        if k > 0 {
            d.add_layer(Placement::Top).unwrap();
        }
        let coords: Vec<TileCoord> = (0..3)
            .map(|_| TileCoord {
                x: r.below(8) as i32,
                y: r.below(8) as i32,
            })
            .collect();
        fill(&mut d, &coords, k);
        let l = d.active_layer();
        match k % 6 {
            1 => d.set_blend_mode(l, BlendMode::Multiply).unwrap(),
            2 => d.set_clipped(l, true).unwrap(),
            3 => {
                d.add_mask(l).unwrap();
                d.modify_mask(
                    l,
                    &{
                        let mut t = DirtyTiles::new();
                        t.mark(coords[0]);
                        t
                    },
                    |_, t| *t.get_mut(3, 3) = 7,
                )
                .unwrap();
                DirtyTiles::new()
            }
            4 => d.set_opacity(l, 0.5).unwrap(),
            _ => DirtyTiles::new(),
        };
    }
    check(&d);
    let picture = composite_digest(&d);
    let back = round_trip(&d);
    assert_eq!(composite_digest(&back), picture);
    let stored: usize = d.layers().map(|l| l.pixels().tile_count()).sum();
    assert!(
        stored <= 360 && stored > 200,
        "sparse: {stored} tiles for 120 layers"
    );
}

#[test]
fn long_histories_respect_the_step_limit_and_stay_exact() {
    let mut d = Document::new(512, 512);
    let mut snapshots = Vec::new();
    for i in 0..260 {
        snapshots.push(history_digest(&d));
        let y = (i % 40) as f32 * 12.0 + 10.0;
        let p = round(6.0, 0.5);
        stroke(
            &mut d,
            &p,
            colour(i),
            &line((10.0, y), (500.0, 512.0 - y), 8),
            i as u64,
        )
        .unwrap();
        if i % 50 == 0 {
            d.add_layer(Placement::Top).unwrap();
            snapshots.push(history_digest(&d));
        }
        // Mixed with an occasional undo/redo pair (no net change).
        if i % 37 == 0 {
            d.undo().unwrap();
            d.redo().unwrap();
        }
    }
    let limits = HistoryLimits::default();
    assert_eq!(
        d.history_len(),
        limits.max_steps,
        "the oldest steps were dropped"
    );
    assert!(d.history_reserved_bytes() <= limits.max_bytes);
    let end = history_digest(&d);
    let n = d.history_len();
    for _ in 0..n {
        d.undo().unwrap();
    }
    // n steps back: the snapshot taken before the n-th last step.
    assert_eq!(history_digest(&d), snapshots[snapshots.len() - n]);
    assert!(d.undo().is_none());
    for _ in 0..n {
        d.redo().unwrap();
    }
    assert_eq!(history_digest(&d), end);
    check(&d);
}

#[test]
fn repeated_save_and_load_cycles_are_stable() {
    let mut d = Document::new(700, 500);
    for cycle in 0..40u64 {
        let c = cycle as f32;
        let p = round(10.0 + c, 0.7);
        stroke(
            &mut d,
            &p,
            colour(cycle as usize),
            &line((20.0 + c * 15.0, 20.0), (680.0 - c * 10.0, 480.0), 12),
            cycle,
        )
        .unwrap();
        if cycle % 10 == 9 {
            d.add_layer(Placement::Top).unwrap();
        }
        // Continue from the loaded document, as a user reopening it would.
        d = round_trip(&d);
    }
    assert_eq!(d.layer_count(), 5);
    check(&d);
}

#[test]
fn a_fully_tiled_layer_at_both_depths() {
    for depth in PixelDepth::ALL {
        let mut d = Document::with_depth(4096, 4096, depth);
        let all: Vec<TileCoord> = IRect::from_size(4096, 4096).tiles().collect();
        assert_eq!(all.len(), 256);
        fill(&mut d, &all, 3);
        d.add_layer(Placement::Top).unwrap();
        fill(&mut d, &all[..64], 9);
        let l = d.active_layer();
        d.set_blend_mode(l, BlendMode::Screen).unwrap();
        let back = round_trip(&d);
        assert_eq!(back.layers().next().unwrap().pixels().tile_count(), 256);
    }
}
