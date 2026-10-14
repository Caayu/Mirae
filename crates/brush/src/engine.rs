use std::collections::VecDeque;

use glam::Vec2;
use tela_color::Rgba;

use crate::airbrush::TimeDeposit;
use crate::dynamics::{Curve, DynamicsInput, slot};
use crate::interpolate::{Interpolator, PathInterpolator};
use crate::pipeline::{JitterFilter, StrokePoint, VelocityEstimator};
use crate::preset::{BrushPreset, Param, StrokeSettings};
use crate::rng::StrokeRng;
use crate::stabilizer::{Stabilizer, StrokeStabilizer, catch_up, stabilizer};
use crate::texture::CanvasTexture;
use crate::{BrushInput, Dab, DabSink, DabSite, DabSpacer, StampContext, StrokeBuffer};

/// Counters for one stroke, for instrumentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StrokeStats {
    pub samples: u32,
    pub path_points: u32,
    pub committed_dabs: u32,
    /// Dabs in the most recent tail rebuild.
    pub tail_dabs: u32,
    /// Time-based (airbrush) dabs.
    pub airbrush_dabs: u32,
}

/// Airbrush dabs use their own index range so their random values never
/// coincide with those of movement dabs.
const AIRBRUSH_INDEX_BASE: u64 = 1 << 40;

/// Per-stroke parameters that are not part of the brush.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeParams {
    /// Screen pixels per canvas pixel (the view zoom).
    pub scale: f32,
    /// Seeds every random choice of the stroke. Same preset + seed + input
    /// samples give identical dabs.
    pub seed: u64,
    /// Stroke processing settings, already resolved from user defaults
    /// and preset overrides.
    pub stroke: StrokeSettings,
}

impl StrokeParams {
    pub fn new(preset: &BrushPreset, user: &StrokeSettings, scale: f32, seed: u64) -> Self {
        Self {
            scale,
            seed,
            stroke: preset.stroke.resolve(user),
        }
    }
}

/// Runs one stroke through every stage after input normalization:
///
/// ```text
/// pressure curve -> velocity -> jitter filter -> stabilizer -> interpolator
///   -> dab spacing -> dynamics, taper & scatter -> DabSink
/// ```
///
/// The last `taper.end` canvas pixels of the path form a provisional tail
/// that is re-emitted as if the stroke ended there, so the preview always
/// shows what lifting the pen now would produce. Points further back than
/// that can no longer be affected by the end taper and are committed.
///
/// All intermediate buffers are owned and reused: no allocation per sample.
#[derive(Debug)]
pub struct StrokeProcessor {
    preset: BrushPreset,
    pressure_curve: Curve,
    /// Screen pixels per canvas pixel.
    scale: f32,
    rng: StrokeRng,
    velocity: VelocityEstimator,
    jitter: JitterFilter,
    stabilizer: Stabilizer,
    interpolator: Interpolator,
    spacer: DabSpacer,
    airbrush: TimeDeposit,
    /// Path points not yet committed, with their arc length from the start.
    tail: VecDeque<(StrokePoint, f32)>,
    tail_dirty: bool,
    head_distance: f32,
    last_path_point: Option<StrokePoint>,
    /// Last sample that entered the stabilizer, target of the final catch-up.
    last_filtered_input: Option<StrokePoint>,
    stats: StrokeStats,
    active: bool,
    buf_jitter: Vec<StrokePoint>,
    buf_stable: Vec<StrokePoint>,
    buf_path: Vec<StrokePoint>,
    buf_sites: Vec<DabSite>,
    buf_dabs: Vec<Dab>,
}

