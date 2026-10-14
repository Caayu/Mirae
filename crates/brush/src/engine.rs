use std::collections::VecDeque;

use crate::dynamics::{Curve, DynamicsInput};
use crate::interpolate::{Interpolator, PathInterpolator};
use crate::pipeline::{JitterFilter, StrokePoint, VelocityEstimator};
use crate::stabilizer::{Stabilizer, StrokeStabilizer, catch_up, stabilizer};
use crate::{BrushInput, BrushSettings, Dab, DabSink, DabSite, DabSpacer, StrokeBuffer};

/// Counters for one stroke, for instrumentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StrokeStats {
    pub samples: u32,
    pub path_points: u32,
    pub committed_dabs: u32,
    /// Dabs in the most recent tail rebuild.
    pub tail_dabs: u32,
}

/// Runs one stroke through every stage after input normalization:
///
/// ```text
/// pressure curve -> velocity -> jitter filter -> stabilizer -> interpolator
///   -> dab spacing -> dynamics & taper -> DabSink
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
    settings: BrushSettings,
    pressure_curve: Curve,
    /// Screen pixels per canvas pixel.
    scale: f32,
    velocity: VelocityEstimator,
    jitter: JitterFilter,
    stabilizer: Stabilizer,
    interpolator: Interpolator,
    spacer: DabSpacer,
    /// Path points not yet committed, with their arc length from the start.
    tail: VecDeque<(StrokePoint, f32)>,
    tail_dirty: bool,
    head_distance: f32,
    last_path_point: Option<StrokePoint>,
    /// Last sample that entered the stabilizer, target of the final catch-up.
    last_filtered_input: Option<StrokePoint>,
    seed: u64,
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

