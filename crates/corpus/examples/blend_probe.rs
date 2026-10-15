//! Black-box probe for blend modes (phase 4C parity gate).
//!
//! `blend_probe psd <out.psd>` writes a layered test document in the
//! public PSD format (Adobe Photoshop File Formats Specification): a base
//! layer of `BOTTOM` swatches and, for every mode in `MODES`, two top
//! layers of `TOP` swatches (100 % and 60 % layer opacity) with that blend
//! mode. Opened in the reference application and exported to PNG through
//! its normal UI, it gives the reference's result for every combination.
//!
//! `blend_probe read <in.png> <out.csv>` samples the centre of every
//! swatch of such an export and writes one CSV row per combination
//! (straight-alpha RGBA as stored in the PNG).
//!
//! The PSD is only a test vehicle (Tela does not read or write PSD); the
//! layout constants are the single source of truth for both commands.

use std::fs::File;
use std::io::{BufWriter, Write};

/// Straight-alpha RGBA swatches of the bottom (base) layer, one per row.
const BOTTOM: [(&str, [u8; 4]); 10] = [
    ("white", [255, 255, 255, 255]),
    ("black", [0, 0, 0, 255]),
    ("gray", [128, 128, 128, 255]),
    ("red", [230, 30, 30, 255]),
    ("blue", [40, 90, 220, 255]),
    ("ochre", [200, 160, 60, 255]),
    ("teal_a50", [40, 180, 120, 128]),
    ("transparent", [0, 0, 0, 0]),
    ("dark", [30, 40, 50, 255]),
    ("light", [220, 230, 240, 255]),
];

/// Straight-alpha RGBA swatches of each top layer, one per column.
const TOP: [(&str, [u8; 4]); 10] = [
    ("white", [255, 255, 255, 255]),
    ("black", [0, 0, 0, 255]),
    ("gray", [128, 128, 128, 255]),
    ("green", [30, 200, 60, 255]),
    ("magenta", [210, 40, 180, 255]),
    ("orange", [240, 150, 40, 255]),
    ("rust_a38", [200, 60, 40, 96]),
    ("transparent", [0, 0, 0, 0]),
    ("silver", [192, 192, 192, 255]),
    ("charcoal", [64, 64, 64, 255]),
];

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

/// Layer opacities (0–255) probed for every mode.
const OPACITIES: [u8; 2] = [255, 153];

const CELL: u32 = 12;
const REGION: u32 = CELL * 10;
const PITCH: u32 = REGION + 8;
const COLUMNS: u32 = 6;

fn region_origin(index: usize) -> (u32, u32) {
    let i = index as u32;
    (8 + (i % COLUMNS) * PITCH, 8 + (i / COLUMNS) * PITCH)
}

fn size() -> (u32, u32) {
    let regions = (MODES.len() * OPACITIES.len()) as u32;
    let rows = regions.div_ceil(COLUMNS);
    (8 + COLUMNS * PITCH, 8 + rows * PITCH)
}

/// One layer as planar straight-alpha channels over its bounds.
struct Layer {
    name: String,
    key: [u8; 4],
    opacity: u8,
    top: u32,
    left: u32,
    width: u32,
    height: u32,
    rgba: Vec<[u8; 4]>,
}

fn layers() -> Vec<Layer> {
    let (w, h) = size();
    let mut base = vec![[0u8; 4]; (w * h) as usize];
    let mut out = Vec::new();
    for index in 0..MODES.len() * OPACITIES.len() {
        let (ox, oy) = region_origin(index);
        for y in 0..REGION {
            for x in 0..REGION {
                let color = BOTTOM[(y / CELL) as usize].1;
                base[((oy + y) * w + ox + x) as usize] = color;
            }
        }
    }
    out.push(Layer {
        name: "base".into(),
        key: *b"norm",
        opacity: 255,
        top: 0,
        left: 0,
        width: w,
        height: h,
        rgba: base,
    });
    for (m, (name, key)) in MODES.iter().enumerate() {
        for (o, &opacity) in OPACITIES.iter().enumerate() {
            let (ox, oy) = region_origin(m * OPACITIES.len() + o);
            let mut rgba = vec![[0u8; 4]; (REGION * REGION) as usize];
            for y in 0..REGION {
                for x in 0..REGION {
                    rgba[(y * REGION + x) as usize] = TOP[(x / CELL) as usize].1;
                }
            }
            out.push(Layer {
                name: format!("{name} {}", u32::from(opacity) * 100 / 255),
                key: **key,
                opacity,
                top: oy,
                left: ox,
                width: REGION,
                height: REGION,
                rgba,
            });
        }
    }
    out
}

