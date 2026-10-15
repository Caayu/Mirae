//! Black-box probe for colour depth (phase 4E parity gate).
//!
//! `depth_probe psd <out.psd>` writes a **16-bit** layered test document in
//! the public PSD format: horizontal bands, each a ramp along x whose
//! values fall *between* 8-bit levels, some followed by a Color Dodge layer
//! that multiplies the band by 10 (`B = Cb / (1 − 0.9)`). A document that
//! keeps more than 8 bits shows a smooth ramp after the gain; one that
//! holds 8-bit values shows the steps of the 8-bit levels, ten levels high.
//! Opened in the reference application and exported to PNG through its
//! normal UI, it shows what the reference keeps of 16-bit data.
//!
//! `depth_probe read <in.png>` prints, for every band, the value along x
//! (red channel, row at the band centre) and compares it with two
//! predictions: the band composed from the 16-bit values, and from the
//! values rounded to 8 bits first.

use std::path::Path;

use tela_corpus::Image;
use tela_corpus::psd::{PsdLayer, PsdMask, write_psd16};

const W: u32 = 1024;
const BAND: u32 = 64;
const PITCH: u32 = 80;
const H: u32 = PITCH * 7;
/// Gain layer: Color Dodge with grey 0.9 divides the backdrop by 0.1.
const DODGE: f64 = 0.9;

/// What a band holds and how it reaches the output.
#[derive(Clone, Copy, Debug)]
enum Band {
    /// Opaque grey 128..132 (8-bit levels) in 16-bit steps: export rounding.
    Fine,
    /// Opaque grey `x / 4` stored exactly (`v·257`): 8-bit content.
    Exact8,
    /// Opaque dark grey 0.04..0.08 in 16-bit steps, then the gain.
    Gain16,
    /// The same ramp rounded to 8-bit levels, then the gain (control).
    Gain8,
    /// White over black, revealed by a 16-bit mask ramp 0.04..0.08, gain.
    MaskGain,
    /// White over black with a 16-bit alpha ramp 0.04..0.08, gain.
    AlphaGain,
    /// Grey 0.08..0.16 at 50 % layer opacity over black, gain.
    OpacityGain,
}

const BANDS: [Band; 7] = [
    Band::Fine,
    Band::Exact8,
    Band::Gain16,
    Band::Gain8,
    Band::MaskGain,
    Band::AlphaGain,
    Band::OpacityGain,
];

fn top(i: usize) -> u32 {
    i as u32 * PITCH
}

/// `0..=1` along the band.
fn t(x: u32) -> f64 {
    f64::from(x) / f64::from(W - 1)
}

fn q16(v: f64) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u16
}

/// The 16-bit value of a band's ramp at `x` (before any gain).
fn ramp(band: Band, x: u32) -> u16 {
    match band {
        Band::Fine => q16((128.0 + 4.0 * t(x)) / 255.0),
        Band::Exact8 => (x / 4) as u16 * 257,
        Band::Gain16 | Band::MaskGain | Band::AlphaGain => q16(0.04 + 0.04 * t(x)),
        Band::Gain8 => (((0.04 + 0.04 * t(x)) * 255.0).round() as u16) * 257,
        Band::OpacityGain => q16(0.08 + 0.08 * t(x)),
    }
}

fn grey(v: u16) -> [u16; 4] {
    [v, v, v, 65535]
}

fn layer(name: &str, band: usize, rgba: Vec<[u16; 4]>) -> PsdLayer<u16> {
    PsdLayer {
        name: name.to_string(),
        mode: *b"norm",
        opacity: 255,
        clipped: false,
        hidden: false,
        rect: (0, top(band), W, BAND),
        rgba,
        mask: None,
    }
}

fn band_pixels(f: impl Fn(u32) -> [u16; 4]) -> Vec<[u16; 4]> {
    (0..BAND).flat_map(|_| (0..W).map(&f)).collect()
}

fn layers() -> Vec<PsdLayer<u16>> {
    let mut out = Vec::new();
    for (i, &band) in BANDS.iter().enumerate() {
        let name = format!("{band:?}");
        match band {
            Band::Fine | Band::Exact8 | Band::Gain16 | Band::Gain8 => {
                out.push(layer(&name, i, band_pixels(|x| grey(ramp(band, x)))));
            }
            Band::MaskGain => {
                out.push(layer("black", i, band_pixels(|_| grey(0))));
                let mut l = layer(&name, i, band_pixels(|_| grey(65535)));
                l.mask = Some(PsdMask {
                    rect: l.rect,
                    values: (0..BAND)
                        .flat_map(|_| (0..W).map(|x| ramp(band, x)))
                        .collect(),
                    default: 255,
                    disabled: false,
                });
                out.push(l);
            }
            Band::AlphaGain => {
                out.push(layer("black", i, band_pixels(|_| grey(0))));
                out.push(layer(
                    &name,
                    i,
                    band_pixels(|x| [65535, 65535, 65535, ramp(band, x)]),
                ));
            }
            Band::OpacityGain => {
                out.push(layer("black", i, band_pixels(|_| grey(0))));
                let mut l = layer(&name, i, band_pixels(|x| grey(ramp(band, x))));
                l.opacity = 128;
                out.push(l);
            }
        }
    }
    // One gain layer over every band from Gain16 down.
    let first = 2;
    let mut dodge = layer("dodge", first, Vec::new());
    dodge.mode = *b"div ";
    dodge.rect = (0, top(first), W, top(BANDS.len()) - top(first));
    dodge.rgba = vec![grey(q16(DODGE)); (W * dodge.rect.3) as usize];
    out.push(dodge);
    out
}