fn hash01(seed: u64, index: u32) -> f32 {
    let mut z = seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

impl StrokeProcessor {
    pub fn new() -> Self {
        let settings = BrushSettings::default();
        Self {
            stabilizer: stabilizer(settings.stabilizer, settings.stabilization),
            interpolator: Interpolator::new(settings.interpolation),
            settings,
            pressure_curve: Curve::linear(),
            scale: 1.0,
            velocity: VelocityEstimator::default(),
            jitter: JitterFilter::default(),
            spacer: DabSpacer::new(),
            tail: VecDeque::with_capacity(1024),
            tail_dirty: false,
            head_distance: 0.0,
            last_path_point: None,
            last_filtered_input: None,
            seed: 0,
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

    pub fn settings(&self) -> &BrushSettings {
        &self.settings
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

    pub fn begin(
        &mut self,
        settings: &BrushSettings,
        pressure_curve: &Curve,
        scale: f32,
        first: BrushInput,
        sink: &mut impl DabSink,
    ) {
        self.settings.clone_from(settings);
        self.pressure_curve.clone_from(pressure_curve);
        self.set_scale(scale);
        self.velocity.reset();
        self.jitter = JitterFilter::new(settings.jitter_threshold);
        self.stabilizer = stabilizer(settings.stabilizer, settings.stabilization);
        self.interpolator = Interpolator::new(settings.interpolation);
        self.spacer.reset();
        self.tail.clear();
        self.tail_dirty = false;
        self.head_distance = 0.0;
        self.last_path_point = None;
        self.last_filtered_input = None;
        self.seed = first.timestamp.to_bits() ^ 0xA5A5_5A5A_DEAD_BEEF;
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
        if !self.tail.is_empty() {
            self.emit_tail(sink);
            sink.commit_tail();
        }
        self.active = false;
    }

    /// Appends freshly interpolated points to the path, committing whatever
    /// the end taper can no longer reach.
    fn consume_path(&mut self, sink: &mut impl DabSink) {
        let taper_len = self.settings.taper.end.max(0.0);
        for i in 0..self.buf_path.len() {
            let p = self.buf_path[i];
            self.stats.path_points += 1;
            if let Some(prev) = self.last_path_point {
                self.head_distance += (p.pos - prev.pos).length();
            }
            self.last_path_point = Some(p);
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
            settings,
            scale,
            seed,
            buf_dabs,
            ..
        } = self;
        self.spacer.push(
            p,
            |site| place(settings, *scale, *seed, site, head, buf_dabs),
            &mut self.buf_sites,
        );
        for d in &self.buf_dabs {
            sink.dab(d);
        }
        self.stats.committed_dabs += self.buf_dabs.len() as u32;
    }

    fn emit_tail(&mut self, sink: &mut impl DabSink) {
        let head = self.head_distance;
        let mut spacer = self.spacer;
        self.buf_dabs.clear();
        self.buf_sites.clear();
        let Self {
            settings,
            scale,
            seed,
            buf_dabs,
            tail,
            buf_sites,
            ..
        } = self;
        for (p, _) in tail.iter() {
            spacer.push(
                *p,
                |site| place(settings, *scale, *seed, site, head, buf_dabs),
                buf_sites,
            );
        }
        sink.begin_tail();
        for d in &self.buf_dabs {
            sink.tail_dab(d);
        }
        self.stats.tail_dabs = self.buf_dabs.len() as u32;
    }
}

/// Resolves the dab at `site` and returns the distance to the next one.
fn place(
    settings: &BrushSettings,
    scale: f32,
    seed: u64,
    site: &DabSite,
    head: f32,
    out: &mut Vec<Dab>,
) -> f32 {
    let p = &site.point;
    let input = DynamicsInput {
        caps: p.caps,
        pressure: p.pressure,
        speed: p.velocity.length() * scale,
        tilt_x: p.tilt.x,
        tilt_y: p.tilt.y,
        rotation: p.rotation,
        direction: p.velocity.y.atan2(p.velocity.x),
        distance: site.distance,
        random: hash01(seed, site.index),
    };
    let to_end = if settings.taper.end > 0.0 {
        (head - site.distance).max(0.0)
    } else {
        f32::INFINITY
    };
    let taper = settings.taper.factor(site.distance, to_end);
    let dab = Dab {
        center: p.pos,
        radius: settings.radius(&input, taper),
        opacity: settings.opacity(&input, taper),
        flow: settings.flow.eval(&input).clamp(0.0, 1.0),
        hardness: settings.hardness.eval(&input).clamp(0.0, 1.0),
    };
    out.push(dab);
    2.0 * dab.radius * settings.spacing
}

/// Drives one stroke at a time into a [`StrokeBuffer`].
#[derive(Debug, Default)]
pub struct BrushEngine {
    pub settings: BrushSettings,
    /// Global pressure calibration applied before brush effectors.
    pub pressure_curve: Curve,
    processor: StrokeProcessor,
    stroke: Option<StrokeBuffer>,
}

impl BrushEngine {
    pub fn new(settings: BrushSettings) -> Self {
        Self {
            settings,
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

    /// `scale`: screen pixels per canvas pixel (the view zoom).
    pub fn begin_stroke(&mut self, input: BrushInput, scale: f32) {
        let mut stroke = StrokeBuffer::new(self.settings.color, self.settings.anti_aliasing);
        self.processor.begin(
            &self.settings,
            &self.pressure_curve,
            scale,
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
    use glam::Vec2;

    use super::*;
    use crate::DabLog;

    fn at(x: f32, y: f32, t: f64) -> BrushInput {
        BrushInput::from_position(Vec2::new(x, y), t)
    }

    #[test]
    fn stroke_lifecycle() {
        let mut e = BrushEngine::new(BrushSettings::default());
        assert!(!e.is_stroking());
        e.add_input(at(0.0, 0.0, 0.0));
        assert!(e.stroke().is_none(), "input outside a stroke is ignored");

        e.begin_stroke(at(10.0, 10.0, 0.0), 1.0);
        e.add_input(at(300.0, 10.0, 0.05));
        let s = e.end_stroke().expect("stroke");
        assert_eq!(s.touched().len(), 2);
        assert!(s.alpha(150, 10) > 0.9);
        assert!(!e.is_stroking());
    }

    #[test]
    fn stroke_ends_on_the_last_sample_even_with_heavy_stabilization() {
        let settings = BrushSettings {
            stabilization: 100.0,
            ..BrushSettings::plain(4.0)
        };
        let mut p = StrokeProcessor::new();
        let mut log = DabLog::default();
        p.begin(
            &settings,
            &Curve::linear(),
            1.0,
            at(0.0, 0.0, 0.0),
            &mut log,
        );
        for i in 1..=100 {
            p.push(at(i as f32 * 4.0, 0.0, i as f64 * 0.004), &mut log);
        }
        p.finish(&mut log);
        let last = log.all().last().unwrap().center;
        assert!((last - Vec2::new(400.0, 0.0)).length() < 0.4, "{last}");
    }

    #[test]
    fn end_taper_preview_matches_final_result() {
        let mut settings = BrushSettings::plain(10.0);
        settings.taper.end = 40.0;
        settings.interpolation = crate::interpolate::InterpolationKind::Linear;
        let mut p = StrokeProcessor::new();
        let mut log = DabLog::default();
        p.begin(
            &settings,
            &Curve::linear(),
            1.0,
            at(0.0, 0.0, 0.0),
            &mut log,
        );
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
        let mut settings = BrushSettings::plain(10.0);
        settings.taper.end = 30.0;
        let mut p = StrokeProcessor::new();
        let mut log = DabLog::default();
        p.begin(
            &settings,
            &Curve::linear(),
            1.0,
            at(0.0, 0.0, 0.0),
            &mut log,
        );
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
    fn random_source_is_reproducible_per_dab() {
        assert_eq!(hash01(7, 3), hash01(7, 3));
        assert_ne!(hash01(7, 3), hash01(7, 4));
        let v: Vec<f32> = (0..1000).map(|i| hash01(99, i)).collect();
        assert!(v.iter().all(|x| (0.0..1.0).contains(x)));
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        assert!((mean - 0.5).abs() < 0.05);
    }
}
