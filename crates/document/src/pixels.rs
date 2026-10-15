//! A layer's pixel storage at the document's depth (docs/adr/0018).
//!
//! This module is the only place in the crate that matches on the depth:
//! everything else calls [`LayerPixels`] methods, which hand each variant
//! to one generic function, so per-pixel code is monomorphized for
//! `[u8; 4]` and `[u16; 4]` and the variant is chosen once per tile or per
//! call, never per pixel.

use std::any::Any;

use tela_canvas::{TILE_SIZE, Tile, TileCoord, TiledSurface};
use tela_color::{PixelDepth, PremulRgba, StoragePixel};

/// The pixels of one layer: premultiplied sRGB-encoded RGBA, sparse, at
/// the depth of the document that owns the layer.
///
/// Read-only outside this crate. The typed views ([`LayerPixels::rgba8`],
/// [`LayerPixels::rgba16`], [`LayerPixels::surface`]) give shared
/// references only; pixels change through
/// [`Document::modify_tiles`](crate::Document::modify_tiles).
#[derive(Clone, Debug)]
pub struct LayerPixels {
    storage: Storage,
}

#[derive(Clone, Debug)]
enum Storage {
    Rgba8(TiledSurface<[u8; 4]>),
    Rgba16(TiledSurface<[u16; 4]>),
}

/// Applies a generic expression to the surface of either variant.
macro_rules! each {
    ($storage:expr, $s:ident => $body:expr) => {
        match $storage {
            Storage::Rgba8($s) => $body,
            Storage::Rgba16($s) => $body,
        }
    };
}

/// `x` as a `U` when `T` is `U` (the stored type is known only as a
/// generic parameter at the call site).
fn cast_mut<T: 'static, U: 'static>(x: &mut T) -> Option<&mut U> {
    (x as &mut dyn Any).downcast_mut()
}

fn cast_ref<T: 'static, U: 'static>(x: &T) -> Option<&U> {
    (x as &dyn Any).downcast_ref()
}

/// Bytes of one tile's pixel buffer at `depth`.
pub(crate) const fn tile_bytes(depth: PixelDepth) -> usize {
    (TILE_SIZE * TILE_SIZE) as usize * depth.bytes_per_pixel()
}

impl LayerPixels {
    /// Empty pixels (no tile allocated).
    pub(crate) fn new(depth: PixelDepth) -> Self {
        let storage = match depth {
            PixelDepth::Rgba8 => Storage::Rgba8(TiledSurface::new()),
            PixelDepth::Rgba16 => Storage::Rgba16(TiledSurface::new()),
        };
        Self { storage }
    }

    /// Pixels made of a stored surface (e.g. decoded from a file or an
    /// image), at the depth `P` stores. A document accepts them only at
    /// its own depth ([`Document::from_parts`](crate::Document::from_parts),
    /// [`Document::add_layer_with_pixels`](crate::Document::add_layer_with_pixels)).
    pub fn from_surface<P: StoragePixel>(surface: TiledSurface<P>) -> Self {
        let any: Box<dyn Any> = Box::new(surface);
        let storage = match any.downcast::<TiledSurface<[u8; 4]>>() {
            Ok(s) => Storage::Rgba8(*s),
            Err(any) => Storage::Rgba16(
                *any.downcast::<TiledSurface<[u16; 4]>>()
                    .expect("StoragePixel is implemented for [u8; 4] and [u16; 4] only"),
            ),
        };
        Self { storage }
    }

    /// Bits per channel of the stored pixels (the document's depth).
    pub fn depth(&self) -> PixelDepth {
        match self.storage {
            Storage::Rgba8(_) => PixelDepth::Rgba8,
            Storage::Rgba16(_) => PixelDepth::Rgba16,
        }
    }

    /// Allocated tiles.
    pub fn tile_count(&self) -> usize {
        each!(&self.storage, s => s.tile_count())
    }

