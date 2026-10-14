use crate::{BrushInput, BrushSettings, Dab, DabSpacer, StrokeBuffer};

/// Drives one stroke at a time through the pipeline.
#[derive(Debug, Default)]
pub struct BrushEngine {
    pub settings: BrushSettings,
    spacer: DabSpacer,
    stroke: Option<StrokeBuffer>,
    dabs: Vec<Dab>,
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

    pub fn begin_stroke(&mut self, input: BrushInput) {
        self.spacer.reset();
        self.stroke = Some(StrokeBuffer::new(&self.settings));
        self.add_input(input);
    }

    pub fn add_input(&mut self, input: BrushInput) {
        let Some(stroke) = self.stroke.as_mut() else {
            return;
        };
        self.dabs.clear();
        self.spacer.push(input, &self.settings, &mut self.dabs);
        for dab in &self.dabs {
            stroke.stamp(dab);
        }
    }

    /// Ends the stroke and hands back its buffer for committing.
    pub fn end_stroke(&mut self) -> Option<StrokeBuffer> {
        self.stroke.take()
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;

    #[test]
    fn stroke_lifecycle() {
        let mut e = BrushEngine::new(BrushSettings::default());
        assert!(!e.is_stroking());
        e.add_input(BrushInput::from_position(Vec2::ZERO, 0.0));
        assert!(e.stroke().is_none(), "input outside a stroke is ignored");

        e.begin_stroke(BrushInput::from_position(Vec2::new(10.0, 10.0), 0.0));
        e.add_input(BrushInput::from_position(Vec2::new(300.0, 10.0), 0.01));
        let s = e.end_stroke().expect("stroke");
        assert_eq!(s.touched().len(), 2);
        assert!(s.mask().pixel(150, 10) > 0.9);
        assert!(!e.is_stroking());
    }
}
