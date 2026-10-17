//! The transform tool (phase 5D): a session that edits one
//! [`TransformParams`] over a [`TransformSource`] until it is committed
//! (one history step) or cancelled (nothing), as the reference does
//! (docs/parity/5D-transform.md). The session lives in the app, never in
//! the document or its file; the document does not change until the
//! commit (invariant TR1).
//!
//! Handles are hit and drawn in screen space, so they keep their size at
//! any zoom (T44); the box itself is mapped from document pixels through
//! the one view transform, so it turns with a rotated view (T45).

use glam::Vec2;
use tela_canvas::ViewTransform;
use tela_document::{Interpolation, MIN_SCALE, Transform, TransformParams, TransformSource};

/// Settings of the transform tool (session tool state, like the brush's;
/// not saved). The reference's defaults: keep aspect on, bicubic (T2,
/// T16).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformTool {
    pub keep_aspect: bool,
    pub interpolation: Interpolation,
}

impl Default for TransformTool {
    fn default() -> Self {
        Self {
            keep_aspect: true,
            interpolation: Interpolation::Bicubic,
        }
    }
}

/// What a press on the canvas grabs during a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// Corner `k` (0 top-left, then clockwise in the source box).
    Corner(usize),
    /// Side `k` (0 top, 1 right, 2 bottom, 3 left).
    Side(usize),
    /// The round handle beyond the top side.
    Rotate,
    /// The pivot's cross.
    Pivot,
    /// Inside the box: moves (T8).
    Inside,
    /// Outside the box: rotates (T7).
    Outside,
}

/// Grab distance of a handle, in screen pixels (a little more than the
/// drawn handle, so it is easy to hit at any zoom).
const GRAB: f32 = 8.0;
/// Distance of the rotation handle beyond the top side, screen pixels.
const ROTATE_OFFSET: f32 = 24.0;

/// The box as drawn, in screen pixels.
#[derive(Clone, Copy, Debug)]
pub struct ScreenBox {
    pub corners: [Vec2; 4],
    pub pivot: Vec2,
    pub rotate: Vec2,
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    handle: Handle,
    /// Where the press was, in document pixels.
    start: [f64; 2],
    params: TransformParams,
}

/// An open transform.
#[derive(Debug)]
pub struct TransformSession {
    pub source: TransformSource,
    /// Every edit of the session, oldest first; `at` is the current one.
    /// Undo and redo inside a session walk them (reference T13).
    steps: Vec<TransformParams>,
    at: usize,
    /// The tool settings when the session began: a cancel restores them
    /// (reference T5).
    pub settings: TransformTool,
    drag: Option<Drag>,
    /// A Move-tool drag: committed when the pointer is released.
    pub ephemeral: bool,
}

impl TransformSession {
    pub fn new(source: TransformSource, settings: TransformTool) -> Self {
        let identity = TransformParams::identity(source.centre());
        Self {
            source,
            steps: vec![identity],
            at: 0,
            settings,
            drag: None,
            ephemeral: false,
        }
    }

    pub fn params(&self) -> TransformParams {
        self.steps[self.at]
    }

    pub fn transform(&self, tool: &TransformTool) -> Transform {
        Transform {
            params: self.params(),
            interpolation: tool.interpolation,
        }
    }

    /// A new edit: a step of the session (drops the redoable ones).
    /// Returns whether anything changed.
    pub fn edit(&mut self, p: TransformParams) -> bool {
        if p == self.params() || !p.is_valid() {
            return false;
        }
        self.steps.truncate(self.at + 1);
        self.steps.push(p);
        self.at += 1;
        true
    }

    /// Changes the current edit in place (a drag in progress).
    fn update(&mut self, p: TransformParams) -> bool {
        if p == self.params() || !p.is_valid() {
            return false;
        }
        self.steps[self.at] = p;
        true
    }

    pub fn can_undo(&self) -> bool {
        self.at > 0
    }

    pub fn can_redo(&self) -> bool {
        self.at + 1 < self.steps.len()
    }

    pub fn undo(&mut self) -> bool {
        if self.drag.is_some() || !self.can_undo() {
            return false;
        }
        self.at -= 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.drag.is_some() || !self.can_redo() {
            return false;
        }
        self.at += 1;
        true
    }

