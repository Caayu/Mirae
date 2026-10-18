//! The gradient panel (phase 5F): the gradient tool's options — its shape,
//! what it draws beyond its ends, how colours mix, dithering, two presets and a
//! small stop editor (a bar showing the gradient with a marker per stop:
//! click to select, drag to move; the selected stop's colour, opacity and
//! position). Like the other panels it edits nothing: it shows what the
//! app lends it ([`GradientModel`]) and returns a [`GradientAction`]. The
//! guide drawn over the canvas while dragging is [`gradient_guide`].

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use tela_color::Color;
use tela_color::gradient::{ColorRamp, GradientStop, GradientStops, Mixing};

use crate::style::{self, space};

/// How a pixel's position follows from the drag (the app maps it to the
/// document's shape).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
    Elliptical,
    Angular,
    Reflected,
}

impl GradientKind {
    pub const ALL: [GradientKind; 5] = [
        GradientKind::Linear,
        GradientKind::Radial,
        GradientKind::Elliptical,
        GradientKind::Angular,
        GradientKind::Reflected,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GradientKind::Linear => "Linear",
            GradientKind::Radial => "Radial",
            GradientKind::Elliptical => "Ellipse",
            GradientKind::Angular => "Angular",
            GradientKind::Reflected => "Reflected",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            GradientKind::Linear => "Along the drag",
            GradientKind::Radial => "Circles around the press; the drag is the radius",
            GradientKind::Elliptical => "An ellipse around the press; the drag sets its radii",
            GradientKind::Angular => "Around the press, starting along the drag",
            GradientKind::Reflected => "Along the drag, mirrored at the press",
        }
    }
}

/// What is drawn beyond the ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GradientEdge {
    #[default]
    Clamp,
    Repeat,
    Reflect,
    Empty,
}

impl GradientEdge {
    pub const ALL: [GradientEdge; 4] = [
        GradientEdge::Clamp,
        GradientEdge::Repeat,
        GradientEdge::Reflect,
        GradientEdge::Empty,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GradientEdge::Clamp => "Extend",
            GradientEdge::Repeat => "Repeat",
            GradientEdge::Reflect => "Mirror",
            GradientEdge::Empty => "None",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            GradientEdge::Clamp => "The end colours continue",
            GradientEdge::Repeat => "Start again every length",
            GradientEdge::Reflect => "Run back and forth",
            GradientEdge::Empty => "Draw nothing beyond the ends",
        }
    }
}

/// Where a stop's colour comes from: the main or sub colour when the drag
/// begins, or a fixed colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StopColor {
    Main,
    Sub,
    Custom(Color),
}

impl StopColor {
    pub fn resolve(self, main: Color, sub: Color) -> Color {
        match self {
            StopColor::Main => main,
            StopColor::Sub => sub,
            StopColor::Custom(c) => c,
        }
    }
}

/// One stop as the panel shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StopRow {
    /// `0..=1`.
    pub position: f32,
    pub color: StopColor,
    /// `0..=1`.
    pub opacity: f32,
}

/// The simple presets (the reference's two basic sub-tools, GD1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientPreset {
    /// Main colour to sub colour.
    MainToSub,
    /// Main colour to transparent.
    MainToTransparent,
}

/// What the panel shows this frame.
#[derive(Clone, Copy, Debug)]
pub struct GradientModel<'a> {
    pub kind: GradientKind,
    pub edge: GradientEdge,
    pub mixing: Mixing,
    pub dither: bool,
    /// In order of position.
    pub stops: &'a [StopRow],
    /// Index into `stops`.
    pub selected: usize,
    /// The colours `Main` and `Sub` stand for now.
    pub main: Color,
    pub sub: Color,
}

/// What the user asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientAction {
    Kind(GradientKind),
    Edge(GradientEdge),
    Mixing(Mixing),
    Dither(bool),
    Preset(GradientPreset),
    /// Select stop `i`.
    Select(usize),
    /// The selected stop's position, `0..=1`.
    Position(f32),
    /// The selected stop's opacity, `0..=1`.
    Opacity(f32),
    /// The selected stop's colour source.
    Color(StopColor),
    /// Give the selected stop the active colour (as a fixed colour).
    UseActiveColor,
    /// Add a stop in the middle of the selected one's segment.
    Add,
    /// Remove the selected stop (one always remains).
    Remove,
    /// Reverse the stops.
    Reverse,
}

const BAR_HEIGHT: f32 = 18.0;
const MARKER: f32 = 6.0;

/// The gradient's stops resolved to colours (for drawing).
fn resolved(m: &GradientModel) -> Option<GradientStops> {
    GradientStops::new(
        m.stops
            .iter()
            .map(|s| GradientStop::new(s.position, s.color.resolve(m.main, m.sub), s.opacity)),
    )
}

