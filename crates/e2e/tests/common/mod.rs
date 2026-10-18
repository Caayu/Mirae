//! Shared by the E2E test files: the engine as an oracle, and small steps
//! every test repeats.

#![allow(dead_code)]

use serde_json::Value;
use tela_color::{Color, PixelDepth};
use tela_document::{
    ComposeScratch, Document, FillTarget, Gradient, GradientExtend, GradientGeometry,
    GradientPaint, GradientShape, GradientStop, GradientStops, LayerPart, Mixing,
};
use tela_e2e::Session;

/// The document point under client pixel `p`, through the app's view (the
/// inverse of the state's `to_screen`): where the app put a press.
pub fn client_to_doc(state: &Value, p: [f32; 2]) -> [f64; 2] {
    let m: Vec<f64> = state["view"]["to_screen"]
        .as_array()
        .expect("view")
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let (a, b, c, d, e, f) = (m[0], m[1], m[2], m[3], m[4], m[5]);
    let det = a * e - b * d;
    let (x, y) = (f64::from(p[0]) - c, f64::from(p[1]) - f);
    [(e * x - b * y) / det, (-d * x + a * y) / det]
}

/// The client pixel the driver presses for document point `p` (rounded,
/// as the cursor lands on whole pixels).
pub fn press_point(s: &Session, p: [f32; 2]) -> [f32; 2] {
    let c = s.doc_to_client(p[0], p[1]);
    [c[0].round(), c[1].round()]
}

/// The gradient settings the panel shows, read from the app's state.
pub struct Settings {
    pub shape: GradientShape,
    pub extend: GradientExtend,
    pub mixing: Mixing,
    pub dither: bool,
    pub stops: GradientStops,
}

pub fn hex(s: &str) -> Color {
    Color::from_hex(s).unwrap_or_else(|_| panic!("colour {s}"))
}

/// The gradient tool's settings in the app now, stops resolved with the
/// app's main and sub colours.
pub fn settings(state: &Value) -> Settings {
    let g = &state["gradient"];
    let main = hex(state["colors"]["main"].as_str().unwrap());
    let sub = hex(state["colors"]["sub"].as_str().unwrap());
    let shape = match g["kind"].as_str().unwrap() {
        "Linear" => GradientShape::Linear,
        "Radial" => GradientShape::Radial,
        "Elliptical" => GradientShape::Elliptical,
        "Angular" => GradientShape::Angular,
        "Reflected" => GradientShape::Reflected,
        k => panic!("shape {k}"),
    };
    let extend = match g["edge"].as_str().unwrap() {
        "Clamp" => GradientExtend::Clamp,
        "Repeat" => GradientExtend::Repeat,
        "Reflect" => GradientExtend::Reflect,
        "Empty" => GradientExtend::Empty,
        e => panic!("ends {e}"),
    };
    let mixing = match g["mixing"].as_str().unwrap() {
        "Perceptual" => Mixing::Perceptual,
        "Standard" => Mixing::Standard,
        "Linear" => Mixing::Linear,
        m => panic!("mixing {m}"),
    };
    let stops = GradientStops::new(g["stops"].as_array().unwrap().iter().map(|s| {
        let color = match s["color"].as_str().unwrap() {
            "main" => main,
            "sub" => sub,
            h => hex(h),
        };
        GradientStop::new(
            s["position"].as_f64().unwrap() as f32,
            color,
            s["opacity"].as_f64().unwrap() as f32,
        )
    }))
    .expect("stops");
    Settings {
        shape,
        extend,
        mixing,
        dither: g["dither"].as_bool().unwrap(),
        stops,
    }
}

/// What the app should have drawn: the same gradient applied by the engine
/// to a fresh document of the same size and depth (one transparent layer).
pub struct Oracle {
    pub doc: Document,
    scratch: ComposeScratch,
}

impl Oracle {
    pub fn new(state: &Value) -> Self {
        let d = &state["document"];
        let depth = if d["bits"].as_u64() == Some(16) {
            PixelDepth::Rgba16
        } else {
            PixelDepth::Rgba8
        };
        Self {
            doc: Document::with_depth(
                d["width"].as_u64().unwrap() as u32,
                d["height"].as_u64().unwrap() as u32,
                depth,
            ),
            scratch: ComposeScratch::new(),
        }
    }

    /// Draws the gradient from document point `a` to `b` with `s`.
    pub fn draw(&mut self, s: &Settings, a: [f64; 2], b: [f64; 2]) {
        let g = Gradient::new(
            GradientGeometry {
                shape: s.shape,
                start: a,
                end: b,
            },
            &s.stops,
            s.mixing,
            s.extend,
            s.dither,
        );
        let target = FillTarget {
            layer: self.doc.active_layer(),
            part: LayerPart::Pixels,
        };
        self.doc
            .apply_gradient(target, &g, GradientPaint::Draw)
            .expect("the oracle draws");
    }

    /// Straight RGBA8 of the composite over the paper at `(x, y)`.
    pub fn composite(&mut self, x: i32, y: i32) -> [u8; 4] {
        let p = self
            .doc
            .composite_pixel(x, y, &mut self.scratch)
            .expect("inside");
        let c = p.unpremultiply();
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(c.r), q(c.g), q(c.b), q(c.a)]
    }
}

/// Largest channel difference.
pub fn diff(a: [u8; 4], b: [u8; 4]) -> u8 {
    a.iter().zip(b).map(|(x, y)| x.abs_diff(y)).max().unwrap()
}

/// A grid of document points covering the canvas (`n` × `n`).
pub fn grid(state: &Value, n: i32) -> Vec<[i32; 2]> {
    let w = state["document"]["width"].as_i64().unwrap() as i32;
    let h = state["document"]["height"].as_i64().unwrap() as i32;
    let mut out = Vec::new();
    for j in 0..n {
        for i in 0..n {
            out.push([(w * (2 * i + 1)) / (2 * n), (h * (2 * j + 1)) / (2 * n)]);
        }
    }
    out
}

impl Oracle {
    /// The oracle layer's stored pixel at `(x, y)`, straight RGBA8.
    pub fn layer(&self, x: i32, y: i32) -> [u8; 4] {
        let p = self
            .doc
            .layer(self.doc.active_layer())
            .expect("layer")
            .pixels()
            .pixel(x, y);
        let c = p.unpremultiply();
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(c.r), q(c.g), q(c.b), q(c.a)]
    }
}
