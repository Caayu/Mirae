//! The fill (bucket) tool: its settings and the document pixel a click
//! fills from (phase 5E, ADR 0026). Session tool state, like the selection
//! tool's: not in the document, its history or its file.

use glam::Vec2;
use tela_canvas::ViewTransform;
use tela_document::{ConnectedFill, FillReference, Tolerance};

/// Settings of the fill tool. The defaults are the reference's default
/// sub-tool ("refer to other layers": the visible canvas; tolerance 10 %;
/// anti-aliasing on; parity record 5E BK1–BK3), without its gap closing
/// and area scaling (D-5E-2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillTool {
    pub reference: FillReference,
    pub tolerance: Tolerance,
    pub anti_alias: bool,
}

impl Default for FillTool {
    fn default() -> Self {
        Self {
            reference: FillReference::Composite,
            tolerance: Tolerance::DEFAULT,
            anti_alias: true,
        }
    }
}

impl FillTool {
    /// The fill a click at document pixel `seed` makes.
    pub fn connected(&self, seed: (i32, i32)) -> ConnectedFill {
        ConnectedFill {
            seed,
            reference: self.reference,
            tolerance: self.tolerance,
            anti_alias: self.anti_alias,
        }
    }
}

/// The document pixel under screen point `p`, through the one view
/// mapping, at any zoom and rotation (reference BK34, BK35).
pub fn seed_at(view: &ViewTransform, p: Vec2) -> (i32, i32) {
    let c = view.to_canvas(p);
    (c.x.floor() as i32, c.y.floor() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_references() {
        let t = FillTool::default();
        assert_eq!(t.reference, FillReference::Composite);
        assert_eq!(t.tolerance.percent(), 10.0);
        assert!(t.anti_alias);
        let f = t.connected((3, -4));
        assert_eq!(f.seed, (3, -4));
        assert_eq!(f.tolerance, t.tolerance);
    }

    #[test]
    fn the_seed_is_the_document_pixel_under_the_cursor() {
        let mut view = ViewTransform::default();
        // Identity: screen pixel = document pixel.
        assert_eq!(seed_at(&view, Vec2::new(10.7, 3.2)), (10, 3));
        assert_eq!(seed_at(&view, Vec2::new(-0.2, 0.0)), (-1, 0));
        // Zoomed 400 % and rotated: the pixel whose square holds the point.
        view.zoom_at(Vec2::ZERO, 4.0);
        view.rotate_at(Vec2::new(50.0, 50.0), 0.5);
        for (x, y) in [(5, 7), (0, 0), (123, 45)] {
            let centre = view.to_screen(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
            assert_eq!(seed_at(&view, centre), (x, y));
        }
    }
}
