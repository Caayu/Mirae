//! Saving and loading `.mirae` documents (layout in [`crate::format`]).
//!
//! Both directions stream one tile at a time through one reused buffer:
//! saving never copies the document, loading builds each tile once, where
//! the document will keep it. The whole document is validated by
//! [`Document::from_parts`] before it is returned, so a load either
//! yields a valid document or nothing.

use std::collections::HashSet;
use std::io::{BufReader, Read, Seek, Write};
use std::path::Path;

use tela_canvas::{Tile, TileCoord, TiledSurface};
use tela_color::with_pixel_type;
use tela_document::{Document, DocumentParts, LayerParts, LayerPixels, MaskParts, RasterLayer};
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::format::{
    LAYERS_PREFIX, MANIFEST_ENTRY, MASK_TILE_BYTES, MIMETYPE, MIMETYPE_ENTRY, Part, TileCodec,
    pixel_tile_bytes, safe_entry_name, tile_entry,
};
use crate::limits::check;
use crate::manifest::{self, sorted};
use crate::{IoError, Limits, Warning, atomic, guarded};

/// How tile entries are compressed. The manifest is always deflated and
/// the `mimetype` entry always stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    /// No compression: fastest, largest.
    Stored,
    /// Deflate (RFC 1951) at `level` 1 (fastest) … 9 (smallest).
    Deflate { level: u8 },
}

impl Default for Compression {
    /// Deflate level 2, chosen by measurement (`io_bench`,
    /// docs/performance.md, 4F): within 2–6 % of the smallest file at
    /// 40–70 % of level 6's time; level 1 doubles 8-bit files, higher
    /// levels cost up to 7× the time for a few percent.
    fn default() -> Self {
        Compression::Deflate { level: 2 }
    }
}

/// How a document is saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SaveOptions {
    pub compression: Compression,
    /// A document beyond these is refused: it could not be loaded again.
    pub limits: Limits,
}

/// A document read from a file, and what the reader did not take over.
#[derive(Debug)]
pub struct Loaded {
    pub document: Document,
    /// Unknown fields and entries that were ignored (never anything that
    /// changes how the document looks: that is an error).
    pub warnings: Vec<Warning>,
}

fn zip_write_err(e: ZipError) -> IoError {
    match e {
        ZipError::Io(e) => IoError::Filesystem(e),
        other => IoError::Filesystem(std::io::Error::other(other.to_string())),
    }
}

fn zip_read_err(e: ZipError) -> IoError {
    match e {
        ZipError::Io(e) if e.kind() != std::io::ErrorKind::UnexpectedEof => IoError::Filesystem(e),
        other => IoError::MalformedArchive(other.to_string()),
    }
}

/// Refuses a document the reader would refuse with the same limits.
fn check_limits(doc: &Document, limits: &Limits) -> Result<(), IoError> {
    let side = u64::from(doc.width().max(doc.height()));
    check(
        "pixels of canvas side",
        side,
        u64::from(limits.max_canvas_side),
    )?;
    check("layers", doc.layer_count() as u64, limits.max_layers as u64)?;
    let (mut tiles, mut bytes) = (0u64, 0u64);
    for l in doc.layers() {
        let name = l.name().len() as u64;
        check("bytes of layer name", name, limits.max_name_bytes as u64)?;
        let pixels = l.pixels().tile_count() as u64;
        let masks = l.mask().map_or(0, |m| m.tile_count()) as u64;
        tiles += pixels + masks;
        bytes += pixels * pixel_tile_bytes(doc.depth()) as u64 + masks * MASK_TILE_BYTES as u64;
    }
    check("tiles", tiles, limits.max_tiles as u64)?;
    check("bytes of pixel data", bytes, limits.max_pixel_bytes)
}

fn write_pixels<P: TileCodec, W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    layer: &RasterLayer,
    options: SimpleFileOptions,
    buf: &mut Vec<u8>,
) -> Result<(), IoError> {
    let surface = layer
        .pixels()
        .surface::<P>()
        .expect("every layer is at the document's depth");
    for c in sorted(surface.tile_coords()) {
        zip.start_file(tile_entry(layer.id().get(), Part::Pixels, c), options)
            .map_err(zip_write_err)?;
        buf.clear();
        P::encode(surface.tile(c).expect("listed coordinate"), buf);
        zip.write_all(buf)?;
    }
    Ok(())
}

