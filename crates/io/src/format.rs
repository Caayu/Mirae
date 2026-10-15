//! The `.mirae` v1 layout (docs/formats/mirae-v1.md, ADR 0019):
//!
//! ```text
//! mimetype                         "application/x-mirae", stored, first
//! manifest.json                    UTF-8 JSON, the document's structure
//! layers/<id>/pixels/<x>_<y>       one pixel tile, raw
//! layers/<id>/mask/<x>_<y>         one mask tile, raw
//! ```
//!
//! A pixel tile is 256 × 256 pixels, row-major, top row first; each pixel
//! R, G, B, A, premultiplied, sRGB-encoded, as stored in memory: one byte
//! per channel in 8-bit documents (262 144 bytes), two bytes
//! **little-endian** per channel in 16-bit documents (524 288 bytes). A
//! mask tile is 256 × 256 bytes of reveal values (255 shows the layer,
//! 0 hides it; 65 536 bytes). Tiles are listed in the manifest; a tile
//! that is not listed does not exist (transparent pixels, a mask that
//! shows everything). Compression comes from the ZIP container.

use tela_canvas::{TILE_SIZE, Tile, TileCoord};
use tela_color::PixelDepth;

/// Content of the `mimetype` entry: identifies a Mirae document.
pub const MIMETYPE: &str = "application/x-mirae";
/// Name of the identification entry.
pub(crate) const MIMETYPE_ENTRY: &str = "mimetype";
/// Name of the manifest entry.
pub(crate) const MANIFEST_ENTRY: &str = "manifest.json";
/// Value of the manifest's `format` field.
pub(crate) const FORMAT_NAME: &str = "mirae";
/// The format version this build writes and the newest it reads.
pub const FORMAT_VERSION: u32 = 1;
/// Prefix of every tile entry; entries under it must be listed.
pub(crate) const LAYERS_PREFIX: &str = "layers/";

pub(crate) const TILE_PIXELS: usize = (TILE_SIZE * TILE_SIZE) as usize;
/// Bytes of a mask tile entry.
pub(crate) const MASK_TILE_BYTES: usize = TILE_PIXELS;

/// Bytes of a pixel tile entry at `depth`.
pub(crate) const fn pixel_tile_bytes(depth: PixelDepth) -> usize {
    TILE_PIXELS * depth.bytes_per_pixel()
}

/// Which surface of a layer a tile belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Pixels,
    Mask,
}

/// The entry name of one tile.
pub(crate) fn tile_entry(layer: u32, part: Part, c: TileCoord) -> String {
    let part = match part {
        Part::Pixels => "pixels",
        Part::Mask => "mask",
    };
    format!("{LAYERS_PREFIX}{layer}/{part}/{}_{}", c.x, c.y)
}

/// A stored pixel type and its byte encoding in a tile entry.
pub(crate) trait TileCodec: tela_color::StoragePixel {
    /// Appends `tile` to `out` (exactly [`pixel_tile_bytes`] bytes).
    fn encode(tile: &Tile<Self>, out: &mut Vec<u8>);
    /// Fills `tile` from `bytes` (exactly [`pixel_tile_bytes`] bytes).
    fn decode(bytes: &[u8], tile: &mut Tile<Self>);
}

impl TileCodec for [u8; 4] {
    fn encode(tile: &Tile<Self>, out: &mut Vec<u8>) {
        out.extend(tile.pixels().iter().flatten());
    }

    fn decode(bytes: &[u8], tile: &mut Tile<Self>) {
        let (px, _) = bytes.as_chunks::<4>();
        tile.pixels_mut().copy_from_slice(px);
    }
}

impl TileCodec for [u16; 4] {
    fn encode(tile: &Tile<Self>, out: &mut Vec<u8>) {
        for p in tile.pixels() {
            for c in p {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
    }

    fn decode(bytes: &[u8], tile: &mut Tile<Self>) {
        let (px, _) = bytes.as_chunks::<8>();
        for (d, s) in tile.pixels_mut().iter_mut().zip(px) {
            *d = [0, 2, 4, 6].map(|i| u16::from_le_bytes([s[i], s[i + 1]]));
        }
    }
}

/// Whether an archive entry name is one this format could ever contain:
/// relative, `/`-separated, no empty, `.` or `..` components, no
/// backslashes, drive letters or control characters. Mirae never extracts
/// entries to disk, but a name that could escape a directory marks a file
/// that was not written by it.
pub(crate) fn safe_entry_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 1024
        && !name.starts_with('/')
        && !name.contains(['\\', ':'])
        && !name.chars().any(char::is_control)
        && name
            .trim_end_matches('/')
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_entries_name_layer_part_and_signed_coordinates() {
        let c = TileCoord { x: -3, y: 12 };
        assert_eq!(tile_entry(7, Part::Pixels, c), "layers/7/pixels/-3_12");
        assert_eq!(tile_entry(7, Part::Mask, c), "layers/7/mask/-3_12");
        assert_eq!(pixel_tile_bytes(PixelDepth::Rgba8), 262_144);
        assert_eq!(pixel_tile_bytes(PixelDepth::Rgba16), 524_288);
        assert_eq!(MASK_TILE_BYTES, 65_536);
    }

    #[test]
    fn codecs_round_trip_and_16_bit_is_little_endian() {
        let mut t: Tile<[u16; 4]> = Tile::new();
        *t.get_mut(0, 0) = [0x0102, 0x0304, 0xfffe, 0x8000];
        *t.get_mut(255, 255) = [1, 2, 3, 65535];
        let mut bytes = Vec::new();
        <[u16; 4]>::encode(&t, &mut bytes);
        assert_eq!(bytes.len(), pixel_tile_bytes(PixelDepth::Rgba16));
        assert_eq!(&bytes[..8], &[2, 1, 4, 3, 0xfe, 0xff, 0, 0x80]);
        let mut back = Tile::new();
        <[u16; 4]>::decode(&bytes, &mut back);
        assert_eq!(back, t);

        let mut t8: Tile<[u8; 4]> = Tile::new();
        *t8.get_mut(3, 1) = [9, 8, 7, 6];
        let mut bytes = Vec::new();
        <[u8; 4]>::encode(&t8, &mut bytes);
        assert_eq!(bytes.len(), pixel_tile_bytes(PixelDepth::Rgba8));
        assert_eq!(&bytes[(256 + 3) * 4..(256 + 4) * 4], &[9, 8, 7, 6]);
        let mut back = Tile::new();
        <[u8; 4]>::decode(&bytes, &mut back);
        assert_eq!(back, t8);
    }

    #[test]
    fn hostile_entry_names_are_recognized() {
        for ok in [
            "mimetype",
            "manifest.json",
            "layers/1/pixels/0_0",
            "layers/",
            "a/b/",
        ] {
            assert!(safe_entry_name(ok), "{ok}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "layers/../../x",
            "layers/./1",
            "a//b",
            "C:\\x",
            "c:x",
            "a\\b",
            "a\u{0}b",
            "a\nb",
        ] {
            assert!(!safe_entry_name(bad), "{bad:?}");
        }
        assert!(!safe_entry_name(&"a".repeat(1025)));
    }
}
