use tela_canvas::TiledSurface;
use tela_color::PremulRgba;

/// How a layer combines with what is below it. Only `Normal` exists until the
/// layer phase adds the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    #[default]
    Normal,
}

impl BlendMode {
    /// Blends `src` (scaled by `opacity`) onto `dst` in place.
    pub fn composite(self, src: &[PremulRgba], opacity: f32, dst: &mut [PremulRgba]) {
        match self {
            BlendMode::Normal => {
                for (d, s) in dst.iter_mut().zip(src) {
                    if s.a > 0.0 {
                        *d = s.scale(opacity).over(*d);
                    }
                }
            }
        }
    }
}

/// Pixel layer stored as premultiplied sRGB RGBA8 tiles.
#[derive(Clone, Debug)]
pub struct RasterLayer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub pixels: TiledSurface<[u8; 4]>,
}

impl RasterLayer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            visible: true,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            pixels: TiledSurface::new(),
        }
    }
}
