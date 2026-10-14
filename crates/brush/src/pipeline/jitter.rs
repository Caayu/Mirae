use super::StrokePoint;

/// Drops sub-threshold movements (sensor noise while the pen rests or crawls)
/// before they reach the stabilizer.
///
/// A sample is forwarded once it is at least `threshold` screen pixels away
/// from the last forwarded one, or its pressure changed noticeably (so a
/// stationary press still updates pressure). The latest withheld sample is
/// flushed by [`JitterFilter::finish`], so the stroke always ends exactly
/// where the pen lifted.
#[derive(Clone, Debug)]
pub struct JitterFilter {
    /// Screen pixels.
    pub threshold: f32,
    pub pressure_threshold: f32,
    last: Option<StrokePoint>,
    pending: Option<StrokePoint>,
}

impl Default for JitterFilter {
    fn default() -> Self {
        Self::new(0.35)
    }
}

impl JitterFilter {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            pressure_threshold: 0.02,
            last: None,
            pending: None,
        }
    }

    pub fn reset(&mut self) {
        self.last = None;
        self.pending = None;
    }

    /// `scale`: screen pixels per canvas pixel.
    pub fn push(&mut self, p: StrokePoint, scale: f32, out: &mut Vec<StrokePoint>) {
        let forward = match &self.last {
            None => true,
            Some(last) => {
                (p.pos - last.pos).length() * scale >= self.threshold
                    || (p.pressure - last.pressure).abs() >= self.pressure_threshold
            }
        };
        if forward {
            self.last = Some(p);
            self.pending = None;
            out.push(p);
        } else {
            self.pending = Some(p);
        }
    }

    pub fn finish(&mut self, out: &mut Vec<StrokePoint>) {
        if let Some(p) = self.pending.take() {
            self.last = Some(p);
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;
    use crate::BrushInput;

    fn point(x: f32, y: f32, pressure: f32) -> StrokePoint {
        BrushInput::from_position(Vec2::new(x, y), 0.0)
            .with_pressure(pressure)
            .into()
    }

    #[test]
    fn small_moves_are_withheld_until_finish() {
        let mut f = JitterFilter::new(1.0);
        let mut out = vec![];
        f.push(point(0.0, 0.0, 0.5), 1.0, &mut out);
        f.push(point(0.3, 0.0, 0.5), 1.0, &mut out);
        f.push(point(0.0, 0.4, 0.5), 1.0, &mut out);
        assert_eq!(out.len(), 1);
        f.finish(&mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].pos, Vec2::new(0.0, 0.4));
    }

    #[test]
    fn threshold_is_in_screen_pixels() {
        let mut f = JitterFilter::new(1.0);
        let mut out = vec![];
        f.push(point(0.0, 0.0, 0.5), 4.0, &mut out);
        // 0.3 canvas px at 4x zoom is 1.2 screen px.
        f.push(point(0.3, 0.0, 0.5), 4.0, &mut out);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn pressure_change_passes_without_motion() {
        let mut f = JitterFilter::new(1.0);
        let mut out = vec![];
        f.push(point(0.0, 0.0, 0.2), 1.0, &mut out);
        f.push(point(0.0, 0.0, 0.6), 1.0, &mut out);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn finish_without_pending_is_noop() {
        let mut f = JitterFilter::default();
        let mut out = vec![];
        f.push(point(0.0, 0.0, 0.5), 1.0, &mut out);
        f.push(point(50.0, 0.0, 0.5), 1.0, &mut out);
        f.finish(&mut out);
        assert_eq!(out.len(), 2);
    }
}
