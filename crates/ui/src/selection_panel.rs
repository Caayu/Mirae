//! The selection panel (tools, mode, anti-aliasing, commands, feather) and
//! the selection outline drawn over the canvas. The panel edits nothing:
//! it shows what the app lends it ([`SelectionModel`]) and returns a
//! [`SelectionAction`]. The outline is presentation only: the app passes
//! loops already mapped to the screen.

use egui::{Color32, Pos2, Shape, Stroke, Ui};

use crate::style;

/// The selection shape a drag makes (the reference's sub-tools).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionKind {
    #[default]
    Rectangle,
    Ellipse,
    Lasso,
}

impl SelectionKind {
    pub const ALL: [SelectionKind; 3] = [
        SelectionKind::Rectangle,
        SelectionKind::Ellipse,
        SelectionKind::Lasso,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SelectionKind::Rectangle => "Rectangle",
            SelectionKind::Ellipse => "Ellipse",
            SelectionKind::Lasso => "Lasso",
        }
    }
}

/// How a new shape combines with the selection (the reference's
/// "Modo de seleção"); Shift, Alt and Shift+Alt at the press override it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    #[default]
    New,
    Add,
    Subtract,
    Intersect,
}

impl SelectionMode {
    pub const ALL: [SelectionMode; 4] = [
        SelectionMode::New,
        SelectionMode::Add,
        SelectionMode::Subtract,
        SelectionMode::Intersect,
    ];

    fn label(self) -> (&'static str, &'static str) {
        match self {
            SelectionMode::New => ("New", "Replace the selection"),
            SelectionMode::Add => ("Add", "Add to the selection (Shift at the press)"),
            SelectionMode::Subtract => {
                ("Subtract", "Subtract from the selection (Alt at the press)")
            }
            SelectionMode::Intersect => (
                "Intersect",
                "Keep only the overlap (Shift+Alt at the press)",
            ),
        }
    }
}

/// What the panel shows this frame.
#[derive(Clone, Copy, Debug)]
pub struct SelectionModel {
    pub kind: SelectionKind,
    pub mode: SelectionMode,
    /// Anti-aliasing of the current kind.
    pub anti_alias: bool,
    /// Feather strength in pixels, as last chosen.
    pub feather: f32,
    /// The document has a selection (the commands that need one are
    /// disabled otherwise, as in the reference).
    pub has_selection: bool,
}

/// What the user asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelectionAction {
    /// Use the selection tool with this shape.
    Tool(SelectionKind),
    Mode(SelectionMode),
    AntiAlias(bool),
    SelectAll,
    Deselect,
    Invert,
    /// Feather by this many pixels now.
    Feather(f32),
    /// The strength field changed (remembered, not applied).
    FeatherStrength(f32),
}

/// Draws the selection tool's options and the selection commands.
pub fn selection_panel(ui: &mut Ui, m: &SelectionModel) -> Option<SelectionAction> {
    let mut action = None;
    style::row(ui, "Shape", |ui| {
        let options: Vec<(SelectionKind, &str, &str)> = SelectionKind::ALL
            .iter()
            .map(|&k| (k, k.label(), "The shape a drag selects"))
            .collect();
        if let Some(kind) = style::segmented(ui, m.kind, &options) {
            action = Some(SelectionAction::Tool(kind));
        }
    });
    style::row(ui, "Mode", |ui| {
        let options: Vec<(SelectionMode, &str, &str)> = SelectionMode::ALL
            .iter()
            .map(|&mode| {
                let (label, tip) = mode.label();
                (mode, label, tip)
            })
            .collect();
        if let Some(mode) = style::segmented(ui, m.mode, &options) {
            action = Some(SelectionAction::Mode(mode));
        }
    });
    style::row(ui, "Edges", |ui| {
        let mut aa = m.anti_alias;
        if ui
            .checkbox(&mut aa, "Anti-aliased")
            .on_hover_text("Anti-aliased edges for this shape")
            .changed()
        {
            action = Some(SelectionAction::AntiAlias(aa));
        }
    });
    style::row(ui, "Select", |ui| {
        if ui
            .button("All")
            .on_hover_text("Select all (Ctrl+A)")
            .clicked()
        {
            action = Some(SelectionAction::SelectAll);
        }
        if ui
            .add_enabled(m.has_selection, egui::Button::new("None"))
            .on_hover_text("Deselect (Ctrl+D)")
            .clicked()
        {
            action = Some(SelectionAction::Deselect);
        }
        if ui
            .add_enabled(m.has_selection, egui::Button::new("Invert"))
            .on_hover_text("Invert the selection (Ctrl+Shift+I)")
            .clicked()
        {
            action = Some(SelectionAction::Invert);
        }
    });
    style::row(ui, "Feather", |ui| {
        let mut px = m.feather;
        if ui
            .add(
                egui::DragValue::new(&mut px)
                    .range(0.1..=1000.0)
                    .speed(0.2)
                    .fixed_decimals(1)
                    .suffix(" px"),
            )
            .on_hover_text("How far the border is blurred")
            .changed()
        {
            action = Some(SelectionAction::FeatherStrength(px));
        }
        if ui
            .add_enabled(m.has_selection, egui::Button::new("Apply"))
            .on_hover_text("Blur the selection's border by this many pixels")
            .on_disabled_hover_text("Needs a selection")
            .clicked()
        {
            action = Some(SelectionAction::Feather(px));
        }
    });
    action
}

/// Screen length of one dash of the outline, in points.
const DASH: f32 = 4.0;

/// Paints marching ants along `loops` (closed, in points) and `open`
/// paths (a shape being dragged), under the panels: a white line with a
/// black dash pattern that moves with `phase` (in points). Draws nothing
/// itself when the lists are empty.
pub fn selection_outline(ctx: &egui::Context, loops: &[Vec<Pos2>], open: &[Pos2], phase: f32) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let white = Stroke::new(1.0, Color32::WHITE);
    let black = Stroke::new(1.0, Color32::BLACK);
    let draw = |path: &[Pos2]| {
        if path.len() < 2 {
            return;
        }
        painter.add(Shape::line(path.to_vec(), white));
        painter.extend(Shape::dashed_line_with_offset(
            path,
            black,
            &[DASH],
            &[DASH],
            phase.rem_euclid(2.0 * DASH),
        ));
    };
    for l in loops {
        let mut closed = l.clone();
        if let Some(&first) = l.first() {
            closed.push(first);
        }
        draw(&closed);
    }
    draw(open);
}
