//! Black-box probe for colour picking (phase 5A parity gate).
//!
//! `color_probe psd <out.psd>` writes an 800 × 600 layered test document in
//! the public PSD format. Opened in the reference application, its
//! eyedropper is used on the swatches and the picked colour is read from
//! the screen (docs/parity/5A-color.md):
//!
//! - rows 1 and 2 (`y` 40–140 and 180–280): six 100 px swatches of one
//!   colour at the alphas of [`ALPHAS`], on the layer "swatches";
//! - row 3 (`y` 330–430): an opaque [`BASE`] band (`x` 50–450) on
//!   "swatches", under an opaque [`TOP`] band (`x` 250–650) on the layer
//!   "multiply" (blend mode Multiply): picking the overlap tells whether
//!   the eyedropper reads the composite or one layer;
//! - row 4 (`y` 480–560, `x` 50–450): a one-pixel black/white checker, for
//!   picking while zoomed out (does the pick read a document pixel or the
//!   reduced display?).
//!
//! The same values are used by Mirae's own tests of the eyedropper.

use std::path::Path;

use tela_corpus::psd::{PsdLayer, write_psd};

/// Swatch alphas of rows 1 and 2, left to right.
const ALPHAS: [u8; 6] = [255, 128, 64, 8, 1, 0];
/// Colour of row 1.
const ROW1: [u8; 3] = [220, 80, 40];
/// Colour of row 2.
const ROW2: [u8; 3] = [37, 149, 213];
/// Bottom band of row 3.
const BASE: [u8; 3] = [220, 80, 40];
/// Multiply band of row 3.
const TOP: [u8; 3] = [100, 150, 250];

const W: u32 = 800;
const H: u32 = 600;

fn swatches() -> Vec<[u8; 4]> {
    let mut px = vec![[0u8; 4]; (W * H) as usize];
    for y in 0..H {
        for x in 0..W {
            let p = &mut px[(y * W + x) as usize];
            let column = (x >= 50 && (x - 50) % 120 < 100).then(|| (x - 50) / 120);
            match (y, column) {
                (40..140, Some(c)) if c < 6 => {
                    *p = [ROW1[0], ROW1[1], ROW1[2], ALPHAS[c as usize]];
                }
                (180..280, Some(c)) if c < 6 => {
                    *p = [ROW2[0], ROW2[1], ROW2[2], ALPHAS[c as usize]];
                }
                (330..430, _) if (50..450).contains(&x) => {
                    *p = [BASE[0], BASE[1], BASE[2], 255];
                }
                (480..560, _) if (50..450).contains(&x) => {
                    let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                    *p = [v, v, v, 255];
                }
                _ => {}
            }
        }
    }
    px
}

fn layers() -> Vec<PsdLayer> {
    let full = |name: &str, mode: [u8; 4], rgba| PsdLayer {
        name: name.into(),
        mode,
        opacity: 255,
        clipped: false,
        hidden: false,
        rect: (0, 0, W, H),
        rgba,
        mask: None,
    };
    let mut top = vec![[0u8; 4]; (W * H) as usize];
    for y in 330..430 {
        for x in 250..650 {
            top[(y * W + x) as usize] = [TOP[0], TOP[1], TOP[2], 255];
        }
    }
    vec![
        full("swatches", *b"norm", swatches()),
        full("multiply", *b"mul ", top),
    ]
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("psd") if args.len() == 3 => write_psd(Path::new(&args[2]), W, H, &layers()),
        _ => {
            eprintln!("usage: color_probe psd <out.psd>");
            std::process::exit(2);
        }
    }
}
