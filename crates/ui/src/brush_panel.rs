//! The brush panel: preset selection, parameters, effectors and curves.
//! Functional rather than pretty; it edits `tela-brush` data directly.

use egui::{ComboBox, Slider, Ui};
use tela_brush::dynamics::{BrushProperty, CombineMode, Curve, DynamicsSource, Effector};
use tela_brush::interpolate::InterpolationKind;
use tela_brush::preset::{PaintMode, Param};
use tela_brush::scatter::ScatterMode;
use tela_brush::stabilizer::StabilizerKind;
use tela_brush::texture::{TextureMode, TexturePattern, TextureSettings};
use tela_brush::{BrushPreset, StrokeSettings};

use crate::curve_editor::curve_editor;

/// What the panel edits for one frame.
pub struct PanelModel<'a> {
    pub preset_names: &'a [String],
    pub selected: usize,
    pub eraser_mode: bool,
    pub preset: &'a mut BrushPreset,
    pub stroke: &'a mut StrokeSettings,
    pub pressure_curve: &'a mut Curve,
}

/// Requests the panels cannot fulfil by themselves.
#[derive(Clone, Debug, PartialEq)]
pub enum PanelAction {
    None,
    SelectPreset(usize),
    ToggleEraser,
    ClearLayer,
    /// From the layer panel.
    Layer(crate::LayerAction),
    /// From the undo/redo buttons.
    History(crate::HistoryCommand),
    /// From the document bar.
    Document(crate::DocumentCommand),
    /// The answer to the unsaved-changes prompt.
    Prompt(crate::PromptChoice),
    /// The answer to the merge-down mask question.
    MergeMask(crate::MergeMaskChoice),
    /// A notice was dismissed.
    DismissNotice,
    /// From the colour panel.
    Color(crate::ColorAction),
    /// From the selection panel.
    Selection(crate::SelectionAction),
}

/// Persistent panel state.
#[derive(Debug)]
pub struct BrushPanel {
    edit: Param,
}

impl Default for BrushPanel {
    fn default() -> Self {
        Self { edit: Param::Size }
    }
}

fn source_name(s: DynamicsSource) -> &'static str {
    match s {
        DynamicsSource::Pressure => "pressure",
        DynamicsSource::Speed => "speed",
        DynamicsSource::TiltMagnitude => "tilt",
        DynamicsSource::TiltDirection => "tilt direction",
        DynamicsSource::Rotation => "barrel rotation",
        DynamicsSource::Direction => "stroke direction",
        DynamicsSource::Distance { .. } => "distance",
        DynamicsSource::DabIndex { .. } => "dab index",
        DynamicsSource::Random => "random",
        DynamicsSource::Noise { .. } => "noise",
        DynamicsSource::Unknown => "unknown",
    }
}

const SOURCES: [DynamicsSource; 10] = [
    DynamicsSource::Pressure,
    DynamicsSource::Speed,
    DynamicsSource::TiltMagnitude,
    DynamicsSource::TiltDirection,
    DynamicsSource::Rotation,
    DynamicsSource::Direction,
    DynamicsSource::Distance { length: 200.0 },
    DynamicsSource::DabIndex { period: 8 },
    DynamicsSource::Random,
    DynamicsSource::Noise { frequency: 0.02 },
];

const MODES: [CombineMode; 6] = [
    CombineMode::Multiply,
    CombineMode::Add,
    CombineMode::Replace,
    CombineMode::Min,
    CombineMode::Max,
    CombineMode::Follow,
];

impl BrushPanel {
    /// Draws the panel contents. Returns the action requested, if any.
    pub fn show(&mut self, ui: &mut Ui, m: PanelModel) -> PanelAction {
        let mut action = PanelAction::None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Brush");
            ComboBox::from_id_salt("preset")
                .selected_text(m.preset_names.get(m.selected).map_or("?", |s| s.as_str()))
                .show_ui(ui, |ui| {
                    for (i, name) in m.preset_names.iter().enumerate() {
                        if ui.selectable_label(i == m.selected, name).clicked() {
                            action = PanelAction::SelectPreset(i);
                        }
                    }
                });
            ui.horizontal(|ui| {
                if ui.selectable_label(m.eraser_mode, "Eraser (E)").clicked() {
                    action = PanelAction::ToggleEraser;
                }
                if ui.button("Clear layer").clicked() {
                    action = PanelAction::ClearLayer;
                }
            });
            ui.separator();

            let p = m.preset;
            for param in Param::ALL {
                let (lo, hi) = param.range();
                let prop = param.get_mut(&mut p.properties);
                let slider = Slider::new(&mut prop.base, lo..=hi).text(param.name());
                let slider = if param == Param::Size || param == Param::Spacing {
                    slider.logarithmic(true)
                } else {
                    slider
                };
                ui.add(slider);
            }
            ui.separator();

            // Stroke settings are shown as the preset resolves them; edits go
            // through `edit_resolved`, which owns the override rule.
            ui.label("Stroke");
            let mut stroke = p.stroke.resolve(m.stroke);
            ui.add(Slider::new(&mut stroke.stabilization, 0.0..=100.0).text("stabilization"));
            ComboBox::from_label("stabilizer")
                .selected_text(stroke.stabilizer.name())
                .show_ui(ui, |ui| {
                    for k in StabilizerKind::ALL {
                        ui.selectable_value(&mut stroke.stabilizer, k, k.name());
                    }
                });
            ComboBox::from_label("interpolation")
                .selected_text(stroke.interpolation.name())
                .show_ui(ui, |ui| {
                    for k in InterpolationKind::ALL {
                        ui.selectable_value(&mut stroke.interpolation, k, k.name());
                    }
                });
            p.stroke.edit_resolved(m.stroke, |s| *s = stroke);
            ui.add(Slider::new(&mut p.taper.start, 0.0..=300.0).text("taper start px"));
            ui.add(Slider::new(&mut p.taper.end, 0.0..=300.0).text("taper end px"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut p.airbrush.enabled, "airbrush");
                ui.add_enabled(
                    p.airbrush.enabled,
                    Slider::new(&mut p.airbrush.rate, 1.0..=400.0).text("dabs/s"),
                );
            });
            let mut erase = p.compositing.mode == PaintMode::Erase;
            if ui.checkbox(&mut erase, "erase").changed() {
                p.compositing.mode = if erase {
                    PaintMode::Erase
                } else {
                    PaintMode::Paint
                };
            }
            ui.separator();

