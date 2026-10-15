//! PNG import as the pixels of a new layer (parity record 4F, F15–F18).
//!
//! Any PNG colour type is accepted (palette, grey, grey + alpha, RGB,
//! RGBA; transparency chunks; 1–16 bits), expanded to straight RGBA and
//! premultiplied in `f64`, then quantized **once** to the document's
//! depth: a 16-bit image keeps its 16 bits in a 16-bit document and is
//! rounded to the nearest 8-bit level in an 8-bit one. The reference
//! truncates 16-bit images to their high byte (F18); Mirae rounds
//! deliberately (parity D-4F-3). Values are taken as sRGB; colour-space
//! chunks are reported, not applied (no colour management yet).
//!
//! The image is centred on the canvas like the reference's import (F15):
//! its top-left pixel at `((W − w) / 2, (H − h) / 2)` rounded down, which
//! may be negative for images larger than the canvas (the pixels outside
//! it are kept, as layers may hold them).

use std::io::{BufRead, BufReader, Seek};
use std::path::Path;

use tela_canvas::{TILE_SIZE, TileCoord, TiledSurface};
use tela_color::{PixelDepth, StoragePixel, with_pixel_type};
use tela_document::LayerPixels;

use crate::limits::check;
use crate::{IoError, Limits, Warning, guarded};

/// Pixels decoded from an image, ready for
/// `Document::add_layer_with_pixels`.
#[derive(Debug)]
pub struct ImportedImage {
    /// At the requested depth, only tiles with visible pixels stored.
    pub pixels: LayerPixels,
    pub width: u32,
    pub height: u32,
    /// Canvas position of the image's top-left pixel.
    pub origin: (i32, i32),
    pub warnings: Vec<Warning>,
}

/// A stored pixel made from straight RGBA in `0..=1`.
trait FromStraight: StoragePixel {
    fn from_straight(c: [f64; 4]) -> Self;
}

impl FromStraight for [u8; 4] {
    fn from_straight([r, g, b, a]: [f64; 4]) -> Self {
        [r * a, g * a, b * a, a].map(|v| (v * 255.0).round() as u8)
    }
}

impl FromStraight for [u16; 4] {
    fn from_straight([r, g, b, a]: [f64; 4]) -> Self {
        [r * a, g * a, b * a, a].map(|v| (v * 65535.0).round() as u16)
    }
}

/// A decoding error; `limits` names the decoder's memory limit if hit.
fn png_err(e: png::DecodingError, limits: &Limits) -> IoError {
    match e {
        png::DecodingError::IoError(e) if e.kind() != std::io::ErrorKind::UnexpectedEof => {
            IoError::Filesystem(e)
        }
        png::DecodingError::LimitsExceeded => IoError::LimitExceeded {
            what: "bytes to decode the image",
            limit: limits.max_image_decode_bytes,
            found: None,
        },
        other => IoError::InvalidImage(other.to_string()),
    }
}

/// Straight RGBA of one decoded row (`channels` samples per pixel of
/// `bits` bits, after `EXPAND`) into `out`.
fn convert_row<P: FromStraight>(data: &[u8], channels: usize, sixteen: bool, out: &mut [P]) {
    let sample = |i: usize| -> f64 {
        if sixteen {
            f64::from(u16::from_be_bytes([data[2 * i], data[2 * i + 1]])) / 65535.0
        } else {
            f64::from(data[i]) / 255.0
        }
    };
    for (x, p) in out.iter_mut().enumerate() {
        let s = x * channels;
        let c = match channels {
            1 => [sample(s), sample(s), sample(s), 1.0],
            2 => [sample(s), sample(s), sample(s), sample(s + 1)],
            3 => [sample(s), sample(s + 1), sample(s + 2), 1.0],
            _ => [sample(s), sample(s + 1), sample(s + 2), sample(s + 3)],
        };
        *p = P::from_straight(c);
    }
}

/// Copies one image row into the surface at canvas row `y`, starting at
/// column `x0`, allocating only tiles that receive a visible pixel.
fn place_row<P: StoragePixel>(surface: &mut TiledSurface<P>, row: &[P], x0: i32, y: i32) {
    let t = TILE_SIZE as i32;
    let end = x0 + row.len() as i32;
    let mut x = x0;
    while x < end {
        let c = TileCoord::containing(x, y);
        let (ox, oy) = c.origin();
        let seg_end = (ox + t).min(end);
        let src = &row[(x - x0) as usize..(seg_end - x0) as usize];
        if src.iter().any(|p| *p != P::default()) {
            let lx = (x - ox) as usize;
            let line = surface.tile_or_insert(c).row_mut((y - oy) as u32);
            line[lx..lx + src.len()].copy_from_slice(src);
        }
        x = seg_end;
    }
}