fn color32(c: tela_color::Rgba) -> Color32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    Color32::from_rgba_unmultiplied(q(c.r), q(c.g), q(c.b), q(c.a))
}

/// Draws the gradient tool's options (the tool itself is chosen in the
/// tool strip).
pub fn gradient_panel(ui: &mut Ui, m: &GradientModel) -> Option<GradientAction> {
    let mut action = None;
    style::row(ui, "Preset", |ui| {
        if ui
            .button("Main to Sub")
            .on_hover_text("From the main colour to the sub colour")
            .clicked()
        {
            action = Some(GradientAction::Preset(GradientPreset::MainToSub));
        }
        if ui
            .button("Main to clear")
            .on_hover_text("From the main colour to transparent")
            .clicked()
        {
            action = Some(GradientAction::Preset(GradientPreset::MainToTransparent));
        }
    });
    style::row(ui, "Shape", |ui| {
        let options: Vec<(GradientKind, &str, &str)> = GradientKind::ALL
            .iter()
            .map(|&k| (k, k.label(), k.hint()))
            .collect();
        if let Some(k) = style::segmented(ui, m.kind, &options) {
            action = Some(GradientAction::Kind(k));
        }
    });
    style::row(ui, "Ends", |ui| {
        let options: Vec<(GradientEdge, &str, &str)> = GradientEdge::ALL
            .iter()
            .map(|&e| (e, e.label(), e.hint()))
            .collect();
        if let Some(e) = style::segmented(ui, m.edge, &options) {
            action = Some(GradientAction::Edge(e));
        }
    });
    style::row(ui, "Mix", |ui| {
        let options = [
            (Mixing::Perceptual, "Perceptual", "OKLab: even lightness"),
            (Mixing::Standard, "sRGB", "The stored (sRGB-encoded) values"),
            (
                Mixing::Linear,
                "Linear light",
                "Linear light (physical mixing)",
            ),
        ];
        if let Some(mixing) = style::segmented(ui, m.mixing, &options) {
            action = Some(GradientAction::Mixing(mixing));
        }
    });
    style::row(ui, "", |ui| {
        let mut dither = m.dither;
        if ui
            .checkbox(&mut dither, "Dither")
            .on_hover_text("Ordered dither at the document's precision against banding")
            .changed()
        {
            action = Some(GradientAction::Dither(dither));
        }
    });
    ui.add_space(space::S);
    if let Some(a) = stop_bar(ui, m) {
        action = Some(a);
    }
    if let Some(a) = stop_editor(ui, m) {
        action = Some(a);
    }
    action
}

/// The bar with the gradient over a checkerboard and a marker per stop.
/// It is inset by a marker's half width on both sides, so the markers of
/// the end stops are whole.
fn stop_bar(ui: &mut Ui, m: &GradientModel) -> Option<GradientAction> {
    let mut action = None;
    let full = ui.available_width().max(60.0);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(full, BAR_HEIGHT + 2.0 * MARKER + 2.0),
        Sense::hover(),
    );
    let width = full - 2.0 * MARKER - 2.0;
    let bar = Rect::from_min_size(
        Pos2::new(rect.left() + MARKER + 1.0, rect.top()),
        Vec2::new(width, BAR_HEIGHT),
    );
    let painter = ui.painter_at(rect);
    // Checkerboard, so transparency shows.
    let cell = BAR_HEIGHT / 2.0;
    let mut x = bar.left();
    let mut k = 0;
    while x < bar.right() {
        for row in 0..2 {
            let shade = if (k + row) % 2 == 0 { 200 } else { 140 };
            let r = Rect::from_min_size(
                Pos2::new(x, bar.top() + row as f32 * cell),
                Vec2::new(cell.min(bar.right() - x), cell),
            );
            painter.rect_filled(r, 0.0, Color32::from_gray(shade));
        }
        x += cell;
        k += 1;
    }
    if let Some(stops) = resolved(m) {
        let ramp = ColorRamp::new(&stops, m.mixing);
        let steps = (width / 2.0).max(2.0) as usize;
        let mut mesh = egui::Mesh::default();
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let c = color32(ramp.eval(t));
            let x = bar.left() + t * width;
            mesh.colored_vertex(Pos2::new(x, bar.top()), c);
            mesh.colored_vertex(Pos2::new(x, bar.bottom()), c);
            if i > 0 {
                let j = (2 * i) as u32;
                mesh.add_triangle(j - 2, j - 1, j);
                mesh.add_triangle(j - 1, j + 1, j);
            }
        }
        painter.add(Shape::mesh(mesh));
    }
    painter.rect_stroke(
        bar,
        0.0,
        Stroke::new(1.0, style::color::BORDER),
        egui::StrokeKind::Inside,
    );
    for (i, s) in m.stops.iter().enumerate() {
        let x = bar.left() + s.position * width;
        let tip = Pos2::new(x, bar.bottom() + 1.0);
        let fill = color32(s.color.resolve(m.main, m.sub).opaque());
        let selected = i == m.selected;
        let outline = if selected {
            Stroke::new(2.0, style::color::ACCENT_HOVER)
        } else {
            Stroke::new(1.0, Color32::BLACK)
        };
        painter.add(Shape::convex_polygon(
            vec![
                tip,
                Pos2::new(x + MARKER, tip.y + 2.0 * MARKER),
                Pos2::new(x - MARKER, tip.y + 2.0 * MARKER),
            ],
            fill,
            outline,
        ));
        let hit = Rect::from_center_size(
            Pos2::new(x, tip.y + MARKER),
            Vec2::new(2.0 * MARKER + 2.0, 2.0 * MARKER + 2.0),
        );
        let response = ui
            .interact(
                hit,
                ui.id().with(("gradient-stop", i)),
                Sense::click_and_drag(),
            )
            .on_hover_text("Click to select this stop; drag to move it");
        if response.clicked() || response.drag_started() {
            action = Some(GradientAction::Select(i));
        }
        if response.dragged()
            && selected
            && let Some(p) = response.interact_pointer_pos()
        {
            action = Some(GradientAction::Position(
                ((p.x - bar.left()) / width).clamp(0.0, 1.0),
            ));
        }
    }
    action
}

