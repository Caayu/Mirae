//! Property tests: invariants checked over generated inputs rather than
//! hand-picked examples. Each property names the invariant it protects (see
//! docs/architecture/invariants.md).

use glam::Vec2;
use proptest::prelude::*;
use tela_brush::dynamics::{BrushProperty, Curve, angle_delta, follow};
use tela_brush::interpolate::InterpolationKind;
use tela_brush::preset::builtin_presets;
use tela_brush::preset::io::{from_json, to_json};
use tela_brush::rng::StrokeRng;
use tela_brush::{BrushInput, BrushPreset, Dab, DabLog, StrokeProcessor};

fn config() -> ProptestConfig {
    ProptestConfig::with_cases(128)
}

/// Any f32, including NaN, infinities and out-of-range values.
fn any_value() -> impl Strategy<Value = f32> {
    prop_oneof![
        -2.0f32..3.0,
        Just(f32::NAN),
        Just(f32::INFINITY),
        Just(f32::NEG_INFINITY),
    ]
}

proptest! {
    #![proptest_config(config())]

    /// Curves never produce values outside [0, 1] or NaN, whatever points
    /// they were built from and wherever they are evaluated.
    #[test]
    fn curves_stay_in_the_unit_range(
        points in prop::collection::vec((any_value(), any_value()), 0..12),
        x in any_value(),
    ) {
        let c = Curve::new(&points);
        let y = c.eval(x);
        prop_assert!((0.0..=1.0).contains(&y), "{y}");
    }

    /// Monotone curves (pressure calibration) never decrease, including
    /// between control points (Fritsch–Carlson has no overshoot).
    #[test]
    fn monotone_curves_never_decrease(
        points in prop::collection::vec((0.0f32..=1.0, 0.0f32..=1.0), 2..10),
    ) {
        let c = Curve::new(&points).monotone();
        prop_assert!(c.is_monotone());
        let mut prev = c.eval(0.0);
        for i in 1..=200 {
            let y = c.eval(i as f32 / 200.0);
            prop_assert!(y >= prev - 1e-6, "dips from {prev} to {y}");
            prev = y;
        }
    }

    /// Editing operations keep a curve sorted, finite and in range.
    #[test]
    fn curve_edits_preserve_validity(
        inserts in prop::collection::vec(any_value(), 0..6),
        moves in prop::collection::vec((0usize..8, any_value(), any_value()), 0..6),
        removes in prop::collection::vec(0usize..8, 0..4),
    ) {
        let mut c = Curve::linear();
        for x in inserts {
            c.insert_point(x);
        }
        for (i, x, y) in moves {
            c.move_point(i, x, y);
        }
        for i in removes {
            c.remove_point(i);
        }
        let pts: Vec<(f32, f32)> = c.points().collect();
        prop_assert!(pts.len() >= 2);
        prop_assert!(pts.windows(2).all(|w| w[0].0 < w[1].0), "{pts:?}");
        prop_assert!(pts.iter().all(|&(x, y)| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)));
        prop_assert_eq!(pts[0].0, 0.0);
        prop_assert_eq!(pts[pts.len() - 1].0, 1.0);
    }

    /// The shortest angular difference lies in (-period/2, period/2] and
    /// really connects the two angles modulo the period.
    #[test]
    fn angle_delta_is_shortest_and_consistent(
        a in -1000.0f32..1000.0,
        b in -1000.0f32..1000.0,
        period in prop_oneof![Just(360.0f32), Just(180.0), Just(90.0)],
    ) {
        let d = angle_delta(a, b, period);
        prop_assert!(d > -period * 0.5 - 1e-3 && d <= period * 0.5 + 1e-3, "{d}");
        let residue = (a + d - b).rem_euclid(period);
        prop_assert!(residue.min(period - residue) < 1e-2, "{residue}");
    }

    /// Following never moves an angle by more than the shortest arc to the
    /// target, and amount 0 / 1 mean "ignore" / "match" (mod period).
    #[test]
    fn follow_moves_along_the_shortest_arc(
        from in 0.0f32..360.0,
        to in 0.0f32..360.0,
        amount in 0.0f32..=1.0,
        period in prop_oneof![Just(360.0f32), Just(180.0)],
    ) {
        let full = angle_delta(from, to, period).abs();
        let moved = angle_delta(from, follow(from, to, amount, period), period).abs();
        prop_assert!(moved <= full + 1e-2, "moved {moved} > {full}");
        prop_assert!(angle_delta(from, follow(from, to, 0.0, period), period).abs() < 1e-3);
        // Exactly opposite angles are the documented degenerate case.
        if full < period * 0.5 - 1.0 {
            let end = follow(from, to, 1.0, period);
            prop_assert!(angle_delta(end, to, period).abs() < 1e-2);
        }
    }

    /// Stroke randomness is a pure function of (seed, stream, index) and
    /// stays in its documented ranges.
    #[test]
    fn stroke_rng_is_pure_and_bounded(
        seed in any::<u64>(),
        stream in any::<u32>(),
        index in any::<u64>(),
        x in 0.0f32..1.0e6,
    ) {
        let (a, b) = (StrokeRng::new(seed), StrokeRng::new(seed));
        prop_assert_eq!(a.u64(stream, index), b.u64(stream, index));
        let u = a.uniform(stream, index);
        prop_assert!((0.0..1.0).contains(&u));
        let s = a.signed(stream, index);
        prop_assert!((-1.0..1.0).contains(&s));
        let n = a.noise(stream, x);
        prop_assert!((0.0..=1.0).contains(&n));
    }

    /// Validation is idempotent: a preset that loaded (whatever repairs it
    /// needed) saves and reloads without further warnings, unchanged.
    #[test]
    fn repaired_presets_round_trip_cleanly(
        which in 0usize..8,
        size in any_value(),
        opacity in any_value(),
        spacing in any_value(),
        taper_end in any_value(),
        rate in any_value(),
        count in any::<u32>(),
        effectors in 0usize..80,
    ) {
        let mut p = builtin_presets().swap_remove(which);
        p.properties.size.base = size;
        p.properties.opacity.base = opacity;
        p.properties.spacing.base = spacing;
        p.taper.end = taper_end;
        p.airbrush.rate = rate;
        p.scatter.count = count;
        let extra = p.properties.flow.effectors.first().cloned().unwrap_or_default();
        p.properties.flow.effectors.resize(effectors, extra);
        // Non-finite numbers serialize as JSON null, which a number field
        // rejects; repair first as an editor would before saving.
        p.validate();
        let once = from_json(&to_json(&p)).unwrap();
        let twice = from_json(&to_json(&once.preset)).unwrap();
        prop_assert!(twice.warnings.is_empty(), "{:?}", twice.warnings);
        prop_assert_eq!(&twice.preset, &once.preset);
        prop_assert!(once.preset.properties.flow.effectors.len() <= BrushProperty::MAX_EFFECTORS);
    }
}

