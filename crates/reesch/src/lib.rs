//! Generic game engine on macroquad + egui. Knows nothing about the game.

pub mod animation;
pub mod assets;
pub mod camera;
pub mod fixed_step;
pub mod screen;

pub use animation::{Animation, AnimationEvent, AnimationPlayer, SpriteSheet};
pub use assets::Assets;
pub use camera::{fit_camera, fit_view};
/// The egui version egui-macroquad was built with: use this one, never a
/// separate egui dependency.
pub use egui_macroquad::egui;
pub use fixed_step::FixedStep;
pub use screen::{Screen, ScreenStack, Transition};

/// Runs the game from its first screen until the last one is popped.
/// Load every asset before, then call it from the `#[macroquad::main]` function.
pub async fn run(assets: Assets, first: Box<dyn Screen>) {
    let mut stack = ScreenStack::new(first);
    while !stack.is_empty() {
        // One egui pass per frame: the top screen builds its UI there.
        egui_macroquad::ui(|ui| stack.update(ui, &assets));
        stack.draw(&assets);
        egui_macroquad::draw();
        macroquad::window::next_frame().await;
    }
}
