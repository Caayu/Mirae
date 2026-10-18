//! Minimal brush-editing UI built with egui.
//!
//! **Responsibility:** the brush panel ([`BrushPanel`]), the colour panel
//! ([`ColorPanel`]), the selection panel ([`selection_panel()`]), the fill
//! panel ([`fill_panel()`]), the gradient panel ([`gradient_panel()`]) and its
//! drag guide ([`gradient_guide()`]), the selection outline ([`selection_outline()`]), the transform panel
//! ([`transform_panel()`]) and box ([`transform_box()`]), the layer panel
//! ([`layer_panel()`]), the document bar ([`document_bar()`]), the curve editor ([`curve_editor`]), egui
//! input/output plumbing ([`UiLayer`]) and
//! reporting which screen regions the UI covers (pen passthrough).
//! **Does not:** own any authoritative state. It edits data it is lent for
//! one frame ([`PanelModel`], [`ColorModel`], [`LayerRow`], [`DocumentInfo`], [`ExportChoice`]) and returns
//! requests it cannot fulfil ([`PanelAction`], [`ColorAction`], [`LayerAction`],
//! [`DocumentCommand`]). **Depends on:** color, brush, renderer, egui, egui-wgpu,
//! egui-winit, wgpu, winit. **Used by:** app only.
//!
//! This crate is the only place egui appears. It edits plain data from
//! `tela-brush` (presets, curves, stroke settings) and draws through the
//! renderer's generic [`FrameOverlay`] hook; the engine crates know nothing
//! about it. See docs/architecture/ui.md.

pub mod brush_panel;
pub mod color_panel;
pub mod curve_editor;
pub mod document_bar;
pub mod fill_panel;
pub mod gradient_panel;
pub mod history_bar;
pub mod layer_panel;
pub mod prompt;
pub mod selection_panel;
pub mod style;
pub mod toolbar;
pub mod transform_panel;

pub use brush_panel::{BrushPanel, PanelAction, PanelModel};
pub use color_panel::{ColorAction, ColorModel, ColorPanel, PickSource};
pub use document_bar::{DocumentCommand, DocumentInfo, ExportChoice, document_panel};
pub use egui;
use egui::{ClippedPrimitive, TexturesDelta, ViewportId};
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
pub use fill_panel::{FillAction, FillModel, fill_panel};
pub use gradient_panel::{
    GradientAction, GradientEdge, GradientGuide, GradientKind, GradientModel, GradientPreset,
    StopColor, StopRow, gradient_guide, gradient_panel,
};
pub use history_bar::{HistoryButtons, HistoryCommand};
pub use layer_panel::{LayerAction, LayerRow, layer_panel};
pub use prompt::{
    ApplyChoice, MergeMaskChoice, PromptChoice, apply_transform_prompt, merge_mask_prompt, notice,
    unsaved_prompt,
};
pub use selection_panel::{
    SelectionAction, SelectionKind, SelectionMode, SelectionModel, selection_outline,
    selection_panel,
};
use tela_renderer::FrameOverlay;
pub use toolbar::{ToolChoice, tool_strip, top_bar};
pub use transform_panel::{
    HANDLE, TransformAction, TransformBox, TransformModel, transform_box, transform_panel,
};
use winit::window::Window;

struct Pending {
    jobs: Vec<ClippedPrimitive>,
    textures: TexturesDelta,
    pixels_per_point: f32,
}

/// One widget as the desktop end-to-end tests see it: egui's own
/// accessibility description of it (role, label, value, state) and where it
/// is, in physical pixels of the window's client area. Built only when
/// inspection is on ([`UiLayer::set_inspect`]); see docs/testing.md.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetInfo {
    /// The AccessKit role, e.g. `Button`, `CheckBox`, `Slider`, `Label`.
    pub role: String,
    /// The visible text of a button, checkbox or label.
    pub label: String,
    /// A text field's text or a value the widget shows.
    pub value: Option<String>,
    /// A slider's or drag value's number.
    pub number: Option<f64>,
    /// `[x0, y0, x1, y1]`, physical pixels.
    pub rect: [f32; 4],
    /// The part of `rect` that is shown (clipped by its panel or scroll
    /// area); empty when the widget is scrolled out of sight.
    pub visible: [f32; 4],
    pub enabled: bool,
    /// Selected (a selectable label) or checked (a checkbox), if it has
    /// such a state.
    pub toggled: Option<bool>,
    /// Has the keyboard focus.
    pub focused: bool,
}