    /// The box on screen; `None` for parameters the document refuses.
    pub fn screen_box(&self, view: &ViewTransform) -> Option<ScreenBox> {
        let p = self.params();
        let corners = self
            .source
            .corners(&p)?
            .map(|c| view.to_screen(Vec2::new(c[0] as f32, c[1] as f32)));
        let pivot = view.to_screen(Vec2::new(
            (p.pivot[0] + p.translate[0]) as f32,
            (p.pivot[1] + p.translate[1]) as f32,
        ));
        let centre = (corners[0] + corners[1] + corners[2] + corners[3]) * 0.25;
        let top = (corners[0] + corners[1]) * 0.5;
        // Beyond whichever side is the source's top, away from the box.
        let out = (top - centre).try_normalize().unwrap_or(Vec2::NEG_Y);
        Some(ScreenBox {
            corners,
            pivot,
            rotate: top + out * ROTATE_OFFSET,
        })
    }

    /// What a press at screen position `at` grabs.
    pub fn hit(&self, view: &ViewTransform, at: Vec2) -> Handle {
        let Some(b) = self.screen_box(view) else {
            return Handle::Outside;
        };
        // The nearest handle within reach (they crowd on a small box).
        let handles = [(Handle::Rotate, b.rotate), (Handle::Pivot, b.pivot)]
            .into_iter()
            .chain((0..4).map(|k| (Handle::Corner(k), b.corners[k])))
            .chain((0..4).map(|k| {
                let side = (b.corners[k] + b.corners[(k + 1) % 4]) * 0.5;
                (Handle::Side(k), side)
            }));
        let nearest = handles
            .map(|(h, p)| (h, at.distance(p)))
            .filter(|&(_, d)| d <= GRAB)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((h, _)) = nearest {
            return h;
        }
        if inside(&b.corners, at) {
            Handle::Inside
        } else {
            Handle::Outside
        }
    }

    /// Starts dragging `handle` from screen position `at`.
    pub fn begin_drag(&mut self, view: &ViewTransform, handle: Handle, at: Vec2) {
        // A drag is one new step of the session.
        let p = self.params();
        self.steps.truncate(self.at + 1);
        self.steps.push(p);
        self.at += 1;
        let q = view.to_canvas(at);
        self.drag = Some(Drag {
            handle,
            start: [f64::from(q.x), f64::from(q.y)],
            params: p,
        });
    }

    /// The pointer moved to screen position `at`; `shift` is held.
    /// Returns whether the parameters changed.
    pub fn drag_to(
        &mut self,
        view: &ViewTransform,
        at: Vec2,
        shift: bool,
        keep_aspect: bool,
    ) -> bool {
        let Some(d) = self.drag else {
            return false;
        };
        let q = view.to_canvas(at);
        let q = [f64::from(q.x), f64::from(q.y)];
        let p0 = d.params;
        let b = self.source.bounds();
        let src = |k: usize| {
            let (x0, y0, x1, y1) = (
                f64::from(b.x0),
                f64::from(b.y0),
                f64::from(b.x1),
                f64::from(b.y1),
            );
            [[x0, y0], [x1, y0], [x1, y1], [x0, y1]][k % 4]
        };
        let mid = |a: [f64; 2], c: [f64; 2]| [(a[0] + c[0]) / 2.0, (a[1] + c[1]) / 2.0];
        let p = match d.handle {
            Handle::Inside => TransformParams {
                translate: [
                    p0.translate[0] + q[0] - d.start[0],
                    p0.translate[1] + q[1] - d.start[1],
                ],
                ..p0
            },
            Handle::Rotate | Handle::Outside => {
                let c = [p0.pivot[0] + p0.translate[0], p0.pivot[1] + p0.translate[1]];
                let angle = |p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
                TransformParams {
                    rotation: p0.rotation + angle(q) - angle(d.start),
                    ..p0
                }
            }
            Handle::Pivot => move_pivot(&p0, q),
            Handle::Corner(k) => scale_about(
                &p0,
                src(k + 2),
                src(k),
                q,
                [true, true],
                keep_aspect || shift,
            ),
            Handle::Side(k) => {
                let (from, to) = (mid(src(k + 2), src(k + 3)), mid(src(k), src(k + 1)));
                let axes = if k % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                };
                scale_about(&p0, from, to, q, axes, keep_aspect || shift)
            }
        };
        self.update(p)
    }

    /// Ends the drag; a drag that changed nothing is not a step.
    pub fn end_drag(&mut self) {
        if self.drag.take().is_some()
            && self.at > 0
            && self.steps[self.at] == self.steps[self.at - 1]
        {
            self.steps.truncate(self.at);
            self.at -= 1;
        }
    }

    /// The width field: `percent` of the source's width (negative flips),
    /// the height following when the aspect is kept (T3).
    pub fn set_width(&mut self, percent: f64, keep_aspect: bool) -> bool {
        let mut p = self.params();
        let s = clamp_scale(percent / 100.0);
        if keep_aspect && p.scale[0] != 0.0 {
            p.scale[1] = clamp_scale(p.scale[1] * s / p.scale[0]);
        }
        p.scale[0] = s;
        self.edit(p)
    }

    pub fn set_height(&mut self, percent: f64, keep_aspect: bool) -> bool {
        let mut p = self.params();
        let s = clamp_scale(percent / 100.0);
        if keep_aspect && p.scale[1] != 0.0 {
            p.scale[0] = clamp_scale(p.scale[0] * s / p.scale[1]);
        }
        p.scale[1] = s;
        self.edit(p)
    }

    /// The angle field, degrees clockwise.
    pub fn set_angle(&mut self, degrees: f64) -> bool {
        let p = TransformParams {
            rotation: degrees.to_radians(),
            ..self.params()
        };
        self.edit(p)
    }

    /// Flips about the pivot: a width (or height) of minus itself (T15).
    pub fn flip(&mut self, horizontal: bool) -> bool {
        let mut p = self.params();
        let axis = usize::from(!horizontal);
        p.scale[axis] = -p.scale[axis];
        self.edit(p)
    }

    /// Back to no change about the box's centre.
    pub fn reset(&mut self) -> bool {
        self.edit(TransformParams::identity(self.source.centre()))
    }
}

