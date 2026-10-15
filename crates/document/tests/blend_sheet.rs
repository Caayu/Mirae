//! Blend-sheet golden (phase 4C): every blend mode at 100 % and 60 % layer
//! opacity over a base of test swatches and the white paper, as displayed.
//! Guards Tela's own compositor against unintended change (the parity
//! with the reference is checked separately by `blend_parity.rs`).
//!
//! Update deliberately, after looking at the image:
//! `TELA_UPDATE_GOLDEN=1 cargo test --release -p tela-document --test blend_sheet`

use std::path::PathBuf;

use tela_canvas::{DirtyTiles, IRect, TILE_SIZE};
use tela_color::{PremulRgba, Rgba};
use tela_document::{BlendMode, Document, Placement};

/// The same swatches as the `blend_probe` example (straight RGBA).
const BOTTOM: [[u8; 4]; 10] = [
    [255, 255, 255, 255],
    [0, 0, 0, 255],
    [128, 128, 128, 255],
    [230, 30, 30, 255],
    [40, 90, 220, 255],
    [200, 160, 60, 255],
    [40, 180, 120, 128],
    [0, 0, 0, 0],
    [30, 40, 50, 255],
    [220, 230, 240, 255],
];
const TOP: [[u8; 4]; 10] = [
    [255, 255, 255, 255],
    [0, 0, 0, 255],
    [128, 128, 128, 255],
    [30, 200, 60, 255],
    [210, 40, 180, 255],
    [240, 150, 40, 255],
    [200, 60, 40, 96],
    [0, 0, 0, 0],
    [192, 192, 192, 255],
    [64, 64, 64, 255],
];
const CELL: u32 = 12;
const REGION: u32 = CELL * 10;
const PITCH: u32 = REGION + 8;
const SIZE: u32 = 8 + 6 * PITCH;

fn stored(c: [u8; 4]) -> [u8; 4] {
    let f = |v: u8| f32::from(v) / 255.0;
    Rgba::new(f(c[0]), f(c[1]), f(c[2]), f(c[3]))
        .premultiply()
        .to_rgba8()
}

fn region(i: usize) -> (u32, u32) {
    let i = i as u32;
    (8 + (i % 6) * PITCH, 8 + (i / 6) * PITCH)
}

/// Writes `color(x, y)` for every pixel of `rect` that it returns `Some` for.
fn paint(
    d: &mut Document,
    id: tela_document::LayerId,
    rect: IRect,
    color: impl Fn(u32, u32) -> Option<[u8; 4]>,
) {
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(rect);
    d.modify_tiles(id, &tiles, |c, t| {
        let (ox, oy) = c.origin();
        for y in 0..TILE_SIZE as i32 {
            for x in 0..TILE_SIZE as i32 {
                let (px, py) = (ox + x, oy + y);
                if px < rect.x0 || px >= rect.x1 || py < rect.y0 || py >= rect.y1 {
                    continue;
                }
                if let Some(v) = color(px as u32, py as u32) {
                    *t.get_mut(x as u32, y as u32) = stored(v);
                }
            }
        }
    })
    .unwrap();
}

fn render() -> Vec<u8> {
    let mut d = Document::new(SIZE, SIZE);
    let base = d.active_layer();
    let regions = BlendMode::ALL.len() * 2;
    for i in 0..regions {
        let (ox, oy) = region(i);
        let rect = IRect::new(
            ox as i32,
            oy as i32,
            (ox + REGION) as i32,
            (oy + REGION) as i32,
        );
        paint(&mut d, base, rect, |_, y| {
            Some(BOTTOM[((y - oy) / CELL) as usize])
        });
    }
    for (m, mode) in BlendMode::ALL.into_iter().enumerate() {
        for (o, opacity) in [1.0, 0.6].into_iter().enumerate() {
            let (ox, oy) = region(m * 2 + o);
            let rect = IRect::new(
                ox as i32,
                oy as i32,
                (ox + REGION) as i32,
                (oy + REGION) as i32,
            );
            let layer = d.add_layer(Placement::Top).unwrap();
            paint(&mut d, layer, rect, |x, _| {
                Some(TOP[((x - ox) / CELL) as usize])
            });
            d.set_opacity(layer, opacity).unwrap();
            d.set_blend_mode(layer, mode).unwrap();
        }
    }
    // As displayed: tiles over the white paper.
    let mut image = vec![255u8; (SIZE * SIZE * 3) as usize];
    let paper = Rgba::WHITE.premultiply();
    for coord in IRect::from_size(SIZE, SIZE).tiles() {
        let Some(tile) = d.compose_tile(coord, None) else {
            continue;
        };
        let (ox, oy) = coord.origin();
        for y in 0..TILE_SIZE as i32 {
            for x in 0..TILE_SIZE as i32 {
                let (px, py) = (ox + x, oy + y);
                if px >= SIZE as i32 || py >= SIZE as i32 {
                    continue;
                }
                let p = PremulRgba::from_rgba8(tile[(y * TILE_SIZE as i32 + x) as usize]);
                let shown = p.over(paper).to_rgba8();
                let i = ((py as u32 * SIZE + px as u32) * 3) as usize;
                image[i..i + 3].copy_from_slice(&shown[..3]);
            }
        }
    }
    image
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/blend_sheet.png")
}

#[test]
fn blend_sheet_matches_its_golden() {
    let image = render();
    let path = golden_path();
    if std::env::var("TELA_UPDATE_GOLDEN").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), SIZE, SIZE);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(&image)
            .unwrap();
        return;
    }
    let file = std::fs::File::open(&path)
        .unwrap_or_else(|_| panic!("missing golden {}; see the module docs", path.display()));
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .unwrap();
    let mut golden = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut golden).unwrap();
    assert_eq!(golden.len(), image.len(), "golden size");
    let worst = golden
        .iter()
        .zip(&image)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        worst <= 1,
        "blend sheet differs from its golden by up to {worst}/255"
    );
}

#[test]
fn rendering_is_deterministic() {
    assert_eq!(render(), render());
}