/// How the bands after `Exact8` are amplified.
#[derive(Clone, Copy, Debug)]
enum Gain {
    /// No gain: the bands as stored (a plain export of the PNG).
    None,
    /// The Color Dodge layer of the PSD.
    Dodge,
    /// A levels adjustment applied in the reference: input `lo..hi` (8-bit
    /// units) to the full range.
    Levels(f64, f64),
}

/// The band's expected output (8-bit) when composed from `v`, the value
/// that reaches the gain (or the output) in the unit range.
fn shown(band: Band, v: f64, gain: Gain) -> u8 {
    let v = match (band, gain) {
        (Band::Fine | Band::Exact8, Gain::Dodge) | (_, Gain::None) => v,
        (Band::OpacityGain, _) => 0.5 * v / (1.0 - DODGE),
        (_, Gain::Dodge) => v / (1.0 - DODGE),
        (_, Gain::Levels(lo, hi)) => (v * 255.0 - lo) / (hi - lo),
    };
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// What happens to a 16-bit value before the gain, per prediction.
#[derive(Clone, Copy, Debug)]
enum Model {
    /// Kept at 16 bits.
    Full,
    /// Rounded to the nearest 8-bit level, `(v + 128) / 257`.
    Round8,
    /// Low byte dropped, `v >> 8`.
    Truncate8,
}

const MODELS: [Model; 3] = [Model::Full, Model::Round8, Model::Truncate8];

fn predict(band: Band, x: u32, model: Model, gain: Gain) -> u8 {
    let v16 = u32::from(ramp(band, x));
    let v = match model {
        Model::Full => f64::from(v16) / 65535.0,
        Model::Round8 => f64::from((v16 + 128) / 257) / 255.0,
        Model::Truncate8 => f64::from(v16 >> 8) / 255.0,
    };
    shown(band, v, gain)
}

/// Bands a single-layer image can carry (no layers, masks or modes).
const PNG_BANDS: usize = 4;

/// The first [`PNG_BANDS`] bands as one opaque 16-bit RGB PNG (white
/// between bands), at the same positions as in the PSD.
fn write_png16(path: &Path) -> std::io::Result<()> {
    let mut data = Vec::with_capacity((W * H * 6) as usize);
    for y in 0..H {
        let band = BANDS[..PNG_BANDS]
            .iter()
            .enumerate()
            .find(|(i, _)| (top(*i)..top(*i) + BAND).contains(&y));
        for x in 0..W {
            let v = band.map_or(65535, |(_, &b)| ramp(b, x));
            for _ in 0..3 {
                data.extend_from_slice(&v.to_be_bytes());
            }
        }
    }
    let f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(f, W, H);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Sixteen);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    w.write_image_data(&data).map_err(std::io::Error::other)
}

fn read(path: &Path, bands: usize, gain: Gain) -> std::io::Result<()> {
    let img = Image::load_png(path)?;
    println!(
        "band,levels,max_jump,then per model (full, round8, truncate8): mismatching pixels / max error"
    );
    for (i, &band) in BANDS[..bands].iter().enumerate() {
        let y = top(i) + BAND / 2;
        let got: Vec<u8> = (0..W).map(|x| img.get(x, y)[0]).collect();
        let mut levels = got.clone();
        levels.sort_unstable();
        levels.dedup();
        let jump = got
            .windows(2)
            .map(|w| w[0].abs_diff(w[1]))
            .max()
            .unwrap_or(0);
        let fits: Vec<String> = MODELS
            .iter()
            .map(|&m| {
                let errs = got
                    .iter()
                    .enumerate()
                    .map(|(x, &g)| g.abs_diff(predict(band, x as u32, m, gain)));
                let n = errs.clone().filter(|&e| e > 0).count();
                format!("{n}/{}", errs.max().unwrap_or(0))
            })
            .collect();
        if std::env::var_os("DEPTH_PROBE_DUMP").is_some() {
            let steps: Vec<String> = (1..got.len())
                .filter(|&x| got[x] != got[x - 1])
                .map(|x| format!("{x}:{}", got[x]))
                .collect();
            eprintln!("{band:?} steps {}", steps.join(" "));
        }
        println!("{band:?},{},{jump},{}", levels.len(), fits.join(","));
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize| args[i].parse::<f64>().expect("a number");
    match (args.get(1).map(String::as_str), args.len()) {
        (Some("psd"), 3) => write_psd16(Path::new(&args[2]), W, H, &layers()),
        (Some("png16"), 3) => write_png16(Path::new(&args[2])),
        (Some("read"), 3) => read(Path::new(&args[2]), BANDS.len(), Gain::Dodge),
        (Some("read-png"), 3) => read(Path::new(&args[2]), PNG_BANDS, Gain::None),
        (Some("read-levels"), 5) => {
            read(Path::new(&args[2]), PNG_BANDS, Gain::Levels(arg(3), arg(4)))
        }
        _ => {
            eprintln!(
                "usage: depth_probe psd <out.psd> | png16 <out.png> | read <export.png> \
                 | read-png <export.png> | read-levels <export.png> <lo> <hi>"
            );
            std::process::exit(2);
        }
    }
}
