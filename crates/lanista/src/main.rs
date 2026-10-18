//! The game: for now, one club managed week after week, its fights watched on
//! screen, and the standard duel.

mod club_screen;
mod combat_screen;
mod fighter_sprite;
mod saves;
mod texts;
mod theme;

use std::rc::Rc;

use club_screen::ClubScreen;
use combat_screen::{DEATH_SOUND, HIT_SOUND, MUSIC};
use lanista_core::design_data::DesignData;
use macroquad::prelude::*;
use reesch::{Assets, Bindings, Button};

/// What the player can do; bound to keys in `main`.
#[derive(Clone, PartialEq)]
enum Action {
    Quit,
    SlowSpeed,
    NormalSpeed,
    FastSpeed,
    /// Skip to the fight's result.
    Skip,
    /// Play the club's next week.
    NextWeek,
}

/// Seed of the game started at launch when there is no autosave, the one of `career --seed 7`.
const CAREER_SEED: u64 = 7;

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
    let data = Rc::new(DesignData::load().expect("load the design data"));
    let mut assets = Assets::default();
    let (layers, details) = fighter_sprite::texture_paths(&data.fighter.appearance);
    for layer in &layers {
        assets
            .load_texture(layer)
            .await
            .unwrap_or_else(|e| panic!("load the fighter layer {layer}: {e}"));
    }
    // A layer's details are optional.
    for detail in &details {
        let _ = assets.load_texture(detail).await;
    }
    for sound in [HIT_SOUND, DEATH_SOUND, MUSIC] {
        assets.load_sound(sound).await.expect("load a sound");
    }
    let bindings = Bindings::new(vec![
        (Action::Quit, Button::Key(KeyCode::Escape)),
        (Action::SlowSpeed, Button::Key(KeyCode::Key1)),
        (Action::NormalSpeed, Button::Key(KeyCode::Key2)),
        (Action::FastSpeed, Button::Key(KeyCode::Key3)),
        (Action::Skip, Button::Key(KeyCode::Enter)),
        (Action::NextWeek, Button::Key(KeyCode::Space)),
    ]);
    reesch::run(
        assets,
        theme::apply,
        Box::new(ClubScreen::open(
            data,
            CAREER_SEED,
            saves::folder(),
            bindings,
        )),
    )
    .await;
}