fn be16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn be32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}

/// PSD, 8-bit RGB, one raw (uncompressed) layer per entry, bottom first.
fn write_psd(path: &str) -> std::io::Result<()> {
    let (w, h) = size();
    let layers = layers();
    let mut info = Vec::new();
    be16(&mut info, layers.len() as u16);
    // Channel order: alpha (-1), R, G, B.
    let channels: [(i16, usize); 4] = [(-1, 3), (0, 0), (1, 1), (2, 2)];
    for l in &layers {
        be32(&mut info, l.top);
        be32(&mut info, l.left);
        be32(&mut info, l.top + l.height);
        be32(&mut info, l.left + l.width);
        be16(&mut info, channels.len() as u16);
        for (id, _) in channels {
            be16(&mut info, id as u16);
            be32(&mut info, 2 + l.width * l.height);
        }
        info.extend_from_slice(b"8BIM");
        info.extend_from_slice(&l.key);
        info.push(l.opacity);
        info.push(0); // clipping: base
        info.push(0); // flags: visible
        info.push(0); // filler
        let mut name = vec![l.name.len() as u8];
        name.extend_from_slice(l.name.as_bytes());
        while name.len() % 4 != 0 {
            name.push(0);
        }
        be32(&mut info, (4 + 4 + name.len()) as u32);
        be32(&mut info, 0); // no layer mask
        be32(&mut info, 0); // no blending ranges
        info.extend_from_slice(&name);
    }
    for l in &layers {
        for (_, c) in channels {
            be16(&mut info, 0); // raw
            info.extend(l.rgba.iter().map(|p| p[c]));
        }
    }
    if info.len() % 2 != 0 {
        info.push(0);
    }

    let mut f = BufWriter::new(File::create(path)?);
    let mut head = Vec::new();
    head.extend_from_slice(b"8BPS");
    be16(&mut head, 1);
    head.extend_from_slice(&[0; 6]);
    be16(&mut head, 3); // composite channels
    be32(&mut head, h);
    be32(&mut head, w);
    be16(&mut head, 8);
    be16(&mut head, 3); // RGB
    be32(&mut head, 0); // color mode data
    be32(&mut head, 0); // image resources
    be32(&mut head, (4 + info.len() + 4) as u32); // layer and mask info
    be32(&mut head, info.len() as u32);
    f.write_all(&head)?;
    f.write_all(&info)?;
    f.write_all(&0u32.to_be_bytes())?; // global layer mask info
    // Merged image: white, raw; readers recompose from the layers.
    let mut merged = Vec::new();
    be16(&mut merged, 0);
    merged.resize(2 + (3 * w * h) as usize, 255);
    f.write_all(&merged)?;
    f.flush()
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
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unsupported PNG color type {other:?}"),
    };
    assert_eq!(
        frame.bit_depth,
        png::BitDepth::Eight,
        "8-bit export expected"
    );
    let px = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * w + x) as usize) * channels;
        let a = if channels == 4 { buf[i + 3] } else { 255 };
        [buf[i], buf[i + 1], buf[i + 2], a]
    };
    let mut out = BufWriter::new(File::create(csv)?);
    writeln!(
        out,
        "mode,opacity,bottom,top,br,bg,bb,ba,tr,tg,tb,ta,r,g,b,a,spread"
    )?;
    for (m, (mode, _)) in MODES.iter().enumerate() {
        for (o, &opacity) in OPACITIES.iter().enumerate() {
            let (ox, oy) = region_origin(m * OPACITIES.len() + o);
            for (bi, (bname, b)) in BOTTOM.iter().enumerate() {
                for (ti, (tname, t)) in TOP.iter().enumerate() {
                    let cx = ox + ti as u32 * CELL + CELL / 2;
                    let cy = oy + bi as u32 * CELL + CELL / 2;
                    let c = px(cx, cy);
                    // Largest channel difference inside the swatch: flags
                    // dithering or misaligned sampling.
                    let mut spread = 0u8;
                    for dy in 0..CELL - 4 {
                        for dx in 0..CELL - 4 {
                            let p = px(
                                ox + ti as u32 * CELL + 2 + dx,
                                oy + bi as u32 * CELL + 2 + dy,
                            );
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
        Some("psd") if args.len() == 3 => write_psd(&args[2]),
        Some("read") if args.len() == 4 => read_png(&args[2], &args[3]),
        _ => {
            eprintln!("usage: blend_probe psd <out.psd> | blend_probe read <in.png> <out.csv>");
            std::process::exit(2);
        }
    }
}