fn decode<P: FromStraight, R: BufRead + Seek>(
    mut reader: png::Reader<R>,
    origin: (i32, i32),
    channels: usize,
    sixteen: bool,
    limits: &Limits,
) -> Result<LayerPixels, IoError> {
    let (w, h) = reader.info().size();
    let mut surface = TiledSurface::<P>::new();
    let mut row = vec![P::default(); w as usize];
    if reader.info().interlaced {
        // Interlaced rows arrive pass by pass: decode the frame first.
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| IoError::InvalidImage("image too large".to_owned()))?;
        check(
            "bytes to decode the image",
            size as u64,
            limits.max_image_decode_bytes,
        )?;
        let mut buf = vec![0; size];
        let frame = reader
            .next_frame(&mut buf)
            .map_err(|e| png_err(e, limits))?;
        for (y, line) in buf
            .chunks_exact(frame.line_size)
            .take(h as usize)
            .enumerate()
        {
            convert_row(line, channels, sixteen, &mut row);
            place_row(&mut surface, &row, origin.0, origin.1 + y as i32);
        }
    } else {
        let mut y = 0;
        while let Some(line) = reader.next_row().map_err(|e| png_err(e, limits))? {
            if y >= h {
                break;
            }
            convert_row(line.data(), channels, sixteen, &mut row);
            place_row(&mut surface, &row, origin.0, origin.1 + y as i32);
            y += 1;
        }
        if y < h {
            return Err(IoError::InvalidImage(
                "the image data ends early".to_owned(),
            ));
        }
    }
    Ok(LayerPixels::from_surface(surface))
}

fn read_unguarded<R: BufRead + Seek>(
    r: R,
    canvas: (u32, u32),
    depth: PixelDepth,
    limits: &Limits,
) -> Result<ImportedImage, IoError> {
    let bytes = usize::try_from(limits.max_image_decode_bytes).unwrap_or(usize::MAX);
    let mut dec = png::Decoder::new_with_limits(r, png::Limits { bytes });
    dec.set_transformations(png::Transformations::EXPAND);
    let reader = dec.read_info().map_err(|e| png_err(e, limits))?;
    let info = reader.info();
    let (width, height) = info.size();
    let side = u64::from(width.max(height));
    check(
        "pixels of image side",
        side,
        u64::from(limits.max_image_side),
    )?;
    let pixels = u64::from(width) * u64::from(height);
    check("image pixels", pixels, limits.max_image_pixels)?;
    let mut warnings = Vec::new();
    let colour_info = info.srgb.is_none()
        && (info.icc_profile.is_some() || info.gama_chunk.is_some() || info.chrm_chunk.is_some());
    if colour_info || (info.srgb.is_some() && info.icc_profile.is_some()) {
        warnings.push(Warning::ColourSpaceIgnored);
    }
    if info.animation_control.is_some() {
        warnings.push(Warning::FirstFrameOnly);
    }
    let (colour, bits) = reader.output_color_type();
    let channels = colour.samples();
    let sixteen = bits == png::BitDepth::Sixteen;
    if !(1..=4).contains(&channels)
        || !matches!(bits, png::BitDepth::Eight | png::BitDepth::Sixteen)
    {
        return Err(IoError::InvalidImage(format!(
            "unexpected decoded format {colour:?} {bits:?}"
        )));
    }
    if sixteen && depth == PixelDepth::Rgba8 {
        warnings.push(Warning::ReducedTo8Bits);
    }
    let centre =
        |canvas: u32, image: u32| (i64::from(canvas) - i64::from(image)).div_euclid(2) as i32;
    let origin = (centre(canvas.0, width), centre(canvas.1, height));
    let pixels =
        with_pixel_type!(depth, P => decode::<P, R>(reader, origin, channels, sixteen, limits))?;
    Ok(ImportedImage {
        pixels,
        width,
        height,
        origin,
        warnings,
    })
}

/// Decodes a PNG for a document of size `canvas` and depth `depth`. Any
/// input gives an error or pixels, never a panic.
pub fn read_png<R: BufRead + Seek>(
    r: R,
    canvas: (u32, u32),
    depth: PixelDepth,
    limits: &Limits,
) -> Result<ImportedImage, IoError> {
    guarded(|| read_unguarded(r, canvas, depth, limits))
}

/// Reads the PNG at `path` (see [`read_png`]).
pub fn import_png(
    path: &Path,
    canvas: (u32, u32),
    depth: PixelDepth,
    limits: &Limits,
) -> Result<ImportedImage, IoError> {
    let file = std::fs::File::open(path)?;
    read_png(BufReader::new(file), canvas, depth, limits)
}
