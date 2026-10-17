//! Black-box probe for "merge down" (phase 5B).
//!
//! `merge_probe psd <dir>` writes one small layered test document per
//! scenario of [`SCENARIOS`] in the public PSD format. Each layer covers
//! the whole 8 × 8 grid of 16 px swatches: rows are the [`BOTTOM`]
//! swatches, columns the [`TOP`] swatches (or one fill colour). In the
//! reference application the layer named in the scenario is merged down
//! through its normal UI, and the document is exported to PNG before and
//! after (with transparency, no paper unless the scenario adds it).
//!
//! `merge_probe read <in.png> <out.csv>` samples the 64 swatches of such
//! an export (`row,col,r,g,b,a,spread`).
//!
//! The PSD is only a test vehicle (Mirae does not read or write PSD); the
//! constants here are the single source of truth for Mirae's own merge
//! parity test, which rebuilds each stack through `Document`.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use tela_corpus::psd::{PsdLayer, PsdMask, write_psd};

/// Swatches of the "bottom" content, one per row (straight RGBA).
const BOTTOM: [[u8; 4]; 8] = [
    [220, 40, 40, 255],
    [220, 40, 40, 128],
    [37, 149, 213, 191],
    [37, 149, 213, 64],
    [40, 190, 60, 255],
    [0, 0, 0, 128],
    [255, 255, 255, 64],
    [0, 0, 0, 0],
];

/// Swatches of the "top" content, one per column (straight RGBA).
const TOP: [[u8; 4]; 8] = [
    [90, 220, 40, 255],
    [90, 220, 40, 128],
    [30, 40, 250, 160],
    [213, 149, 37, 64],
    [0, 0, 0, 255],
    [255, 255, 255, 128],
    [128, 128, 128, 200],
    [0, 0, 0, 0],
];

/// Mask value per column where a scenario masks a layer (255 reveals).
const MASK: [u8; 8] = [255, 160, 96, 0, 255, 160, 96, 0];

/// A fill under a clipping group (opaque amber) or a clipped fill.
const UNDER: [u8; 4] = [213, 149, 37, 255];
const FILL_BLUE: [u8; 4] = [30, 40, 250, 200];

const CELL: u32 = 16;
const N: u32 = 8;
const MARGIN: u32 = 16;
const SIZE: u32 = 2 * MARGIN + N * CELL;

#[derive(Clone, Copy)]
enum Content {
    Bottom,
    Top,
    Fill([u8; 4]),
}

#[derive(Clone, Copy)]
struct L {
    name: &'static str,
    content: Content,
    mode: &'static [u8; 4],
    opacity: u8,
    clipped: bool,
    hidden: bool,
    mask: Option<bool>,
}

const fn layer(name: &'static str, content: Content) -> L {
    L {
        name,
        content,
        mode: b"norm",
        opacity: 255,
        clipped: false,
        hidden: false,
        mask: None,
    }
}

const fn op(mut l: L, opacity: u8) -> L {
    l.opacity = opacity;
    l
}

const fn mode(mut l: L, key: &'static [u8; 4]) -> L {
    l.mode = key;
    l
}

const fn clip(mut l: L) -> L {
    l.clipped = true;
    l
}

const fn hide(mut l: L) -> L {
    l.hidden = true;
    l
}

/// A mask of [`MASK`] columns; `true` = disabled.
const fn masked(mut l: L, disabled: bool) -> L {
    l.mask = Some(disabled);
    l
}

const B: L = layer("bottom", Content::Bottom);
const T: L = layer("top", Content::Top);

