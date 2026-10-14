//! Presents the composed canvas with wgpu.
//!
//! **Responsibility:** GPU device/surface, the display texture and its mips,
//! drawing the canvas under a [`ViewTransform`], an offscreen target with
//! [`Renderer::read_pixels`] for tests, and the [`FrameOverlay`] hook.
//! **Does not:** see or modify the document, layers, brushes or UI; it
//! only receives finished tiles. **Depends on:** canvas, wgpu, glam,
//! bytemuck, log. **Used by:** app; ui implements `FrameOverlay`.
//!
//! The renderer never sees the document model: callers hand it finished
//! premultiplied RGBA8 tiles and a view transform.
//!
//! On the GPU the whole document lives in one *display texture* with a mip
//! chain. Sampling a single continuous texture is what makes the canvas
//! seam-free at every zoom and rotation; per-tile textures cannot filter
//! across their borders. The CPU side stays sparse, and only dirty tiles
//! (and their mip levels, computed by [`mip`]) are uploaded.

pub mod mip;

use glam::{Mat4, Vec2, Vec3};
use tela_canvas::{TILE_SIZE, TileCoord, ViewTransform};
use wgpu::util::DeviceExt;

use crate::mip::{MipChain, TILE_LEVELS};

/// Why the renderer could not start or accept a document.
#[derive(Debug)]
pub enum RendererError {
    /// The window surface could not be created.
    Surface(wgpu::CreateSurfaceError),
    /// No GPU adapter fits.
    Adapter(wgpu::RequestAdapterError),
    /// The device could not be opened.
    Device(wgpu::RequestDeviceError),
    /// The adapter cannot present to this surface.
    UnsupportedSurface,
    /// The document exceeds the GPU's maximum texture size (no paging yet).
    #[allow(missing_docs)]
    DocumentTooLarge { width: u32, height: u32, max: u32 },
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(e) => write!(f, "cannot create surface: {e}"),
            Self::Adapter(e) => write!(f, "no suitable GPU adapter: {e}"),
            Self::Device(e) => write!(f, "cannot open GPU device: {e}"),
            Self::UnsupportedSurface => write!(f, "surface not supported by adapter"),
            Self::DocumentTooLarge { width, height, max } => {
                write!(
                    f,
                    "document {width}x{height} exceeds the GPU texture limit {max}"
                )
            }
        }
    }
}

impl std::error::Error for RendererError {}

/// What to draw this frame besides the canvas texture itself.
pub struct Frame<'a> {
    /// Canvas -> screen mapping.
    pub view: &'a ViewTransform,
    /// Paper color under the layers, straight-alpha sRGB `0..=1`.
    pub paper: [f32; 4],
    /// Background outside the document, straight-alpha sRGB `0..=1`.
    pub workspace: [f32; 4],
}

/// Extra drawing on top of the canvas (e.g. UI). Receives the frame's
/// encoder and target; the renderer knows nothing about what it draws.
pub trait FrameOverlay {
    /// Records drawing into `encoder` targeting `target` (`size` in
    /// physical pixels), after the canvas pass.
    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
    );
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    canvas_to_clip: [[f32; 4]; 4],
    paper: [f32; 4],
    doc_size: [f32; 4],
}

