use std::sync::Arc;
use std::time::Instant;

use glam::Vec2;
use tela_brush::dynamics::{DynamicsSource, InputCaps};
use tela_brush::preset::PaintMode;
use tela_brush::{BrushEngine, BrushInput, BrushPreset, PixelAlpha};
use tela_canvas::{DirtyTiles, Tile, TileCoord, ViewTransform};
use tela_color::{Color, PixelDepth, PremulRgba, with_pixel_type};
use tela_document::{
    BlendMode, ComposeScratch, Document, FillPaint, FillReference, FillTarget, GradientPaint,
    HistoryAction, LayerId, LayerPart, LowerMask, Overlay, OverlayPaint, Placement, Tolerance,
};
use tela_renderer::{Frame, Renderer};
use tela_tablet::{DeviceKind, Phase, TabletEvent, TabletSample, TimestampNormalizer, clock};
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{CursorIcon, Window, WindowId};

mod e2e;
mod gradient_preview;
mod files;
mod transform;

use crate::colors::{ColorState, Slot};
use crate::filling::{self, FillTool};
use crate::gradients::{GradientSession, GradientTool};
use crate::selecting::{SelectionDrag, SelectionTool};
use crate::stats::LatencyStats;
use crate::tools::{CanvasTool, Tools};
use crate::transforming::{TransformSession, TransformTool};
use files::{FileState, Pending};
use tela_ui::{
    BrushPanel, ColorAction, ColorModel, ColorPanel, DocumentCommand, DocumentInfo, FillAction,
    FillModel, GradientAction, GradientModel, HistoryButtons, HistoryCommand, LayerAction,
    LayerRow, MergeMaskChoice, PanelAction, PanelModel, PickSource, SelectionAction,
    SelectionModel, ToolChoice, TransformModel, UiLayer, egui, style,
};
use transform::AfterTransform;

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
    /// The eyedropper: every position picks; the last pick is remembered
    /// as a recent colour when the drag ends.
    Pick(PickSource),
    /// A selection shape being drawn (`App::selection_drag`).
    Select,
    /// A transform handle (or the Move tool) being dragged.
    Transform,
    /// A gradient being dragged (`App::gradient`).
    Gradient,
}

