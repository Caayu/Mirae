//! PNG export of the document's composite (parity record 4F, F12–F14).
//!
//! The canvas is composed one row of tiles at a time with the document's
//! own compositor (`compose_tile_into`, the same code the screen uses) and
//! finished in `f32` before it is quantized once:
//!
//! - **with the paper**: the composite over the opaque paper colour, as the
//!   reference's export with its paper layer shown (F14); alpha 255;
//! - **transparent**: the composite un-premultiplied (straight alpha), as
//!   the reference's export without paper (F13). Tiles where a layer in a
//!   blend mode other than Normal meets the stack are composed over the
//!   paper (ADR 0016); how the reference blends them over a translucent
//!   backdrop is not known yet (D-4C-1), so a transparent export of such a
//!   document is refused instead of invented.
//!
//! The file is RGBA, 8 or 16 bits per channel, with an `sRGB` chunk
//! (PNG 3rd edition §11.3.2.5) declaring the values' colour space; no
//! resolution chunk (documents have no resolution yet).

use std::io::Write;
use std::path::Path;

use tela_canvas::{TILE_SIZE, TileCoord};
use tela_color::{PremulRgba, unorm8, unorm16};
use tela_document::{ComposeScratch, Document};

use crate::limits::check;
use crate::{IoError, Limits, atomic};

/// What lies under the layers in the exported image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PngBackground {
    /// The document's paper colour: an opaque image, as on screen.
    #[default]
    Paper,
    /// Nothing: transparent where no layer covers the canvas.
    Transparent,
}

/// Bits per channel of the exported image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PngBits {
    /// 8 bits per channel: the reference's only export depth (4E, E3).
    #[default]
    Eight,
    /// 16 bits per channel: keeps a 16-bit document's precision (the
    /// composite is quantized once, from `f32`).
    Sixteen,
}

/// How a document is exported to PNG.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PngExport {
    pub background: PngBackground,
    pub bits: PngBits,
}

fn png_err(e: png::EncodingError) -> IoError {
    match e {
        png::EncodingError::IoError(e) => IoError::Filesystem(e),
        other => IoError::InvalidImage(other.to_string()),
    }
}

/// The tiles covering the canvas, as `(columns, rows)`.
fn tile_grid(doc: &Document) -> (i32, i32) {
    let t = TILE_SIZE;
    (
        doc.width().div_ceil(t) as i32,
        doc.height().div_ceil(t) as i32,
    )
}

/// One exported pixel: straight RGBA in `0..=1`.
fn finish(p: PremulRgba, paper: Option<PremulRgba>) -> [f32; 4] {
    match paper {
        Some(paper) => {
            let c = p.over(paper);
            [c.r, c.g, c.b, 1.0]
        }
        None => {
            let s = p.unpremultiply();
            [s.r, s.g, s.b, s.a]
        }
    }
}

/// Writes the document's composite as a PNG to `out`.
pub fn write_png<W: Write>(doc: &Document, out: W, options: &PngExport) -> Result<(), IoError> {
    let side = u64::from(doc.width().max(doc.height()));
    check(
        "pixels of canvas side",
        side,
        u64::from(Limits::default().max_canvas_side),
    )?;
    let (cols, rows) = tile_grid(doc);
    let paper = match options.background {
        PngBackground::Paper => Some(doc.background().premultiply()),
        PngBackground::Transparent => {
            let blends = (0..rows)
                .flat_map(|y| (0..cols).map(move |x| TileCoord { x, y }))
                .any(|c| doc.paper_backdrop(c));
            if blends {
                return Err(IoError::Unsupported(
                    "a transparent export of layers in blend modes other than Normal \
                     (how they combine without the paper is not known yet, D-4C-1); \
                     export with the paper instead"
                        .to_owned(),
                ));
            }
            None
        }
    };
    let (width, height) = (doc.width() as usize, doc.height() as usize);
    let sixteen = options.bits == PngBits::Sixteen;
    let bytes_per_pixel = if sixteen { 8 } else { 4 };
    let mut enc = png::Encoder::new(out, doc.width(), doc.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(if sixteen {
        png::BitDepth::Sixteen
    } else {
        png::BitDepth::Eight
    });
    enc.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let mut writer = enc.write_header().map_err(png_err)?;
    let row_bytes = width * bytes_per_pixel;
    let mut stream = writer.stream_writer_with_size(row_bytes).map_err(png_err)?;
    let t = TILE_SIZE as usize;
    let mut strip = vec![0u8; row_bytes * t];
    let mut scratch = ComposeScratch::new();
    for ty in 0..rows {
        let strip_rows = t.min(height - ty as usize * t);
        for tx in 0..cols {
            let c = TileCoord { x: tx, y: ty };
            let drawn = doc.compose_tile_into(c, None, &mut scratch);
            let x0 = tx as usize * t;
            let cols_here = t.min(width - x0);
            for y in 0..strip_rows {
                let line = &mut strip[y * row_bytes..(y + 1) * row_bytes];
                for x in 0..cols_here {
                    let p = if drawn {
                        scratch.premultiplied()[y * t + x]
                    } else {
                        PremulRgba::TRANSPARENT
                    };
                    let px = finish(p, paper);
                    let at = (x0 + x) * bytes_per_pixel;
                    if sixteen {
                        for (i, v) in px.into_iter().enumerate() {
                            line[at + 2 * i..at + 2 * i + 2]
                                .copy_from_slice(&unorm16(v).to_be_bytes());
                        }
                    } else {
                        for (i, v) in px.into_iter().enumerate() {
                            line[at + i] = unorm8(v);
                        }
                    }
                }
            }
        }
        stream.write_all(&strip[..strip_rows * row_bytes])?;
    }
    stream.finish().map_err(png_err)?;
    Ok(())
}

/// Exports to `path`, replacing it atomically.
pub fn export_png(doc: &Document, path: &Path, options: &PngExport) -> Result<(), IoError> {
    atomic::write_file(path, |w| write_png(doc, w, options))
}
