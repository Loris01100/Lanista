//! Generic game engine on macroquad + egui. Knows nothing about the game.

pub mod fixed_step;
pub mod screen;

/// The egui version egui-macroquad was built with: use this one, never a
/// separate egui dependency.
pub use egui_macroquad::egui;
pub use fixed_step::FixedStep;
pub use screen::{Screen, ScreenStack, Transition};

/// Runs the game from its first screen until the last one is popped.
/// Call it from the `#[macroquad::main]` function.
pub async fn run(first: Box<dyn Screen>) {
    let mut stack = ScreenStack::new(first);
    while !stack.is_empty() {
        // One egui pass per frame: the top screen builds its UI there.
        egui_macroquad::ui(|ui| stack.update(ui));
        stack.draw();
        egui_macroquad::draw();
        macroquad::window::next_frame().await;
    }
}