    /// Allocated tiles, in unspecified order.
    pub fn tile_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        let (a, b) = match &self.storage {
            Storage::Rgba8(s) => (Some(s.tile_coords()), None),
            Storage::Rgba16(s) => (None, Some(s.tile_coords())),
        };
        a.into_iter().flatten().chain(b.into_iter().flatten())
    }

    /// Whether the tile at `c` is allocated.
    pub fn has_tile(&self, c: TileCoord) -> bool {
        each!(&self.storage, s => s.tile(c).is_some())
    }

    /// One pixel, decoded (transparent where no tile is allocated).
    pub fn pixel(&self, x: i32, y: i32) -> PremulRgba {
        each!(&self.storage, s => s.pixel(x, y).to_premul())
    }

    /// One pixel at 8 bits per channel: exact in an 8-bit document, the
    /// nearest level in a 16-bit one.
    pub fn pixel_rgba8(&self, x: i32, y: i32) -> [u8; 4] {
        each!(&self.storage, s => s.pixel(x, y).to_rgba8())
    }

    /// The stored surface, if the document is 8-bit.
    pub fn rgba8(&self) -> Option<&TiledSurface<[u8; 4]>> {
        self.surface()
    }

    /// The stored surface, if the document is 16-bit.
    pub fn rgba16(&self) -> Option<&TiledSurface<[u16; 4]>> {
        self.surface()
    }

    /// The stored surface, if it stores `P`.
    pub fn surface<P: StoragePixel>(&self) -> Option<&TiledSurface<P>> {
        each!(&self.storage, s => cast_ref(s))
    }

    /// The stored surface for editing, if it stores `P`.
    pub(crate) fn surface_mut<P: StoragePixel>(&mut self) -> Option<&mut TiledSurface<P>> {
        each!(&mut self.storage, s => cast_mut(s))
    }

    /// Bytes of one allocated tile.
    pub(crate) fn tile_bytes(&self) -> usize {
        tile_bytes(self.depth())
    }

    /// Decodes tile `c` into `out` (transparent if not allocated).
    /// Returns whether the tile is allocated.
    pub(crate) fn decode_tile(&self, c: TileCoord, out: &mut [PremulRgba]) -> bool {
        each!(&self.storage, s => decode(s, c, out))
    }

    /// The same pixels at `depth` (exact widening or nearest-level
    /// narrowing, see `tela_color`); a copy even when the depth is equal.
    pub(crate) fn converted(&self, depth: PixelDepth) -> Self {
        let storage = match (&self.storage, depth) {
            (Storage::Rgba8(s), PixelDepth::Rgba8) => Storage::Rgba8(s.clone()),
            (Storage::Rgba16(s), PixelDepth::Rgba16) => Storage::Rgba16(s.clone()),
            (Storage::Rgba8(s), PixelDepth::Rgba16) => Storage::Rgba16(convert(s)),
            (Storage::Rgba16(s), PixelDepth::Rgba8) => Storage::Rgba8(convert(s)),
        };
        Self { storage }
    }

    /// Swaps the tiles held in `backup` with the stored ones (undo/redo).
    /// Returns the coordinates swapped.
    ///
    /// # Panics
    ///
    /// If `backup` holds the other depth: history replays only on the
    /// state it recorded, whose depth is the backup's.
    pub(crate) fn swap_tiles(
        &mut self,
        backup: &mut TileBackup,
        mut on_tile: impl FnMut(TileCoord),
    ) {
        match (&mut self.storage, backup) {
            (Storage::Rgba8(s), TileBackup::Rgba8(b)) => swap(s, b, &mut on_tile),
            (Storage::Rgba16(s), TileBackup::Rgba16(b)) => swap(s, b, &mut on_tile),
            _ => panic!("history replays on the state it recorded (depth mismatch)"),
        }
    }

    /// Hash of one allocated tile's stored values (test digests).
    #[cfg(test)]
    pub(crate) fn hash_tile(&self, c: TileCoord) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        each!(&self.storage, s => s.tile(c).map(|t| {
            let mut h = std::hash::DefaultHasher::new();
            t.pixels().hash(&mut h);
            h.finish()
        }))
    }
}

