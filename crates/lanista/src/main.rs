//! The game: for now, one duel watched on screen.

mod combat_screen;
mod theme;

use combat_screen::{CombatScreen, FIGHTER_SHEET};
use macroquad::prelude::*;
use reesch::{Assets, Bindings, Button};

/// What the player can do; bound to keys in `main`.
#[derive(PartialEq)]
enum Action {
    Quit,
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
        .load_texture(FIGHTER_SHEET)
        .await
        .expect("load the fighter spritesheet");
    // ponytail: borrows the setup of the `simulate` tool; moves to
    // `lanista_core` with the seed choice (phase 2, step 4).
    let data = tools::design_data().expect("load the design data");
    let arena = tools::random_duel(&data, 42);
    let bindings = Bindings::new(vec![(Action::Quit, Button::Key(KeyCode::Escape))]);
    reesch::run(
        assets,
        theme::apply,
        Box::new(CombatScreen::new(arena, tools::MAX_TICKS, bindings)),
    )
    .await;
}
