//! Banding at 8 and 16 bits per channel (phase 4E): objective numbers,
//! not "looks smoother".
//!
//! Every scenario is run with 8-bit and 16-bit storage and measured on
//! the 8-bit display line (over white paper) against the exact `f64`
//! result (`tela_corpus::banding`). Shown levels and jumps are what the
//! eye sees; errors and lost pixels say how far storage rounding moved
//! the result; stored levels show the precision kept behind the display.
//!
//! `cargo run --release -p tela-corpus --example banding`

use tela_canvas::{DirtyTiles, IRect, Tile};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_corpus::banding::{Metrics, Profile, repeated_strokes, soft_stroke, stored_ramp};
use tela_document::{BlendMode, Document, Placement};

/// A dark grey ramp 0.04 → 0.08 across a 1024 px document, under a
/// Color Dodge layer of grey 0.9 that multiplies it by 10 (the gain of
/// the reference probe, parity record 4E). Display: the composed row;
/// exact: `255 · v / 0.1`.
fn ramp_under_gain(depth: PixelDepth) -> Profile {
    const W: u32 = 1024;
    let mut d = Document::with_depth(W, 1, depth);
    let ramp = d.active_layer();
    let dodge = d.add_layer(Placement::Top).unwrap();
    d.set_blend_mode(dodge, BlendMode::ColorDodge).unwrap();
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(IRect::from_size(W, 1));
    let value = |x: i32| 0.04 + 0.04 * f64::from(x) / f64::from(W - 1);
    let mut stored = Vec::new();
    with_pixel_type!(depth, P => {
        d.modify_tiles(ramp, &tiles, |c, t: &mut Tile<P>| {
            let (ox, _) = c.origin();
            for x in 0..256 {
                let v = value(ox + x as i32) as f32;
                *t.get_mut(x, 0) = P::from_premul(PremulRgba::new(v, v, v, 1.0));
            }
        })
        .unwrap();
        d.modify_tiles(dodge, &tiles, |_, t: &mut Tile<P>| {
            for x in 0..256 {
                *t.get_mut(x, 0) = P::from_premul(PremulRgba::new(0.9, 0.9, 0.9, 1.0));
            }
        })
        .unwrap();
        let s = d.layer(ramp).unwrap().pixels().surface::<P>().unwrap();
        for x in 0..W as i32 {
            stored.push(format!("{:?}", s.pixel(x, 0)));
        }
    });
    let mut shown = Vec::new();
    for c in tiles.iter() {
        let t = d.compose_tile(c, None).unwrap();
        shown.extend(t[..256].iter().map(|p| p[0]));
    }
    shown.truncate(W as usize);
    stored.sort_unstable();
    stored.dedup();
    Profile {
        exact: (0..W as i32)
            .map(|x| (value(x) / 0.1).min(1.0) * 255.0)
            .collect(),
        shown,
        stored_levels: stored.len(),
    }
}

fn row(name: &str, depth: PixelDepth, p: &Profile) {
    let m = Metrics::of(p);
    println!(
        "| {name} | {} | {} | {} | {} ({:.1}) | {:.2} | {:.3} | {} | {} |",
        depth.bits(),
        m.levels,
        m.stored_levels,
        m.max_jump,
        m.exact_max_jump,
        m.max_error,
        m.mean_error,
        m.widest_plateau,
        m.lost
    );
}

fn both(name: &str, f: impl Fn(PixelDepth) -> Profile) {
    for depth in PixelDepth::ALL {
        row(name, depth, &f(depth));
    }
}

fn strokes<P: StoragePixel>(size: f32, opacity: f32, flow: f32, n: u32) -> Profile {
    repeated_strokes::<P>(&soft_stroke(size, opacity, flow), n)
}

fn main() {
    println!(
        "| scenario | bits | shown levels | stored levels | max jump (exact) | max error | mean error | widest plateau px | lost px |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    both(
        "4000 px gradient 0 → 1",
        |d| with_pixel_type!(d, P => stored_ramp::<P>(0.0, 1.0, 4000)),
    );
    both("dark gradient under a ×10 gain", ramp_under_gain);
    both(
        "1 soft stroke, opacity 0.15",
        |d| with_pixel_type!(d, P => strokes::<P>(200.0, 0.15, 0.35, 1)),
    );
    both(
        "60 soft strokes, opacity 0.03",
        |d| with_pixel_type!(d, P => strokes::<P>(200.0, 0.03, 0.35, 60)),
    );
    both(
        "200 soft strokes, opacity 0.01",
        |d| with_pixel_type!(d, P => strokes::<P>(200.0, 0.01, 0.35, 200)),
    );
    both(
        "400 soft strokes, opacity 0.004",
        |d| with_pixel_type!(d, P => strokes::<P>(200.0, 0.004, 1.0, 400)),
    );
}