/// Writes `doc` as a `.mirae` archive to `out` and returns `out`. The
/// history is not saved. The output is deterministic: the same document
/// gives the same bytes.
pub fn write_document<W: Write + Seek>(
    doc: &Document,
    out: W,
    options: &SaveOptions,
) -> Result<W, IoError> {
    check_limits(doc, &options.limits)?;
    let mut zip = ZipWriter::new(out);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));
    let tiles = match options.compression {
        Compression::Stored => stored,
        Compression::Deflate { level } => SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(i64::from(level.clamp(1, 9)))),
    };
    zip.start_file(MIMETYPE_ENTRY, stored)
        .map_err(zip_write_err)?;
    zip.write_all(MIMETYPE.as_bytes())?;
    zip.start_file(MANIFEST_ENTRY, deflated)
        .map_err(zip_write_err)?;
    zip.write_all(&manifest::write(doc))?;
    let mut buf = Vec::with_capacity(pixel_tile_bytes(doc.depth()));
    for layer in doc.layers() {
        with_pixel_type!(doc.depth(), P => write_pixels::<P, W>(&mut zip, layer, tiles, &mut buf))?;
        if let Some(mask) = layer.mask() {
            for c in sorted(mask.tile_coords()) {
                zip.start_file(tile_entry(layer.id().get(), Part::Mask, c), tiles)
                    .map_err(zip_write_err)?;
                buf.clear();
                buf.extend(mask.reveal_tile(c).expect("listed coordinate"));
                zip.write_all(&buf)?;
            }
        }
    }
    zip.finish().map_err(zip_write_err)
}

/// Saves `doc` to `path`, replacing the file atomically: if anything
/// fails, the previous file is left as it was (see `atomic`).
pub fn save_document(doc: &Document, path: &Path, options: &SaveOptions) -> Result<(), IoError> {
    check_limits(doc, &options.limits)?;
    atomic::write_file(path, |w| write_document(doc, w, options).map(drop))
}

/// Reads a whole entry of at most `max` bytes; `None` if it is absent.
fn read_small<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    what: &'static str,
    max: u64,
) -> Result<Option<Vec<u8>>, IoError> {
    let mut f = match zip.by_name(name) {
        Ok(f) => f,
        Err(ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(zip_read_err(e)),
    };
    check_entry(name, f.compression(), f.encrypted())?;
    check(what, f.size(), max)?;
    let mut v = Vec::new();
    (&mut f)
        .take(max + 1)
        .read_to_end(&mut v)
        .map_err(|e| IoError::MalformedArchive(format!("`{name}`: {e}")))?;
    check(what, v.len() as u64, max)?;
    Ok(Some(v))
}

fn check_entry(name: &str, method: CompressionMethod, encrypted: bool) -> Result<(), IoError> {
    if encrypted {
        return Err(IoError::MalformedArchive(format!("`{name}` is encrypted")));
    }
    if !matches!(
        method,
        CompressionMethod::Stored | CompressionMethod::Deflated
    ) {
        return Err(IoError::MalformedArchive(format!(
            "`{name}` uses compression {method:?} (only stored and deflate)"
        )));
    }
    Ok(())
}

/// Reads tile entry `name` into `buf`: exactly `size` bytes, checksum
/// verified (the ZIP reader checks it when the entry is read to its end).
fn read_tile<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    size: usize,
    buf: &mut Vec<u8>,
) -> Result<(), IoError> {
    let corrupt = |problem: String| IoError::CorruptTile {
        entry: name.to_owned(),
        problem,
    };
    let mut f = match zip.by_name(name) {
        Ok(f) => f,
        Err(ZipError::FileNotFound) => return Err(corrupt("missing".to_owned())),
        Err(e) => return Err(corrupt(e.to_string())),
    };
    check_entry(name, f.compression(), f.encrypted())?;
    if f.size() != size as u64 {
        return Err(corrupt(format!("{} bytes, expected {size}", f.size())));
    }
    buf.clear();
    (&mut f)
        .take(size as u64 + 1)
        .read_to_end(buf)
        .map_err(|e| corrupt(e.to_string()))?;
    if buf.len() != size {
        return Err(corrupt(format!("{} bytes, expected {size}", buf.len())));
    }
    Ok(())
}

fn read_pixels<P: TileCodec, R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    layer: u32,
    coords: &[TileCoord],
    buf: &mut Vec<u8>,
) -> Result<LayerPixels, IoError> {
    let mut surface = TiledSurface::<P>::new();
    for &c in coords {
        read_tile(
            zip,
            &tile_entry(layer, Part::Pixels, c),
            pixel_tile_bytes(P::DEPTH),
            buf,
        )?;
        let mut tile = Tile::new();
        P::decode(buf, &mut tile);
        surface.insert_tile(c, tile);
    }
    Ok(LayerPixels::from_surface(surface))
}

