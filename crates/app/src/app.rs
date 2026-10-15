use std::sync::Arc;
use std::time::Instant;

use glam::Vec2;
use tela_brush::dynamics::{DynamicsSource, InputCaps};
use tela_brush::{BrushEngine, BrushInput, BrushPreset, PixelAlpha};
use tela_canvas::{DirtyTiles, TileCoord, ViewTransform};
use tela_color::PremulRgba;
use tela_document::{
    BlendMode, ComposeScratch, Document, HistoryAction, LayerId, LayerPart, Overlay, OverlayPaint,
    Placement,
};
use tela_renderer::{Frame, Renderer};
use tela_tablet::{DeviceKind, Phase, TabletEvent, TabletSample, TimestampNormalizer, clock};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::stats::LatencyStats;
use crate::tools::Tools;
use tela_ui::{
    BrushPanel, HistoryButtons, HistoryCommand, LayerAction, LayerRow, PanelAction, PanelModel,
    UiLayer, egui,
};

const DOCUMENT_SIZE: (u32, u32) = (4000, 3000);
const WORKSPACE_COLOR: [f32; 4] = [0.18, 0.18, 0.2, 1.0];

/// Events delivered to the event loop from outside winit.
#[derive(Debug, Clone, Copy)]
pub enum AppEvent {
    /// The pen backend queued samples.
    Tablet,
}

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
    ui: UiLayer,
    #[cfg(windows)]
    ink: Option<tela_tablet::windows::InkBackend>,
}

pub struct App {
    proxy: EventLoopProxy<AppEvent>,
    gpu: Option<Gpu>,
    document: Document,
    view: ViewTransform,
    engine: BrushEngine,
    /// Tiles whose composite must be rebuilt before the next frame.
    stale: DirtyTiles,
    compose: ComposeScratch,
    cursor: Vec2,
    drag: Drag,
    space_held: bool,
    mouse_left_down: bool,
    modifiers: ModifiersState,
    timestamps: TimestampNormalizer,
    tablet_events: Vec<TabletEvent>,
    stats: LatencyStats,
    show_stats: bool,
    last_device: Option<DeviceKind>,
    /// Seed of the next stroke; recorded strokes replay identically.
    stroke_seed: u64,
    /// What the stroke in progress edits, fixed when it began.
    stroke_target: Option<StrokeTarget>,
    tools: Tools,
    panel: BrushPanel,
    last_title: String,
}

/// What a stroke edits, captured when it begins so that nothing changed
/// during the stroke (selection, edit target, transparency lock) can move
/// it: the layer, its pixels or its mask, and whether paint keeps the
/// pixels' alpha.
#[derive(Clone, Copy, Debug)]
struct StrokeTarget {
    layer: LayerId,
    part: LayerPart,
    alpha: PixelAlpha,
}

impl StrokeTarget {
    /// The document's current edit target.
    fn active(doc: &Document) -> Self {
        let layer = doc.active_layer();
        let part = doc.active_part();
        let locked = doc.layer(layer).is_some_and(|l| l.alpha_locked());
        let alpha = if part == LayerPart::Pixels && locked {
            PixelAlpha::Preserved
        } else {
            PixelAlpha::Free
        };
        Self { layer, part, alpha }
    }
}

