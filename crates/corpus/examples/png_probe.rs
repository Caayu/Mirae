//! Black-box probe for PNG import and export (phase 4F parity gate).
//!
//! `png_probe write <dir>` writes three test images of our own, each a
//! grid of swatches inside a 2 px opaque magenta frame (so the position
//! of an imported image can be measured in an export):
//!
//! - `rgba8.png`: RGBA 8-bit, straight alpha; rows are colours, columns
//!   alphas (255 … 0).
//! - `rgb8.png`: the same colours, RGB 8-bit (no alpha channel).
//! - `rgba16.png`: RGBA 16-bit; the top half opaque greys whose low byte
//!   is set (truncating and rounding to 8 bits give different levels),
//!   the bottom half white with 16-bit alphas.
//!
//! Imported into the reference application (Arquivo › Importar › Imagem)
//! and exported to PNG through its normal UI, they show where an import
//! lands, what it does to straight-alpha colour, and how 16 bits are
//! reduced.
//!
//! `png_probe read <rgba8|rgb8|rgba16> <export.png>` finds the frame in an
//! export, samples every swatch centre and compares it with the models
//! printed in the header.

use std::path::Path;

/// Swatch edge in pixels.
const SW: u32 = 24;
/// Opaque frame width around the grid.
const FRAME: u32 = 2;
const MAGENTA: [u8; 4] = [255, 0, 255, 255];

const COLOURS: [[u8; 3]; 10] = [
    [255, 255, 255],
    [0, 0, 0],
    [128, 128, 128],
    [255, 0, 0],
    [10, 200, 90],
    [200, 160, 60],
    [1, 2, 3],
    [254, 253, 252],
    [37, 99, 201],
    [129, 127, 128],
];
const ALPHAS: [u8; 10] = [255, 254, 200, 128, 100, 64, 17, 5, 1, 0];

/// Opaque grey levels with their low byte set (top half of `rgba16`).
fn grey16(i: u32) -> u16 {
    (i * 1677 + 129) as u16
}

/// White's 16-bit alphas (bottom half of `rgba16`).
fn alpha16(i: u32) -> u16 {
    [
        65535, 65534, 65408, 51400, 32896, 32895, 16577, 4500, 1000, 300,
    ][i as usize]
}

const COLS16: u32 = 10;
const ROWS16: u32 = 8;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Rgba8,
    Rgb8,
    Rgba16,
}

fn grid(kind: Kind) -> (u32, u32) {
    match kind {
        Kind::Rgba8 | Kind::Rgb8 => (ALPHAS.len() as u32, COLOURS.len() as u32),
        Kind::Rgba16 => (COLS16, ROWS16),
    }
}

/// The straight 16-bit RGBA of swatch `(col, row)`.
fn swatch16(kind: Kind, col: u32, row: u32) -> [u16; 4] {
    match kind {
        Kind::Rgba8 => {
            let c = COLOURS[row as usize];
            let a = ALPHAS[col as usize];
            [c[0], c[1], c[2], a].map(|v| u16::from(v) * 257)
        }
        Kind::Rgb8 => {
            let c = COLOURS[row as usize];
            [c[0], c[1], c[2], 255].map(|v| u16::from(v) * 257)
        }
        Kind::Rgba16 if row < ROWS16 / 2 => {
            let v = grey16(row * COLS16 + col);
            [v, v, v, 65535]
        }
        Kind::Rgba16 => [65535, 65535, 65535, alpha16(col)],
    }
}

fn size(kind: Kind) -> (u32, u32) {
    let (c, r) = grid(kind);
    (c * SW + 2 * FRAME, r * SW + 2 * FRAME)
}

/// Straight 16-bit RGBA at `(x, y)` of the test image.
fn pixel16(kind: Kind, x: u32, y: u32) -> [u16; 4] {
    let (w, h) = size(kind);
    if x < FRAME || y < FRAME || x >= w - FRAME || y >= h - FRAME {
        return MAGENTA.map(|v| u16::from(v) * 257);
    }
    swatch16(kind, (x - FRAME) / SW, (y - FRAME) / SW)
}

fn write(dir: &Path) -> std::io::Result<()> {
    for (kind, name) in [
        (Kind::Rgba8, "rgba8.png"),
        (Kind::Rgb8, "rgb8.png"),
        (Kind::Rgba16, "rgba16.png"),
    ] {
        let (w, h) = size(kind);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let p = pixel16(kind, x, y);
                match kind {
                    Kind::Rgba8 => data.extend(p.map(|v| (v / 257) as u8)),
                    Kind::Rgb8 => data.extend(p[..3].iter().map(|v| (v / 257) as u8)),
                    Kind::Rgba16 => p.iter().for_each(|v| data.extend(v.to_be_bytes())),
                }
            }
        }
        let f = std::io::BufWriter::new(std::fs::File::create(dir.join(name))?);
        let mut enc = png::Encoder::new(f, w, h);
        enc.set_color(match kind {
            Kind::Rgb8 => png::ColorType::Rgb,
            _ => png::ColorType::Rgba,
        });
        enc.set_depth(match kind {
            Kind::Rgba16 => png::BitDepth::Sixteen,
            _ => png::BitDepth::Eight,
        });
        let mut wr = enc.write_header().map_err(std::io::Error::other)?;
        wr.write_image_data(&data).map_err(std::io::Error::other)?;
        println!("{name}: {w} x {h}");
    }
    Ok(())
}

