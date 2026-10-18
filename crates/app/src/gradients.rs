//! The gradient tool (phase 5F, ADR 0027): its settings and stops, and the
//! gradient being dragged. Session tool state, like the fill tool's: not
//! in the document, its history or its file. A drag is `begin → update →
//! commit | cancel`: the document changes only at the commit, one history
//! step, through the same per-pixel function the preview used.

use glam::Vec2;
use tela_canvas::ViewTransform;
use tela_color::Color;
use tela_document::{
    FillTarget, Gradient, GradientExtend, GradientGeometry, GradientPaint, GradientShape,
    GradientStop, GradientStops, Mixing,
};
use tela_ui::{GradientAction, GradientEdge, GradientKind, GradientPreset, StopColor, StopRow};

/// Settings of the gradient tool. The defaults: linear, the end colours
/// continuing, main to sub colour (the reference's default sub-tool,
/// GD1, GD2), dithering on (GD3), OKLab mixing (the nearest of Mirae's
/// three to the reference's default "Perceptiva", D-5F-1).
#[derive(Clone, Debug, PartialEq)]
pub struct GradientTool {
    pub kind: GradientKind,
    pub edge: GradientEdge,
    pub mixing: Mixing,
    pub dither: bool,
    /// Ordered by position; never empty.
    stops: Vec<StopRow>,
    selected: usize,
}

impl Default for GradientTool {
    fn default() -> Self {
        let mut t = Self {
            kind: GradientKind::Linear,
            edge: GradientEdge::Clamp,
            mixing: Mixing::Perceptual,
            dither: true,
            stops: Vec::new(),
            selected: 0,
        };
        t.preset(GradientPreset::MainToSub);
        t
    }
}

