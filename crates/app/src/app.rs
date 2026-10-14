use std::sync::Arc;
use std::time::Instant;

use glam::Vec2;
use tela_brush::{BrushEngine, BrushInput, BrushSettings};
use tela_canvas::{DirtyTiles, TileCoord, ViewTransform};
use tela_color::PremulRgba;
use tela_document::{Document, Overlay};
use tela_renderer::{Frame, Renderer};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

const DOCUMENT_SIZE: (u32, u32) = (4000, 3000);
const WORKSPACE_COLOR: [f32; 4] = [0.18, 0.18, 0.2, 1.0];

/// What a pointer drag is currently doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    None,
    Paint,
    Pan,
}

struct Gpu {
    window: Arc<Window>,
    renderer: Renderer,
}

pub struct App {
    gpu: Option<Gpu>,
    document: Document,
    view: ViewTransform,
    engine: BrushEngine,
    /// Tiles whose composite must be rebuilt before the next frame.
    stale: DirtyTiles,
    cursor: Vec2,
    drag: Drag,
    space_held: bool,
    modifiers: ModifiersState,
    clock: Instant,
}

impl App {
    pub fn new() -> Self {
        let (w, h) = DOCUMENT_SIZE;
        Self {
            gpu: None,
            document: Document::new(w, h),
            view: ViewTransform::default(),
            engine: BrushEngine::new(BrushSettings::default()),
            stale: DirtyTiles::new(),
            cursor: Vec2::ZERO,
            drag: Drag::None,
            space_held: false,
            modifiers: ModifiersState::empty(),
            clock: Instant::now(),
        }
    }

    fn request_redraw(&self) {
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }

    fn fit_view(&mut self) {
        if let Some(gpu) = &self.gpu {
            let doc = Vec2::new(self.document.width as f32, self.document.height as f32);
            self.view = ViewTransform::fit(doc, gpu.renderer.size());
        }
    }

    fn sample(&self) -> BrushInput {
        let t = self.clock.elapsed().as_secs_f64();
        BrushInput::from_position(self.view.to_canvas(self.cursor), t)
    }

    fn on_pointer_button(&mut self, button: MouseButton, state: ElementState) {
        match (button, state, self.drag) {
            (MouseButton::Left, ElementState::Pressed, Drag::None) if self.space_held => {
                self.drag = Drag::Pan;
            }
            (MouseButton::Left, ElementState::Pressed, Drag::None) => {
                self.drag = Drag::Paint;
                self.engine.begin_stroke(self.sample());
            }
            (MouseButton::Middle, ElementState::Pressed, Drag::None) => self.drag = Drag::Pan,
            (MouseButton::Left, ElementState::Released, Drag::Paint) => {
                self.drag = Drag::None;
                self.finish_stroke();
            }
            (MouseButton::Left | MouseButton::Middle, ElementState::Released, Drag::Pan) => {
                self.drag = Drag::None;
            }
            _ => {}
        }
        self.request_redraw();
    }

    fn on_cursor_moved(&mut self, pos: Vec2) {
        let delta = pos - self.cursor;
        self.cursor = pos;
        match self.drag {
            Drag::Paint => {
                let s = self.sample();
                self.engine.add_input(s);
            }
            Drag::Pan => self.view.pan_by(delta),
            Drag::None => return,
        }
        self.request_redraw();
    }

    fn on_wheel(&mut self, delta: MouseScrollDelta) {
        let steps = match delta {
            MouseScrollDelta::LineDelta(_, y) => y,
            MouseScrollDelta::PixelDelta(p) => p.y as f32 / 120.0,
        };
        if self.modifiers.shift_key() {
            self.view
                .rotate_at(self.cursor, steps.signum() * 15f32.to_radians());
        } else {
            self.view.zoom_at(self.cursor, 1.15f32.powf(steps));
        }
        self.request_redraw();
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        match &event.logical_key {
            Key::Named(NamedKey::Space) => self.space_held = pressed,
            Key::Named(NamedKey::Home) if pressed => {
                self.fit_view();
                self.request_redraw();
            }
            Key::Character(c) if pressed => match c.as_str() {
                "]" => self.resize_brush(1.25),
                "[" => self.resize_brush(1.0 / 1.25),
                _ => {}
            },
            _ => {}
        }
    }

    fn resize_brush(&mut self, factor: f32) {
        let s = &mut self.engine.settings;
        s.size = (s.size * factor).clamp(1.0, 2000.0);
        log::info!("brush size {:.1}px", s.size);
    }

    fn finish_stroke(&mut self) {
        if let Some(stroke) = self.engine.end_stroke() {
            let touched = stroke.commit_into(&mut self.document.active_layer_mut().pixels);
            self.stale.extend(&touched);
        }
    }

    /// Recomposites stale tiles and uploads them.
    fn refresh_tiles(&mut self) {
        let Some(gpu) = &mut self.gpu else {
            return;
        };
        if let Some(stroke) = self.engine.stroke_mut() {
            self.stale.extend(&stroke.take_dirty());
        }
        if self.stale.is_empty() {
            return;
        }
        let stroke = self.engine.stroke();
        let paint = |c: TileCoord, px: &mut [PremulRgba]| {
            if let Some(s) = stroke {
                s.paint_over_tile(c, px);
            }
        };
        let covers = |c: TileCoord| stroke.is_some_and(|s| s.touched().contains(c));
        let overlay = Overlay {
            above_layer: self.document.active_layer,
            paint: &paint,
            covers: &covers,
        };
        for coord in self.stale.take() {
            let pixels = self.document.compose_tile(coord, Some(&overlay));
            gpu.renderer.upload_tile(coord, pixels.as_deref());
        }
    }

    fn redraw(&mut self) {
        self.refresh_tiles();
        let Some(gpu) = &mut self.gpu else {
            return;
        };
        let bg = self.document.background;
        gpu.renderer.render(&Frame {
            view: &self.view,
            document_size: Vec2::new(self.document.width as f32, self.document.height as f32),
            paper: [bg.r, bg.g, bg.b, bg.a],
            workspace: WORKSPACE_COLOR,
        });
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Tela")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("cannot create window: {e}");
                event_loop.exit();
                return;
            }
        };
        let size = window.inner_size();
        match pollster::block_on(Renderer::new(window.clone(), size.width, size.height)) {
            Ok(renderer) => {
                self.gpu = Some(Gpu { window, renderer });
                self.fit_view();
                self.request_redraw();
            }
            Err(e) => {
                log::error!("{e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.renderer.resize(size.width, size.height);
                }
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } => self.on_key(&event),
            WindowEvent::CursorMoved { position, .. } => {
                self.on_cursor_moved(Vec2::new(position.x as f32, position.y as f32));
            }
            WindowEvent::MouseInput { state, button, .. } => self.on_pointer_button(button, state),
            WindowEvent::MouseWheel { delta, .. } => self.on_wheel(delta),
            WindowEvent::Focused(false) => {
                // Releases outside the window would otherwise leave a drag stuck.
                if self.drag == Drag::Paint {
                    self.finish_stroke();
                }
                self.drag = Drag::None;
                self.space_held = false;
                self.request_redraw();
            }
            _ => {}
        }
    }
}
