use glam::{Affine2, Vec2};

use crate::IRect;

/// Maps canvas pixels to window pixels: `screen = pan + R(rotation) * zoom * canvas`.
///
/// Storing the three parameters (rather than an accumulated matrix) keeps the
/// transform free of numerical drift no matter how many interactive edits are
/// applied, and keeps it a similarity transform (no shear) by construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransform {
    /// Window position of canvas pixel (0, 0).
    pub pan: Vec2,
    /// Screen pixels per canvas pixel.
    pub zoom: f32,
    /// Counter-clockwise rotation in radians (y-down screen space, so it
    /// appears clockwise on screen).
    pub rotation: f32,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
            rotation: 0.0,
        }
    }
}

impl ViewTransform {
    pub const MIN_ZOOM: f32 = 1.0 / 64.0;
    pub const MAX_ZOOM: f32 = 256.0;

    /// Unrotated view that fits `canvas` inside `viewport` with a margin and
    /// centers it.
    pub fn fit(canvas: Vec2, viewport: Vec2) -> Self {
        let margin = 0.9;
        let zoom = (viewport.x / canvas.x)
            .min(viewport.y / canvas.y)
            .clamp(Self::MIN_ZOOM, Self::MAX_ZOOM)
            * margin;
        Self {
            pan: (viewport - canvas * zoom) * 0.5,
            zoom,
            rotation: 0.0,
        }
    }

    pub fn canvas_to_screen(&self) -> Affine2 {
        Affine2::from_scale_angle_translation(Vec2::splat(self.zoom), self.rotation, self.pan)
    }

    pub fn screen_to_canvas(&self) -> Affine2 {
        self.canvas_to_screen().inverse()
    }

    pub fn to_screen(&self, p: Vec2) -> Vec2 {
        self.canvas_to_screen().transform_point2(p)
    }

    pub fn to_canvas(&self, p: Vec2) -> Vec2 {
        self.screen_to_canvas().transform_point2(p)
    }

    pub fn pan_by(&mut self, screen_delta: Vec2) {
        self.pan += screen_delta;
    }

    /// Multiplies the zoom while keeping the canvas point under `anchor`
    /// (a screen position) fixed, which is what makes wheel zoom feel natural.
    pub fn zoom_at(&mut self, anchor: Vec2, factor: f32) {
        let c = self.to_canvas(anchor);
        self.zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.pin(c, anchor);
    }

    /// Rotates by `delta` radians around the screen point `anchor`.
    pub fn rotate_at(&mut self, anchor: Vec2, delta: f32) {
        let c = self.to_canvas(anchor);
        self.rotation = (self.rotation + delta).rem_euclid(std::f32::consts::TAU);
        self.pin(c, anchor);
    }

    /// Solves `pan` so canvas point `c` lands on screen point `s`.
    fn pin(&mut self, c: Vec2, s: Vec2) {
        let linear = Affine2::from_scale_angle_translation(
            Vec2::splat(self.zoom),
            self.rotation,
            Vec2::ZERO,
        );
        self.pan = s - linear.transform_point2(c);
    }

    /// Canvas-space bounding box of what a `viewport`-sized window shows,
    /// used to cull tiles that cannot be visible.
    pub fn visible_canvas_rect(&self, viewport: Vec2) -> IRect {
        let inv = self.screen_to_canvas();
        let corners = [
            Vec2::ZERO,
            Vec2::new(viewport.x, 0.0),
            viewport,
            Vec2::new(0.0, viewport.y),
        ]
        .map(|p| inv.transform_point2(p));
        let min = corners.iter().fold(Vec2::INFINITY, |m, p| m.min(*p));
        let max = corners.iter().fold(Vec2::NEG_INFINITY, |m, p| m.max(*p));
        IRect::enclosing(min.x, min.y, max.x, max.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-3
    }

    fn sample_view() -> ViewTransform {
        ViewTransform {
            pan: Vec2::new(120.0, -40.0),
            zoom: 2.5,
            rotation: 0.7,
        }
    }

    #[test]
    fn screen_canvas_round_trip() {
        let v = sample_view();
        for p in [Vec2::ZERO, Vec2::new(10.0, 3.0), Vec2::new(-500.0, 2000.0)] {
            assert!(close(v.to_canvas(v.to_screen(p)), p));
        }
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut v = sample_view();
        let anchor = Vec2::new(300.0, 200.0);
        let before = v.to_canvas(anchor);
        v.zoom_at(anchor, 1.7);
        assert!(close(v.to_canvas(anchor), before));
        assert!((v.zoom - 2.5 * 1.7).abs() < 1e-5);
    }

    #[test]
    fn rotate_keeps_anchor_fixed() {
        let mut v = sample_view();
        let anchor = Vec2::new(-50.0, 640.0);
        let before = v.to_canvas(anchor);
        v.rotate_at(anchor, 1.1);
        assert!(close(v.to_canvas(anchor), before));
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = ViewTransform::default();
        v.zoom_at(Vec2::ZERO, 1e9);
        assert_eq!(v.zoom, ViewTransform::MAX_ZOOM);
    }

    #[test]
    fn fit_centers_canvas() {
        let v = ViewTransform::fit(Vec2::new(1000.0, 500.0), Vec2::new(800.0, 800.0));
        let center = v.to_screen(Vec2::new(500.0, 250.0));
        assert!(close(center, Vec2::new(400.0, 400.0)));
    }

    #[test]
    fn visible_rect_contains_viewport_corners() {
        let v = sample_view();
        let vp = Vec2::new(800.0, 600.0);
        let r = v.visible_canvas_rect(vp);
        for s in [Vec2::ZERO, vp, Vec2::new(800.0, 0.0), Vec2::new(0.0, 600.0)] {
            let c = v.to_canvas(s);
            assert!(c.x >= r.x0 as f32 && c.x <= r.x1 as f32);
            assert!(c.y >= r.y0 as f32 && c.y <= r.y1 as f32);
        }
    }
}
