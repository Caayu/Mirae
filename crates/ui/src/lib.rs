//! Minimal brush-editing UI built with egui.
//!
//! This crate is the only place egui appears. It edits plain data from
//! `tela-brush` (presets, curves, stroke settings) and draws through the
//! renderer's generic [`FrameOverlay`] hook; the engine crates know nothing
//! about it.

pub mod brush_panel;
pub mod curve_editor;

pub use brush_panel::{BrushPanel, PanelAction, PanelModel};
pub use egui;
use egui::{ClippedPrimitive, TexturesDelta, ViewportId};
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use tela_renderer::FrameOverlay;
use winit::window::Window;

struct Pending {
    jobs: Vec<ClippedPrimitive>,
    textures: TexturesDelta,
    pixels_per_point: f32,
}

/// egui context + winit input state + wgpu renderer.
pub struct UiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: Renderer,
    pending: Option<Pending>,
    /// Regions covered by UI, physical pixels `[x0, y0, x1, y1]`.
    occupied: Vec<[f32; 4]>,
}

impl UiLayer {
    pub fn new(window: &Window, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let renderer = Renderer::new(device, format, RendererOptions::default());
        Self {
            ctx,
            state,
            renderer,
            pending: None,
            occupied: Vec::new(),
        }
    }

    /// Feeds a window event to egui. `consumed` means the canvas must not
    /// also react to it.
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    pub fn wants_pointer(&self) -> bool {
        self.ctx.is_pointer_over_egui() || self.ctx.egui_wants_pointer_input()
    }

    pub fn wants_keyboard(&self) -> bool {
        self.ctx.egui_wants_keyboard_input()
    }

    /// Whether a window-space physical-pixel position is over the UI.
    pub fn covers(&self, x: f32, y: f32) -> bool {
        self.occupied
            .iter()
            .any(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
    }

    pub fn occupied(&self) -> &[[f32; 4]] {
        &self.occupied
    }

    /// Runs the UI for one frame. `build` returns the rectangles (in points)
    /// its panels occupy. Returns whether egui wants another frame soon.
    pub fn build(
        &mut self,
        window: &Window,
        mut build: impl FnMut(&mut egui::Ui) -> Vec<egui::Rect>,
    ) -> bool {
        let input = self.state.take_egui_input(window);
        let mut rects = Vec::new();
        let output = self.ctx.run_ui(input, |ui| rects = build(ui));
        self.state
            .handle_platform_output(window, output.platform_output);
        let ppp = output.pixels_per_point;
        self.occupied = rects
            .iter()
            .map(|r| [r.min.x * ppp, r.min.y * ppp, r.max.x * ppp, r.max.y * ppp])
            .collect();
        let jobs = self.ctx.tessellate(output.shapes, ppp);
        let mut textures = output.textures_delta;
        if let Some(prev) = self.pending.take() {
            // A frame that was never drawn still owes its texture updates.
            let mut older = prev.textures;
            older.append(textures);
            textures = older;
        }
        self.pending = Some(Pending {
            jobs,
            textures,
            pixels_per_point: ppp,
        });
        output
            .viewport_output
            .get(&ViewportId::ROOT)
            .is_some_and(|v| v.repaint_delay.is_zero())
    }
}

impl FrameOverlay for UiLayer {
    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
    ) {
        let Some(p) = self.pending.take() else {
            return;
        };
        for (id, deltas) in &p.textures.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        let screen = ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: p.pixels_per_point,
        };
        // No custom paint callbacks are used, so no extra command buffers.
        let _ = self
            .renderer
            .update_buffers(device, queue, encoder, &p.jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let mut pass = pass.forget_lifetime();
            self.renderer.render(&mut pass, &p.jobs, &screen);
        }
        for id in &p.textures.free {
            self.renderer.free_texture(id);
        }
    }
}
