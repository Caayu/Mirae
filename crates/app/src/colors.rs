//! Colour tool state: the main and sub colours, which one paints, the
//! recent colours and the swatches (phase 5A, ADR 0021).
//!
//! Tool state of the session, not of the document: nothing here is saved
//! in `.mirae` files or recorded in the undo history (the reference does
//! not record colour changes either; parity record 5A, O8). A stroke
//! copies the active colour when it begins (`BrushEngine::color`).
//!
//! Semantics follow the reference where it defines them (O1–O3, O9):
//! two slots, one of them active (the one that paints); `X` makes the
//! other one active without changing either colour; the recent colours
//! list the colours strokes used and the eyedropper picked, newest first,
//! each once.

use tela_color::Color;

/// Which of the two colours paints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Slot {
    /// The main colour (black by default).
    #[default]
    Main,
    /// The sub colour (white by default).
    Sub,
}

/// Most recent colours kept (Mirae's choice; the reference's capacity
/// was not measured).
pub const RECENT_MAX: usize = 24;
/// Most swatches kept.
pub const SWATCHES_MAX: usize = 64;

/// The built-in swatches: greys, primaries, secondaries and a few
/// in-between hues of our own choosing (not the reference's set).
const DEFAULT_SWATCHES: [[u8; 3]; 16] = [
    [0, 0, 0],
    [64, 64, 64],
    [128, 128, 128],
    [192, 192, 192],
    [255, 255, 255],
    [255, 0, 0],
    [255, 128, 0],
    [255, 255, 0],
    [0, 192, 0],
    [0, 255, 255],
    [0, 96, 255],
    [0, 0, 255],
    [160, 0, 255],
    [255, 0, 255],
    [128, 64, 32],
    [255, 200, 160],
];

/// The colour tool state.
#[derive(Clone, Debug)]
pub struct ColorState {
    main: Color,
    sub: Color,
    active: Slot,
    recent: Vec<Color>,
    swatches: Vec<Color>,
}

impl Default for ColorState {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorState {
    /// Black main colour (active), white sub colour, no recent colours,
    /// the built-in swatches.
    pub fn new() -> Self {
        Self {
            main: Color::BLACK,
            sub: Color::WHITE,
            active: Slot::Main,
            recent: Vec::with_capacity(RECENT_MAX),
            swatches: DEFAULT_SWATCHES
                .iter()
                .copied()
                .map(Color::from_rgb8)
                .collect(),
        }
    }

    pub fn main(&self) -> Color {
        self.main
    }

    pub fn sub(&self) -> Color {
        self.sub
    }

    /// The slot that paints.
    pub fn active_slot(&self) -> Slot {
        self.active
    }

    /// The colour the next stroke paints with.
    pub fn active(&self) -> Color {
        self.get(self.active)
    }

    pub fn get(&self, slot: Slot) -> Color {
        match slot {
            Slot::Main => self.main,
            Slot::Sub => self.sub,
        }
    }

    /// Sets the active slot's colour (picker, values, hex, eyedropper,
    /// swatches).
    pub fn set_active(&mut self, color: Color) {
        match self.active {
            Slot::Main => self.main = color,
            Slot::Sub => self.sub = color,
        }
    }

    /// Makes `slot` the one that paints.
    pub fn select(&mut self, slot: Slot) {
        self.active = slot;
    }

    /// The reference's `X`: the other slot paints; colours unchanged.
    pub fn toggle_active(&mut self) {
        self.active = match self.active {
            Slot::Main => Slot::Sub,
            Slot::Sub => Slot::Main,
        };
    }

