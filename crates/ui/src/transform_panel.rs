//! The transform panel (start, flip, the numeric fields, interpolation,
//! keep aspect, OK / Cancel) and the transform box drawn over the canvas.
//! Like the other panels it edits nothing: it shows what the app lends it
//! ([`TransformModel`]) and returns a [`TransformAction`]. The box is
//! presentation only: the app passes points already mapped to the screen.

use egui::{Color32, Pos2, Shape, Stroke, Ui, Vec2};

use crate::style;

/// What the panel shows this frame.
#[derive(Clone, Copy, Debug)]
pub struct TransformModel {
    /// A transform session is open.
    pub active: bool,
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

/// Draws the transform options (for the Move tool and an open transform).
pub fn transform_panel(ui: &mut Ui, m: &TransformModel) -> Option<TransformAction> {
    let mut action = None;
    style::row(ui, "Flip", |ui| {
        if ui
            .button("Horizontal")
            .on_hover_text("Flip horizontally (opens a transform)")
            .clicked()
        {
            action = Some(TransformAction::FlipHorizontal);
        }
        if ui
            .button("Vertical")
            .on_hover_text("Flip vertically (opens a transform)")
            .clicked()
        {
            action = Some(TransformAction::FlipVertical);
        }
    });
    if !m.active {
        style::row(ui, "", |ui| {
            if ui
                .button("Start transform")
                .on_hover_text("Scale, rotate and move the layer or the selection (Ctrl+T)")
                .clicked()
            {
                action = Some(TransformAction::Start);
            }
        });
        return action;
    }
    let field = |ui: &mut Ui, value: f64, range: std::ops::RangeInclusive<f64>, suffix: &str| {
        let mut v = value;
        let changed = ui
            .add(
                egui::DragValue::new(&mut v)
                    .range(range)
                    .speed(0.5)
                    .max_decimals(2)
                    .suffix(suffix),
            )
            .changed();
        changed.then_some(v)
    };
    style::row(ui, "Size", |ui| {
        ui.label("W");
        if let Some(w) = field(ui, m.width, -10_000.0..=10_000.0, " %") {
            action = Some(TransformAction::Width(w));
        }
        ui.label("H");
        if let Some(h) = field(ui, m.height, -10_000.0..=10_000.0, " %") {
            action = Some(TransformAction::Height(h));
        }
    });
    style::row(ui, "Angle", |ui| {
        if let Some(a) = field(ui, m.angle, -360.0..=360.0, "°") {
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
    style::row(ui, "Resample", |ui| {
        let options = [
            (
                false,
                "Bicubic",
                "Smooth, sharp edges (the reference's default)",
            ),
            (true, "Nearest", "Hard pixels, for pixel art"),
        ];
        if let Some(nearest) = style::segmented(ui, m.nearest, &options) {
            action = Some(TransformAction::Nearest(nearest));
        }
    });
    style::row(ui, "", |ui| {
        if ui
            .button(egui::RichText::new("Apply").strong())
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
        if ui
            .button("Reset")
            .on_hover_text("Back to no change")
            .clicked()
        {
            action = Some(TransformAction::Reset);
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
