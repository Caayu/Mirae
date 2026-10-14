//! Which preset a stroke uses: the selected brush, or the eraser when the
//! stylus is flipped to its eraser end (or eraser mode is toggled).

use tela_brush::BrushPreset;
use tela_brush::preset::builtin::ids;
use tela_brush::preset::{PaintMode, builtin_presets};
use tela_tablet::DeviceKind;

pub struct Tools {
    pub presets: Vec<BrushPreset>,
    pub brush: usize,
    pub eraser: usize,
    /// Eraser for devices without an eraser end (mouse, plain pens).
    pub eraser_mode: bool,
}

impl Tools {
    pub fn new() -> Self {
        let mut presets = builtin_presets();
        if presets.is_empty() {
            presets.push(BrushPreset::default());
        }
        let find = |id: &str| presets.iter().position(|p| p.id == id);
        let brush = find(ids::INK_PEN).unwrap_or(0);
        let eraser = presets
            .iter()
            .position(|p| p.compositing.mode == PaintMode::Erase)
            .unwrap_or_else(|| {
                let mut e = BrushPreset::plain(30.0);
                e.id = ids::ERASER.into();
                e.name = "Eraser".into();
                e.compositing.mode = PaintMode::Erase;
                presets.push(e);
                presets.len() - 1
            });
        Self {
            presets,
            brush,
            eraser,
            eraser_mode: false,
        }
    }

    fn erasing(&self, device: DeviceKind) -> bool {
        self.eraser_mode || device == DeviceKind::Eraser
    }

    /// The preset a stroke started by `device` uses.
    pub fn for_stroke(&self, device: DeviceKind) -> &BrushPreset {
        &self.presets[if self.erasing(device) {
            self.eraser
        } else {
            self.brush
        }]
    }

    /// The preset the user is currently editing.
    pub fn active(&self) -> &BrushPreset {
        &self.presets[if self.eraser_mode {
            self.eraser
        } else {
            self.brush
        }]
    }

    pub fn active_mut(&mut self) -> &mut BrushPreset {
        let i = if self.eraser_mode {
            self.eraser
        } else {
            self.brush
        };
        &mut self.presets[i]
    }

    /// Selects preset `i` as the brush (or as the eraser, if it erases).
    pub fn select(&mut self, i: usize) {
        if i >= self.presets.len() {
            return;
        }
        if self.presets[i].compositing.mode == PaintMode::Erase {
            self.eraser = i;
            self.eraser_mode = true;
        } else {
            self.brush = i;
            self.eraser_mode = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eraser_end_selects_the_eraser_preset() {
        let t = Tools::new();
        assert_eq!(
            t.for_stroke(DeviceKind::Pen).compositing.mode,
            PaintMode::Paint
        );
        assert_eq!(
            t.for_stroke(DeviceKind::Eraser).compositing.mode,
            PaintMode::Erase
        );
        assert_eq!(t.for_stroke(DeviceKind::Mouse).id, ids::INK_PEN);
    }

    #[test]
    fn eraser_mode_and_selection() {
        let mut t = Tools::new();
        t.eraser_mode = true;
        assert_eq!(
            t.for_stroke(DeviceKind::Mouse).compositing.mode,
            PaintMode::Erase
        );
        let pencil = t.presets.iter().position(|p| p.id == ids::PENCIL).unwrap();
        t.select(pencil);
        assert!(!t.eraser_mode);
        assert_eq!(t.active().id, ids::PENCIL);
    }
}
