//! The game: the main menu, then one club managed week after week, its fights
//! watched on screen.

mod club_screen;
mod combat_screen;
mod entrance;
mod fight_view;
mod fighter_sprite;
mod lang;
mod options;
mod saves;
mod texts;
mod theme;
mod title_screen;

use std::rc::Rc;

use combat_screen::{DEATH_SOUND, HIT_SOUND, MUSIC};
use lanista_core::design_data::DesignData;
use macroquad::prelude::*;
use reesch::{Assets, Audio, Bindings, Button};
use title_screen::TitleScreen;

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
    let folder = saves::folder();
    if let Some(folder) = folder.clone() {
        saves::log_panics(folder);
    }
    if let Some(code) = folder.as_deref().and_then(saves::read_language) {
        lang::set_language(&code);
    }
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
    let (layers, optional) = fighter_sprite::texture_paths(&data.fighter.appearance);
    for layer in &layers {
        assets
            .load_texture(layer)
            .await
            .unwrap_or_else(|e| panic!("load the fighter layer {layer}: {e}"));
    }
    // Details and the classes' own looks may be missing.
    for path in &optional {
        let _ = assets.load_texture(path).await;
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
    let mut audio = Audio::default();
    let display = folder
        .as_deref()
        .map(|folder| saves::read_settings(folder, &mut audio))
        .unwrap_or_default();
    reesch::run(
        assets,
        audio,
        |ui, _assets| {
            theme::apply(ui);
            options::apply(display);
        },
        Box::new(TitleScreen::new(data, folder, bindings)),
    )
    .await;
}