/// `v` clamped to the valid scale range, keeping its sign.
fn clamp_scale(v: f64) -> f64 {
    let m = v.abs().clamp(MIN_SCALE, tela_document::MAX_SCALE);
    if v < 0.0 { -m } else { m }
}

/// Scales so that source point `moved` follows the pointer `q` while
/// source point `anchor` stays where it is (a corner about the opposite
/// corner, a side about the opposite side, T10–T12), on the `axes` that
/// move. `proportional` scales both axes by the smaller ratio.
fn scale_about(
    p0: &TransformParams,
    anchor: [f64; 2],
    moved: [f64; 2],
    q: [f64; 2],
    axes: [bool; 2],
    proportional: bool,
) -> TransformParams {
    let m = p0.matrix();
    let fixed = m.apply(anchor);
    // The pointer relative to the anchor, in the box's (rotated) axes.
    let (s, c) = p0.rotation.sin_cos();
    let (dx, dy) = (q[0] - fixed[0], q[1] - fixed[1]);
    let local = [c * dx + s * dy, -s * dx + c * dy];
    let span = [moved[0] - anchor[0], moved[1] - anchor[1]];
    let mut ratio = [1.0f64; 2];
    for i in 0..2 {
        if axes[i] && span[i] != 0.0 && p0.scale[i] != 0.0 {
            ratio[i] = local[i] / span[i] / p0.scale[i];
        }
    }
    if proportional {
        let r = match axes {
            [true, true] => {
                if ratio[0].abs() <= ratio[1].abs() {
                    ratio[0]
                } else {
                    ratio[1]
                }
            }
            [true, false] => ratio[0],
            _ => ratio[1],
        };
        ratio = [r, r];
    }
    let scale = [
        clamp_scale(p0.scale[0] * ratio[0]),
        clamp_scale(p0.scale[1] * ratio[1]),
    ];
    let mut p = TransformParams { scale, ..*p0 };
    // Keep the anchor where it was.
    let now = p.matrix().apply(anchor);
    p.translate = [
        p.translate[0] + fixed[0] - now[0],
        p.translate[1] + fixed[1] - now[1],
    ];
    p
}

/// Moves the pivot to document point `q` without moving the content: the
/// same map about another point (T9).
fn move_pivot(p0: &TransformParams, q: [f64; 2]) -> TransformParams {
    let m = p0.matrix();
    let linear = tela_document::Affine {
        e: 0.0,
        f: 0.0,
        ..m
    };
    let Some(inv) = linear.inverse() else {
        return *p0;
    };
    // M(x) = A·x + c; the new pivot P' with P' + t' = q keeps M when
    // A·P' = q − c.
    let pivot = inv.apply([q[0] - m.e, q[1] - m.f]);
    TransformParams {
        pivot,
        translate: [q[0] - pivot[0], q[1] - pivot[1]],
        ..*p0
    }
}

