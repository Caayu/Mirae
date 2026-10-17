//! The selection tool: its settings, and a drag turned into a document
//! selection shape. Points are kept in screen pixels while dragging and
//! mapped to the document once, through the view, when the drag ends, so
//! rectangles and ellipses follow the screen axes in a rotated view, as in
//! the reference (parity record 5C, S9).

use glam::Vec2;
use tela_canvas::ViewTransform;
use tela_document::{SelectionOp, SelectionShape};
use tela_ui::{SelectionKind, SelectionMode};

/// Settings of the selection tool (session tool state, like the brush
/// settings; not saved).
#[derive(Clone, Debug)]
pub struct SelectionTool {
    pub kind: SelectionKind,
    pub mode: SelectionMode,
    /// Per kind: the reference's defaults are rectangle off, ellipse and
    /// lasso on (S3).
    anti_alias: [bool; 3],
    /// Feather strength in pixels for the next feather command.
    pub feather: f32,
}

impl Default for SelectionTool {
    fn default() -> Self {
        Self {
            kind: SelectionKind::Rectangle,
            mode: SelectionMode::New,
            anti_alias: [false, true, true],
            feather: 10.0,
        }
    }
}

fn kind_index(kind: SelectionKind) -> usize {
    match kind {
        SelectionKind::Rectangle => 0,
        SelectionKind::Ellipse => 1,
        SelectionKind::Lasso => 2,
    }
}

impl SelectionTool {
    pub fn anti_alias(&self) -> bool {
        self.anti_alias[kind_index(self.kind)]
    }

    pub fn set_anti_alias(&mut self, on: bool) {
        self.anti_alias[kind_index(self.kind)] = on;
    }

    /// The operator a drag starting now uses: Shift+Alt intersects, Shift
    /// adds, Alt subtracts (held at the press, S4); otherwise the mode.
    pub fn op(&self, shift: bool, alt: bool) -> SelectionOp {
        match (shift, alt, self.mode) {
            (true, true, _) => SelectionOp::Intersect,
            (true, false, _) => SelectionOp::Add,
            (false, true, _) => SelectionOp::Subtract,
            (false, false, SelectionMode::New) => SelectionOp::Replace,
            (false, false, SelectionMode::Add) => SelectionOp::Add,
            (false, false, SelectionMode::Subtract) => SelectionOp::Subtract,
            (false, false, SelectionMode::Intersect) => SelectionOp::Intersect,
        }
    }
}

/// A selection drag in progress, in screen pixels.
#[derive(Clone, Debug)]
pub struct SelectionDrag {
    pub kind: SelectionKind,
    pub op: SelectionOp,
    pub anti_alias: bool,
    /// Rectangle and ellipse: the press and the (constrained) current
    /// point. Lasso: every point of the path.
    points: Vec<Vec2>,
}

/// Lasso points closer than this (screen pixels) to the previous one are
/// not kept; the path stays as precise as the pointer.
const LASSO_STEP: f32 = 0.5;

/// Vertices of an ellipse preview on screen.
const PREVIEW_SEGMENTS: usize = 96;

impl SelectionDrag {
    pub fn begin(tool: &SelectionTool, op: SelectionOp, at: Vec2) -> Self {
        Self {
            kind: tool.kind,
            op,
            anti_alias: tool.anti_alias(),
            points: vec![at, at],
        }
    }

    /// The pointer moved to `at`; `square` (Shift during the drag)
    /// constrains rectangles and ellipses to their smaller side (S4).
    pub fn move_to(&mut self, at: Vec2, square: bool) {
        match self.kind {
            SelectionKind::Lasso => {
                if at.distance(*self.points.last().expect("begun")) >= LASSO_STEP {
                    self.points.push(at);
                }
            }
            SelectionKind::Rectangle | SelectionKind::Ellipse => {
                let start = self.points[0];
                let d = at - start;
                let end = if square {
                    let side = d.x.abs().min(d.y.abs());
                    start + Vec2::new(side.copysign(d.x), side.copysign(d.y))
                } else {
                    at
                };
                self.points[1] = end;
            }
        }
    }

    /// The outline to show while dragging, on screen (closed implicitly).
    pub fn preview(&self) -> Vec<Vec2> {
        match self.kind {
            SelectionKind::Lasso => self.points.clone(),
            SelectionKind::Rectangle => {
                let (a, b) = (self.points[0], self.points[1]);
                vec![a, Vec2::new(b.x, a.y), b, Vec2::new(a.x, b.y)]
            }
            SelectionKind::Ellipse => {
                let (a, b) = (self.points[0], self.points[1]);
                let (c, r) = ((a + b) * 0.5, (b - a).abs() * 0.5);
                (0..PREVIEW_SEGMENTS)
                    .map(|k| {
                        let t = std::f32::consts::TAU * k as f32 / PREVIEW_SEGMENTS as f32;
                        c + Vec2::new(r.x * t.cos(), r.y * t.sin())
                    })
                    .collect()
            }
        }
    }