/// An exported PNG as 8-bit straight RGBA (RGB gets alpha 255).
fn load(path: &Path) -> std::io::Result<(u32, u32, Vec<[u8; 4]>)> {
    let mut dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path)?));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut r = dec.read_info().map_err(std::io::Error::other)?;
    let mut buf = vec![0; r.output_buffer_size().expect("small image")];
    let info = r.next_frame(&mut buf).map_err(std::io::Error::other)?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(std::io::Error::other("expected an 8-bit export"));
    }
    let px = match info.color_type {
        png::ColorType::Rgba => buf.as_chunks::<4>().0.to_vec(),
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => return Err(std::io::Error::other(format!("colour type {other:?}"))),
    };
    println!(
        "export: {} x {}, {:?} {:?}",
        info.width, info.height, info.color_type, info.bit_depth
    );
    Ok((info.width, info.height, px))
}

/// Models of an 8-bit import followed by a straight-alpha 8-bit export.
fn models8(c: u8, a: u8) -> [u8; 2] {
    let straight = c;
    // Premultiplied 8-bit storage, rounded both ways.
    let unpremul = if a == 0 {
        0
    } else {
        let p = (f64::from(c) * f64::from(a) / 255.0).round();
        (p * 255.0 / f64::from(a)).round().min(255.0) as u8
    };
    [straight, unpremul]
}

fn read(kind: Kind, path: &Path) -> std::io::Result<()> {
    let (w, h, px) = load(path)?;
    let at = |x: u32, y: u32| px[(y * w + x) as usize];
    // The frame's top-left corner: the first magenta pixel in scan order.
    let origin = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .find(|&(x, y)| at(x, y) == MAGENTA)
        .ok_or_else(|| std::io::Error::other("frame not found"))?;
    let (iw, ih) = size(kind);
    println!(
        "image {iw} x {ih} at ({}, {}); canvas centre would put it at ({}, {})",
        origin.0,
        origin.1,
        (w - iw) / 2,
        (h - ih) / 2
    );
    let (cols, rows) = grid(kind);
    let mut worst = [0u8; 4];
    match kind {
        Kind::Rgba16 => println!("col,row,in16 (r,a),got (r,a),trunc8,round8"),
        _ => println!("col,row,in (r,g,b,a),got (r,g,b,a),straight err,premul8 err"),
    }
    for row in 0..rows {
        for col in 0..cols {
            let x = origin.0 + FRAME + col * SW + SW / 2;
            let y = origin.1 + FRAME + row * SW + SW / 2;
            let got = at(x, y);
            let input = swatch16(kind, col, row);
            match kind {
                Kind::Rgba16 => {
                    let (r, a) = (input[0], input[3]);
                    let t = ((r >> 8) as u8, (a >> 8) as u8);
                    let n = (tela_color::narrow16(r), tela_color::narrow16(a));
                    let et = got[0].abs_diff(t.0).max(got[3].abs_diff(t.1));
                    let en = got[0].abs_diff(n.0).max(got[3].abs_diff(n.1));
                    worst[0] = worst[0].max(et);
                    worst[1] = worst[1].max(en);
                    println!(
                        "{col},{row},{r} {a},{} {},{}/{} err {et},{}/{} err {en}",
                        got[0], got[3], t.0, t.1, n.0, n.1
                    );
                }
                _ => {
                    let inp = input.map(|v| (v / 257) as u8);
                    if got[3] != inp[3] {
                        worst[2] = worst[2].max(got[3].abs_diff(inp[3]));
                    }
                    let (mut es, mut ep) = (0u8, 0u8);
                    if inp[3] > 0 {
                        for ch in 0..3 {
                            let [s, p] = models8(inp[ch], inp[3]);
                            es = es.max(got[ch].abs_diff(s));
                            ep = ep.max(got[ch].abs_diff(p));
                        }
                    }
                    worst[0] = worst[0].max(es);
                    worst[1] = worst[1].max(ep);
                    println!("{col},{row},{inp:?},{got:?},{es},{ep}");
                }
            }
        }
    }
    match kind {
        Kind::Rgba16 => println!("worst: trunc8 {}, round8 {}", worst[0], worst[1]),
        _ => println!(
            "worst colour error: straight {}, premul8 {}; worst alpha error {}",
            worst[0], worst[1], worst[2]
        ),
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let kind = |s: &str| match s {
        "rgba8" => Some(Kind::Rgba8),
        "rgb8" => Some(Kind::Rgb8),
        "rgba16" => Some(Kind::Rgba16),
        _ => None,
    };
    match (args.get(1).map(String::as_str), args.len()) {
        (Some("write"), 3) => write(Path::new(&args[2])),
        (Some("read"), 4) if kind(&args[2]).is_some() => {
            read(kind(&args[2]).expect("checked"), Path::new(&args[3]))
        }
        _ => {
            eprintln!("usage: png_probe write <dir> | read <rgba8|rgb8|rgba16> <export.png>");
            std::process::exit(2);
        }
    }
}
