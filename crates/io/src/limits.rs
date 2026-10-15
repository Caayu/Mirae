/// Upper bounds on what a file may declare. Every bound is checked
/// **before** the data it governs is read, so a hostile file cannot make
/// the reader allocate or decompress more than these allow; saving checks
/// the same bounds, so Mirae never writes a file it would refuse to read.
///
/// The defaults are generous for professional work on a desktop and are
/// documented in docs/formats/mirae-v1.md; an app may lower them (for
/// example to the memory it has).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Canvas width and height, pixels.
    pub max_canvas_side: u32,
    /// Layers in one document.
    pub max_layers: usize,
    /// Stored tiles in one document, pixel and mask tiles together.
    pub max_tiles: usize,
    /// Decoded pixel and mask data in one document, bytes (256 KiB per
    /// 8-bit tile, 512 KiB per 16-bit tile, 64 KiB per mask tile).
    pub max_pixel_bytes: u64,
    /// Bytes of a layer name (UTF-8).
    pub max_name_bytes: usize,
    /// Bytes of the manifest (`manifest.json`, decompressed).
    pub max_manifest_bytes: u64,
    /// Entries in the ZIP archive.
    pub max_entries: u64,
    /// Bytes of the ZIP central directory.
    pub max_directory_bytes: u64,
    /// Width and height of an imported image, pixels.
    pub max_image_side: u32,
    /// Pixels of an imported image.
    pub max_image_pixels: u64,
    /// Bytes the PNG decoder may allocate (one interlaced frame, or a row).
    pub max_image_decode_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_canvas_side: 65_536,
            max_layers: 10_000,
            max_tiles: 1_000_000,
            max_pixel_bytes: 16 << 30,
            max_name_bytes: 4096,
            max_manifest_bytes: 16 << 20,
            max_entries: 1_000_000 + 1024,
            max_directory_bytes: 256 << 20,
            max_image_side: 65_536,
            max_image_pixels: 1 << 28,
            max_image_decode_bytes: 2 << 30,
        }
    }
}

/// `Err(LimitExceeded)` when `found > limit`.
pub(crate) fn check(what: &'static str, found: u64, limit: u64) -> Result<(), crate::IoError> {
    if found > limit {
        return Err(crate::IoError::LimitExceeded {
            what,
            limit,
            found: Some(found),
        });
    }
    Ok(())
}
