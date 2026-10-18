//! What the undo/redo buttons of the top bar show and return
//! ([`crate::toolbar::top_bar`]). The bar knows only what the app tells it:
//! a label for the step each button would revert or re-apply, or `None`
//! when there is none (the button is then disabled).

/// What the history buttons show this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct HistoryButtons<'a> {
    /// The step Undo would revert, e.g. "Paint".
    pub undo: Option<&'a str>,
    /// The step Redo would re-apply.
    pub redo: Option<&'a str>,
}

/// Which button was pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryCommand {
    Undo,
    Redo,
}
