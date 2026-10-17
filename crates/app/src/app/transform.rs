//! Transform sessions in the app (phase 5D): starting one (Ctrl+T, the
//! panel, a flip, the Move tool), driving it with the pointer and the
//! panel, its preview, the commit (one history step) and the cancel
//! (nothing), and what happens when something else is asked for during
//! one. Behaviour measured in the reference (docs/parity/5D-transform.md):
//! Enter commits, Escape cancels, undo and redo walk the session's own
//! edits (T13); choosing another layer or tool asks "Apply the
//! transform?" (T41, T42); file commands wait (T43).

use glam::Vec2;

use tela_document::LayerError;
use tela_ui::{ApplyChoice, PanelAction, TransformAction, TransformBox, egui};
use winit::window::CursorIcon;

use super::{App, Drag};
use crate::tools::CanvasTool;
use crate::transforming::{Handle, TransformSession};

/// What waits for the answer to "Apply the transform?".
#[derive(Clone, Debug)]
pub(super) enum AfterTransform {
    /// A panel request (another layer, a tool, a layer or selection
    /// command).
    Panel(PanelAction),
    /// Closing the window.
    Close,
}

impl App {
    /// Opens a transform session on the active layer (Ctrl+T). Refused
    /// silently on hidden and locked layers (T34, T35); "nothing to
    /// transform" is told (T36). Returns whether a session is open.
    pub(super) fn begin_transform(&mut self) -> bool {
        if self.transform.is_some() {
            return true;
        }
        if self.engine.is_stroking() || self.drag != Drag::None {
            return false;
        }
        match self.document.transform_source(self.document.active_layer()) {
            Ok(source) => {
                self.transform = Some(TransformSession::new(source, self.transform_tool));
                true
            }
            Err(LayerError::NothingToTransform(_)) => {
                self.file.notice = Some("There is nothing to transform.".into());
                false
            }
            Err(e) => {
                log::info!(target: "tela::transform", "not transforming: {e}");
                false
            }
        }
    }

    /// Commits the open session as one history step (T4, T14).
    pub(super) fn commit_transform(&mut self) {
        let Some(session) = self.transform.take() else {
            return;
        };
        let t = session.transform(&self.transform_tool);
        self.stale.extend(&session.source.affected_tiles(&t.params));
        match self.document.apply_transform(&session.source, &t) {
            Ok(dirty) => self.stale.extend(&dirty),
            Err(e) => {
                log::warn!(target: "tela::transform", "transform discarded: {e}");
                self.file.notice = Some(format!("The transform could not be applied: {e}."));
            }
        }
        self.drag = Drag::None;
        self.request_redraw();
    }

    /// Discards the open session: the document never changed (TR2); the
    /// tool settings changed during it return (T5).
    pub(super) fn cancel_transform(&mut self) {
        let Some(session) = self.transform.take() else {
            return;
        };
        self.stale
            .extend(&session.source.affected_tiles(&session.params()));
        self.transform_tool = session.settings;
        self.drag = Drag::None;
        self.request_redraw();
    }

    /// Runs `edit` on the session and redraws what it changed.
    fn edit_transform(&mut self, edit: impl FnOnce(&mut TransformSession) -> bool) {
        let Some(s) = self.transform.as_mut() else {
            return;
        };
        let before = s.source.affected_tiles(&s.params());
        if edit(s) {
            self.stale.extend(&before);
            self.stale.extend(&s.source.affected_tiles(&s.params()));
            self.request_redraw();
        }
    }

    /// Undo or redo inside the session (T13).
    pub(super) fn transform_history(&mut self, redo: bool) {
        self.edit_transform(|s| if redo { s.redo() } else { s.undo() });
    }

    /// A press on the canvas while a session is open or the Move tool is
    /// active. Returns whether it was taken.
    pub(super) fn transform_press(&mut self, at: Vec2) -> bool {
        if self.transform.is_none() {
            if self.tool != CanvasTool::Move || !self.begin_transform() {
                return false;
            }
            // The Move tool: one drag, committed on release.
            if let Some(s) = &mut self.transform {
                s.ephemeral = true;
            }
        }
        let view = self.view;
        let Some(s) = self.transform.as_mut() else {
            return false;
        };
        let handle = if s.ephemeral {
            Handle::Inside
        } else {
            s.hit(&view, at)
        };
        s.begin_drag(&view, handle, at);
        self.drag = Drag::Transform;
        true
    }