impl Default for StrokeProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl StrokeProcessor {
    pub fn new() -> Self {
        let stroke = StrokeSettings::default();
        Self {
            stabilizer: stabilizer(stroke.stabilizer, stroke.stabilization),
            interpolator: Interpolator::new(stroke.interpolation),
            preset: BrushPreset::default(),
            pressure_curve: Curve::linear(),
            scale: 1.0,
            rng: StrokeRng::default(),
            velocity: VelocityEstimator::default(),
            jitter: JitterFilter::default(),
            spacer: DabSpacer::new(),
            airbrush: TimeDeposit::default(),
            tail: VecDeque::with_capacity(1024),
            tail_dirty: false,
            head_distance: 0.0,
            last_path_point: None,
            last_filtered_input: None,
            stats: StrokeStats::default(),
            active: false,
            buf_jitter: Vec::with_capacity(16),
            buf_stable: Vec::with_capacity(256),
            buf_path: Vec::with_capacity(1024),
            buf_sites: Vec::with_capacity(1024),
            buf_dabs: Vec::with_capacity(1024),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn stats(&self) -> StrokeStats {
        self.stats
    }

    pub fn preset(&self) -> &BrushPreset {
        &self.preset
    }

    /// Updates the view scale mid-stroke (e.g. the user zoomed).
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.max(1e-4);
    }

    /// Longest chord of the flattened path: about 1.5 screen pixels, but
    /// never coarser than one canvas pixel (strokes made zoomed out must
    /// still be smooth when inspected zoomed in).
    fn flatten_step(&self) -> f32 {
        (1.5 / self.scale).clamp(0.1, 1.0)
    }

    /// Starts a stroke with the preset's own stroke settings over the
    /// defaults, seed 0. Convenient for tests and tools.
    pub fn begin(
        &mut self,
        preset: &BrushPreset,
        pressure_curve: &Curve,
        scale: f32,
        first: BrushInput,
        sink: &mut impl DabSink,
    ) {
        let params = StrokeParams::new(preset, &StrokeSettings::default(), scale, 0);
        self.begin_with(preset, pressure_curve, &params, first, sink);
    }

    pub fn begin_with(
        &mut self,
        preset: &BrushPreset,
        pressure_curve: &Curve,
        params: &StrokeParams,
        first: BrushInput,
        sink: &mut impl DabSink,
    ) {
        self.preset.clone_from(preset);
        self.pressure_curve.clone_from(pressure_curve);
        self.set_scale(params.scale);
        self.rng = StrokeRng::new(params.seed);
        let s = &params.stroke;
        self.velocity.reset();
        self.jitter = JitterFilter::new(s.jitter_threshold);
        self.stabilizer = stabilizer(s.stabilizer, s.stabilization);
        self.interpolator = Interpolator::new(s.interpolation);
        self.spacer.reset();
        self.airbrush.reset();
        self.tail.clear();
        self.tail_dirty = false;
        self.head_distance = 0.0;
        self.last_path_point = None;
        self.last_filtered_input = None;
        self.stats = StrokeStats::default();
        self.active = true;
        self.push(first, sink);
    }

    pub fn push(&mut self, input: BrushInput, sink: &mut impl DabSink) {
        if !self.active {
            return;
        }
        self.stats.samples += 1;
        let mut p = StrokePoint::from(input);
        if p.caps.pressure {
            p.pressure = self.pressure_curve.eval(p.pressure);
        }
        self.velocity.process(&mut p);

        let step = self.flatten_step();
        self.buf_jitter.clear();
        self.jitter.push(p, self.scale, &mut self.buf_jitter);
        self.buf_path.clear();
        for q in &self.buf_jitter {
            self.last_filtered_input = Some(*q);
            let s = self.stabilizer.push(*q, self.scale);
            self.interpolator.push(s, step, &mut self.buf_path);
        }
        self.consume_path(sink);
        if let Some(t) = self.airbrush.last_time() {
            self.emit_airbrush(t, sink);
        }
    }

    /// Advances the airbrush clock while the pen may be resting (no samples
    /// arriving). `now` uses the same clock as the input timestamps. Call
    /// once per frame during a stroke; frame rate does not affect the result.
    pub fn advance_time(&mut self, now: f64, sink: &mut impl DabSink) {
        if self.active {
            let horizon = self.airbrush.horizon_at(now);
            self.emit_airbrush(horizon, sink);
        }
    }

    fn emit_airbrush(&mut self, horizon: f64, sink: &mut impl DabSink) {
        if !self.preset.airbrush.enabled {
            return;
        }
        self.buf_dabs.clear();
        let Self {
            preset,
            scale,
            rng,
            buf_dabs,
            airbrush,
            ..
        } = self;
        airbrush.emit(preset.airbrush.rate, horizon, |k, p, distance, moved| {
            let index = AIRBRUSH_INDEX_BASE + k;
            let input = dynamics_at(p, distance, index, *scale, rng);
            let taper = preset.taper.factor(distance, f32::INFINITY);
            let dab = resolve_dab(preset, &input, p.pos, taper);
            let dir = p.velocity.try_normalize().unwrap_or(Vec2::X);
            // A pen that covered a full spacing step since the last instant
            // is moving: its movement dabs already carry the deposition.
            if moved >= preset.properties.eval(Param::Spacing, &input) * dab.extent_along(dir) {
                return;
            }
            let scatter = preset.properties.eval(Param::Scatter, &input);
            preset
                .scatter
                .apply(&dab, scatter, dir, rng, index, buf_dabs);
        });
        sink.dabs(&self.buf_dabs);
        self.stats.airbrush_dabs += self.buf_dabs.len() as u32;
    }

    /// Rebuilds the provisional tail if it changed. Call once per frame
    /// before displaying the stroke.
    pub fn update_preview(&mut self, sink: &mut impl DabSink) {
        if self.tail_dirty && !self.tail.is_empty() {
            self.emit_tail(sink);
        }
        self.tail_dirty = false;
    }

    pub fn finish(&mut self, sink: &mut impl DabSink) {
        if !self.active {
            return;
        }
        let step = self.flatten_step();
        self.buf_jitter.clear();
        self.jitter.finish(&mut self.buf_jitter);
        self.buf_path.clear();
        for q in &self.buf_jitter {
            self.last_filtered_input = Some(*q);
            let s = self.stabilizer.push(*q, self.scale);
            self.interpolator.push(s, step, &mut self.buf_path);
        }
        if let Some(last) = self.last_filtered_input {
            self.buf_stable.clear();
            catch_up(&mut self.stabilizer, last, self.scale, &mut self.buf_stable);
            for s in &self.buf_stable {
                self.interpolator.push(*s, step, &mut self.buf_path);
            }
        }
        self.interpolator.finish(step, &mut self.buf_path);
        self.consume_path(sink);
        // Catch-up points carry synthetic times; deposit only up to the last
        // real sample.
        if let Some(last) = self.last_filtered_input {
            self.emit_airbrush(last.time, sink);
        }
        if !self.tail.is_empty() {
            self.emit_tail(sink);
            sink.commit_tail();
        }
        self.active = false;
    }

    /// Appends freshly interpolated points to the path, committing whatever
    /// the end taper can no longer reach.
    fn consume_path(&mut self, sink: &mut impl DabSink) {
        let taper_len = self.preset.taper.end.max(0.0);
        for i in 0..self.buf_path.len() {
            let p = self.buf_path[i];
            self.stats.path_points += 1;
            if let Some(prev) = self.last_path_point {
                self.head_distance += (p.pos - prev.pos).length();
            }
            self.last_path_point = Some(p);
            if self.preset.airbrush.enabled {
                self.airbrush.record(p, self.head_distance);
            }
            if taper_len <= 0.0 {
                self.commit_point(p, sink);
            } else {
                self.tail.push_back((p, self.head_distance));
                self.tail_dirty = true;
            }
        }
        while let Some(&(p, d)) = self.tail.front() {
            if d >= self.head_distance - taper_len {
                break;
            }
            self.tail.pop_front();
            self.commit_point(p, sink);
        }
    }

    fn commit_point(&mut self, p: StrokePoint, sink: &mut impl DabSink) {
        let head = self.head_distance;
        self.buf_dabs.clear();
        self.buf_sites.clear();
        let Self {
            preset,
            scale,
            rng,
            buf_dabs,
            ..
        } = self;
        self.spacer.push(
            p,
            |site| place(preset, *scale, rng, site, head, buf_dabs),
            &mut self.buf_sites,
        );
        sink.dabs(&self.buf_dabs);
        self.stats.committed_dabs += self.buf_dabs.len() as u32;
    }

    fn emit_tail(&mut self, sink: &mut impl DabSink) {
        let head = self.head_distance;
        let mut spacer = self.spacer;
        self.buf_dabs.clear();
        self.buf_sites.clear();
        let Self {
            preset,
            scale,
            rng,
            buf_dabs,
            tail,
            buf_sites,
            ..
        } = self;
        for (p, _) in tail.iter() {
            spacer.push(
                *p,
                |site| place(preset, *scale, rng, site, head, buf_dabs),
                buf_sites,
            );
        }
        sink.begin_tail();
        sink.tail_dabs(&self.buf_dabs);
        self.stats.tail_dabs = self.buf_dabs.len() as u32;
    }
}

/// Dynamics inputs at a path point.
fn dynamics_at(
    p: &StrokePoint,
    distance: f32,
    index: u64,
    scale: f32,
    rng: &StrokeRng,
) -> DynamicsInput {
    DynamicsInput {
        caps: p.caps,
        pressure: p.pressure,
        speed: p.velocity.length() * scale,
        tilt_x: p.tilt.x,
        tilt_y: p.tilt.y,
        rotation: p.rotation,
        direction: p.velocity.y.atan2(p.velocity.x),
        distance,
        dab_index: index,
        rng: *rng,
    }
}

/// Resolves the dab at `site` (plus its scattered copies) into `out` and
/// returns the distance to the next site.
fn place(
    preset: &BrushPreset,
    scale: f32,
    rng: &StrokeRng,
    site: &DabSite,
    head: f32,
    out: &mut Vec<Dab>,
) -> f32 {
    let p = &site.point;
    let input = dynamics_at(p, site.distance, site.index as u64, scale, rng);
    let to_end = if preset.taper.end > 0.0 {
        (head - site.distance).max(0.0)
    } else {
        f32::INFINITY
    };
    let taper = preset.taper.factor(site.distance, to_end);
    let mut dab = resolve_dab(preset, &input, p.pos, taper);
    let dir = p.velocity.try_normalize().unwrap_or(Vec2::X);
    // Spacing rule: the next dab is `spacing` times the unscattered dab's
    // extent along the direction of travel (its diameter for round tips).
    let step = preset.properties.eval(Param::Spacing, &input) * dab.extent_along(dir);
    if preset.airbrush.enabled {
        // Airbrush paint is proportional to dwell time: a movement dab
        // stands for the `step / speed` seconds the pen spent around it,
        // capped at one emission period (resting pens are handled by the
        // time-based emitter).
        let speed = p.velocity.length();
        let periods = if speed > 0.0 {
            (step / speed * preset.airbrush.rate).min(1.0)
        } else {
            1.0
        };
        dab.flow = 1.0 - (1.0 - dab.flow).powf(periods);
    }
    let scatter = preset.properties.eval(Param::Scatter, &input);
    preset
        .scatter
        .apply(&dab, scatter, dir, rng, site.index as u64, out);
    step
}

/// The dab a preset produces for given dynamics inputs at `center`.
pub fn resolve_dab(preset: &BrushPreset, input: &DynamicsInput, center: Vec2, taper: f32) -> Dab {
    let props = &preset.properties;
    let angle = props
        .angle
        .eval_angle(input, slot::ANGLE, preset.tip.symmetry_period());
    Dab {
        center,
        radius: preset.radius(input, taper),
        aspect: props.eval(Param::Aspect, input),
        angle: angle.to_radians(),
        opacity: preset.opacity(input, taper),
        flow: props.eval(Param::Flow, input),
        hardness: props.eval(Param::Hardness, input),
        texture: if preset.texture.is_some() {
            props.eval(Param::TextureStrength, input)
        } else {
            0.0
        },
    }
}

/// Drives one stroke at a time into a [`StrokeBuffer`].
#[derive(Debug, Default)]
pub struct BrushEngine {
    pub preset: BrushPreset,
    /// The user's stroke settings; the preset may override them.
    pub stroke_defaults: StrokeSettings,
    /// Global pressure calibration applied before brush effectors.
    pub pressure_curve: Curve,
    pub color: Rgba,
    processor: StrokeProcessor,
    stroke: Option<StrokeBuffer>,
}

impl BrushEngine {
    pub fn new(preset: BrushPreset) -> Self {
        Self {
            preset,
            color: Rgba::BLACK,
            ..Default::default()
        }
    }

