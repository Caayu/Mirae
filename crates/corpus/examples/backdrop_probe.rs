//! Black-box probe for blending over a translucent backdrop without paper
//! (phase 5B, D-4C-1).
//!
//! `backdrop_probe psd <out.psd>` writes a layered test document in the
//! public PSD format: a base layer whose rows are every [`BOTTOM`] colour
//! at every [`BOTTOM_ALPHA`] (0, 1/255, 25 %, 50 %, 75 %, 100 %) and, for
//! every mode in [`MODES`] and layer opacity in [`OPACITIES`], a top layer
//! whose columns are every [`TOP`] colour at every [`TOP_ALPHA`] plus one
//! transparent control column. The reference application opens it (no
//! paper layer: the backdrop is the base alone) and exports a PNG with
//! transparency through its normal UI.
//!
//! `backdrop_probe read <in.png> <out.csv>` samples every swatch of such an
//! export into the CSV columns of `blend_probe` (mode, opacity, bottom and
//! top names and straight RGBA, measured straight RGBA, spread), which
//! `backdrop_fit` scores against candidate models.
//!
//! The PSD is only a test vehicle (Mirae does not read or write PSD); the
//! layout constants are the single source of truth for both commands.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use tela_corpus::psd::{PsdLayer, write_psd};

/// Bottom colours (straight RGB), each at every [`BOTTOM_ALPHA`].
const BOTTOM: [(&str, [u8; 3]); 6] = [
    ("black", [0, 0, 0]),
    ("white", [255, 255, 255]),
    ("red", [220, 40, 40]),
    ("green", [40, 190, 60]),
    ("blue", [40, 70, 220]),
    ("sky", [37, 149, 213]),
];

/// Backdrop alphas: transparent, 1/255, 25 %, 50 %, 75 %, opaque.
const BOTTOM_ALPHA: [u8; 6] = [0, 1, 64, 128, 191, 255];

/// Top colours (straight RGB), each at every [`TOP_ALPHA`].
const TOP: [(&str, [u8; 3]); 8] = [
    ("black", [0, 0, 0]),
    ("white", [255, 255, 255]),
    ("gray", [128, 128, 128]),
    ("red", [240, 20, 60]),
    ("green", [90, 220, 40]),
    ("blue", [30, 40, 250]),
    ("amber", [213, 149, 37]),
    ("purple", [160, 90, 200]),
];

/// Top pixel alphas.
const TOP_ALPHA: [u8; 3] = [255, 160, 64];

/// Tela name and PSD blend-mode key (public specification).
const MODES: [(&str, &[u8; 4]); 17] = [
    ("normal", b"norm"),
    ("multiply", b"mul "),
    ("screen", b"scrn"),
    ("overlay", b"over"),
    ("darken", b"dark"),
    ("lighten", b"lite"),
    ("color-dodge", b"div "),
    ("color-burn", b"idiv"),
    ("hard-light", b"hLit"),
    ("soft-light", b"sLit"),
    ("difference", b"diff"),
    ("exclusion", b"smud"),
    ("hue", b"hue "),
    ("saturation", b"sat "),
    ("color", b"colr"),
    ("luminosity", b"lum "),
    ("add", b"lddg"),
];

/// Layer opacities (0–255): 100 %, 60 %, 25 %.
const OPACITIES: [u8; 3] = [255, 153, 64];

const CELL: u32 = 8;
/// Rows: every bottom colour at every bottom alpha.
const ROWS: u32 = (BOTTOM.len() * BOTTOM_ALPHA.len()) as u32;
/// Columns: every top colour at every top alpha, then a transparent one.
const COLS: u32 = (TOP.len() * TOP_ALPHA.len()) as u32 + 1;
const REGION_W: u32 = CELL * COLS;
const REGION_H: u32 = CELL * ROWS;
const GAP: u32 = 8;
const REGIONS_PER_ROW: u32 = 9;

fn region_origin(index: usize) -> (u32, u32) {
    let i = index as u32;
    (
        GAP + (i % REGIONS_PER_ROW) * (REGION_W + GAP),
        GAP + (i / REGIONS_PER_ROW) * (REGION_H + GAP),
    )
}

fn size() -> (u32, u32) {
    let regions = (MODES.len() * OPACITIES.len()) as u32;
    (
        GAP + REGIONS_PER_ROW * (REGION_W + GAP),
        GAP + regions.div_ceil(REGIONS_PER_ROW) * (REGION_H + GAP),
    )
}