impl App {
    pub fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        let (w, h) = DOCUMENT_SIZE;
        Self {
            proxy,
            gpu: None,
            document: Document::new(w, h),
            view: ViewTransform::default(),
            engine: BrushEngine::new(BrushPreset::default()),
            stale: DirtyTiles::new(),
            compose: ComposeScratch::new(),
            cursor: Vec2::ZERO,
            drag: Drag::None,
            space_held: false,
            mouse_left_down: false,
            modifiers: ModifiersState::empty(),
            timestamps: TimestampNormalizer::default(),
            tablet_events: Vec::with_capacity(512),
            stats: LatencyStats::default(),
            show_stats: false,
            last_device: None,
            stroke_seed: 0,
            stroke_target: None,
            tools: Tools::new(),
            panel: BrushPanel::default(),
            last_title: String::new(),
        }
    }

    fn request_redraw(&self) {
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }

    fn fit_view(&mut self) {
        if let Some(gpu) = &self.gpu {
            let doc = Vec2::new(self.document.width() as f32, self.document.height() as f32);
            // Fit into the area the side panels leave free.
            let size = gpu.renderer.size();
            let occupied = gpu.ui.occupied();
            let left = occupied
                .iter()
                .filter(|r| r[0] <= 1.0)
                .map(|r| r[2])
                .fold(0.0f32, f32::max);
            let right = occupied
                .iter()
                .filter(|r| r[2] >= size.x - 1.0)
                .map(|r| r[0])
                .fold(size.x, f32::min);
            self.view = ViewTransform::fit(doc, Vec2::new((right - left).max(1.0), size.y));
            self.view.pan.x += left;
        }
    }

    fn update_title(&mut self) {
        let Some(gpu) = &self.gpu else {
            return;
        };
        let s = self.tools.active();
        let st = s.stroke.resolve(&self.engine.stroke_defaults);
        let taper = if s.taper.start > 0.0 || s.taper.end > 0.0 {
            "on"
        } else {
            "off"
        };
        let opacity_pressure = s
            .properties
            .opacity
            .effectors
            .iter()
            .any(|e| e.enabled && e.source == DynamicsSource::Pressure);
        let mut title = format!(
            "Tela — {} {:.0}px | stab {} {:.0} | {} | taper {taper} | opacity×pressure {} | {}",
            s.name,
            s.properties.size.base,
            st.stabilizer.name(),
            st.stabilization,
            st.interpolation.name(),
            if opacity_pressure { "on" } else { "off" },
            match self.last_device {
                Some(DeviceKind::Pen) => "pen",
                Some(DeviceKind::Eraser) => "pen (eraser end)",
                Some(DeviceKind::Touch) => "touch",
                Some(DeviceKind::Mouse) | None => "mouse",
            },
        );
        if self.show_stats && !self.stats.summary.is_empty() {
            title.push_str(" || ");
            title.push_str(&self.stats.summary);
        }
        if title != self.last_title {
            gpu.window.set_title(&title);
            self.last_title = title;
        }
    }

    /// Maps a normalized device sample into the brush engine's canvas space.
    fn brush_input(&mut self, s: &TabletSample) -> BrushInput {
        // Tilt is measured against the screen; express it in canvas axes.
        let tilt = Vec2::from_angle(-self.view.rotation).rotate(Vec2::new(s.tilt_x, s.tilt_y));
        BrushInput {
            position: self.view.to_canvas(s.position),
            pressure: s.pressure,
            tilt,
            rotation: s.rotation,
            timestamp: self.timestamps.normalize(s.timestamp),
            caps: InputCaps {
                pressure: s.caps.pressure,
                tilt: s.caps.tilt,
                rotation: s.caps.rotation,
            },
        }
    }

    /// Single entry point for mouse and pen.
    fn on_pointer(&mut self, ev: TabletEvent) {
        let pos = ev.sample.position;
        let moved = pos - self.cursor;
        if ev.phase != Phase::Leave && ev.phase != Phase::Cancel {
            self.cursor = pos;
        }
        if self.last_device != Some(ev.sample.device_kind) {
            self.last_device = Some(ev.sample.device_kind);
            self.update_title();
        }
        match (ev.phase, self.drag) {
            (Phase::Down, Drag::None) if self.space_held => self.drag = Drag::Pan,
            (Phase::Down, Drag::None) => {
                let target = StrokeTarget::active(&self.document);
                if let Err(e) = self.document.paint_target(target.layer, target.part) {
                    // Like the reference, hidden and locked layers (and
                    // disabled masks) take no paint; the press does nothing.
                    log::info!(target: "tela::stroke", "not painting: {e}");
                    return;
                }
                self.drag = Drag::Paint;
                self.stroke_target = Some(target);
                self.timestamps.reset();
                let input = self.brush_input(&ev.sample);
                let t = Instant::now();
                self.engine.preset = self.tools.for_stroke(ev.sample.device_kind).clone();
                self.stroke_seed = self.stroke_seed.wrapping_add(1);
                self.engine
                    .begin_stroke(input, self.view.zoom, self.stroke_seed);
                self.stats
                    .sample(ev.sample.timestamp, t.elapsed().as_secs_f64());
            }
            (Phase::Move | Phase::Up, Drag::Paint) => {
                let input = self.brush_input(&ev.sample);
                let t = Instant::now();
                self.engine.add_input_at(input, clock::now());
                self.stats
                    .sample(ev.sample.timestamp, t.elapsed().as_secs_f64());
                if ev.phase == Phase::Up {
                    self.finish_stroke();
                }
            }
            (Phase::Cancel, Drag::Paint) => self.finish_stroke(),
            (Phase::Move | Phase::Hover, Drag::Pan) => self.view.pan_by(moved),
            (Phase::Up | Phase::Cancel, Drag::Pan) => self.drag = Drag::None,
            _ => return,
        }
        self.request_redraw();
    }

    fn finish_stroke(&mut self) {
        self.drag = Drag::None;
        let target = self.stroke_target.take();
        if let Some(stroke) = self.engine.end_stroke() {
            // The preview disappears either way, so its tiles recompose.
            self.stale.extend(stroke.touched());
            let tiles = stroke.touched();
            let doc = &mut self.document;
            let committed = target.map(|t| match t.part {
                LayerPart::Pixels => doc.modify_tiles(t.layer, tiles, |c, tile| {
                    stroke.commit_tile(c, tile, t.alpha)
                }),
                LayerPart::Mask => {
                    doc.modify_mask(t.layer, tiles, |c, tile| stroke.commit_alpha_tile(c, tile))
                }
            });
            if let Some(Err(e)) = committed {
                log::warn!(target: "tela::stroke", "stroke discarded: {e}");
            }
            log::debug!(target: "tela::stroke", "{:?}", self.engine.stats());
        }
    }

    fn drain_tablet(&mut self) {
        #[cfg(windows)]
        if let Some(ink) = self.gpu.as_ref().and_then(|g| g.ink.as_ref()) {
            ink.drain(&mut self.tablet_events);
        }
        let mut events = std::mem::take(&mut self.tablet_events);
        for ev in events.drain(..) {
            self.on_pointer(ev);
        }
        self.tablet_events = events;
    }

    fn on_mouse_button(&mut self, button: MouseButton, state: ElementState) {
        let pressed = state == ElementState::Pressed;
        match button {
            MouseButton::Left => {
                self.mouse_left_down = pressed;
                let phase = if pressed { Phase::Down } else { Phase::Up };
                self.on_pointer(TabletEvent {
                    phase,
                    pointer_id: 0,
                    sample: TabletSample::mouse(self.cursor, clock::now()),
                });
            }
            MouseButton::Middle if pressed && self.drag == Drag::None => self.drag = Drag::Pan,
            MouseButton::Middle if !pressed && self.drag == Drag::Pan => self.drag = Drag::None,
            _ => {}
        }
    }

    fn on_cursor_moved(&mut self, pos: Vec2) {
        let phase = if self.mouse_left_down {
            Phase::Move
        } else {
            Phase::Hover
        };
        self.on_pointer(TabletEvent {
            phase,
            pointer_id: 0,
            sample: TabletSample::mouse(pos, clock::now()),
        });
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
        self.engine.set_scale(self.view.zoom);
        self.request_redraw();
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        if let Key::Named(NamedKey::Space) = event.logical_key {
            self.space_held = pressed;
            return;
        }
        if !pressed || self.engine.is_stroking() {
            return;
        }
        if self.modifiers.control_key() {
            let shift = self.modifiers.shift_key();
            let command = match event.physical_key {
                PhysicalKey::Code(KeyCode::KeyN) if shift => {
                    self.apply_layer(LayerAction::Add);
                    None
                }
                PhysicalKey::Code(KeyCode::KeyZ) if shift => Some(HistoryCommand::Redo),
                PhysicalKey::Code(KeyCode::KeyZ) => Some(HistoryCommand::Undo),
                PhysicalKey::Code(KeyCode::KeyY) => Some(HistoryCommand::Redo),
                _ => return,
            };
            if let Some(c) = command {
                self.apply_history(c);
            }
            self.request_redraw();
            return;
        }
        let p = self.tools.active_mut();
        let user = &mut self.engine.stroke_defaults;
        match &event.logical_key {
            Key::Named(NamedKey::Home) => self.fit_view(),
            Key::Named(NamedKey::F3) => self.show_stats = !self.show_stats,
            Key::Character(c) => match c.as_str() {
                "]" => p.properties.size.base = (p.properties.size.base * 1.25).clamp(1.0, 2000.0),
                "[" => p.properties.size.base = (p.properties.size.base / 1.25).clamp(1.0, 2000.0),
                "=" | "+" => p.stroke.edit_resolved(user, |s| {
                    s.stabilization = (s.stabilization + 10.0).min(100.0);
                }),
                "-" => p.stroke.edit_resolved(user, |s| {
                    s.stabilization = (s.stabilization - 10.0).max(0.0);
                }),
                "s" | "S" => p
                    .stroke
                    .edit_resolved(user, |s| s.stabilizer = s.stabilizer.next()),
                "i" | "I" => p
                    .stroke
                    .edit_resolved(user, |s| s.interpolation = s.interpolation.next()),
                "t" | "T" => {
                    let on = p.taper.start <= 0.0 && p.taper.end <= 0.0;
                    p.taper.start = if on { 30.0 } else { 0.0 };
                    p.taper.end = if on { 60.0 } else { 0.0 };
                }
                "o" | "O" => {
                    if let Some(e) = p.properties.opacity.effector_mut(DynamicsSource::Pressure) {
                        e.enabled = !e.enabled;
                    }
                }
                "c" | "C" => self.clear_canvas(),
                "e" | "E" => self.tools.eraser_mode = !self.tools.eraser_mode,
                d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                    let i = (d.as_bytes()[0] - b'1') as usize;
                    self.tools.select(i);
                }
                _ => return,
            },
            _ => return,
        }
        self.update_title();
        self.request_redraw();
    }

    fn clear_canvas(&mut self) {
        match self.document.clear_layer(self.document.active_layer()) {
            Ok(dirty) => self.stale.extend(&dirty),
            Err(e) => log::info!("not clearing: {e}"),
        }
    }

    /// Undo or redo. Like layer commands, ignored while a stroke is in
    /// progress (reference behaviour, docs/parity/4B-undo-redo.md U18).
    fn apply_history(&mut self, command: HistoryCommand) {
        if self.engine.is_stroking() {
            return;
        }
        let dirty = match command {
            HistoryCommand::Undo => self.document.undo(),
            HistoryCommand::Redo => self.document.redo(),
        };
        if let Some(dirty) = dirty {
            self.stale.extend(&dirty);
        }
    }

    /// Applies a layer command to the document. Ignored while a stroke is
    /// in progress, like the reference (docs/parity/4A-layers.md, O14):
    /// the stroke always finishes on the layer it started on.
    fn apply_layer(&mut self, action: LayerAction) {
        if self.engine.is_stroking() {
            return;
        }
        let doc = &mut self.document;
        let active = doc.active_layer();
        let Some(target) = (match action {
            LayerAction::Select(key)
            | LayerAction::SetVisible(key, _)
            | LayerAction::SetLocked(key, _) => doc.find_layer(key),
            _ => Some(active),
        }) else {
            log::info!("layer command for a layer that no longer exists");
            return;
        };
        let result = match action {
            LayerAction::Add => doc
                .add_layer(Placement::Above(active))
                .map(|_| DirtyTiles::new()),
            LayerAction::Remove => doc.remove_layer(active),
            LayerAction::MoveUp => match doc.layer_above(active) {
                Some(above) => doc.move_layer(active, Placement::Above(above)),
                None => Ok(DirtyTiles::new()),
            },
            LayerAction::MoveDown => match doc.layer_below(active) {
                Some(below) => doc.move_layer(active, Placement::Below(below)),
                None => Ok(DirtyTiles::new()),
            },
            LayerAction::SetOpacity(o) => doc.set_opacity(active, o),
            LayerAction::SetBlendMode(i) => match BlendMode::ALL.get(i) {
                Some(&mode) => doc.set_blend_mode(active, mode),
                None => return,
            },
            LayerAction::Select(_) => doc.set_active_layer(target).map(|()| DirtyTiles::new()),
            LayerAction::SetVisible(_, v) => doc.set_visible(target, v),
            LayerAction::SetLocked(_, v) => doc.set_locked(target, v).map(|()| DirtyTiles::new()),
            LayerAction::SetClipped(v) => doc.set_clipped(active, v),
            LayerAction::SetAlphaLocked(v) => {
                doc.set_alpha_locked(active, v).map(|()| DirtyTiles::new())
            }
            LayerAction::AddMask => doc.add_mask(active),
            LayerAction::RemoveMask => doc.remove_mask(active),
            LayerAction::SetMaskEnabled(v) => doc.set_mask_enabled(active, v),
            LayerAction::EditMask(mask) => {
                let part = if mask {
                    LayerPart::Mask
                } else {
                    LayerPart::Pixels
                };
                doc.set_active_part(part).map(|()| DirtyTiles::new())
            }
        };
        match result {
            Ok(dirty) => self.stale.extend(&dirty),
            Err(e) => log::info!("layer command refused: {e}"),
        }
    }

    /// Recomposites stale tiles and uploads them.
    fn refresh_tiles(&mut self) -> usize {
        let Some(gpu) = &mut self.gpu else {
            return 0;
        };
        self.engine.update_preview();
        if let Some(stroke) = self.engine.stroke_mut() {
            self.stale.extend(&stroke.take_dirty());
        }
        if self.stale.is_empty() {
            return 0;
        }
        let stroke = self.engine.stroke();
        let target = self
            .stroke_target
            .unwrap_or_else(|| StrokeTarget::active(&self.document));
        // Preview and commit share the stroke's compositing rules
        // (paint_over_tile / commit_tile, paint_alpha_tile /
        // commit_alpha_tile), with the target fixed at stroke start.
        let pixels = |c: TileCoord, px: &mut [PremulRgba]| {
            if let Some(s) = stroke {
                s.paint_over_tile(c, px, target.alpha);
            }
        };
        let mask = |c: TileCoord, values: &mut [f32]| {
            if let Some(s) = stroke {
                s.paint_alpha_tile(c, values);
            }
        };
        let covers = |c: TileCoord| stroke.is_some_and(|s| s.touched().contains(c));
        let overlay = Overlay {
            target: target.layer,
            paint: match target.part {
                LayerPart::Pixels => OverlayPaint::Pixels(&pixels),
                LayerPart::Mask => OverlayPaint::Mask(&mask),
            },
            covers: &covers,
        };
        let tiles = self.stale.take();
        for &coord in &tiles {
            let has = self
                .document
                .compose_tile_into(coord, Some(&overlay), &mut self.compose);
            gpu.renderer
                .upload_tile(coord, has.then_some(self.compose.out.as_slice()));
        }
        tiles.len()
    }

    /// Builds the brush panel for this frame and returns what it asked for.
    fn build_ui(&mut self) -> PanelAction {
        let Some(gpu) = &mut self.gpu else {
            return PanelAction::None;
        };
        let names: Vec<String> = self.tools.presets.iter().map(|p| p.name.clone()).collect();
        let selected = if self.tools.eraser_mode {
            self.tools.eraser
        } else {
            self.tools.brush
        };
        let eraser_mode = self.tools.eraser_mode;
        let active = self.document.active_layer();
        let editing_mask = self.document.active_part() == LayerPart::Mask;
        let rows: Vec<LayerRow> = self
            .document
            .layers()
            .map(|l| LayerRow {
                key: l.id().get(),
                name: l.name(),
                visible: l.visible(),
                locked: l.locked(),
                opacity: l.opacity(),
                blend: BlendMode::ALL
                    .iter()
                    .position(|&m| m == l.blend_mode())
                    .unwrap_or(0),
                active: l.id() == active,
                clipped: l.clipped(),
                alpha_locked: l.alpha_locked(),
                mask: l.mask().map(|m| m.enabled()),
                editing_mask: l.id() == active && editing_mask,
            })
            .collect();
        let history = HistoryButtons {
            undo: self.document.next_undo().map(history_label),
            redo: self.document.next_redo().map(history_label),
        };
        let mut action = PanelAction::None;
        let mut layer_action = None;
        let mut history_command = None;
        let (tools, engine, panel) = (&mut self.tools, &mut self.engine, &mut self.panel);
        let repaint = gpu.ui.build(&gpu.window, |ui| {
            let layers = egui::Panel::right("layer-panel")
                .default_size(230.0)
                .show(ui, |ui| {
                    history_command = tela_ui::history_bar(ui, &history);
                    ui.separator();
                    layer_action = tela_ui::layer_panel(ui, &rows, &BLEND_LABELS);
                });
            let r = egui::Panel::left("brush-panel")
                .default_size(290.0)
                .show(ui, |ui| {
                    action = panel.show(
                        ui,
                        PanelModel {
                            preset_names: &names,
                            selected,
                            eraser_mode,
                            preset: tools.active_mut(),
                            stroke: &mut engine.stroke_defaults,
                            pressure_curve: &mut engine.pressure_curve,
                        },
                    );
                });
            vec![r.response.rect, layers.response.rect]
        });
        #[cfg(windows)]
        if let Some(ink) = &gpu.ink {
            ink.set_passthrough(gpu.ui.occupied());
        }
        if repaint {
            gpu.window.request_redraw();
        }
        match (history_command, layer_action) {
            (Some(c), _) => PanelAction::History(c),
            (None, Some(a)) => PanelAction::Layer(a),
            (None, None) => action,
        }
    }

    fn apply(&mut self, action: PanelAction) {
        match action {
            PanelAction::None => return,
            PanelAction::SelectPreset(i) => self.tools.select(i),
            PanelAction::ToggleEraser => self.tools.eraser_mode = !self.tools.eraser_mode,
            PanelAction::ClearLayer => self.clear_canvas(),
            PanelAction::Layer(a) => self.apply_layer(a),
            PanelAction::History(c) => self.apply_history(c),
        }
        self.request_redraw();
    }

    fn redraw(&mut self) {
        // Airbrush presets keep depositing while the pen rests, so keep
        // frames coming for the duration of such a stroke.
        if self.engine.is_stroking() && self.engine.is_time_based() {
            self.engine.advance_time(clock::now());
            self.request_redraw();
        }
        let t = Instant::now();
        let tiles = self.refresh_tiles();
        if tiles > 0 {
            self.stats.frame(t.elapsed().as_secs_f64(), tiles);
        }
        let action = self.build_ui();
        self.apply(action);
        let Some(gpu) = &mut self.gpu else {
            return;
        };
        let bg = self.document.background();
        let frame = Frame {
            view: &self.view,
            paper: [bg.r, bg.g, bg.b, bg.a],
            workspace: WORKSPACE_COLOR,
        };
        gpu.renderer.render_with(&frame, Some(&mut gpu.ui));
        self.update_title();
        let before = self.stats.summary.len();
        self.stats.submitted();
        if self.show_stats && self.stats.summary.len() != before {
            self.update_title();
        }
    }
}

