//! GPU tests on an offscreen target. Skipped (with a message) when the
//! machine has no usable adapter.

use glam::Vec2;
use tela_canvas::{TILE_SIZE, TileCoord, ViewTransform};
use tela_renderer::{Frame, Renderer};

const PAPER: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const WORKSPACE: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

fn headless(w: u32, h: u32) -> Option<Renderer> {
    let r = pollster::block_on(Renderer::new_headless(w, h));
    if r.is_none() {
        eprintln!("no GPU adapter: skipping");
    }
    r
}

fn upload(r: &mut Renderer, width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) {
    r.set_document_size(width, height).unwrap();
    for ty in 0..height.div_ceil(TILE_SIZE) {
        for tx in 0..width.div_ceil(TILE_SIZE) {
            let tile: Vec<[u8; 4]> = (0..TILE_SIZE * TILE_SIZE)
                .map(|i| {
                    pixel(
                        tx * TILE_SIZE + i % TILE_SIZE,
                        ty * TILE_SIZE + i / TILE_SIZE,
                    )
                })
                .collect();
            r.upload_tile(
                TileCoord {
                    x: tx as i32,
                    y: ty as i32,
                },
                Some(&tile),
            );
        }
    }
}

fn render(r: &mut Renderer, view: &ViewTransform) -> Vec<[u8; 4]> {
    r.render(&Frame {
        view,
        paper: PAPER,
        workspace: WORKSPACE,
    });
    r.read_pixels().unwrap()
}

fn hash(x: u32, y: u32) -> u8 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    (h >> 24) as u8
}

#[test]
fn no_seams_when_minified_and_rotated() {
    let (w, h) = (400u32, 300u32);
    let Some(mut r) = headless(w, h) else { return };
    let color = [200, 40, 40, 255];
    upload(&mut r, 1024, 768, |_, _| color);
    for (zoom, rotation) in [(0.31, 0.37), (0.5, 0.0), (0.77, 1.1), (0.13, 2.0)] {
        let mut view = ViewTransform {
            zoom,
            rotation,
            ..Default::default()
        };
        view.pan = Vec2::new(w as f32, h as f32) * 0.5 - view.to_screen(Vec2::new(512.0, 384.0))
            + view.pan;
        let px = render(&mut r, &view);
        let mut checked = 0;
        for sy in 0..h {
            for sx in 0..w {
                let c = view.to_canvas(Vec2::new(sx as f32 + 0.5, sy as f32 + 0.5));
                let margin = 3.0 / zoom;
                if c.x < margin || c.y < margin || c.x > 1024.0 - margin || c.y > 768.0 - margin {
                    continue;
                }
                let p = px[(sy * w + sx) as usize];
                let d = p
                    .iter()
                    .zip(color)
                    .map(|(a, b)| a.abs_diff(b))
                    .max()
                    .unwrap();
                assert!(
                    d <= 1,
                    "seam at ({sx},{sy}) zoom {zoom} rot {rotation}: {p:?}"
                );
                checked += 1;
            }
        }
        assert!(checked > 1000);
    }
}

#[test]
fn identical_at_100_percent() {
    let (w, h) = (300u32, 280u32);
    let Some(mut r) = headless(w, h) else { return };
    let pixel = |x, y| {
        let v = hash(x, y);
        [v, v / 2, 255 - v, 255]
    };
    upload(&mut r, 600, 520, pixel);
    // Integer pan offsets the document by whole pixels.
    let view = ViewTransform {
        pan: Vec2::new(-200.0, -150.0),
        ..Default::default()
    };
    let px = render(&mut r, &view);
    for sy in 0..h {
        for sx in 0..w {
            assert_eq!(
                px[(sy * w + sx) as usize],
                pixel(sx + 200, sy + 150),
                "({sx},{sy})"
            );
        }
    }
}

#[test]
fn magnified_texels_are_flat_squares() {
    let (w, h) = (256u32, 256u32);
    let Some(mut r) = headless(w, h) else { return };
    let pixel = |x: u32, y: u32| {
        if (x + y).is_multiple_of(2) {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        }
    };
    upload(&mut r, 256, 256, pixel);
    let view = ViewTransform {
        zoom: 4.0,
        ..Default::default()
    };
    let px = render(&mut r, &view);
    for ty in 0..h / 4 {
        for tx in 0..w / 4 {
            let expected = pixel(tx, ty);
            for dy in 1..3 {
                for dx in 1..3 {
                    let p = px[((ty * 4 + dy) * w + tx * 4 + dx) as usize];
                    assert_eq!(p, expected, "texel ({tx},{ty}) blurred");
                }
            }
        }
    }
}
