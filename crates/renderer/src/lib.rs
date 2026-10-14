//! Presents composed canvas tiles with wgpu.
//!
//! The renderer never sees the document model: callers hand it finished
//! premultiplied RGBA8 tiles and a view transform. It keeps one GPU texture per
//! non-empty tile and draws only the tiles intersecting the viewport.

use std::collections::HashMap;

use glam::{Mat4, Vec2, Vec3};
use tela_canvas::{IRect, TILE_SIZE, TileCoord, ViewTransform};
use wgpu::util::DeviceExt;

#[derive(Debug)]
pub enum RendererError {
    Surface(wgpu::CreateSurfaceError),
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
    UnsupportedSurface,
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(e) => write!(f, "cannot create surface: {e}"),
            Self::Adapter(e) => write!(f, "no suitable GPU adapter: {e}"),
            Self::Device(e) => write!(f, "cannot open GPU device: {e}"),
            Self::UnsupportedSurface => write!(f, "surface not supported by adapter"),
        }
    }
}

impl std::error::Error for RendererError {}

/// What to draw this frame besides the tiles themselves.
pub struct Frame<'a> {
    pub view: &'a ViewTransform,
    pub document_size: Vec2,
    /// Straight-alpha sRGB colors.
    pub paper: [f32; 4],
    pub workspace: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    canvas_to_clip: [[f32; 4]; 4],
    paper: [f32; 4],
    doc_size: [f32; 4],
}

struct GpuTile {
    _texture: wgpu::Texture,
    nearest: wgpu::BindGroup,
    linear: wgpu::BindGroup,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    tile_layout: wgpu::BindGroupLayout,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    paper_pipeline: wgpu::RenderPipeline,
    tile_pipeline: wgpu::RenderPipeline,
    tiles: HashMap<TileCoord, GpuTile>,
}

impl Renderer {
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
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("tela device"),
                ..Default::default()
            })
            .await
            .map_err(RendererError::Device)?;

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

        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals"),
            contents: bytemuck::bytes_of(&Globals {
                canvas_to_clip: Mat4::IDENTITY.to_cols_array_2d(),
                paper: [1.0; 4],
                doc_size: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let tile_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tile"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("tile"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };
        let nearest = sampler(wgpu::FilterMode::Nearest);
        let linear = sampler(wgpu::FilterMode::Linear);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let paper_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("paper"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let tile_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tile"),
            bind_group_layouts: &[Some(&globals_layout), Some(&tile_layout)],
            immediate_size: 0,
        });
        let pipeline =
            |label, layout, vs, fs, buffers: &[Option<wgpu::VertexBufferLayout>], blend| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(vs),
                        compilation_options: Default::default(),
                        buffers,
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(fs),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: config.format,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let paper_pipeline = pipeline(
            "paper",
            &paper_layout,
            "vs_paper",
            "fs_paper",
            &[],
            Some(wgpu::BlendState::REPLACE),
        );
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2],
        };
        let tile_pipeline = pipeline(
            "tile",
            &tile_pipeline_layout,
            "vs_tile",
            "fs_tile",
            &[Some(instance_layout)],
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            globals,
            globals_bind,
            tile_layout,
            nearest,
            linear,
            paper_pipeline,
            tile_pipeline,
            tiles: HashMap::new(),
        })
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.config.width as f32, self.config.height as f32)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Replaces a tile's pixels (premultiplied RGBA8, 256x256 row-major), or
    /// frees it when `pixels` is `None`.
    pub fn upload_tile(&mut self, coord: TileCoord, pixels: Option<&[[u8; 4]]>) {
        let Some(pixels) = pixels else {
            self.tiles.remove(&coord);
            return;
        };
        let size = wgpu::Extent3d {
            width: TILE_SIZE,
            height: TILE_SIZE,
            depth_or_array_layers: 1,
        };
        let tile = self.tiles.entry(coord).or_insert_with(|| {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("tile"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind = |sampler: &wgpu::Sampler| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("tile"),
                    layout: &self.tile_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                })
            };
            GpuTile {
                nearest: bind(&self.nearest),
                linear: bind(&self.linear),
                _texture: texture,
            }
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tile._texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TILE_SIZE * 4),
                rows_per_image: Some(TILE_SIZE),
            },
            size,
        );
    }

    pub fn render(&mut self, frame: &Frame) {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            other => {
                log::warn!("skipping frame: {other:?}");
                return;
            }
        };

        self.queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                canvas_to_clip: canvas_to_clip(frame.view, self.size()).to_cols_array_2d(),
                paper: frame.paper,
                doc_size: [frame.document_size.x, frame.document_size.y, 0.0, 0.0],
            }),
        );

        let visible = frame
            .view
            .visible_canvas_rect(self.size())
            .intersect(&IRect::new(
                0,
                0,
                frame.document_size.x as i32,
                frame.document_size.y as i32,
            ));
        let drawn: Vec<TileCoord> = visible
            .tiles()
            .filter(|c| self.tiles.contains_key(c))
            .collect();
        let origins: Vec<[f32; 2]> = drawn
            .iter()
            .map(|c| {
                let (x, y) = c.origin();
                [x as f32, y as f32]
            })
            .collect();
        let instances = (!origins.is_empty()).then(|| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("tile instances"),
                    contents: bytemuck::cast_slice(&origins),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });

        let view = surface_texture.texture.create_view(&Default::default());
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
            pass.set_bind_group(0, &self.globals_bind, &[]);
            pass.set_pipeline(&self.paper_pipeline);
            pass.draw(0..6, 0..1);

            if let Some(instances) = &instances {
                // Nearest when magnifying keeps pixels crisp and avoids seams
                // between independently sampled tiles.
                let magnified = frame.view.zoom >= 1.0;
                pass.set_pipeline(&self.tile_pipeline);
                pass.set_vertex_buffer(0, instances.slice(..));
                for (i, coord) in drawn.iter().enumerate() {
                    let tile = &self.tiles[coord];
                    let bind = if magnified {
                        &tile.nearest
                    } else {
                        &tile.linear
                    };
                    pass.set_bind_group(1, bind, &[]);
                    let i = i as u32;
                    pass.draw(0..6, i..i + 1);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(surface_texture);
    }
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
}
