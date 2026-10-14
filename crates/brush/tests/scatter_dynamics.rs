//! Scatter driven by effectors, through the whole stroke pipeline.

use glam::Vec2;
use tela_brush::dynamics::{BrushProperty, Curve, DynamicsSource, Effector};
use tela_brush::scatter::ScatterMode;
use tela_brush::{BrushInput, BrushPreset as BrushSettings, Dab, DabLog, StrokeProcessor};

fn draw(settings: &BrushSettings, pressure: f32) -> Vec<Dab> {
    let mut p = StrokeProcessor::new();
    let mut log = DabLog::default();
    let at = |i: usize| {
        BrushInput::from_position(Vec2::new(100.0 + i as f32 * 5.0, 200.0), i as f64 * 0.005)
            .with_pressure(pressure)
    };
    p.begin(settings, &Curve::linear(), 1.0, at(0), &mut log);
    for i in 1..=120 {
        p.push(at(i), &mut log);
    }
    p.finish(&mut log);
    log.all().copied().collect()
}

fn spread(dabs: &[Dab]) -> f32 {
    dabs.iter()
        .map(|d| (d.center.y - 200.0).abs())
        .fold(0.0, f32::max)
}

fn scattered() -> BrushSettings {
    let mut s = BrushSettings::plain(10.0);
    // Full scatter at no pressure, none at full pressure.
    s.properties.scatter = BrushProperty::constant(1.0).with(Effector::new(
        DynamicsSource::Pressure,
        Curve::new(&[(0.0, 1.0), (1.0, 0.0)]),
    ));
    s
}

#[test]
fn more_pressure_means_less_scatter() {
    let s = scattered();
    let (light, medium, firm) = (
        spread(&draw(&s, 0.1)),
        spread(&draw(&s, 0.5)),
        spread(&draw(&s, 1.0)),
    );
    assert!(light > medium && medium > firm, "{light} {medium} {firm}");
    assert!(firm < 1e-3);
    // Scatter never changes how many dabs are placed.
    assert_eq!(draw(&s, 0.1).len(), draw(&s, 1.0).len());
}

#[test]
fn more_speed_means_more_scatter() {
    let mut s = BrushSettings::plain(10.0);
    s.properties.scatter =
        BrushProperty::constant(1.0).with(Effector::new(DynamicsSource::Speed, Curve::linear()));
    s.scatter.mode = ScatterMode::Radial;
    let run = |dt: f64| {
        let mut p = StrokeProcessor::new();
        let mut log = DabLog::default();
        let at =
            |i: usize| BrushInput::from_position(Vec2::new(i as f32 * 5.0, 0.0), i as f64 * dt);
        p.begin(&s, &Curve::linear(), 1.0, at(0), &mut log);
        for i in 1..=200 {
            p.push(at(i), &mut log);
        }
        log.all().map(|d| d.center.y.abs()).sum::<f32>() / log.all().count() as f32
    };
    assert!(run(0.002) > run(0.02) * 2.0);
}

#[test]
fn scattered_strokes_are_reproducible() {
    let s = scattered();
    assert_eq!(draw(&s, 0.3), draw(&s, 0.3));
}
