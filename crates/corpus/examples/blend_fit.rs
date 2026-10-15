//! Which compositing space matches the reference? (phase 4C, ADR 0016)
//!
//! Reads the reference's measured blend results (with its paper layer,
//! `crates/document/tests/reference/blend_with_paper.csv`) and computes
//! every combination with Tela's blend math twice: on the stored
//! sRGB-encoded values, and on linear-light values (sRGB transfer function
//! removed before blending, re-applied after). Prints the largest and mean
//! channel error per mode for both.
//!
//! `cargo run --release -p tela-corpus --example blend_fit`

use tela_color::{PremulRgba, Rgba, srgb};
use tela_document::BlendMode;

fn main() {
    let path = format!(
        "{}/../document/tests/reference/blend_with_paper.csv",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).expect("reference CSV");
    println!("| mode | sRGB-encoded max | mean | linear-light max | mean |");
    println!("|---|---:|---:|---:|---:|");
    for mode in BlendMode::ALL {
        let mut stats = [(0u8, 0.0f64, 0usize); 2];
        for line in text.lines().skip(1) {
            let v: Vec<&str> = line.split(',').collect();
            if v[0] != mode.name() {
                continue;
            }
            let n = |i: usize| f32::from(v[i].parse::<u8>().unwrap()) / 255.0;
            let opacity = n(1);
            let bottom = Rgba::new(n(4), n(5), n(6), n(7));
            let top = Rgba::new(n(8), n(9), n(10), n(11));
            let measured = [12, 13, 14].map(|i| v[i].parse::<u8>().unwrap());
            for (k, linear) in [false, true].into_iter().enumerate() {
                let enc = |c: Rgba| {
                    if linear {
                        Rgba::new(
                            srgb::to_linear(c.r),
                            srgb::to_linear(c.g),
                            srgb::to_linear(c.b),
                            c.a,
                        )
                    } else {
                        c
                    }
                };
                // The paper (white, opaque), the base layer (Normal), then
                // the probed layer in `mode`.
                let mut px = [Rgba::WHITE.premultiply()];
                BlendMode::Normal.composite(&[enc(bottom).premultiply()], 1.0, &mut px);
                mode.composite(&[enc(top).premultiply()], opacity, &mut px);
                let s: Rgba = PremulRgba::unpremultiply(px[0]);
                let out = [s.r, s.g, s.b].map(|c| {
                    let c = if linear { srgb::from_linear(c) } else { c };
                    (c.clamp(0.0, 1.0) * 255.0).round() as u8
                });
                for i in 0..3 {
                    let e = out[i].abs_diff(measured[i]);
                    stats[k].0 = stats[k].0.max(e);
                    stats[k].1 += f64::from(e);
                    stats[k].2 += 1;
                }
            }
        }
        let mean = |s: (u8, f64, usize)| s.1 / s.2 as f64;
        println!(
            "| {} | {} | {:.3} | {} | {:.3} |",
            mode.name(),
            stats[0].0,
            mean(stats[0]),
            stats[1].0,
            mean(stats[1])
        );
    }
}