enum Target {
    Surface {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
    Offscreen {
        texture: wgpu::Texture,
    },
}

struct Display {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    width: u32,
    height: u32,
    levels: u32,
}

pub struct Renderer {
    target: Target,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    device: wgpu::Device,
    queue: wgpu::Queue,
    globals: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    display: Option<Display>,
    mips: MipChain,
    zeros: Vec<[u8; 4]>,
}

impl Renderer {
    /// Renderer presenting to a window surface.
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, RendererError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(target)
            .map_err(RendererError::Surface)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(RendererError::Adapter)?;
        log::info!("GPU adapter: {:?}", adapter.get_info());
        let (device, queue) = request_device(&adapter).await?;

        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or(RendererError::UnsupportedSurface)?;
        // Pixels are stored sRGB-encoded and blended in that space, so the
        // swapchain must not re-encode them.
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *f;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);
        let format = config.format;
        let size = [config.width, config.height];
        Ok(Self::with_target(
            Target::Surface { surface, config },
            format,
            size,
            device,
            queue,
        ))
    }

    /// Renderer drawing into an offscreen texture (tests, export). Returns
    /// `None` when no GPU adapter is available.
    pub async fn new_headless(width: u32, height: u32) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .ok()?;
        let (device, queue) = request_device(&adapter).await.ok()?;
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Some(Self::with_target(
            Target::Offscreen { texture },
            format,
            [width, height],
            device,
            queue,
        ))
    }

    fn with_target(
        target: Target,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        device: wgpu::Device,
        queue: wgpu::Queue,
    ) -> Self {
        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals"),
            contents: bytemuck::bytes_of(&Globals {
                canvas_to_clip: Mat4::IDENTITY.to_cols_array_2d(),
                paper: [1.0; 4],
                doc_size: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("canvas"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canvas"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("canvas"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_canvas"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_canvas"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            target,
            format,
            size,
            device,
            queue,
            globals,
            layout,
            sampler,
            pipeline,
            display: None,
            mips: MipChain::default(),
            zeros: vec![[0; 4]; (TILE_SIZE * TILE_SIZE) as usize],
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.size[0] as f32, self.size[1] as f32)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.size = [width, height];
        if let Target::Surface { surface, config } = &mut self.target {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
    }

    /// (Re)creates the display texture for a document. Returns `true` when
    /// the texture was recreated, in which case every non-empty tile must be
    /// uploaded again.
    pub fn set_document_size(&mut self, width: u32, height: u32) -> Result<bool, RendererError> {
        if self
            .display
            .as_ref()
            .is_some_and(|d| d.width == width && d.height == height)
        {
            return Ok(false);
        }
        let max = self.device.limits().max_texture_dimension_2d;
        if width > max || height > max || width == 0 || height == 0 {
            return Err(RendererError::DocumentTooLarge { width, height, max });
        }
        let full = 32 - width.max(height).leading_zeros();
        let levels = full.min(TILE_LEVELS);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("document display"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.display = Some(Display {
            texture,
            bind,
            width,
            height,
            levels,
        });
        Ok(true)
    }

    /// Replaces a tile's pixels (premultiplied RGBA8, 256x256 row-major), or
    /// clears it when `pixels` is `None`. Tiles outside the document are
    /// ignored.
    pub fn upload_tile(&mut self, coord: TileCoord, pixels: Option<&[[u8; 4]]>) {
        let Some(display) = &self.display else {
            return;
        };
        let level0 = pixels.unwrap_or(&self.zeros);
        if display.levels > 1 {
            self.mips.build(level0);
        }
        for k in 0..display.levels {
            let Some(region) = tile_region(coord, k, display.width, display.height) else {
                return;
            };
            let data = if k == 0 { level0 } else { self.mips.level(k) };
            let side = TILE_SIZE >> k;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &display.texture,
                    mip_level: k,
                    origin: wgpu::Origin3d {
                        x: region.0,
                        y: region.1,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 4),
                    rows_per_image: Some(side),
                },
                wgpu::Extent3d {
                    width: region.2,
                    height: region.3,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Draws the frame and presents it.
    pub fn render(&mut self, frame: &Frame) {
        self.render_with(frame, None);
    }

    pub fn render_with(&mut self, frame: &Frame, overlay: Option<&mut dyn FrameOverlay>) {
        let (surface_texture, view) = match &self.target {
            Target::Surface { surface, config } => match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                    let v = t.texture.create_view(&Default::default());
                    (Some(t), v)
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    surface.configure(&self.device, config);
                    return;
                }
                other => {
                    log::warn!("skipping frame: {other:?}");
                    return;
                }
            },
            Target::Offscreen { texture } => (None, texture.create_view(&Default::default())),
        };

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let [r, g, b, a] = frame.workspace.map(f64::from);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            if let Some(display) = &self.display {
                self.queue.write_buffer(
                    &self.globals,
                    0,
                    bytemuck::bytes_of(&Globals {
                        canvas_to_clip: canvas_to_clip(frame.view, self.size()).to_cols_array_2d(),
                        paper: frame.paper,
                        doc_size: [display.width as f32, display.height as f32, 0.0, 0.0],
                    }),
                );
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &display.bind, &[]);
                pass.draw(0..6, 0..1);
            }
        }
        if let Some(overlay) = overlay {
            overlay.draw(&self.device, &self.queue, &mut encoder, &view, self.size);
        }
        self.queue.submit([encoder.finish()]);
        if let Some(t) = surface_texture {
            self.queue.present(t);
        }
    }

    /// Reads back the offscreen target as RGBA8 rows (headless only).
    pub fn read_pixels(&self) -> Option<Vec<[u8; 4]>> {
        let Target::Offscreen { texture } = &self.target else {
            return None;
        };
        let [w, h] = self.size;
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = slice.get_mapped_range().ok()?;
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            let start = (y * row) as usize;
            for x in 0..w as usize {
                let p = &data[start + x * 4..start + x * 4 + 4];
                out.push([p[0], p[1], p[2], p[3]]);
            }
        }
        Some(out)
    }
}