    /// The document shape: the screen points mapped through `view` once.
    pub fn shape(&self, view: &ViewTransform) -> SelectionShape {
        let doc = |p: Vec2| {
            let c = view.to_canvas(p);
            [c.x, c.y]
        };
        match self.kind {
            SelectionKind::Lasso => {
                SelectionShape::Polygon(self.points.iter().map(|&p| doc(p)).collect())
            }
            SelectionKind::Rectangle => {
                // Screen axes map to document axes only when the view is
                // turned by a multiple of 90°; otherwise the rectangle is
                // a rotated polygon in the document.
                let quarter = (view.rotation / std::f32::consts::FRAC_PI_2).round();
                let aligned = (view.rotation - quarter * std::f32::consts::FRAC_PI_2).abs() < 1e-6;
                let (a, b) = (self.points[0], self.points[1]);
                if aligned {
                    SelectionShape::Rect(doc(a), doc(b))
                } else {
                    SelectionShape::Polygon(
                        [a, Vec2::new(b.x, a.y), b, Vec2::new(a.x, b.y)]
                            .into_iter()
                            .map(doc)
                            .collect(),
                    )
                }
            }
            SelectionKind::Ellipse => {
                let (a, b) = (self.points[0], self.points[1]);
                let r = (b - a).abs() * (0.5 / view.zoom);
                SelectionShape::Ellipse {
                    center: doc((a + b) * 0.5),
                    radii: [r.x, r.y],
                    // Screen axes in document space.
                    rotation: -view.rotation,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_at_the_press_override_the_mode() {
        let mut tool = SelectionTool::default();
        assert_eq!(tool.op(false, false), SelectionOp::Replace);
        assert_eq!(tool.op(true, false), SelectionOp::Add);
        assert_eq!(tool.op(false, true), SelectionOp::Subtract);
        assert_eq!(tool.op(true, true), SelectionOp::Intersect);
        tool.mode = SelectionMode::Subtract;
        assert_eq!(tool.op(false, false), SelectionOp::Subtract);
        assert_eq!(tool.op(true, false), SelectionOp::Add);
        tool.mode = SelectionMode::Intersect;
        assert_eq!(tool.op(false, false), SelectionOp::Intersect);
        assert_eq!(tool.op(false, true), SelectionOp::Subtract);
    }

    #[test]
    fn anti_aliasing_defaults_follow_the_reference_per_shape() {
        let mut tool = SelectionTool::default();
        assert!(!tool.anti_alias());
        tool.kind = SelectionKind::Ellipse;
        assert!(tool.anti_alias());
        tool.set_anti_alias(false);
        tool.kind = SelectionKind::Lasso;
        assert!(tool.anti_alias());
    }

    #[test]
    fn shapes_map_through_the_view_once() {
        let tool = SelectionTool::default();
        let view = ViewTransform {
            pan: Vec2::new(100.0, 50.0),
            zoom: 2.0,
            rotation: 0.0,
        };
        let mut drag = SelectionDrag::begin(&tool, SelectionOp::Replace, Vec2::new(120.0, 70.0));
        drag.move_to(Vec2::new(320.0, 170.0), false);
        assert_eq!(
            drag.shape(&view),
            SelectionShape::Rect([10.0, 10.0], [110.0, 60.0])
        );
        // Shift during the drag: a square of the smaller side.
        drag.move_to(Vec2::new(320.0, 170.0), true);
        assert_eq!(
            drag.shape(&view),
            SelectionShape::Rect([10.0, 10.0], [60.0, 60.0])
        );
        // A rotated view gives a polygon whose corners are the screen
        // corners mapped back.
        let turned = ViewTransform {
            rotation: 0.5,
            ..view
        };
        let SelectionShape::Polygon(p) = drag.shape(&turned) else {
            panic!("a polygon");
        };
        let back = turned.to_screen(Vec2::new(p[2][0], p[2][1]));
        assert!(back.distance(Vec2::new(220.0, 170.0)) < 1e-3);
        // Ellipses: screen axes, radii in document pixels.
        let mut e = SelectionDrag::begin(
            &SelectionTool {
                kind: SelectionKind::Ellipse,
                ..SelectionTool::default()
            },
            SelectionOp::Add,
            Vec2::new(100.0, 50.0),
        );
        e.move_to(Vec2::new(300.0, 150.0), false);
        assert_eq!(
            e.shape(&turned),
            SelectionShape::Ellipse {
                center: {
                    let c = turned.to_canvas(Vec2::new(200.0, 100.0));
                    [c.x, c.y]
                },
                radii: [50.0, 25.0],
                rotation: -0.5,
            }
        );
    }

    #[test]
    fn the_lasso_keeps_every_distinct_point() {
        let tool = SelectionTool {
            kind: SelectionKind::Lasso,
            ..SelectionTool::default()
        };
        let mut drag = SelectionDrag::begin(&tool, SelectionOp::Replace, Vec2::ZERO);
        for i in 0..100 {
            drag.move_to(Vec2::new(i as f32, (i * i) as f32 * 0.01), false);
        }
        drag.move_to(Vec2::new(99.1, 98.0), false);
        assert_eq!(drag.preview().len(), 2 + 99);
    }
}