impl GradientTool {
    pub fn stops(&self) -> &[StopRow] {
        &self.stops
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn preset(&mut self, p: GradientPreset) {
        let end = match p {
            GradientPreset::MainToSub => StopRow {
                position: 1.0,
                color: StopColor::Sub,
                opacity: 1.0,
            },
            GradientPreset::MainToTransparent => StopRow {
                position: 1.0,
                color: StopColor::Main,
                opacity: 0.0,
            },
        };
        self.stops = vec![
            StopRow {
                position: 0.0,
                color: StopColor::Main,
                opacity: 1.0,
            },
            end,
        ];
        self.selected = 0;
    }

    /// Applies a panel request (`active` is the colour the user paints
    /// with now, for "set this stop to the active colour"). Tool and
    /// shape changes are the app's.
    pub fn apply(&mut self, action: GradientAction, active: Color) {
        let i = self.selected;
        match action {
            GradientAction::Kind(k) => self.kind = k,
            GradientAction::Edge(e) => self.edge = e,
            GradientAction::Mixing(m) => self.mixing = m,
            GradientAction::Dither(on) => self.dither = on,
            GradientAction::Preset(p) => self.preset(p),
            GradientAction::Select(k) => self.selected = k.min(self.stops.len() - 1),
            GradientAction::Position(p) => {
                self.stops[i].position = unit(p);
                self.sort();
            }
            GradientAction::Opacity(o) => self.stops[i].opacity = unit(o),
            GradientAction::Color(c) => self.stops[i].color = c,
            GradientAction::UseActiveColor => self.stops[i].color = StopColor::Custom(active),
            GradientAction::Add => {
                if self.stops.len() >= tela_color::gradient::MAX_STOPS {
                    return;
                }
                // Halfway to the next stop (or the previous, at the end),
                // with the colour found there.
                let s = self.stops[i];
                let other = self
                    .stops
                    .get(i + 1)
                    .or(i.checked_sub(1).and_then(|k| self.stops.get(k)));
                let position = other.map_or(if s.position < 0.5 { 1.0 } else { 0.0 }, |o| {
                    (s.position + o.position) / 2.0
                });
                self.stops.push(StopRow { position, ..s });
                self.selected = self.stops.len() - 1;
                self.sort();
            }
            GradientAction::Remove => {
                if self.stops.len() > 1 {
                    self.stops.remove(i);
                    self.selected = i.min(self.stops.len() - 1);
                }
            }
            GradientAction::Reverse => {
                for s in &mut self.stops {
                    s.position = 1.0 - s.position;
                }
                self.stops.reverse();
                self.selected = self.stops.len() - 1 - i;
            }
        }
    }

    /// Keeps the stops in position order (stably), following the selected
    /// one.
    fn sort(&mut self) {
        let mut order: Vec<usize> = (0..self.stops.len()).collect();
        order.sort_by(|&a, &b| self.stops[a].position.total_cmp(&self.stops[b].position));
        self.selected = order.iter().position(|&k| k == self.selected).unwrap_or(0);
        self.stops = order.iter().map(|&k| self.stops[k]).collect();
    }

    /// The stops with their colours resolved now (captured when a drag
    /// begins, invariant GR1: nothing is read again during it).
    pub fn resolve(&self, main: Color, sub: Color) -> GradientStops {
        GradientStops::new(
            self.stops
                .iter()
                .map(|s| GradientStop::new(s.position, s.color.resolve(main, sub), s.opacity)),
        )
        .expect("the tool always has a stop")
    }

    pub fn shape(&self) -> GradientShape {
        match self.kind {
            GradientKind::Linear => GradientShape::Linear,
            GradientKind::Radial => GradientShape::Radial,
            GradientKind::Elliptical => GradientShape::Elliptical,
            GradientKind::Angular => GradientShape::Angular,
            GradientKind::Reflected => GradientShape::Reflected,
        }
    }

    pub fn extend(&self) -> GradientExtend {
        match self.edge {
            GradientEdge::Clamp => GradientExtend::Clamp,
            GradientEdge::Repeat => GradientExtend::Repeat,
            GradientEdge::Reflect => GradientExtend::Reflect,
            GradientEdge::Empty => GradientExtend::Empty,
        }
    }
}

fn unit(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

/// A gradient being dragged: everything it draws was fixed when the drag
/// began, except where it ends.
pub struct GradientSession {
    pub target: FillTarget,
    pub paint: GradientPaint,
    shape: GradientShape,
    extend: GradientExtend,
    mixing: Mixing,
    dither: bool,
    stops: GradientStops,
    /// Document points of the press (fixed: a zoom or pan during the drag
    /// does not move it) and of the pointer now.
    start: Vec2,
    end: Vec2,
    /// The colours a draw adds to the recent colours when committed.
    pub colors: Vec<Color>,
}

impl GradientSession {
    /// Begins at document point `start` (the press through the view) with
    /// the tool's settings and the colours resolved now.
    pub fn begin(
        tool: &GradientTool,
        target: FillTarget,
        paint: GradientPaint,
        main: Color,
        sub: Color,
        start: Vec2,
    ) -> Self {
        let colors = tool
            .stops
            .iter()
            .filter(|s| s.opacity > 0.0)
            .map(|s| s.color.resolve(main, sub))
            .collect();
        Self {
            target,
            paint,
            shape: tool.shape(),
            extend: tool.extend(),
            mixing: tool.mixing,
            dither: tool.dither,
            stops: tool.resolve(main, sub),
            start,
            end: start,
            colors,
        }
    }

    /// The pointer moved to screen point `p`, mapped through `view` (the
    /// one screen → document mapping, any zoom, rotation or pan). With
    /// `snap`, the drag keeps to multiples of 45° on screen (the end
    /// projected on that direction, GD23), and an ellipse becomes a circle.
    pub fn update(&mut self, p: Vec2, snap: bool, view: &ViewTransform) {
        let p = if snap {
            snapped(view.to_screen(self.start), p, self.shape)
        } else {
            p
        };
        self.end = view.to_canvas(p);
    }

    /// The gradient in document coordinates.
    pub fn gradient(&self) -> Gradient {
        let (a, b) = (self.start, self.end);
        Gradient::new(
            GradientGeometry {
                shape: self.shape,
                start: [f64::from(a.x), f64::from(a.y)],
                end: [f64::from(b.x), f64::from(b.y)],
            },
            &self.stops,
            self.mixing,
            self.extend,
            self.dither,
        )
    }

    /// The guide in screen points: the drag, and where a radial or
    /// elliptical gradient ends (its outline mapped through the view).
    pub fn guide(&self, view: &ViewTransform, points_per_pixel: f32) -> tela_ui::GradientGuide {
        let pos =
            |v: Vec2| tela_ui::egui::Pos2::new(v.x / points_per_pixel, v.y / points_per_pixel);
        let (a, b) = (self.start, self.end);
        let radii = match self.shape {
            GradientShape::Radial => Some(Vec2::splat(a.distance(b))),
            GradientShape::Elliptical => Some((b - a).abs()),
            _ => None,
        };
        let outline = radii
            .filter(|r| r.x > 0.0 && r.y > 0.0)
            .map(|r| {
                (0..96)
                    .map(|k| {
                        let t = k as f32 / 96.0 * std::f32::consts::TAU;
                        pos(view.to_screen(a + Vec2::new(r.x * t.cos(), r.y * t.sin())))
                    })
                    .collect()
            })
            .unwrap_or_default();
        tela_ui::GradientGuide {
            start: pos(view.to_screen(a)),
            end: pos(view.to_screen(b)),
            outline,
        }
    }
}

/// `p` constrained from `start` (screen points) to a multiple of 45°,
/// projected on it; for an ellipse, to a square (a circle).
fn snapped(start: Vec2, p: Vec2, shape: GradientShape) -> Vec2 {
    let d = p - start;
    if shape == GradientShape::Elliptical {
        let r = d.x.abs().max(d.y.abs());
        return start + Vec2::new(r * d.x.signum(), r * d.y.signum());
    }
    let step = std::f32::consts::FRAC_PI_4;
    let a = (d.y.atan2(d.x) / step).round() * step;
    let dir = Vec2::new(a.cos(), a.sin());
    start + dir * d.dot(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tela_document::{Document, LayerPart};

    #[test]
    fn defaults_are_the_references_main_to_sub() {
        let t = GradientTool::default();
        assert_eq!(t.kind, GradientKind::Linear);
        assert_eq!(t.edge, GradientEdge::Clamp);
        assert!(t.dither);
        let s = t.resolve(Color::BLACK, Color::WHITE);
        let p: Vec<_> = s
            .stops()
            .iter()
            .map(|s| (s.position, s.color, s.opacity))
            .collect();
        assert_eq!(p, [(0.0, Color::BLACK, 1.0), (1.0, Color::WHITE, 1.0)]);
    }

    #[test]
    fn editing_stops_keeps_them_ordered_and_selected() {
        let mut t = GradientTool::default();
        let red = Color::new(1.0, 0.0, 0.0);
        t.apply(GradientAction::Add, red);
        assert_eq!(t.stops().len(), 3);
        assert_eq!(t.stops()[t.selected()].position, 0.5);
        t.apply(GradientAction::UseActiveColor, red);
        t.apply(GradientAction::Position(0.9), red);
        // The stops re-sort to 0, 0.9, 1.0 and the moved one stays selected.
        assert_eq!(t.selected(), 1);
        assert_eq!(t.stops()[1].color, StopColor::Custom(red));
        t.apply(GradientAction::Opacity(f32::NAN), red);
        assert_eq!(t.stops()[1].opacity, 0.0);
        t.apply(GradientAction::Reverse, red);
        let p: Vec<_> = t.stops().iter().map(|s| s.position).collect();
        assert!(
            (p[1] - 0.1).abs() < 1e-6 && p[0] == 0.0 && p[2] == 1.0,
            "{p:?}"
        );
        assert_eq!(t.stops()[t.selected()].color, StopColor::Custom(red));
        for _ in 0..5 {
            t.apply(GradientAction::Remove, red);
        }
        assert_eq!(t.stops().len(), 1, "one stop always remains");
        t.apply(GradientAction::Select(99), red);
        assert_eq!(t.selected(), 0);
        t.apply(
            GradientAction::Preset(GradientPreset::MainToTransparent),
            red,
        );
        let s = t.resolve(red, Color::WHITE);
        assert_eq!(s.stops()[1].opacity, 0.0);
        assert_eq!(s.stops()[1].color, red);
    }

    #[test]
    fn a_session_captures_colours_and_maps_through_the_view() {
        let d = Document::new(64, 64);
        let target = FillTarget {
            layer: d.active_layer(),
            part: LayerPart::Pixels,
        };
        let tool = GradientTool::default();
        let mut view = ViewTransform::default();
        view.zoom_at(Vec2::ZERO, 2.0);
        let press = view.to_canvas(Vec2::new(20.0, 10.0));
        let mut s = GradientSession::begin(
            &tool,
            target,
            GradientPaint::Draw,
            Color::BLACK,
            Color::WHITE,
            press,
        );
        s.update(Vec2::new(220.0, 30.0), false, &view);
        let g = s.gradient();
        let geo = g.geometry();
        assert_eq!(geo.start, [10.0, 5.0]);
        assert_eq!(geo.end, [110.0, 15.0]);
        // Shift: snapped to the horizontal, the end projected (GD23).
        s.update(Vec2::new(220.0, 30.0), true, &view);
        let geo = s.gradient().geometry();
        // A zoom during the drag does not move the start.
        view.zoom_at(Vec2::ZERO, 3.0);
        assert_eq!(s.gradient().geometry().start, [10.0, 5.0]);
        assert!(
            (geo.end[1] - 5.0).abs() < 1e-4 && (geo.end[0] - 110.0).abs() < 1e-4,
            "{geo:?}"
        );
        // Colours were captured: the stops do not follow later changes.
        assert_eq!(s.colors, [Color::BLACK, Color::WHITE]);
    }
}