    pub fn is_stroking(&self) -> bool {
        self.stroke.is_some()
    }

    /// The stroke in progress, for previewing.
    pub fn stroke(&self) -> Option<&StrokeBuffer> {
        self.stroke.as_ref()
    }

    pub fn stroke_mut(&mut self) -> Option<&mut StrokeBuffer> {
        self.stroke.as_mut()
    }

    pub fn stats(&self) -> StrokeStats {
        self.processor.stats()
    }

    /// The stroke settings the current preset resolves to.
    pub fn resolved_stroke(&self) -> StrokeSettings {
        self.preset.stroke.resolve(&self.stroke_defaults)
    }

    /// `scale`: screen pixels per canvas pixel (the view zoom). `seed`
    /// makes the stroke reproducible.
    pub fn begin_stroke(&mut self, input: BrushInput, scale: f32, seed: u64) {
        let ctx = StampContext {
            tip: self.preset.tip.prepare(),
            texture: self.preset.texture.as_ref().map(CanvasTexture::new),
            anti_aliasing: self.preset.compositing.anti_aliasing,
            mode: self.preset.compositing.mode,
            parallel: true,
        };
        let mut stroke = StrokeBuffer::new(self.color, ctx);
        let params = StrokeParams::new(&self.preset, &self.stroke_defaults, scale, seed);
        self.processor.begin_with(
            &self.preset,
            &self.pressure_curve,
            &params,
            input,
            &mut stroke,
        );
        self.stroke = Some(stroke);
    }

