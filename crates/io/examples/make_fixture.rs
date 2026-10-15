//! Writes a v1 compatibility fixture **once**:
//! `cargo run -p tela-io --example make_fixture -- <rgba8|rgba16> <out.mirae>`.
//!
//! The committed fixtures (`crates/io/tests/fixtures/v1/`) are a contract:
//! every future version must load them as `tests/fixture.rs` describes.
//! They are never regenerated; this program refuses to overwrite a file.

use std::io::Write;

use tela_canvas::{DirtyTiles, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, Document, LayerId, LayerPart, Placement};
use tela_io::{SaveOptions, write_document};

fn tiles(coords: &[(i32, i32)]) -> DirtyTiles {
    let mut t = DirtyTiles::new();
    for &(x, y) in coords {
        t.mark(TileCoord { x, y });
    }
    t
}

/// Fills tiles with `f(x, y)` (canvas pixel coordinates), premultiplied.
fn fill(d: &mut Document, id: LayerId, at: &[(i32, i32)], f: impl Fn(i32, i32) -> PremulRgba) {
    with_pixel_type!(d.depth(), P => {
        d.modify_tiles(id, &tiles(at), |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for y in 0..256 {
                for x in 0..256 {
                    *t.get_mut(x, y) = P::from_premul(f(ox + x as i32, oy + y as i32));
                }
            }
        })
        .unwrap();
    });
}

fn build(depth: PixelDepth) -> Document {
    let mut d = Document::with_depth(600, 400, depth);
    let bg = d.active_layer();
    d.set_name(bg, "Background").unwrap();
    // A horizontal ramp whose values fall between 8-bit levels.
    fill(
        &mut d,
        bg,
        &[(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)],
        |x, _| {
            let v = 0.2 + 0.6 * x as f32 / 600.0;
            PremulRgba::new(v, v * 0.9, v * 0.7, 1.0)
        },
    );
    let ink = d.add_layer(Placement::Top).unwrap();
    d.set_name(ink, "Ink").unwrap();
    // A translucent disc, partly off the canvas (tile x = -1).
    fill(&mut d, ink, &[(-1, 0), (0, 0)], |x, y| {
        let r = (((x - 40) * (x - 40) + (y - 120) * (y - 120)) as f32).sqrt();
        let a = (1.0 - r / 110.0).clamp(0.0, 1.0) * 0.8;
        PremulRgba::new(0.1 * a, 0.2 * a, 0.6 * a, a)
    });
    d.set_blend_mode(ink, BlendMode::Multiply).unwrap();
    d.set_opacity(ink, 0.75).unwrap();
    d.set_alpha_locked(ink, true).unwrap();
    let shade = d.add_layer(Placement::Top).unwrap();
    d.set_name(shade, "Clipped shade").unwrap();
    fill(&mut d, shade, &[(0, 0)], |x, y| {
        let a = ((x + y) % 64) as f32 / 64.0;
        PremulRgba::new(a, a * 0.5, 0.0, a)
    });
    d.set_clipped(shade, true).unwrap();
    d.set_blend_mode(shade, BlendMode::Screen).unwrap();
    d.set_opacity(shade, 0.5).unwrap();
    let gone = d.add_layer(Placement::Top).unwrap();
    d.remove_layer(gone).unwrap();
    let masked = d.add_layer(Placement::Top).unwrap();
    d.set_name(masked, "Masked — ✓").unwrap();
    fill(&mut d, masked, &[(1, 0), (1, 1)], |_, y| {
        PremulRgba::new(0.9, 0.1, 0.1, 1.0).scale(if y % 2 == 0 { 1.0 } else { 0.5 })
    });
    d.add_mask(masked).unwrap();
    d.modify_mask(masked, &tiles(&[(1, 0)]), |c, t| {
        let (ox, _) = c.origin();
        for y in 0..256u32 {
            for x in 0..256u32 {
                *t.get_mut(x, y) = ((ox as u32 + x) % 256) as u8;
            }
        }
    })
    .unwrap();
    let hidden = d.add_layer(Placement::Bottom).unwrap();
    d.set_name(hidden, "Hidden locked").unwrap();
    fill(&mut d, hidden, &[(2, 1)], |_, _| {
        PremulRgba::new(0.0, 0.5, 0.0, 0.5)
    });
    d.add_mask(hidden).unwrap();
    d.modify_mask(hidden, &tiles(&[(2, 1)]), |_, t| *t.get_mut(10, 10) = 0)
        .unwrap();
    d.set_mask_enabled(hidden, false).unwrap();
    d.set_visible(hidden, false).unwrap();
    d.set_locked(hidden, true).unwrap();
    d.set_active_layer(masked).unwrap();
    d.set_active_part(LayerPart::Mask).unwrap();
    d
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let depth = match args.get(1).map(String::as_str) {
        Some("rgba8") => PixelDepth::Rgba8,
        Some("rgba16") => PixelDepth::Rgba16,
        _ => return Err("usage: make_fixture <rgba8|rgba16> <out.mirae>".into()),
    };
    let path = args.get(2).ok_or("missing output path")?;
    let bytes = write_document(
        &build(depth),
        std::io::Cursor::new(Vec::new()),
        &SaveOptions::default(),
    )?
    .into_inner();
    // create_new: an existing fixture is never replaced.
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(&bytes)?;
    println!("{path}: {} bytes", bytes.len());
    Ok(())
}