/// Scenario name and its stack, bottom first. In every scenario the layer
/// merged down is the top one ("top"), except `md_base_down`, where it is
/// "bottom" (the base of a clipping group) merged into "under".
const SCENARIOS: &[(&str, &[L])] = &[
    ("md_normal", &[B, T]),
    ("md_top_op60", &[B, op(T, 153)]),
    ("md_bottom_op60", &[op(B, 153), T]),
    ("md_top_multiply", &[B, mode(T, b"mul ")]),
    ("md_top_screen_op60", &[B, op(mode(T, b"scrn"), 153)]),
    ("md_top_hue", &[B, mode(T, b"hue ")]),
    ("md_bottom_multiply", &[mode(B, b"mul "), T]),
    (
        "md_both_modes",
        &[op(mode(B, b"mul "), 204), op(mode(T, b"scrn"), 153)],
    ),
    ("md_top_mask", &[B, masked(T, false)]),
    ("md_bottom_mask", &[masked(B, false), T]),
    ("md_top_mask_disabled", &[B, masked(T, true)]),
    ("md_bottom_mask_disabled", &[masked(B, true), T]),
    ("md_clip_into_base", &[B, clip(T)]),
    (
        "md_clip_multiply_into_base",
        &[B, clip(op(mode(T, b"mul "), 204))],
    ),
    (
        "md_clip_into_clip",
        &[
            layer("under", Content::Fill(UNDER)),
            clip(layer("bottom", Content::Fill(FILL_BLUE))),
            clip(T),
        ],
    ),
    (
        "md_base_down",
        &[
            layer("under", Content::Fill(UNDER)),
            B,
            clip(layer("clip", Content::Top)),
        ],
    ),
    (
        "md_under_bottom_multiply",
        &[layer("under", Content::Fill(UNDER)), mode(B, b"mul "), T],
    ),
    (
        "md_under_top_screen",
        &[
            layer("under", Content::Fill(UNDER)),
            B,
            op(mode(T, b"scrn"), 153),
        ],
    ),
    ("md_top_hidden", &[B, hide(T)]),
    ("md_bottom_hidden", &[hide(B), T]),
    ("md_both_hidden", &[hide(B), hide(T)]),
];

fn pixel(c: Content, row: usize, col: usize) -> [u8; 4] {
    match c {
        Content::Bottom => BOTTOM[row],
        Content::Top => TOP[col],
        Content::Fill(p) => p,
    }
}

fn psd_layers(stack: &[L]) -> Vec<PsdLayer> {
    stack
        .iter()
        .map(|l| {
            let mut rgba = Vec::with_capacity((SIZE * SIZE) as usize);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let inside = (MARGIN..MARGIN + N * CELL).contains(&x)
                        && (MARGIN..MARGIN + N * CELL).contains(&y);
                    rgba.push(if inside {
                        pixel(
                            l.content,
                            ((y - MARGIN) / CELL) as usize,
                            ((x - MARGIN) / CELL) as usize,
                        )
                    } else {
                        [0; 4]
                    });
                }
            }
            let mask = l.mask.map(|disabled| PsdMask {
                rect: (MARGIN, MARGIN, N * CELL, N * CELL),
                values: (0..N * CELL * N * CELL)
                    .map(|i| MASK[((i % (N * CELL)) / CELL) as usize])
                    .collect(),
                default: 255,
                disabled,
            });
            PsdLayer {
                name: l.name.into(),
                mode: *l.mode,
                opacity: l.opacity,
                clipped: l.clipped,
                hidden: l.hidden,
                rect: (0, 0, SIZE, SIZE),
                rgba,
                mask,
            }
        })
        .collect()
}

fn read_png(path: &str, csv: &str) -> std::io::Result<()> {
    let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path)?));
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().expect("png size fits")];
    let frame = reader.next_frame(&mut buf)?;
    assert_eq!((frame.width, frame.height), (SIZE, SIZE), "export size");
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unsupported PNG color type {other:?}"),
    };
    let px = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * SIZE + x) as usize) * channels;
        let a = if channels == 4 { buf[i + 3] } else { 255 };
        [buf[i], buf[i + 1], buf[i + 2], a]
    };
    let mut out = BufWriter::new(File::create(csv)?);
    writeln!(out, "row,col,r,g,b,a,spread")?;
    for row in 0..N {
        for col in 0..N {
            let (x0, y0) = (MARGIN + col * CELL, MARGIN + row * CELL);
            let c = px(x0 + CELL / 2, y0 + CELL / 2);
            let mut spread = 0u8;
            for dy in 2..CELL - 2 {
                for dx in 2..CELL - 2 {
                    let p = px(x0 + dx, y0 + dy);
                    for k in 0..4 {
                        spread = spread.max(p[k].abs_diff(c[k]));
                    }
                }
            }
            writeln!(
                out,
                "{row},{col},{},{},{},{},{spread}",
                c[0], c[1], c[2], c[3]
            )?;
        }
    }
    out.flush()
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("psd") if args.len() == 3 => {
            let dir = Path::new(&args[2]);
            std::fs::create_dir_all(dir)?;
            for (name, stack) in SCENARIOS {
                write_psd(
                    &dir.join(format!("{name}.psd")),
                    SIZE,
                    SIZE,
                    &psd_layers(stack),
                )?;
            }
            Ok(())
        }
        Some("read") if args.len() == 4 => read_png(&args[2], &args[3]),
        _ => {
            eprintln!("usage: merge_probe psd <dir> | merge_probe read <in.png> <out.csv>");
            std::process::exit(2);
        }
    }
}