/// Plain brush over straight segments: dab placement depends only on the
/// path, not on how many collinear samples describe it.
fn dabs_along(vertices: &[Vec2], subdivisions: &[usize]) -> Vec<Dab> {
    let mut preset = BrushPreset::plain(9.0);
    preset.stroke.interpolation = Some(InterpolationKind::Linear);
    let mut samples = vec![vertices[0]];
    for (seg, k) in vertices.windows(2).zip(subdivisions) {
        for j in 1..=*k {
            samples.push(seg[0].lerp(seg[1], j as f32 / *k as f32));
        }
    }
    let input = |i: usize, p: Vec2| BrushInput::from_position(p, i as f64 * 0.004);
    let mut proc = StrokeProcessor::new();
    let mut log = DabLog::default();
    proc.begin(
        &preset,
        &Curve::linear(),
        1.0,
        input(0, samples[0]),
        &mut log,
    );
    for (i, p) in samples.iter().enumerate().skip(1) {
        proc.push(input(i, *p), &mut log);
    }
    proc.finish(&mut log);
    log.all().copied().collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn spacing_is_independent_of_sample_density(
        vertices in prop::collection::vec((0.0f32..600.0, 0.0f32..600.0), 2..6),
        subdivisions in prop::collection::vec(1usize..25, 5),
    ) {
        let vertices: Vec<Vec2> = vertices.into_iter().map(Vec2::from).collect();
        // Segments shorter than a few pixels make the corner ambiguous at
        // the flattening resolution; real strokes are sampled densely.
        prop_assume!(vertices.windows(2).all(|w| w[0].distance(w[1]) > 4.0));
        let sparse = dabs_along(&vertices, &[1; 5]);
        let dense = dabs_along(&vertices, &subdivisions);
        prop_assert_eq!(sparse.len(), dense.len());
        for (a, b) in sparse.iter().zip(&dense) {
            prop_assert!(a.center.distance(b.center) < 0.01, "{} vs {}", a.center, b.center);
        }
    }
}

