//! The game: for now, one duel watched on screen.

mod combat_screen;
mod theme;

use combat_screen::{CombatScreen, FIGHTER_SHEET};
use lanista_core::design_data::DesignData;
use macroquad::prelude::*;
use reesch::{Assets, Bindings, Button};

/// What the player can do; bound to keys in `main`.
#[derive(PartialEq)]
enum Action {
    Quit,
    SlowSpeed,
    NormalSpeed,
    FastSpeed,
    /// Skip to the fight's result.
    Skip,
}

/// Seed of the duel shown at launch; another one is picked on screen.
const FIRST_SEED: u64 = 42;

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
    let data = DesignData::load().expect("load the design data");
    let bindings = Bindings::new(vec![
        (Action::Quit, Button::Key(KeyCode::Escape)),
        (Action::SlowSpeed, Button::Key(KeyCode::Key1)),
        (Action::NormalSpeed, Button::Key(KeyCode::Key2)),
        (Action::FastSpeed, Button::Key(KeyCode::Key3)),
        (Action::Skip, Button::Key(KeyCode::Enter)),
    ]);
    reesch::run(
        assets,
        theme::apply,
        Box::new(CombatScreen::new(data, FIRST_SEED, bindings)),
    )
    .await;
}