/// The previous state of some tiles of one layer, at the depth the layer
/// had when they were recorded (`None` = absent).
#[derive(Clone, Debug)]
pub(crate) enum TileBackup {
    Rgba8(Vec<(TileCoord, Option<Tile<[u8; 4]>>)>),
    Rgba16(Vec<(TileCoord, Option<Tile<[u16; 4]>>)>),
}

impl TileBackup {
    /// Wraps tiles of the stored type `P`.
    pub(crate) fn new<P: StoragePixel>(tiles: Vec<(TileCoord, Option<Tile<P>>)>) -> Self {
        let mut tiles = Some(tiles);
        if let Some(t) = cast_mut::<_, Option<Vec<(TileCoord, Option<Tile<[u8; 4]>>)>>>(&mut tiles)
        {
            return TileBackup::Rgba8(t.take().expect("set above"));
        }
        if let Some(t) = cast_mut::<_, Option<Vec<(TileCoord, Option<Tile<[u16; 4]>>)>>>(&mut tiles)
        {
            return TileBackup::Rgba16(t.take().expect("set above"));
        }
        unreachable!("tiles come from a layer surface, which stores [u8; 4] or [u16; 4]")
    }

    /// Bytes held: tile buffers plus one entry per tile.
    pub(crate) fn held_bytes(&self) -> usize {
        fn held<P: StoragePixel>(t: &[(TileCoord, Option<Tile<P>>)]) -> usize {
            size_of_val(t) + t.iter().filter(|(_, t)| t.is_some()).count() * tile_bytes(P::DEPTH)
        }
        match self {
            TileBackup::Rgba8(t) => held(t),
            TileBackup::Rgba16(t) => held(t),
        }
    }
}

fn decode<P: StoragePixel>(s: &TiledSurface<P>, c: TileCoord, out: &mut [PremulRgba]) -> bool {
    match s.tile(c) {
        Some(t) => {
            for (d, p) in out.iter_mut().zip(t.pixels()) {
                *d = p.to_premul();
            }
            true
        }
        None => {
            out.fill(PremulRgba::TRANSPARENT);
            false
        }
    }
}

/// Converts every tile through 8-bit values: widening is `v · 257`,
/// narrowing the nearest level (both exact integer rules, no `f32`).
fn convert<P: StoragePixel, Q: StoragePixel>(s: &TiledSurface<P>) -> TiledSurface<Q> {
    let mut out = TiledSurface::new();
    for c in s.tile_coords() {
        let src = s.tile(c).expect("listed coordinate");
        let mut dst: Tile<Q> = Tile::new();
        for (d, p) in dst.pixels_mut().iter_mut().zip(src.pixels()) {
            *d = convert_pixel(*p);
        }
        out.insert_tile(c, dst);
    }
    out
}

fn convert_pixel<P: StoragePixel, Q: StoragePixel>(p: P) -> Q {
    match (P::DEPTH, Q::DEPTH) {
        // Wider or equal: through the exact 8-bit value only when P is
        // 8-bit; otherwise the identity (same depth).
        (PixelDepth::Rgba8, _) => Q::from_rgba8(p.to_rgba8()),
        (PixelDepth::Rgba16, PixelDepth::Rgba8) => Q::from_rgba8(p.to_rgba8()),
        (PixelDepth::Rgba16, PixelDepth::Rgba16) => {
            *cast_ref::<P, Q>(&p).expect("same depth, same type")
        }
    }
}