async fn request_device(
    adapter: &wgpu::Adapter,
) -> Result<(wgpu::Device, wgpu::Queue), RendererError> {
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("tela device"),
            ..Default::default()
        })
        .await
        .map_err(RendererError::Device)
}

/// Destination of tile `coord` in mip level `k` of a `width`x`height`
/// texture, as `(x, y, w, h)` clipped to the level's extent. `None` when the
/// tile lies outside.
pub fn tile_region(
    coord: TileCoord,
    k: u32,
    width: u32,
    height: u32,
) -> Option<(u32, u32, u32, u32)> {
    if coord.x < 0 || coord.y < 0 {
        return None;
    }
    let side = TILE_SIZE >> k;
    let (lw, lh) = ((width >> k).max(1), (height >> k).max(1));
    let (x, y) = (coord.x as u32 * side, coord.y as u32 * side);
    if x >= lw || y >= lh {
        return None;
    }
    Some((x, y, side.min(lw - x), side.min(lh - y)))
}

/// Canvas pixels -> clip space for a surface of `surface` pixels.
pub fn canvas_to_clip(view: &ViewTransform, surface: Vec2) -> Mat4 {
    let screen_to_clip = Mat4::from_translation(Vec3::new(-1.0, 1.0, 0.0))
        * Mat4::from_scale(Vec3::new(2.0 / surface.x, -2.0 / surface.y, 1.0));
    let a = view.canvas_to_screen();
    let canvas_to_screen = Mat4::from_cols(
        a.matrix2.x_axis.extend(0.0).extend(0.0),
        a.matrix2.y_axis.extend(0.0).extend(0.0),
        Vec3::Z.extend(0.0),
        a.translation.extend(0.0).extend(1.0),
    );
    screen_to_clip * canvas_to_screen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_to_clip_matches_view_transform() {
        let view = ViewTransform {
            pan: Vec2::new(30.0, 70.0),
            zoom: 1.5,
            rotation: 0.4,
        };
        let surface = Vec2::new(800.0, 600.0);
        let m = canvas_to_clip(&view, surface);
        for c in [Vec2::ZERO, Vec2::new(100.0, -20.0), Vec2::new(512.0, 384.0)] {
            let s = view.to_screen(c);
            let expected = Vec2::new(s.x / surface.x * 2.0 - 1.0, 1.0 - s.y / surface.y * 2.0);
            let clip = m.project_point3(c.extend(0.0)).truncate();
            assert!((clip - expected).length() < 1e-5, "{clip} vs {expected}");
        }
    }

    #[test]
    fn tile_regions_are_clipped_per_level() {
        // 600x300 document: tile (2, 1) is partially outside at level 0.
        assert_eq!(
            tile_region(TileCoord { x: 2, y: 1 }, 0, 600, 300),
            Some((512, 256, 88, 44))
        );
        assert_eq!(
            tile_region(TileCoord { x: 2, y: 1 }, 1, 600, 300),
            Some((256, 128, 44, 22))
        );
        assert_eq!(tile_region(TileCoord { x: 3, y: 0 }, 0, 600, 300), None);
        assert_eq!(tile_region(TileCoord { x: -1, y: 0 }, 0, 600, 300), None);
        assert_eq!(
            tile_region(TileCoord { x: 0, y: 0 }, 8, 600, 300),
            Some((0, 0, 1, 1))
        );
    }
}
