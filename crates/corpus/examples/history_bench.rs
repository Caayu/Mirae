//! Cost of undo history for real strokes (phase 4B).
//!
//! For each stroke size, on an empty layer and over a fully painted layer:
//! committing the stroke without history (`StrokeBuffer::commit_into` on a
//! bare surface: the pre-4B path), committing it through
//! `Document::modify_tiles` (records the step: copies the touched tiles'
//! previous content), undo, redo, and the bytes the history holds/reserves.
//! Medians of several runs; each run starts from a fresh copy (copying is
//! outside the timings).
//!
//! `cargo run --release -p tela-corpus --example history_bench`

use std::time::Instant;

use glam::Vec2;
use tela_brush::{BrushEngine, BrushInput, BrushPreset, PixelAlpha, StrokeBuffer};
use tela_canvas::{DirtyTiles, IRect, TiledSurface};
use tela_document::Document;

const DOC: u32 = 4096;
const RUNS: usize = 9;

/// A straight stroke of `length` px with a round brush of `size` px.
fn stroke(size: f32, from: Vec2, to: Vec2) -> StrokeBuffer {
    let mut engine = BrushEngine::new(BrushPreset::plain(size));
    let steps = (from.distance(to) / 2.0).ceil() as usize;
    engine.begin_stroke(BrushInput::from_position(from, 0.0), 1.0, 7);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let input = BrushInput::from_position(from.lerp(to, t), f64::from(t));
        engine.add_input_at(input, f64::from(t));
    }
    engine.end_stroke().expect("a stroke was begun")
}

fn painted_document(painted: bool) -> Document {
    let mut d = Document::new(DOC, DOC);
    if painted {
        let mut all = DirtyTiles::new();
        all.mark_rect(IRect::from_size(DOC, DOC));
        let l = d.active_layer();
        d.modify_tiles(l, &all, |_, t| t.pixels_mut().fill([90, 60, 30, 255]))
            .unwrap();
        // The fill itself is a step; start every measurement with an empty
        // history so only the stroke is counted.
        d.set_history_limits(tela_document::HistoryLimits {
            max_steps: 0,
            ..Default::default()
        });
        d.set_history_limits(tela_document::HistoryLimits::default());
    }
    d
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn ms(f: impl FnOnce()) -> f64 {
    let t = Instant::now();
    f();
    t.elapsed().as_secs_f64() * 1e3
}

fn main() {
    let cases = [
        (
            "small",
            5.0,
            Vec2::new(100.0, 100.0),
            Vec2::new(300.0, 120.0),
        ),
        (
            "medium",
            25.0,
            Vec2::new(100.0, 100.0),
            Vec2::new(1100.0, 400.0),
        ),
        (
            "large",
            100.0,
            Vec2::new(100.0, 100.0),
            Vec2::new(2100.0, 1600.0),
        ),
        (
            "huge brush",
            500.0,
            Vec2::new(300.0, 300.0),
            Vec2::new(3800.0, 3800.0),
        ),
    ];
    println!(
        "| stroke | size px | layer | tiles | commit, no history ms | commit + history ms | undo ms | redo ms | held after commit | held after undo | reserved |"
    );
    println!("|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    for (name, size, from, to) in cases {
        let s = stroke(size, from, to);
        let tiles = s.touched().len();
        for painted in [false, true] {
            let base = painted_document(painted);
            let layer = base.active_layer();
            let surface = base.layer(layer).unwrap().pixels().clone();
            let (mut bare, mut commit, mut undo, mut redo) = (vec![], vec![], vec![], vec![]);
            let (mut held, mut held_undone, mut reserved) = (0, 0, 0);
            for _ in 0..RUNS {
                let mut target = surface.clone();
                bare.push(ms(|| {
                    s.commit_into(&mut target);
                }));
                drop::<TiledSurface<[u8; 4]>>(target);

                let mut d = base.clone();
                commit.push(ms(|| {
                    d.modify_tiles(layer, s.touched(), |c, t| {
                        s.commit_tile(c, t, PixelAlpha::Free)
                    })
                    .unwrap();
                }));
                held = d.history_bytes();
                reserved = d.history_reserved_bytes();
                undo.push(ms(|| {
                    d.undo().unwrap();
                }));
                held_undone = d.history_bytes();
                redo.push(ms(|| {
                    d.redo().unwrap();
                }));
            }
            let mib = |b: usize| format!("{:.2} MiB", b as f64 / (1024.0 * 1024.0));
            println!(
                "| {name} | {size:.0} | {} | {tiles} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | {} |",
                if painted { "painted" } else { "empty" },
                median(bare),
                median(commit),
                median(undo),
                median(redo),
                mib(held),
                mib(held_undone),
                mib(reserved),
            );
        }
    }
}
