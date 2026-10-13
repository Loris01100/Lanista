//! Phase 0 prototype, now on `reesch`: a Kenney sprite and an egui button.

mod theme;

use macroquad::prelude::*;
use reesch::{Assets, Bindings, Button, Screen, Transition, egui};

const SPRITE: &str = "kenney_tiny_dungeon/tile_0096.png";

#[derive(PartialEq)]
enum Action {
    Quit,
}

struct Prototype {
    clicks: u32,
    bindings: Bindings<Action>,
}

impl Screen for Prototype {
    fn update(&mut self, ui: &egui::Context, _assets: &Assets) -> Transition {
        let mut quit = false;
        egui::Window::new("Prototype").show(ui, |ui| {
            if ui.button("Cliquer").clicked() {
                self.clicks += 1;
            }
            ui.label(format!("Clics : {}", self.clicks));
            quit = ui.button("Quitter").clicked();
        });
        quit |= self.bindings.pressed(ui, &Action::Quit);
        if quit {
            Transition::Pop
        } else {
            Transition::None
        }
    }

    fn draw(&self, assets: &Assets) {
        clear_background(DARKGRAY);
        draw_texture_ex(
            assets.texture(SPRITE),
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

fn window() -> Conf {
    Conf {
        window_title: "Lanista".to_owned(),
        window_width: 1280,
        window_height: 800,
        ..Default::default()
    }
}

#[macroquad::main(window)]
async fn main() {
    // ponytail: relative to the working directory (the workspace root with
    // `cargo run`); resolve next to the executable when packaging for Steam.
    set_pc_assets_folder("assets");
    let mut assets = Assets::default();
    assets
        .load_texture(SPRITE)
        .await
        .expect("load prototype sprite");
    let bindings = Bindings::new(vec![(Action::Quit, Button::Key(KeyCode::Escape))]);
    reesch::run(
        assets,
        theme::apply,
        Box::new(Prototype {
            clicks: 0,
            bindings,
        }),
    )
    .await;
}