    /// Exchanges the two colours; the active slot stays, so the other
    /// colour now paints.
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.main, &mut self.sub);
    }

    /// Main black, sub white, main active (the reference's "main colour
    /// to black and sub colour to white" command, O3).
    pub fn reset(&mut self) {
        self.main = Color::BLACK;
        self.sub = Color::WHITE;
        self.active = Slot::Main;
    }

    /// Recent colours, newest first.
    pub fn recent(&self) -> &[Color] {
        &self.recent
    }

    /// Records a colour a stroke used or the eyedropper picked: moved (or
    /// added) to the front, the oldest dropped beyond [`RECENT_MAX`].
    pub fn remember(&mut self, color: Color) {
        if let Some(i) = self.recent.iter().position(|&c| c == color) {
            self.recent.remove(i);
        }
        self.recent.insert(0, color);
        self.recent.truncate(RECENT_MAX);
    }

    pub fn swatches(&self) -> &[Color] {
        &self.swatches
    }

    /// Adds the active colour at the end of the swatches unless it is
    /// already there or the list is full. Returns whether it was added.
    pub fn add_swatch(&mut self) -> bool {
        let c = self.active();
        if self.swatches.len() >= SWATCHES_MAX || self.swatches.contains(&c) {
            return false;
        }
        self.swatches.push(c);
        true
    }

    /// Removes swatch `i` (ignored if it does not exist).
    pub fn remove_swatch(&mut self, i: usize) {
        if i < self.swatches.len() {
            self.swatches.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::from_rgb8([r, g, b])
    }

    #[test]
    fn starts_black_on_white_with_main_active() {
        let s = ColorState::new();
        assert_eq!(
            (s.main(), s.sub(), s.active_slot()),
            (Color::BLACK, Color::WHITE, Slot::Main)
        );
        assert_eq!(s.active(), Color::BLACK);
        assert!(s.recent().is_empty());
        assert_eq!(s.swatches().len(), DEFAULT_SWATCHES.len());
    }

    #[test]
    fn toggling_changes_which_colour_paints_not_the_colours() {
        let mut s = ColorState::new();
        s.set_active(rgb(255, 0, 0));
        s.toggle_active();
        assert_eq!(s.active(), Color::WHITE);
        s.set_active(rgb(0, 0, 255));
        assert_eq!((s.main(), s.sub()), (rgb(255, 0, 0), rgb(0, 0, 255)));
        s.toggle_active();
        assert_eq!(s.active(), rgb(255, 0, 0));
    }

    #[test]
    fn swapping_twice_is_the_identity_and_reset_is_black_on_white() {
        let mut s = ColorState::new();
        s.set_active(rgb(37, 149, 213));
        s.select(Slot::Sub);
        s.set_active(rgb(200, 10, 10));
        let before = (s.main(), s.sub(), s.active_slot());
        s.swap();
        assert_eq!((s.main(), s.sub()), (before.1, before.0));
        assert_eq!(
            s.active(),
            before.0,
            "the active slot now holds the other colour"
        );
        s.swap();
        assert_eq!((s.main(), s.sub(), s.active_slot()), before);
        s.reset();
        assert_eq!(
            (s.main(), s.sub(), s.active_slot()),
            (Color::BLACK, Color::WHITE, Slot::Main)
        );
    }

    #[test]
    fn recent_colours_are_newest_first_unique_and_bounded() {
        let mut s = ColorState::new();
        // The reference's order after painting black, blue, green, orange
        // and black again (O9): black moves to the front, once.
        for c in [
            Color::BLACK,
            rgb(37, 149, 213),
            rgb(63, 128, 63),
            rgb(255, 128, 0),
            Color::BLACK,
        ] {
            s.remember(c);
        }
        assert_eq!(
            s.recent(),
            [
                Color::BLACK,
                rgb(255, 128, 0),
                rgb(63, 128, 63),
                rgb(37, 149, 213)
            ]
        );
        for i in 0..100u8 {
            s.remember(rgb(i, 0, 0));
        }
        assert_eq!(s.recent().len(), RECENT_MAX);
        assert_eq!(s.recent()[0], rgb(99, 0, 0));
    }

    #[test]
    fn swatches_add_without_duplicates_and_remove() {
        let mut s = ColorState::new();
        let n = s.swatches().len();
        s.set_active(rgb(37, 149, 213));
        assert!(s.add_swatch());
        assert!(!s.add_swatch(), "already there");
        assert_eq!(s.swatches().len(), n + 1);
        assert_eq!(s.swatches()[n], rgb(37, 149, 213));
        s.remove_swatch(n);
        s.remove_swatch(999);
        assert_eq!(s.swatches().len(), n);
        for i in 0..200u8 {
            s.set_active(rgb(i, 1, 2));
            s.add_swatch();
        }
        assert_eq!(s.swatches().len(), SWATCHES_MAX);
    }
}