    pub(super) fn transform_move(&mut self, at: Vec2) {
        let (view, shift, keep) = (
            self.view,
            self.modifiers.shift_key(),
            self.transform_tool.keep_aspect,
        );
        self.edit_transform(|s| s.drag_to(&view, at, shift, keep));
    }

    pub(super) fn transform_release(&mut self, at: Vec2) {
        self.transform_move(at);
        self.drag = Drag::None;
        let ephemeral = self.transform.as_ref().is_some_and(|s| s.ephemeral);
        if let Some(s) = &mut self.transform {
            s.end_drag();
        }
        if ephemeral {
            self.commit_transform();
        }
    }

    /// Requests from the transform panel.
    pub(super) fn apply_transform_action(&mut self, action: TransformAction) {
        let keep = self.transform_tool.keep_aspect;
        match action {
            TransformAction::Start => {
                self.begin_transform();
            }
            TransformAction::MoveTool => {
                if self.transform.is_none() {
                    self.tool = CanvasTool::Move;
                }
            }
            TransformAction::FlipHorizontal | TransformAction::FlipVertical => {
                // The reference opens a session with the flip applied (T15).
                if self.begin_transform() {
                    let h = action == TransformAction::FlipHorizontal;
                    self.edit_transform(|s| s.flip(h));
                }
            }
            TransformAction::Width(w) => self.edit_transform(|s| s.set_width(w, keep)),
            TransformAction::Height(h) => self.edit_transform(|s| s.set_height(h, keep)),
            TransformAction::Angle(a) => self.edit_transform(|s| s.set_angle(a)),
            TransformAction::Reset => self.edit_transform(TransformSession::reset),
            TransformAction::KeepAspect(on) => self.transform_tool.keep_aspect = on,
            TransformAction::Nearest(on) => {
                self.transform_tool.interpolation = if on {
                    tela_document::Interpolation::Nearest
                } else {
                    tela_document::Interpolation::Bicubic
                };
                if let Some(s) = &self.transform {
                    self.stale.extend(&s.source.affected_tiles(&s.params()));
                }
            }
            TransformAction::Commit => self.commit_transform(),
            TransformAction::Cancel => self.cancel_transform(),
        }
    }

    /// Whether `action` must wait for "Apply the transform?" while a
    /// session is open: anything that changes the document, the edit
    /// target or the canvas tool (T41, T42). Colour, view and history
    /// requests do not.
    pub(super) fn ask_before(&self, action: &PanelAction) -> bool {
        self.transform.is_some()
            && matches!(
                action,
                PanelAction::Layer(_)
                    | PanelAction::Selection(_)
                    | PanelAction::Fill(_)
                    | PanelAction::SelectPreset(_)
                    | PanelAction::ToggleEraser
                    | PanelAction::ClearLayer
            )
    }

    /// The answer to "Apply the transform?".
    pub(super) fn answer_apply(&mut self, choice: ApplyChoice) {
        let Some(after) = self.transform_exit.take() else {
            return;
        };
        match choice {
            ApplyChoice::Apply => self.commit_transform(),
            ApplyChoice::Discard => self.cancel_transform(),
            ApplyChoice::Cancel => return,
        }
        match after {
            AfterTransform::Panel(a) => self.apply(a),
            AfterTransform::Close => self.request(super::files::Pending::Close),
        }
    }

    /// The box to draw this frame, in egui points (`ppp` physical pixels
    /// per point).
    pub(super) fn transform_box(&self, ppp: f32) -> Option<TransformBox> {
        let b = self.transform.as_ref()?.screen_box(&self.view)?;
        let pt = |p: Vec2| egui::Pos2::new(p.x / ppp, p.y / ppp);
        Some(TransformBox {
            corners: b.corners.map(pt),
            pivot: pt(b.pivot),
            rotate: pt(b.rotate),
        })
    }

    /// The cursor over the canvas: what a press would grab.
    pub(super) fn transform_cursor(&self) -> Option<CursorIcon> {
        if let Some(s) = &self.transform {
            if s.ephemeral {
                return Some(CursorIcon::Move);
            }
            return Some(match s.hit(&self.view, self.cursor) {
                Handle::Inside => CursorIcon::Move,
                Handle::Corner(_) | Handle::Side(_) => CursorIcon::Crosshair,
                Handle::Pivot => CursorIcon::Cell,
                Handle::Rotate | Handle::Outside => CursorIcon::Grab,
            });
        }
        (self.tool == CanvasTool::Move).then_some(CursorIcon::Move)
    }
}