            ui.label("Tip, scatter, texture");
            ui.horizontal(|ui| {
                ui.checkbox(&mut p.tip.flip_x, "flip X");
                ui.checkbox(&mut p.tip.flip_y, "flip Y");
            });
            ComboBox::from_label("scatter mode")
                .selected_text(format!("{:?}", p.scatter.mode))
                .show_ui(ui, |ui| {
                    for mode in [ScatterMode::Perpendicular, ScatterMode::Radial] {
                        ui.selectable_value(&mut p.scatter.mode, mode, format!("{mode:?}"));
                    }
                });
            ui.add(Slider::new(&mut p.scatter.count, 1..=16).text("dabs per step"));
            ui.add(
                Slider::new(&mut p.scatter.rotation_jitter, 0.0..=360.0).text("rotation jitter°"),
            );
            ui.add(Slider::new(&mut p.scatter.scale_jitter, 0.0..=1.0).text("scale jitter"));
            let mut textured = p.texture.is_some();
            if ui.checkbox(&mut textured, "paper texture").changed() {
                p.texture = textured.then(TextureSettings::default);
            }
            if let Some(t) = &mut p.texture {
                ComboBox::from_label("pattern")
                    .selected_text(format!("{:?}", t.pattern))
                    .show_ui(ui, |ui| {
                        for pat in [
                            TexturePattern::PaperFine,
                            TexturePattern::PaperRough,
                            TexturePattern::Chalk,
                        ] {
                            ui.selectable_value(&mut t.pattern, pat, format!("{pat:?}"));
                        }
                    });
                ComboBox::from_label("texture mode")
                    .selected_text(format!("{:?}", t.mode))
                    .show_ui(ui, |ui| {
                        for mode in [
                            TextureMode::Multiply,
                            TextureMode::Subtract,
                            TextureMode::Height,
                        ] {
                            ui.selectable_value(&mut t.mode, mode, format!("{mode:?}"));
                        }
                    });
                ui.add(
                    Slider::new(&mut t.scale, 0.25..=8.0)
                        .logarithmic(true)
                        .text("scale"),
                );
                ui.add(Slider::new(&mut t.contrast, 0.0..=4.0).text("contrast"));
                ui.add(Slider::new(&mut t.brightness, -1.0..=1.0).text("brightness"));
            }
            ui.separator();

            ui.label("Pressure curve (device calibration)");
            curve_editor(ui, m.pressure_curve, true, "pressure-curve");
            ui.separator();

            ui.label("Effectors");
            ComboBox::from_label("property")
                .selected_text(self.edit.name())
                .show_ui(ui, |ui| {
                    for param in Param::ALL {
                        ui.selectable_value(&mut self.edit, param, param.name());
                    }
                });
            effectors_editor(ui, self.edit.get_mut(&mut p.properties), self.edit);
        });
        action
    }
}

fn effectors_editor(ui: &mut Ui, prop: &mut BrushProperty, param: Param) {
    let mut remove = None;
    for (i, e) in prop.effectors.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut e.enabled, "");
                ComboBox::from_id_salt("source")
                    .selected_text(source_name(e.source))
                    .show_ui(ui, |ui| {
                        for s in SOURCES {
                            let selected =
                                std::mem::discriminant(&s) == std::mem::discriminant(&e.source);
                            if ui.selectable_label(selected, source_name(s)).clicked() {
                                e.source = s;
                            }
                        }
                    });
                ComboBox::from_id_salt("mode")
                    .selected_text(format!("{:?}", e.mode))
                    .show_ui(ui, |ui| {
                        for mode in MODES {
                            if mode == CombineMode::Follow && param != Param::Angle {
                                continue;
                            }
                            ui.selectable_value(&mut e.mode, mode, format!("{mode:?}"));
                        }
                    });
                if ui.small_button("remove").clicked() {
                    remove = Some(i);
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut e.min).speed(0.01).prefix("min "));
                ui.add(egui::DragValue::new(&mut e.max).speed(0.01).prefix("max "));
            });
            if e.mode != CombineMode::Follow {
                curve_editor(ui, &mut e.curve, false, ("effector", i));
            }
            ui.separator();
        });
    }
    if let Some(i) = remove {
        prop.effectors.remove(i);
    }
    if ui.button("add effector").clicked() {
        prop.effectors
            .push(Effector::new(DynamicsSource::Pressure, Curve::linear()));
    }
}
