//! Generic game engine on macroquad + egui. Knows nothing about the game.

pub mod fixed_step;
pub mod screen;

pub use fixed_step::FixedStep;
pub use screen::{Screen, ScreenStack, Transition};