proptest! {
    #![proptest_config(config())]

    /// A1 / A2 (phase 4D): with alpha preserved, painting never changes a
    /// pixel's alpha and never creates pixels where there were none,
    /// whatever the colour, opacity, flow and position of the stroke.
    #[test]
    fn preserved_alpha_never_changes_alpha(
        pixels in prop::collection::vec(any::<[u8; 4]>(), 64),
        color in any::<[u8; 3]>(),
        opacity in 0.0f32..=1.0,
        flow in 0.01f32..=1.0,
        x in 0.0f32..8.0,
        y in 0.0f32..8.0,
    ) {
        preserved_alpha_holds(&pixels, color, opacity, flow, x, y)?;
    }

    /// A1 / A2 at 16 bits per channel (phase 4E): the same stroke rule,
    /// quantized to the finer storage.
    #[test]
    fn preserved_alpha_never_changes_alpha_at_16_bits(
        pixels in prop::collection::vec(any::<[u16; 4]>(), 64),
        color in any::<[u8; 3]>(),
        opacity in 0.0f32..=1.0,
        flow in 0.01f32..=1.0,
        x in 0.0f32..8.0,
        y in 0.0f32..8.0,
    ) {
        preserved_alpha_holds(&pixels, color, opacity, flow, x, y)?;
    }
}

/// Paints one dab with alpha preserved over an 8 × 8 corner of a tile of
/// valid premultiplied pixels and checks A1, A2 and validity.
fn preserved_alpha_holds<P>(
    pixels: &[P],
    color: [u8; 3],
    opacity: f32,
    flow: f32,
    x: f32,
    y: f32,
) -> Result<(), TestCaseError>
where
    P: Channels,
{
    use tela_brush::{DabSink, PixelAlpha, StampContext, StrokeBuffer};
    use tela_canvas::{Tile, TileCoord};
    use tela_color::Rgba;
    let [r, g, b] = color.map(|v| f32::from(v) / 255.0);
    let mut s = StrokeBuffer::new(
        Rgba::new(r, g, b, 1.0),
        StampContext {
            anti_aliasing: true,
            ..Default::default()
        },
    );
    s.dab(&Dab {
        center: Vec2::new(x, y),
        radius: 3.0,
        opacity,
        flow,
        ..Dab::default()
    });
    let mut tile: Tile<P> = Tile::new();
    for (i, p) in pixels.iter().enumerate() {
        // Valid premultiplied input: colour never above alpha.
        let [r, g, b, a] = p.channels();
        *tile.get_mut(i as u32 % 8, i as u32 / 8) =
            P::from_channels([r.min(a), g.min(a), b.min(a), a]);
    }
    let before = tile.clone();
    s.commit_tile(TileCoord { x: 0, y: 0 }, &mut tile, PixelAlpha::Preserved);
    for (a, b) in tile.pixels().iter().zip(before.pixels()) {
        let (a, b) = (a.channels(), b.channels());
        prop_assert_eq!(a[3], b[3], "A1");
        if b[3] == 0 {
            prop_assert_eq!(a, [0; 4], "A2");
        }
        prop_assert!(a[0] <= a[3] && a[1] <= a[3] && a[2] <= a[3]);
    }
    Ok(())
}

/// Channel access for the stored formats under test.
trait Channels: tela_color::StoragePixel {
    fn channels(self) -> [u32; 4];
    fn from_channels(c: [u32; 4]) -> Self;
}

impl Channels for [u8; 4] {
    fn channels(self) -> [u32; 4] {
        self.map(u32::from)
    }
    fn from_channels(c: [u32; 4]) -> Self {
        c.map(|v| v as u8)
    }
}

impl Channels for [u16; 4] {
    fn channels(self) -> [u32; 4] {
        self.map(u32::from)
    }
    fn from_channels(c: [u32; 4]) -> Self {
        c.map(|v| v as u16)
    }
}