    pub fn add_input(&mut self, input: BrushInput) {
        if let Some(stroke) = self.stroke.as_mut() {
            self.processor.push(input, stroke);
        }
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.processor.set_scale(scale);
    }

    /// Lets airbrush presets keep depositing while the pen rests; call once
    /// per frame during a stroke with the input clock's current time.
    pub fn advance_time(&mut self, now: f64) {
        if let Some(stroke) = self.stroke.as_mut() {
            self.processor.advance_time(now, stroke);
        }
    }

    /// Whether the current preset deposits over time.
    pub fn is_time_based(&self) -> bool {
        self.preset.airbrush.enabled
    }

    /// Rebuilds the provisional tail; call once per frame before display.
    pub fn update_preview(&mut self) {
        if let Some(stroke) = self.stroke.as_mut() {
            self.processor.update_preview(stroke);
        }
    }

    /// Ends the stroke and hands back its buffer for committing.
    pub fn end_stroke(&mut self) -> Option<StrokeBuffer> {
        let mut stroke = self.stroke.take()?;
        self.processor.finish(&mut stroke);
        Some(stroke)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DabLog;
    use crate::interpolate::InterpolationKind;

    fn at(x: f32, y: f32, t: f64) -> BrushInput {
        BrushInput::from_position(Vec2::new(x, y), t)
    }

    #[test]
    fn stroke_lifecycle() {
        let mut e = BrushEngine::new(BrushPreset::default());
        assert!(!e.is_stroking());
        e.add_input(at(0.0, 0.0, 0.0));
        assert!(e.stroke().is_none(), "input outside a stroke is ignored");

        e.begin_stroke(at(10.0, 10.0, 0.0), 1.0, 1);
        e.add_input(at(300.0, 10.0, 0.05));
        let s = e.end_stroke().expect("stroke");
        assert_eq!(s.touched().len(), 2);
        assert!(s.alpha(150, 10) > 0.9);
        assert!(!e.is_stroking());
    }

    #[test]
    fn stroke_ends_on_the_last_sample_even_with_heavy_stabilization() {
        let mut preset = BrushPreset::plain(4.0);
        preset.stroke.stabilization = Some(100.0);
        let mut p = StrokeProcessor::new();
        let mut log = DabLog::default();
        p.begin(&preset, &Curve::linear(), 1.0, at(0.0, 0.0, 0.0), &mut log);
        for i in 1..=100 {
            p.push(at(i as f32 * 4.0, 0.0, i as f64 * 0.004), &mut log);
        }
        p.finish(&mut log);
        let last = log.all().last().unwrap().center;
        assert!((last - Vec2::new(400.0, 0.0)).length() < 0.4, "{last}");
    }

    #[test]
    fn end_taper_preview_matches_final_result() {
        let mut preset = BrushPreset::plain(10.0);
        preset.taper.end = 40.0;
        preset.stroke.interpolation = Some(InterpolationKind::Linear);
        let mut p = StrokeProcessor::new();
        let mut log = DabLog::default();
        p.begin(&preset, &Curve::linear(), 1.0, at(0.0, 0.0, 0.0), &mut log);
        for i in 1..=50 {
            p.push(at(i as f32 * 4.0, 0.0, i as f64 * 0.01), &mut log);
        }
        p.update_preview(&mut log);
        let preview: Vec<Dab> = log.all().copied().collect();
        p.finish(&mut log);
        let final_: Vec<Dab> = log.all().copied().collect();
        assert_eq!(preview.len(), final_.len());
        for (a, b) in preview.iter().zip(&final_) {
            assert!((a.center - b.center).length() < 1e-3 && (a.radius - b.radius).abs() < 1e-3);
        }
        // Tip is thin, body is full width.
        assert!(final_.last().unwrap().radius < 1.0);
        assert!((final_[final_.len() / 2].radius - 5.0).abs() < 1e-3);
    }

    #[test]
    fn tail_moves_forward_as_the_stroke_grows() {
        let mut preset = BrushPreset::plain(10.0);
        preset.taper.end = 30.0;
        let mut p = StrokeProcessor::new();
        let mut log = DabLog::default();
        p.begin(&preset, &Curve::linear(), 1.0, at(0.0, 0.0, 0.0), &mut log);
        for i in 1..=100 {
            p.push(at(i as f32 * 3.0, 0.0, i as f64 * 0.01), &mut log);
            p.update_preview(&mut log);
            // Everything committed lies at least `end` behind the head.
            if let Some(c) = log.committed.last() {
                assert!(c.center.x <= i as f32 * 3.0 - 30.0 + 1e-3);
                assert!(
                    (c.radius - 5.0).abs() < 1e-4,
                    "committed dabs are untapered"
                );
            }
        }
        assert!(p.stats().committed_dabs > 0 && p.stats().tail_dabs > 0);
    }

    #[test]
    fn seed_controls_randomness() {
        let mut preset = BrushPreset::plain(10.0);
        preset.properties.scatter = crate::dynamics::BrushProperty::constant(0.5);
        let run = |seed| {
            let mut p = StrokeProcessor::new();
            let mut log = DabLog::default();
            let params = StrokeParams::new(&preset, &StrokeSettings::default(), 1.0, seed);
            p.begin_with(
                &preset,
                &Curve::linear(),
                &params,
                at(0.0, 0.0, 0.0),
                &mut log,
            );
            for i in 1..=40 {
                p.push(at(i as f32 * 5.0, 0.0, i as f64 * 0.01), &mut log);
            }
            p.finish(&mut log);
            log.all().copied().collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }
}