fn read_mask<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    layer: u32,
    coords: &[TileCoord],
    buf: &mut Vec<u8>,
) -> Result<TiledSurface<u8>, IoError> {
    let mut surface = TiledSurface::new();
    for &c in coords {
        read_tile(zip, &tile_entry(layer, Part::Mask, c), MASK_TILE_BYTES, buf)?;
        let mut tile = Tile::new();
        tile.pixels_mut().copy_from_slice(buf);
        surface.insert_tile(c, tile);
    }
    Ok(surface)
}

fn read_unguarded<R: Read + Seek>(mut r: R, limits: &Limits) -> Result<Loaded, IoError> {
    let declared = crate::zipcheck::declared_entries(&mut r, limits)?;
    let mut zip = ZipArchive::new(r).map_err(|e| match e {
        ZipError::InvalidArchive(_) | ZipError::UnsupportedArchive(_) => {
            IoError::MalformedArchive(e.to_string())
        }
        other => zip_read_err(other),
    })?;
    if zip.len() as u64 != declared {
        return Err(IoError::MalformedArchive(
            "duplicate entry names".to_owned(),
        ));
    }
    let mut names = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let name = zip
            .name_for_index(i)
            .and_then(Result::ok)
            .ok_or_else(|| IoError::MalformedArchive("an entry name is not UTF-8".to_owned()))?;
        if !safe_entry_name(&name) {
            return Err(IoError::MalformedArchive(format!(
                "unsafe entry name {:?}",
                name.as_ref()
            )));
        }
        names.push(name.into_owned());
    }
    let mime = read_small(&mut zip, MIMETYPE_ENTRY, "bytes of mimetype", 256)?;
    if mime.as_deref() != Some(MIMETYPE.as_bytes()) {
        return Err(IoError::NotADocument);
    }
    let bytes = read_small(
        &mut zip,
        MANIFEST_ENTRY,
        "bytes of manifest",
        limits.max_manifest_bytes,
    )?
    .ok_or_else(|| IoError::InvalidManifest("`manifest.json` is missing".to_owned()))?;
    let mut warnings = Vec::new();
    let m = manifest::read(&bytes, limits, &mut warnings)?;
    drop(bytes);

    // Every entry must be accounted for: tiles the manifest does not list
    // would be data silently dropped.
    let mut expected = HashSet::new();
    for l in &m.layers {
        expected.extend(l.pixels.iter().map(|&c| tile_entry(l.id, Part::Pixels, c)));
        if let Some((_, tiles)) = &l.mask {
            expected.extend(tiles.iter().map(|&c| tile_entry(l.id, Part::Mask, c)));
        }
    }
    for name in &names {
        if name == MIMETYPE_ENTRY || name == MANIFEST_ENTRY || expected.contains(name) {
            continue;
        }
        if name.starts_with(LAYERS_PREFIX) && !name.ends_with('/') {
            return Err(IoError::MalformedArchive(format!(
                "entry `{name}` is not listed in the manifest"
            )));
        }
        if !name.ends_with('/') {
            warnings.push(Warning::UnknownEntry(name.clone()));
        }
    }
    drop(expected);

    let mut buf = Vec::with_capacity(pixel_tile_bytes(m.depth) + 1);
    let mut layers = Vec::with_capacity(m.layers.len());
    for l in m.layers {
        let pixels = with_pixel_type!(m.depth, P => read_pixels::<P, _>(&mut zip, l.id, &l.pixels, &mut buf))?;
        let mask = match &l.mask {
            None => None,
            Some((enabled, tiles)) => Some(MaskParts {
                enabled: *enabled,
                reveal: read_mask(&mut zip, l.id, tiles, &mut buf)?,
            }),
        };
        layers.push(LayerParts {
            id: l.id,
            name: l.name,
            visible: l.visible,
            opacity: l.opacity,
            blend_mode: l.blend_mode,
            locked: l.locked,
            alpha_locked: l.alpha_locked,
            clipped: l.clipped,
            pixels,
            mask,
        });
    }
    let document = Document::from_parts(DocumentParts {
        width: m.width,
        height: m.height,
        background: m.paper,
        depth: m.depth,
        layers,
        active_layer: m.active_layer,
        active_part: m.active_part,
        next_layer_id: m.next_layer_id,
    })
    .map_err(IoError::InvalidDocument)?;
    Ok(Loaded { document, warnings })
}

/// Reads a `.mirae` document. Any input — truncated, hostile, from a newer
/// version — gives an error or a valid document, never a panic.
pub fn read_document<R: Read + Seek>(r: R, limits: &Limits) -> Result<Loaded, IoError> {
    guarded(|| read_unguarded(r, limits))
}

/// Loads the `.mirae` document at `path` (see [`read_document`]).
pub fn load_document(path: &Path, limits: &Limits) -> Result<Loaded, IoError> {
    let file = std::fs::File::open(path)?;
    read_document(BufReader::with_capacity(1 << 20, file), limits)
}