/// Whether `p` lies inside the convex quadrilateral `c`.
fn inside(c: &[Vec2; 4], p: Vec2) -> bool {
    let mut sign = 0.0f32;
    for k in 0..4 {
        let (a, b) = (c[k], c[(k + 1) % 4]);
        let cross = (b - a).perp_dot(p - a);
        if cross != 0.0 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use tela_canvas::{DirtyTiles, Tile, TileCoord};
    use tela_document::Document;

    use super::*;

    fn session() -> TransformSession {
        let mut d = Document::new(400, 300);
        let id = d.active_layer();
        let mut tiles = DirtyTiles::new();
        tiles.mark(TileCoord { x: 0, y: 0 });
        d.modify_tiles(id, &tiles, |_, t: &mut Tile<[u8; 4]>| {
            for y in 100..140 {
                for x in 100..180 {
                    *t.get_mut(x, y) = [200, 0, 0, 255];
                }
            }
        })
        .unwrap();
        TransformSession::new(d.transform_source(id).unwrap(), TransformTool::default())
    }

    fn view() -> ViewTransform {
        ViewTransform {
            pan: Vec2::new(50.0, 20.0),
            zoom: 2.0,
            rotation: 0.3,
        }
    }

    fn drag(
        s: &mut TransformSession,
        v: &ViewTransform,
        from: Vec2,
        to: Vec2,
        shift: bool,
        keep: bool,
    ) {
        let h = s.hit(v, from);
        s.begin_drag(v, h, from);
        s.drag_to(v, to, shift, keep);
        s.end_drag();
    }

    fn doc(v: &ViewTransform, x: f32, y: f32) -> Vec2 {
        v.to_screen(Vec2::new(x, y))
    }

    #[test]
    fn handles_are_hit_in_screen_space_at_any_zoom() {
        let mut s = session();
        for zoom in [0.25, 1.0, 8.0] {
            let v = ViewTransform { zoom, ..view() };
            let b = s.screen_box(&v).unwrap();
            // A few pixels off, outward: still the handle, whatever the
            // zoom (handles keep their screen size, T44).
            let centre = (b.corners[0] + b.corners[2]) * 0.5;
            let corner = b.corners[2] + (b.corners[2] - centre).normalize() * 5.0;
            assert_eq!(s.hit(&v, corner), Handle::Corner(2));
            let side = (b.corners[1] + b.corners[2]) * 0.5;
            let side = side + (side - centre).normalize() * 5.0;
            assert_eq!(s.hit(&v, side), Handle::Side(1));
            assert_eq!(s.hit(&v, b.rotate), Handle::Rotate);
            assert_eq!(s.hit(&v, b.pivot), Handle::Pivot);
            if zoom >= 1.0 {
                assert_eq!(s.hit(&v, doc(&v, 110.0, 105.0)), Handle::Inside);
            }
            assert_eq!(s.hit(&v, doc(&v, 300.0, 250.0)), Handle::Outside);
        }
        s.edit(TransformParams {
            rotation: 0.7,
            ..s.params()
        });
        let v = view();
        let b = s.screen_box(&v).unwrap();
        assert_eq!(s.hit(&v, b.corners[0]), Handle::Corner(0));
    }

    #[test]
    fn dragging_inside_moves_and_outside_rotates_about_the_pivot() {
        let mut s = session();
        let v = view();
        drag(
            &mut s,
            &v,
            doc(&v, 120.0, 110.0),
            doc(&v, 150.0, 90.0),
            false,
            true,
        );
        let p = s.params();
        assert!((p.translate[0] - 30.0).abs() < 1e-3 && (p.translate[1] + 20.0).abs() < 1e-3);
        let c = [p.pivot[0] + p.translate[0], p.pivot[1] + p.translate[1]];
        // A quarter turn around the moved pivot, dragged from outside.
        let from = doc(&v, c[0] as f32 + 150.0, c[1] as f32);
        let to = doc(&v, c[0] as f32, c[1] as f32 + 150.0);
        drag(&mut s, &v, from, to, false, true);
        let r = s.params().rotation;
        assert!((r - std::f64::consts::FRAC_PI_2).abs() < 1e-3, "{r}");
        assert_eq!(s.params().pivot, p.pivot, "rotation keeps the pivot");
    }

    #[test]
    fn corners_scale_about_the_opposite_corner() {
        let mut s = session();
        let v = view();
        let anchor = s.params().matrix().apply([180.0, 140.0]);
        // T10: keep aspect, the smaller ratio wins.
        drag(
            &mut s,
            &v,
            doc(&v, 100.0, 100.0),
            doc(&v, 72.0, 90.0),
            false,
            true,
        );
        let p = s.params();
        assert!((p.scale[0] - p.scale[1]).abs() < 1e-9);
        assert!((p.scale[0] - 1.25).abs() < 1e-3, "{:?}", p.scale);
        let still = p.matrix().apply([180.0, 140.0]);
        assert!((still[0] - anchor[0]).abs() < 1e-6 && (still[1] - anchor[1]).abs() < 1e-6);
        // T12: free without keep aspect; Shift makes it proportional.
        s.undo();
        drag(
            &mut s,
            &v,
            doc(&v, 100.0, 100.0),
            doc(&v, 72.0, 90.0),
            false,
            false,
        );
        let p = s.params();
        assert!((p.scale[0] - 1.35).abs() < 1e-3 && (p.scale[1] - 1.25).abs() < 1e-3);
        s.undo();
        drag(
            &mut s,
            &v,
            doc(&v, 100.0, 100.0),
            doc(&v, 72.0, 90.0),
            true,
            false,
        );
        assert!((s.params().scale[0] - 1.25).abs() < 1e-3);
    }

    #[test]
    fn sides_scale_one_axis_or_both_with_keep_aspect() {
        let mut s = session();
        let v = view();
        // T11: the right side, keep aspect: both axes follow.
        drag(
            &mut s,
            &v,
            doc(&v, 180.0, 120.0),
            doc(&v, 220.0, 120.0),
            false,
            true,
        );
        let p = s.params();
        assert!((p.scale[0] - 1.5).abs() < 1e-3 && (p.scale[1] - 1.5).abs() < 1e-3);
        s.undo();
        drag(
            &mut s,
            &v,
            doc(&v, 180.0, 120.0),
            doc(&v, 220.0, 120.0),
            false,
            false,
        );
        let p = s.params();
        assert!((p.scale[0] - 1.5).abs() < 1e-3 && (p.scale[1] - 1.0).abs() < 1e-9);
        // Dragging past the opposite side flips.
        s.undo();
        drag(
            &mut s,
            &v,
            doc(&v, 180.0, 120.0),
            doc(&v, 60.0, 120.0),
            false,
            false,
        );
        assert!(s.params().scale[0] < 0.0);
    }

    #[test]
    fn moving_the_pivot_does_not_move_the_content() {
        let mut s = session();
        s.set_angle(30.0);
        s.set_width(150.0, true);
        let v = view();
        let before = s.params().matrix();
        let b = s.screen_box(&v).unwrap();
        drag(
            &mut s,
            &v,
            b.pivot,
            b.pivot + Vec2::new(40.0, 30.0),
            false,
            true,
        );
        let after = s.params().matrix();
        for q in [[100.0, 100.0], [180.0, 140.0], [0.0, 0.0]] {
            let (a, b) = (before.apply(q), after.apply(q));
            assert!((a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6);
        }
        assert_ne!(s.params().pivot, s.source.centre());
    }

    #[test]
    fn fields_flip_and_the_session_history() {
        let mut s = session();
        assert!(!s.can_undo());
        assert!(s.set_width(200.0, true));
        assert_eq!(s.params().scale, [2.0, 2.0], "T3: height follows");
        assert!(s.set_angle(10.0));
        assert!(s.set_height(50.0, false));
        assert_eq!(s.params().scale, [2.0, 0.5]);
        // T13: undo inside the session walks the edits back, redo forward.
        assert!(s.undo());
        assert_eq!(s.params().scale, [2.0, 2.0]);
        assert!(s.redo());
        assert!(!s.redo());
        assert!(s.flip(true));
        assert_eq!(s.params().scale[0], -2.0);
        // A new edit after an undo drops the redoable ones.
        s.undo();
        s.set_angle(20.0);
        assert!(!s.can_redo());
        assert!(s.reset());
        assert_eq!(s.params(), TransformParams::identity(s.source.centre()));
        // Invalid values are not edits.
        let n = s.steps.len();
        assert!(!s.set_angle(f64::NAN));
        assert_eq!(s.steps.len(), n);
        // A drag that changes nothing is not a step.
        let v = view();
        let at = doc(&v, 120.0, 110.0);
        s.begin_drag(&v, Handle::Inside, at);
        s.drag_to(&v, at, false, true);
        s.end_drag();
        assert_eq!(s.steps.len(), n);
    }
}
