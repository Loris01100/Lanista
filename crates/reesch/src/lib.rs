//! Generic game engine on macroquad + egui. Knows nothing about the game.

pub mod animation;
pub mod assets;
pub mod audio;
pub mod camera;
pub mod fixed_step;
pub mod input;
pub mod render_target;
pub mod screen;

pub use animation::{Animation, AnimationEvent, AnimationPlayer, SpriteSheet};
pub use assets::Assets;
pub use audio::Audio;
pub use camera::{fit_camera, fit_view, pixel_camera, pixel_view};
/// The egui version egui-macroquad was built with: use this one, never a
/// separate egui dependency.
pub use egui_macroquad::egui;
pub use fixed_step::FixedStep;
pub use input::{Bindings, Button};
pub use render_target::{SceneTarget, target_camera};
pub use screen::{Screen, ScreenStack, Transition};

/// Screen height the UI is designed for (Steam Deck, 1280 × 800): the UI is
/// scaled to keep the same layout at any window height.
const UI_DESIGN_HEIGHT: f32 = 800.0;

thread_local! {
    /// The player's interface size, a factor over the fit to the window height.
    static UI_SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

/// Makes the UI `scale` times as big as the fit to the window height, from the
/// next frame on.
pub fn set_ui_scale(scale: f32) {
    UI_SCALE.set(scale);
}

/// Runs the game from its first screen until the last one is popped.
/// Load every asset before, then call it from the `#[macroquad::main]` function.
/// `audio` starts at the volumes the game kept; `setup_ui` runs once to set the egui
/// theme and fonts, those loaded with `Assets::load_font` read with `Assets::font`.
pub async fn run(
    assets: Assets,
    mut audio: Audio,
    setup_ui: impl FnOnce(&egui::Context, &Assets),
    first: Box<dyn Screen>,
) {
    egui_macroquad::cfg(|ui| setup_ui(ui, &assets));
    let mut stack = ScreenStack::new(first);
    while !stack.is_empty() {
        // One egui pass per frame: the top screen builds its UI there.
        egui_macroquad::ui(|ui| {
            let zoom = macroquad::window::screen_height() / UI_DESIGN_HEIGHT * UI_SCALE.get();
            if zoom > 0.0 && (ui.zoom_factor() - zoom).abs() > f32::EPSILON {
                ui.set_zoom_factor(zoom);
            }
            stack.update(ui, &assets, &mut audio);
        });
        stack.draw(&assets);
        egui_macroquad::draw();
        macroquad::window::next_frame().await;
    }
}