/// The selected stop's colour source, opacity and position; add, remove,
/// reverse.
fn stop_editor(ui: &mut Ui, m: &GradientModel) -> Option<GradientAction> {
    let mut action = None;
    let s = m.stops.get(m.selected).copied()?;
    let title = format!("Stop {} of {}", m.selected + 1, m.stops.len());
    style::row(ui, &title, |ui| {
        let current = match s.color {
            StopColor::Main => 0,
            StopColor::Sub => 1,
            StopColor::Custom(_) => 2,
        };
        let options = [
            (0, "Main", "The main colour when the drag begins"),
            (1, "Sub", "The sub colour when the drag begins"),
            (2, "Fixed", "Fix this stop to the active colour now"),
        ];
        match style::segmented(ui, current, &options) {
            Some(0) => action = Some(GradientAction::Color(StopColor::Main)),
            Some(1) => action = Some(GradientAction::Color(StopColor::Sub)),
            Some(_) => action = Some(GradientAction::UseActiveColor),
            None => {}
        }
    });
    style::row(ui, "Opacity", |ui| {
        let mut opacity = s.opacity * 100.0;
        if ui
            .add(
                egui::Slider::new(&mut opacity, 0.0..=100.0)
                    .fixed_decimals(0)
                    .suffix(" %"),
            )
            .changed()
        {
            action = Some(GradientAction::Opacity(opacity / 100.0));
        }
    });
    style::row(ui, "Position", |ui| {
        let mut position = s.position * 100.0;
        if ui
            .add(
                egui::Slider::new(&mut position, 0.0..=100.0)
                    .fixed_decimals(1)
                    .suffix(" %"),
            )
            .changed()
        {
            action = Some(GradientAction::Position(position / 100.0));
        }
    });
    style::row(ui, "", |ui| {
        if ui
            .button("Add stop")
            .on_hover_text("A stop halfway along the selected stop's segment")
            .clicked()
        {
            action = Some(GradientAction::Add);
        }
        if ui
            .add_enabled(m.stops.len() > 1, egui::Button::new("Remove"))
            .on_hover_text("Remove the selected stop")
            .clicked()
        {
            action = Some(GradientAction::Remove);
        }
        if ui
            .button("Reverse")
            .on_hover_text("Reverse the order of the stops")
            .clicked()
        {
            action = Some(GradientAction::Reverse);
        }
    });
    action
}

/// What the guide over the canvas shows while dragging, in screen points:
/// the drag's line and, for radial and elliptical gradients, the
/// outline where the gradient ends.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientGuide {
    pub start: Pos2,
    pub end: Pos2,
    /// A closed outline (circle or ellipse) to draw, if any.
    pub outline: Vec<Pos2>,
}

/// Paints the drag guide on egui's background layer (under the panels):
/// presentation only, like the reference's guide line (GD37).
pub fn gradient_guide(ctx: &egui::Context, g: &GradientGuide) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let white = Stroke::new(3.0, Color32::from_white_alpha(200));
    let black = Stroke::new(1.0, Color32::BLACK);
    for stroke in [white, black] {
        painter.line_segment([g.start, g.end], stroke);
        if g.outline.len() > 2 {
            let mut closed = g.outline.clone();
            closed.push(g.outline[0]);
            painter.add(Shape::line(closed, stroke));
        }
    }
    for p in [g.start, g.end] {
        painter.circle(p, 3.5, Color32::WHITE, Stroke::new(1.0, Color32::BLACK));
    }
}
