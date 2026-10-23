//! The game: for now, one club managed week after week, its fights watched on
//! screen, and the standard duel.

mod club_screen;
mod combat_screen;
mod fight_view;
mod fighter_sprite;
mod lang;
mod options;
mod saves;
mod texts;
mod theme;

use std::rc::Rc;

use club_screen::ClubScreen;
use combat_screen::{DEATH_SOUND, HIT_SOUND, MUSIC};
use lanista_core::design_data::DesignData;
use macroquad::prelude::*;
use reesch::{Assets, Audio, Bindings, Button};

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
    // `assets/` and `data/` are read from the working directory: the workspace root
    // with `cargo run`, else the executable's folder, wherever it was launched from.
    if !std::path::Path::new("assets").is_dir()
        && let Some(folder) = std::env::current_exe()
            .ok()
            .as_deref()
            .and_then(std::path::Path::parent)
    {
        let _ = std::env::set_current_dir(folder);
    }
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
    let folder = saves::folder();
    let mut audio = Audio::default();
    let display = folder
        .as_deref()
        .map(|folder| saves::read_settings(folder, &mut audio))
        .unwrap_or_default();
    reesch::run(
        assets,
        audio,
        |ui| {
            theme::apply(ui);
            options::apply(ui, display);
        },
        Box::new(ClubScreen::open(data, CAREER_SEED, folder, bindings)),
    )
    .await;
}