/// Blend-mode labels in `BlendMode::ALL` order.
const BLEND_LABELS: [&str; 17] = [
    "Normal",
    "Multiply",
    "Screen",
    "Overlay",
    "Darken",
    "Lighten",
    "Color Dodge",
    "Color Burn",
    "Hard Light",
    "Soft Light",
    "Difference",
    "Exclusion",
    "Hue",
    "Saturation",
    "Color",
    "Luminosity",
    "Add",
];
const _: () = assert!(BLEND_LABELS.len() == BlendMode::ALL.len());

/// Button tooltip text for a history step.
fn history_label(action: HistoryAction) -> &'static str {
    match action {
        HistoryAction::Paint => "paint",
        HistoryAction::ClearLayer => "clear layer",
        HistoryAction::AddLayer => "new layer",
        HistoryAction::RemoveLayer => "delete layer",
        HistoryAction::MoveLayer => "move layer",
        HistoryAction::Opacity => "opacity",
        HistoryAction::BlendMode => "blend mode",
        HistoryAction::Rename => "rename",
        HistoryAction::Lock => "lock",
        HistoryAction::AlphaLock => "lock transparent pixels",
        HistoryAction::Clip => "clipping",
        HistoryAction::AddMask => "new mask",
        HistoryAction::RemoveMask => "delete mask",
        HistoryAction::MaskEnabled => "enable mask",
        HistoryAction::PaintMask => "paint mask",
    }
}

