//! Phase 0 prototype, now on `reesch`: a Kenney sprite and an egui button.

use macroquad::prelude::*;
use reesch::{Screen, Transition, egui};

struct Prototype {
    sprite: Texture2D,
    clicks: u32,
}

impl Screen for Prototype {
    fn update(&mut self, ui: &egui::Context) -> Transition {
        let mut quit = false;
        egui::Window::new("Prototype").show(ui, |ui| {
            if ui.button("Cliquer").clicked() {
                self.clicks += 1;
            }
            ui.label(format!("Clics : {}", self.clicks));
            quit = ui.button("Quitter").clicked();
        });
        if quit {
            Transition::Pop
        } else {
            Transition::None
        }
    }

    fn draw(&self) {
        clear_background(DARKGRAY);
        draw_texture_ex(
            &self.sprite,
            screen_width() / 2.0 - 64.0,
            screen_height() / 2.0 - 64.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(128.0, 128.0)),
                ..Default::default()
            },
        );
    }
}

#[macroquad::main("Lanista")]
async fn main() {
    let sprite = Texture2D::from_file_with_format(
        include_bytes!("../../../assets/kenney_tiny_dungeon/tile_0096.png"),
        None,
    );
    // Pixel art: no blur when scaled up.
    sprite.set_filter(FilterMode::Nearest);
    reesch::run(Box::new(Prototype { sprite, clicks: 0 })).await;
}