/// Name and straight RGBA of bottom row `row`.
fn bottom(row: u32) -> (String, [u8; 4]) {
    let (name, c) = BOTTOM[row as usize / BOTTOM_ALPHA.len()];
    let a = BOTTOM_ALPHA[row as usize % BOTTOM_ALPHA.len()];
    (format!("{name}_a{a}"), [c[0], c[1], c[2], a])
}

/// Name and straight RGBA of top column `col` (the last is transparent).
fn top(col: u32) -> (String, [u8; 4]) {
    if col as usize == TOP.len() * TOP_ALPHA.len() {
        return ("transparent".into(), [0; 4]);
    }
    let (name, c) = TOP[col as usize / TOP_ALPHA.len()];
    let a = TOP_ALPHA[col as usize % TOP_ALPHA.len()];
    (format!("{name}_a{a}"), [c[0], c[1], c[2], a])
}

fn layers() -> Vec<PsdLayer> {
    let (w, h) = size();
    let mut base = vec![[0u8; 4]; (w * h) as usize];
    let mut out = Vec::new();
    for index in 0..MODES.len() * OPACITIES.len() {
        let (ox, oy) = region_origin(index);
        for y in 0..REGION_H {
            for x in 0..REGION_W {
                base[((oy + y) * w + ox + x) as usize] = bottom(y / CELL).1;
            }
        }
    }
    out.push(PsdLayer {
        name: "base".into(),
        mode: *b"norm",
        opacity: 255,
        clipped: false,
        hidden: false,
        rect: (0, 0, w, h),
        rgba: base,
        mask: None,
    });
    for (m, (name, key)) in MODES.iter().enumerate() {
        for (o, &opacity) in OPACITIES.iter().enumerate() {
            let (ox, oy) = region_origin(m * OPACITIES.len() + o);
            let mut rgba = Vec::with_capacity((REGION_W * REGION_H) as usize);
            for _ in 0..REGION_H {
                for x in 0..REGION_W {
                    rgba.push(top(x / CELL).1);
                }
            }
            out.push(PsdLayer {
                name: format!("{name} {}", u32::from(opacity) * 100 / 255),
                mode: **key,
                opacity,
                clipped: false,
                hidden: false,
                rect: (ox, oy, REGION_W, REGION_H),
                rgba,
                mask: None,
            });
        }
    }
    out
}

fn read_png(path: &str, csv: &str) -> std::io::Result<()> {
    let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path)?));
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().expect("png size fits")];
    let frame = reader.next_frame(&mut buf)?;
    let (w, h) = size();
    assert_eq!(
        (frame.width, frame.height),
        (w, h),
        "unexpected export size"
    );
    assert_eq!(
        frame.color_type,
        png::ColorType::Rgba,
        "an export with transparency is expected"
    );
    assert_eq!(
        frame.bit_depth,
        png::BitDepth::Eight,
        "8-bit export expected"
    );
    let px = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * w + x) as usize) * 4;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    };
    // Outside every region nothing is drawn: the export must be
    // transparent there, or something (a paper) was composed in.
    let (gx, gy) = (GAP / 2, GAP / 2);
    assert_eq!(px(gx, gy)[3], 0, "the gap is not transparent: paper?");
    let mut out = BufWriter::new(File::create(csv)?);
    writeln!(
        out,
        "mode,opacity,bottom,top,br,bg,bb,ba,tr,tg,tb,ta,r,g,b,a,spread"
    )?;
    for (m, (mode, _)) in MODES.iter().enumerate() {
        for (o, &opacity) in OPACITIES.iter().enumerate() {
            let (ox, oy) = region_origin(m * OPACITIES.len() + o);
            for row in 0..ROWS {
                let (bname, b) = bottom(row);
                for col in 0..COLS {
                    let (tname, t) = top(col);
                    let (x0, y0) = (ox + col * CELL, oy + row * CELL);
                    let c = px(x0 + CELL / 2, y0 + CELL / 2);
                    // Largest channel difference inside the swatch: flags
                    // dithering or misaligned sampling.
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
                        "{mode},{opacity},{bname},{tname},{},{},{},{},{},{},{},{},{},{},{},{},{spread}",
                        b[0], b[1], b[2], b[3], t[0], t[1], t[2], t[3], c[0], c[1], c[2], c[3]
                    )?;
                }
            }
        }
    }
    out.flush()
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("psd") if args.len() == 3 => {
            let (w, h) = size();
            write_psd(Path::new(&args[2]), w, h, &layers())
        }
        Some("read") if args.len() == 4 => read_png(&args[2], &args[3]),
        _ => {
            eprintln!(
                "usage: backdrop_probe psd <out.psd> | backdrop_probe read <in.png> <out.csv>"
            );
            std::process::exit(2);
        }
    }
}
