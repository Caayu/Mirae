//! Point-based curve editor.
//!
//! Drag a point to move it, double-click empty space to add one, right-click
//! a point to remove it. Endpoints keep x = 0 and x = 1. Every edit goes
//! through `Curve`'s sanitizing operations, so the curve can never hold NaN
//! or unordered points; with `monotone` the result is also non-decreasing.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2, pos2};
use tela_brush::dynamics::Curve;

/// Pick radius around a point, in points.
const PICK: f32 = 9.0;

/// Maps between curve space (`[0, 1]²`, y up) and the widget rectangle.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub rect: Rect,
}

impl Frame {
    pub fn to_screen(&self, x: f32, y: f32) -> Pos2 {
        pos2(
            self.rect.left() + x * self.rect.width(),
            self.rect.bottom() - y * self.rect.height(),
        )
    }

    pub fn to_curve(&self, p: Pos2) -> (f32, f32) {
        (
            ((p.x - self.rect.left()) / self.rect.width()).clamp(0.0, 1.0),
            ((self.rect.bottom() - p.y) / self.rect.height()).clamp(0.0, 1.0),
        )
    }

    /// Index of the control point within `PICK` of `p`, nearest first.
    pub fn pick(&self, curve: &Curve, p: Pos2) -> Option<usize> {
        curve
            .points()
            .enumerate()
            .map(|(i, (x, y))| (i, self.to_screen(x, y).distance(p)))
            .filter(|&(_, d)| d <= PICK)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}

/// Shows the editor; returns whether the curve changed.
pub fn curve_editor(
    ui: &mut egui::Ui,
    curve: &mut Curve,
    monotone: bool,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    let (response, painter) = ui.allocate_painter(
        Vec2::new(ui.available_width().min(220.0), 130.0),
        Sense::click_and_drag(),
    );
    let frame = Frame {
        rect: response.rect,
    };
    let drag_id = response.id.with(id_salt).with("drag");
    let mut changed = false;

    if response.drag_started()
        && let Some(p) = response.interact_pointer_pos()
    {
        let picked = frame.pick(curve, p);
        ui.data_mut(|d| d.insert_temp(drag_id, picked));
    }
    if response.dragged()
        && let (Some(Some(i)), Some(p)) = (
            ui.data(|d| d.get_temp::<Option<usize>>(drag_id)),
            response.interact_pointer_pos(),
        )
    {
        let (x, y) = frame.to_curve(p);
        curve.move_point(i, x, y);
        changed = true;
    }
    if response.drag_stopped() {
        ui.data_mut(|d| d.remove::<Option<usize>>(drag_id));
    }
    if response.double_clicked()
        && let Some(p) = response.interact_pointer_pos()
        && frame.pick(curve, p).is_none()
    {
        let (x, y) = frame.to_curve(p);
        let i = curve.insert_point(x);
        curve.move_point(i, x, y);
        changed = true;
    }
    if response.secondary_clicked()
        && let Some(p) = response.interact_pointer_pos()
        && let Some(i) = frame.pick(curve, p)
    {
        changed |= curve.remove_point(i);
    }
    if changed && monotone && !curve.is_monotone() {
        *curve = curve.monotone();
    }

    // Drawing.
    let visuals = ui.visuals();
    painter.rect_filled(frame.rect, 2.0, visuals.extreme_bg_color);
    let grid = Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color);
    for k in 1..4 {
        let t = k as f32 / 4.0;
        painter.line_segment([frame.to_screen(t, 0.0), frame.to_screen(t, 1.0)], grid);
        painter.line_segment([frame.to_screen(0.0, t), frame.to_screen(1.0, t)], grid);
    }
    let line: Vec<Pos2> = (0..=64)
        .map(|i| {
            let x = i as f32 / 64.0;
            frame.to_screen(x, curve.eval(x))
        })
        .collect();
    painter.add(egui::Shape::line(
        line,
        Stroke::new(2.0, visuals.selection.bg_fill),
    ));
    for (x, y) in curve.points() {
        painter.circle_filled(frame.to_screen(x, y), 4.0, Color32::WHITE);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_maps_both_ways() {
        let f = Frame {
            rect: Rect::from_min_size(pos2(10.0, 20.0), Vec2::new(200.0, 100.0)),
        };
        assert_eq!(f.to_screen(0.0, 0.0), pos2(10.0, 120.0));
        assert_eq!(f.to_screen(1.0, 1.0), pos2(210.0, 20.0));
        assert_eq!(f.to_curve(pos2(110.0, 70.0)), (0.5, 0.5));
        assert_eq!(f.to_curve(pos2(-100.0, 500.0)), (0.0, 0.0), "clamped");
    }

    #[test]
    fn picking_finds_the_nearest_point_within_reach() {
        let f = Frame {
            rect: Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(100.0, 100.0)),
        };
        let c = Curve::new(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]);
        assert_eq!(f.pick(&c, pos2(52.0, 51.0)), Some(1));
        assert_eq!(f.pick(&c, pos2(30.0, 30.0)), None);
        assert_eq!(f.pick(&c, pos2(1.0, 99.0)), Some(0));
    }
}
