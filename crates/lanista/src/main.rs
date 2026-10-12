//! Phase 0 prototype: a macroquad window with a Kenney sprite and an egui button.

use egui_macroquad::egui;
use macroquad::prelude::*;

#[macroquad::main("Lanista")]
async fn main() {
    let sprite = Texture2D::from_file_with_format(
        include_bytes!("../../../assets/kenney_tiny_dungeon/tile_0096.png"),
        None,
    );
    // Pixel art: no blur when scaled up.
    sprite.set_filter(FilterMode::Nearest);
    let mut clicks = 0_u32;

    loop {
        clear_background(DARKGRAY);
        draw_texture_ex(
            &sprite,
            screen_width() / 2.0 - 64.0,
            screen_height() / 2.0 - 64.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(128.0, 128.0)),
                ..Default::default()
            },
        );

        egui_macroquad::ui(|ctx| {
            egui::Window::new("Prototype").show(ctx, |ui| {
                if ui.button("Cliquer").clicked() {
                    clicks += 1;
                }
                ui.label(format!("Clics : {clicks}"));
            });
        });
        egui_macroquad::draw();

        next_frame().await;
    }
}