/// Marching-ants speed, points per second.
const ANTS_SPEED: f32 = 8.0;
/// How often the ants move while the document is otherwise idle.
const ANTS_FRAME: std::time::Duration = std::time::Duration::from_millis(125);
/// How long one frame may spend composing a gradient preview (the tiles
/// on screen first); the rest is composed in the next frames.
const GRADIENT_FRAME_BUDGET: std::time::Duration = std::time::Duration::from_millis(12);

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
    /// Main/sub colours, recent colours and swatches (session state, not
    /// saved with the document; ADR 0021).
    colors: ColorState,
    color_panel: ColorPanel,
    /// What left drags do: paint, pick or select.
    tool: CanvasTool,
    /// Selection tool settings (session state, not saved).
    selection: SelectionTool,
    /// Fill tool settings (session state, not saved).
    fill_tool: FillTool,
    /// Gradient tool settings and stops (session state, not saved).
    gradient_tool: GradientTool,
    /// The gradient being dragged, and the tiles its preview last covered.
    gradient: Option<GradientSession>,
    gradient_tiles: DirtyTiles,
    /// Composes the gradient preview in parallel, oldest tiles first.
    preview_workers: gradient_preview::PreviewWorkers,
    /// Which of fill and gradient G selects from another tool: the one
    /// used last (G cycles them, 5E BK4).
    fill_or_gradient: CanvasTool,
    /// The selection shape being drawn, in screen pixels.
    selection_drag: Option<SelectionDrag>,
    /// The selection's outline in document pixels, for the selection
    /// revision it was traced at (`None`: trace again).
    outline: Option<(u64, Vec<Vec<[i32; 2]>>)>,
    /// When the marching ants started (their phase).
    ants_epoch: Instant,
    /// When the ants next move (while a selection or a selection drag is
    /// shown).
    ants_due: Instant,
    /// The cursor last set on the window.
    cursor_icon: CursorIcon,
    pick_source: PickSource,
    /// The colour the current pick drag last set.
    picked: Option<Color>,
    last_title: String,
    /// The document's file, save point and the file commands' status.
    file: FileState,
    /// A merge down waiting for the answer to the lower-mask question
    /// (the layer to merge).
    merging: Option<LayerId>,
    /// The open transform session (session state: not in the document,
    /// its history or its file until committed; ADR 0025).
    transform: Option<TransformSession>,
    /// Transform tool settings (keep aspect, interpolation).
    transform_tool: TransformTool,
    /// What waits for the answer to "Apply the transform?".
    transform_exit: Option<AfterTransform>,
    /// The end-to-end test mode, when `TELA_E2E_DIR` asks for it.
    e2e: Option<e2e::E2e>,
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
        let document = Document::new(w, h);
        Self {
            proxy,
            gpu: None,
            file: FileState::new(&document),
            document,
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
            colors: ColorState::new(),
            color_panel: ColorPanel::default(),
            tool: CanvasTool::Brush,
            selection: SelectionTool::default(),
            fill_tool: FillTool::default(),
            gradient_tool: GradientTool::default(),
            gradient: None,
            gradient_tiles: DirtyTiles::new(),
            preview_workers: gradient_preview::PreviewWorkers::new(),
            fill_or_gradient: CanvasTool::Gradient,
            selection_drag: None,
            outline: None,
            ants_epoch: Instant::now(),
            ants_due: Instant::now(),
            cursor_icon: CursorIcon::Default,
            pick_source: PickSource::Canvas,
            picked: None,
            last_title: String::new(),
            merging: None,
            transform: None,
            transform_tool: TransformTool::default(),
            transform_exit: None,
            e2e: e2e::E2e::from_env(),
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
            // Fit into the area the panels leave free.
            let (lo, hi) = canvas_area(gpu.renderer.size(), gpu.ui.occupied());
            self.view = ViewTransform::fit(doc, (hi - lo).max(Vec2::ONE));
            self.view.pan += lo;
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
            "Mirae — {}{} — {}-bit | {} {:.0}px | stab {} {:.0} | {} | taper {taper} | opacity×pressure {} | {}",
            self.document_name(),
            if self.is_modified() { "*" } else { "" },
            self.document.depth().bits(),
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
            // A transform session takes the canvas (handles, move,
            // rotate); so does the Move tool. The stylus eraser end too:
            // nothing is painted during a transform.
            (Phase::Down, Drag::None)
                if (self.transform.is_some() || self.tool == CanvasTool::Move)
                    && !self.modifiers.alt_key() =>
            {
                if !self.transform_press(pos) {
                    return;
                }
            }
            (Phase::Move | Phase::Hover, Drag::Transform) => self.transform_move(pos),
            (Phase::Up | Phase::Cancel, Drag::Transform) => self.transform_release(pos),
            (Phase::Down, Drag::None) if self.transform.is_some() => return,
            // The eyedropper tool, or Alt with any tool (the reference
            // picks the displayed colour with Alt; parity record 5A, O10).
            // The stylus eraser end keeps erasing.
            // The selection tool: Shift, Alt and Shift+Alt at the press
            // choose the operator (reference 5C S4); the stylus eraser end
            // keeps erasing.
            (Phase::Down, Drag::None)
                if self.tool == CanvasTool::Selection
                    && ev.sample.device_kind != DeviceKind::Eraser =>
            {
                let op = self
                    .selection
                    .op(self.modifiers.shift_key(), self.modifiers.alt_key());
                self.selection_drag = Some(SelectionDrag::begin(&self.selection, op, pos));
                self.drag = Drag::Select;
            }
            (Phase::Move | Phase::Hover, Drag::Select) => {
                if let Some(d) = &mut self.selection_drag {
                    d.move_to(pos, self.modifiers.shift_key());
                }
            }
            (Phase::Up, Drag::Select) => {
                if let Some(d) = &mut self.selection_drag {
                    d.move_to(pos, self.modifiers.shift_key());
                }
                self.finish_selection();
            }
            (Phase::Cancel, Drag::Select) => {
                self.selection_drag = None;
                self.drag = Drag::None;
            }
            (Phase::Down, Drag::None)
                if (self.modifiers.alt_key() && self.tool != CanvasTool::Selection)
                    || (self.tool == CanvasTool::Eyedropper
                        && ev.sample.device_kind != DeviceKind::Eraser) =>
            {
                let source = if self.modifiers.alt_key() {
                    PickSource::Canvas
                } else {
                    self.pick_source
                };
                self.begin_pick(pos, source);
            }
            (Phase::Move | Phase::Hover, Drag::Pick(source)) => self.pick(pos, source),
            (Phase::Up | Phase::Cancel, Drag::Pick(_)) => self.end_pick(),
            // The fill tool fills at the press (Alt still picks, above).
            (Phase::Down, Drag::None) if self.tool == CanvasTool::Fill => {
                self.bucket(pos, ev.sample.device_kind);
            }
            // The gradient tool: press, drag (Shift snaps), release.
            (Phase::Down, Drag::None) if self.tool == CanvasTool::Gradient => {
                self.begin_gradient(pos, ev.sample.device_kind);
            }
            (Phase::Move | Phase::Hover, Drag::Gradient) => self.update_gradient(pos),
            (Phase::Up, Drag::Gradient) => {
                self.update_gradient(pos);
                self.commit_gradient();
            }
            (Phase::Cancel, Drag::Gradient) => self.cancel_gradient(),
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
                // The stroke captures the active colour now (COL2); a paint
                // stroke makes it a recent colour, as in the reference (O9).
                self.engine.color = self.colors.active();
                if self.engine.preset.compositing.mode != PaintMode::Erase {
                    self.colors.remember(self.engine.color);
                }
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
            // Limited by the selection exactly as the preview was (the
            // selection cannot change during a stroke: selection commands
            // and undo are ignored until it ends, like the reference, S27).
            let committed = target.map(|t| match t.part {
                // The commit is monomorphized per depth; the document's depth
                // picks the instance once per stroke.
                LayerPart::Pixels => with_pixel_type!(doc.depth(), P => {
                    doc.modify_tiles_in_selection(t.layer, tiles, |c, tile: &mut Tile<P>, within| {
                        stroke.commit_tile_within(c, tile, t.alpha, within)
                    })
                }),
                LayerPart::Mask => {
                    doc.modify_mask_in_selection(t.layer, tiles, |c, tile, within| {
                        stroke.commit_alpha_tile_within(c, tile, within)
                    })
                }
            });
            if let Some(Err(e)) = committed {
                log::warn!(target: "tela::stroke", "stroke discarded: {e}");
            }
            log::debug!(target: "tela::stroke", "{:?}", self.engine.stats());
        }
    }

    /// A bucket fill at screen point `pos` into the edit target (the active
    /// layer's pixels or mask), with the active colour captured now, or
    /// clearing with the stylus eraser end or eraser mode (the reference's
    /// transparent colour erases, 5E BK24; Mirae has no transparent colour,
    /// debt #36). One history step; hidden and locked targets do nothing
    /// (BK31).
    fn bucket(&mut self, pos: Vec2, device: DeviceKind) {
        let t = StrokeTarget::active(&self.document);
        let target = FillTarget {
            layer: t.layer,
            part: t.part,
        };
        let colour = self.colors.active();
        let paint = if self.tools.eraser_mode || device == DeviceKind::Eraser {
            FillPaint::Erase
        } else {
            FillPaint::Color(colour)
        };
        let fill = self.fill_tool.connected(filling::seed_at(&self.view, pos));
        let started = Instant::now();
        match self.document.fill_connected(target, &fill, paint) {
            Ok(dirty) => {
                log::debug!(
                    target: "tela::fill",
                    "{} tiles in {:.1} ms",
                    dirty.len(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
                if !dirty.is_empty() && matches!(paint, FillPaint::Color(_)) {
                    self.colors.remember(colour);
                }
                self.stale.extend(&dirty);
            }
            Err(e) => log::info!(target: "tela::fill", "not filling: {e}"),
        }
    }

    /// Begins a gradient at screen point `pos` into the edit target (the
    /// active layer's pixels or mask), with the tool's settings and the
    /// main and sub colours captured now; the stylus eraser end or eraser
    /// mode erases by it. Hidden and locked targets take nothing (as
    /// strokes and fills).
    fn begin_gradient(&mut self, pos: Vec2, device: DeviceKind) {
        let t = StrokeTarget::active(&self.document);
        if let Err(e) = self.document.paint_target(t.layer, t.part) {
            log::info!(target: "tela::gradient", "not drawing: {e}");
            return;
        }
        let paint = if self.tools.eraser_mode || device == DeviceKind::Eraser {
            GradientPaint::Erase
        } else {
            GradientPaint::Draw
        };
        let target = FillTarget {
            layer: t.layer,
            part: t.part,
        };
        self.gradient = Some(GradientSession::begin(
            &self.gradient_tool,
            target,
            paint,
            self.colors.main(),
            self.colors.sub(),
            self.view.to_canvas(pos),
        ));
        self.gradient_tiles = DirtyTiles::new();
        self.preview_workers.reset();
        self.drag = Drag::Gradient;
    }

    /// The tiles the gradient preview covers now (and covered before) are
    /// recomposed: nothing accumulates from earlier frames (GR2).
    fn update_gradient(&mut self, pos: Vec2) {
        let snap = self.modifiers.shift_key();
        let Some(session) = &mut self.gradient else {
            return;
        };
        session.update(pos, snap, &self.view);
        let g = session.gradient();
        let covered = self
            .document
            .gradient_preview(session.target, &g, session.paint)
            .map(|p| p.tiles().clone())
            .unwrap_or_default();
        self.stale.extend(&self.gradient_tiles);
        self.stale.extend(&covered);
        self.gradient_tiles = covered;
    }

    /// Draws the gradient into the document: one history step, through the
    /// same function the preview used (GR4, GR5). A drag without length
    /// draws nothing and records nothing (GD24).
    fn commit_gradient(&mut self) {
        self.drag = Drag::None;
        let Some(session) = self.gradient.take() else {
            return;
        };
        self.stale.extend(&std::mem::take(&mut self.gradient_tiles));
        let g = session.gradient();
        let started = Instant::now();
        match self
            .document
            .apply_gradient(session.target, &g, session.paint)
        {
            Ok(dirty) => {
                log::debug!(
                    target: "tela::gradient",
                    "{} tiles in {:.1} ms",
                    dirty.len(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
                if !dirty.is_empty() && session.paint == GradientPaint::Draw {
                    for &c in &session.colors {
                        self.colors.remember(c);
                    }
                }
                self.stale.extend(&dirty);
            }
            Err(e) => log::info!(target: "tela::gradient", "not drawing: {e}"),
        }
    }

    /// Drops the gradient being dragged: the document never changed
    /// (GR3); the preview's tiles recompose without it.
    fn cancel_gradient(&mut self) {
        self.drag = Drag::None;
        self.gradient = None;
        self.stale.extend(&std::mem::take(&mut self.gradient_tiles));
    }

    /// Gradient panel requests.
    fn apply_gradient_action(&mut self, action: GradientAction) {
        self.gradient_tool.apply(action, self.colors.active());
    }

    /// The tool strip: the canvas tool (and eraser mode), or a transform.
    fn choose_tool(&mut self, choice: ToolChoice) {
        let (tool, eraser) = match choice {
            ToolChoice::Brush => (CanvasTool::Brush, false),
            ToolChoice::Eraser => (CanvasTool::Brush, true),
            ToolChoice::Eyedropper => (CanvasTool::Eyedropper, self.tools.eraser_mode),
            ToolChoice::Select => (CanvasTool::Selection, self.tools.eraser_mode),
            ToolChoice::Move => (CanvasTool::Move, self.tools.eraser_mode),
            ToolChoice::Fill => (CanvasTool::Fill, self.tools.eraser_mode),
            ToolChoice::Gradient => (CanvasTool::Gradient, self.tools.eraser_mode),
            ToolChoice::Transform => {
                self.begin_transform();
                return;
            }
        };
        self.tool = tool;
        self.tools.eraser_mode = eraser;
        if matches!(tool, CanvasTool::Fill | CanvasTool::Gradient) {
            self.fill_or_gradient = tool;
        }
    }

    /// Which tool the strip shows as active.
    fn tool_choice(&self) -> ToolChoice {
        if self.transform.is_some() {
            return ToolChoice::Transform;
        }
        match self.tool {
            CanvasTool::Brush if self.tools.eraser_mode => ToolChoice::Eraser,
            CanvasTool::Brush => ToolChoice::Brush,
            CanvasTool::Eyedropper => ToolChoice::Eyedropper,
            CanvasTool::Selection => ToolChoice::Select,
            CanvasTool::Move => ToolChoice::Move,
            CanvasTool::Fill => ToolChoice::Fill,
            CanvasTool::Gradient => ToolChoice::Gradient,
        }
    }

    /// Fill panel requests. Filling waits for a stroke or a drag to end.
    fn apply_fill(&mut self, action: FillAction) {
        match action {
            FillAction::Canvas(canvas) => {
                self.fill_tool.reference = if canvas {
                    FillReference::Composite
                } else {
                    FillReference::Target
                };
            }
            FillAction::Tolerance(t) => self.fill_tool.tolerance = Tolerance::from_percent(t),
            FillAction::AntiAlias(on) => self.fill_tool.anti_alias = on,
            FillAction::FillSelection => {
                if self.engine.is_stroking() || self.drag != Drag::None {
                    return;
                }
                let t = StrokeTarget::active(&self.document);
                let target = FillTarget {
                    layer: t.layer,
                    part: t.part,
                };
                let paint = if self.tools.eraser_mode {
                    FillPaint::Erase
                } else {
                    FillPaint::Color(self.colors.active())
                };
                match self.document.fill_selection(target, paint) {
                    Ok(dirty) => self.stale.extend(&dirty),
                    Err(e) => log::info!(target: "tela::fill", "not filling: {e}"),
                }
            }
        }
    }

    /// Ends a selection drag: the shape, mapped to the document once
    /// through the view, combines with the selection (one history step if
    /// it changed anything; a click in New mode deselects, S5).
    fn finish_selection(&mut self) {
        self.drag = Drag::None;
        if let Some(d) = self.selection_drag.take() {
            let shape = d.shape(&self.view);
            self.document.select(&shape, d.anti_alias, d.op);
        }
    }

    /// The selection outline and the shape being dragged, in egui points
    /// (`ppp` physical pixels per point). The outline is traced once per
    /// selection change; each frame only maps it through the view, keeping
    /// points at least a point apart (a staircase seen from afar becomes
    /// its slope).
    fn outline_on_screen(&mut self, ppp: f32) -> (Vec<Vec<egui::Pos2>>, Vec<egui::Pos2>) {
        let revision = self.document.selection_revision();
        if self.outline.as_ref().is_none_or(|(r, _)| *r != revision) {
            let loops = self
                .document
                .selection()
                .map(|s| s.outline())
                .unwrap_or_default();
            self.outline = Some((revision, loops));
        }
        let view = &self.view;
        let to_point = |p: Vec2| egui::Pos2::new(p.x / ppp, p.y / ppp);
        let mut loops = Vec::new();
        for l in self.outline.as_ref().map_or(&[][..], |(_, l)| l.as_slice()) {
            let mut points: Vec<egui::Pos2> = Vec::with_capacity(l.len());
            for p in l {
                let s = to_point(view.to_screen(Vec2::new(p[0] as f32, p[1] as f32)));
                if points.last().is_none_or(|q| q.distance(s) >= 1.0) {
                    points.push(s);
                }
            }
            if points.len() >= 2 {
                loops.push(points);
            }
        }
        let dragged = self.selection_drag.as_ref().map_or_else(Vec::new, |d| {
            let mut path: Vec<egui::Pos2> = d.preview().into_iter().map(to_point).collect();
            if let Some(&first) = path.first() {
                path.push(first);
            }
            path
        });
        (loops, dragged)
    }

    /// Selection commands. Ignored during a stroke (reference S27) and
    /// while a selection shape is being drawn.
    fn apply_selection(&mut self, action: SelectionAction) {
        let busy = self.engine.is_stroking() || self.drag == Drag::Select;
        let doc = &mut self.document;
        match action {
            SelectionAction::Tool(kind) => {
                self.tool = CanvasTool::Selection;
                self.selection.kind = kind;
            }
            SelectionAction::Mode(mode) => self.selection.mode = mode,
            SelectionAction::AntiAlias(on) => self.selection.set_anti_alias(on),
            SelectionAction::FeatherStrength(px) => self.selection.feather = px,
            _ if busy => {}
            SelectionAction::SelectAll => {
                doc.select_all();
            }
            SelectionAction::Deselect => {
                doc.deselect();
            }
            SelectionAction::Invert => {
                doc.invert_selection();
            }
            SelectionAction::Feather(px) => {
                self.selection.feather = px;
                doc.feather_selection(px);
            }
        }
    }

    /// Starts an eyedropper drag at screen position `pos`.
    fn begin_pick(&mut self, pos: Vec2, source: PickSource) {
        self.drag = Drag::Pick(source);
        self.picked = None;
        self.pick(pos, source);
    }

    /// Sets the active colour from the document pixel under screen
    /// position `pos` — the document pixel at any zoom, not the reduced
    /// display (reference O14). Where there is no colour (outside the
    /// document, or a transparent pixel of the active layer) the colour
    /// does not change: Mirae has no "transparent colour" to switch to
    /// (D-5A-2).
    fn pick(&mut self, pos: Vec2, source: PickSource) {
        let p = self.view.to_canvas(pos);
        let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
        let doc = &self.document;
        let inside = x >= 0 && y >= 0 && x < doc.width() as i32 && y < doc.height() as i32;
        let sample = match source {
            PickSource::Canvas => doc.composite_pixel(x, y, &mut self.compose),
            PickSource::Layer if inside => doc
                .layer(doc.active_layer())
                .map(|l| l.pixels().pixel(x, y)),
            PickSource::Layer => None,
        };
        if let Some(color) = sample.and_then(Color::from_premultiplied) {
            self.colors.set_active(color);
            self.picked = Some(color);
        }
        self.request_redraw();
    }

    /// Ends an eyedropper drag; the picked colour becomes a recent colour
    /// (reference O9).
    fn end_pick(&mut self) {
        self.drag = Drag::None;
        if let Some(c) = self.picked.take() {
            self.colors.remember(c);
        }
        self.request_redraw();
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
            // Right-click picks the displayed colour with any tool, as in
            // the reference (parity record 5A, O10).
            MouseButton::Right if pressed && self.drag == Drag::None => {
                self.begin_pick(self.cursor, PickSource::Canvas);
            }
            MouseButton::Right if !pressed && matches!(self.drag, Drag::Pick(_)) => self.end_pick(),
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
        // While a question is open, keys belong to it.
        if self.file.asking.is_some()
            || self.file.notice.is_some()
            || self.merging.is_some()
            || self.transform_exit.is_some()
        {
            return;
        }
        // During a gradient drag only Escape (Mirae cancels; the reference
        // ignores it, GD38) and the view keys act: the target and the
        // document stay as they were when the drag began.
        if self.drag == Drag::Gradient {
            match event.logical_key {
                Key::Named(NamedKey::Escape) => self.cancel_gradient(),
                Key::Named(NamedKey::Home) => self.fit_view(),
                _ => return,
            }
            self.request_redraw();
            return;
        }
        let (ctrl, shift, alt) = (
            self.modifiers.control_key(),
            self.modifiers.shift_key(),
            self.modifiers.alt_key(),
        );
        // During a transform: Enter commits, Escape cancels, undo and redo
        // walk the session's edits (reference T4, T5, T13); tool and file
        // shortcuts do nothing (T42, T43).
        if self.transform.is_some() {
            if self.drag == Drag::Transform {
                return;
            }
            match (&event.logical_key, event.physical_key) {
                (Key::Named(NamedKey::Enter), _) => self.commit_transform(),
                (Key::Named(NamedKey::Escape), _) => self.cancel_transform(),
                (_, PhysicalKey::Code(KeyCode::KeyZ)) if ctrl => self.transform_history(shift),
                (_, PhysicalKey::Code(KeyCode::KeyY)) if ctrl => self.transform_history(true),
                (Key::Named(NamedKey::Home), _) => self.fit_view(),
                (Key::Named(NamedKey::F3), _) => self.show_stats = !self.show_stats,
                _ => return,
            }
            self.request_redraw();
            return;
        }
        // The reference's Edit › Fill (Alt+Delete, 5E BK32).
        if alt && !ctrl && !shift && event.logical_key == Key::Named(NamedKey::Delete) {
            self.apply_fill(FillAction::FillSelection);
            self.request_redraw();
            return;
        }
        if ctrl && !shift && !alt && event.physical_key == PhysicalKey::Code(KeyCode::KeyT) {
            self.begin_transform();
            self.request_redraw();
            return;
        }
        // File commands: Ctrl+N, Ctrl+O, Ctrl+S, Save As on Ctrl+Shift+S
        // and on the reference's Shift+Alt+S (parity record 4F, F11).
        let file = match event.physical_key {
            PhysicalKey::Code(KeyCode::KeyN) if ctrl && !shift && !alt => {
                Some(DocumentCommand::New(8))
            }
            PhysicalKey::Code(KeyCode::KeyO) if ctrl && !shift && !alt => {
                Some(DocumentCommand::Open)
            }
            PhysicalKey::Code(KeyCode::KeyS) if ctrl && !shift && !alt => {
                Some(DocumentCommand::Save)
            }
            PhysicalKey::Code(KeyCode::KeyS) if shift && (ctrl != alt) => {
                Some(DocumentCommand::SaveAs)
            }
            _ => None,
        };
        if let Some(c) = file {
            self.apply_document(c);
            self.request_redraw();
            return;
        }
        if ctrl {
            let command = match event.physical_key {
                PhysicalKey::Code(KeyCode::KeyN) if shift => {
                    self.apply_layer(LayerAction::Add);
                    None
                }
                // The reference's "merge with layer below" (parity record 5B).
                PhysicalKey::Code(KeyCode::KeyE) if !shift => {
                    self.apply_layer(LayerAction::MergeDown);
                    None
                }
                // The reference's selection shortcuts (5C S1).
                PhysicalKey::Code(KeyCode::KeyA) if !shift && !alt => {
                    self.apply_selection(SelectionAction::SelectAll);
                    None
                }
                PhysicalKey::Code(KeyCode::KeyD) if !shift && !alt => {
                    self.apply_selection(SelectionAction::Deselect);
                    None
                }
                PhysicalKey::Code(KeyCode::KeyI) if shift && !alt => {
                    self.apply_selection(SelectionAction::Invert);
                    None
                }
                // Undo/redo would change the selection under a drag.
                PhysicalKey::Code(KeyCode::KeyZ | KeyCode::KeyY) if self.drag == Drag::Select => {
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
            Key::Named(NamedKey::Escape) if self.drag == Drag::Select => {
                self.selection_drag = None;
                self.drag = Drag::None;
            }
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
                "e" | "E" => {
                    self.tools.eraser_mode = !self.tools.eraser_mode;
                    self.tool = CanvasTool::Brush;
                }
                // The reference's selection tool key (5C S2); B returns to
                // the brush (Mirae's choice: free here).
                "m" | "M" => self.tool = CanvasTool::Selection,
                // The Move tool (Mirae's key; the reference's Move layer
                // tool was not observed, docs/parity/5D-transform.md).
                "k" | "K" => self.tool = CanvasTool::Move,
                // The reference's G cycles gradient and fill (5E BK4, 5F
                // GD1); from another tool it takes the one used last, the
                // gradient at first (the reference's first G).
                "g" | "G" => {
                    self.tool = match self.tool {
                        CanvasTool::Fill => CanvasTool::Gradient,
                        CanvasTool::Gradient => CanvasTool::Fill,
                        _ => self.fill_or_gradient,
                    };
                    self.fill_or_gradient = self.tool;
                }
                // B is the brush itself, so it also leaves eraser mode (E
                // toggles it); before, B after E kept erasing.
                "b" | "B" => {
                    self.tool = CanvasTool::Brush;
                    self.tools.eraser_mode = false;
                }
                // The reference's X: paint with the other colour (O2).
                "x" | "X" => self.colors.toggle_active(),
                // Main black, sub white: unbound in the reference (O3);
                // D is free in Mirae.
                "d" | "D" => self.colors.reset(),
                d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                    let i = (d.as_bytes()[0] - b'1') as usize;
                    self.tools.select(i);
                    self.tool = CanvasTool::Brush;
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
        if self.engine.is_stroking() || matches!(self.drag, Drag::Select | Drag::Gradient) {
            return;
        }
        // During a transform, undo and redo walk its own edits (T13).
        if self.transform.is_some() {
            self.transform_history(command == HistoryCommand::Redo);
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

    /// Document and file commands: new documents of either depth, depth
    /// conversion (one undoable step), open, save, save as, PNG import and
    /// export (see `files`). Ignored while a stroke is in progress, like
    /// layer commands.
    fn apply_document(&mut self, command: DocumentCommand) {
        if self.engine.is_stroking() {
            return;
        }
        // The reference disables New, Open, Save, Save As (and closing)
        // while a transform is open, not exporting (T43).
        if self.transform.is_some() && command != DocumentCommand::ExportPng {
            self.file.status = Some("Apply or cancel the transform first.".into());
            return;
        }
        let depth = |bits| match bits {
            16 => PixelDepth::Rgba16,
            _ => PixelDepth::Rgba8,
        };
        match command {
            DocumentCommand::New(bits) => self.request(Pending::New(depth(bits))),
            DocumentCommand::Convert(bits) => {
                let dirty = self.document.convert_depth(depth(bits));
                self.stale.extend(&dirty);
                log::info!("converted to {bits}-bit");
            }
            DocumentCommand::Open => self.request(Pending::Open),
            DocumentCommand::Save => {
                self.save();
            }
            DocumentCommand::SaveAs => {
                self.save_as();
            }
            DocumentCommand::ImportPng => self.import_png(),
            DocumentCommand::ExportPng => self.export_png(),
        }
        self.update_title();
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
            | LayerAction::Rename(key, _)
            | LayerAction::MoveTo { key, .. }
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
            // Drag and drop: placed relative to the row it was dropped on,
            // by id; the document recomputes clipping groups (ADR 0023).
            LayerAction::MoveTo {
                target: on, above, ..
            } => match doc.find_layer(on) {
                Some(on) if above => doc.move_layer(target, Placement::Above(on)),
                Some(on) => doc.move_layer(target, Placement::Below(on)),
                None => Ok(DirtyTiles::new()),
            },
            LayerAction::Rename(_, name) => doc.set_name(target, name).map(|()| DirtyTiles::new()),
            LayerAction::MergeDown => {
                let lower_masked = doc
                    .layer_below(active)
                    .and_then(|below| doc.layer(below))
                    .and_then(|l| l.mask())
                    .is_some_and(|m| m.enabled());
                if lower_masked {
                    // The reference asks first (Apply / Keep / Cancel).
                    self.merging = Some(active);
                    return;
                }
                doc.merge_down(active, LowerMask::Apply)
            }
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
            LayerAction::Clear => doc.clear_layer(active),
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

    /// Finishes a merge down that waited for the lower-mask question.
    fn answer_merge(&mut self, choice: MergeMaskChoice) {
        let Some(id) = self.merging.take() else {
            return;
        };
        let mask = match choice {
            MergeMaskChoice::Apply => LowerMask::Apply,
            MergeMaskChoice::Keep => LowerMask::Keep,
            MergeMaskChoice::Cancel => return,
        };
        match self.document.merge_down(id, mask) {
            Ok(dirty) => self.stale.extend(&dirty),
            Err(e) => log::info!("merge down refused: {e}"),
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
        // A transform previews through the same per-tile function the
        // commit uses (TR4): the target layer's pixels and mask.
        if let Some(session) = &self.transform {
            let t = session.transform(&self.transform_tool);
            let document = &self.document;
            let source = &session.source;
            let pixels = |c: TileCoord, px: &mut [PremulRgba]| {
                source.transform_pixels_tile(&t, c, &document.selection_tile(c), px);
            };
            let mask = |c: TileCoord, r: &mut [f32]| {
                source.transform_mask_tile(&t, c, r);
            };
            let covered = source.affected_tiles(&t.params);
            let covers = |c: TileCoord| covered.contains(c);
            let overlay = Overlay {
                target: source.layer(),
                paint: OverlayPaint::Layer {
                    pixels: &pixels,
                    mask: &mask,
                },
                covers: &covers,
            };
            let tiles = self.stale.take();
            for &coord in &tiles {
                let has = document.compose_tile_into(coord, Some(&overlay), &mut self.compose);
                gpu.renderer
                    .upload_tile(coord, has.then_some(self.compose.out.as_slice()));
            }
            return tiles.len();
        }
        // A gradient previews through the same per-pixel function its
        // commit uses (GR5). A whole large canvas can take longer than a
        // frame: the tiles on screen go first, then the rest within a
        // budget per frame; what is left waits for the next frames.
        if let Some(session) = &self.gradient {
            let g = session.gradient();
            let size = gpu.renderer.size();
            let corners = [
                Vec2::ZERO,
                Vec2::new(size.x, 0.0),
                Vec2::new(0.0, size.y),
                size,
            ]
            .map(|p| self.view.to_canvas(p));
            let (lo, hi) = corners.iter().fold(
                (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
                |(lo, hi), p| (lo.min(*p), hi.max(*p)),
            );
            let mut visible = DirtyTiles::new();
            visible.mark_rect(tela_canvas::IRect::enclosing(lo.x, lo.y, hi.x, hi.y));
            let mut tiles = self.stale.take();
            self.preview_workers.order(&mut tiles, &visible);
            let renderer = &mut gpu.renderer;
            let done = self.preview_workers.compose(
                &self.document,
                &g,
                session.target,
                session.paint,
                &tiles,
                GRADIENT_FRAME_BUDGET,
                |c, px| renderer.upload_tile(c, px),
            );
            if done < tiles.len() {
                for &c in &tiles[done..] {
                    self.stale.mark(c);
                }
                gpu.window.request_redraw();
            }
            return done;
        }
        let stroke = self.engine.stroke();
        let target = self
            .stroke_target
            .unwrap_or_else(|| StrokeTarget::active(&self.document));
        // Preview and commit share the stroke's compositing rules
        // (paint_over_tile / commit_tile, paint_alpha_tile /
        // commit_alpha_tile), with the target fixed at stroke start, and
        // the same selection coverage per tile (SEL5).
        let document = &self.document;
        let pixels = |c: TileCoord, px: &mut [PremulRgba]| {
            if let Some(s) = stroke {
                s.paint_over_tile_within(c, px, target.alpha, document.selection_tile(c));
            }
        };
        let mask = |c: TileCoord, values: &mut [f32]| {
            if let Some(s) = stroke {
                s.paint_alpha_tile_within(c, values, document.selection_tile(c));
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
        let name = self.document_name();
        let modified = self.is_modified();
        let ppp = self
            .gpu
            .as_ref()
            .map_or(1.0, |g| g.window.scale_factor() as f32);
        // During a transform the box replaces the marching ants (the
        // selection follows the content when it is committed, T32).
        let (loops, dragged) = if self.transform.is_some() {
            (Vec::new(), Vec::new())
        } else {
            self.outline_on_screen(ppp)
        };
        let transform_box = self.transform_box(ppp);
        let transform_model = {
            let p = self.transform.as_ref().map(TransformSession::params);
            TransformModel {
                active: self.transform.is_some(),
                width: p.map_or(100.0, |p| p.scale[0] * 100.0),
                height: p.map_or(100.0, |p| p.scale[1] * 100.0),
                angle: p.map_or(0.0, |p| p.rotation.to_degrees()),
                keep_aspect: self.transform_tool.keep_aspect,
                nearest: self.transform_tool.interpolation == tela_document::Interpolation::Nearest,
            }
        };
        let mut transform_action = None;
        let asking_apply = self.transform_exit.is_some();
        let mut apply_answer = None;
        let phase = self.ants_epoch.elapsed().as_secs_f32() * ANTS_SPEED;
        let selection_model = SelectionModel {
            kind: self.selection.kind,
            mode: self.selection.mode,
            anti_alias: self.selection.anti_alias(),
            feather: self.selection.feather,
            has_selection: self.document.has_selection(),
        };
        let mut selection_action = None;
        let fill_model = FillModel {
            canvas: self.fill_tool.reference == FillReference::Composite,
            tolerance: self.fill_tool.tolerance.percent(),
            anti_alias: self.fill_tool.anti_alias,
            has_selection: self.document.has_selection(),
        };
        let mut fill_action = None;
        let gradient_model = GradientModel {
            kind: self.gradient_tool.kind,
            edge: self.gradient_tool.edge,
            mixing: self.gradient_tool.mixing,
            dither: self.gradient_tool.dither,
            stops: self.gradient_tool.stops(),
            selected: self.gradient_tool.selected(),
            main: self.colors.main(),
            sub: self.colors.sub(),
        };
        let mut gradient_action = None;
        let gradient_guide = self.gradient.as_ref().map(|g| g.guide(&self.view, ppp));
        let active_tool = self.tool_choice();
        let mut tool_choice = None;
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
        let info = DocumentInfo {
            name: &name,
            modified,
            status: self.file.status.as_deref(),
            width: self.document.width(),
            height: self.document.height(),
            bits: self.document.depth().bits(),
        };
        let mut action = PanelAction::None;
        let mut layer_action = None;
        let mut history_command = None;
        let mut document_command = None;
        let mut color_action = None;
        let color_model = ColorModel {
            main: self.colors.main(),
            sub: self.colors.sub(),
            main_active: self.colors.active_slot() == Slot::Main,
            recent: self.colors.recent(),
            swatches: self.colors.swatches(),
            pick_source: self.pick_source,
        };
        let color_panel = &mut self.color_panel;
        let (tools, engine, panel) = (&mut self.tools, &mut self.engine, &mut self.panel);
        let export = &mut self.file.export;
        let asking = self.file.asking.as_ref().map(Pending::doing);
        let notice = self.file.notice.as_deref();
        let mut prompt = None;
        let mut dismissed = false;
        let merging = self.merging.is_some();
        let mut merge_answer = None;
        let repaint = gpu.ui.build(&gpu.window, |ui| {
            // Painted first, on the background layer: the panels cover it.
            tela_ui::selection_outline(ui.ctx(), &loops, &dragged, phase);
            if let Some(b) = &transform_box {
                tela_ui::transform_box(ui.ctx(), b);
            }
            if let Some(g) = &gradient_guide {
                tela_ui::gradient_guide(ui.ctx(), g);
            }
            // The layout (docs/architecture/ui.md): the top bar, the tool
            // strip and the active tool's options at the left, documents
            // and layers at the right. Fixed widths, so nothing moves under
            // the cursor; every panel scrolls, so every control can be
            // reached in a small window.
            let top = egui::Panel::top("top-bar")
                .resizable(false)
                .exact_size(style::TOP_BAR_HEIGHT)
                .frame(style::panel_frame(style::color::CHROME))
                .show(ui, |ui| {
                    (document_command, history_command) =
                        tela_ui::top_bar(ui, &info, &history);
                });
            let strip = egui::Panel::left("tool-strip")
                .resizable(false)
                .exact_size(style::TOOL_STRIP_WIDTH)
                .frame(style::panel_frame(style::color::CHROME))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("tool-strip-scroll")
                        .show(ui, |ui| {
                            tool_choice = tela_ui::tool_strip(ui, active_tool);
                        });
                });
            let options = egui::Panel::left("tool-options")
                .resizable(false)
                .exact_size(style::OPTIONS_WIDTH)
                .frame(style::panel_frame(style::color::PANEL))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("tool-options-scroll")
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            egui::CollapsingHeader::new("Colour")
                                .default_open(true)
                                .show(ui, |ui| {
                                    color_action = color_panel.show(ui, &color_model);
                                });
                            ui.separator();
                            ui.heading(active_tool.label());
                            match active_tool {
                                ToolChoice::Brush | ToolChoice::Eraser => {
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
                                }
                                ToolChoice::Eyedropper => style::hint(
                                    ui,
                                    "Click or drag on the canvas to pick a colour. \
                                     Right-click or Alt picks with any tool. \
                                     Where it reads: Pick from, above.",
                                ),
                                ToolChoice::Select => {
                                    selection_action =
                                        tela_ui::selection_panel(ui, &selection_model);
                                }
                                ToolChoice::Fill => {
                                    fill_action = tela_ui::fill_panel(ui, &fill_model);
                                }
                                ToolChoice::Gradient => {
                                    gradient_action =
                                        tela_ui::gradient_panel(ui, &gradient_model);
                                }
                                ToolChoice::Move | ToolChoice::Transform => {
                                    transform_action =
                                        tela_ui::transform_panel(ui, &transform_model);
                                }
                            }
                        });
                });
            // A fixed width: long names and markers are truncated, so the
            // controls never move under the cursor (debt #17).
            let layers = egui::Panel::right("layer-panel")
                .resizable(false)
                .exact_size(style::LAYERS_WIDTH)
                .frame(style::panel_frame(style::color::PANEL))
                .show(ui, |ui| {
                    egui::CollapsingHeader::new("Document")
                        .default_open(true)
                        .show(ui, |ui| {
                            document_command = document_command
                                .or(tela_ui::document_panel(ui, &info, export));
                        });
                    ui.separator();
                    ui.heading("Layers");
                    layer_action = tela_ui::layer_panel(ui, &rows, &BLEND_LABELS);
                });
            let mut occupied = vec![
                top.response.rect,
                strip.response.rect,
                options.response.rect,
                layers.response.rect,
            ];
            // Questions block the whole window (the pen goes to them too).
            if asking_apply {
                let (choice, area) = tela_ui::apply_transform_prompt(ui.ctx());
                apply_answer = choice;
                occupied.push(area);
            } else if merging {
                let (choice, area) = tela_ui::merge_mask_prompt(ui.ctx());
                merge_answer = choice;
                occupied.push(area);
            } else if let Some(doing) = asking {
                let (choice, area) = tela_ui::unsaved_prompt(ui.ctx(), &name, doing);
                prompt = choice;
                occupied.push(area);
            } else if let Some(text) = notice {
                let (done, area) = tela_ui::notice(ui.ctx(), text);
                dismissed = done;
                occupied.push(area);
            }
            occupied
        });
        #[cfg(windows)]
        if let Some(ink) = &gpu.ink {
            ink.set_passthrough(gpu.ui.occupied());
        }
        if repaint {
            gpu.window.request_redraw();
        }
        if let Some(choice) = prompt {
            return PanelAction::Prompt(choice);
        }
        if let Some(choice) = merge_answer {
            return PanelAction::MergeMask(choice);
        }
        if let Some(choice) = apply_answer {
            return PanelAction::ApplyTransform(choice);
        }
        if let Some(a) = transform_action {
            return PanelAction::Transform(a);
        }
        if let Some(t) = tool_choice {
            return PanelAction::Tool(t);
        }
        if dismissed {
            return PanelAction::DismissNotice;
        }
        if let Some(a) = selection_action {
            return PanelAction::Selection(a);
        }
        if let Some(a) = fill_action {
            return PanelAction::Fill(a);
        }
        if let Some(a) = gradient_action {
            return PanelAction::Gradient(a);
        }
        match (
            document_command,
            history_command,
            layer_action,
            color_action,
        ) {
            (Some(c), _, _, _) => PanelAction::Document(c),
            (None, Some(c), _, _) => PanelAction::History(c),
            (None, None, Some(a), _) => PanelAction::Layer(a),
            (None, None, None, Some(a)) => PanelAction::Color(a),
            (None, None, None, None) => action,
        }
    }

    /// Colour panel requests. None of them is a document change or a
    /// history step (reference O8).
    fn apply_color(&mut self, action: ColorAction) {
        let c = &mut self.colors;
        match action {
            ColorAction::Set(color) => c.set_active(color),
            ColorAction::SelectMain(main) => c.select(if main { Slot::Main } else { Slot::Sub }),
            ColorAction::Toggle => c.toggle_active(),
            ColorAction::Swap => c.swap(),
            ColorAction::Reset => c.reset(),
            ColorAction::AddSwatch => {
                c.add_swatch();
            }
            ColorAction::RemoveSwatch(i) => c.remove_swatch(i),
            ColorAction::SetPickSource(s) => self.pick_source = s,
        }
    }

    fn apply(&mut self, action: PanelAction) {
        // Another layer, tool or document command during a transform asks
        // first (reference T41, T42).
        if self.ask_before(&action) {
            self.transform_exit = Some(AfterTransform::Panel(action));
            self.request_redraw();
            return;
        }
        match action {
            PanelAction::None => return,
            PanelAction::Transform(a) => self.apply_transform_action(a),
            PanelAction::ApplyTransform(choice) => self.answer_apply(choice),
            PanelAction::SelectPreset(i) => {
                self.tools.select(i);
                self.tool = CanvasTool::Brush;
            }
            PanelAction::Tool(choice) => self.choose_tool(choice),
            PanelAction::Selection(a) => self.apply_selection(a),
            PanelAction::Fill(a) => self.apply_fill(a),
            PanelAction::Gradient(a) => self.apply_gradient_action(a),

            PanelAction::Layer(a) => self.apply_layer(a),
            PanelAction::History(c) => self.apply_history(c),
            PanelAction::Document(c) => self.apply_document(c),
            PanelAction::Prompt(choice) => {
                self.answer(choice);
                self.update_title();
            }
            PanelAction::MergeMask(choice) => self.answer_merge(choice),
            PanelAction::DismissNotice => self.file.notice = None,
            PanelAction::Color(a) => self.apply_color(a),
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
        // A crosshair while the selection tool is active (5C: the tool is
        // distinguishable from the brush); during a transform, what a
        // press would grab.
        let icon = if let Some(icon) = self.transform_cursor() {
            icon
        } else if matches!(
            self.tool,
            CanvasTool::Selection | CanvasTool::Fill | CanvasTool::Gradient
        ) {
            CursorIcon::Crosshair
        } else {
            CursorIcon::Default
        };
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
        if icon != self.cursor_icon {
            gpu.window.set_cursor(icon);
            self.cursor_icon = icon;
        }
        self.update_title();
        let before = self.stats.summary.len();
        self.stats.submitted();
        if self.show_stats && self.stats.summary.len() != before {
            self.update_title();
        }
        self.e2e_frame();
    }
}

/// The part of a window of `size` the panels in `occupied` leave to the
/// canvas, as its top-left and bottom-right corners (physical pixels).
/// Side panels are the tall rectangles at either side, the top bar the wide
/// one at the top; a modal question (as tall and wide as the window) is
/// not a panel.
fn canvas_area(size: Vec2, occupied: &[[f32; 4]]) -> (Vec2, Vec2) {
    let (mut lo, mut hi) = (Vec2::ZERO, size);
    for r in occupied {
        let (w, h) = (r[2] - r[0], r[3] - r[1]);
        let tall = h >= size.y * 0.5;
        let wide = w >= size.x * 0.5;
        if tall && wide {
            continue;
        }
        if tall && r[0] < size.x * 0.5 {
            lo.x = lo.x.max(r[2]);
        } else if tall {
            hi.x = hi.x.min(r[0]);
        } else if wide && r[1] <= 1.0 {
            lo.y = lo.y.max(r[3]);
        } else if wide {
            hi.y = hi.y.min(r[1]);
        }
    }
    (lo, hi.max(lo + Vec2::ONE))
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
        HistoryAction::ConvertDepth => "convert depth",
        HistoryAction::ImportLayer => "import image",
        HistoryAction::MergeDown => "merge down",
        HistoryAction::Select => "selection",
        HistoryAction::SelectAll => "select all",
        HistoryAction::Deselect => "deselect",
        HistoryAction::InvertSelection => "invert selection",
        HistoryAction::FeatherSelection => "feather selection",
        HistoryAction::Transform => "transform",
        HistoryAction::Fill => "fill",
        HistoryAction::Gradient => "gradient",
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
            .with_title("Mirae")
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
                let mut ui = UiLayer::new(&window, renderer.device(), renderer.format());
                ui.set_inspect(self.e2e.is_some());
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

    /// The marching ants move a few times a second while a selection (or a
    /// selection drag) is shown; otherwise the loop sleeps until an event.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let mut wake = None;
        if self.document.has_selection() || self.selection_drag.is_some() {
            let now = Instant::now();
            if now >= self.ants_due {
                self.request_redraw();
                self.ants_due = now + ANTS_FRAME;
            }
            wake = Some(self.ants_due);
        }
        // The test mode looks for requests while the app is idle.
        if let Some(e2e) = &mut self.e2e {
            let now = Instant::now();
            if now >= e2e.due {
                e2e.due = now + e2e::POLL;
            }
            wake = Some(wake.map_or(e2e.due, |w: Instant| w.min(e2e.due)));
        }
        event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            self.e2e_idle();
            if self.document.has_selection() || self.selection_drag.is_some() {
                self.request_redraw();
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Tablet => {
                if let Some(e2e) = &mut self.e2e {
                    e2e.input();
                }
                self.drain_tablet();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(e2e) = &mut self.e2e
            && matches!(
                event,
                WindowEvent::MouseInput { .. }
                    | WindowEvent::KeyboardInput { .. }
                    | WindowEvent::CursorMoved { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::ModifiersChanged(_)
                    | WindowEvent::Focused(_)
                    | WindowEvent::Resized(_)
            )
        {
            e2e.input();
        }
        if let Some(gpu) = &mut self.gpu {
            let response = gpu.ui.on_window_event(&gpu.window, &event);
            // egui asks for a repaint after every redraw event it sees; a
            // frame must not request the next one, or the app redraws at
            // the display's rate while idle. egui's own requests (an
            // animation, a tooltip) come back from `build` instead.
            if response.repaint && !matches!(event, WindowEvent::RedrawRequested) {
                gpu.window.request_redraw();
            }
            // Input the UI took (or that lands on it outside a stroke) never
            // reaches the canvas.
            // Where the pointer is now: egui's own "over the UI" answer is
            // from the last frame's pointer, so a press that follows a move
            // before any frame (the first click after the window moved,
            // a quick click) would reach the canvas. The panels' areas
            // under the current position decide too.
            let pointer = match &event {
                WindowEvent::CursorMoved { position, .. } => {
                    Vec2::new(position.x as f32, position.y as f32)
                }
                _ => self.cursor,
            };
            let ui_input = match &event {
                WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. } => {
                    self.drag == Drag::None
                        && (response.consumed
                            || gpu.ui.wants_pointer()
                            || gpu.ui.covers(pointer.x, pointer.y))
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
            // Never discard work silently: a modified document asks first
            // (reference F7, F10); Cancel keeps the window open.
            WindowEvent::CloseRequested => {
                if self.transform.is_some() {
                    // Mirae asks whether to apply the transform first (the
                    // reference disables its quit command, T43).
                    self.transform_exit = Some(AfterTransform::Close);
                    self.request_redraw();
                } else if !self.engine.is_stroking() {
                    self.request(Pending::Close);
                }
            }
            WindowEvent::DroppedFile(path) => {
                if !self.engine.is_stroking() && self.transform.is_none() {
                    self.dropped(path);
                    self.update_title();
                    self.request_redraw();
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    // The canvas keeps its place in the middle of the area
                    // the panels leave: half of the change in that area's
                    // size (the panels have fixed widths).
                    let before = gpu.renderer.size();
                    gpu.renderer.resize(size.width, size.height);
                    let after = gpu.renderer.size();
                    if before.x > 0.0 && before.y > 0.0 {
                        self.view.pan += (after - before) * 0.5;
                    }
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
                if self.drag == Drag::Transform {
                    self.transform_release(self.cursor);
                }
                // A gradient whose release will never come is cancelled
                // (nothing was drawn yet), not left previewing; a pick
                // keeps the colour it reached.
                if self.drag == Drag::Gradient {
                    self.cancel_gradient();
                }
                if matches!(self.drag, Drag::Pick(_)) {
                    self.end_pick();
                }
                self.drag = Drag::None;
                self.mouse_left_down = false;
                self.selection_drag = None;
                self.space_held = false;
                self.request_redraw();
            }
            _ => {}
        }
        if self.file.quit {
            event_loop.exit();
        }
    }
}