#[cfg(windows)]
fn attach_ink(
    window: &Window,
    proxy: EventLoopProxy<AppEvent>,
) -> Option<tela_tablet::windows::InkBackend> {
    match tela_tablet::windows::InkBackend::attach(window, move || {
        let _ = proxy.send_event(AppEvent::Tablet);
    }) {
        Ok(ink) => {
            log::info!("Windows Ink pen input enabled");
            Some(ink)
        }
        Err(e) => {
            log::warn!("{e}; pen input falls back to mouse emulation");
            None
        }
    }
}

impl ApplicationHandler<AppEvent> for App {
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
            Ok(mut renderer) => {
                if let Err(e) =
                    renderer.set_document_size(self.document.width(), self.document.height())
                {
                    log::error!("{e}");
                    event_loop.exit();
                    return;
                }
                self.stale.extend(&self.document.content_tiles());
                let ui = UiLayer::new(&window, renderer.device(), renderer.format());
                self.gpu = Some(Gpu {
                    ui,
                    #[cfg(windows)]
                    ink: attach_ink(&window, self.proxy.clone()),
                    window,
                    renderer,
                });
                // Lay the panel out once so fitting knows its width.
                self.build_ui();
                self.fit_view();
                self.update_title();
                self.request_redraw();
            }
            Err(e) => {
                log::error!("{e}");
                event_loop.exit();
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Tablet => self.drain_tablet(),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(gpu) = &mut self.gpu {
            let response = gpu.ui.on_window_event(&gpu.window, &event);
            if response.repaint {
                gpu.window.request_redraw();
            }
            // Input the UI took (or that lands on it outside a stroke) never
            // reaches the canvas.
            let ui_input = match &event {
                WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. } => {
                    self.drag == Drag::None && (response.consumed || gpu.ui.wants_pointer())
                }
                WindowEvent::KeyboardInput { .. } => response.consumed || gpu.ui.wants_keyboard(),
                _ => false,
            };
            if ui_input {
                if let WindowEvent::CursorMoved { position, .. } = &event {
                    self.cursor = Vec2::new(position.x as f32, position.y as f32);
                }
                return;
            }
        }
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
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_button(button, state),
            WindowEvent::MouseWheel { delta, .. } => self.on_wheel(delta),
            WindowEvent::Focused(false) => {
                // Releases outside the window would otherwise leave a drag stuck.
                if self.drag == Drag::Paint {
                    self.finish_stroke();
                }
                self.drag = Drag::None;
                self.mouse_left_down = false;
                self.space_held = false;
                self.request_redraw();
            }
            _ => {}
        }
    }
}