fn swap<P: StoragePixel>(
    s: &mut TiledSurface<P>,
    backup: &mut [(TileCoord, Option<Tile<P>>)],
    on_tile: &mut impl FnMut(TileCoord),
) {
    for (coord, held) in backup.iter_mut() {
        let current = s.remove_tile(*coord);
        if let Some(tile) = held.take() {
            s.insert_tile(*coord, tile);
        }
        *held = current;
        on_tile(*coord);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface8() -> LayerPixels {
        let mut p = LayerPixels::new(PixelDepth::Rgba8);
        let s = p.surface_mut::<[u8; 4]>().unwrap();
        for v in 0..=255u8 {
            s.set_pixel(i32::from(v), 3, [v / 3, v / 2, v, v]);
        }
        s.set_pixel(-700, 900, [1, 2, 3, 4]);
        p
    }

    #[test]
    fn typed_views_match_the_depth_only() {
        let p = surface8();
        assert!(p.rgba8().is_some() && p.rgba16().is_none());
        let w = p.converted(PixelDepth::Rgba16);
        assert!(w.rgba16().is_some() && w.rgba8().is_none());
        assert_eq!(w.depth(), PixelDepth::Rgba16);
        let mut w = w;
        assert!(w.surface_mut::<[u8; 4]>().is_none());
    }

    #[test]
    fn widening_and_narrowing_keep_tiles_and_round_trip() {
        let p = surface8();
        let w = p.converted(PixelDepth::Rgba16);
        assert_eq!(w.tile_count(), p.tile_count());
        let mut a: Vec<_> = p.tile_coords().collect();
        let mut b: Vec<_> = w.tile_coords().collect();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "same sparse layout");
        for v in 0..=255u8 {
            let x = i32::from(v);
            assert_eq!(
                w.rgba16().unwrap().pixel(x, 3),
                [v / 3, v / 2, v, v].map(tela_color::widen8)
            );
            assert_eq!(w.pixel(x, 3), p.pixel(x, 3), "decodes bit-identically");
        }
        let back = w.converted(PixelDepth::Rgba8);
        for c in a {
            assert_eq!(
                back.rgba8().unwrap().tile(c),
                p.rgba8().unwrap().tile(c),
                "8 -> 16 -> 8 is the identity"
            );
        }
    }

    #[test]
    fn narrowing_rounds_each_channel_to_the_nearest_level() {
        let mut p = LayerPixels::new(PixelDepth::Rgba16);
        let s = p.surface_mut::<[u16; 4]>().unwrap();
        s.set_pixel(0, 0, [128, 129, 32896 + 128, 65535]);
        let n = p.converted(PixelDepth::Rgba8);
        assert_eq!(n.rgba8().unwrap().pixel(0, 0), [0, 1, 128, 255]);
        assert_eq!(p.pixel_rgba8(0, 0), [0, 1, 128, 255]);
    }

    #[test]
    fn backups_keep_their_depth() {
        let t: Vec<(TileCoord, Option<Tile<[u16; 4]>>)> = vec![(TileCoord { x: 0, y: 0 }, None)];
        let b = TileBackup::new(t);
        assert!(matches!(b, TileBackup::Rgba16(ref t) if t.len() == 1));
        let t8: Vec<(TileCoord, Option<Tile<[u8; 4]>>)> =
            vec![(TileCoord { x: 0, y: 0 }, Some(Tile::new()))];
        let b8 = TileBackup::new(t8);
        assert_eq!(
            b8.held_bytes(),
            tile_bytes(PixelDepth::Rgba8) + size_of::<(TileCoord, Option<Tile<[u8; 4]>>)>()
        );
        assert_eq!(
            tile_bytes(PixelDepth::Rgba16),
            2 * tile_bytes(PixelDepth::Rgba8)
        );
    }

    #[test]
    #[should_panic(expected = "depth mismatch")]
    fn swapping_a_backup_of_the_other_depth_is_a_bug() {
        let mut p = LayerPixels::new(PixelDepth::Rgba16);
        let mut b = TileBackup::new::<[u8; 4]>(vec![(TileCoord { x: 0, y: 0 }, None)]);
        p.swap_tiles(&mut b, |_| ());
    }
}
