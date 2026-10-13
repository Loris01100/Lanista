//! Game actions bound to keys and mouse buttons. Input that egui is using
//! (text field focused, pointer over a window) does not reach the game.

use egui_macroquad::egui;
use macroquad::input::{
    KeyCode, MouseButton, is_key_down, is_key_pressed, is_mouse_button_down,
    is_mouse_button_pressed,
};

/// A physical input an action can be bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// The game's actions (its own enum) and the buttons that trigger them; an
/// action may have several buttons.
#[derive(Debug, Clone)]
pub struct Bindings<A> {
    bindings: Vec<(A, Button)>,
}

impl<A: PartialEq> Bindings<A> {
    #[must_use]
    pub fn new(bindings: Vec<(A, Button)>) -> Self {
        Self { bindings }
    }

    /// True on the frame one of the action's buttons goes down.
    #[must_use]
    pub fn pressed(&self, ui: &egui::Context, action: &A) -> bool {
        self.any(ui, action, is_key_pressed, is_mouse_button_pressed)
    }

    /// True while one of the action's buttons is held.
    #[must_use]
    pub fn down(&self, ui: &egui::Context, action: &A) -> bool {
        self.any(ui, action, is_key_down, is_mouse_button_down)
    }

    fn any(
        &self,
        ui: &egui::Context,
        action: &A,
        key: fn(KeyCode) -> bool,
        mouse: fn(MouseButton) -> bool,
    ) -> bool {
        self.bindings
            .iter()
            .filter(|(bound, _)| bound == action)
            .any(|(_, button)| match *button {
                Button::Key(code) => !ui.wants_keyboard_input() && key(code),
                Button::Mouse(button) => !ui.wants_pointer_input() && mouse(button),
            })
    }
}