/// egui context + winit input state + wgpu renderer.
pub struct UiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: Renderer,
    pending: Option<Pending>,
    /// Regions covered by UI, physical pixels `[x0, y0, x1, y1]`.
    occupied: Vec<[f32; 4]>,
    /// Whether the widgets are described each frame (`widgets`).
    inspect: bool,
    widgets: Vec<WidgetInfo>,
}

impl UiLayer {
    pub fn new(window: &Window, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let ctx = egui::Context::default();
        style::apply(&ctx);
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
            inspect: false,
            widgets: Vec::new(),
        }
    }

    /// Describes every widget each frame (egui's AccessKit tree, read
    /// back as [`WidgetInfo`]). Off by default: only the end-to-end test
    /// mode turns it on; nothing else changes.
    pub fn set_inspect(&mut self, on: bool) {
        self.inspect = on;
        if on {
            self.ctx.enable_accesskit();
        } else {
            self.ctx.disable_accesskit();
            self.widgets.clear();
        }
    }

    /// The widgets of the last frame, in the order egui laid them out
    /// (empty unless inspection is on).
    pub fn widgets(&self) -> &[WidgetInfo] {
        &self.widgets
    }

    /// Whether a text field (or another widget) holds the keyboard.
    pub fn has_focus(&self) -> bool {
        self.ctx.memory(|m| m.focused().is_some())
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
        let mut output = self.ctx.run_ui(input, |ui| rects = build(ui));
        let ppp = output.pixels_per_point;
        if self.inspect
            && let Some(update) = output.platform_output.accesskit_update.take()
        {
            self.widgets = describe(&self.ctx, &update, ppp);
        }
        self.state
            .handle_platform_output(window, output.platform_output);
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

/// The egui widget id an AccessKit node id stands for (egui makes each node
/// id from its widget id's value), if it is one.
#[allow(unsafe_code)]
fn widget_id(node: u64) -> Option<egui::Id> {
    if node == 0 {
        return None;
    }
    // SAFETY: `from_high_entropy_bits` is `unsafe` only because a low-
    // entropy value would make egui's id maps slow; there is no memory
    // safety requirement. The value is non-zero (checked above) and is
    // an id egui itself produced (`Id::accesskit_id` is `Id::value`), so
    // it is high-entropy. It is used only to look the widget up.
    Some(unsafe { egui::Id::from_high_entropy_bits(node) })
}

/// The widgets of an AccessKit tree in layout order (depth first from the
/// root), with their bounds scaled from points to physical pixels.
/// Containers and unlabelled custom areas are left out.
fn describe(
    ctx: &egui::Context,
    update: &egui::accesskit::TreeUpdate,
    ppp: f32,
) -> Vec<WidgetInfo> {
    use egui::accesskit::{NodeId, Role, Toggled};
    let nodes: std::collections::HashMap<NodeId, &egui::accesskit::Node> =
        update.nodes.iter().map(|(id, n)| (*id, n)).collect();
    let mut out = Vec::new();
    let mut stack = vec![egui::accesskit_root_id().accesskit_id()];
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(node) = nodes.get(&id) else {
            continue;
        };
        // Children in reverse, so the first is visited first.
        stack.extend(node.children().iter().rev().copied());
        let role = node.role();
        let label = node.label().unwrap_or_default().to_owned();
        let value = node.value().map(str::to_owned);
        let (Some(b), keep) = (
            node.bounds(),
            !matches!(role, Role::GenericContainer | Role::Unknown)
                || !label.is_empty()
                || value.is_some(),
        ) else {
            continue;
        };
        if !keep {
            continue;
        }
        let rect = [
            b.x0 as f32 * ppp,
            b.y0 as f32 * ppp,
            b.x1 as f32 * ppp,
            b.y1 as f32 * ppp,
        ];
        // What is shown of it: egui's interaction rectangle is the widget
        // clipped by its panel and scroll area. Text runs and other nodes
        // that are not widgets have none; their bounds are what is shown.
        let visible = widget_id(id.0)
            .and_then(|w| ctx.read_response(w))
            .map_or(rect, |r| {
                let c = r.interact_rect;
                if c.is_positive() {
                    [c.min.x * ppp, c.min.y * ppp, c.max.x * ppp, c.max.y * ppp]
                } else {
                    [0.0; 4]
                }
            });
        out.push(WidgetInfo {
            role: format!("{role:?}"),
            label,
            value,
            number: node.numeric_value(),
            rect,
            visible,
            enabled: !node.is_disabled(),
            toggled: match node.toggled() {
                Some(Toggled::True) => Some(true),
                Some(Toggled::False) => Some(false),
                _ => None,
            },
            focused: update.focus == id,
        });
    }
    out
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
