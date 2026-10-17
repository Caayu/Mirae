//! The transform panel (start, flip, the numeric fields, interpolation,
//! keep aspect, OK / Cancel) and the transform box drawn over the canvas.
//! Like the other panels it edits nothing: it shows what the app lends it
//! ([`TransformModel`]) and returns a [`TransformAction`]. The box is
//! presentation only: the app passes points already mapped to the screen.

use egui::{Color32, Pos2, Shape, Stroke, Ui, Vec2};

/// What the panel shows this frame.
#[derive(Clone, Copy, Debug)]
pub struct TransformModel {
    /// A transform session is open.
    pub active: bool,
    /// The Move tool is the canvas tool.
    pub moving: bool,
    /// Width and height in percent (negative: flipped), angle in degrees
    /// (clockwise), of the open session.
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub keep_aspect: bool,
    /// Nearest neighbour instead of bicubic.
    pub nearest: bool,
}

/// What the user asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformAction {
    /// Open a transform session (Ctrl+T).
    Start,
    /// Use the Move tool on the canvas (K).
    MoveTool,
    /// Flip the content horizontally / vertically (opens a session when
    /// none is open, as the reference does).
    FlipHorizontal,
    FlipVertical,
    Width(f64),
    Height(f64),
    Angle(f64),
    KeepAspect(bool),
    Nearest(bool),
    /// Back to no change (the reference's reset button).
    Reset,
    /// Commit (Enter) or cancel (Escape) the session.
    Commit,
    Cancel,
}

/// Draws the transform panel.
pub fn transform_panel(ui: &mut Ui, m: &TransformModel) -> Option<TransformAction> {
    let mut action = None;
    ui.label("Transform");
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!m.active, egui::Button::new("Transform"))
            .on_hover_text("Scale, rotate and move the layer or the selection (Ctrl+T)")
            .clicked()
        {
            action = Some(TransformAction::Start);
        }
        if ui
            .selectable_label(m.moving, "Move")
            .on_hover_text("Drag to move the layer or the selection (K)")
            .clicked()
        {
            action = Some(TransformAction::MoveTool);
        }
        if ui
            .button("Flip H")
            .on_hover_text("Flip horizontally")
            .clicked()
        {
            action = Some(TransformAction::FlipHorizontal);
        }
        if ui
            .button("Flip V")
            .on_hover_text("Flip vertically")
            .clicked()
        {
            action = Some(TransformAction::FlipVertical);
        }
    });
    if !m.active {
        return action;
    }
    ui.horizontal(|ui| {
        let mut w = m.width;
        ui.label("W");
        if ui
            .add(
                egui::DragValue::new(&mut w)
                    .range(-10_000.0..=10_000.0)
                    .speed(0.5)
                    .max_decimals(2)
                    .suffix(" %"),
            )
            .changed()
        {
            action = Some(TransformAction::Width(w));
        }
        let mut h = m.height;
        ui.label("H");
        if ui
            .add(
                egui::DragValue::new(&mut h)
                    .range(-10_000.0..=10_000.0)
                    .speed(0.5)
                    .max_decimals(2)
                    .suffix(" %"),
            )
            .changed()
        {
            action = Some(TransformAction::Height(h));
        }
    });
    ui.horizontal(|ui| {
        let mut a = m.angle;
        ui.label("Angle");
        if ui
            .add(
                egui::DragValue::new(&mut a)
                    .range(-360.0..=360.0)
                    .speed(0.5)
                    .max_decimals(2)
                    .suffix("°"),
            )
            .changed()
        {
            action = Some(TransformAction::Angle(a));
        }
        let mut keep = m.keep_aspect;
        if ui
            .checkbox(&mut keep, "Keep aspect")
            .on_hover_text("Scale both sides together (Shift when off)")
            .changed()
        {
            action = Some(TransformAction::KeepAspect(keep));
        }
    });
    ui.horizontal(|ui| {
        if ui
            .selectable_label(!m.nearest, "Bicubic")
            .on_hover_text("Smooth, sharp edges (the reference's default)")
            .clicked()
        {
            action = Some(TransformAction::Nearest(false));
        }
        if ui
            .selectable_label(m.nearest, "Nearest")
            .on_hover_text("Hard pixels, for pixel art")
            .clicked()
        {
            action = Some(TransformAction::Nearest(true));
        }
        if ui.button("Reset").clicked() {
            action = Some(TransformAction::Reset);
        }
    });
    ui.horizontal(|ui| {
        if ui
            .button(egui::RichText::new("OK").strong())
            .on_hover_text("Apply the transform (Enter)")
            .clicked()
        {
            action = Some(TransformAction::Commit);
        }
        if ui
            .button("Cancel")
            .on_hover_text("Discard the transform (Escape)")
            .clicked()
        {
            action = Some(TransformAction::Cancel);
        }
    });
    action
}

/// The transform box on screen, in points.
#[derive(Clone, Copy, Debug)]
pub struct TransformBox {
    /// The box's corners, in order.
    pub corners: [Pos2; 4],
    /// The pivot (scaling and rotation keep it in place).
    pub pivot: Pos2,
    /// The rotation handle, beyond the top side.
    pub rotate: Pos2,
}

/// Half the side of a square handle, in points: constant on screen at any
/// zoom (reference T44).
pub const HANDLE: f32 = 4.0;

/// Paints the transform box under the panels: its outline, a square
/// handle on every corner and side, a round rotation handle joined to the
/// top side and the pivot's cross.
pub fn transform_box(ctx: &egui::Context, b: &TransformBox) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let blue = Color32::from_rgb(70, 160, 230);
    let line = Stroke::new(1.0, blue);
    let mut outline = b.corners.to_vec();
    outline.push(b.corners[0]);
    painter.add(Shape::line(outline, line));
    let top = b.corners[0].lerp(b.corners[1], 0.5);
    painter.line_segment([top, b.rotate], line);
    for k in 0..4 {
        let (p, q) = (b.corners[k], b.corners[(k + 1) % 4]);
        for at in [p, p.lerp(q, 0.5)] {
            let r = egui::Rect::from_center_size(at, Vec2::splat(2.0 * HANDLE));
            painter.rect_filled(r, 1.0, Color32::WHITE);
            painter.rect_stroke(r, 1.0, line, egui::StrokeKind::Middle);
        }
    }
    painter.circle(b.rotate, HANDLE, Color32::WHITE, line);
    let c = b.pivot;
    let dark = Stroke::new(1.0, Color32::BLACK);
    for (d, s) in [(1.0, dark), (0.0, Stroke::new(1.0, Color32::WHITE))] {
        painter.line_segment(
            [c - Vec2::new(6.0 + d, 0.0), c + Vec2::new(6.0 + d, 0.0)],
            s,
        );
        painter.line_segment(
            [c - Vec2::new(0.0, 6.0 + d), c + Vec2::new(0.0, 6.0 + d)],
            s,
        );
    }
}
