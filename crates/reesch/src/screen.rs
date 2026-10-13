//! Screen stack: the top screen is updated and drawn; its `update` says
//! whether to push, replace or pop.

use egui_macroquad::egui;

/// What the stack does after the top screen's `update`.
pub enum Transition {
    None,
    /// Put a screen on top; the current one waits underneath.
    Push(Box<dyn Screen>),
    /// Swap the current screen for another.
    Replace(Box<dyn Screen>),
    /// Go back to the screen underneath; popping the last one empties the stack.
    Pop,
}

/// One screen of the game (menu, combat, roster…).
pub trait Screen {
    /// Advances the screen by one frame. Build the egui UI here, since its
    /// clicks decide the transition.
    fn update(&mut self, ui: &egui::Context) -> Transition;

    /// Draws the screen; egui is drawn on top afterwards.
    fn draw(&self);
}

/// Screens on top of each other; only the top one is active.
pub struct ScreenStack {
    screens: Vec<Box<dyn Screen>>,
}

impl ScreenStack {
    #[must_use]
    pub fn new(first: Box<dyn Screen>) -> Self {
        Self {
            screens: vec![first],
        }
    }

    /// Updates the top screen and applies its transition.
    pub fn update(&mut self, ui: &egui::Context) {
        let Some(top) = self.screens.last_mut() else {
            return;
        };
        match top.update(ui) {
            Transition::None => {}
            Transition::Push(screen) => self.screens.push(screen),
            Transition::Replace(screen) => {
                self.screens.pop();
                self.screens.push(screen);
            }
            Transition::Pop => {
                self.screens.pop();
            }
        }
    }

    pub fn draw(&self) {
        if let Some(top) = self.screens.last() {
            top.draw();
        }
    }

    /// True once the last screen is popped: time to quit.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.screens.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.screens.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays a scripted list of transitions, one per update.
    struct Scripted(Vec<Transition>);

    impl Screen for Scripted {
        fn update(&mut self, _ui: &egui::Context) -> Transition {
            self.0.pop().unwrap_or(Transition::None)
        }

        fn draw(&self) {}
    }

    fn idle() -> Box<dyn Screen> {
        Box::new(Scripted(Vec::new()))
    }

    #[test]
    fn push_replace_pop() {
        // Scripts pop from the end: push, then replace, then pop.
        let mut stack = ScreenStack::new(Box::new(Scripted(vec![
            Transition::Pop,
            Transition::Push(Box::new(Scripted(vec![Transition::Replace(Box::new(
                Scripted(vec![Transition::Pop]),
            ))]))),
        ])));
        let ui = egui::Context::default();

        stack.update(&ui); // first pushes a screen
        assert_eq!(stack.len(), 2);
        stack.update(&ui); // that screen replaces itself
        assert_eq!(stack.len(), 2);
        stack.update(&ui); // the replacement pops back to the first
        assert_eq!(stack.len(), 1);
        stack.update(&ui); // the first pops: nothing left
        assert!(stack.is_empty());
        stack.update(&ui); // updating an empty stack does nothing
        assert!(stack.is_empty());
    }

    #[test]
    fn none_keeps_the_stack() {
        let mut stack = ScreenStack::new(idle());
        stack.update(&egui::Context::default());
        assert_eq!(stack.len(), 1);
    }
}
